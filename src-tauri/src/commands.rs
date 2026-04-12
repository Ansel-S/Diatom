
use crate::state::AppState;

/// Type alias — eliminates the 33-char `tauri::State<'_, AppState>` repetition
/// across all command handlers without changing any public API.
type St<'r> = tauri::State<'r, AppState>;


/// Convenience: convert any `Display` error to `String` for Tauri command returns.
#[inline(always)]
fn es<E: std::fmt::Display>(e: E) -> String { e.to_string() }

#[tauri::command]
pub async fn cmd_net_monitor_log(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let entries = state.net_monitor.recent(500);
    Ok(serde_json::json!({ "entries": entries }))
}

#[tauri::command]
pub async fn cmd_net_monitor_clear(
    state: St<'_>,
) -> Result<(), String> {
    state.net_monitor.clear();
    Ok(())
}


#[tauri::command]
pub async fn cmd_bandwidth_set_global(
    kbps: u32,
    state: St<'_>,
) -> Result<(), String> {
    state.bandwidth_limiter.set_global_limit(kbps);
    state.bandwidth_limiter.save_to_db(&state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_bandwidth_rule_upsert(
    rule: crate::engine::bandwidth::BandwidthRule,
    state: St<'_>,
) -> Result<(), String> {
    state.bandwidth_limiter.upsert_rule(rule);
    state.bandwidth_limiter.save_to_db(&state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_bandwidth_rule_remove(
    domain_pattern: String,
    state: St<'_>,
) -> Result<(), String> {
    state.bandwidth_limiter.remove_rule(&domain_pattern);
    state.bandwidth_limiter.save_to_db(&state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_bandwidth_status(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let global = state.db.get_setting("bandwidth_global_kbps")
        .and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let rules  = state.db.get_setting("bandwidth_rules")
        .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
        .unwrap_or(serde_json::json!([]));
    Ok(serde_json::json!({ "global_kbps": global, "rules": rules }))
}


#[tauri::command]
pub async fn cmd_plugin_list(
    state: St<'_>,
) -> Result<Vec<crate::engine::plugins::PluginManifest>, String> {
    Ok(state.plugin_registry.list_manifests())
}

#[tauri::command]
pub async fn cmd_plugin_install(
    path: String,
    state: St<'_>,
) -> Result<String, String> {
    let plugin = crate::engine::plugins::WasmPlugin::load(path.into(), None)
        .map_err(es)?;
    let id = state.plugin_registry.install(plugin);
    Ok(id.to_string())
}

#[tauri::command]
pub async fn cmd_plugin_remove(
    id: String,
    state: St<'_>,
) -> Result<bool, String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(es)?;
    Ok(state.plugin_registry.remove(uuid))
}


#[tauri::command]
pub async fn cmd_privacy_config_get(
    state: St<'_>,
) -> Result<crate::privacy::config::PrivacyConfig, String> {
    Ok(state.privacy.read().unwrap().clone())
}

#[tauri::command]
pub async fn cmd_privacy_config_set(
    config: crate::privacy::config::PrivacyConfig,
    state: St<'_>,
) -> Result<(), String> {
    *state.privacy.write().unwrap() = config;
    Ok(())
}


#[tauri::command]
pub async fn cmd_ohttp_status(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let relay = state.db.get_setting("ohttp_relay")
        .unwrap_or_else(|| crate::privacy::ohttp::OHTTP_RELAYS[0].to_owned());
    let has_key = state.db.get_setting("ohttp_key_config").is_some();
    Ok(serde_json::json!({
        "relay": relay,
        "has_key_config": has_key,
        "relays": crate::privacy::ohttp::OHTTP_RELAYS,
    }))
}


#[tauri::command]
pub async fn cmd_onion_suggest(
    host: String,
) -> Result<Option<crate::privacy::onion::OnionSuggestion>, String> {
    Ok(crate::privacy::onion::lookup(&host))
}


#[tauri::command]
pub async fn cmd_threat_check(
    url: String,
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let domain = crate::utils::domain_of(&url);
    let local  = crate::privacy::threat::check_local(&state.threat_list.read().unwrap(), &domain);
    Ok(serde_json::json!({ "domain": domain, "flagged": local }))
}


#[tauri::command]
pub async fn cmd_wifi_scan() -> Result<Option<crate::privacy::wifi::WifiInfo>, String> {
    Ok(crate::privacy::wifi::detect_current_network())
}

#[tauri::command]
pub async fn cmd_wifi_trust_network(
    ssid: String, bssid: String,
    state: St<'_>,
) -> Result<(), String> {
    crate::privacy::wifi::trust_network(&state.db, &ssid, &bssid)
        .map_err(es)
}

#[tauri::command]
pub async fn cmd_wifi_distrust_network(
    ssid: String, bssid: String,
    state: St<'_>,
) -> Result<(), String> {
    crate::privacy::wifi::distrust_network(&state.db, &ssid, &bssid)
        .map_err(es)
}

#[tauri::command]
pub async fn cmd_wifi_trusted_networks(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let info    = crate::privacy::wifi::detect_current_network();
    let trusted = info.as_ref().map(|w| {
        crate::privacy::wifi::is_trusted(&state.db, &w.ssid, &w.bssid)
    }).unwrap_or(false);
    Ok(serde_json::json!({ "current": info, "current_trusted": trusted }))
}


#[tauri::command]
pub async fn cmd_freeze_page(
    url: String,
    title: String,
    raw_html: String,
    state: St<'_>,
) -> Result<String, String> {
    let master_key = *state.master_key.lock().unwrap();
    let ws = state.workspace_id();
    let bundle = crate::storage::freeze::freeze_page(
        &raw_html, &url, &title, &ws, &master_key, &state.bundles_dir(),
    ).map_err(es)?;
    let id = bundle.bundle_row.id.clone();
    state.db.insert_bundle(&bundle.bundle_row).map_err(es)?;
    Ok(id)
}

#[tauri::command]
pub async fn cmd_museum_search(
    query: String,
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let ws = state.workspace_id();
    let results = tokio::task::spawn_blocking({
        let db = state.db.clone();
        move || db.search_bundles_fts(&query, &ws)
    }).await.map_err(es)?;
    Ok(serde_json::json!({ "results": results.unwrap_or_default() }))
}

#[tauri::command]
pub async fn cmd_museum_list(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let ws  = state.workspace_id();
    let rows = state.db.list_bundles(&ws, 100).unwrap_or_default();
    Ok(serde_json::json!({ "bundles": rows }))
}

#[tauri::command]
pub async fn cmd_museum_get(
    id: String,
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let bundle = state.db.get_bundle_by_id(&id)
        .map_err(es)?
        .ok_or_else(|| format!("bundle {} not found", id))?;
    Ok(serde_json::json!({ "bundle": bundle }))
}

#[tauri::command]
pub async fn cmd_museum_delete(
    id: String,
    state: St<'_>,
) -> Result<(), String> {
    if let Ok(Some(b)) = state.db.get_bundle_by_id(&id) {
        let _ = std::fs::remove_file(state.bundles_dir().join(&b.bundle_path));
    }
    state.db.delete_bundle(&id).map_err(es)
}

/// Record that the user opened a Museum snapshot.
/// Promotes the entry back to the hot (full-text) tier if it was cold.
#[tauri::command]
pub async fn cmd_museum_touch_access(
    id: String,
    state: St<'_>,
) -> Result<(), String> {
    state.db.touch_bundle_access(&id).map_err(es)
}

/// Deep Dig: search cold-tier bundles by keyword fingerprint.
///
/// Called only when the user explicitly clicks the "Deep Dig" button after a
/// hot-tier FTS search returns no results.  Scans tfidf_tags (top-N TF-IDF
/// keywords) with a LIKE match — no full-text index required.
/// Returns up to 20 results, serialised as { bundles: [...] }.
#[tauri::command]
pub async fn cmd_museum_deep_dig(
    query: String,
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let ws = state.workspace_id().to_string();
    let db = state.db.clone();
    let results = tokio::task::spawn_blocking(move || {
        db.search_cold_keyword(&query, &ws)
    })
    .await
    .map_err(es)?
    .map_err(es)?;
    Ok(serde_json::json!({ "bundles": results }))
}


#[tauri::command]
pub async fn cmd_storage_report(
    state: St<'_>,
) -> Result<crate::storage::guard::StorageReport, String> {
    let budget = load_storage_budget(&state.db);
    Ok(crate::storage::guard::report(&state.db, &budget))
}

#[tauri::command]
pub async fn cmd_storage_evict_lru(
    target_pct: u8,
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let budget      = load_storage_budget(&state.db);
    let bundles_dir = state.bundles_dir();
    let (deleted, freed) = crate::storage::guard::evict_lru(
        &state.db, &budget, target_pct, &bundles_dir,
    ).map_err(es)?;
    Ok(serde_json::json!({ "deleted": deleted, "freed_bytes": freed }))
}

#[tauri::command]
pub async fn cmd_storage_budget_set(
    museum_mb: Option<u64>, index_mb: Option<u64>,
    state: St<'_>,
) -> Result<(), String> {
    if let Some(mb) = museum_mb {
        state.db.set_setting("museum_budget_mb", &mb.to_string()).map_err(es)?;
    }
    if let Some(mb) = index_mb {
        state.db.set_setting("index_budget_mb", &mb.max(10).to_string()).map_err(es)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_storage_degrade_cold(
    state: St<'_>,
) -> Result<u32, String> {
    let budget = load_storage_budget(&state.db);
    crate::storage::guard::degrade_cold_indexes(&state.db, &budget)
        .map_err(es)
}

fn load_storage_budget(db: &crate::storage::db::Db) -> crate::storage::guard::StorageBudget {
    let d = crate::storage::guard::StorageBudget::default();
    let parse_mb = |key: &str, fallback: u32| -> u32 {
        db.get_setting(key).and_then(|s| s.parse().ok()).unwrap_or(fallback)
    };
    crate::storage::guard::StorageBudget {
        museum_budget_mb: parse_mb("museum_budget_mb", d.museum_budget_mb),
        kpack_budget_mb:  parse_mb("kpack_budget_mb",  d.kpack_budget_mb),
        index_budget_mb:  parse_mb("index_budget_mb",  d.index_budget_mb),
        warn_at_pct:      parse_mb("storage_warn_pct", d.warn_at_pct),
        hard_cap_enabled: db.get_setting("storage_hard_cap")
            .map(|v| v == "true")
            .unwrap_or(d.hard_cap_enabled),
    }
}


#[tauri::command]
pub async fn cmd_vault_list(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let entries = state.vault.lock().unwrap().list_titles();
    Ok(serde_json::json!({ "entries": entries }))
}

#[tauri::command]
pub async fn cmd_vault_add(
    entry: crate::storage::vault::VaultEntry,
    state: St<'_>,
) -> Result<String, String> {
    state.vault.lock().unwrap()
        .add(entry, &state.db, &state.master_key.lock().unwrap())
        .map_err(es)
}

#[tauri::command]
pub async fn cmd_vault_update(
    id: String, entry: crate::storage::vault::VaultEntry,
    state: St<'_>,
) -> Result<(), String> {
    state.vault.lock().unwrap()
        .update(&id, entry, &state.db, &state.master_key.lock().unwrap())
        .map_err(es)
}

#[tauri::command]
pub async fn cmd_vault_delete(
    id: String, state: St<'_>,
) -> Result<(), String> {
    state.vault.lock().unwrap().delete(&id, &state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_vault_autofill(
    url: String, state: St<'_>,
) -> Result<serde_json::Value, String> {
    let domain = crate::utils::domain_of(&url);
    let key    = *state.master_key.lock().unwrap();
    let result = state.vault.lock().unwrap()
        .autofill_for_domain(&domain, &key)
        .map_err(es)?;
    Ok(serde_json::json!({ "matches": result }))
}


#[tauri::command]
pub async fn cmd_slm_status(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let online = crate::features::labs::is_lab_enabled(&state.db, "slm_server");
    Ok(serde_json::json!({ "enabled": online }))
}

#[tauri::command]
pub async fn cmd_slm_complete(
    prompt: String,
    state: St<'_>,
) -> Result<String, String> {
    use crate::ai::slm::{ChatRequest, ChatMessage, SlmServer};

    let mut cache = state.slm_cache.lock().await;
    let server = if let Some(srv) = cache.as_ref() {
        srv.clone()
    } else {
        let privacy_mode = state.privacy.read().map(|p| p.extreme_mode).unwrap_or(false);
        let srv = std::sync::Arc::new(
            SlmServer::new(privacy_mode, None).await,
        );
        *cache = Some(srv.clone());
        srv
    };
    drop(cache);

    let req = ChatRequest {
        messages: vec![ChatMessage { role: "user".into(),
        content: prompt }],
        model: None,
        stream: false,
        max_tokens: None,
    };
    let resp = server.chat(&req).await.map_err(es)?;
    Ok(resp.choices.into_iter().next()
        .map(|c| c.message.content)
        .unwrap_or_default())
}

#[tauri::command]
pub async fn cmd_slm_reset(
    state: St<'_>,
) -> Result<(), String> {
    *state.slm_cache.lock().await = None;
    Ok(())
}


#[tauri::command]
pub async fn cmd_ai_rename_suggest(
    ctx: crate::ai::renamer::DownloadContext,
    state: St<'_>,
) -> Result<crate::ai::renamer::RenameResult, String> {
    if crate::features::labs::is_lab_enabled(&state.db, "slm_server") {
        match crate::ai::renamer::suggest_via_slm(&ctx).await {
            Ok(result) => return Ok(result),
            Err(e) => tracing::debug!("ai_rename: SLM failed ({}), using slug fallback", e),
        }
    }
    Ok(crate::ai::renamer::suggest_from_title(&ctx))
}


#[tauri::command]
pub async fn cmd_shadow_search(
    query: String,
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let ws = state.workspace_id();
    let results = crate::ai::shadow_index::search_local(&state.db, &ws, &query)
        .map_err(es)?;
    Ok(serde_json::json!({ "results": results }))
}


#[tauri::command]
pub async fn cmd_mcp_status(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let token_exists = std::path::Path::new(&state.data_dir).join("mcp.token").exists();
    Ok(serde_json::json!({
        "port": crate::ai::mcp::MCP_PORT,
        "token_present": token_exists,
    }))
}


#[tauri::command]
pub async fn cmd_tabs_list(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let tabs = state.tabs.lock().unwrap().list();
    Ok(serde_json::json!({ "tabs": tabs }))
}

#[tauri::command]
pub async fn cmd_tab_open(
    url: String, state: St<'_>,
) -> Result<String, String> {
    let id = state.tabs.lock().unwrap().open(url);
    Ok(id)
}

#[tauri::command]
pub async fn cmd_tab_close(
    id: String, state: St<'_>,
) -> Result<(), String> {
    state.tabs.lock().unwrap().close(&id);
    Ok(())
}

#[tauri::command]
pub async fn cmd_tab_activate(
    id: String, state: St<'_>,
) -> Result<(), String> {
    state.tabs.lock().unwrap().activate(&id);
    Ok(())
}


#[tauri::command]
pub async fn cmd_tab_limit_get(
    state: St<'_>,
) -> Result<u32, String> {
    Ok(crate::browser::budget::TabBudgetConfig::load(&state.db).max_tabs)
}

#[tauri::command]
pub async fn cmd_tab_limit_set(
    limit: u32, state: St<'_>,
) -> Result<(), String> {
    let cfg = crate::browser::budget::TabBudgetConfig { max_tabs: limit.clamp(1, 50) };
    cfg.save(&state.db).map_err(es)
}


#[tauri::command]
pub async fn cmd_tab_proxy_set(
    tab_id: String, proxy: Option<crate::browser::proxy::ProxyConfig>,
    state: St<'_>,
) -> Result<(), String> {
    state.tab_proxy.set(&tab_id, proxy.clone()).map_err(es)?;
    if let Some(ref p) = proxy {
        crate::browser::proxy::save_proxy(&state.db, &tab_id, p).map_err(es)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_tab_proxy_get(
    tab_id: String, state: St<'_>,
) -> Result<Option<crate::browser::proxy::ProxyConfig>, String> {
    Ok(state.tab_proxy.get(&tab_id)
        .or_else(|| crate::browser::proxy::load_proxy(&state.db, &tab_id)))
}

#[tauri::command]
pub async fn cmd_tab_proxy_remove(
    tab_id: String, state: St<'_>,
) -> Result<(), String> {
    state.tab_proxy.remove(&tab_id);
    Ok(())
}


#[tauri::command]
pub async fn cmd_dom_crush(
    domain: String, selector: String,
    state: St<'_>,
) -> Result<(), String> {
    crate::browser::dom_crusher::add_rule(&state.db, &domain, &selector)
        .map_err(es)
}

#[tauri::command]
pub async fn cmd_dom_blocks_for(
    domain: String, state: St<'_>,
) -> Result<Vec<String>, String> {
    crate::browser::dom_crusher::rules_for_domain(&state.db, &domain)
        .map_err(es)
}


#[tauri::command]
pub async fn cmd_boosts_for_domain(
    domain: String, state: St<'_>,
) -> Result<Vec<crate::browser::boosts::BoostRule>, String> {
    crate::browser::boosts::boosts_for_domain(&state.db, &domain).map_err(es)
}

#[tauri::command]
pub async fn cmd_boosts_list(
    state: St<'_>,
) -> Result<Vec<crate::browser::boosts::BoostRule>, String> {
    crate::browser::boosts::all_boosts(&state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_boost_upsert(
    rule: crate::browser::boosts::BoostRule,
    state: St<'_>,
) -> Result<(), String> {
    if rule.builtin {
        return Err("built-in Boosts cannot be modified via this command".into());
    }
    crate::browser::boosts::upsert(&state.db, &rule).map_err(es)
}

#[tauri::command]
pub async fn cmd_boost_delete(
    id: String, state: St<'_>,
) -> Result<(), String> {
    crate::browser::boosts::delete(&state.db, &id).map_err(es)
}


#[tauri::command]
pub async fn cmd_totp_list(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let entries = state.totp.lock().unwrap().list_accounts();
    Ok(serde_json::json!({ "accounts": entries }))
}

#[tauri::command]
pub async fn cmd_totp_add(
    account: crate::auth::totp::TotpAccount,
    state: St<'_>,
) -> Result<String, String> {
    let key = *state.master_key.lock().unwrap();
    state.totp.lock().unwrap().add(account, &state.db, &key).map_err(es)
}

#[tauri::command]
pub async fn cmd_totp_code(
    account_id: String, state: St<'_>,
) -> Result<crate::auth::totp::TotpCode, String> {
    state.totp.lock().unwrap().current_code(&account_id).map_err(es)
}

#[tauri::command]
pub async fn cmd_totp_delete(
    account_id: String, state: St<'_>,
) -> Result<(), String> {
    state.totp.lock().unwrap().delete(&account_id, &state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_totp_import(
    format: String,
    data: String,
    state: St<'_>,
) -> Result<u32, String> {
    let key = *state.master_key.lock().unwrap();
    state.totp.lock().unwrap()
        .import(&format, &data, &state.db, &key)
        .map_err(es)
}


#[tauri::command]
pub async fn cmd_biometric_verify(
    reason: String,
) -> Result<bool, String> {
    crate::auth::passkey::verify(&reason).await.map_err(es)
}


#[tauri::command]
pub async fn cmd_trust_get(
    domain: String, state: St<'_>,
) -> Result<String, String> {
    let level = state.trust.lock().unwrap().get(&domain);
    Ok(level.as_str().to_owned())
}

#[tauri::command]
pub async fn cmd_trust_set(
    domain: String,
    level: String,
    state: St<'_>,
) -> Result<(), String> {
    state.trust.lock().unwrap().set(&domain, &level, "user", &state.db);
    Ok(())
}


#[tauri::command]
pub async fn cmd_noise_fingerprint(
    state: St<'_>,
) -> Result<String, String> {
    let kp = state.with_master_key(|key| {
        crate::sync::noise::derive_keypair_from_master(key)
    });
    Ok(kp.fingerprint())
}


#[tauri::command]
pub async fn cmd_nostr_publish(
    kind: u64,
    content: String,
    state: St<'_>,
) -> Result<String, String> {
    let key = *state.master_key.lock().unwrap();
    let relay = state.db.get_setting("nostr_relay").unwrap_or_else(|| crate::sync::nostr::DEFAULT_RELAY.to_owned());
    crate::sync::nostr::publish(&relay, kind, &content, &key)
        .await.map_err(es)
}

#[tauri::command]
pub async fn cmd_nostr_fetch(
    kind: u64, state: St<'_>,
) -> Result<serde_json::Value, String> {
    let key = *state.master_key.lock().unwrap();
    let relay = state.db.get_setting("nostr_relay").unwrap_or_else(|| crate::sync::nostr::DEFAULT_RELAY.to_owned());
    let events = crate::sync::nostr::fetch(&relay, kind, &key)
        .await.map_err(es)?;
    Ok(serde_json::json!({ "events": events }))
}


#[tauri::command]
pub async fn cmd_zen_status(
    state: St<'_>,
) -> Result<crate::features::zen::ZenConfig, String> {
    Ok(state.zen.lock().unwrap().clone())
}

#[tauri::command]
pub async fn cmd_zen_activate(
    state: St<'_>,
) -> Result<(), String> {
    state.zen.lock().unwrap().activate(&state.db);
    Ok(())
}

#[tauri::command]
pub async fn cmd_zen_deactivate(
    _unlock_phrase: String, // reserved for future phrase-lock feature
    state: St<'_>,
) -> Result<bool, String> {
    state.zen.lock().unwrap().deactivate(&state.db);
    Ok(true)
}


#[tauri::command]
pub async fn cmd_rss_feeds_list(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let feeds = state.rss.lock().unwrap().list_feeds();
    Ok(serde_json::json!({ "feeds": feeds }))
}

#[tauri::command]
pub async fn cmd_rss_feed_add(
    url: String, state: St<'_>,
) -> Result<String, String> {
    state.rss.lock().unwrap().add_feed(url, &state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_rss_feed_remove(
    id: String, state: St<'_>,
) -> Result<(), String> {
    state.rss.lock().unwrap().remove_feed(&id, &state.db).map_err(es)
}

#[tauri::command]
pub async fn cmd_rss_items(
    feed_id: Option<String>, state: St<'_>,
) -> Result<serde_json::Value, String> {
    let items = state.rss.lock().unwrap().items(feed_id.as_deref(), 50);
    Ok(serde_json::json!({ "items": items }))
}

#[tauri::command]
pub async fn cmd_rss_mark_read(
    item_id: String, state: St<'_>,
) -> Result<(), String> {
    state.rss.lock().unwrap().mark_read(&item_id, &state.db).map_err(es)
}


#[tauri::command]
pub async fn cmd_panic_toggle(
    app_handle: tauri::AppHandle,
    state: St<'_>,
) -> Result<(), String> {
    let cfg = crate::features::panic::load_config(&state.db);
    crate::features::panic::toggle(&app_handle, &cfg);
    Ok(())
}

#[tauri::command]
pub async fn cmd_panic_config_get(
    state: St<'_>,
) -> Result<crate::features::panic::PanicConfig, String> {
    Ok(crate::features::panic::load_config(&state.db))
}

#[tauri::command]
pub async fn cmd_panic_config_set(
    config: crate::features::panic::PanicConfig,
    state: St<'_>,
) -> Result<(), String> {
    crate::features::panic::save_config(&state.db, &config).map_err(es)
}


#[tauri::command]
pub async fn cmd_breach_check_password(
    password: String, state: St<'_>,
) -> Result<crate::features::breach::PasswordBreachResult, String> {
    use sha1::{Sha1, Digest};
    let mut h = Sha1::new();
    h.update(password.as_bytes());
    let sha1_hex = format!("{:X}", h.finalize());

    if let Some(cached) = crate::features::breach::load_cached_password(&state.db, &sha1_hex) {
        return Ok(cached);
    }
    let client = reqwest::Client::new();
    let result = crate::features::breach::check_password(&client, &password)
        .await.map_err(es)?;
    crate::features::breach::cache_password_result(&state.db, &sha1_hex, &result);
    Ok(result)
}

#[tauri::command]
pub async fn cmd_breach_check_email(
    email: String, state: St<'_>,
) -> Result<crate::features::breach::EmailBreachResult, String> {
    if state.db.get_setting("breach_monitor_email_optin").as_deref() != Some("true") {
        return Err("Email breach check requires explicit opt-in".into());
    }
    let client = reqwest::Client::new();
    crate::features::breach::check_email(&client, &email)
        .await.map_err(es)
}


#[tauri::command]
pub async fn cmd_search_engines_list() -> Result<Vec<crate::features::search::SearchEngine>, String> {
    Ok(crate::features::search::builtin_engines())
}

#[tauri::command]
pub async fn cmd_search_engine_get_default(
    state: St<'_>,
) -> Result<String, String> {
    Ok(crate::features::search::get_default(&state.db))
}

#[tauri::command]
pub async fn cmd_search_engine_set_default(
    engine_id: String, state: St<'_>,
) -> Result<(), String> {
    crate::features::search::set_default(&state.db, &engine_id).map_err(es)
}

#[tauri::command]
pub async fn cmd_searxng_set_endpoint(
    endpoint: String, state: St<'_>,
) -> Result<(), String> {
    crate::features::search::set_searxng_endpoint(&state.db, &endpoint)
        .map_err(es)
}


#[tauri::command]
pub async fn cmd_tos_audit(
    url: String, text: String,
) -> Result<serde_json::Value, String> {
    let flags = crate::features::tos::audit_text(&url, &text);
    Ok(serde_json::json!({ "flags": flags }))
}


#[tauri::command]
pub async fn cmd_war_report(
    state: St<'_>,
) -> Result<crate::features::report::WarReport, String> {
    let ws = state.workspace_id();
    crate::features::report::build(&state.db, &ws).map_err(es)
}


#[tauri::command]
pub async fn cmd_labs_list(
    state: St<'_>,
) -> Result<Vec<crate::features::labs::Lab>, String> {
    Ok(crate::features::labs::all_labs(&state.db))
}

#[tauri::command]
pub async fn cmd_lab_set(
    id: String,
    enabled: bool,
    state: St<'_>,
) -> Result<(), String> {
    crate::features::labs::set_lab(&state.db, &id, enabled).map_err(es)
}


#[tauri::command]
pub async fn cmd_power_budget_status() -> Result<serde_json::Value, String> {
    let budget = crate::features::sentinel::power_budget_current();
    Ok(serde_json::json!({
        "state":                    format!("{:?}", budget.state),
        "battery_pct":              budget.battery_pct,
        "sentinel_interval_secs":   budget.sentinel_interval_secs,
        "tab_budget_interval_secs": budget.tab_budget_interval_secs,
        "pir_enabled":              budget.pir_enabled,
        "decoy_enabled":            budget.decoy_enabled,
    }))
}

#[tauri::command]
pub async fn cmd_signal_window_ready(
    state: St<'_>,
) -> Result<(), String> {
    state.window_ready_token.cancel();
    Ok(())
}


#[tauri::command]
pub async fn cmd_home_base_data(
    state: St<'_>,
) -> Result<serde_json::Value, String> {
    let ws_id = state.workspace_id();

    let top_domains = tokio::task::spawn_blocking({
        let db = state.db.clone();
    let ws = ws_id.clone();
        move || db.top_domains(&ws, 8)
    }).await.map_err(es)?.unwrap_or_default();

    let pinned = tokio::task::spawn_blocking({
        let db = state.db.clone();
    let ws = ws_id.clone();
        move || db.pinned_bookmarks(&ws, 8)
    }).await.map_err(es)?.unwrap_or_default();

    let recent_museum = tokio::task::spawn_blocking({
        let db = state.db.clone();
    let ws = ws_id.clone();
        move || db.list_bundles(&ws, 3)
    }).await.map_err(es)?.unwrap_or_default();

    let rss_unread = state.rss.lock().unwrap().unread_count();
    let slm_online = state.db.get_setting("lab_slm_server").map(|v| v == "true").unwrap_or(false);

    Ok(serde_json::json!({
        "top_domains":   top_domains,
        "pinned":        pinned,
        "recent_museum": recent_museum,
        "rss_unread":    rss_unread,
        "slm_status":    { "online": slm_online },
    }))
}

#[tauri::command]
pub async fn cmd_peek_fetch(
    url: String, state: St<'_>,
) -> Result<serde_json::Value, String> {
    let domain = crate::utils::domain_of(&url);

    if crate::engine::blocker::is_blocked(&domain) {
        return Ok(serde_json::json!({
            "url": url, "title": domain,
            "description": null, "og_image": null, "blocked": true,
        }));
    }

    let ua     = state.current_ua(false);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .user_agent(ua)
        .build().map_err(es)?;

    let html = client.get(&url).send().await.map_err(es)?
        .text().await.unwrap_or_default();

    let title       = extract_meta(&html, "og:title").or_else(|| extract_tag(&html, "title")).unwrap_or_else(|| domain.clone());
    let description = extract_meta(&html, "og:description").or_else(|| extract_meta(&html, "description"));
    let og_image    = extract_meta(&html, "og:image");

    Ok(serde_json::json!({
        "url": url, "title": title,
        "description": description, "og_image": og_image, "blocked": false,
    }))
}

fn extract_meta(html: &str, name: &str) -> Option<String> {
    let lower   = html.to_lowercase();
    let needle  = format!("property=\"{}\"", name.to_lowercase());
    let needle2 = format!("name=\"{}\"", name.to_lowercase());
    let pos = lower.find(&needle).or_else(|| lower.find(&needle2))?;
    let after       = &html[pos..];
    let content_pos = after.to_lowercase().find("content=\"")? + "content=\"".len();
    let end         = after[content_pos..].find('"')?;
    Some(after[content_pos..content_pos + end].to_owned())
}

fn extract_tag(html: &str, tag: &str) -> Option<String> {
    let open  = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = html.to_lowercase().find(&open)? + open.len();
    let end   = html[start..].to_lowercase().find(&close)?;
    Some(html[start..start + end].trim().to_owned())
}


#[tauri::command]
pub async fn cmd_compliance_registry() -> Result<serde_json::Value, String> {
    let registry = crate::features::compliance::feature_registry();
    Ok(serde_json::json!({ "features": registry }))
}


/// Return the fingerprint normalisation injection script.
/// Called by the frontend's init sequence to apply fp-norm to the WebView.
#[tauri::command]
pub async fn cmd_fp_norm_script(
    state: St<'_>,
) -> Result<String, String> {
    Ok(state.fp_norm_script())
}

