// diatom/src-tauri/src/browser
// Browser UI: tabs, budget, proxy, DOM tools, accessibility, and DevPanel bridge.
pub mod tabs;
pub mod budget;
pub mod proxy;
pub mod dom_crusher;
pub mod boosts;
pub mod a11y;
pub mod dev_panel;

pub use tabs::TabStore;
pub use budget::{TabBudgetConfig, DEFAULT_TAB_LIMIT};
pub use proxy::{TabProxyRegistry, ProxyConfig};
pub use boosts::BoostRule;
