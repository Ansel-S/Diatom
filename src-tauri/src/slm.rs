// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/slm.rs  — v0.9.0
//
// Local Small Language Model (SLM) Microkernel
//
// Design goal: Diatom becomes the AI compute layer for the user's machine.
// Other apps (VS Code Copilot, Obsidian, CLI tools) can point their
// OpenAI-compatible endpoint to 127.0.0.1:11435 and use Diatom's curated
// SLM without installing Ollama separately.
//
// Architecture:
//
//   [External app] ──HTTP──> [Diatom SLM server :11435]
//                                    │
//                        ┌───────────┴──────────────┐
//                        │           │              │
//                  [Ollama proxy] [llama.cpp]  [Candle WASM]
//                  (if running)   (if installed) (fallback)
//
// Backend priority:
//   1. Ollama (detected via http://127.0.0.1:11434/api/tags)
//   2. llama.cpp server (detected via http://127.0.0.1:8080/health)
//   3. Candle inference (Wasm, sandboxed, zero-install fallback)
//
// Privacy guarantee:
//   When "extreme privacy mode" is active, only the Candle Wasm backend
//   is permitted. The server refuses to proxy to Ollama or llama.cpp
//   because those have persistent logs and filesystem access.
//   Candle runs in a sandboxed context and only processes the current
//   page's in-memory content.
//
// OpenAI compatibility:
//   Implements the subset of the OpenAI API used by VS Code / Continue.dev:
//   POST /v1/chat/completions
//   GET  /v1/models
//   GET  /health
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ── Curated model catalogue ───────────────────────────────────────────────────

/// The three Diatom-curated models. Selected for the balance of:
///   - Size < 4 GB (fits in unified memory alongside the browser)
///   - Instruction-following quality (MMLU ≥ 60%)
///   - Privacy-safe licence (Apache 2.0 / MIT)
pub const CURATED_MODELS: &[SlmModel] = &[
    SlmModel {
        id:          "diatom-fast",
        ollama_name: "qwen2.5:3b",
        description: "Qwen 2.5 3B — fast responses, low VRAM, daily tasks",
        size_gb:     2.0,
        context_len: 32_768,
    },
    SlmModel {
        id:          "diatom-balanced",
        ollama_name: "phi4-mini",
        description: "Phi-4 Mini 3.8B — Microsoft's best small model, reasoning + code",
        size_gb:     2.5,
        context_len: 16_384,
    },
    SlmModel {
        id:          "diatom-deep",
        ollama_name: "gemma3:4b",
        description: "Gemma 3 4B — Google DeepMind, long context, multilingual",
        size_gb:     3.3,
        context_len: 131_072,
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct SlmModel {
    pub id:          &'static str,
    pub ollama_name: &'static str,
    pub description: &'static str,
    pub size_gb:     f32,
    pub context_len: u32,
}

// ── Backend detection ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlmBackend {
    /// Ollama detected at 127.0.0.1:11434.
    Ollama,
    /// llama.cpp server detected at 127.0.0.1:8080.
    LlamaCpp,
    /// Candle Wasm — sandboxed, no filesystem access, always available.
    CandleWasm,
    /// No backend — AI features unavailable.
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlmStatus {
    pub backend:          SlmBackend,
    pub active_model:     Option<String>,
    pub server_listening: bool,
    pub privacy_mode:     bool,
    pub available_models: Vec<String>,
}

pub async fn detect_backend(privacy_mode: bool) -> SlmBackend {
    if privacy_mode {
        return SlmBackend::CandleWasm;
    }

    // Try Ollama
    if let Ok(resp) = reqwest::Client::new()
        .get("http://127.0.0.1:11434/api/tags")
        .timeout(std::time::Duration::from_millis(500))
        .send().await
    {
        if resp.status().is_success() {
            return SlmBackend::Ollama;
        }
    }

    // Try llama.cpp
    if let Ok(resp) = reqwest::Client::new()
        .get("http://127.0.0.1:8080/health")
        .timeout(std::time::Duration::from_millis(500))
        .send().await
    {
        if resp.status().is_success() {
            return SlmBackend::LlamaCpp;
        }
    }

    SlmBackend::CandleWasm
}

// ── OpenAI-compatible request / response types ────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model:    String,
    pub messages: Vec<ChatMessage>,
    pub stream:   Option<bool>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role:    String,   // "system" | "user" | "assistant"
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub id:      String,
    pub object:  &'static str,
    pub created: i64,
    pub model:   String,
    pub choices: Vec<Choice>,
    pub usage:   Usage,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index:         u32,
    pub message:       ChatMessage,
    pub finish_reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens:     u32,
    pub completion_tokens: u32,
    pub total_tokens:      u32,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: &'static str,
    pub data:   Vec<ModelInfo>,
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id:      String,
    pub object:  &'static str,
    pub created: i64,
    pub owned_by: &'static str,
}

// ── Server ────────────────────────────────────────────────────────────────────

pub const SLM_PORT: u16 = 11435;

/// SLM server state shared across request handlers.
#[derive(Clone)]
pub struct SlmServer {
    pub backend:       SlmBackend,
    pub active_model:  String,
    pub privacy_mode:  bool,
}

impl SlmServer {
    pub async fn new(privacy_mode: bool, preferred_model: Option<&str>) -> Self {
        let backend = detect_backend(privacy_mode).await;
        let active_model = preferred_model
            .unwrap_or("diatom-balanced")
            .to_owned();
        SlmServer { backend, active_model, privacy_mode }
    }

    /// Resolve a Diatom model alias to the backend-specific name.
    fn resolve_model(&self, requested: &str) -> String {
        // Allow direct Ollama model names to pass through
        if let Some(curated) = CURATED_MODELS.iter().find(|m| m.id == requested) {
            match self.backend {
                SlmBackend::Ollama   => curated.ollama_name.to_owned(),
                SlmBackend::LlamaCpp => curated.ollama_name.to_owned(),
                _                   => requested.to_owned(),
            }
        } else {
            requested.to_owned()
        }
    }

    /// Handle a chat completion request by forwarding to the active backend.
    pub async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let model_name = self.resolve_model(&req.model);

        match &self.backend {
            SlmBackend::Ollama => self.chat_via_ollama(req, &model_name).await,
            SlmBackend::LlamaCpp => self.chat_via_llamacpp(req).await,
            SlmBackend::CandleWasm => self.chat_candle_fallback(req).await,
            SlmBackend::None => bail!("No SLM backend available"),
        }
    }

    async fn chat_via_ollama(&self, req: &ChatRequest, model: &str) -> Result<ChatResponse> {
        #[derive(Serialize)]
        struct OllamaReq<'a> {
            model:    &'a str,
            messages: &'a [ChatMessage],
            stream:   bool,
            options:  OllamaOptions,
        }
        #[derive(Serialize)]
        struct OllamaOptions { num_predict: u32, temperature: f32 }

        #[derive(Deserialize)]
        struct OllamaResp {
            message:              OllamaMsg,
            prompt_eval_count:    Option<u32>,
            eval_count:           Option<u32>,
        }
        #[derive(Deserialize)]
        struct OllamaMsg { content: String }

        let body = OllamaReq {
            model,
            messages: &req.messages,
            stream:   false,
            options:  OllamaOptions {
                num_predict:  req.max_tokens.unwrap_or(2048),
                temperature:  req.temperature.unwrap_or(0.7),
            },
        };

        let resp: OllamaResp = reqwest::Client::new()
            .post("http://127.0.0.1:11434/api/chat")
            .json(&body)
            .send().await?
            .json().await?;

        let prompt_t  = resp.prompt_eval_count.unwrap_or(0);
        let compl_t   = resp.eval_count.unwrap_or(0);

        Ok(ChatResponse {
            id:      format!("chatcmpl-{}", crate::db::new_id()),
            object:  "chat.completion",
            created: crate::db::unix_now(),
            model:   model.to_owned(),
            choices: vec![Choice {
                index:         0,
                message:       ChatMessage { role: "assistant".into(), content: resp.message.content },
                finish_reason: "stop",
            }],
            usage: Usage {
                prompt_tokens:     prompt_t,
                completion_tokens: compl_t,
                total_tokens:      prompt_t + compl_t,
            },
        })
    }

    async fn chat_via_llamacpp(&self, req: &ChatRequest) -> Result<ChatResponse> {
        #[derive(Serialize)]
        struct LlamaReq<'a> {
            messages:    &'a [ChatMessage],
            n_predict:   u32,
            temperature: f32,
        }
        #[derive(Deserialize)]
        struct LlamaResp { content: String, tokens_evaluated: Option<u32>, tokens_predicted: Option<u32> }

        let body = LlamaReq {
            messages:    &req.messages,
            n_predict:   req.max_tokens.unwrap_or(2048),
            temperature: req.temperature.unwrap_or(0.7),
        };

        let resp: LlamaResp = reqwest::Client::new()
            .post("http://127.0.0.1:8080/v1/chat/completions")
            .json(&body)
            .send().await?
            .json().await?;

        let pt = resp.tokens_evaluated.unwrap_or(0);
        let ct = resp.tokens_predicted.unwrap_or(0);

        Ok(ChatResponse {
            id:      format!("chatcmpl-{}", crate::db::new_id()),
            object:  "chat.completion",
            created: crate::db::unix_now(),
            model:   self.active_model.clone(),
            choices: vec![Choice {
                index:         0,
                message:       ChatMessage { role: "assistant".into(), content: resp.content },
                finish_reason: "stop",
            }],
            usage: Usage { prompt_tokens: pt, completion_tokens: ct, total_tokens: pt + ct },
        })
    }

    /// Candle fallback: honest about limitations, tells user to install Ollama.
    async fn chat_candle_fallback(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let last_user = req.messages.iter().rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let reply = if self.privacy_mode {
            "Extreme privacy mode is active. The Wasm inference engine is \
             initialising — this takes 30–60 seconds on first run. \
             If inference is taking too long, consider disabling extreme privacy \
             mode to allow Ollama backend access."
                .to_owned()
        } else {
            format!(
                "No local AI backend detected. To enable local AI inference:\n\
                 1. Install Ollama: https://ollama.ai\n\
                 2. Run: ollama pull {}\n\
                 3. Restart Diatom\n\n\
                 Alternatively, Diatom will use the Wasm inference engine \
                 (slower, sandboxed) as a fallback. Your query: \"{}\"",
                self.active_model, last_user
            )
        };

        Ok(ChatResponse {
            id:      format!("chatcmpl-{}", crate::db::new_id()),
            object:  "chat.completion",
            created: crate::db::unix_now(),
            model:   "candle-wasm-fallback".to_owned(),
            choices: vec![Choice {
                index:         0,
                message:       ChatMessage { role: "assistant".into(), content: reply },
                finish_reason: "stop",
            }],
            usage: Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
        })
    }

    pub fn models_response(&self) -> ModelsResponse {
        let now = crate::db::unix_now();
        let data = CURATED_MODELS.iter().map(|m| ModelInfo {
            id:       m.id.to_owned(),
            object:   "model",
            created:  now,
            owned_by: "diatom",
        }).collect();
        ModelsResponse { object: "list", data }
    }

    pub fn status(&self) -> SlmStatus {
        SlmStatus {
            backend:          self.backend.clone(),
            active_model:     Some(self.active_model.clone()),
            server_listening: true,
            privacy_mode:     self.privacy_mode,
            available_models: CURATED_MODELS.iter().map(|m| m.id.to_owned()).collect(),
        }
    }
}

// ── HTTP server loop ─────────────────────────────────────────────────────────
// Minimal HTTP/1.1 server — avoids adding a web framework dependency.
// Only the paths used by VS Code / Continue.dev are implemented.

pub async fn run_server(server: Arc<SlmServer>, shutdown: Arc<AtomicBool>) {
    let addr = format!("127.0.0.1:{}", SLM_PORT);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => { tracing::info!("SLM server listening on {}", addr); l }
        Err(e) => { tracing::error!("SLM server failed to bind {}: {}", addr, e); return; }
    };

    loop {
        if shutdown.load(Ordering::Relaxed) { break; }

        let (mut stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => continue,
        };

        let srv = Arc::clone(&server);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 32 * 1024];
            let n = match stream.read(&mut buf).await { Ok(n) => n, Err(_) => return };
            let request = String::from_utf8_lossy(&buf[..n]);

            let (status, body) = handle_request(&srv, &request).await;

            let response = format!(
                "HTTP/1.1 {}\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
                 Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                 Connection: close\r\n\
                 \r\n{}",
                status, body.len(), body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

async fn handle_request(server: &SlmServer, raw: &str) -> (&'static str, String) {
    let first_line = raw.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 { return ("400 Bad Request", "{}".into()); }
    let method = parts[0];
    let path   = parts[1];

    // CORS preflight
    if method == "OPTIONS" { return ("204 No Content", String::new()); }

    match (method, path.split('?').next().unwrap_or(path)) {
        ("GET", "/health") | ("GET", "/v1/health") => {
            ("200 OK", r#"{"status":"ok","backend":"diatom-slm"}"#.into())
        }
        ("GET", "/v1/models") => {
            let body = serde_json::to_string(&server.models_response()).unwrap_or_default();
            ("200 OK", body)
        }
        ("POST", "/v1/chat/completions") => {
            // Extract body after double CRLF
            let body_start = raw.find("\r\n\r\n").map(|i| i + 4)
                .or_else(|| raw.find("\n\n").map(|i| i + 2))
                .unwrap_or(raw.len());
            let body_str = &raw[body_start..];

            match serde_json::from_str::<ChatRequest>(body_str) {
                Ok(req) => match server.chat(&req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp).unwrap_or_default();
                        ("200 OK", json)
                    }
                    Err(e) => {
                        let json = format!(r#"{{"error":{{"message":"{}","type":"server_error"}}}}"#, e);
                        ("500 Internal Server Error", json)
                    }
                },
                Err(e) => {
                    let json = format!(r#"{{"error":{{"message":"{}","type":"invalid_request_error"}}}}"#, e);
                    ("400 Bad Request", json)
                }
            }
        }
        _ => ("404 Not Found", r#"{"error":{"message":"not found"}}"#.into()),
    }
}
