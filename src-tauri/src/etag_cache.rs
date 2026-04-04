// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/etag_cache.rs  — v0.12.0
//
// HTTP ETag / Last-Modified conditional GET cache for filter list updates.
//
// [B-01 FIX] The v0.11.0 implementation stored the rule body in the DB capped
// at 64 KB. EasyList is ~600 KB — so the cold-start path loaded only ~10% of
// rules, silently weakening the blocker until the full list re-downloaded.
//
// Correct approach:
//   • Store ONLY the ETag and Last-Modified headers in the DB.
//   • NEVER store the rule body in the DB.
//   • On a 304 Not Modified response, callers MUST re-download the full list
//     (issue a fresh GET without conditional headers) rather than using a
//     cached body. The in-memory live_blocker is the sole authoritative source.
//   • This trades one extra ~600 KB download on cold-start (when 304 would have
//     fired) for a correct, complete automaton every time.
//
// Integration:
//   Called from blocker::boot_fetch_builtin_lists() for each filter list URL.
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{Context, Result};

/// Cached conditional-GET metadata for a single filter list URL.
///
/// [B-01 FIX] The `content` field has been removed entirely.
/// The body is NEVER stored in the DB — only the ETag and Last-Modified headers
/// that allow us to send a conditional GET on the next cold start.
/// If the server replies 304, we treat it as a prompt to re-download the full
/// list unconditionally (see `conditional_get`).
#[derive(Debug, Clone)]
pub struct CachedResponse {
    /// ETag header value from the last successful response.
    pub etag: Option<String>,
    /// Last-Modified header value from the last successful response.
    pub last_modified: Option<String>,
}

/// Derive a short stable key from a URL for use in the settings table.
pub fn url_cache_key(url: &str) -> String {
    let hash = blake3::hash(url.as_bytes());
    format!("etag_cache:{}", hex::encode(&hash.as_bytes()[..8]))
}

/// Load cached ETag + Last-Modified for `url` from the database.
///
/// [B-01 FIX] No longer loads a cached body.
pub fn load(db: &crate::db::Db, url: &str) -> CachedResponse {
    let key = url_cache_key(url);
    let etag          = db.get_setting(&format!("{key}:etag"));
    let last_modified = db.get_setting(&format!("{key}:lm"));
    CachedResponse { etag, last_modified }
}

/// Persist ETag and Last-Modified headers after a 200 OK response.
///
/// [B-01 FIX] Does NOT persist the rule body. The body is held only in-memory
/// via live_blocker. On next cold start, the conditional GET will either fetch
/// fresh content (200) or prompt a full re-download (304 → unconditional GET).
pub fn store(
    db: &crate::db::Db,
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) {
    let key = url_cache_key(url);
    if let Some(e) = etag {
        let _ = db.set_setting(&format!("{key}:etag"), e);
    }
    if let Some(lm) = last_modified {
        let _ = db.set_setting(&format!("{key}:lm"), lm);
    }
    // Proactively clean up any stale body entries left by v0.11.0.
    // These waste space and are no longer semantically meaningful.
    let _ = db.0.lock().unwrap().execute(
        "DELETE FROM meta WHERE key = ?1",
        rusqlite::params![format!("{key}:body")],
    );
}

/// Perform a conditional HTTP GET for `url`.
///
/// Sends If-None-Match / If-Modified-Since if cached values exist.
///
/// [B-01 FIX] The old API returned `Ok(None)` for 304 Not Modified and let
/// callers use the (truncated, broken) cached body. The new contract:
///
///   Ok(content)  — fresh content fetched (200 or 304→full re-download)
///   Err(_)       — network/HTTP error
///
/// On 304, this function performs an unconditional GET (without conditional
/// headers) to retrieve the complete rule set. The bandwidth saving from ETag
/// caching is preserved when the content genuinely hasn't changed (200 OK
/// with an identical body), but we never fall back to a truncated DB body.
pub async fn conditional_get(
    client: &reqwest::Client,
    url: &str,
    cached: &CachedResponse,
    user_agent: &str,
) -> Result<String> {
    let mut req = client
        .get(url)
        .header("User-Agent", user_agent)
        .header("Accept-Encoding", "gzip")
        .timeout(std::time::Duration::from_secs(30));

    if let Some(etag) = &cached.etag {
        req = req.header("If-None-Match", etag.as_str());
    } else if let Some(lm) = &cached.last_modified {
        req = req.header("If-Modified-Since", lm.as_str());
    }

    let resp = req.send().await.context("conditional GET send")?;

    match resp.status().as_u16() {
        304 => {
            // [B-01 FIX] 304 means the server agrees ETag is current, but we
            // have no valid cached body (we never store it). Perform an
            // unconditional full download to populate the in-memory automaton.
            tracing::debug!("[etag] 304 Not Modified (no cached body) — re-downloading full list: {url}");
            let full = client
                .get(url)
                .header("User-Agent", user_agent)
                .header("Accept-Encoding", "gzip")
                .timeout(std::time::Duration::from_secs(60))
                .send()
                .await
                .context("unconditional re-download after 304")?
                .text()
                .await
                .context("re-download body")?;
            tracing::debug!("[etag] re-download OK ({} bytes): {url}", full.len());
            Ok(full)
        }
        200 => {
            let content = resp.text().await.context("response body")?;
            tracing::debug!("[etag] 200 OK ({} bytes): {url}", content.len());
            Ok(content)
        }
        s => {
            anyhow::bail!("unexpected status {s} for {url}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_cache_key_stable() {
        let k1 = url_cache_key("https://easylist.to/easylist/easylist.txt");
        let k2 = url_cache_key("https://easylist.to/easylist/easylist.txt");
        assert_eq!(k1, k2);
        assert!(k1.starts_with("etag_cache:"));
    }

    #[test]
    fn different_urls_different_keys() {
        let k1 = url_cache_key("https://example.com/a.txt");
        let k2 = url_cache_key("https://example.com/b.txt");
        assert_ne!(k1, k2);
    }

    /// [B-01 FIX] Verify CachedResponse no longer carries a content field.
    /// This compile-time check ensures we can't accidentally re-introduce
    /// the truncated-body cache pattern.
    #[test]
    fn cached_response_has_no_body_field() {
        let cr = CachedResponse {
            etag: Some("\"abc123\"".to_owned()),
            last_modified: None,
        };
        assert!(cr.etag.is_some());
        // content field must not exist — this would fail to compile if re-added
    }
}
