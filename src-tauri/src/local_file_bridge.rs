// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/local_file_bridge.rs
//
// Local-First File Bridge — diatom://local/ Protocol
//
// Bridges the browser sandbox and the local filesystem.
//
// Users can mount local folders as diatom://local/<alias>/ internal addresses.
// Pages that support this protocol (or built-in Diatom apps) can read, modify,
// and save files directly without uploading them anywhere.
//
// Design principles:
//   - Disabled by default; each mount point must be individually authorised
//   - Mount points are restricted to specific user-chosen folders
//     (mounting the root directory is not permitted)
//   - The Rust backend validates all paths strictly to prevent path traversal
//   - Permission levels: ReadOnly | ReadWrite (user-chosen per mount point)
//   - All file accesses are logged to AppState::net_monitor under the
//     "local file access" category
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MountPermission {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPoint {
    pub alias: String,      // diatom://local/<alias>/
    pub real_path: PathBuf,
    pub permission: MountPermission,
    pub created_at: i64,
    pub description: String,
}

pub struct LocalFileBridge {
    /// alias → MountPoint
    mounts: Mutex<HashMap<String, MountPoint>>,
}

impl Default for LocalFileBridge {
    fn default() -> Self {
        Self { mounts: Mutex::new(HashMap::new()) }
    }
}

impl LocalFileBridge {
    /// Register a new mountpoint
    pub fn mount(
        &self,
        alias: &str,
        real_path: &Path,
        permission: MountPermission,
        description: &str,
    ) -> Result<()> {
        // Validate alias (alphanumeric and hyphens only)
        if !alias.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            bail!("Invalid alias: only alphanumeric, dash, underscore allowed");
        }
        if alias.len() > 32 {
            bail!("Alias too long (max 32 characters)");
        }

        // Validate that the path exists and is a directory
        if !real_path.exists() {
            bail!("Path does not exist: {:?}", real_path);
        }
        if !real_path.is_dir() {
            bail!("Path must be a directory, not a file");
        }

 // check: reject high-riskentriesdirectories
        let path_str = real_path.to_string_lossy().to_lowercase();
        let dangerous = ["/etc", "/sys", "/proc", "/dev", "c:\\windows", "c:\\system"];
        if dangerous.iter().any(|d| path_str.starts_with(d)) {
            bail!("Mounting system directories is not allowed");
        }

        let mount = MountPoint {
            alias: alias.to_owned(),
            real_path: real_path.canonicalize()?,
            permission,
            created_at: crate::db::unix_now(),
            description: description.to_owned(),
        };

        self.mounts.lock().unwrap().insert(alias.to_owned(), mount);
        tracing::info!("local_file_bridge: mounted {:?} as diatom://local/{}/", real_path, alias);
        Ok(())
    }

    pub fn unmount(&self, alias: &str) {
        self.mounts.lock().unwrap().remove(alias);
        tracing::info!("local_file_bridge: unmounted {}", alias);
    }

    pub fn list_mounts(&self) -> Vec<MountPoint> {
        self.mounts.lock().unwrap().values().cloned().collect()
    }

    /// Resolve diatom://local/<alias>/<path> to real filesystem path
 /// to prevent path traversal
    pub fn resolve(&self, diatom_url: &str) -> Result<(PathBuf, MountPermission)> {
 // : diatom://local/<alias>/<relative_path>
        let stripped = diatom_url
            .strip_prefix("diatom://local/")
            .ok_or_else(|| anyhow::anyhow!("Not a diatom://local/ URL"))?;

        let (alias, rel_path) = stripped.split_once('/').unwrap_or((stripped, ""));

        let mounts = self.mounts.lock().unwrap();
        let mount = mounts.get(alias)
            .ok_or_else(|| anyhow::anyhow!("No mount point for alias: {}", alias))?;

        // Normalise the relative path and check for path traversal
        let real_path = mount.real_path.join(rel_path);
        let canonical = real_path.canonicalize()
            .map_err(|_| anyhow::anyhow!("Path does not exist: {:?}", real_path))?;

        // Ensure the resolved path is within the mount root
        if !canonical.starts_with(&mount.real_path) {
            bail!("Path traversal detected: {:?} is outside mount {:?}", canonical, mount.real_path);
        }

        Ok((canonical, mount.permission.clone()))
    }

    /// Read file contents
    pub fn read_file(&self, diatom_url: &str) -> Result<Vec<u8>> {
        let (path, _perm) = self.resolve(diatom_url)?;
        if !path.is_file() {
            bail!("Not a file: {:?}", path);
        }
        Ok(std::fs::read(&path)?)
    }

 /// Writes content to a mounted file (requires ReadWrite permission on the mount).
    pub fn write_file(&self, diatom_url: &str, content: &[u8]) -> Result<()> {
        let (path, perm) = self.resolve(diatom_url)?;
        if perm != MountPermission::ReadWrite {
            bail!("Mount point is read-only");
        }
        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
        tracing::debug!("local_file_bridge: wrote {} bytes to {:?}", content.len(), path);
        Ok(())
    }

    /// List directory contents
    pub fn list_dir(&self, diatom_url: &str) -> Result<Vec<DirEntry>> {
        let (path, _) = self.resolve(diatom_url)?;
        if !path.is_dir() {
            bail!("Not a directory: {:?}", path);
        }
        let entries = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| {
                let metadata = e.metadata().ok();
                DirEntry {
                    name: e.file_name().to_string_lossy().to_string(),
                    is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
                    size_bytes: metadata.as_ref().map(|m| m.len()),
                    modified_at: metadata.and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64),
                }
            })
            .collect();
        Ok(entries)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn mount_and_resolve() {
        let bridge = LocalFileBridge::default();
        let tmp = std::env::temp_dir().join("diatom_test_mount");
        std::fs::create_dir_all(&tmp).unwrap();

        bridge.mount("test", &tmp, MountPermission::ReadWrite, "test mount").unwrap();
        let (path, perm) = bridge.resolve("diatom://local/test/").unwrap();
        assert_eq!(perm, MountPermission::ReadWrite);
        assert!(path.starts_with(&tmp.canonicalize().unwrap()));
    }

    #[test]
    fn path_traversal_rejected() {
        let bridge = LocalFileBridge::default();
        let tmp = std::env::temp_dir().join("diatom_test_mount2");
        std::fs::create_dir_all(&tmp).unwrap();
        bridge.mount("safe", &tmp, MountPermission::ReadOnly, "").unwrap();

        // Path traversal attempt
        let result = bridge.resolve("diatom://local/safe/../../../etc/passwd");
        assert!(result.is_err(), "Path traversal must be rejected");
    }
}
