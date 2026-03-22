// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/trust.rs
//
// Domain trust levels. Controls which browser capabilities are granted
// per site without requiring extensions or per-permission dialogs.
//
// L0 Untrusted   — aggressive blocking, no JS storage, no cookies
// L1 Standard    — default; blocker active, cookies sandboxed to workspace
// L2 Trusted     — blocker relaxed (first-party scripts allowed), cookies OK
// L3 Allowlisted — blocker disabled, full storage access (payment sites etc.)
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Untrusted,
    Standard,
    Trusted,
    Allowlisted,
}

impl TrustLevel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "untrusted"   => TrustLevel::Untrusted,
            "trusted"     => TrustLevel::Trusted,
            "allowlisted" => TrustLevel::Allowlisted,
            _             => TrustLevel::Standard,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TrustLevel::Untrusted   => "untrusted",
            TrustLevel::Standard    => "standard",
            TrustLevel::Trusted     => "trusted",
            TrustLevel::Allowlisted => "allowlisted",
        }
    }

    /// Whether the tracker blocker runs at this trust level.
    pub fn blocker_active(&self) -> bool {
        matches!(self, TrustLevel::Untrusted | TrustLevel::Standard)
    }

    /// Whether first-party cookies are allowed.
    pub fn cookies_allowed(&self) -> bool {
        !matches!(self, TrustLevel::Untrusted)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustProfile {
    pub domain: String,
    pub level:  TrustLevel,
    pub source: String,   // "user" | "auto" | "compliance"
    pub set_at: i64,
}

// ── Store ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct TrustStore {
    /// Exact domain matches (eTLD+1 from blocker::domain_of).
    profiles: HashMap<String, TrustProfile>,
}

impl TrustStore {
    pub fn get(&self, domain: &str) -> TrustProfile {
        self.profiles.get(domain).cloned().unwrap_or_else(|| TrustProfile {
            domain: domain.to_owned(),
            level:  TrustLevel::Standard,
            source: "default".to_owned(),
            set_at: 0,
        })
    }

    pub fn set(&mut self, domain: &str, level: &str, source: &str) {
        self.profiles.insert(domain.to_owned(), TrustProfile {
            domain: domain.to_owned(),
            level:  TrustLevel::from_str(level),
            source: source.to_owned(),
            set_at: crate::db::unix_now(),
        });
    }

    pub fn all(&self) -> Vec<TrustProfile> {
        let mut v: Vec<TrustProfile> = self.profiles.values().cloned().collect();
        v.sort_by_key(|p| p.set_at);
        v
    }

    pub fn remove(&mut self, domain: &str) {
        self.profiles.remove(domain);
    }

    pub fn is_level(&self, domain: &str, level: TrustLevel) -> bool {
        self.get(domain).level == level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_standard() {
        let store = TrustStore::default();
        assert_eq!(store.get("example.com").level, TrustLevel::Standard);
    }

    #[test]
    fn set_and_retrieve() {
        let mut store = TrustStore::default();
        store.set("payment.example.com", "allowlisted", "user");
        assert_eq!(store.get("payment.example.com").level, TrustLevel::Allowlisted);
        assert!(!store.get("payment.example.com").level.blocker_active());
    }
}
