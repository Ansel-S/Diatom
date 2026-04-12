// diatom/src-tauri/src/features — v0.14.3
// Standalone features: Zen, RSS, Panic Button, Breach Monitor, Search, etc.
pub mod zen;
pub mod rss;
pub mod panic;
pub mod breach;
pub mod search;
pub mod pricing;
pub mod tos;
pub mod localfiles;
pub mod sentinel;
pub mod report;
pub mod labs;
pub mod compliance;

pub use zen::ZenConfig;
pub use rss::RssStore;
pub use sentinel::SentinelCache;
pub use labs::is_lab_enabled;
pub use localfiles::LocalFileBridge;
pub use panic::PanicConfig;
pub use breach::{PasswordBreachResult, EmailBreachResult};
pub use search::SearchEngine;
