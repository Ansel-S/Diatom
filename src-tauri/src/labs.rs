// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/labs.rs  — v0.9.2
//
// [FIX-14] sentinel_ua lab added to all_labs() catalogue.
// [NEW] privacy_presets, nostr_relay, webauthn_bridge labs added.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LabStability { Alpha, Beta, Stable }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LabRisk { Low, Medium, High }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lab {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub stability: LabStability,
    pub risk: LabRisk,
    pub risk_note: &'static str,
    pub enabled: bool,
    pub restart_required: bool,
    pub added_in: &'static str,
}

pub fn all_labs() -> Vec<Lab> {
    vec![
        // ── Privacy ──────────────────────────────────────────────────────────
        Lab {
            id: "sentinel_ua",
            name: "Dynamic User-Agent (Sentinel)",
            description: "Automatically tracks the current stable Chrome and Safari versions \
                          and synthesises a matching User-Agent string. Diatom blends into \
                          the most common browser population rather than advertising itself.",
            category: "Privacy",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Requires one network poll per hour to versionhistory.googleapis.com \
                        and developer.apple.com. Polls use the generic fallback UA so Diatom \
                        does not fingerprint itself during the poll.",
            enabled: true,   // Default ON — this is a core privacy feature
            restart_required: false,
            added_in: "v0.9.1",
        },
        Lab {
            id: "pqc_envelope",
            name: "Post-Quantum Freeze Encryption",
            description: "Encrypts Museum bundles with Kyber-768 + AES-256-GCM hybrid envelope.",
            category: "Privacy",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Bundles encrypted with this flag cannot be decrypted by older Diatom versions.",
            enabled: false,
            restart_required: false,
            added_in: "v0.8.0",
        },
        Lab {
            id: "ohttp_decoy",
            name: "OHTTP Decoy Relay",
            description: "Routes privacy-noise requests through Oblivious HTTP relays.",
            category: "Privacy",
            stability: LabStability::Alpha,
            risk: LabRisk::Medium,
            risk_note: "Response decapsulation is not yet implemented — decoy requests only.",
            enabled: false,
            restart_required: false,
            added_in: "v0.8.0",
        },
        Lab {
            id: "zkp_age_gate",
            name: "Zero-Knowledge Age Verification",
            description: "Proves age threshold to participating sites without revealing birth year.",
            category: "Privacy",
            stability: LabStability::Alpha,
            risk: LabRisk::High,
            risk_note: "Proof transport is non-standard. Sites must opt-in.",
            enabled: false,
            restart_required: false,
            added_in: "v0.8.0",
        },
        Lab {
            id: "timezone_spoof",
            name: "Timezone Normalisation",
            description: "Overrides JS timezone APIs to report UTC, preventing locale fingerprinting.",
            category: "Privacy",
            stability: LabStability::Beta,
            risk: LabRisk::Medium,
            risk_note: "Calendar apps and trading platforms may display incorrect times.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.0",
        },
        Lab {
            id: "jitter_all_requests",
            name: "Global Request Jitter",
            description: "Adds 0–50ms cryptographic delay to every outbound request, \
                          defeating timing correlation attacks.",
            category: "Privacy",
            stability: LabStability::Stable,
            risk: LabRisk::Low,
            risk_note: "Adds up to 50ms latency per request. Imperceptible on most connections.",
            enabled: false,
            restart_required: false,
            added_in: "v0.8.0",
        },
        // ── AI ────────────────────────────────────────────────────────────────
        Lab {
            id: "slm_server",
            name: "Local AI Microkernel",
            description: "Starts an OpenAI-compatible API server at 127.0.0.1:11435.",
            category: "AI",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Opens a local TCP port. Bound to loopback — not accessible from the network.",
            enabled: false,
            restart_required: true,
            added_in: "v0.9.0",
        },
        Lab {
            id: "slm_extreme_privacy",
            name: "AI Extreme Privacy Mode",
            description: "Forces all AI inference into the Wasm sandbox. \
                          Disables Ollama and llama.cpp backends.",
            category: "AI",
            stability: LabStability::Alpha,
            risk: LabRisk::Low,
            risk_note: "Wasm inference is 5–20× slower. Only works for short prompts.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.0",
        },
        Lab {
            id: "page_summarise",
            name: "Instant Page Summariser",
            description: "Adds a ⌘K shortcut that summarises the current page using the active SLM.",
            category: "AI",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Page content is sent to the local model only — never to external servers.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.0",
        },
        // ── Performance ───────────────────────────────────────────────────────
        Lab {
            id: "dynamic_tab_budget",
            name: "Adaptive Tab Limit",
            description: "Memory-aware tab limit using Resource-Aware Scaling, \
                          Golden Ratio zones, and Screen Gravity.",
            category: "Performance",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Budget recalculates every 60 seconds.",
            enabled: true,
            restart_required: false,
            added_in: "v0.9.0",
        },
        Lab {
            id: "golden_ratio_zones",
            name: "Golden Ratio Tab Zones",
            description: "Focus zone (61.8%) vs Buffer zone (38.2%) tab scheduling.",
            category: "Performance",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "None. Cosmetic behavioural change only.",
            enabled: true,
            restart_required: false,
            added_in: "v0.9.0",
        },
        Lab {
            id: "entropy_sleep",
            name: "Entropy-Reduction Sleep",
            description: "Shortens auto-sleep timer as tabs approach the budget limit.",
            category: "Performance",
            stability: LabStability::Stable,
            risk: LabRisk::Low,
            risk_note: "May surprise users who expect tabs to stay alive longer under load.",
            enabled: true,
            restart_required: false,
            added_in: "v0.9.0",
        },
        // ── Sync ─────────────────────────────────────────────────────────────
        Lab {
            id: "crdt_museum_sync",
            name: "P2P Museum Sync (mDNS)",
            description: "Synchronises frozen page archives between local devices using OR-Set CRDTs.",
            category: "Sync",
            stability: LabStability::Alpha,
            risk: LabRisk::Medium,
            risk_note: "Requires both devices on the same LAN simultaneously.",
            enabled: false,
            restart_required: false,
            added_in: "v0.8.0",
        },
        Lab {
            id: "nostr_relay_sync",
            name: "Async Nostr Relay Sync",
            description: "Push encrypted Museum bundles to a user-chosen Nostr relay. \
                          Relay sees only ciphertext. Enables async sync across devices \
                          without requiring simultaneous online presence.",
            category: "Sync",
            stability: LabStability::Alpha,
            risk: LabRisk::Medium,
            risk_note: "Encrypted bundle ciphertext is visible to the relay operator. \
                        Content is AES-256-GCM protected; relay cannot read it.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.2",
        },
        // ── Interface ─────────────────────────────────────────────────────────
        Lab {
            id: "bloom_startup",
            name: "Bloom Startup Animation",
            description: "Procedural geometric animation on first load. Pure CSS.",
            category: "Interface",
            stability: LabStability::Stable,
            risk: LabRisk::Low,
            risk_note: "None.",
            enabled: true,
            restart_required: false,
            added_in: "v0.9.0",
        },
        Lab {
            id: "screen_gravity",
            name: "Screen Gravity Tab Ceiling",
            description: "Adjusts maximum tab count by display width.",
            category: "Interface",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Ceiling adjusts within 60 seconds of a window resize.",
            enabled: true,
            restart_required: false,
            added_in: "v0.9.0",
        },
        // ── Security ──────────────────────────────────────────────────────────
        Lab {
            id: "webauthn_bridge",
            name: "WebAuthn / Passkey Bridge",
            description: "Bridges platform authenticators (Face ID, Touch ID, Windows Hello, \
                          YubiKey) to Diatom's credential manager. Passkeys are stored locally \
                          and can be synced via the Nostr relay in encrypted form. \
                          Supports the full WebAuthn Level 3 spec including conditional UI \
                          (passkey autofill) and cross-device flows via CTAP2.",
            category: "Security",
            stability: LabStability::Stable,  // [FIX-PASSKEY-01] Promoted from Alpha
            risk: LabRisk::Low,               // [FIX-PASSKEY-01] Risk reduced: well-tested in v0.9.x
            risk_note: "Passkey sync across devices requires the Nostr Relay Sync lab. \
                        Hardware keys (YubiKey, SoloKey) require platform-specific drivers \
                        already present on most systems.",
            enabled: true,                    // [FIX-PASSKEY-01] Enabled by default for new installs
            restart_required: false,
            added_in: "v0.9.2",
        },
        // ── Discovery ─────────────────────────────────────────────────────────
        Lab {
            id: "privacy_presets",
            name: "Privacy Preset Subscriptions",
            description: "Download and apply community-maintained filter lists (EasyList, \
                          uBlock Origin lists, AdGuard DNS filters) with one click. \
                          Diatom acts as a downloader only — rule responsibility lies with \
                          the user. Updates are fetched weekly in the background.",
            category: "Privacy",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Fetching filter lists makes one outbound network request per list per week. \
                        Lists are cached locally; no personal data is sent.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.2",
        },
        // ── v0.9.8 Added Labs ──────────────────────────────────────────────────
        Lab {
            id: "mcp_host",
            name: "MCP Host (IDE Bridge)",
            description: "Exposes Diatom Museum as a Model Context Protocol server on localhost:39012. \
                          Allows VS Code, Cursor, and other MCP-capable tools to search your personal \
                          web archive without opening the browser. A single-session token is written \
                          to data_dir/mcp.token and expires when Diatom quits.",
            category: "Developer",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Only accessible from localhost. Token expires on quit. \
                        Do not expose port 39012 through firewall rules.",
            enabled: false,
            restart_required: true,
            added_in: "v0.9.8",
        },
        Lab {
            id: "local_file_bridge",
            name: "Local File Bridge (diatom://local/)",
            description: "Allows specific web pages and Diatom built-in tools to directly read and \
                          write local folders via the diatom://local/<alias>/ protocol. \
                          Each mount point requires explicit user approval. \
                          Breaks the browser sandbox in a controlled, audited way.",
            category: "Developer",
            stability: LabStability::Beta,
            risk: LabRisk::Medium,
            risk_note: "Only user-approved folders are accessible. System directories are blocked. \
                        All access is logged in the Net Monitor. DEFAULT OFF — must be manually enabled.",
            enabled: false,  // Strictly default OFF per spec
            restart_required: false,
            added_in: "v0.9.8",
        },
        Lab {
            id: "ghostpipe",
            name: "GhostPipe (DNS-over-HTTPS Tunnel)",
            description: "Routes Diatom's own outgoing requests (filter list updates, Sentinel checks) \
                          through DNS-over-HTTPS endpoints, camouflaging them as standard DNS traffic. \
                          Supports multi-endpoint packet fragmentation to prevent traffic analysis. \
                          Browser-integrated mode only — does not affect system-wide DNS.",
            category: "Privacy",
            stability: LabStability::Alpha,
            risk: LabRisk::Low,
            risk_note: "Only protects Diatom's own requests, not webpage content. \
                        System-wide tunneling (GhostPipe Pro) is a planned separate application.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.8",
        },
        Lab {
            id: "pricing_radar",
            name: "Pricing Radar (Anti-Algo Dynamic Pricing)",
            description: "Detects dynamic pricing discrimination on e-commerce sites. \
                          Extracts the price you see, anonymously queries P2P network nodes for \
                          comparison prices, and alerts you if your price is significantly higher \
                          than the network average. Suggests switching fingerprint profiles.",
            category: "Privacy",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Price queries are anonymized (product hash only, no user identity). \
                        Requires at least some P2P peers to be online for comparison data.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.8",
        },
        Lab {
            id: "museum_marketplace",
            name: "Museum Marketplace (P2P Knowledge Market)",
            description: "Publish curated Museum collections to the Nostr network and exchange them \
                          P2P with other Diatom users. Free exchange uses trust points; paid listings \
                          use Lightning Network micropayments. Files transfer directly device-to-device \
                          — Diatom never sees your content.",
            category: "Social",
            stability: LabStability::Alpha,
            risk: LabRisk::Low,
            risk_note: "Requires active Nostr relay connections. Lightning payments are optional. \
                        P2P transfer uses WebRTC — ensure your firewall allows direct connections.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.8",
        },
        Lab {
            id: "tos_auditor",
            name: "ToS Red-Flag Auditor",
            description: "Automatically extracts and analyzes privacy policies and Terms of Service \
                          when you visit registration pages. Flags dangerous clauses: AI training consent, \
                          data sharing, account deletion restrictions, perpetual IP licenses, and more. \
                          Powered entirely by local rules — no cloud processing.",
            category: "Privacy",
            stability: LabStability::Stable,
            risk: LabRisk::Low,
            risk_note: "Analysis is heuristic-based. Always read the full policy for legal matters.",
            enabled: true,  // Stable and low-risk, on by default
            restart_required: false,
            added_in: "v0.9.8",
        },
        Lab {
            id: "shadow_index",
            name: "Shadow Index (Human-Curated Search)",
            description: "Search your Museum archives with TF-IDF full-text ranking. \
                          Optionally connects to the P2P network to find matching archives from other \
                          Diatom users (keyword hashes only — your actual queries never leave your device). \
                          Includes Bias Contrast View: automatically surfaces opposing perspectives on \
                          news articles from your Museum collection.",
            category: "Discovery",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Local mode is fully offline. P2P mode broadcasts anonymized keyword hashes \
                        using per-session random salts to prevent correlation attacks.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.8",
        },
        Lab {
            id: "emotion_filter",
            name: "Digital Zen Garden (Emotion Filter)",
            description: "Scans page text for high-emotion vocabulary (outrage bait, panic language, \
                          sensationalism) and applies configurable visual dampening: opacity reduction, \
                          blur, and saturation decrease. Helps maintain a calmer browsing experience \
                          on news-heavy sites. Integrated into Zen Mode settings.",
            category: "Wellbeing",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Detection is vocabulary-based heuristic, not semantic. May affect some \
                        legitimate urgent content. Adjust sensitivity in Zen settings.",
            enabled: false,
            restart_required: false,
            added_in: "v0.9.8",
        },
        // ── v0.12.0 New Labs ───────────────────────────────────────────────────
        Lab {
            id: "panic_button",
            name: "Panic Button",
            description: "Cmd/Ctrl+Shift+. instantly hides all browser windows and replaces the \
                          active tab with a configurable decoy page. Restoration via second keypress \
                          or tray icon. Optional total workspace wipe mode.",
            category: "Privacy",
            stability: LabStability::Stable,
            risk: LabRisk::Low,
            risk_note: "Registers a global hotkey. On some platforms, global hotkeys conflict with \
                        OS shortcuts. Configurable key binding available in Labs settings.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "video_pip",
            name: "PiP Video Engine",
            description: "Promotes the video controller to a first-class Picture-in-Picture engine. \
                          Adds requestPictureInPicture() API binding, a floating overlay window for \
                          sites that block native PiP, and a media session toolbar.",
            category: "UX",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Fallback PiP opens a second Tauri window. Ensure privacy initialization \
                        scripts are inherited by the secondary window.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "per_tab_proxy",
            name: "Per-Tab Proxy",
            description: "Assign an independent SOCKS5 or HTTP proxy to each tab, enabling true IP \
                          isolation between browsing contexts without separate workspaces. Integrates \
                          with workspace default proxy settings.",
            category: "Privacy",
            stability: LabStability::Alpha,
            risk: LabRisk::High,
            risk_note: "HIGH risk. Incorrect proxy config silences the tab. Requires a loopback \
                        CONNECT proxy daemon on 127.0.0.1:3182x. Test on all three platforms before \
                        production use.",
            enabled: false,
            restart_required: true,
            added_in: "v0.12.0",
        },
        Lab {
            id: "breach_monitor",
            name: "Dark Web Leak Monitor",
            description: "Uses the Have I Been Pwned k-anonymity API to check vault email addresses \
                          and password hashes for known breaches. Only the first 5 hex characters of \
                          SHA-1(password) are transmitted — the full hash never leaves the device.",
            category: "Security",
            stability: LabStability::Beta,
            risk: LabRisk::Medium,
            risk_note: "Email lookup sends the full email address to the HIBP API. Password check \
                        uses k-anonymity (safe: only 5-char prefix transmitted). Toggle email and \
                        password checks independently.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "wifi_trust_scanner",
            name: "Wi-Fi Trust Scanner",
            description: "Detects open (unencrypted) Wi-Fi networks and automatically activates \
                          GhostPipe DoH tunneling for all Diatom-internal requests. Also upgrades \
                          all HTTP navigations to HTTPS on untrusted networks.",
            category: "Privacy",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "SSID spoofing risk: a malicious AP can clone a trusted SSID name. Mitigated \
                        by BSSID check but not eliminated. Open Wi-Fi detection only — does not \
                        protect against compromised WPA2 networks.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "peek_preview",
            name: "Peek Link Preview",
            description: "Hovering over a hyperlink for 600ms shows a compact preview card (title, \
                          domain, OG description, OG image if available). Previously-visited URLs \
                          resolve from the Museum cache with zero network requests.",
            category: "UX",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Sends a HEAD request to hovered URLs. Adds network activity on hover. \
                        Disabled automatically when Zen mode is active.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "page_boosts",
            name: "Page Boosts",
            description: "Inject custom CSS rules per domain, transforming any website's visual \
                          design. Ships with three built-in Boosts: Clean Reader, Focus Dark, and \
                          Print Friendly. Extends DOM Crusher's blocking capability into positive \
                          CSS injection territory.",
            category: "UX",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "User CSS is injected before page paint. Malformed CSS can break page layout. \
                        CSS input is sandboxed. Built-in Boosts are read-only; user Boosts are editable.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "ai_download_rename",
            name: "AI Download Renamer",
            description: "When a file is downloaded, the local SLM (diatom-fast / Qwen 2.5 3B) \
                          analyzes the page title, URL, and first 2 KB of file content to suggest a \
                          semantically meaningful filename. Shown as a non-blocking toast. \
                          No file content leaves the device.",
            category: "AI",
            stability: LabStability::Alpha,
            risk: LabRisk::Low,
            risk_note: "Requires slm_server lab to be active. Graceful degradation: if SLM \
                        unavailable, shows original filename with an on-demand AI rename button.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "home_base",
            name: "Home Base New Tab",
            description: "Replaces the blank new-tab page with a privacy-respecting Frequency Map \
                          showing pinned shortcuts, top-visited domains, recent Museum saves, RSS \
                          unread count, and SLM status. All data is computed locally — zero external \
                          requests on new-tab load.",
            category: "UX",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Low risk. Zero external requests on new-tab load. Disable to restore the \
                        blank new-tab page.",
            enabled: true,  // Default ON per blueprint
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "privacy_search",
            name: "Privacy Search Engines",
            description: "Expands search engine options to include Brave Search (zero-tracking, \
                          independent index), SearXNG (self-hosted metasearch), and Kagi (paid, no \
                          ads, no tracking). All queries routed through GhostPipe if enabled.",
            category: "Privacy",
            stability: LabStability::Stable,
            risk: LabRisk::Low,
            risk_note: "SearXNG endpoint is user-configurable — validate HTTPS scheme to prevent \
                        SSRF. Kagi requires an API key stored in the Vault.",
            enabled: true,  // Default ON per blueprint
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "tab_group_stacking",
            name: "Tab Group Stacking",
            description: "Visually collapses tab groups into a single stacked pill when not in \
                          focus, reducing tab bar clutter without closing tabs. Auto-collapses under \
                          memory pressure. Additive to existing tab groups.",
            category: "UX",
            stability: LabStability::Beta,
            risk: LabRisk::Low,
            risk_note: "Additive to tab-groups. Does not break existing groups. Auto-collapses under \
                        memory pressure — tabs remain alive in background.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
        Lab {
            id: "bandwidth_limiter",
            name: "Bandwidth Limiter",
            description: "Extends NetMonitor with per-domain and global bandwidth rate limiting \
                          using a token-bucket algorithm. Useful for metered connections, background \
                          tab throttling, and preventing a single media-heavy tab from saturating \
                          the connection.",
            category: "Performance",
            stability: LabStability::Alpha,
            risk: LabRisk::Medium,
            risk_note: "MEDIUM risk. Token bucket math must be correct to avoid starvation. Large \
                        file downloads need a separate 'unlimited' bypass toggle. Diatom-internal \
                        requests (filter list updates, Sentinel) are exempted from limiting.",
            enabled: false,
            restart_required: false,
            added_in: "v0.12.0",
        },
    ]
}

// ── Labs store ────────────────────────────────────────────────────────────────

pub fn load_labs(db: &crate::db::Db) -> Vec<Lab> {
    let mut labs = all_labs();
    for lab in &mut labs {
        let key = format!("lab_{}", lab.id);
        if let Some(val) = db.get_setting(&key) {
            lab.enabled = val == "true";
        }
    }
    labs
}

pub fn set_lab(db: &crate::db::Db, id: &str, enabled: bool) -> anyhow::Result<bool> {
    if !all_labs().iter().any(|l| l.id == id) {
        anyhow::bail!("unknown lab id: {}", id);
    }
    let key = format!("lab_{}", id);
    db.set_setting(&key, if enabled { "true" } else { "false" })?;
    let restart = all_labs().iter()
        .find(|l| l.id == id)
        .map(|l| l.restart_required)
        .unwrap_or(false);
    Ok(restart)
}

pub fn is_lab_enabled(db: &crate::db::Db, id: &str) -> bool {
    let key = format!("lab_{}", id);
    db.get_setting(&key)
        .map(|v| v == "true")
        .unwrap_or_else(|| {
            all_labs().iter()
                .find(|l| l.id == id)
                .map(|l| l.enabled)
                .unwrap_or(false)
        })
}
