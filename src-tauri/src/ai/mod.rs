// diatom/src-tauri/src/ai — v0.14.3
// Local AI: SLM microkernel, download renamer, shadow index, MCP host.
pub mod slm;
pub mod renamer;
pub mod shadow_index;
pub mod mcp;

pub use renamer::{DownloadContext, RenameResult};
pub use slm::SlmServer;
