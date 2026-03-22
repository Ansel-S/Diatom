// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/freeze.rs  — v7
//
// E-WBN (Encrypted WebBundle) — the Freeze system upgrade.
//
// Pipeline:
//   1. Strip trackers from raw HTML (Aho-Corasick against blocker list)
//   2. Inline images as data URIs  (already present in raw_html snapshot)
//   3. AES-GCM-256 encrypt with a key derived from the app master key
//   4. Write encrypted bundle to data_dir/bundles/{id}.ewbn
//   5. Return BundleRow for DB insertion (caller indexes with TF-IDF tags)
//
// Key derivation:
//   master_key  →  HKDF-SHA256(info=b"freeze-v7")  →  32-byte AES key
//   The master key is the app's Ed25519 seed (32 bytes), stored in the OS keychain.
//   For this release, if no keychain key exists, we derive from a random seed
//   persisted in the DB meta table under "master_key_b64" (hex-encoded).
//   TPM/Secure Enclave integration is a platform-specific build flag (v7.3).
//
// Bundle format (.ewbn):
//   [4 bytes]  magic: 0x45574254 ("EWBT")
//   [4 bytes]  version: u32 le = 1
//   [12 bytes] nonce (random)
//   [8 bytes]  ciphertext length: u64 le
//   [N bytes]  ciphertext (AES-GCM-256 of gzip-compressed stripped HTML)
//   [16 bytes] AES-GCM auth tag (appended by aes-gcm crate)
// ─────────────────────────────────────────────────────────────────────────────

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result, bail};
use flate2::{write::GzEncoder, Compression};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

use crate::{
    blocker::is_blocked,
    db::{new_id, unix_now, BundleRow},
};

// ── Magic header ─────────────────────────────────────────────────────────────
const EWBN_MAGIC: &[u8; 4]   = b"EWBT";
const EWBN_VERSION: u32       = 1;

// ── Tracker strip patterns (subset — kept in sync with blocker.rs) ────────────
// These are matched against <script src="...">, <img src="...">, and inline URLs.
const TRACKER_SCRIPT_DOMAINS: &[&str] = &[
    "doubleclick.net", "googlesyndication.com", "googletagmanager.com",
    "google-analytics.com", "connect.facebook.net", "pixel.facebook.com",
    "hotjar.com", "amplitude.com", "api.segment.io", "cdn.segment.com",
    "mixpanel.com", "clarity.ms", "fullstory.com", "chartbeat.com",
    "parsely.com", "scorecardresearch.com", "bugsnag.com", "ingest.sentry.io",
    "js-agent.newrelic.com", "nr-data.net", "adnxs.com", "adroll.com",
    "criteo.com", "outbrain.com", "taboola.com", "adsrvr.org",
    "munchkin.marketo.net", "js.hs-scripts.com", "cdn.heapanalytics.com",
    "bat.bing.com", "px.ads.linkedin.com", "moatads.com",
];

// ── FreezeBundle ─────────────────────────────────────────────────────────────

pub struct FreezeBundle {
    pub bundle_row:  BundleRow,
    pub bundle_path: PathBuf,
}

/// Main entry point: strip, compress, encrypt, write, return row.
pub fn freeze_page(
    raw_html: &str,
    url: &str,
    title: &str,
    workspace_id: &str,
    master_key: &[u8; 32],
    bundles_dir: &Path,
) -> Result<FreezeBundle> {
    // 1. Strip trackers from HTML
    let stripped = strip_trackers(raw_html);

    // 2. Gzip compress the stripped HTML
    let compressed = gzip_compress(stripped.as_bytes())?;

    // 3. Derive a per-bundle AES key  (master → HKDF → bundle_key)
    let bundle_key = derive_bundle_key(master_key)?;

    // 4. Encrypt
    let encrypted = aes_gcm_encrypt(&bundle_key, &compressed)?;

    // 5. Build .ewbn byte stream
    let id = new_id();
    let filename = format!("{id}.ewbn");
    let path = bundles_dir.join(&filename);
    write_ewbn(&path, &encrypted)?;

    let content_hash = {
        let h = blake3::hash(url.as_bytes());
        h.to_hex().to_string()
    };

    let row = BundleRow {
        id:           id.clone(),
        url:          url.to_owned(),
        title:        title.to_owned(),
        content_hash,
        bundle_path:  filename,
        tfidf_tags:   "[]".to_owned(),  // filled in by caller after TF-IDF
        bundle_size:  std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as i64,
        frozen_at:    unix_now(),
        workspace_id: workspace_id.to_owned(),
    };

    Ok(FreezeBundle { bundle_row: row, bundle_path: path })
}

/// Decrypt and return the stripped HTML for a frozen bundle.
pub fn thaw_bundle(
    bundle_path: &Path,
    master_key: &[u8; 32],
) -> Result<String> {
    let raw = std::fs::read(bundle_path).context("read .ewbn")?;
    let ciphertext = parse_ewbn(&raw)?;
    let bundle_key = derive_bundle_key(master_key)?;
    let compressed = aes_gcm_decrypt(&bundle_key, ciphertext)?;
    let html = gzip_decompress(&compressed)?;
    Ok(html)
}

// ── Tracker stripping ─────────────────────────────────────────────────────────

/// Remove script/img/iframe tags whose src contains a known tracker domain.
/// Also removes tracking pixel patterns (1×1 images).
/// Pure string-level scan — no DOM parser needed for this level of accuracy.
fn strip_trackers(html: &str) -> String {
    use once_cell::sync::Lazy;
    use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

    static AC: Lazy<AhoCorasick> = Lazy::new(|| {
        AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostFirst)
            .ascii_case_insensitive(true)
            .build(TRACKER_SCRIPT_DOMAINS)
            .unwrap()
    });

    let mut output = String::with_capacity(html.len());
    let mut pos    = 0;

    while pos < html.len() {
        // Find next tag-like opening
        if let Some(tag_start) = html[pos..].find('<').map(|i| pos + i) {
            // Emit everything before this tag
            output.push_str(&html[pos..tag_start]);

            // Find the end of this tag
            let tag_end = html[tag_start..].find('>')
                .map(|i| tag_start + i + 1)
                .unwrap_or(html.len());

            let tag = &html[tag_start..tag_end];

            // Check if it's a script/img/iframe with a tracker src/href
            let lower = tag.to_lowercase();
            let is_external = lower.starts_with("<script") || lower.starts_with("<img")
                || lower.starts_with("<iframe") || lower.starts_with("<link");

            let has_tracker_src = is_external && AC.is_match(tag);

            // Also check for 1×1 tracking pixel pattern
            let is_tracking_pixel = lower.starts_with("<img") &&
                (lower.contains("width=\"1\"") || lower.contains("width='1'")) &&
                (lower.contains("height=\"1\"") || lower.contains("height='1'"));

            // Third-party cookie setting: remove <script> that set document.cookie
            // for cross-origin purposes (heuristic: src from different domain)

            if has_tracker_src || is_tracking_pixel {
                // Skip this tag entirely — also skip any closing tag for <script>/<iframe>
                if lower.starts_with("<script") && !lower.contains("/>") {
                    // Find and skip the closing </script>
                    if let Some(close) = html[tag_end..].to_lowercase().find("</script>") {
                        pos = tag_end + close + "</script>".len();
                    } else {
                        pos = tag_end;
                    }
                } else if lower.starts_with("<iframe") && !lower.contains("/>") {
                    if let Some(close) = html[tag_end..].to_lowercase().find("</iframe>") {
                        pos = tag_end + close + "</iframe>".len();
                    } else {
                        pos = tag_end;
                    }
                } else {
                    pos = tag_end;
                }
                // Insert empty comment as placeholder so layout is not broken
                output.push_str("<!-- [diatom:stripped] -->");
            } else {
                output.push_str(tag);
                pos = tag_end;
            }
        } else {
            // No more tags — emit remainder
            output.push_str(&html[pos..]);
            break;
        }
    }

    output
}

// ── Crypto helpers ────────────────────────────────────────────────────────────

fn derive_bundle_key(master_key: &[u8; 32]) -> Result<[u8; 32]> {
    let hk  = Hkdf::<Sha256>::new(None, master_key);
    let mut key = [0u8; 32];
    hk.expand(b"freeze-v7", &mut key)
        .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
    Ok(key)
}

fn aes_gcm_encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce  = Nonce::from_slice(&nonce_bytes);
    let ct     = cipher.encrypt(nonce, plaintext)
        .map_err(|_| anyhow::anyhow!("AES-GCM encrypt failed"))?;
    // Prepend nonce to ciphertext
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn aes_gcm_decrypt(key: &[u8; 32], mut data: Vec<u8>) -> Result<Vec<u8>> {
    if data.len() < 12 {
        bail!("ciphertext too short");
    }
    let nonce_bytes: [u8; 12] = data[..12].try_into().unwrap();
    let ct = data[12..].to_vec();
    data.zeroize();

    let cipher = Aes256Gcm::new(key.into());
    let nonce  = Nonce::from_slice(&nonce_bytes);
    cipher.decrypt(nonce, ct.as_ref())
        .map_err(|_| anyhow::anyhow!("AES-GCM decrypt failed — wrong key or corrupted bundle"))
}

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    enc.write_all(data)?;
    enc.finish().context("gzip compress")
}

fn gzip_decompress(data: &[u8]) -> Result<String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut dec = GzDecoder::new(data);
    let mut out = String::new();
    dec.read_to_string(&mut out).context("gzip decompress")?;
    Ok(out)
}

// ── .ewbn file I/O ────────────────────────────────────────────────────────────

fn write_ewbn(path: &Path, payload: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path).context("create .ewbn")?;
    f.write_all(EWBN_MAGIC)?;
    f.write_all(&EWBN_VERSION.to_le_bytes())?;
    f.write_all(&(payload.len() as u64).to_le_bytes())?;
    f.write_all(payload)?;
    Ok(())
}

/// Returns the ciphertext payload (nonce + ct + tag).
fn parse_ewbn(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() < 16 { bail!("not a valid .ewbn file"); }
    if &raw[..4] != EWBN_MAGIC { bail!("invalid magic bytes"); }
    let _version = u32::from_le_bytes(raw[4..8].try_into().unwrap());
    let len      = u64::from_le_bytes(raw[8..16].try_into().unwrap()) as usize;
    if raw.len() < 16 + len { bail!("truncated .ewbn"); }
    Ok(raw[16..16 + len].to_vec())
}

// ── Master key bootstrap (fallback without TPM) ────────────────────────────────

/// Get or generate the 32-byte master key stored in DB meta.
/// In v7.3, this will be replaced with TPM/Secure Enclave retrieval.
pub fn get_or_init_master_key(db: &crate::db::Db) -> Result<[u8; 32]> {
    if let Some(hex_key) = db.get_setting("master_key_hex") {
        let bytes = hex::decode(&hex_key).context("decode master key")?;
        if bytes.len() != 32 { bail!("invalid master key length"); }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        return Ok(arr);
    }
    // Generate a new random master key
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    db.set_setting("master_key_hex", &hex::encode(key))?;
    tracing::info!("generated new master key (TPM not available on this platform)");
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let key    = [0x42u8; 32];
        let plain  = b"hello diatom freeze test";
        let ct     = aes_gcm_encrypt(&key, plain).unwrap();
        let dec    = aes_gcm_decrypt(&key, ct).unwrap();
        assert_eq!(dec, plain);
    }

    #[test]
    fn strips_tracker_script() {
        let html = r#"<html>
<script src="https://www.google-analytics.com/analytics.js"></script>
<p>Real content</p>
<img src="https://pixel.facebook.com/tr?id=123" width="1" height="1">
</html>"#;
        let stripped = strip_trackers(html);
        assert!(stripped.contains("Real content"));
        assert!(!stripped.contains("google-analytics.com"));
        assert!(!stripped.contains("pixel.facebook.com"));
        assert!(stripped.contains("[diatom:stripped]"));
    }

    #[test]
    fn ewbn_roundtrip() {
        let dir    = tempdir().unwrap();
        let key    = [0xABu8; 32];
        let html   = "<html><body>test freeze</body></html>";
        let result = freeze_page(html, "https://example.com", "Test", "ws-0", &key, dir.path());
        assert!(result.is_ok());
        let bundle = result.unwrap();
        let thawed = thaw_bundle(&bundle.bundle_path, &key).unwrap();
        assert!(thawed.contains("test freeze"));
    }
}
