// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/zen.rs  — v7
//
// Zen Mode: deep-work protection state machine.
//
// State transitions:
//   Off → Active (user command or /zen)
//   Active → Off (user types ≥50-char unlock phrase — validated JS side)
//
// The unlock phrase is intentionally validated in JS (diatom-api.js)
// rather than Rust because it is a UX gate, not a security gate.
// The security property of Zen mode is notification suppression (SW level)
// and the interstitial page, not cryptographic enforcement.
//
// Blocked categories are matched against the domain of the navigation target.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ZenState {
    Off,
    Active,
}

impl Default for ZenState {
    fn default() -> Self {
        ZenState::Off
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZenConfig {
    pub state: ZenState,
    pub aphorism: String,
    pub blocked_categories: Vec<String>,
    /// Unix timestamp when Zen mode was activated (for session tracking)
    pub activated_at: Option<i64>,
}

impl Default for ZenConfig {
    fn default() -> Self {
        ZenConfig {
            state: ZenState::Off,
            aphorism: "Now will always have been.".to_owned(),
            blocked_categories: vec!["social".into(), "entertainment".into()],
            activated_at: None,
        }
    }
}

/// Domain-to-category mapping for Zen mode blocking.
/// Returns the category name if the domain is in a blocked category.
pub fn domain_category(domain: &str) -> Option<&'static str> {
    const SOCIAL: &[&str] = &[
        "twitter.com",
        "x.com",
        "instagram.com",
        "facebook.com",
        "tiktok.com",
        "weibo.com",
        "douyin.com",
        "threads.net",
        "mastodon.social",
        "bluesky.app",
        "reddit.com",
        "discord.com",
        "snapchat.com",
        "linkedin.com",
        "pinterest.com",
    ];
    const ENTERTAINMENT: &[&str] = &[
        "youtube.com",
        "bilibili.com",
        "netflix.com",
        "twitch.tv",
        "hulu.com",
        "disneyplus.com",
        "primevideo.com",
        "9gag.com",
        "ifunny.co",
        "tumblr.com",
        "buzzfeed.com",
        "dailymotion.com",
        "vimeo.com",
        "rumble.com",
        "odysee.com",
    ];

    let d = domain.to_lowercase();
    let d = d.trim_start_matches("www.");

    if SOCIAL
        .iter()
        .any(|s| d == *s || d.ends_with(&format!(".{s}")))
    {
        return Some("social");
    }
    if ENTERTAINMENT
        .iter()
        .any(|s| d == *s || d.ends_with(&format!(".{s}")))
    {
        return Some("entertainment");
    }
    None
}

impl ZenConfig {
    pub fn activate(&mut self) {
        self.state = ZenState::Active;
        self.activated_at = Some(crate::db::unix_now());
    }

    pub fn deactivate(&mut self) {
        self.state = ZenState::Off;
        self.activated_at = None;
    }

    pub fn is_active(&self) -> bool {
        self.state == ZenState::Active
    }

    /// Returns Some(category) if navigation to this domain should be blocked.
    pub fn blocks_domain(&self, domain: &str) -> Option<&'static str> {
        if !self.is_active() {
            return None;
        }
        domain_category(domain).filter(|cat| self.blocked_categories.iter().any(|c| c == cat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zen_blocks_social_when_active() {
        let mut cfg = ZenConfig::default();
        cfg.activate();
        assert_eq!(cfg.blocks_domain("twitter.com"), Some("social"));
        assert_eq!(cfg.blocks_domain("youtube.com"), Some("entertainment"));
        assert_eq!(cfg.blocks_domain("github.com"), None);
    }

    #[test]
    fn zen_off_blocks_nothing() {
        let cfg = ZenConfig::default();
        assert!(cfg.blocks_domain("twitter.com").is_none());
    }

    #[test]
    fn deactivate_clears_timestamp() {
        let mut cfg = ZenConfig::default();
        cfg.activate();
        assert!(cfg.activated_at.is_some());
        cfg.deactivate();
        assert!(cfg.activated_at.is_none());
    }
}
