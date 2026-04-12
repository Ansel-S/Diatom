
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod engine;
pub mod privacy;
pub mod storage;
pub mod ai;
pub mod browser;
pub mod auth;
pub mod sync;
pub mod features;

pub mod state;
pub mod commands;
pub mod utils;

use state::AppState;


fn main() {
    let initial_power = features::sentinel::power_budget_current();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new(
            tauri::api::path::app_data_dir(&tauri::Config::default())
                .expect("app_data_dir"),
            initial_power,
        ).expect("AppState::new"))
        .invoke_handler(tauri::generate_handler![
            commands::cmd_net_monitor_log,
            commands::cmd_net_monitor_clear,
            commands::cmd_bandwidth_set_global,
            commands::cmd_bandwidth_rule_upsert,
            commands::cmd_bandwidth_rule_remove,
            commands::cmd_bandwidth_status,
            commands::cmd_plugin_list,
            commands::cmd_plugin_install,
            commands::cmd_plugin_remove,
            commands::cmd_privacy_config_get,
            commands::cmd_privacy_config_set,
            commands::cmd_fp_norm_script,
            commands::cmd_ohttp_status,
            commands::cmd_onion_suggest,
            commands::cmd_threat_check,
            commands::cmd_wifi_scan,
            commands::cmd_wifi_trust_network,
            commands::cmd_wifi_distrust_network,
            commands::cmd_wifi_trusted_networks,
            commands::cmd_freeze_page,
            commands::cmd_museum_search,
            commands::cmd_museum_list,
            commands::cmd_museum_get,
            commands::cmd_museum_delete,
            commands::cmd_museum_touch_access,
            commands::cmd_museum_deep_dig,
            commands::cmd_storage_report,
            commands::cmd_storage_evict_lru,
            commands::cmd_storage_budget_set,
            commands::cmd_storage_degrade_cold,
            commands::cmd_vault_list,
            commands::cmd_vault_add,
            commands::cmd_vault_update,
            commands::cmd_vault_delete,
            commands::cmd_vault_autofill,
            commands::cmd_slm_status,
            commands::cmd_slm_complete,
            commands::cmd_slm_reset,
            commands::cmd_ai_rename_suggest,
            commands::cmd_shadow_search,
            commands::cmd_mcp_status,
            commands::cmd_tabs_list,
            commands::cmd_tab_open,
            commands::cmd_tab_close,
            commands::cmd_tab_activate,
            commands::cmd_tab_limit_get,
            commands::cmd_tab_limit_set,
            commands::cmd_tab_proxy_set,
            commands::cmd_tab_proxy_get,
            commands::cmd_tab_proxy_remove,
            commands::cmd_dom_crush,
            commands::cmd_dom_blocks_for,
            commands::cmd_boosts_for_domain,
            commands::cmd_boosts_list,
            commands::cmd_boost_upsert,
            commands::cmd_boost_delete,
            commands::cmd_totp_list,
            commands::cmd_totp_add,
            commands::cmd_totp_code,
            commands::cmd_totp_delete,
            commands::cmd_totp_import,
            commands::cmd_biometric_verify,
            commands::cmd_trust_get,
            commands::cmd_trust_set,
            commands::cmd_noise_fingerprint,
            commands::cmd_nostr_publish,
            commands::cmd_nostr_fetch,
            commands::cmd_zen_status,
            commands::cmd_zen_activate,
            commands::cmd_zen_deactivate,
            commands::cmd_rss_feeds_list,
            commands::cmd_rss_feed_add,
            commands::cmd_rss_feed_remove,
            commands::cmd_rss_items,
            commands::cmd_rss_mark_read,
            commands::cmd_panic_toggle,
            commands::cmd_panic_config_get,
            commands::cmd_panic_config_set,
            commands::cmd_breach_check_password,
            commands::cmd_breach_check_email,
            commands::cmd_search_engines_list,
            commands::cmd_search_engine_get_default,
            commands::cmd_search_engine_set_default,
            commands::cmd_searxng_set_endpoint,
            commands::cmd_tos_audit,
            commands::cmd_war_report,
            commands::cmd_labs_list,
            commands::cmd_lab_set,
            commands::cmd_power_budget_status,
            commands::cmd_signal_window_ready,
            commands::cmd_home_base_data,
            commands::cmd_peek_fetch,
            commands::cmd_compliance_registry,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state: tauri::State<AppState> = app.state();

            let blocker_arc = state.live_blocker.clone();
            tauri::async_runtime::spawn(async move {
                engine::blocker::boot_fetch_builtin_lists(blocker_arc).await;
            });

            if features::labs::is_lab_enabled(&state.db, "slm_server") {
                let slm_cache = state.slm_cache.clone();
                let db2 = state.db.clone();
                tauri::async_runtime::spawn(async move {
                    ai::slm::ensure_slm_running(&slm_cache, &db2).await;
                });
            }

            let token = state.window_ready_token.clone();
            let handle2 = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::select! {
                    _ = token.cancelled() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
                        if let Some(w) = handle2.get_webview_window("main") {
                            let _ = w.show();
                        }
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.try_state::<AppState>() {
                    state.shutdown_token.cancel();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("diatom failed to start");
}

