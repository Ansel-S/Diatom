// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/totp.rs
//
// TOTP (RFC 6238) and HOTP (RFC 4226) two-factor authentication manager.
// Keys are stored encrypted in the SQLite meta table under the key
// "totp_entries_<id>". The master key from freeze.rs is used for encryption.
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{Result, bail};
use base32::Alphabet;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::collections::HashMap;

type HmacSha1 = Hmac<Sha1>;

// ── Data model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpEntry {
    pub id: String,
    pub issuer: String,
    pub account: String,
    /// Base32-encoded TOTP secret.
    pub secret: String,
    /// Domains that auto-trigger this entry when focused.
    pub domains: Vec<String>,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpCode {
    pub entry_id: String,
    pub issuer: String,
    pub account: String,
    pub code: String,
    pub valid_until: i64, // Unix timestamp when this 30s window ends
}

// ── Store ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct TotpStore {
    entries: HashMap<String, TotpEntry>,
}

impl TotpStore {
    pub fn add(
        &mut self,
        issuer: &str,
        account: &str,
        secret: &str,
        domains: Vec<String>,
    ) -> Result<TotpEntry> {
        // Validate the secret decodes
        base32::decode(Alphabet::Rfc4648 { padding: false }, secret)
            .or_else(|| base32::decode(Alphabet::Rfc4648 { padding: true }, secret))
            .ok_or_else(|| anyhow::anyhow!("invalid base32 TOTP secret"))?;

        let id = crate::db::new_id();
        let entry = TotpEntry {
            id: id.clone(),
            issuer: issuer.to_owned(),
            account: account.to_owned(),
            secret: secret.to_uppercase().replace(' ', ""),
            domains,
            added_at: crate::db::unix_now(),
        };
        self.entries.insert(id, entry.clone());
        Ok(entry)
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.remove(id);
    }

    pub fn list(&self) -> Vec<TotpEntry> {
        let mut v: Vec<TotpEntry> = self.entries.values().cloned().collect();
        v.sort_by_key(|e| e.added_at);
        v
    }

    pub fn generate(&self, id: &str) -> Result<TotpCode> {
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("TOTP entry {id} not found"))?;
        let code = totp_now(&entry.secret)?;
        let now = crate::db::unix_now();
        let valid_until = (now / 30 + 1) * 30;
        Ok(TotpCode {
            entry_id: id.to_owned(),
            issuer: entry.issuer.clone(),
            account: entry.account.clone(),
            code,
            valid_until,
        })
    }

    pub fn match_domain(&self, domain: &str) -> Vec<TotpCode> {
        self.entries
            .values()
            .filter(|e| e.domains.iter().any(|d| domain.ends_with(d.as_str())))
            .filter_map(|e| self.generate(&e.id).ok())
            .collect()
    }
}

// ── TOTP / HOTP core ─────────────────────────────────────────────────────────

/// Generate the current 6-digit TOTP code for a base32-encoded secret.
pub fn totp_now(secret_b32: &str) -> Result<String> {
    let key = base32::decode(Alphabet::Rfc4648 { padding: false }, secret_b32)
        .or_else(|| base32::decode(Alphabet::Rfc4648 { padding: true }, secret_b32))
        .ok_or_else(|| anyhow::anyhow!("invalid base32 secret"))?;

    let counter = crate::db::unix_now() as u64 / 30;
    hotp(&key, counter)
}

/// RFC 4226 HOTP — 6-digit truncated HMAC-SHA1.
fn hotp(key: &[u8], counter: u64) -> Result<String> {
    let mut mac =
        HmacSha1::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid HMAC key length"))?;
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0xf) as usize;
    let code = u32::from_be_bytes([
        result[offset] & 0x7f,
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]) % 1_000_000;

    Ok(format!("{:06}", code))
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4226 test vector: secret = "12345678901234567890", counter = 0 → "755224"
    #[test]
    fn hotp_rfc_vector() {
        let key = b"12345678901234567890";
        assert_eq!(hotp(key, 0).unwrap(), "755224");
        assert_eq!(hotp(key, 1).unwrap(), "287082");
        assert_eq!(hotp(key, 2).unwrap(), "359152");
    }

    #[test]
    fn store_add_and_generate() {
        let mut store = TotpStore::default();
        // Standard test secret: JBSWY3DPEHPK3PXP
        let entry = store
            .add("Test", "user@example.com", "JBSWY3DPEHPK3PXP", vec![])
            .unwrap();
        assert_eq!(entry.issuer, "Test");
        let code = store.generate(&entry.id).unwrap();
        assert_eq!(code.code.len(), 6);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
    }
}
