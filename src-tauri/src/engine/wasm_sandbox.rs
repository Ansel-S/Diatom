
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::browser::dev_panel::DevPanelState;
use crate::state::AppState;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id:       String,
    pub name:     String,
    pub version:  String,
    pub wasm_hash: String,
}


/// Stage a wasm binary into `~/.diatom/extensions/<id>/` and return a manifest.
/// Called internally; the public Tauri surface is cmd_plugin_install in commands.rs.
pub async fn load_plugin(
    path: String,
    app_state: State<'_, AppState>,
    panel_state: State<'_, DevPanelState>,
) -> Result<PluginManifest, String> {
    let wasm_bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("read wasm: {e}"))?;

    let hash = sha256_hex(&wasm_bytes);

    let ext_dir = app_state
        .data_dir
        .join("extensions")
        .join(&hash[..16]);
    tokio::fs::create_dir_all(&ext_dir)
        .await
        .map_err(|e| format!("create ext dir: {e}"))?;
    let dest = ext_dir.join("extension.wasm");
    tokio::fs::write(&dest, &wasm_bytes)
        .await
        .map_err(|e| format!("write wasm: {e}"))?;


    Ok(PluginManifest {
        id:        hash[..16].to_string(),
        name:      path
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .trim_end_matches(".wasm")
            .to_string(),
        version:   "0.0.0".into(),
        wasm_hash: hash,
    })
}

/// Remove a staged plugin by its manifest ID.
/// Called internally; the public Tauri surface is cmd_plugin_remove in commands.rs.
pub async fn unload_plugin(
    id: String,
    app_state: State<'_, AppState>,
    _panel_state: State<'_, DevPanelState>,
) -> Result<(), String> {
    let ext_dir = app_state.data_dir.join("extensions").join(&id);
    if ext_dir.exists() {
        tokio::fs::remove_dir_all(&ext_dir)
            .await
            .map_err(|e| format!("remove ext dir: {e}"))?;
    }
    Ok(())
}

/// List staged plugins by reading the extensions directory.
/// Called internally; the public Tauri surface is cmd_plugin_list in commands.rs.
pub async fn list_plugins(
    app_state: State<'_, AppState>,
) -> Result<Vec<PluginManifest>, String> {
    let ext_root = app_state.data_dir.join("extensions");
    if !ext_root.exists() {
        return Ok(vec![]);
    }

    let mut out = Vec::new();
    let mut dir = tokio::fs::read_dir(&ext_root)
        .await
        .map_err(|e| format!("readdir: {e}"))?;

    while let Ok(Some(entry)) = dir.next_entry().await {
        let wasm = entry.path().join("extension.wasm");
        if !wasm.exists() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let name = tokio::fs::read_to_string(entry.path().join("extension.toml"))
            .await
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("name"))
                    .and_then(|l| l.splitn(2, '=').nth(1))
                    .map(|v| v.trim().trim_matches('"').to_string())
            })
            .unwrap_or_else(|| id.clone());

        out.push(PluginManifest {
            id:        id.clone(),
            name,
            version:   "?".into(),
            wasm_hash: id,
        });
    }
    Ok(out)
}


fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    use ring::digest::{digest, SHA256};
    let d = digest(&SHA256, data);
    d.as_ref().iter().fold(String::with_capacity(64), |mut s, b| {
        write!(s, "{:02x}", b).unwrap();
        s
    })
}

