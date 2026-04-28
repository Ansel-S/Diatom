//! `diatom_agent` — Native micro-agent capability for Diatom.
//!
//! ## Architecture
//!
//! ```text
//!  ┌──────────────────────────────────────────────────────────────────┐
//!  │  User (browser UI)                                               │
//!  │   └── invoke('cmd_agent_start', { goal })                        │
//!  └──────────────────────────────────┬───────────────────────────────┘
//!                                     │ Tauri IPC
//!  ┌──────────────────────────────────▼───────────────────────────────┐
//!  │  Tauri backend                                                   │
//!  │   └── AgentManager::start(goal)                                  │
//!  │         │                                                        │
//!  │         ├── planner::plan() → Vec<String>        ──SLM──► steps │
//!  │         │                                                        │
//!  │         └── for each step:                                       │
//!  │               executor::decide() ──SLM──► ToolCall JSON          │
//!  │               AgentIo::emit(AgentEvent::ToolCall)                │
//!  │               await JS result (cmd_agent_tool_result)            │
//!  └──────────────────────────────────────────────────────────────────┘
//!                                     │ Tauri event bus
//!  ┌──────────────────────────────────▼───────────────────────────────┐
//!  │  agent.js (browser webview)                                      │
//!  │   listen('agent-event') → dispatchToolCall(call)                 │
//!  │   → DOM click / type / navigate …                                │
//!  │   → invoke('cmd_agent_tool_result', { ok, output })              │
//!  └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Tauri integration (src-tauri/src/agent_commands.rs)
//!
//! ```rust
//! use diatom_agent::{AgentConfig, AgentRunner, AgentIo, AgentEvent};
//! use std::sync::{Arc, Mutex};
//!
//! // Keep the active runner so cmd_agent_abort can cancel it.
//! static RUNNER: Mutex<Option<AgentRunner>> = Mutex::new(None);
//!
//! #[tauri::command]
//! async fn cmd_agent_start(goal: String, model: String, app: AppHandle) -> u64 {
//!     let plan_id = next_plan_id();
//!     let io = Arc::new(TauriAgentIo::new(app));
//!     let runner = AgentRunner::start(
//!         AgentConfig { goal, model, plan_id, tool_timeout_secs: 15 },
//!         io,
//!     );
//!     *RUNNER.lock().unwrap() = Some(runner);
//!     plan_id
//! }
//!
//! #[tauri::command]
//! async fn cmd_agent_tool_result(ok: bool, output: String) -> bool {
//!     let guard = RUNNER.lock().unwrap();
//!     if let Some(runner) = guard.as_ref() {
//!         runner.result_tx.deliver(ToolResult { ok, output, image_b64: None }).await
//!     } else {
//!         false
//!     }
//! }
//! ```

pub mod executor;
pub mod planner;
pub mod runner;
pub mod tools;

// Re-export the most commonly used types.
pub use runner::{AgentConfig, AgentEvent, AgentIo, AgentRunner, ResultSender};
pub use tools::{ToolCall, ToolResult};
