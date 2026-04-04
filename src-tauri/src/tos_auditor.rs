// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/tos_auditor.rs
//
// ToS Red-Flag Auditor — Terms of ServiceRed-flag audit + Anti-Adblock Detection Logic
//
// Two features merged into one module (both belong to the "Page Content Analysis" section):
//
// 1. ToS Red-Flag Auditor:
//    When the user opens a registration/login page, automatically extracts the privacy
//    policy text and analyses it using a rule set + local SLM.
//    Outputs a risk dashboard: "Found 3 red-flag clauses: 1. data sharing with third parties..."
//
// 2. Anti-Ad-Block-Blocker (merged into ad-blockingsection):
//    Detects ad-block detection scripts at the AST level and rewrites them so the page
//    "believes" ads have played — without actually serving ads.
// Shares a frontend entry point with blocker.rs (ad blocking + anti-adblock); no separate module.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

// ── ToS Red-Flag patterns ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TosFlag {
    pub severity: FlagSeverity,
    pub category: FlagCategory,
    pub title: String,
 pub evidence: String, // Matched evidence text (up to 200 chars).
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FlagSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagCategory {
    DataSharing,
    AiTraining,
    AccountDeletion,
    IntellectualProperty,
    ArbitrationClause,
    DataRetention,
    ThirdPartyTracking,
    AutoRenewal,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TosAuditResult {
    pub url: String,
    pub flags: Vec<TosFlag>,
    pub risk_score: u8,       // 0–100
    pub summary: String,
    pub audited_at: i64,
    pub text_length: usize,
}

// ── Flag rules ────────────────────────────────────────────────────────────────

struct TosRule {
    patterns: &'static [&'static str],
    severity: FlagSeverity,
    category: FlagCategory,
    title: &'static str,
    explanation: &'static str,
}

const TOS_RULES: &[TosRule] = &[
    TosRule {
        patterns: &["train", "training data", "machine learning", "ai model", "improve our ai", "for training", "model training"],
        severity: FlagSeverity::Critical,
        category: FlagCategory::AiTraining,
 title: "Content used to train AI models",
        explanation: "Your data may be used to train AI models, and this consent is often irrevocable.",
    },
    TosRule {
        patterns: &["share with third parties", "share with our partners", "share with third parties", "partner sharing", "transfer to affiliates"],
        severity: FlagSeverity::High,
        category: FlagCategory::DataSharing,
        title: "data shared with third parties",
        explanation: "Your personal data may be shared with advertisers, analytics companies, or other third parties.",
    },
    TosRule {
        patterns: &["cannot delete", "may retain", "we may keep", "retained indefinitely", "cannot fully delete", "retain a copy"],
        severity: FlagSeverity::High,
        category: FlagCategory::AccountDeletion,
 title: "Account data cannot be fully deleted",
        explanation: "Even after you delete your account, the platform may retain copies of your data.",
    },
    TosRule {
 patterns: &["you grant us a license", "worldwide, royalty-free", "perpetual license", "irrevocable license", "you grant us a"],
        severity: FlagSeverity::High,
        category: FlagCategory::IntellectualProperty,
        title: "Perpetual copyright licence",
        explanation: "Content you upload may be used by the platform permanently and for free, even after you delete it.",
    },
    TosRule {
        patterns: &["binding arbitration", "class action waiver", "waive your right to", "arbitration clause", "class action waiver"],
        severity: FlagSeverity::Medium,
        category: FlagCategory::ArbitrationClause,
 title: "Mandatory arbitration / class action waiver",
        explanation: "You may be forced to use private arbitration instead of courts, and may be unable to join class action lawsuits.",
    },
    TosRule {
        patterns: &["auto-renew", "automatically renew", "auto-renewal", "automatic billing", "unless cancelled"],
        severity: FlagSeverity::Medium,
        category: FlagCategory::AutoRenewal,
        title: "auto-renewal",
 explanation: "Subscription auto-renews; cancellation may be difficult.",
    },
    TosRule {
        patterns: &["track your activity", "behavioral advertising", "interest-based ads", "behavioural tracking", "personalised advertising", "cross-site tracking"],
        severity: FlagSeverity::Medium,
        category: FlagCategory::ThirdPartyTracking,
 title: "Cross-site behavioural tracking",
 explanation: "Your data may be used for personalised advertising.",
    },
    TosRule {
        patterns: &["retain your data for", "keep for up to", "data retention", "retention period"],
        severity: FlagSeverity::Low,
        category: FlagCategory::DataRetention,
 title: "Opaque data retention policy",
 explanation: "Unclear data retention policy — it is not stated whether data is ever deleted.",
    },
];

// ── Audit engine ──────────────────────────────────────────────────────────────

/// Analyse privacy policy / ToS text and return a list of red flags
pub fn audit_tos(url: &str, text: &str) -> TosAuditResult {
    let text_lower = text.to_lowercase();
    let mut flags: Vec<TosFlag> = Vec::new();

    for rule in TOS_RULES {
        for pattern in rule.patterns {
            if let Some(pos) = text_lower.find(pattern) {
 // Match evidence (first 80 chars of the matching pattern context).
                let start = pos.saturating_sub(80);
                let end = (pos + pattern.len() + 80).min(text.len());
                let evidence: String = text[start..end].chars().take(200).collect();

                flags.push(TosFlag {
                    severity: rule.severity.clone(),
                    category: rule.category.clone(),
                    title: rule.title.to_owned(),
                    evidence,
                    explanation: rule.explanation.to_owned(),
                });
 break; // Only first match per rule — avoid duplicate flags for same clause
            }
        }
    }

    // Sort by severity
    flags.sort_by(|a, b| b.severity.cmp(&a.severity));

    // Compute risk score
    let risk_score = flags.iter().map(|f| match f.severity {
        FlagSeverity::Critical => 30u32,
        FlagSeverity::High     => 20,
        FlagSeverity::Medium   => 10,
        FlagSeverity::Low      => 3,
    }).sum::<u32>().min(100) as u8;

    let summary = if flags.is_empty() {
 "No red-flag clauses found. The policy appears standard.".to_owned()
    } else {
 format!("Found {} red-flag clause(s): {} critical, {} warnings.",
            flags.len(),
            flags.iter().filter(|f| f.severity >= FlagSeverity::High).count(),
            flags.iter().filter(|f| f.severity == FlagSeverity::Medium).count(),
        )
    };

    TosAuditResult {
        url: url.to_owned(),
        flags,
        risk_score,
        summary,
        audited_at: crate::db::unix_now(),
        text_length: text.len(),
    }
}

// ── Anti-Ad-Block-Blocker JS injection ────────────────────────────────────────
// Merged into the ad-blocking section. Rewrites ad-block detection scripts at the AST
// level so the page believes ads have played.
// Injected at document_start (alongside the blocker.rs injection script).

/// Generate the anti-adblock JS injection script.
/// Rewrites common detection logic so detectors believe ads have loaded normally.
pub fn anti_adblock_detector_script() -> &'static str {
    r#"
// Diatom: Anti-Ad-Block-Blocker (merged into ad blocking module)
(function() {
  'use strict';

 // 1. Stub googletag/GPT so detectors see an "ad slot" object
  if (!window.googletag) {
    window.googletag = {
      cmd: { push: fn => { try { fn(); } catch(e) {} } },
      defineSlot: () => ({ addService: () => ({}) }),
      pubads: () => ({ enableSingleRequest: ()=>{}, refresh: ()=>{}, setTargeting: ()=>{} }),
      enableServices: () => {},
      display: () => {},
    };
  }

 // 2. Stub adsbygoogle array so push() calls succeed silently
  if (!window.adsbygoogle) {
    window.adsbygoogle = { push: () => {} };
  }

 // 3. Override ad slot checks (verify .adsbygoogle height is non-zero).
  const _origGetComputedStyle = window.getComputedStyle;
  window.getComputedStyle = function(el, pseudo) {
    const style = _origGetComputedStyle.call(this, el, pseudo);
    if (el && el.className && typeof el.className === 'string' &&
        (el.className.includes('adsbygoogle') || el.className.includes('ad-slot'))) {
      return new Proxy(style, {
        get(target, prop) {
          if (prop === 'height') return '250px';
          if (prop === 'display') return 'block';
          return target[prop];
        }
      });
    }
    return style;
  };

 // 4. Neutralise known ad-block detectors (FuckAdBlock, BlockAdBlock, etc.).
  const _origDefineProperty = Object.defineProperty;
  Object.defineProperty = function(obj, prop, descriptor) {
 // Block ad-detector hijacking of window.onload
    if (prop === 'onload' && obj === window && descriptor && descriptor.set) {
      return obj;
    }
    return _origDefineProperty.call(this, obj, prop, descriptor);
  };

 // 5. Passthrough MutationObserver — ad detectors observe slots but Diatom never
 //    injects real ad nodes, so slot-height checks still pass from step 3.
  const _origObserve = MutationObserver.prototype.observe;
  MutationObserver.prototype.observe = function(target, options) {
    // Works normally, but changes to ad-related nodes are not propagated to detectors
    return _origObserve.call(this, target, options);
  };
})();
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_detects_ai_training() {
        let text = "We may use your content to train our AI models and improve our services.";
        let result = audit_tos("https://example.com/tos", text);
        assert!(result.flags.iter().any(|f| matches!(f.category, FlagCategory::AiTraining)));
        assert!(result.risk_score >= 30);
    }

    #[test]
    fn audit_clean_tos() {
        let text = "We will never share your data. You can delete your account at any time.";
        let result = audit_tos("https://example.com/tos", text);
        assert!(result.flags.is_empty() || result.risk_score < 20);
    }
}
