// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/dom_crusher.rs  — v7
//
// DOM Crusher: persistent per-domain CSS selector blocking.
//
// Flow:
//   1. User Ctrl+clicks an element in the page.
//   2. diatom-api.js generates a minimal CSS selector and calls
//      cmd_dom_crush(domain, selector).
//   3. This module validates + stores the rule in dom_blocks.
//   4. On every page load for that domain, diatom-api.js calls
//      cmd_dom_blocks_for(domain) and removes matching elements before first paint.
//
// Selector validation:
//   • Max length 512 chars (prevents injection attacks via overly long selectors).
//   • Must not start with dangerous pseudo-elements: :root, :host, html, body
//     (prevents crushing the entire page).
//   • No `*` wildcard at the root level (prevents crushing everything).
//   • No embedded <script> or javascript: (XSS guard).
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{Result, bail};

const MAX_SELECTOR_LEN: usize = 512;

/// Dangerous selectors that would crush the entire page
const DISALLOWED_ROOTS: &[&str] = &[
    ":root", ":host", "html ", "html>", "html,", "body ", "body>", "body,", "* {", "*{",
];

/// Validate a CSS selector before storing it.
pub fn validate_selector(selector: &str) -> Result<()> {
    let s = selector.trim();

    if s.is_empty() {
        bail!("selector cannot be empty");
    }
    if s.len() > MAX_SELECTOR_LEN {
        bail!("selector too long (max {MAX_SELECTOR_LEN} chars)");
    }
    // XSS guard
    if s.contains('<') || s.contains("javascript:") {
        bail!("selector contains forbidden characters");
    }
    // Prevent nuking the whole page
    let lower = s.to_lowercase();
    if lower.trim_start() == "*" || lower.trim_start().starts_with("* ") {
        bail!("wildcard-only selectors are not allowed");
    }
    for dis in DISALLOWED_ROOTS {
        if lower.starts_with(dis) || lower == dis.trim() {
            bail!("selector targets a root element — this would crush the entire page");
        }
    }
    Ok(())
}

/// Generate a minimal, stable CSS selector from the element's path info.
/// Called from diatom-api.js — this Rust function is exposed as a Tauri command
/// so the JS side can ask for validation and cleaning server-side.
pub fn clean_selector(selector: &str) -> String {
    // Normalise whitespace
    let s = selector.split_whitespace().collect::<Vec<_>>().join(" ");
    // Trim leading/trailing punctuation noise
    s.trim_matches(|c: char| c == ',' || c == ';').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_selector_passes() {
        assert!(validate_selector(".cookie-banner").is_ok());
        assert!(validate_selector("#newsletter-modal").is_ok());
        assert!(validate_selector("div.sticky-header > button.close").is_ok());
        assert!(validate_selector("[data-testid='promo-bar']").is_ok());
    }

    #[test]
    fn dangerous_selectors_blocked() {
        assert!(validate_selector("*").is_err());
        assert!(validate_selector("html").is_err());
        assert!(validate_selector(":root").is_err());
        assert!(validate_selector("<script>alert(1)</script>").is_err());
        assert!(validate_selector("div[onclick='javascript:void(0)']").is_err());
    }

    #[test]
    fn length_limit_enforced() {
        let long = "a".repeat(513);
        assert!(validate_selector(&long).is_err());
    }
}

// ── DOM Reshuffler — Element Rearrangement ─────────────────────────────────────────────────
// [FIX-DOM-02] DOM Reshuffle merged into existing dom_crusher.rs (same "Page Content Rewrite" section).
//
// Function: lets users rearrange pages like building blocks.
//   - Ad slots are not merely hidden; instead they are replaced with Museum cards or custom widgets.
//   - Sidebars can be replaced with TOTP codes, RSS feeds, etc.
//
// Implementation: JS injection script + Tauri command cmd_dom_reshuffle_set
//   Frontend configures "replacement rules": selector → replacement_type.
//   Backend persists rules; they are injected at startup via initialization_script.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReshuffleRule {
    pub rule_id: String,
    pub domain_pattern: String,  // Supports wildcard: *.reddit.com
    pub selector: String,        // CSS selectors
    pub replacement: ReplacementContent,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ReplacementContent {
 /// Replace matched elements with Museum archive cards (randomly selected from the archive).
    MuseumCard { museum_id: Option<String> },
    /// replaced with TOTP TOTP codesdisplay
    TotpWidget { issuer_filter: Option<String> },
 /// replaced with HTML
    CustomHtml { html: String },
 /// Replace matched elements with a Museum diff view (shows content changes over time).
    Blank,
}

/// generate DOM Reshuffler injection script
pub fn reshuffle_script(rules: &[ReshuffleRule]) -> String {
    if rules.is_empty() { return String::new(); }

    let rules_json = serde_json::to_string(rules).unwrap_or_default();
    format!(r#"
(function diatomReshuffler() {{
  const RULES = {rules_json};
  const host = location.hostname;

  function matchesDomain(pattern) {{
    if (pattern === '*') return true;
    if (pattern.startsWith('*.')) {{
      return host.endsWith(pattern.slice(1));
    }}
    return host === pattern || host.endsWith('.' + pattern);
  }}

  function applyRule(rule) {{
    if (!rule.enabled) return;
    if (!matchesDomain(rule.domain_pattern)) return;
    document.querySelectorAll(rule.selector).forEach(el => {{
      switch (rule.replacement.type) {{
        case 'blank':
          el.style.cssText = 'visibility:hidden!important;height:0!important;overflow:hidden!important;';
          break;
        case 'custom_html':
          el.innerHTML = rule.replacement.html;
          el.style.border = '1px dashed rgba(96,165,250,.3)';
          el.title = 'Reshaped by Diatom';
          break;
        case 'museum_card':
 el.innerHTML = '<div style="padding:1rem;background:var(--c-surface,#1e293b);border:1px solid rgba(96,165,250,.2);border-radius:.5rem;color:#94a3b8;font-size:.75rem">📚 Museum archive (loading...)</div>';
          window.__TAURI__?.core?.invoke('cmd_museum_random_card').then(card => {{
            if (card) el.innerHTML = `<div style="padding:.75rem;background:var(--c-surface,#1e293b);border:1px solid rgba(96,165,250,.2);border-radius:.5rem"><a href="${{card.url}}" style="color:#60a5fa;text-decoration:none;font-size:.8rem;font-weight:500">${{card.title}}</a><p style="color:#94a3b8;font-size:.72rem;margin:.3rem 0 0">${{card.snippet}}</p></div>`;
          }}).catch(() => {{}});
          break;
      }}
    }});
  }}

  function runAll() {{ RULES.forEach(applyRule); }}
  runAll();
  const obs = new MutationObserver(() => runAll());
  obs.observe(document.body, {{ childList: true, subtree: true }});
}})();
"#, rules_json=rules_json)
}
