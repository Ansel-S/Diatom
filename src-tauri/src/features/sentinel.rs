
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};


/// Sentinel polls Chrome version history for these platforms.
const CHROME_PLATFORMS: &[(&str, &str)] = &[
    ("win64", "windows"),
    ("mac_arm64", "mac"),
    ("linux", "linux"),
];

/// Poll interval: 60 minutes in seconds.
pub const POLL_INTERVAL_S: u64 = 3_600;


/// Fallback static WebKit build table (used when Sentinel cache is cold).
/// [FIX-SENTINEL-01] Retained as fallback; no longer the sole source of truth.
const SAFARI_WEBKIT_BUILDS: &[(u32, u32, u32, u32)] = &[
    (18, 5, 619, 5),  // projected
    (18, 4, 619, 4),
    (18, 3, 619, 3),
    (18, 2, 619, 2),
    (18, 1, 619, 1),
    (18, 0, 619, 1),
    (17, 6, 605, 1),
    (17, 5, 605, 1),
    (17, 4, 605, 1),
    (17, 3, 605, 1),
    (17, 2, 605, 1),
    (17, 1, 605, 1),
    (17, 0, 605, 1),
    (16, 6, 615, 3),
    (16, 5, 615, 3),
    (16, 4, 615, 3),
    (16, 3, 615, 3),
    (16, 2, 614, 4),
    (16, 1, 614, 3),
    (16, 0, 614, 3),
    (15, 6, 612, 3),
    (15, 5, 612, 2),
];

/// Global Sentinel cache — populated by `run_sentinel_loop`, read by
/// `webkit_build_for` for fast UA synthesis without locking AppState.
///
/// [FIX-B02] This static was referenced in `webkit_build_for()` but never
/// defined anywhere, causing a compile error. Initialised lazily here;
/// `run_sentinel_loop` calls `set_global_cache()` after each successful refresh.
static SENTINEL_CACHE: OnceLock<Mutex<SentinelCache>> = OnceLock::new();

/// Update the global Sentinel cache after a successful refresh.
pub fn set_global_cache(cache: SentinelCache) {
    let m = SENTINEL_CACHE.get_or_init(|| Mutex::new(SentinelCache::default()));
    if let Ok(mut guard) = m.lock() {
        *guard = cache;
    }
}

/// Canonical WebKit UA build string for a given Safari version.
///
/// [FIX-SENTINEL-01] Now queries the live SentinelCache first.
/// [B-04 FIX] The previous implementation returned a hardcoded "619" for any
/// unknown Safari major (e.g. Safari 19 would get WebKit/619.x.15 — the Safari
/// 18 build number — producing a detectable anachronism). Now returns a
/// SENTINEL_STALE result and logs a warning for unknown majors, forcing the
/// static table to be updated. The raw version-derived string is used as a
/// best-effort fallback with a clear log marker.
///
/// Returns e.g. "619.3.15" for Safari 18.3.
pub fn webkit_build_for(safari_major: u32, safari_minor: u32) -> String {
    if let Some(cache) = SENTINEL_CACHE.get().and_then(|m| m.lock().ok()) {
        if let Some(ref safari) = cache.safari {
            if safari.major > safari_major
                || (safari.major == safari_major && safari.minor >= safari_minor)
            {
                let wk_major = webkit_major_for_safari_major(safari.major);
                let wk_sub = safari_minor;
                return format!("{}.{}.15", wk_major, wk_sub);
            }
        }
    }

    if let Some(row) = SAFARI_WEBKIT_BUILDS
        .iter()
        .find(|&&(mj, mn, _, _)| mj == safari_major && mn == safari_minor)
    {
        return format!("{}.{}.15", row.2, row.3);
    }
    if let Some(row) = SAFARI_WEBKIT_BUILDS
        .iter()
        .filter(|&&(mj, _, _, _)| mj == safari_major)
        .max_by_key(|&&(_, mn, _, _)| mn)
    {
        return format!("{}.{}.15", row.2, row.3);
    }
    tracing::warn!(
        "sentinel: webkit_build_for({}, {}) — unknown Safari major; \
         SENTINEL_STALE: update SAFARI_WEBKIT_BUILDS table",
        safari_major, safari_minor
    );
    let wk_major = webkit_major_for_safari_major(safari_major);
    format!("{}.{}.15 /* SENTINEL_STALE */", wk_major, safari_minor)
}

/// Map a Safari major version to the corresponding WebKit major number.
///
/// [B-04 FIX] Extracted from webkit_build_for() catch-all to make the
/// unknown-major path explicit and testable.
fn webkit_major_for_safari_major(safari_major: u32) -> u32 {
    match safari_major {
        19 => {
            tracing::warn!("sentinel: Safari 19 detected — webkit_major_for_safari_major \
                           needs updating. Returning provisional estimate.");
            625 // provisional; must be updated when Apple ships Safari 19
        }
        18 => 619,
        17 => 605,
        16 => 615,
        15 => 612,
        _ => {
            tracing::warn!(
                "sentinel: completely unknown Safari major {}; returning 619 as \
                 placeholder. SENTINEL_STALE: table update required.",
                safari_major
            );
            619
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChromeVersionInfo {
    /// Full version string, e.g. "124.0.6367.207"
    pub version: String,
    /// Major version number, e.g. 124
    pub major: u32,
    /// Platform key, e.g. "win64"
    pub platform: String,
    /// Channel ("stable" | "extended")
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SafariVersionInfo {
    /// Full version string, e.g. "17.6"
    pub version: String,
    /// Major, e.g. 17
    pub major: u32,
    /// Minor, e.g. 6
    pub minor: u32,
    /// Synthesised WebKit build string, e.g. "605.1.15"
    pub webkit_build: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SentinelCache {
    /// Chrome stable versions per platform.
    pub chrome: Vec<ChromeVersionInfo>,
    /// Chrome Extended Stable (enterprise-relevant).
    pub chrome_extended: Option<ChromeVersionInfo>,
    /// Safari / WebKit version.
    pub safari: Option<SafariVersionInfo>,
    /// Unix timestamp of the last successful refresh.
    pub last_refresh: u64,
    /// Whether the last refresh fully succeeded.
    pub refresh_ok: bool,
    /// Number of consecutive refresh failures.
    pub fail_streak: u32,
    /// True if any critical CVE was detected in the current Chrome version.
    pub cve_critical: bool,
    /// List of CVE IDs in the current Chrome release notes (max 20).
    pub recent_cves: Vec<String>,
    /// Detected previous Chrome major (used to emit engine-upgrade event).
    pub prev_chrome_major: u32,
}

impl SentinelCache {
    /// Returns the Chrome stable version for Windows (used for Windows UA).
    pub fn chrome_win(&self) -> Option<&ChromeVersionInfo> {
        self.chrome.iter().find(|v| v.platform == "windows")
    }

    /// Returns the Chrome stable version for macOS.
    pub fn chrome_mac(&self) -> Option<&ChromeVersionInfo> {
        self.chrome.iter().find(|v| v.platform == "mac")
    }

    /// True if the cache is fresh (within 2× poll interval).
    pub fn is_fresh(&self) -> bool {
        let now = unix_now();
        now.saturating_sub(self.last_refresh) < POLL_INTERVAL_S * 2
    }

    /// Synthesise a Windows Chrome UA string with the full version number.
    /// [FIX-25] Real Chrome uses the full 4-part version (e.g. 124.0.6367.207),
    /// not the truncated major.0.0.0 form which is detectable by fingerprint scanners.
    pub fn chrome_ua_windows(&self) -> String {
        let ver = self.chrome_win()
            .map(|v| v.version.as_str())
            .unwrap_or("124.0.6367.207");
        format!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/{ver} Safari/537.36"
        )
    }

    /// Synthesise a macOS Safari UA string.
    pub fn safari_ua_macos(&self) -> String {
        match &self.safari {
            Some(s) => {
                let wb = &s.webkit_build;
                let v = &s.version;
                format!(
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                     AppleWebKit/{wb} (KHTML, like Gecko) \
                     Version/{v} Safari/{wb}"
                )
            }
            None => {
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/619.1.26 (KHTML, like Gecko) \
                 Version/18.0 Safari/619.1.26"
                    .to_owned()
            }
        }
    }
}


fn make_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(crate::engine::blocker::platform_fallback_ua())
        .build()?)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}


/// Response types for the Chrome Version History API v1.
#[derive(Deserialize)]
struct VersionHistoryResponse {
    versions: Vec<VersionEntry>,
}

#[derive(Deserialize)]
struct VersionEntry {
    version: String,
}

/// Fetch the latest Chrome Stable version for a given platform.
async fn fetch_chrome_stable(
    client: &reqwest::Client,
    platform: &str,
    display_name: &str,
) -> Result<ChromeVersionInfo> {
    let url = format!(
        "https://versionhistory.googleapis.com/v1/chrome/platforms/{platform}/\
         channels/stable/versions?filter=endtime%3Dnone&order_by=version+desc&pageSize=1"
    );
    let resp = client.get(&url).send().await?;
    let body: VersionHistoryResponse = resp.json().await?;
    let version = body
        .versions
        .into_iter()
        .next()
        .map(|v| v.version)
        .unwrap_or_else(|| "124.0.6367.207".to_owned());
    let major = version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(124);
    Ok(ChromeVersionInfo {
        version,
        major,
        platform: display_name.to_owned(),
        channel: "stable".to_owned(),
    })
}

async fn fetch_chrome_extended(client: &reqwest::Client) -> Result<ChromeVersionInfo> {
    let url = "https://versionhistory.googleapis.com/v1/chrome/platforms/win64/\
               channels/extended/versions?filter=endtime%3Dnone&order_by=version+desc&pageSize=1";
    let resp = client.get(url).send().await?;
    let body: VersionHistoryResponse = resp.json().await?;
    let version = body
        .versions
        .into_iter()
        .next()
        .map(|v| v.version)
        .unwrap_or_else(|| "124.0.6367.150".to_owned());
    let major = version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(124);
    Ok(ChromeVersionInfo {
        version,
        major,
        platform: "windows".to_owned(),
        channel: "extended".to_owned(),
    })
}


/// Parse Safari version from the Apple Developer News RSS feed.
/// Title format: "Safari 17.6 Release Notes" or "macOS 14.6 – Safari 17.6"
fn parse_safari_version(rss_text: &str) -> Option<(u32, u32)> {
    for line in rss_text.lines() {
        let line = line.trim();
        if !line.contains("Safari") {
            continue;
        }
        if let Some(pos) = line.find("Safari ") {
            let after = &line[pos + 7..];
            let version_str: String = after
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let parts: Vec<&str> = version_str.splitn(3, '.').collect();
            if parts.len() >= 2 {
                if let (Ok(mj), Ok(mn)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    if mj >= 16 {
                        return Some((mj, mn));
                    }
                }
            }
        }
    }
    None
}

async fn fetch_safari_version(client: &reqwest::Client) -> Result<SafariVersionInfo> {
    let rss_text = client
        .get("https://developer.apple.com/news/releases/rss/releases.rss")
        .send()
        .await?
        .text()
        .await?;

    let (major, minor) = parse_safari_version(&rss_text).unwrap_or((18, 0));
    let webkit_build = webkit_build_for(major, minor);
    Ok(SafariVersionInfo {
        version: format!("{}.{}", major, minor),
        major,
        minor,
        webkit_build,
    })
}


/// Check the Chrome Releases blog RSS for critical CVEs in the latest stable.
/// Returns (is_critical, cve_list).
async fn fetch_chrome_cves(client: &reqwest::Client) -> (bool, Vec<String>) {
    let Ok(resp) = client
        .get("https://chromereleases.googleblog.com/feeds/posts/default?max-results=3")
        .send()
        .await
    else {
        return (false, vec![]);
    };
    let Ok(text) = resp.text().await else {
        return (false, vec![]);
    };

    let mut cves: Vec<String> = Vec::new();
    let mut is_critical = false;

    for word in text.split_ascii_whitespace() {
        let clean: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        if clean.starts_with("CVE-") && clean.len() >= 13 {
            if !cves.contains(&clean) {
                cves.push(clean);
            }
        }
    }
    if text.contains("Critical") {
        is_critical = true;
    }
    cves.truncate(20);
    (is_critical, cves)
}


/// Perform a full Sentinel refresh. Returns the new cache state.
pub async fn refresh(prev_cache: &SentinelCache) -> SentinelCache {
    let client = match make_client() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!("sentinel: failed to build HTTP client: {}", err);
            let mut c = prev_cache.clone();
            c.fail_streak += 1;
            c.refresh_ok = false;
            return c;
        }
    };

    let mut new_cache = prev_cache.clone();
    let mut ok = true;

    let mut chrome_versions = Vec::new();
    for &(platform_key, display_name) in CHROME_PLATFORMS {
        match fetch_chrome_stable(&client, platform_key, display_name).await {
            Ok(info) => {
                tracing::info!(
                    "sentinel: Chrome {} stable → {}",
                    display_name,
                    info.version
                );
                chrome_versions.push(info);
            }
            Err(err) => {
                tracing::warn!(
                    "sentinel: Chrome {} fetch failed: {}",
                    display_name,
                    err
                );
                ok = false;
                if let Some(prev) = prev_cache.chrome.iter().find(|v| v.platform == display_name) {
                    chrome_versions.push(prev.clone());
                }
            }
        }
    }

    match fetch_chrome_extended(&client).await {
        Ok(ext) => {
            tracing::info!("sentinel: Chrome Extended Stable → {}", ext.version);
            new_cache.chrome_extended = Some(ext);
        }
        Err(err) => {
            tracing::warn!("sentinel: Chrome Extended fetch failed: {}", err);
            ok = false;
        }
    }

    match fetch_safari_version(&client).await {
        Ok(safari) => {
            tracing::info!("sentinel: Safari → {} (WebKit {})", safari.version, safari.webkit_build);
            new_cache.safari = Some(safari);
        }
        Err(err) => {
            tracing::warn!("sentinel: Safari fetch failed: {}", err);
            ok = false;
        }
    }

    let (cve_critical, cves) = fetch_chrome_cves(&client).await;
    if !cves.is_empty() {
        tracing::info!(
            "sentinel: CVE sweep → {} entries, critical={}",
            cves.len(),
            cve_critical
        );
    }

    let prev_major = prev_cache.prev_chrome_major;
    let new_major = chrome_versions
        .iter()
        .find(|v| v.platform == "windows")
        .map(|v| v.major)
        .unwrap_or(prev_major);

    if chrome_versions.len() > 0 {
        new_cache.chrome = chrome_versions;
    }
    new_cache.prev_chrome_major = new_major;
    new_cache.cve_critical = cve_critical;
    new_cache.recent_cves = cves;
    new_cache.last_refresh = unix_now();
    new_cache.refresh_ok = ok;
    new_cache.fail_streak = if ok { 0 } else { prev_cache.fail_streak + 1 };

    if prev_major > 0 && new_major > prev_major {
        tracing::info!(
            "sentinel: Chrome major version upgrade detected: {} → {}",
            prev_major,
            new_major
        );
    }

    new_cache
}


/// Spawn the sentinel background loop. Runs every POLL_INTERVAL_S seconds.
/// Caller passes `app_handle` so we can emit events and write state.
///
/// [AUDIT-FIX §2.2] `token` is a CancellationToken from AppState::shutdown_token.
/// When the main window is destroyed, token.cancel() is called, causing the
/// loop to exit promptly rather than waiting for the full POLL_INTERVAL_S sleep.
pub async fn run_sentinel_loop(
    app_handle: tauri::AppHandle,
    initial_delay_s: u64,
    token: tokio_util::sync::CancellationToken,
) {
    if initial_delay_s > 0 {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(initial_delay_s)) => {},
            _ = token.cancelled() => { return; },
        }
    }

    loop {
        let new_cache = {
            let prev = app_handle
                .try_state::<crate::state::AppState>()
                .map(|st| st.sentinel.lock().unwrap().clone())
                .unwrap_or_default();
            refresh(&prev).await
        };

        let prev_major = {
            app_handle
                .try_state::<crate::state::AppState>()
                .map(|st| st.sentinel.lock().unwrap().prev_chrome_major)
                .unwrap_or(0)
        };
        let new_major = new_cache
            .chrome
            .iter()
            .find(|v| v.platform == "windows")
            .map(|v| v.major)
            .unwrap_or(0);

        if let Some(st) = app_handle.try_state::<crate::state::AppState>() {
            *st.sentinel.lock().unwrap() = new_cache.clone();

            set_global_cache(new_cache.clone());

            if let Ok(json) = serde_json::to_string(&new_cache) {
                let _ = st.db.set_setting("sentinel_cache", &json);
            }
        }

        if prev_major > 0 && new_major > prev_major {
            let _ = app_handle.emit(
                "diatom:engine-upgrade",
                serde_json::json!({
                    "prev_major": prev_major,
                    "new_major": new_major,
                    "chrome_version": new_cache.chrome_win().map(|v| v.version.as_str()).unwrap_or("")
                }),
            );
        }
        if new_cache.cve_critical {
            let _ = app_handle.emit(
                "diatom:cve-critical",
                serde_json::json!({
                    "cves": new_cache.recent_cves,
                    "chrome_version": new_cache.chrome_win().map(|v| v.version.as_str()).unwrap_or("")
                }),
            );
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_S)) => {},
            _ = token.cancelled() => {
                tracing::info!("sentinel: shutdown signal received — exiting loop");
                return;
            },
        }
    }
}


#[derive(Debug, Clone, Serialize)]
pub struct SentinelStatus {
    pub cache: SentinelCache,
    /// Current synthesised Chrome UA (Windows).
    pub ua_chrome_win: String,
    /// Current synthesised Safari UA (macOS).
    pub ua_safari_mac: String,
    /// Lab is enabled?
    pub lab_active: bool,
    /// How many seconds until next scheduled refresh.
    pub next_refresh_in_s: u64,
}

impl SentinelStatus {
    pub fn from_cache(cache: &SentinelCache, lab_active: bool) -> Self {
        let ua_chrome_win = cache.chrome_ua_windows();
        let ua_safari_mac = cache.safari_ua_macos();
        let elapsed = unix_now().saturating_sub(cache.last_refresh);
        let next_refresh_in_s = POLL_INTERVAL_S.saturating_sub(elapsed);
        SentinelStatus {
            cache: cache.clone(),
            ua_chrome_win,
            ua_safari_mac,
            lab_active,
            next_refresh_in_s,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkit_build_known_version() {
        assert_eq!(webkit_build_for(18, 0), "619.1.15");
        assert_eq!(webkit_build_for(17, 6), "605.1.15");
    }

    #[test]
    fn webkit_build_unknown_minor_falls_back_to_same_major() {
        let build = webkit_build_for(18, 9);
        assert!(build.starts_with("619."), "expected 619.x.15, got: {build}");
    }

    /// [B-04 FIX] Unknown Safari major must NOT return a stale Safari 18 build.
    /// It must include SENTINEL_STALE so monitoring can catch it.
    #[test]
    fn webkit_build_unknown_major_returns_stale_marker() {
        let build = webkit_build_for(99, 0);
        assert!(
            build.contains("SENTINEL_STALE"),
            "unknown major must produce SENTINEL_STALE marker, got: {build}"
        );
    }

    #[test]
    fn parse_safari_rss_standard_title() {
        let rss = r#"<title>Safari 18.2 Release Notes</title>"#;
        assert_eq!(parse_safari_version(rss), Some((18, 2)));
    }

    #[test]
    fn parse_safari_rss_combined_title() {
        let rss = r#"<title>macOS Sequoia 15.2 – Safari 18.2</title>"#;
        assert_eq!(parse_safari_version(rss), Some((18, 2)));
    }

    #[test]
    fn chrome_ua_synthesis() {
        let mut cache = SentinelCache::default();
        cache.chrome.push(ChromeVersionInfo {
            version: "124.0.6367.207".to_owned(),
            major: 124,
            platform: "windows".to_owned(),
            channel: "stable".to_owned(),
        });
        let ua = cache.chrome_ua_windows();
        assert!(ua.contains("Chrome/124.0.6367.207"), "UA should use full version: {ua}");
        assert!(ua.contains("Windows NT 10.0"));
    }

    #[test]
    fn safari_ua_synthesis_from_cache() {
        let mut cache = SentinelCache::default();
        cache.safari = Some(SafariVersionInfo {
            version: "17.6".to_owned(),
            major: 17,
            minor: 6,
            webkit_build: "605.1.15".to_owned(),
        });
        let ua = cache.safari_ua_macos();
        assert!(ua.contains("Version/17.6"));
        assert!(ua.contains("AppleWebKit/605.1.15"));
    }

    #[test]
    fn cache_freshness() {
        let mut cache = SentinelCache::default();
        assert!(!cache.is_fresh());
        cache.last_refresh = unix_now();
        assert!(cache.is_fresh());
    }
}

