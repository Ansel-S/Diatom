
// ── v0.10.0 New Commands ────────────────────────────────────────────────────

/// Get the Noise P2P keypair fingerprint for TOFU display in the UI.
#[tauri::command]
pub async fn cmd_noise_fingerprint(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let kp = state.with_master_key(|key| {
        crate::noise_transport::derive_keypair_from_master(key)
    });
    Ok(kp.fingerprint())
}

/// Get OHTTP configuration status.
#[tauri::command]
pub async fn cmd_ohttp_status(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let relay = state.db.get_setting("ohttp_relay")
        .unwrap_or_else(|| crate::ohttp::OHTTP_RELAYS[0].to_owned());
    let has_key = state.db.get_setting("ohttp_key_config").is_some();
    Ok(serde_json::json!({
        "relay": relay,
        "has_key_config": has_key,
        "relays": crate::ohttp::OHTTP_RELAYS,
    }))
}

/// List installed Wasm plugins.
#[tauri::command]
pub async fn cmd_plugin_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::wasm_sandbox::PluginManifest>, String> {
    Ok(state.plugin_registry.list_manifests())
}

/// Install a Wasm plugin from a local file path.
#[tauri::command]
pub async fn cmd_plugin_install(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let plugin = crate::wasm_sandbox::WasmPlugin::load(path.into(), None)
        .map_err(|e| e.to_string())?;
    let id = state.plugin_registry.install(plugin);
    Ok(id.to_string())
}

/// Remove a Wasm plugin by ID.
#[tauri::command]
pub async fn cmd_plugin_remove(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    Ok(state.plugin_registry.remove(uuid))
}

/// Get the Echo DP epsilon setting (privacy budget per weekly computation).
#[tauri::command]
pub async fn cmd_echo_dp_epsilon_get(
    state: tauri::State<'_, AppState>,
) -> Result<f64, String> {
    Ok(state.db.get_setting("echo_dp_epsilon")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(crate::dp_echo::DEFAULT_EPSILON))
}

/// Set the Echo DP epsilon (lower = more private = more noise).
#[tauri::command]
pub async fn cmd_echo_dp_epsilon_set(
    epsilon: f64,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if epsilon <= 0.0 || epsilon > 10.0 {
        return Err("epsilon must be in (0, 10]".into());
    }
    state.db.set_setting("echo_dp_epsilon", &epsilon.to_string())
        .map_err(|e| e.to_string())
}

/// Return the current power budget state (battery level, interval adjustments).
/// Used by the UI to show adaptive scheduling status in Labs / About.
#[tauri::command]
pub async fn cmd_power_budget_status() -> Result<serde_json::Value, String> {
    let budget = crate::power_budget::PowerBudget::current();
    Ok(serde_json::json!({
        "state":                     format!("{:?}", budget.state),
        "battery_pct":               budget.battery_pct,
        "sentinel_interval_secs":    budget.sentinel_interval_secs,
        "tab_budget_interval_secs":  budget.tab_budget_interval_secs,
        "pir_enabled":               budget.pir_enabled,
        "decoy_enabled":             budget.decoy_enabled,
    }))
}
