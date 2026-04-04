// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/breach_monitor.rs  — v0.12.0  [F-04]
//
// Dark Web Leak Monitor — 暗网泄露监控
//
// Uses the Have I Been Pwned k-anonymity API to check vault email addresses
// and password hashes for known breaches. Privacy model:
//
//   Password check: only the first 5 hex characters of SHA-1(password) are
//   transmitted. The full hash never leaves the device. HIBP returns ~800
//   matching suffixes; Diatom matches locally. This is the k-anonymity model
//   defined in https://haveibeenpwned.com/API/v3#SearchingPwnedPasswordsByRange.
//
//   Email check: sends the full email address to HIBP (documented risk — user
//   must opt-in via a separate toggle). Requests use a random User-Agent
//   (never the Diatom UA) to prevent correlation.
//
// Results are cached in the DB for 7 days to avoid redundant API calls.
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha1::{Sha1, Digest};

const PWNED_PASSWORDS_URL: &str = "https://api.pwnedpasswords.com/range/";
const PWNED_EMAIL_URL:      &str = "https://haveibeenpwned.com/api/v3/breachedaccount/";
/// Cache TTL: 7 days in seconds.
const CACHE_TTL_SECS: i64 = 7 * 24 * 3_600;

// ── Result types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordBreachResult {
    /// True if the password appeared in at least one known breach.
    pub pwned: bool,
    /// Number of times this password appeared in breach datasets.
    pub pwned_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBreachEntry {
    pub name: String,
    pub breach_date: String,
    pub data_classes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBreachResult {
    pub email: String,
    pub breaches: Vec<EmailBreachEntry>,
}

// ── Password check (k-anonymity) ─────────────────────────────────────────────

/// Compute SHA-1 of a password and return (full_hash_upper, prefix_5_chars).
fn sha1_prefix(password: &str) -> (String, String) {
    let mut hasher = Sha1::new();
    hasher.update(password.as_bytes());
    let hash = format!("{:X}", hasher.finalize());
    let prefix = hash[..5].to_owned();
    (hash, prefix)
}

/// Check a single password against HIBP k-anonymity API.
///
/// Only the 5-character SHA-1 prefix is transmitted — never the full hash or
/// the original password.
pub async fn check_password(
    client: &reqwest::Client,
    password: &str,
) -> Result<PasswordBreachResult> {
    let (full_hash, prefix) = sha1_prefix(password);
    let url = format!("{}{}", PWNED_PASSWORDS_URL, prefix);

    let resp = client
        .get(&url)
        .header("Add-Padding", "true")  // prevents response-size side-channel
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .context("HIBP password range request")?
        .text()
        .await
        .context("HIBP response body")?;

    // HIBP returns lines of "SUFFIX:COUNT" — match against our full hash suffix
    let suffix = &full_hash[5..];
    for line in resp.lines() {
        let mut parts = line.splitn(2, ':');
        if let (Some(s), Some(c)) = (parts.next(), parts.next()) {
            if s.eq_ignore_ascii_case(suffix) {
                let count: u64 = c.trim().parse().unwrap_or(1);
                return Ok(PasswordBreachResult { pwned: true, pwned_count: count });
            }
        }
    }
    Ok(PasswordBreachResult { pwned: false, pwned_count: 0 })
}

/// Check an email address against HIBP breach database.
///
/// WARNING: This transmits the full email address to HIBP. The user must have
/// explicitly opted in via the `breach_monitor_email` toggle. Requests use a
/// random User-Agent (not the Diatom UA) to reduce correlation.
pub async fn check_email(
    client: &reqwest::Client,
    email: &str,
) -> Result<EmailBreachResult> {
    #[derive(Deserialize)]
    struct HibpBreach {
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "BreachDate")]
        breach_date: String,
        #[serde(rename = "DataClasses")]
        data_classes: Vec<String>,
    }

    let url = format!("{}{}", PWNED_EMAIL_URL, urlencoding_encode(email));

    let resp = client
        .get(&url)
        .header("hibp-api-key", "") // free tier — no key needed for breach lookup
        .header("User-Agent", random_generic_ua())
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .context("HIBP email breach request")?;

    if resp.status().as_u16() == 404 {
        // 404 means no breaches found
        return Ok(EmailBreachResult { email: email.to_owned(), breaches: vec![] });
    }

    let entries: Vec<HibpBreach> = resp.json().await.context("HIBP email response parse")?;
    let breaches = entries.into_iter().map(|e| EmailBreachEntry {
        name: e.name,
        breach_date: e.breach_date,
        data_classes: e.data_classes,
    }).collect();

    Ok(EmailBreachResult { email: email.to_owned(), breaches })
}

// ── Cache helpers ─────────────────────────────────────────────────────────────

/// Cache a password breach result for 7 days.
pub fn cache_password_result(db: &crate::db::Db, password_sha1: &str, result: &PasswordBreachResult) {
    if let Ok(json) = serde_json::to_string(result) {
        let key = format!("breach_pw:{}", &password_sha1[..8]);
        let expiry = crate::db::unix_now() + CACHE_TTL_SECS;
        let entry = format!("{}:{}", expiry, json);
        let _ = db.set_setting(&key, &entry);
    }
}

/// Load a cached password result. Returns None if expired or absent.
pub fn load_cached_password(db: &crate::db::Db, password_sha1: &str) -> Option<PasswordBreachResult> {
    let key = format!("breach_pw:{}", &password_sha1[..8]);
    let entry = db.get_setting(&key)?;
    let colon = entry.find(':')?;
    let expiry: i64 = entry[..colon].parse().ok()?;
    if crate::db::unix_now() > expiry {
        return None; // Expired
    }
    serde_json::from_str(&entry[colon + 1..]).ok()
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn urlencoding_encode(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        '@' => "%40".to_owned(),
        '+' => "%2B".to_owned(),
        _ => format!("%{:02X}", c as u32),
    }).collect()
}

/// Return a random generic browser UA to avoid Diatom correlation on HIBP calls.
fn random_generic_ua() -> &'static str {
    // Rotate between a small set of common UAs
    const UAS: &[&str] = &[
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15",
        "Mozilla/5.0 (X11; Linux x86_64; rv:125.0) Gecko/20100101 Firefox/125.0",
    ];
    let idx = (crate::db::unix_now() as usize / 3600) % UAS.len();
    UAS[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_prefix_length() {
        let (full, prefix) = sha1_prefix("password123");
        assert_eq!(full.len(), 40);
        assert_eq!(prefix.len(), 5);
        assert!(full.starts_with(&prefix));
    }

    #[test]
    fn sha1_prefix_known_value() {
        // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        let (full, prefix) = sha1_prefix("password");
        assert_eq!(prefix, "5BAA6");
        assert_eq!(&full[..5], "5BAA6");
    }

    #[test]
    fn url_encode_email() {
        let encoded = urlencoding_encode("user@example.com");
        assert!(encoded.contains("%40"));
        assert!(!encoded.contains('@'));
    }
}
