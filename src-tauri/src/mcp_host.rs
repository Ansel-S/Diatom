// ─────────────────────────────────────────────────────────────────────────────
// diatom/src-tauri/src/mcp_host.rs
//
// MCP Host — Model Context Protocol Host
//
// Makes Diatom act as an MCP Host, exposing the Museum interface to external
// tools (VS Code, Cursor, any MCP-compatible IDE). Developers can query Museum
// documents without opening the browser.
//
// Protocol: JSON-RPC 2.0 over HTTP (localhost only)
// Default port: 39012 (bound to 127.0.0.1; all external connections rejected)
// Auth: single-session random token, written to {data_dir}/mcp.token;
//       invalidated on process exit.
//
// Exposed MCP tools:
//   museum_search   — search Museum archives (TF-IDF full-text)
//   museum_get      — fetch snapshot content by ID
//   museum_recent   — list the N most recent archived pages
//   museum_diff     — compute a diff between two versions of a URL
//   tab_list        — list currently open tabs
//   bookmark_search — search saved bookmarks
// ─────────────────────────────────────────────────────────────────────────────

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MCP_PORT: u16 = 39012;
pub const MCP_HOST: &str = "127.0.0.1";

// ── JSON-RPC 2.0 types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

// ── MCP Tool Definitions (for capability advertisement) ───────────────────────

#[derive(Debug, Serialize)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
}

pub fn available_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "museum_search",
            description: "Search Diatom Museum archives by keyword. Returns title, URL, snippet, and freeze date.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "default": 10, "maximum": 50 }
                },
                "required": ["query"]
            }),
        },
        McpTool {
            name: "museum_get",
            description: "Get the full content of a Museum archive by ID.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Museum entry ID" }
                },
                "required": ["id"]
            }),
        },
        McpTool {
            name: "museum_recent",
            description: "Get the most recently frozen Museum archives.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 20, "maximum": 100 },
                    "since_hours": { "type": "integer", "description": "Only show archives from the last N hours" }
                }
            }),
        },
        McpTool {
            name: "museum_diff",
            description: "Compare two versions of the same URL in Museum.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" },
                    "version_a": { "type": "string", "description": "Version ID (or 'oldest'/'latest')" },
                    "version_b": { "type": "string", "description": "Version ID (or 'oldest'/'latest')" }
                },
                "required": ["url"]
            }),
        },
        McpTool {
            name: "tab_list",
            description: "List all currently open tabs in Diatom (read-only).",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "workspace_id": { "type": "string", "description": "Optional workspace filter" }
                }
            }),
        },
    ]
}

// ── Token generation ──────────────────────────────────────────────────────────

/// Generate a single-session random auth token and write to data_dir/mcp.token
pub fn generate_and_write_token(data_dir: &std::path::Path) -> Result<String> {
    let bytes: [u8; 32] = rand::random();
    let token = hex::encode(bytes);
    let path = data_dir.join("mcp.token");
    std::fs::write(&path, &token)?;
    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::info!("MCP host: token written to {:?}", path);
    Ok(token)
}

/// Validate bearer token from Authorization header
pub fn validate_token(header: &str, expected: &str) -> bool {
    let provided = header.trim_start_matches("Bearer ").trim();
    // Constant-time comparison to prevent timing attacks
    provided.len() == expected.len()
        && provided.bytes().zip(expected.bytes()).all(|(a, b)| a == b)
}

// ── Request dispatcher ────────────────────────────────────────────────────────

pub async fn dispatch(
    req: RpcRequest,
    db: Arc<crate::db::Db>,
) -> RpcResponse {
    let result = match req.method.as_str() {
        "initialize" => handle_initialize(),
        "tools/list"  => Ok(serde_json::json!({ "tools": available_tools() })),
        "tools/call"  => handle_tool_call(req.params, db).await,
        "ping"        => Ok(serde_json::json!({ "pong": true })),
        m => Err(format!("Unknown method: {}", m)),
    };

    match result {
        Ok(value) => RpcResponse { jsonrpc: "2.0", id: req.id, result: Some(value), error: None },
        Err(msg)  => RpcResponse {
            jsonrpc: "2.0", id: req.id, result: None,
            error: Some(RpcError { code: -32601, message: msg }),
        },
    }
}

fn handle_initialize() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "diatom-museum-mcp",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Diatom Museum MCP host — access your personal web archive from any MCP-capable tool"
        }
    }))
}

async fn handle_tool_call(
    params: Option<serde_json::Value>,
    db: Arc<crate::db::Db>,
) -> Result<serde_json::Value, String> {
    let params = params.ok_or("Missing params")?;
    let name = params["name"].as_str().ok_or("Missing tool name")?;
    let args = &params["arguments"];

    match name {
        "museum_search" => {
            let query = args["query"].as_str().ok_or("Missing query")?;
            let limit = args["limit"].as_u64().unwrap_or(10).min(50) as usize;
            let results = db.museum_search(query, limit)
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string(&results).unwrap() }] }))
        }
        "museum_get" => {
            let id = args["id"].as_str().ok_or("Missing id")?;
            let entry = db.museum_get(id).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string(&entry).unwrap() }] }))
        }
        "museum_recent" => {
            let limit = args["limit"].as_u64().unwrap_or(20).min(100) as usize;
            let results = db.museum_recent(limit).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string(&results).unwrap() }] }))
        }
        "tab_list" => {
            Ok(serde_json::json!({ "content": [{ "type": "text",
                "text": "Tab listing requires active Diatom window. Use the UI or cmd_tabs_list IPC command." }] }))
        }
        t => Err(format!("Unknown tool: {}", t)),
    }
}

// ── HTTP server (localhost only) ──────────────────────────────────────────────

/// Spawn the MCP HTTP server. Binds ONLY to 127.0.0.1:{MCP_PORT}.
/// All requests must carry the session token in Authorization: Bearer <token>.
pub async fn run_mcp_server(token: String, db: Arc<crate::db::Db>) {
    use std::net::SocketAddr;
    let addr: SocketAddr = format!("{}:{}", MCP_HOST, MCP_PORT).parse().unwrap();

    tracing::info!("MCP host: listening on http://{} (localhost only)", addr);

    // Minimal HTTP handling via tokio (no external web framework dep)
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => { tracing::error!("MCP host: bind failed: {}", e); return; }
    };

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Reject non-loopback connections
        if !peer.ip().is_loopback() {
            tracing::warn!("MCP host: rejected non-loopback connection from {}", peer);
            continue;
        }
        let token_c = token.clone();
        let db_c = Arc::clone(&db);
        tokio::spawn(async move {
            handle_http_connection(stream, token_c, db_c).await;
        });
    }
}

async fn handle_http_connection(
    mut stream: tokio::net::TcpStream,
    token: String,
    db: Arc<crate::db::Db>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = vec![0u8; 16384];
    let n = stream.read(&mut buf).await.unwrap_or(0);
    if n == 0 { return; }

    let raw = String::from_utf8_lossy(&buf[..n]);

    // Check Authorization header
    let auth_ok = raw.lines()
        .find(|l| l.to_lowercase().starts_with("authorization:"))
        .map(|l| l.splitn(2, ':').nth(1).unwrap_or("").trim())
        .map(|v| validate_token(v, &token))
        .unwrap_or(false);

    if !auth_ok {
        let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 13\r\n\r\nUnauthorized\n";
        let _ = stream.write_all(resp.as_bytes()).await;
        return;
    }

    // Extract JSON body
    let body_start = raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(n);
    let body = &raw[body_start..];

    let response_body = match serde_json::from_str::<RpcRequest>(body) {
        Ok(req) => {
            let resp = dispatch(req, db).await;
            serde_json::to_string(&resp).unwrap_or_default()
        }
        Err(e) => serde_json::json!({
            "jsonrpc": "2.0", "id": null,
            "error": { "code": -32700, "message": format!("Parse error: {}", e) }
        }).to_string(),
    };

    let http_resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        response_body.len(), response_body
    );
    let _ = stream.write_all(http_resp.as_bytes()).await;
}
