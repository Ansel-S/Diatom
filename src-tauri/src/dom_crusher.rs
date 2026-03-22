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

use anyhow::{bail, Result};

const MAX_SELECTOR_LEN: usize = 512;

/// Dangerous selectors that would crush the entire page
const DISALLOWED_ROOTS: &[&str] = &[
    ":root", ":host", "html ", "html>", "html,",
    "body ", "body>", "body,", "* {", "*{",
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
    let s = selector
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
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
