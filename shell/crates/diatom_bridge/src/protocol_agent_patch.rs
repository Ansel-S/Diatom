// ─────────────────────────────────────────────────────────────────────────────
// PATCH: Add to protocol.rs — insert after the existing `SlmMessage` struct
// (just before the ResonanceContext struct near the bottom of the file).
//
// In BrowserMessage enum, add after `SlmCompletion`:
//
//     /// Forward a micro-agent event from the backend runner to the DevPanel.
//     /// The DevPanel surfaces agent progress in its AI overlay sidebar.
//     AgentEvent(AgentEventPayload),
//
// In DevPanelMessage enum, add after `SlmRequest`:
//
//     /// Start a new agent run. The backend spawns an `AgentRunner` task.
//     AgentStart {
//         id:    RequestId,
//         goal:  String,
//         model: String,
//     },
//
//     /// Abort the currently running agent plan.
//     AgentAbort { plan_id: u64 },
//
//     /// Deliver a tool-execution result from the DevPanel JS bridge.
//     AgentToolResult {
//         plan_id: u64,
//         ok:      bool,
//         output:  String,
//         /// Optional base64 JPEG from a screenshot action.
//         #[serde(default, skip_serializing_if = "Option::is_none")]
//         image_b64: Option<String>,
//     },
//
// ─────────────────────────────────────────────────────────────────────────────
//
// Add these new data types at the bottom of protocol.rs:

use serde::{Deserialize, Serialize};

/// Payload for `BrowserMessage::AgentEvent`.
/// Mirrors `diatom_agent::runner::AgentEvent` but lives here so `diatom_bridge`
/// does not need to take a dependency on `diatom_agent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEventPayload {
    PlanReady  { plan_id: u64, steps: Vec<String> },
    ToolCall   { plan_id: u64, step_idx: usize, step_desc: String, call_json: String },
    StepDone   { plan_id: u64, step_idx: usize, output: String },
    Done       { plan_id: u64, summary: String },
    Failed     { plan_id: u64, reason: String },
    StepTimeout { plan_id: u64, step_idx: usize },
    Cancelled  { plan_id: u64 },
}
