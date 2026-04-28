//! `diatom_ui` — stable UI-renderer facade for Diatom.
//!
//! This crate sits between the Diatom DevPanel code and the GPUI framework.
//! DevPanel crates declare a dependency on `diatom_ui`; they do not depend on
//! `gpui` directly. When GPUI's internal API changes (it has no semver
//! guarantee), only the `GpuiRenderer` implementation in this file needs
//! updating.
//!
//! ## Design rationale
//!
//! GPUI is Zed's internal UI framework. Diatom vendors it via `strip-zed.sh`.
//! It has no public API stability promise: Zed can rename, move, or remove any
//! type at any time. By routing all UI calls through the trait defined here,
//! we:
//!
//! 1. Contain GPUI churn to a single file.
//! 2. Keep the DevPanel testable with a mock renderer (no GPUI needed in unit
//!    tests).
//! 3. Make a future renderer swap (e.g. to Servo or a different native toolkit)
//!    a single-crate change.
//!
//! ## What goes here
//!
//! Only the subset of GPUI used by the DevPanel's three panels (Console,
//! Network, Sources). As the DevPanel grows, new methods are added to the
//! trait and implemented in `GpuiRenderer`. The trait is **not** a general
//! GPUI wrapper — it models exactly what Diatom needs.

use anyhow::Result;

// ── Tab data types ────────────────────────────────────────────────────────────

/// A single entry in the tab bar.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TabDescriptor {
    pub id:     String,
    pub url:    String,
    pub title:  String,
    /// `"awake"` | `"shallow"` | `"deep"`
    pub sleep:  String,
}

// ── Toolbar state ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ToolbarState {
    pub can_back:    bool,
    pub can_forward: bool,
    pub loading:     bool,
    pub url:         String,
    pub title:       String,
}

// ── Panel kind ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevPanelKind {
    Console,
    Network,
    Sources,
}

// ── Console entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub level:       ConsoleLevel,
    pub text:        String,
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel { Log, Info, Warn, Error, Debug }

// ── The stable trait ─────────────────────────────────────────────────────────

/// A Diatom UI renderer.
///
/// All methods have default no-op implementations so callers can be compiled
/// against the trait without a live GPUI environment (e.g. in unit tests or
/// CI builds that do not link the GPU stack).
pub trait UiRenderer: Send + Sync {
    // ── Tab bar ───────────────────────────────────────────────────────────────

    /// Replace the full tab list with `tabs`. `active_id` is the currently
    /// visible tab.
    fn update_tabs(&self, tabs: &[TabDescriptor], active_id: Option<&str>) {}

    // ── Toolbar ───────────────────────────────────────────────────────────────

    /// Refresh the toolbar (back/forward state, loading spinner, URL text).
    fn update_toolbar(&self, state: &ToolbarState) {}

    // ── DevPanel ──────────────────────────────────────────────────────────────

    /// Ensure the DevPanel window is visible and focused on `panel`.
    fn show_dev_panel(&self, panel: DevPanelKind) {}

    /// Append a console entry to the Console panel.
    fn push_console_entry(&self, entry: ConsoleEntry) {}

    /// Replace the network log displayed in the Network panel.
    fn update_network_log(&self, entries: Vec<NetworkEntry>) {}

    /// Set the source file content shown in the Sources panel.
    fn set_source_file(&self, url: &str, content: &str) {}
}

// ── Network entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub id:             String,
    pub url:            String,
    pub method:         String,
    pub status:         Option<u16>,
    pub latency_ms:     u64,
    pub request_bytes:  u64,
    pub response_bytes: u64,
    pub blocked:        bool,
}

// ── GPUI implementation ───────────────────────────────────────────────────────

/// The live GPUI-backed renderer.
///
/// This is the only type in the Diatom codebase allowed to import from `gpui`
/// directly. Everything outside this module goes through `UiRenderer`.
///
/// The implementation stubs below are intentionally minimal — they mark the
/// boundary. Fill them in as each DevPanel feature is implemented; the trait
/// surface above defines the contract.
pub struct GpuiRenderer {
    // Handle to the GPUI application context.
    // Wrapped in Option so unit tests can construct GpuiRenderer::headless().
    #[allow(dead_code)]
    cx: Option<gpui::AsyncAppContext>,
}

impl GpuiRenderer {
    /// Create a live renderer with a GPUI app context.
    pub fn new(cx: gpui::AsyncAppContext) -> Self {
        Self { cx: Some(cx) }
    }

    /// Create a no-op renderer for use in unit tests (no GPU required).
    pub fn headless() -> Self {
        Self { cx: None }
    }
}

impl UiRenderer for GpuiRenderer {
    fn update_tabs(&self, _tabs: &[TabDescriptor], _active_id: Option<&str>) {
        // TODO: call cx.update(|cx| { /* update tab model */ })
        // Deferred to DevPanel v1 implementation milestone.
    }

    fn update_toolbar(&self, _state: &ToolbarState) {
        // TODO: push toolbar state into GPUI view model
    }

    fn show_dev_panel(&self, _panel: DevPanelKind) {
        // TODO: focus GPUI devtools window on correct panel
    }

    fn push_console_entry(&self, _entry: ConsoleEntry) {
        // TODO: append to GPUI console list model
    }

    fn update_network_log(&self, _entries: Vec<NetworkEntry>) {
        // TODO: refresh GPUI network table
    }

    fn set_source_file(&self, _url: &str, _content: &str) {
        // TODO: update GPUI sources editor model
    }
}

// ── No-op mock (for tests) ────────────────────────────────────────────────────

/// A completely inert renderer used in tests and benchmarks.
pub struct NullRenderer;
impl UiRenderer for NullRenderer {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_renderer_implements_trait() {
        let r: &dyn UiRenderer = &NullRenderer;
        r.update_tabs(&[], None);
        r.update_toolbar(&ToolbarState::default());
        r.show_dev_panel(DevPanelKind::Console);
        r.push_console_entry(ConsoleEntry {
            level: ConsoleLevel::Log,
            text: "test".into(),
            source_file: None,
            source_line: None,
        });
        r.update_network_log(vec![]);
        r.set_source_file("http://localhost/app.js", "console.log(1)");
    }

    #[test]
    fn gpui_renderer_headless_does_not_panic() {
        let r = GpuiRenderer::headless();
        r.update_tabs(&[TabDescriptor {
            id: "t1".into(), url: "https://example.com".into(),
            title: "Example".into(), sleep: "awake".into(),
        }], Some("t1"));
        r.update_toolbar(&ToolbarState {
            can_back: true, can_forward: false, loading: false,
            url: "https://example.com".into(), title: "Example".into(),
        });
    }
}
