
use serde::{Deserialize, Serialize};

/// Monotonically-increasing request ID for correlated request/response pairs.
pub type RequestId = u64;


/// Messages sent from the Diatom browser shell to the GPUI DevPanel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BrowserMessage {
    /// DevPanel should open (or focus) with the given project root.
    Open {
        id: RequestId,
        /// Absolute path to the workspace root.
        project_root: String,
    },

    /// Notify the DevPanel of the currently loaded page's URL and title.
    PageNavigated {
        url: String,
        title: String,
        /// Serialised DOM snapshot (tag, id, classes, attrs).
        /// None when privacy mode suppresses DOM export.
        dom_snapshot: Option<DomNode>,
    },

    /// Push a console log entry from the WebView into the DevPanel console.
    ConsoleEntry {
        level: ConsoleLevel,
        text: String,
        source_file: Option<String>,
        source_line: Option<u32>,
    },

    /// Push a network event captured by Diatom's net_monitor.
    NetworkEvent(NetworkEventPayload),

    /// Respond to a DevPanel source-file fetch request.
    SourceFileContent {
        id: RequestId,
        url: String,
        /// UTF-8 source text (JS, CSS, HTML).
        content: String,
    },

    /// SLM streaming completion delta forwarded from :11435.
    SlmCompletion {
        id: RequestId,
        delta: String,
        done: bool,
    },

    /// Browser shell is shutting down — DevPanel should save state and exit.
    Shutdown,
}


/// Messages sent from the GPUI DevPanel to the Diatom browser shell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum DevPanelMessage {
    /// Request the source text of a URL (Sources panel).
    FetchSourceFile {
        id: RequestId,
        url: String,
    },

    /// Evaluate JavaScript in the current WebView page.
    EvalJs {
        id: RequestId,
        script: String,
    },

    /// Highlight a DOM element in the WebView (mirrors Chrome element picker).
    HighlightElement {
        selector: String,
    },

    /// Request an SLM completion from Diatom's :11435 endpoint.
    SlmRequest {
        id: RequestId,
        model: String,
        messages: Vec<SlmMessage>,
        stream: bool,
    },

    /// DevPanel wants the current net_monitor snapshot.
    RequestNetworkLog {
        id: RequestId,
    },

    /// Open a resolved local filesystem path in the external Zed IDE.
    ///
    /// The Diatom shell resolves the URL to a local path (project_root + URL path)
    /// and spawns `zed <path>:<line>`. If `zed` is not in PATH, a notification
    /// is shown; no fallback to a cloud URL is attempted.
    OpenInZedIde {
        /// Source URL as displayed in the Sources panel.
        url: String,
        /// Optional line number to jump to.
        line: Option<u32>,
    },

    /// DevPanel is ready; Diatom should send the initial page state.
    Ready,

    /// DevPanel closed itself (user closed the editor window).
    Closed,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<DomNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEventPayload {
    pub id: String,
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub latency_ms: u64,
    pub blocked: bool,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlmMessage {
    pub role: String,
    pub content: String,
}

/// Context snapshot pushed to the Resonance UDS for Zed to consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceContext {
    pub page_url: String,
    pub page_title: String,
    /// Recent console errors (up to last 20).
    pub console_errors: Vec<String>,
    /// Simplified DOM root (depth-limited to 3 levels).
    pub dom_root: Option<DomNode>,
    /// Active source file URL and content snippet (first 4 KB).
    pub active_source: Option<ActiveSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSource {
    pub url: String,
    pub snippet: String,
}

