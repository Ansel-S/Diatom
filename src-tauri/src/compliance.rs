// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/compliance.rs  — v7.1
//
// Legal compliance annotations and enforcement for user-facing features.
//
// This module is the single source of truth for legal framing of
// features that have known risk areas. It provides:
//   1. Runtime opt-in gates with informed-consent text
//   2. Feature-level legal classification metadata
//   3. Safe Harbor enforcement (Ghost Redirect, Mesh, E-WBN)
//
// Each feature is annotated with:
//   - Legal classification (what it is in law)
//   - Compliance controls (what we enforce in code)
//   - Residual risk (what remains despite controls)
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Feature legal registry ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureLegal {
    pub id:                &'static str,
    pub display_name:      &'static str,
    pub legal_class:       &'static str,   // how we frame it in law
    pub requires_consent:  bool,
    pub consent_text:      &'static str,   // shown to user before activation
    pub controls:          &'static [&'static str],
    pub residual_risk:     &'static str,
}

/// All features with non-trivial legal surface area.
pub static FEATURE_LEGAL_REGISTRY: &[FeatureLegal] = &[
    FeatureLegal {
        id: "decoy_traffic",
        display_name: "隐私噪声注入",
        legal_class: "Defensive privacy noise — equivalent to a browser's fingerprint randomisation. \
                      NOT commercial click fraud: no ad clicks, no form submissions, no revenue diversion.",
        requires_consent: true,
        consent_text: "隐私噪声注入会在后台向公开网页发送匿名请求，以干扰广告追踪画像。\
                       所有请求严格遵守目标网站的 robots.txt，且不超过每8秒1次/每个域名。\
                       此功能不模拟广告点击或任何商业行为。",
        controls: &[
            "robots.txt compliance enforced in decoy.rs before every request",
            "rate limit: max 1 req/8s per domain, max 3 domains per session",
            "GET only: no POST, no form submissions, no ad-click endpoints",
            "full request log written to local DB for user audit",
            "explicit opt-in required; disabled by default",
        ],
        residual_risk: "Some jurisdictions may still classify as unwanted automated access. \
                        Users in the EU/UK should consult local counsel before enabling.",
    },

    FeatureLegal {
        id: "dom_crusher",
        display_name: "DOM Crusher（元素永久屏蔽）",
        legal_class: "User Stylesheet — legally identical to a browser's 'Reader mode' or \
                      user-defined CSS overrides. Hides elements via display:none; does NOT \
                      delete, modify, or redistribute page content.",
        requires_consent: false,
        consent_text: "",
        controls: &[
            "implementation: CSS display:none !important injected via <style> tag",
            "DOM nodes are hidden, not removed — scripts continue to execute",
            "rules are stored locally per-domain; never shared or synced externally",
            "selector validator rejects dangerous patterns (html, :root, *)",
        ],
        residual_risk: "Some site ToS prohibit ad-blocking. Diatom does not screen ToS; \
                        users are responsible for compliance with site-specific terms.",
    },

    FeatureLegal {
        id: "ghost_redirect",
        display_name: "Ghost Redirect（离线语义回退）",
        legal_class: "Local file search — equivalent to macOS Spotlight surfacing a locally \
                      cached file. Only surfaces content the USER personally froze on their \
                      own device. Does not fetch, mirror, or distribute third-party content.",
        requires_consent: false,
        consent_text: "",
        controls: &[
            "BYOD only: Ghost Redirect only indexes user-frozen E-WBN bundles",
            "no P2P sharing of frozen pages between users",
            "no automatic background crawling of third-party sites",
            "stale-content warning shown for bundles > 30 days old",
            "Mesh E-WBN transfer is end-to-end encrypted; never forms a public pool",
        ],
        residual_risk: "Frozen pages may contain copyrighted content. Diatom does not screen \
                        content at freeze time. Users are responsible for compliance with \
                        applicable copyright law when freezing pages.",
    },

    FeatureLegal {
        id: "echo_analysis",
        display_name: "The Echo（人格演化回声）",
        legal_class: "Local self-reflection tool. All computation runs on-device in a Wasm \
                      sandbox. No data is transmitted to any server. Not a medical or \
                      psychological diagnostic tool.",
        requires_consent: true,
        consent_text: "回声（The Echo）在你的设备上本地计算，不向任何服务器上传数据。\
                       分析结果仅供个人自我反思，不构成心理诊断意见，亦不代表任何专业评估。\
                       你可以随时导出或删除所有 Echo 数据。",
        controls: &[
            "all computation in Wasm sandbox with no filesystem/network access",
            "raw reading events are purged after Echo computation via memzero",
            "EchoInput contains only aggregated weights — no URLs, no titles",
            "GDPR Article 15 export via echo_export.js",
            "GDPR Article 17 deletion available in settings",
            "non-medical disclaimer shown before first use",
        ],
        residual_risk: "Persona analysis may constitute 'profiling' under GDPR Article 4(4) \
                        even when performed locally. The absence of a data controller (Diatom \
                        has no backend) likely removes the Article 22 automated-decision concern, \
                        but users in highly regulated jurisdictions should verify.",
    },

    FeatureLegal {
        id: "mesh_sync",
        display_name: "Diatom Mesh（局域网同步）",
        legal_class: "Private P2P local network protocol — equivalent to AirDrop. \
                      No central index server. Device discovery via mDNS (LAN only).",
        requires_consent: false,
        consent_text: "",
        controls: &[
            "no Diatom-operated index server; all discovery is mDNS/BLE local",
            "end-to-end encrypted: Noise_XX handshake, AES-GCM payload",
            "no automatic content sync without user-initiated action",
            "E-WBN bundles transferred only between the user's own authenticated devices",
        ],
        residual_risk: "Napster-style liability requires a centralised index facilitating \
                        infringing distribution. Diatom Mesh has no central index and no \
                        inter-user sharing, so this risk is negligible.",
    },
];

// ── Consent gate ──────────────────────────────────────────────────────────────

/// Check whether the user has consented to a feature that requires it.
/// Returns Err with the consent text if consent is not yet recorded.
pub fn check_consent(feature_id: &str, db: &crate::db::Db) -> Result<(), String> {
    let feature = FEATURE_LEGAL_REGISTRY.iter()
        .find(|f| f.id == feature_id);

    let Some(f) = feature else { return Ok(()); };
    if !f.requires_consent { return Ok(()); }

    let key = format!("consent:{}", feature_id);
    if db.get_setting(&key).as_deref() == Some("true") {
        return Ok(());
    }

    Err(f.consent_text.to_owned())
}

/// Record that the user has consented to a feature.
pub fn record_consent(feature_id: &str, db: &crate::db::Db) -> anyhow::Result<()> {
    let key = format!("consent:{}", feature_id);
    db.set_setting(&key, "true")
}

/// Revoke consent (user turns feature off in settings).
pub fn revoke_consent(feature_id: &str, db: &crate::db::Db) -> anyhow::Result<()> {
    let key = format!("consent:{}", feature_id);
    db.set_setting(&key, "false")
}

/// Get the legal metadata for a feature (exposed to UI for transparency).
pub fn feature_legal(feature_id: &str) -> Option<&'static FeatureLegal> {
    FEATURE_LEGAL_REGISTRY.iter().find(|f| f.id == feature_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_consent_features_have_text() {
        for f in FEATURE_LEGAL_REGISTRY {
            if f.requires_consent {
                assert!(!f.consent_text.is_empty(),
                    "Feature '{}' requires consent but has no consent text", f.id);
            }
        }
    }

    #[test]
    fn all_features_have_controls() {
        for f in FEATURE_LEGAL_REGISTRY {
            assert!(!f.controls.is_empty(),
                "Feature '{}' has no compliance controls listed", f.id);
        }
    }
}
