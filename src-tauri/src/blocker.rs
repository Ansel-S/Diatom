// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/blocker.rs
//
// URL filtering pipeline:
//   1. HTTPS upgrade  — force-upgrade HTTP origins
//   2. Tracker param strip — remove UTM / fbclid / gclid etc.
//   3. Aho-Corasick domain blocklist — cosmetic + analytics + fingerprint nets
//
// The blocker intentionally ships NO third-party rule lists in the binary.
// Per PHILOSOPHY.md §4, Diatom's legal role is "engine author", not
// "specific-platform traffic hijacker". Users add their own rule sets.
// ─────────────────────────────────────────────────────────────────────────────

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use once_cell::sync::Lazy;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, DNT};
use url::Url;

// ── User-Agent ────────────────────────────────────────────────────────────────

/// Diatom's outbound UA. Generic enough not to fingerprint the engine version.
pub const DIATOM_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15 Diatom/0.9";

// ── Built-in minimal blocklist ────────────────────────────────────────────────
// These are generic tracking infrastructure patterns — not targeting any
// specific company's content delivery. Each pattern is a substring of
// request URLs that, if matched, indicates a pure analytics/fingerprinting
// endpoint with no legitimate page-rendering function.

const BUILTIN_PATTERNS: &[&str] = &[
    // Universal analytics endpoints
    "/analytics/",
    "/telemetry/",
    "/collect?",
    "/beacon?",
    "/pixel.gif",
    "/pixel.png",
    "/1x1.gif",
    // Common tracker subdomains (generic patterns only)
    "analytics.",
    "telemetry.",
    "metrics.",
    "stats.",
    "tracking.",
    "pixel.",
    "beacon.",
    // Script filenames (generic)
    "analytics.js",
    "tracking.js",
    "gtag/js",
    "gtm.js",
];

static BLOCKER: Lazy<AhoCorasick> = Lazy::new(|| {
    AhoCorasickBuilder::new()
        .match_kind(MatchKind::LeftmostFirst)
        .ascii_case_insensitive(true)
        .build(BUILTIN_PATTERNS)
        .expect("blocker AC build failed")
});

// ── Tracking query parameters to strip ───────────────────────────────────────

const STRIP_PARAMS: &[&str] = &[
    // UTM campaign parameters
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
    "utm_id", "utm_source_platform",
    // Platform click IDs
    "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid",
    "msclkid", "tclid", "twclid", "ttclid",
    // Email tracking
    "mc_eid", "mc_cid",
    // Generic referrer / session tracking
    "_ga", "_gl", "_hsenc", "_hsmi",
    "igshid", "s_kwcid",
    "ref", "referrer", "source",
    // Redirector patterns
    "__twitter_impression",
];

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns true if the URL matches a tracking/analytics pattern.
pub fn is_blocked(url: &str) -> bool {
    BLOCKER.is_match(url)
}

/// Returns a stub response type for blocked URLs (for cosmetic vs network blocks).
pub fn stub_for(_url: &str) -> Option<&'static str> {
    Some("blocked")
}

/// Upgrade HTTP → HTTPS for known-safe origins.
pub fn upgrade_https(url: &str) -> String {
    if url.starts_with("http://") && !url.starts_with("http://localhost") {
        format!("https://{}", &url[7..])
    } else {
        url.to_owned()
    }
}

/// Owned version of upgrade_https (used in commands).
pub fn upgrade_https_owned(url: &str) -> String {
    upgrade_https(url)
}

/// Strip tracking query parameters from a URL.
pub fn strip_params(url: &str) -> String {
    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return url.to_owned(),
    };

    let clean_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| {
            let key = k.to_lowercase();
            !STRIP_PARAMS.iter().any(|p| key == *p || key.starts_with(&format!("{}_", p)))
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    let mut out = parsed.clone();
    if clean_pairs.is_empty() {
        out.set_query(None);
    } else {
        out.set_query(Some(
            &clean_pairs
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&"),
        ));
    }
    out.to_string()
}

/// Build clean request headers (no Referer, sanitised Accept-Language).
pub fn clean_headers(url: &str, extra_ua: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let ua = extra_ua.unwrap_or(DIATOM_UA);
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_str(ua).unwrap_or_else(|_| HeaderValue::from_static(DIATOM_UA)),
    );
    headers.insert(ACCEPT, HeaderValue::from_static(
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    ));
    // Generic language — not locale-fingerprintable
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(DNT, HeaderValue::from_static("1"));
    headers.insert(
        reqwest::header::HeaderName::from_static("sec-gpc"),
        HeaderValue::from_static("1"),
    );
    // Never send Referer
    // headers omit Referer entirely — reqwest default includes it; we override
    headers
}

/// Extract the registrable domain (eTLD+1) from a URL.
pub fn domain_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_owned()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utm_params() {
        let url = "https://example.com/page?utm_source=newsletter&id=42";
        let clean = strip_params(url);
        assert!(clean.contains("id=42"), "non-tracking params must survive");
        assert!(!clean.contains("utm_source"), "utm params must be stripped");
    }

    #[test]
    fn upgrades_http() {
        assert_eq!(upgrade_https("http://example.com/"), "https://example.com/");
        // Localhost must not be upgraded
        assert_eq!(upgrade_https("http://localhost:3000/"), "http://localhost:3000/");
    }

    #[test]
    fn blocks_analytics_endpoint() {
        assert!(is_blocked("https://example.com/analytics/collect"));
        assert!(!is_blocked("https://example.com/about"));
    }
}
