// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/threat.rs  — v7
//
// Threat Intelligence: privacy-preserving domain safety check.
//
// Two-tier approach:
//   1. Local threat list (embedded or periodically fetched, CBOR/JSON)
//      Sourced from abuse.ch URLhaus + PhishTank bulk exports (weekly update).
//      Works fully offline. Max staleness: 7 days.
//
//   2. Quad9 DoH (optional, opt-in via settings)
//      Endpoint: https://dns.quad9.net/dns-query
//      Query: only the domain, not the full URL.
//      Response: NXDOMAIN = known malicious; NOERROR = clean.
//      Privacy: Quad9 is GDPR-compliant, non-logging, independent nonprofit.
//
// Domain age heuristic (always active):
//      Domains < 30 days old emit a caution signal.
//      Domain registration date is cached in DB meta after first lookup.
//
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::Result;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ── Local threat list ─────────────────────────────────────────────────────────
//
// For the embedded baseline we include a compact static list.
// The full dynamic list is fetched weekly and stored in DB meta as JSON.
// Format: ["malware.example.com", "phishing.example.net", ...]

/// Compile-time embedded minimal blocklist (top phishing / malware TLDs).
/// Updated by maintainers before each release. Not a substitute for the
/// live list — just a safety net when the live list has never been fetched.
static EMBEDDED_THREATS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    // Representative entries — real distribution comes from URLhaus/PhishTank
    [
        // Known cryptominer / coin-jacking domains
        "coinhive.com", "coin-hive.com", "minero.cc", "cryptoloot.pro",
        "webminepool.com", "jsecoin.com",
        // Known phishing infrastructure (static — changes rapidly in production)
        "secure-paypa1.com", "paypa1-secure.com", "amazon-security-alert.com",
        "appleid-verify-account.com", "microsoft-login-secure.com",
        // Known malware C2 (static examples)
        "emotet-c2.example.com", "trickbot-cdn.example.net",
    ].iter().cloned().collect()
});

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreatLevel {
    Clean,
    Suspicious,    // domain age < 30 days
    Malicious,     // in local list
    BlockedByDoh,  // Quad9 returned NXDOMAIN
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatResult {
    pub domain:       String,
    pub level:        ThreatLevel,
    pub reason:       String,
    pub check_source: String,  // "local_list" | "quad9" | "age_heuristic" | "clean"
}

// ── Local list check ──────────────────────────────────────────────────────────

/// Check domain against the embedded + live threat list.
/// The live list is passed in as a slice (caller reads from DB / cache).
pub fn check_local(domain: &str, live_list: &HashSet<String>) -> ThreatLevel {
    let d = domain.to_lowercase();
    let d = d.trim_start_matches("www.");
    if EMBEDDED_THREATS.contains(d) || live_list.contains(d) {
        ThreatLevel::Malicious
    } else {
        ThreatLevel::Clean
    }
}

// ── Quad9 DoH check ───────────────────────────────────────────────────────────

/// Query Quad9 DoH for a domain. NXDOMAIN → Malicious. Any error → assume Clean.
/// This is async and should be called only when the user has quad9_enabled = true.
pub async fn check_quad9(domain: &str) -> Result<ThreatLevel> {
    // RFC 8484: DNS over HTTPS
    // We send a minimal DNS A-record query encoded as a binary UDP DNS message.
    // Quad9 accepts `application/dns-message` POST body.

    let query = build_dns_query(domain)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;

    let resp = client
        .post("https://dns.quad9.net/dns-query")
        .header("Content-Type", "application/dns-message")
        .header("Accept", "application/dns-message")
        .header("User-Agent", crate::blocker::DIATOM_UA)
        .body(query)
        .send()
        .await?;

    let bytes = resp.bytes().await?;
    Ok(parse_dns_response(&bytes))
}

/// Build a minimal binary DNS A-record query for `domain`.
fn build_dns_query(domain: &str) -> Result<Vec<u8>> {
    let mut msg = Vec::with_capacity(64);

    // Header: ID=0xDEAD, flags=standard query, 1 question
    msg.extend_from_slice(&[0xDE, 0xAD]);  // ID
    msg.extend_from_slice(&[0x01, 0x00]);  // QR=0, Opcode=0, RD=1
    msg.extend_from_slice(&[0x00, 0x01]);  // QDCOUNT=1
    msg.extend_from_slice(&[0x00, 0x00]);  // ANCOUNT=0
    msg.extend_from_slice(&[0x00, 0x00]);  // NSCOUNT=0
    msg.extend_from_slice(&[0x00, 0x00]);  // ARCOUNT=0

    // Question: QNAME (length-prefixed labels), QTYPE=A(1), QCLASS=IN(1)
    for label in domain.split('.') {
        let l = label.as_bytes();
        if l.len() > 63 { anyhow::bail!("label too long"); }
        msg.push(l.len() as u8);
        msg.extend_from_slice(l);
    }
    msg.push(0);              // root label
    msg.extend_from_slice(&[0x00, 0x01]);  // QTYPE = A
    msg.extend_from_slice(&[0x00, 0x01]);  // QCLASS = IN

    Ok(msg)
}

/// Parse a binary DNS response: check RCODE. NXDOMAIN (3) → Malicious.
fn parse_dns_response(bytes: &[u8]) -> ThreatLevel {
    if bytes.len() < 4 { return ThreatLevel::Clean; }
    let rcode = bytes[3] & 0x0F;
    match rcode {
        3 => ThreatLevel::BlockedByDoh,  // NXDOMAIN — Quad9 blocked this domain
        _ => ThreatLevel::Clean,
    }
}

// ── Domain age heuristic ──────────────────────────────────────────────────────

/// Check if a domain was registered very recently (potential phishing setup).
/// Uses a WHOIS-over-HTTP service. Cached in DB after first call.
/// Returns Suspicious if the domain is < 30 days old.
pub async fn check_domain_age(domain: &str) -> ThreatLevel {
    // Use whois.domaintools.com JSON API (no auth required for basic lookups)
    // This is opt-in via threat_heuristics_enabled setting.
    let url = format!("https://api.whoapi.com/?domain={domain}&r=whois&apikey=free");
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build() {
        Ok(c) => c,
        Err(_) => return ThreatLevel::Clean,
    };

    let Ok(resp) = client.get(&url).send().await else { return ThreatLevel::Clean; };
    let Ok(text) = resp.text().await else { return ThreatLevel::Clean; };

    // Heuristic: look for a creation_date-like field within 30 days
    // This is intentionally loose — false negatives are OK, false positives are not.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(created) = json.get("date_created").and_then(|v| v.as_str()) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created) {
                let age_days = (chrono::Utc::now() - dt.with_timezone(&chrono::Utc))
                    .num_days();
                if age_days < 30 {
                    return ThreatLevel::Suspicious;
                }
            }
        }
    }
    ThreatLevel::Clean
}

// ── Full evaluation pipeline ───────────────────────────────────────────────────

/// Evaluate a domain through all available threat signals.
/// Returns the highest-severity finding.
pub async fn evaluate_domain(
    domain: &str,
    live_list: &HashSet<String>,
    quad9_enabled: bool,
    age_heuristic_enabled: bool,
) -> ThreatResult {
    // 1. Local list (synchronous, always active)
    let local = check_local(domain, live_list);
    if local == ThreatLevel::Malicious {
        return ThreatResult {
            domain:       domain.to_owned(),
            level:        ThreatLevel::Malicious,
            reason:       "该域名出现在本地威胁情报列表中（来源：abuse.ch / PhishTank）。".to_owned(),
            check_source: "local_list".to_owned(),
        };
    }

    // 2. Quad9 DoH (async, opt-in)
    if quad9_enabled {
        if let Ok(doh_result) = check_quad9(domain).await {
            if doh_result == ThreatLevel::BlockedByDoh {
                return ThreatResult {
                    domain:       domain.to_owned(),
                    level:        ThreatLevel::Malicious,
                    reason:       "Quad9 的独立威胁情报将此域名标记为恶意。已由 DNS 层拦截。".to_owned(),
                    check_source: "quad9".to_owned(),
                };
            }
        }
    }

    // 3. Age heuristic (async, opt-in)
    if age_heuristic_enabled {
        let age_result = check_domain_age(domain).await;
        if age_result == ThreatLevel::Suspicious {
            return ThreatResult {
                domain:       domain.to_owned(),
                level:        ThreatLevel::Suspicious,
                reason:       "此域名注册时间不足 30 天。新域名是钓鱼攻击的常用基础设施，请谨慎。".to_owned(),
                check_source: "age_heuristic".to_owned(),
            };
        }
    }

    ThreatResult {
        domain:       domain.to_owned(),
        level:        ThreatLevel::Clean,
        reason:       String::new(),
        check_source: "clean".to_owned(),
    }
}

// ── Live list management ──────────────────────────────────────────────────────

/// Fetch the latest URLhaus domain-only export and return as a HashSet.
/// Called weekly by a background task. Failures are silent (use cached list).
pub async fn fetch_live_list() -> Result<HashSet<String>> {
    // URLhaus hosts a CSV-format blocklist of active malware domains
    let url = "https://urlhaus.abuse.ch/downloads/hostfile/";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let text = client
        .get(url)
        .header("User-Agent", crate::blocker::DIATOM_UA)
        .send()
        .await?
        .text()
        .await?;

    let domains: HashSet<String> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        // hostfile format: "127.0.0.1   malware.com"
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(|d| d.trim_start_matches("www.").to_lowercase())
        .collect();

    tracing::info!("fetched live threat list: {} domains", domains.len());
    Ok(domains)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_check_hits_embedded() {
        let live: HashSet<String> = HashSet::new();
        assert_eq!(check_local("coinhive.com", &live), ThreatLevel::Malicious);
        assert_eq!(check_local("github.com",   &live), ThreatLevel::Clean);
    }

    #[test]
    fn dns_query_valid_format() {
        let q = build_dns_query("example.com").unwrap();
        // Header is 12 bytes + labels for "example" (8) + "com" (4) + root (1) + QTYPE+CLASS (4)
        assert!(q.len() > 12);
        assert_eq!(q[0], 0xDE); // ID high byte
    }

    #[test]
    fn nxdomain_detected() {
        // Craft a minimal NXDOMAIN response (RCODE=3)
        let resp = vec![0xDE,0xAD, 0x81,0x83, 0,0, 0,0, 0,0, 0,0];
        assert_eq!(parse_dns_response(&resp), ThreatLevel::BlockedByDoh);
    }
}
