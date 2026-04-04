// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/main.rs  — v0.11.0
//
// [FIX S-01] Windows WebView2 telemetry disabled via registry policy key at
//   startup (HKCU first, HKLM fallback). Suppresses Chromium crash/perf
//   reports sent to Microsoft without requiring a full app restart.
//
// [FIX S-02] Version bumped to 0.9.8 across Cargo.toml, tauri.conf.json,
//   README badge. Single source of truth: CARGO_PKG_VERSION.
//
// [FIX S-03] CSP enabled in tauri.conf.json. Chrome-layer JS now protected
//   against XSS → IPC privilege escalation attacks.
//
// [FIX I-01] DIATOM_UA platform-aware fallback in blocker.rs. Windows/Linux
//   requests no longer send macOS Safari UA, eliminating OS fingerprint leak.
//
// [FIX I-06] Labs crdt_museum_sync description corrected to accurately
//   describe the framework-only state (no actual P2P transfer yet).
//
//   All JS injection (privacy, diatom-api, a11y, __DIATOM_INIT__) is now done
//   via WebviewWindowBuilder::initialization_script(). This guarantees that
//   every script runs in document_start position — before any page JavaScript
//   executes — on *every* navigation, not just at app startup. The previous
//   win.eval() approach only ran once at launch; navigating to a new URL
//   silently lost all privacy protections.
//
// [AUDIT-FIX §3.2] include_str! debug/release split:
//   In debug builds, diatom-api.js is read from disk at runtime so iterating
//   on the file does not require a full Rust recompile. Release builds embed
//   it at compile time (zero runtime filesystem access).
//
// [AUDIT-FIX §2.2] CancellationToken cooperative shutdown:
//   Sentinel (3600 s sleep), tab-budget (60 s), and threat-refresh (7-day
//   sleep) loops now select! on AppState::shutdown_token. The token is
//   cancelled in the WindowEvent::Destroyed handler, allowing the tokio
//   runtime to exit cleanly without stalling on long-running sleeps.
// ─────────────────────────────────────────────────────────────────────────────

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod a11y;
mod blocker;
mod commands;
mod compat;
mod compliance;
mod db;
mod decoy;
mod dom_crusher;
mod echo;
mod freeze;
mod labs;
mod privacy;
mod rss;
mod sentinel;
mod slm;
mod state;
mod storage_guard;
mod tab_budget;
mod tabs;
mod threat;
mod totp;
mod trust;
mod utils;
mod nostr;
mod passkey;
mod vault;
mod war_report;
mod zen;
// ── v0.9.8 New Modules ──────────────────────────────────────────────────────────
mod net_monitor;
mod museum_version;
mod mcp_host;
mod marketplace;
mod shadow_index;
mod tos_auditor;
mod pricing_radar;
mod ghostpipe;
mod local_file_bridge;
// ── v0.10.0 New Modules ────────────────────────────────────────────────────────
mod noise_transport;    // Noise Protocol P2P transport (replaces WebRTC stub)
mod dp_echo;            // ε-Differential Privacy for Echo output
mod pir;                // Private Information Retrieval (cover-traffic blocklist fetch)
mod ohttp;              // Oblivious HTTP — complete RFC 9458 implementation
mod wasm_sandbox;       // Wasm Component Model + WASI plugin sandbox
// ── v0.12.0 New Modules ────────────────────────────────────────────────────────
mod panic_button;       // F-01: Panic Button — instant privacy lockdown
mod tab_proxy;          // F-03: Per-Tab Independent Proxy
mod breach_monitor;     // F-04: Dark Web Leak Monitor (HIBP k-anonymity)
mod wifi_trust;         // F-05: Wi-Fi Trust Scanner
mod boosts;             // F-07: Page Boosts — per-domain CSS injection
mod download_renamer;   // F-08: AI Download Renamer
mod search_engine;      // F-10: Privacy Search Integration
mod bandwidth_limiter;  // F-12: Network Bandwidth Limiter
mod power_budget;       // Battery-aware background task scheduling
mod etag_cache;         // ETag/If-None-Match conditional GET (saves ~1 MB/week)

use anyhow::Result;
use state::AppState;
use std::sync::Arc;
use tauri::{Builder, Emitter, Manager, WindowEvent};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("diatom=info".parse().unwrap())
                .add_directive("tauri=warn".parse().unwrap()),
        )
        .compact()
        .init();

    Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::cmd_preprocess_url,
            commands::cmd_fetch,
            commands::cmd_tab_create,
            commands::cmd_tab_close,
            commands::cmd_tab_activate,
            commands::cmd_tab_update,
            commands::cmd_tab_sleep,
            commands::cmd_tab_wake,
            commands::cmd_tabs_state,
            commands::cmd_tab_budget,
            commands::cmd_tab_budget_config_set,
            commands::cmd_workspaces_list,
            commands::cmd_workspace_create,
            commands::cmd_workspace_switch,
            commands::cmd_workspace_fire,
            commands::cmd_history_search,
            commands::cmd_history_clear,
            commands::cmd_bookmark_add,
            commands::cmd_bookmark_list,
            commands::cmd_bookmark_remove,
            commands::cmd_setting_get,
            commands::cmd_setting_set,
            commands::cmd_is_blocked,
            commands::cmd_clean_url,
            commands::cmd_noise_seed,
            commands::cmd_totp_list,
            commands::cmd_totp_add,
            commands::cmd_totp_generate,
            commands::cmd_totp_match,
            commands::cmd_totp_remove,
            commands::cmd_trust_get,
            commands::cmd_trust_set,
            commands::cmd_trust_list,
            commands::cmd_rss_feeds,
            commands::cmd_rss_add,
            commands::cmd_rss_fetch,
            commands::cmd_rss_items,
            commands::cmd_rss_mark_read,
            commands::cmd_snapshot_save,
            commands::cmd_system_info,
            commands::cmd_devtools_open,
            commands::cmd_record_reading,
            commands::cmd_echo_compute,
            commands::cmd_war_report,
            commands::cmd_freeze_page,
            commands::cmd_museum_list,
            commands::cmd_museum_search,
            commands::cmd_museum_delete,
            commands::cmd_museum_thaw,
            commands::cmd_dom_crush,
            commands::cmd_dom_blocks_for,
            commands::cmd_dom_block_remove,
            commands::cmd_zen_activate,
            commands::cmd_zen_deactivate,
            commands::cmd_zen_state,
            commands::cmd_zen_set_aphorism,
            commands::cmd_threat_check,
            commands::cmd_threat_list_refresh,
            commands::cmd_knowledge_packs_list,
            commands::cmd_knowledge_pack_add,
            commands::cmd_compat_handoff,
            commands::cmd_compat_page_report,
            commands::cmd_compat_is_legacy,
            commands::cmd_compat_is_payment,
            commands::cmd_compat_add_legacy,
            commands::cmd_compat_remove_legacy,
            commands::cmd_compat_list_legacy,
            commands::cmd_storage_report,
            commands::cmd_storage_evict_lru,
            commands::cmd_storage_budget_set,
            commands::cmd_feature_consent_check,
            commands::cmd_feature_consent_record,
            commands::cmd_feature_consent_revoke,
            commands::cmd_feature_legal_info,
            commands::cmd_decoy_fire,
            commands::cmd_decoy_log,
            commands::cmd_labs_list,
            commands::cmd_lab_set,
            commands::cmd_lab_is_enabled,
            commands::cmd_slm_status,
            commands::cmd_slm_chat,
            commands::cmd_slm_models,
            commands::cmd_slm_set_model,
            commands::cmd_slm_server_toggle,
            commands::cmd_slm_reset,
            commands::cmd_sentinel_status,
            commands::cmd_sentinel_refresh,
            commands::cmd_init_bundle,
            commands::cmd_onboarding_complete,
            commands::cmd_onboarding_is_done,
            commands::cmd_onboarding_all,
            commands::cmd_filter_sub_add,
            commands::cmd_filter_subs_list,
            commands::cmd_filter_sub_sync,
            commands::cmd_nostr_relay_add,
            commands::cmd_nostr_relays,
            commands::cmd_nostr_sync_bookmarks,
            commands::cmd_local_auth,
            commands::cmd_biometric_available,
            commands::cmd_biometric_status,
            // ── v0.9.8 New Commands ─────────────────────────────────────────────
            commands::cmd_net_monitor_log,
            commands::cmd_net_monitor_summary,
            commands::cmd_net_monitor_clear,
            commands::cmd_museum_diff,
            commands::cmd_museum_content_hash,
            commands::cmd_temporal_audit_banner,
            commands::cmd_marketplace_create_listing,
            commands::cmd_marketplace_publish,
            commands::cmd_marketplace_initiate_download,
            commands::cmd_shadow_search,
            commands::cmd_bias_contrast_mermaid,
            commands::cmd_tos_audit,
            commands::cmd_anti_adblock_script,
            commands::cmd_price_extractor_script,
            commands::cmd_canonical_product_id,
            commands::cmd_price_comparison,
            commands::cmd_ghostpipe_status,
            commands::cmd_ghostpipe_configure,
            commands::cmd_ghostpipe_resolve,
            commands::cmd_local_bridge_list,
            commands::cmd_local_bridge_mount,
            commands::cmd_local_bridge_unmount,
            commands::cmd_local_bridge_read,
            commands::cmd_local_bridge_write,
            commands::cmd_local_bridge_ls,
            commands::cmd_zen_emotion_filter_script,
            commands::cmd_dom_reshuffle_script,
            commands::cmd_museum_random_card,
            commands::cmd_mcp_status,
            // ── Vault (password manager) [NEW v0.9.5] ────────────────────────
            commands::cmd_vault_login_add,
            commands::cmd_vault_login_update,
            commands::cmd_vault_login_delete,
            commands::cmd_vault_login_get,
            commands::cmd_vault_logins_list,
            commands::cmd_vault_logins_search,
            commands::cmd_vault_match_domain,
            commands::cmd_vault_card_add,
            commands::cmd_vault_card_delete,
            commands::cmd_vault_card_get,
            commands::cmd_vault_cards_list,
            commands::cmd_vault_note_add,
            commands::cmd_vault_note_delete,
            // ── Tab Groups [NEW v0.9.6] ──────────────────────────────────────
            commands::cmd_tab_group_create,
            commands::cmd_tab_groups_list,
            commands::cmd_tab_group_delete,
            commands::cmd_tab_group_rename,
            commands::cmd_tab_group_move_tab,
            commands::cmd_tab_group_collapse,
            commands::cmd_tab_group_set_project_mode,
            commands::cmd_vault_note_get,
            commands::cmd_vault_notes_list,
            commands::cmd_vault_stats,
            commands::cmd_vault_import_csv,
            commands::cmd_vault_generate_password,
            commands::cmd_vault_generate_passphrase,
            commands::cmd_vault_score_password,
            // ── Enhanced TOTP [NEW v0.9.5] ───────────────────────────────────
            commands::cmd_totp_import_aegis,
            commands::cmd_totp_import_bitwarden,
            commands::cmd_totp_import_uri_list,
            commands::cmd_totp_import_2fas,
            commands::cmd_totp_export_aegis,
            commands::cmd_totp_add_from_uri,
                    // ── v0.11.0 New Commands ────────────────────────────────────────────
            commands::cmd_power_budget_status,
            commands::cmd_noise_fingerprint,
            commands::cmd_ohttp_status,
            commands::cmd_plugin_list,
            commands::cmd_plugin_install,
            commands::cmd_plugin_remove,
            commands::cmd_echo_dp_epsilon_get,
            commands::cmd_echo_dp_epsilon_set,
        ])
        .run(tauri::generate_context!())
        .expect("Diatom failed to start");
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // [FIX S-01] Windows: disable WebView2 telemetry via registry policy key.
    // WebView2 Runtime (Chromium-based) sends performance metrics + crash
    // reports to Microsoft by default, violating Diatom's zero-telemetry
    // promise. Writing this registry DWORD at startup suppresses it without
    // requiring admin rights on most configurations.
    // Long-term fix: switch to Servo when it matures on Windows (~2026-2027).
    #[cfg(target_os = "windows")]
    {
        let result = std::process::Command::new("reg")
            .args([
                "add",
                "HKCU\\SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2",
                "/v", "MetricsReportingEnabled",
                "/t", "REG_DWORD",
                "/d", "0",
                "/f",
            ])
            .output();
        match result {
            Ok(out) if out.status.success() =>
                tracing::info!("webview2: telemetry disabled via registry (HKCU)"),
            Ok(_) => {
                // HKCU failed — try HKLM (requires elevation; silently skip if denied)
                let _ = std::process::Command::new("reg")
                    .args([
                        "add",
                        "HKLM\\SOFTWARE\\Policies\\Microsoft\\Edge\\WebView2",
                        "/v", "MetricsReportingEnabled",
                        "/t", "REG_DWORD",
                        "/d", "0",
                        "/f",
                    ])
                    .output();
                tracing::warn!("webview2: HKCU policy write failed; attempted HKLM (may need elevation)");
            }
            Err(e) => tracing::warn!("webview2: could not run reg.exe: {e}"),
        }
    }

    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    tracing::info!(
        "Diatom v{} — data: {:?}",
        env!("CARGO_PKG_VERSION"),
        data_dir
    );

    // [v0.11.0 PERF / B-03 FIX] Detect power state for adaptive background scheduling.
    // Store in AppState so background loops can read it without recomputing.
    let power = power_budget::PowerBudget::current();
    tracing::info!(
        "power state: {:?} (battery={:?}%) — sentinel={}s tab-budget={}s",
        power.state, power.battery_pct,
        power.sentinel_interval_secs, power.tab_budget_interval_secs
    );

    let state = AppState::new(data_dir, power)?;
    app.manage(state);

    let st = app.state::<AppState>();

    // ── Build __DIATOM_INIT__ snapshot ────────────────────────────────────────
    let init_seed     = *st.noise_seed.lock().unwrap();
    let init_zen      = st.zen.lock().unwrap().is_active();
    let init_platform = st.platform;
    let init_js = format!(
        "window.__DIATOM_INIT__ = {{ noise_seed: {}, crusher_rules: [], zen_active: {}, platform: '{}' }};",
        init_seed, init_zen, init_platform
    );

    // ── Privacy injection script ──────────────────────────────────────────────
    // [AUDIT-FIX §3.1] Moved from win.eval() to initialization_script().
    // win.eval() only ran once at startup — every subsequent navigation
    // discarded the privacy hooks silently. initialization_script() re-injects
    // before page JS runs on every navigation.
    let privacy_js = st.privacy.read().unwrap().injection_script();

    // ── diatom-api.js: debug reads from disk; release embeds at compile time ──
    // [AUDIT-FIX §3.2] Editing diatom-api.js in debug no longer requires a
    // full Rust recompile — just restart the Tauri dev server. Release builds
    // use include_str! as before (zero runtime FS dependency).
    #[cfg(debug_assertions)]
    let api_js: String = {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("diatom-api.js");
        std::fs::read_to_string(&path).unwrap_or_else(|e| {
            tracing::warn!(
                "diatom-api.js runtime read failed ({e}) — using compiled-in fallback"
            );
            include_str!("../resources/diatom-api.js").to_owned()
        })
    };
    #[cfg(not(debug_assertions))]
    let api_js: &str = include_str!("../resources/diatom-api.js");

    let a11y_aria = crate::a11y::generate_aria_injection_script();
    let a11y_nav  = crate::a11y::keyboard_nav_script();

    // ── Build main window with initialization_script ──────────────────────────
    // [AUDIT-FIX §3.1] All five injection scripts are registered here via
    // initialization_script(), which Tauri 2.0 guarantees runs in
    // document_start order on every WebView navigation — equivalent to a
    // Chrome extension content_script with "run_at": "document_start".
    //
    // Execution order is deterministic (Tauri appends in registration order):
    //   1. init_js       — __DIATOM_INIT__ global (read by diatom-api.js init)
    //   2. privacy_js    — canvas/WebRTC/battery spoofing before any page JS
    //   3. api_js        — IPC bridge (window.__TAURI__ wrappers)
    //   4. a11y_aria     — ARIA live-region injection
    //   5. a11y_nav      — keyboard navigation helpers
    //
    // NOTE: The window geometry (size, min-size, decorations, etc.) mirrors
    // the values previously in tauri.conf.json windows[] array. That array
    // entry is intentionally cleared to avoid duplicate-window creation.
    let win = tauri::WebviewWindowBuilder::new(
            app,
            "main",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .initialization_script(&init_js)
        .initialization_script(&privacy_js)
        .initialization_script(&api_js)
        .initialization_script(&a11y_aria)
        .initialization_script(a11y_nav)
        .title("Diatom")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .resizable(true)
        .fullscreen(false)
        .decorations(true)
        .center()
        .visible(false)
        .build()?;

    let _ = win.emit("diatom:ready", ());

    // ── Window-destroy handler: cancel all background tasks ───────────────────
    // [AUDIT-FIX §2.2] Sentinel sleeps 3600 s; threat-refresh sleeps 7 days.
    // Without cancellation the tokio runtime hangs on exit waiting for all
    // spawned futures to complete. token.cancel() wakes every select! branch
    // simultaneously — tasks log a message and return immediately.
    {
        let ah = app.handle().clone();
        win.on_window_event(move |event| {
            if let WindowEvent::Destroyed = event {
                if let Some(st) = ah.try_state::<AppState>() {
                    tracing::info!("window destroyed — cancelling all background tasks");
                    st.shutdown_token.cancel();
                }
            }
        });
    }

    // ── Sentinel background task ───────────────────────────────────────────────
    // [FIX-14] Always spawn regardless of lab setting.
    {
        let ah    = app.handle().clone();
        let token = st.shutdown_token.child_token();
        tauri::async_runtime::spawn(async move {
            sentinel::run_sentinel_loop(ah, 15, token).await;
        });
        tracing::info!("sentinel: version-tracking task spawned");
    }

    // ── Boot-time filter list fetch (60k+ rules, no subscription needed) ─────
    // [FIX-BLOCKER-01] Fetches EasyList, EasyPrivacy, uBlock Filters, Peter Lowe
    // list in the background and installs a merged 30k+ pattern automaton.
    // Requests are served by the static BUILTIN_PATTERNS (400+ entries) until
    // the fetch completes (~5–15 s on a typical connection).
    {
        let live_blocker = Arc::clone(&st.live_blocker);
        tauri::async_runtime::spawn(async move {
            crate::blocker::boot_fetch_builtin_lists(live_blocker).await;
        });
        tracing::info!("blocker: boot-time filter fetch task spawned");
    }

    // ── MCP host (localhost:39012) — Museum API for external IDE tools ────────
    // [NEW v0.9.8] Generates a single-session token, writes to data_dir/mcp.token.
    // Only spawned when the mcp_host lab is enabled.
    if labs::is_lab_enabled(&st.db, "mcp_host") {
        match mcp_host::generate_and_write_token(&st.data_dir) {
            Ok(token) => {
                let db_arc = Arc::new(st.db.clone());
                tauri::async_runtime::spawn(async move {
                    mcp_host::run_mcp_server(token, db_arc).await;
                });
                tracing::info!("MCP host: started on {}:{}", mcp_host::MCP_HOST, mcp_host::MCP_PORT);
            }
            Err(e) => tracing::warn!("MCP host: failed to start: {}", e),
        }
    }

    // ── SLM server background task (when lab enabled) ─────────────────────────
    // [B-06 FIX] Replace Arc<AtomicBool> with CancellationToken child of
    // AppState.shutdown_token so shutdown is immediate (not polled every 100ms).
    if labs::is_lab_enabled(&st.db, "slm_server") {
        let privacy_mode = labs::is_lab_enabled(&st.db, "slm_extreme_privacy");
        let preferred    = st.db.get_setting("slm_active_model");
        let token        = st.shutdown_token.child_token();
        *st.slm_shutdown_token.lock().unwrap() = Some(token.clone());
        tauri::async_runtime::spawn(async move {
            let server = Arc::new(slm::SlmServer::new(privacy_mode, preferred.as_deref()).await);
            tracing::info!("SLM server online — backend: {:?}", server.backend);
            slm::run_server(server, token).await;
        });
    }

    // ── Power state monitor (5 min interval) ──────────────────────────────────
    // [B-03 FIX] Re-reads PowerBudget every 5 minutes and emits
    // diatom:power-state-changed when the tier changes (AC↔Battery↔LowBattery).
    // Background loops read their interval from AppState.power_budget at the
    // top of each iteration — this monitor keeps that field current.
    {
        let ah    = app.handle().clone();
        let token = st.shutdown_token.child_token();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(5 * 60)) => {},
                    _ = token.cancelled() => { return; },
                }
                let new_power = power_budget::PowerBudget::current();
                if let Some(st) = ah.try_state::<AppState>() {
                    let old_state = st.power_budget.lock().unwrap().state;
                    if new_power.state != old_state {
                        tracing::info!(
                            "power state transition: {:?} → {:?}",
                            old_state, new_power.state
                        );
                        let _ = ah.emit(
                            "diatom:power-state-changed",
                            serde_json::json!({
                                "state": new_power.state,
                                "battery_pct": new_power.battery_pct,
                            }),
                        );
                    }
                    *st.power_budget.lock().unwrap() = new_power;
                }
            }
        });
    }

    // ── Window visibility watchdog (B-05 FIX) ─────────────────────────────────
    // [B-05 FIX] The window is created with .visible(false). If main.js fails
    // to load (JS parse error, missing module), the window stays invisible
    // forever with no error surface. This 3-second watchdog forces the window
    // visible and emits diatom:boot-error if no diatom:window-ready IPC arrives.
    {
        let win_handle = win.clone();
        let ah = app.handle().clone();
        let token = st.shutdown_token.child_token();
        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
                    // No window-ready received — force the window visible
                    tracing::warn!("watchdog: no diatom:window-ready in 3s — forcing window visible");
                    let _ = win_handle.show();
                    let _ = ah.emit("diatom:boot-error", serde_json::json!({
                        "reason": "window-ready timeout — JS may have failed to load"
                    }));
                },
                _ = token.cancelled() => {},
            }
        });
    }
    // [B-03 FIX] Previously used a hardcoded sleep(60s) that never consulted
    // the PowerBudget module. Now reads the interval from AppState.power_budget
    // at the top of each iteration so battery-aware scheduling actually works.
    // [AUDIT-FIX §2.2] select! on cancellation — exits in <1 ms on shutdown.
    if labs::is_lab_enabled(&st.db, "dynamic_tab_budget") {
        let ah    = app.handle().clone();
        let token = st.shutdown_token.child_token();
        tauri::async_runtime::spawn(async move {
            loop {
                // Re-read power budget at each iteration — adapts to AC↔battery transitions.
                let interval_secs = ah
                    .try_state::<AppState>()
                    .map(|s| s.power_budget.lock().unwrap().tab_budget_interval_secs)
                    .unwrap_or(60);

                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(interval_secs)) => {},
                    _ = token.cancelled() => {
                        tracing::info!("tab-budget: shutdown signal — exiting loop");
                        return;
                    },
                }
                if let Some(st) = ah.try_state::<AppState>() {
                    let cfg   = st.tab_budget_cfg.lock().unwrap().clone();
                    let omega = st.tabs.lock().unwrap().avg_mem_weight();
                    let count = st.tabs.lock().unwrap().count() as u32;
                    let sw    = *st.screen_width_px.lock().unwrap();
                    let b     = tab_budget::compute_budget(&cfg, sw, omega, count);
                    let _ = ah.emit(
                        "diatom:budget-update",
                        serde_json::to_value(&b).unwrap_or_default(),
                    );
                }
            }
        });
    }

    // ── Weekly threat list refresh ─────────────────────────────────────────────
    // [FIX-07] URLhaus refresh is independent of quad9_enabled.
    // [AUDIT-FIX §2.2] 7-day sleep now exits instantly on shutdown signal.
    {
        let ah    = app.handle().clone();
        let token = st.shutdown_token.child_token();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(7 * 24 * 3_600)) => {},
                    _ = token.cancelled() => {
                        tracing::info!("threat-refresh: shutdown signal — exiting loop");
                        return;
                    },
                }
                if let Some(st) = ah.try_state::<AppState>() {
                    if let Ok(list) = threat::fetch_live_list().await {
                        let _ = st.db.set_setting(
                            "threat_list_json",
                            &serde_json::to_string(&list).unwrap_or_default(),
                        );
                        *st.threat_list.write().unwrap() = list;
                        tracing::info!("threat list refreshed (URLhaus)");
                    }
                }
            }
        });
    }

    Ok(())
}
