// diatom/src-tauri/src/engine
// Network request pipeline: blocking, bandwidth, caching, monitoring, tunnelling.
pub mod blocker;
pub mod bandwidth;
pub mod cache;
pub mod monitor;
pub mod ghostpipe;
pub mod compat;
pub mod plugins;
pub mod url_stripper;
pub mod wasm_sandbox;

// Convenience re-exports used by state.rs and commands.rs
pub use bandwidth::BandwidthLimiter;
pub use bandwidth::BandwidthRule;
pub use monitor::NetMonitor;
pub use ghostpipe::GhostPipeConfig;
pub use compat::CompatStore;
pub use plugins::{PluginManifest, PluginRegistry, WasmPlugin};
pub use url_stripper::strip as strip_tracking_params;
