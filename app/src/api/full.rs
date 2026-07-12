//! Lingshu HTTP API — Phase 7: HTTP Gateway
//!
//! Endpoints:
//!   GET  /health                 — Health check
//!   GET  /metrics                — Prometheus metrics
//!   GET  /version                — Version info
//!   GET  /v1/models              — List models
//!   POST /v1/chat/completions    — OpenAI compatible chat
//!   POST /v1/embeddings          — OpenAI compatible embeddings
//!   POST /v1/chat                — Internal chat
//!   POST /v1/embed               — Internal embed
//!   POST /v1/agent/run           — Run agent
//!   GET  /ws                     — WebSocket streaming

use lingshu_traits::EventBus;
use std::sync::Arc;

use axum::{
    extract::{ws, Path, Query, State, WebSocketUpgrade},
    http::{header, Method, StatusCode},
    response::Redirect,
    response::{Html, Json},
    routing::{delete, get, post},
    Router,
};
use lingshu_evaluator;
use lingshu_tenant;
use lingshu_federation;
use lingshu_watch_plugin;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use axum::response::{
    sse::{Event, Sse},
    IntoResponse,
};
use futures::stream::Stream;
use futures::stream::StreamExt;
use lingshu_audit::{AuditEntry, AuditEventType, AuditLogStore};
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_observability::health::HealthRegistry;
use lingshu_plugin::{
    hot_reload::HotReloadWatcher,
    market::{InstallOptions, MarketPluginEntry, PluginMarket, RegistrySource},
    PluginRegistry,
};
use lingshu_traits::llm::{LlmMessage, LlmRequest, LlmRole};
use lingshu_websocket::{
    ClientMessage, Connection, ConnectionManager, ConnectionState, SseBroadcaster, SseEvent,
};
use std::convert::Infallible;
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream;

use crate::LingshuRuntime;
use lingshu_channel::InboundEvent;

// ── Shared State ────────────────────────────────────

pub struct AppState {
    pub tenant_manager: std::sync::Arc<lingshu_tenant::TenantManager>,
    pub vault_client: std::sync::Arc<dyn lingshu_vault::VaultClientTrait>,
    pub tee_system: std::sync::Arc<lingshu_tee::TeeSystem>,
    pub runtime: Arc<LingshuRuntime>,
    pub plugin_event_bus: Arc<lingshu_plugin::event::EventBus>,
    pub plugin_registry: Arc<PluginRegistry>,
    pub plugin_market: tokio::sync::RwLock<PluginMarket>,
    pub hot_reload_watcher: HotReloadWatcher,
    /// BeEF 安全测试插件管理器.
    pub beef_manager: Arc<tokio::sync::RwLock<Option<Arc<lingshu_beef_plugin::BeefManager>>>>,
    pub health_registry: Arc<HealthRegistry>,
    /// Watch Skill 视频分析插件管理器.
    pub watch_manager: Arc<tokio::sync::RwLock<Option<Arc<lingshu_watch_plugin::WatchManager>>>>,
    pub ws_manager: Arc<ConnectionManager>,
    pub sse_broadcaster: Arc<SseBroadcaster>,
    /// 文件存储 (多模态上传)
    pub file_store: Arc<tokio::sync::RwLock<Vec<FileRecord>>>,
    /// 凭证管理 (多 Git 提供商)
    pub credential_manager: std::sync::Arc<lingshu_credentials::CredentialManager>,
    /// JWT 认证服务 (用于 Admin / WebUI 面板)
    pub jwt_service: lingshu_security::auth::JwtService,
}

// ── Response Types ──────────────────────────────────

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime: String,
    checks: Vec<HealthCheckItem>,
}

#[derive(Serialize)]
struct HealthCheckItem {
    name: String,
    healthy: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct MetricsJsonResponse {
    cpu_usage: f64,
    memory_mb: f64,
    active_sessions: u64,
    total_agents: u64,
    llm_requests_total: u64,
    llm_tokens_total: u64,
    federation_nodes: u64,
    uptime_secs: u64,
    custom_metrics: Vec<MetricSample>,
}

#[derive(Debug, Clone, Serialize)]
struct MetricSample {
    name: String,
    value: f64,
    labels: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
struct VersionResponse {
    version: String,
    build: String,
    rustc: String,
}

#[derive(Serialize)]
struct ModelInfo {
    id: String,
    object: String,
    created: u64,
    owned_by: String,
}

#[derive(Deserialize)]
struct ChatCompletionRequest {
    session_id: Option<String>,
    model: String,
    messages: Vec<ChatMsg>,
    stream: Option<bool>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    user: Option<String>,
    tools: Option<Vec<lingshu_traits::llm::ToolDefinition>>,
}

#[derive(Deserialize)]
struct ChatMsg {
    role: String,
    content: String,
    name: Option<String>,
}

#[derive(Serialize)]
struct ChatCompletionResp {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: UsageInfo,
}

#[derive(Serialize)]
struct Choice {
    index: u32,
    message: RespMsg,
    finish_reason: String,
}

#[derive(Serialize)]
struct RespMsg {
    role: String,
    content: String,
}

/// 文件上传记录.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub file_type: String,
    pub data: String, // Base64 encoded
    pub analysis: Option<serde_json::Value>,
    pub created_at: String,
}

/// 文件上传请求.
#[derive(Deserialize)]
struct FileUploadRequest {
    pub name: String,
    /// Base64 编码的文件数据
    pub data: String,
    /// 可选，覆盖自动检测的 MIME 类型
    pub mime_type: Option<String>,
}

/// 文件分析请求.
#[derive(Deserialize)]
struct FileAnalyzeRequest {
    pub file_id: String,
}

/// 文件列表响应.
#[derive(Serialize)]
struct FileListResponse {
    pub files: Vec<FileRecord>,
    pub total: usize,
}

/// 多模态聊天请求 (支持图像输入).
#[derive(Deserialize)]
struct MultimodalChatRequest {
    pub prompt: String,
    pub file_ids: Vec<String>,
    pub session_id: Option<String>,
    pub model: Option<String>,
}

#[derive(Serialize)]
struct UsageInfo {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Serialize)]
pub struct ChatResp {
    session_id: String,
    message: String,
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
pub struct ChatReq {
    prompt: String,
    session_id: Option<String>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct EmbedReq {
    model: Option<String>,
    input: Vec<String>,
}

#[derive(Serialize)]
pub struct EmbedResp {
    object: String,
    data: Vec<EmbedItem>,
    model: String,
    usage: UsageInfo,
}

#[derive(Serialize)]
struct EmbedItem {
    object: String,
    index: usize,
    embedding: Vec<f64>,
}

#[derive(Deserialize)]
pub struct AgentRunReq {
    session_id: Option<String>,
    #[allow(dead_code)]
    agent_id: Option<String>,
    input: Value,
}

#[derive(Serialize)]
pub struct AgentRunResp {
    agent_id: String,
    status: String,
    output: Value,
}

#[derive(Deserialize)]
struct EmbedInput {
    _text: String,
}

#[derive(Serialize)]
struct EmbedOutput {
    embedding: Vec<f64>,
    dimensions: usize,
}

// ── Router ──────────────────────────────────────────

// ── API Documentation ─────────────────────────────────

/// Render HTML API documentation page.
async fn docs_handler() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lingshu API Documentation</title>
  <style>
    body { font-family: -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif; max-width: 900px; margin: 0 auto; padding: 2rem; background: #0d1117; color: #c9d1d9; }
    h1 { color: #58a6ff; border-bottom: 1px solid #30363d; padding-bottom: 0.5rem; }
    h2 { color: #79c0ff; margin-top: 2rem; }
    h3 { color: #c9d1d9; }
    code { background: #161b22; padding: 0.2em 0.4em; border-radius: 4px; font-size: 0.9em; }
    pre { background: #161b22; padding: 1rem; border-radius: 6px; overflow-x: auto; }
    table { width: 100%; border-collapse: collapse; margin: 1rem 0; }
    th, td { text-align: left; padding: 0.5rem; border-bottom: 1px solid #30363d; }
    th { color: #58a6ff; }
    .method { display: inline-block; padding: 2px 6px; border-radius: 4px; font-weight: bold; font-size: 0.8em; }
    .get { background: #1f6feb33; color: #58a6ff; }
    .post { background: #23863633; color: #3fb950; }
    .tag { background: #30363d; color: #8b949e; padding: 2px 6px; border-radius: 4px; font-size: 0.8em; }
    a { color: #58a6ff; }
  </style>
</head>
<body>
  <h1>📖 Lingshu API Reference</h1>
  <p>Base URL: <code>http://&lt;host&gt;:8080</code></p>
  <p>See <a href="/docs/openapi.json">OpenAPI JSON</a> for machine-readable spec.</p>

  <h2>Core</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/health</code></td><td>Health check</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/metrics</code></td><td>Prometheus metrics</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/version</code></td><td>Version info</td></tr>
  </table>

  <h2>Chat</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/models</code></td><td>List available models</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/chat/completions</code></td><td>OpenAI-compatible chat completion</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/chat</code></td><td>Internal chat</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v2/chat/stream</code></td><td>Streaming chat</td></tr>
  </table>

  <h2>Agent</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/agent/run</code></td><td>Run an agent</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/agents</code></td><td>List agents</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/agents/:id</code></td><td>Get agent status</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/agents/:id/pause</code></td><td>Pause agent</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/agents/:id/resume</code></td><td>Resume agent</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/agents/:id/cancel</code></td><td>Cancel agent</td></tr>
  </table>

  <h2>Embeddings</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/embeddings</code></td><td>OpenAI-compatible embeddings</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/embed</code></td><td>Internal embed</td></tr>
  </table>

  <h2>WebSocket & SSE</h2>
  <table>
    <tr><th>Path</th><th>Description</th></tr>
    <tr><td><code>/ws</code></td><td>WebSocket connection</td></tr>
    <tr><td><code>/v2/ws</code></td><td>V2 WebSocket</td></tr>
    <tr><td><code>/v2/events</code></td><td>SSE events</td></tr>
  </table>

  <h2>MCP</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/mcp</code></td><td>MCP protocol endpoint</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/mcp/tools</code></td><td>List MCP tools</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/mcp/ui</code></td><td>MCP admin UI</td></tr>
  </table>

  <h2>Files (多模态)</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/files/upload</code></td><td>Upload file</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/files/analyze</code></td><td>Analyze file</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/files</code></td><td>List files</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/files/:id</code></td><td>Get file</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/chat/multimodal</code></td><td>Multimodal chat</td></tr>
  </table>

  <h2>Plugins</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/plugins</code></td><td>List plugins</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/plugins</code></td><td>Install plugin</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/plugins/:id</code></td><td>Get plugin</td></tr>
    <tr><td><span class="method delete">DELETE</span></td><td><code>/v1/plugins/:id</code></td><td>Uninstall plugin</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/plugins/:id/start</code></td><td>Start plugin</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/plugins/:id/stop</code></td><td>Stop plugin</td></tr>
  </table>

  <h2>Knowledge Graph</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/graph/:project</code></td><td>Query graph</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/graph/:project</code></td><td>Analyze graph</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/graph/:project/view</code></td><td>View graph</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/projects</code></td><td>List projects</td></tr>
  </table>

  <h2>Credentials</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/credentials</code></td><td>List credentials</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/credentials</code></td><td>Create credential</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/credentials/:id</code></td><td>Get credential</td></tr>
    <tr><td><span class="method put">PUT</span></td><td><code>/v1/credentials/:id</code></td><td>Update credential</td></tr>
    <tr><td><span class="method delete">DELETE</span></td><td><code>/v1/credentials/:id</code></td><td>Delete credential</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/credentials/:id/token</code></td><td>Get credential token</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/credentials/:id/validate</code></td><td>Validate credential</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/credentials/providers</code></td><td>List providers</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/credentials/ui</code></td><td>Credentials UI</td></tr>
  </table>

  <h2>Evaluator (ES)</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/eval/run</code></td><td>Run evaluation</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/eval/result</code></td><td>Get evaluation result</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/eval/regression</code></td><td>Regression analysis</td></tr>
  </table>

  <h2>Federation (Fed)</h2>
  <table>
    <tr><th>Method</th><th>Path</th><th>Description</th></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/federation/status</code></td><td>Federation status</td></tr>
    <tr><td><span class="method get">GET</span></td><td><code>/v1/federation/nodes</code></td><td>List federated nodes</td></tr>
    <tr><td><span class="method post">POST</span></td><td><code>/v1/federation/execute</code></td><td>Remote execution</td></tr>
  </table>
</body>
</html>"#,
    )
}

/// Return OpenAPI 3.0 JSON spec.
pub async fn openapi_json_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"openapi":"3.0.0","info":{"title":"Lingshu API","version":"3.4.0"}}))
}

/// Swagger UI — interactive API documentation
async fn swagger_ui_handler() -> Html<&'static str> {
    Html(r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <title>Lingshu API — Swagger UI</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css">
  <style>body { margin: 0; background: #0d1117; }</style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: "/docs/openapi.json",
      dom_id: "#swagger-ui",
      deepLinking: true,
      presets: [SwaggerUIBundle.presets.apis],
    });
  </script>
</body>
</html>"##)
}
/// Server-rendered Web Console (v4.3 Enterprise).
/// 完整的 AJAX 管理面板，无需 wasm 编译。
async fn admin_handler() -> Html<String> {
    Html(r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lingshu Web Console</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: #0d1117; color: #c9d1d9; font-size: 14px; }
    #app { display: flex; min-height: 100vh; }
    .sidebar { width: 220px; background: #161b22; border-right: 1px solid #30363d; padding: 1rem 0; flex-shrink: 0; overflow-y: auto; }
    .sidebar .logo { padding: 0 1.2rem 0.8rem; border-bottom: 1px solid #30363d; margin-bottom: 0.3rem; font-size: 1.1rem; font-weight: 700; color: #58a6ff; display: flex; align-items: center; gap: 0.4rem; }
    .sidebar .logo small { font-size: 0.65rem; color: #6e7681; font-weight: 400; margin-left: auto; }
    .sidebar nav a { display: flex; align-items: center; gap: 0.4rem; padding: 0.5rem 1.2rem; color: #8b949e; text-decoration: none; font-size: 0.85rem; transition: background 0.12s; border-left: 2px solid transparent; }
    .sidebar nav a:hover { background: #1c2128; color: #c9d1d9; }
    .sidebar nav a.active { background: #1c2128; color: #e6edf3; border-left-color: #58a6ff; font-weight: 600; }
    .sidebar .divider { border-top: 1px solid #21262d; margin: 0.5rem 1.2rem; }
    .main { flex: 1; padding: 1.5rem 2rem; overflow-y: auto; max-height: 100vh; }
    h1 { font-size: 1.4rem; margin-bottom: 1.2rem; color: #e6edf3; font-weight: 600; }
    h2 { font-size: 1.1rem; margin: 1.5rem 0 0.8rem; color: #c9d1d9; }
    .cards { display: flex; gap: 0.8rem; flex-wrap: wrap; margin-bottom: 1rem; }
    .card { background: #161b22; border-radius: 8px; padding: 0.8rem 1rem; min-width: 140px; flex: 1; border: 1px solid #30363d; }
    .card .lbl { font-size: 0.7rem; color: #8b949e; text-transform: uppercase; letter-spacing: 0.04em; }
    .card .val { font-size: 1.3rem; font-weight: 700; margin-top: 0.2rem; }
    .card .sub { font-size: 0.78rem; color: #6e7681; margin-top: 0.15rem; }
    .section { margin: 1.2rem 0; }
    table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
    th, td { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid #21262d; }
    th { color: #58a6ff; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.03em; white-space: nowrap; }
    td { color: #c9d1d9; }
    tr:hover td { background: #161b22; }
    code { background: #1c2128; padding: 0.15em 0.4em; border-radius: 3px; font-size: 0.85em; }
    .badge { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 0.75rem; font-weight: 500; }
    .badge.ok, .badge.active, .badge.online, .badge.healthy, .badge.running { background: #23863633; color: #3fb950; }
    .badge.warn, .badge.paused { background: #d2992233; color: #d29922; }
    .badge.err, .badge.offline, .badge.error { background: #f8514933; color: #f85149; }
    .badge.info { background: #1f6feb33; color: #58a6ff; }
    .error-box { background: #f8514922; color: #f85149; padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 0.8rem; border: 1px solid #f8514933; }
    .info-box { background: #1f6feb22; color: #58a6ff; padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 0.8rem; border: 1px solid #1f6feb33; }
    #loading { text-align: center; padding: 3rem; color: #6e7681; font-size: 0.9rem; }
    .actions { display: flex; gap: 0.4rem; }
    .btn { display: inline-flex; align-items: center; gap: 0.3rem; padding: 0.25rem 0.6rem; border-radius: 4px; font-size: 0.78rem; cursor: pointer; border: 1px solid #30363d; background: #21262d; color: #c9d1d9; transition: border-color 0.12s; }
    .btn:hover { border-color: #58a6ff; }
    .btn-danger { border-color: #f8514933; color: #f85149; }
    .btn-danger:hover { border-color: #f85149; }
    .btn-primary { background: #238636; border-color: #2ea043; color: #fff; }
    .btn-primary:hover { background: #2ea043; }
    .filter-bar { display: flex; gap: 0.5rem; margin-bottom: 0.8rem; flex-wrap: wrap; align-items: center; }
    .filter-bar input, .filter-bar select { background: #0d1117; border: 1px solid #30363d; border-radius: 4px; color: #c9d1d9; padding: 0.3rem 0.6rem; font-size: 0.83rem; }
    .filter-bar input:focus { border-color: #58a6ff; outline: none; }
    pre.dump { background: #161b22; padding: 0.8rem; border-radius: 6px; font-size: 0.78rem; overflow-x: auto; color: #8b949e; }
    .tt { color: #8b949e; font-family: monospace; font-size: 0.8rem; }
    .empty { text-align: center; padding: 3rem; color: #6e7681; }
    .remote { color: #58a6ff; }
  </style>
</head>
<body>
  <div id="app">
    <div class="sidebar">
      <div class="logo">⚡ Lingshu<small>v4.3</small></div>
      <nav>
        <a href="#" class="active" data-page="dashboard">📊 Dashboard</a>
        <a href="#" data-page="agents">🤖 Agents</a>
        <a href="#" data-page="plugins">🧩 Plugins</a>
        <a href="#" data-page="billing">💰 Billing</a>
        <div class="divider"></div>
        <a href="#" data-page="discovery">🔍 MCP Discovery</a>
        <a href="#" data-page="tenants">🏢 Tenants</a>
        <a href="#" data-page="audit">📋 Audit</a>
        <div class="divider"></div>
        <a href="#" data-page="federation">🌐 Federation</a>
        <a href="#" data-page="eval">📋 Eval</a>
        <a href="/docs">📖 API Docs</a>
      </nav>
    </div>
    <div class="main" id="main-content">
      <div id="loading"><p>⏳ Loading...</p></div>
    </div>
  </div>

  <script>
    (function() {
      const main = document.getElementById('main-content');
      const navLinks = document.querySelectorAll('.sidebar nav a[data-page]');

      async function loadPage(page) {
        history.replaceState(null, '', '/admin?page=' + page);
        navLinks.forEach(a => { a.classList.toggle('active', a.dataset.page === page); });
        main.innerHTML = '<div id="loading"><p>⏳ Loading...</p></div>';
        try {
          if (page === 'dashboard') await renderDashboard();
          else if (page === 'agents') await renderAgents();
          else if (page === 'plugins') await renderPlugins();
          else if (page === 'billing') await renderBilling();
          else if (page === 'discovery') await renderDiscovery();
          else if (page === 'tenants') await renderTenants();
          else if (page === 'audit') await renderAudit();
          else if (page === 'federation') await renderFederation();
          else if (page === 'eval') await renderEval();
          else await renderDashboard();
        } catch(e) {
          main.innerHTML = '<div class="error-box">⚠ Error: ' + e.message + '</div>';
        }
      }

      async function api(path) {
        const r = await fetch(path);
        if (!r.ok) { let txt; try { txt = await r.text(); } catch(e) { txt = r.status; } throw new Error(r.status + ' ' + txt.substring(0,80)); }
        return r.json();
      }

      function card(label, value, opts) {
        opts = opts || {};
        const cls = opts.cls || 'ok';
        return '<div class="card"><div class="lbl">' + label + '</div><div class="val">' + value + '</div>' +
          (opts.sub ? '<div class="sub">' + opts.sub + '</div>' : '') + '</div>';
      }

      function badge(text) {
        if (!text) return '<span class="badge">—</span>';
        const cls = text.toString().toLowerCase();
        return '<span class="badge ' + cls + '">' + text + '</span>';
      }

      function esc(s) {
        return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
      }

      function fmtTime(t) {
        if (!t) return '—';
        return t.substring(0,19).replace('T',' ');
      }

      // ── Dashboard ──────────────────────────────────
      async function renderDashboard() {
        let [health, ver, agents, billing] = await Promise.all([
          api('/health').catch(() => null),
          api('/version').catch(() => null),
          api('/v1/agents').catch(() => []),
          api('/v1/billing/stats').catch(() => null),
        ]);
        let status = health && health.status === 'ok' ? 'ok' : health ? 'warn' : 'err';
        let st = health ? health.status || 'unknown' : 'unreachable';
        let agentCount = Array.isArray(agents) ? agents.length : 0;
        let totalTokens = billing ? (billing.total_tokens || 0).toLocaleString() : '—';
        let html = '<h1>📊 Dashboard</h1><div class="cards">' +
          card('System', badge(st), { cls: status, sub: health ? 'up ' + (health.uptime_secs || health.uptime || '?') + 's' : '' }) +
          card('Version', ver ? ver.version : '—') +
          card('Agents', agentCount) +
          card('Total Tokens', totalTokens) +
        '</div>';
        html += '<div class="section"><h2>🔌 Services</h2><table><thead><tr><th>Service</th><th>Status</th><th>Detail</th></tr></thead><tbody>';
        if (health && health.services) {
          for (const [svc, st] of Object.entries(health.services)) {
            let s = typeof st === 'object' ? (st.status || st.ok || 'unknown') : st;
            html += '<tr><td>' + esc(svc) + '</td><td>' + badge(s) + '</td><td class="tt">' + esc(JSON.stringify(st).substring(0,100)) + '</td></tr>';
          }
        } else {
          html += '<tr><td colspan="3" class="empty" style="padding:1rem">No service details</td></tr>';
        }
        html += '</tbody></table></div>';
        main.innerHTML = html;
      }

      // ── Agents ─────────────────────────────────────
      async function renderAgents() {
        let agents = await api('/v1/agents').catch(() => []);
        let html = '<h1>🤖 Agents</h1>';
        let list = Array.isArray(agents) ? agents : (agents.agents || agents.data || []);
        if (list.length === 0) {
          html += '<div class="empty"><p>No agents running.</p></div>';
        } else {
          html += '<table><thead><tr><th>ID</th><th>Status</th><th>Model</th><th>Task</th><th>Created</th><th>Actions</th></tr></thead><tbody>';
          for (const a of list) {
            let aid = a.agent_id || a.id || '?';
            let st = a.status || 'unknown';
            html += '<tr><td><code>' + esc(aid.substring(0,12)) + '</code></td><td>' + badge(st) +
              '</td><td>' + esc(a.model || '—') + '</td><td>' + esc((a.task || a.description || '').substring(0,40)) +
              '</td><td class="tt">' + fmtTime(a.created_at || a.created) +
              '</td><td class="actions">' +
              '<button class="btn" onclick="action('restart','' + aid + '')">↻</button>' +
              '<button class="btn" onclick="action('pause','' + aid + '')">⏸</button>' +
              '<button class="btn btn-danger" onclick="action('delete','' + aid + '')">✕</button>' +
              '</td></tr>';
          }
          html += '</tbody></table>';
        }
        html += '<div class="section"><h2>🚀 Run Agent</h2>' +
          '<div class="filter-bar"><input id="agent-task" placeholder="Task description..." style="width:300px">' +
          '<button class="btn btn-primary" onclick="runAgent()">▶ Run</button></div></div>';
        main.innerHTML = html;
      }

      window.action = async function(cmd, id) {
        try {
          const endpoints = { restart: 'POST /v1/agent/' + id + '/restart', pause: 'POST /v1/agent/' + id + '/pause', cancel: 'POST /v1/agent/' + id + '/cancel', delete: 'DELETE /v1/agent/' + id };
          const ep = endpoints[cmd];
          if (!ep) return alert('Unknown cmd: ' + cmd);
          const [method, path] = ep.split(' ');
          const r = await fetch(path, { method: method });
          if (!r.ok) { let t = await r.text(); alert('Error: ' + t.substring(0,100)); }
          else { renderAgents(); }
        } catch(e) { alert(e.message); }
      };

      window.runAgent = async function() {
        const task = document.getElementById('agent-task').value;
        if (!task) return alert('Enter a task');
        try {
          await fetch('/v1/agent/run', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({task:task}) });
          document.getElementById('agent-task').value = '';
          renderAgents();
        } catch(e) { alert(e.message); }
      };

      // ── Plugins ────────────────────────────────────
      async function renderPlugins() {
        let [list, market] = await Promise.all([
          api('/v1/plugins').catch(() => []),
          api('/v1/plugins/market/list').catch(() => null),
        ]);
        let html = '<h1>🧩 Plugins</h1><div class="cards">' +
          card('Installed', Array.isArray(list) ? list.length : (list.plugins ? list.plugins.length : 0)) +
          card('Market', market ? (market.total || 0) : '—') +
        '</div>';
        // Installed plugins
        let plugins = Array.isArray(list) ? list : (list.plugins || []);
        if (plugins.length > 0) {
          html += '<div class="section"><h2>📦 Installed</h2><table><thead><tr><th>Name</th><th>Version</th><th>Status</th><th>Actions</th></tr></thead><tbody>';
          for (const p of plugins) {
            let pname = p.name || p.manifest?.name || '?';
            let pver = p.version || p.manifest?.version || '—';
            let pst = p.status || 'unknown';
            html += '<tr><td>' + esc(pname) + '</td><td>' + esc(pver) + '</td><td>' + badge(pst) +
              '</td><td class="actions">' +
              '<button class="btn" onclick="fetch('/v1/plugins/' + p.id + '/start',{method:'POST'}).then(()=>renderPlugins())">▶</button>' +
              '<button class="btn" onclick="fetch('/v1/plugins/' + p.id + '/stop',{method:'POST'}).then(()=>renderPlugins())">⏹</button>' +
              '</td></tr>';
          }
          html += '</tbody></table></div>';
        } else {
          html += '<div class="info-box">ℹ No plugins installed.</div>';
        }
        html += '<div class="section"><h2>🔍 Search Market</h2>' +
          '<div class="filter-bar"><input id="market-q" placeholder="Search plugins..." style="width:250px">' +
          '<button class="btn" onclick="searchMarket()">🔍 Search</button></div>' +
          '<div id="market-results"></div></div>';
        main.innerHTML = html;
      }

      window.searchMarket = async function() {
        const q = document.getElementById('market-q').value;
        const el = document.getElementById('market-results');
        el.innerHTML = '⏳ Searching...';
        try {
          let data = await api('/v1/plugins/market/search?q=' + encodeURIComponent(q));
          let pl = data.plugins || [];
          if (pl.length === 0) { el.innerHTML = '<p class="empty">No results.</p>'; return; }
          let html = '<table><thead><tr><th>Name</th><th>Version</th><th>Description</th></tr></thead><tbody>';
          for (const p of pl) {
            html += '<tr><td>' + esc(p.name) + '</td><td>' + esc(p.version) + '</td><td class="tt">' + esc(p.description.substring(0,80)) + '</td></tr>';
          }
          html += '</tbody></table>';
          el.innerHTML = html;
        } catch(e) { el.innerHTML = '<div class="error-box">' + e.message + '</div>'; }
      };

      // ── Billing ────────────────────────────────────
      async function renderBilling() {
        let stats = await api('/v1/billing/stats').catch(() => null);
        let html = '<h1>💰 Token Cost & Billing</h1>';
        if (stats) {
          let costIn = (stats.total_input_tokens || 0) * 0.002 / 1000;
          let costOut = (stats.total_output_tokens || 0) * 0.008 / 1000;
          html += '<div class="cards">' +
            card('Total Requests', (stats.total_requests || 0).toLocaleString()) +
            card('Input Tokens', (stats.total_input_tokens || 0).toLocaleString()) +
            card('Output Tokens', (stats.total_output_tokens || 0).toLocaleString()) +
            card('Est. Cost', '$' + (costIn + costOut).toFixed(4), { sub: 'GPT-3.5 rate' }) +
          '</div>';
          if (stats.per_model && stats.per_model.length > 0) {
            html += '<div class="section"><h2>📊 Per Model</h2><table><thead><tr><th>Model</th><th>Requests</th><th>Input Tokens</th><th>Output Tokens</th><th>Total Tokens</th></tr></thead><tbody>';
            for (const m of stats.per_model) {
              html += '<tr><td><code>' + esc(m.model) + '</code></td><td>' + (m.requests || 0) + '</td><td>' + (m.input_tokens || 0).toLocaleString() +
                '</td><td>' + (m.output_tokens || 0).toLocaleString() + '</td><td>' + (m.total_tokens || 0).toLocaleString() + '</td></tr>';
            }
            html += '</tbody></table></div>';
          }
        } else {
          html += '<div class="empty"><p>Billing stats unavailable. Try recording some usage first.</p></div>';
        }
        html += '<div class="section"><h2>📝 Record Usage</h2>' +
          '<div class="filter-bar">' +
          'User: <input id="bu-user" value="test-user" style="width:120px">' +
          'Model: <input id="bu-model" value="gpt-4" style="width:100px">' +
          'In: <input id="bu-in" value="100" style="width:60px">' +
          'Out: <input id="bu-out" value="50" style="width:60px">' +
          '<button class="btn btn-primary" onclick="recordUsage()">➕ Record</button></div></div>';
        main.innerHTML = html;
      }

      window.recordUsage = async function() {
        const body = {
          user_id: document.getElementById('bu-user').value,
          model: document.getElementById('bu-model').value,
          input_tokens: parseInt(document.getElementById('bu-in').value) || 0,
          output_tokens: parseInt(document.getElementById('bu-out').value) || 0,
        };
        try {
          await fetch('/v1/billing/usage', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify(body) });
          renderBilling();
        } catch(e) { alert(e.message); }
      };

      // ── Discovery ──────────────────────────────────
      async function renderDiscovery() {
        let [servers, health] = await Promise.all([
          api('/v1/discovery/servers').catch(() => null),
          api('/v1/discovery/health').catch(() => null),
        ]);
        let html = '<h1>🔍 MCP Discovery</h1><div class="cards">' +
          card('Servers', servers ? (servers.total || servers.servers?.length || 0) : '—') +
          card('Discovery', health ? badge(health.status || 'healthy') : badge('unknown')) +
        '</div>';
        let srvList = servers ? (servers.servers || []) : [];
        if (srvList.length > 0) {
          html += '<div class="section"><h2>📡 Discovered Servers</h2><table><thead><tr><th>Name</th><th>Type</th><th>Endpoint</th><th>Status</th></tr></thead><tbody>';
          for (const s of srvList) {
            html += '<tr><td>' + esc(s.name || '?') + '</td><td>' + esc(s.type || s.discovery_type || '—') +
              '</td><td><code>' + esc(s.endpoint || s.url || '—') + '</code></td><td>' + badge(s.status || 'unknown') + '</td></tr>';
          }
          html += '</tbody></table></div>';
        } else {
          html += '<div class="info-box">ℹ No MCP servers discovered yet.</div>';
        }
        if (health) {
          html += '<div class="section"><h2>⚕️ Health</h2><pre class="dump">' + esc(JSON.stringify(health, null, 2)) + '</pre></div>';
        }
        main.innerHTML = html;
      }

      // ── Tenants ────────────────────────────────────
      async function renderTenants() {
        let [orgs, stats] = await Promise.all([
          api('/v1/tenant/orgs').catch(() => []),
          api('/v1/tenant/stats').catch(() => null),
        ]);
        let html = '<h1>🏢 Multi-Tenant</h1><div class="cards">' +
          card('Orgs', stats ? stats.total_orgs : (Array.isArray(orgs) ? orgs.length : 0)) +
          card('Projects', stats ? stats.total_projects : '—') +
          card('Users', stats ? stats.total_users : '—') +
          card('Active', stats ? stats.active_orgs : '—') +
        '</div>';
        let orgList = Array.isArray(orgs) ? orgs : [];
        if (orgList.length > 0) {
          html += '<div class="section"><h2>🏢 Organizations</h2><table><thead><tr><th>Name</th><th>Slug</th><th>Status</th><th>Settings</th></tr></thead><tbody>';
          for (const o of orgList) {
            let s = o.settings || {};
            html += '<tr><td><strong>' + esc(o.name) + '</strong></td><td><code>' + esc(o.slug) + '</code></td><td>' + badge(o.status || 'active') +
              '</td><td class="tt">P:' + (s.max_projects||'—') + ' U:' + (s.max_users||'—') + '</td></tr>';
          }
          html += '</tbody></table></div>';
        } else {
          html += '<div class="empty"><p>No organizations yet.</p></div>';
        }
        main.innerHTML = html;
      }

      // ── Audit ──────────────────────────────────────
      async function renderAudit() {
        let audit = await api('/v1/audit/logs?limit=100').catch(() => null);
        let html = '<h1>📋 Audit Log</h1>';
        if (audit) {
          let entries = audit.entries || [];
          html += '<div class="cards">' + card('Total', audit.total || entries.length) + card('Displayed', entries.length) + '</div>';
          if (entries.length > 0) {
            html += '<div class="section"><table><thead><tr><th>Time</th><th>Type</th><th>Event</th><th>Actor</th><th>Result</th><th>Detail</th></tr></thead><tbody>';
            for (const e of entries) {
              html += '<tr><td class="tt">' + fmtTime(e.timestamp) + '</td><td>' + badge(e.event_type) + '</td><td>' + esc(e.event_name||'') +
                '</td><td>' + esc(e.actor||'') + '</td><td>' + badge(e.result||'') +
                '</td><td class="tt">' + esc((e.detail||'').substring(0,60)) + '</td></tr>';
            }
            html += '</tbody></table></div>';
          } else {
            html += '<div class="empty"><p>No audit entries.</p></div>';
          }
        } else {
          html += '<div class="empty"><p>Audit log unavailable.</p></div>';
        }
        main.innerHTML = html;
      }

      // ── Federation (保留原实现) ────────────────────
      async function renderFederation() {
        let [status, nodes] = await Promise.all([
          api('/v1/federation/status').catch(() => null),
          api('/v1/federation/nodes').catch(() => []),
        ]);
        let html = '<h1>🌐 Federation</h1><div class="cards">';
        if (status) {
          html += card('Status', status.enabled ? 'Enabled' : 'Disabled', { cls: status.enabled ? 'ok' : 'warn' });
          html += card('Nodes', status.node_count || 0);
          html += card('Peers', Array.isArray(nodes) ? nodes.length : 0);
        } else { html += card('Status', 'Unavailable', { cls: 'err' }); }
        html += '</div>';
        if (Array.isArray(nodes) && nodes.length > 0) {
          html += '<div class="section"><h2>📋 Peers</h2><table><thead><tr><th>Name</th><th>Address</th><th>Status</th><th>Capabilities</th></tr></thead><tbody>';
          for (const n of nodes) {
            html += '<tr><td>' + esc(n.name||'') + '</td><td><code>' + esc(n.addr||n.address||'') + '</code></td><td>' + badge(n.status||'') +
              '</td><td class="tt">' + esc((n.capabilities||[]).join(', ')) + '</td></tr>';
          }
          html += '</tbody></table></div>';
        }
        main.innerHTML = html;
      }

      // ── Eval (保留原实现) ──────────────────────────
      async function renderEval() {
        let result = await api('/v1/eval/result').catch(() => null);
        let html = '<h1>📋 Evaluation</h1>';
        if (result) {
          let st = (result.status === 'passed' || result.status === 'success') ? 'ok' : result.status === 'failed' ? 'err' : 'warn';
          html += '<div class="cards">' +
            card('Score', result.score != null ? result.score.toFixed(2) : '—', { cls: st }) +
            card('Status', result.status || '—', { cls: st }) +
            card('ID', result.id ? result.id.substring(0,8) : '—') +
          '</div>';
          if (result.metrics && Object.keys(result.metrics).length > 0) {
            html += '<div class="section"><h2>📈 Metrics</h2><table><thead><tr><th>Metric</th><th>Value</th></tr></thead><tbody>';
            for (const [k, v] of Object.entries(result.metrics)) {
              html += '<tr><td>' + esc(k) + '</td><td style="font-family:monospace;color:#3fb950">' + (typeof v === 'number' ? v.toFixed(4) : v) + '</td></tr>';
            }
            html += '</tbody></table></div>';
          }
        } else {
          html += '<div class="empty"><p>No evaluation results.</p></div>';
        }
        main.innerHTML = html;
      }

      // ── Init ───────────────────────────────────────
      navLinks.forEach(a => {
        a.addEventListener('click', function(e) { e.preventDefault(); loadPage(this.dataset.page); });
      });
      // Restore page from URL
      const params = new URLSearchParams(window.location.search);
      const initialPage = params.get('page') || 'dashboard';
      loadPage(initialPage);
    })();
  </script>
</body>
</html>"##.to_string())
}// ── Webhook Handlers ──────────────────────────────────────

/// POST /v1/channels/feishu/webhook — 飞书事件回调
/// 接收飞书开放平台事件订阅回调，自动解析为 InboundEvent。
async fn feishu_webhook_handler(
    State(state): State<Arc<AppState>>,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let channel = state.runtime.channel_registry.get("feishu").await
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "feishu channel not found"})),
        ))?;
    // 飞书回调验证: 处理 URL 验证挑战
    if let Some(challenge) = raw.get("challenge").and_then(|v| v.as_str()) {
        return Ok(Json(json!({"challenge": challenge})));
    }
    // 构造 InboundEvent，raw 字段保留原始事件供通道解析
    let event = InboundEvent {
        channel_id: "feishu".into(),
        message_id: None,
        sender_id: None,
        sender_name: None,
        chat_type: lingshu_channel::ChatType::Direct,
        chat_id: None,
        text: None,
        media_urls: vec![],
        reply_to_id: None,
        timestamp: chrono::Utc::now().timestamp(),
        raw: Some(raw),
    };
    channel.handle_inbound(event).await.map_err(|e| {
        tracing::warn!(error = %e, "feishu inbound handler failed");
        (StatusCode::OK, Json(json!({"status": "accepted", "warning": e.to_string()})))
    })?;
    Ok(Json(json!({"status": "ok"})))
}

/// POST /v1/channels/qq/webhook — QQ 机器人回调
async fn qq_webhook_handler(
    State(state): State<Arc<AppState>>,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let channel = state.runtime.channel_registry.get("qq").await
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "qq channel not found"})),
        ))?;
    let event = InboundEvent {
        channel_id: "qq".into(),
        message_id: None,
        sender_id: None,
        sender_name: None,
        chat_type: lingshu_channel::ChatType::Direct,
        chat_id: None,
        text: None,
        media_urls: vec![],
        reply_to_id: None,
        timestamp: chrono::Utc::now().timestamp(),
        raw: Some(raw),
    };
    channel.handle_inbound(event).await.map_err(|e| {
        tracing::warn!(error = %e, "qq inbound handler failed");
        (StatusCode::OK, Json(json!({"status": "accepted", "warning": e.to_string()})))
    })?;
    Ok(Json(json!({"status": "ok"})))
}

/// 构建路由 — 聚合模块化路由 + full.rs 遗留路由.
///
/// 先组合各模块路由，再合并 full.rs 遗留路由，最后应用共享状态.
pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    Router::new()
        .route("/health", get(health_handler))
        // Login / Logout (无需认证)
        .route("/login", get(login_page_handler).post(login_handler))
        .route("/logout", get(logout_handler))
        .route("/api/auth/me", get(auth_me_handler))
        .route("/metrics", get(metrics_handler))
        .route("/v1/metrics", get(v1_metrics_handler))
        .route("/version", get(version_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/embeddings", post(embeddings_handler))
        .route("/v1/chat", post(chat_handler))
        .route("/v1/embed", post(embed_handler))
        .route("/v1/agent/run", post(agent_run_handler))
        .route("/v1/agents", get(agent_list_handler))
        .route("/v1/agents/:id", get(agent_status_handler))
        .route("/v1/agents/:id/pause", post(agent_pause_handler))
        .route("/v1/agents/:id/resume", post(agent_resume_handler))
        .route("/v1/agents/:id/cancel", post(agent_cancel_handler))
        // Agent Lifecycle Management (v4.3 Enterprise)
        .route("/v1/agent/:id/restart", post(agent_restart_handler))
        .route("/v1/agent/:id/update", post(agent_update_handler))
        .route("/v1/agent/:id", delete(agent_delete_handler))
        .route("/ws", get(ws_handler))
        // v2 Real-time API
        .route("/v2/chat/stream", get(v2_chat_stream_handler))
        .route("/v2/ws", get(v2_ws_handler))
        .route("/v2/events", get(v2_events_handler))
        // File API (多模态)
        .route("/v1/files/upload", post(upload_file_handler))
        .route("/v1/files/analyze", post(analyze_file_handler))
        .route("/v1/files", get(list_files_handler))
        .route("/v1/files/:id", get(get_file_handler))
        .route("/v1/chat/multimodal", post(multimodal_chat_handler))
        // Plugin API
        .route(
            "/v1/plugins",
            get(plugin_list_handler).post(plugin_install_handler),
        )
        .route(
            "/v1/plugins/:id",
            get(plugin_get_handler).delete(plugin_uninstall_handler),
        )
        .route("/v1/plugins/:id/start", post(plugin_start_handler))
        .route("/v1/plugins/:id/stop", post(plugin_stop_handler))
        // Plugin Market API
        .route("/v1/plugins/market/search", get(market_search_handler))
        .route("/v1/plugins/market/list", get(market_list_handler))
        .route("/v1/plugins/market/sources/:source_type", delete(market_remove_source_handler))
        .route("/v1/plugins/market/install", post(market_install_handler))
        .route(
            "/v1/plugins/market/sources",
            get(market_sources_handler).post(market_add_source_handler),
        )
        .route(
            "/v1/plugins/hotreload/start",
            post(hot_reload_start_handler),
        )
        .route("/v1/plugins/hotreload/stop", post(hot_reload_stop_handler))
        .route("/v1/plugins/events", get(plugin_events_handler))
        // BeEF Security Testing API
        .route("/v1/security/beef/status", get(beef_status_handler))
        .route("/v1/security/beef/start", post(beef_start_handler))
        .route("/v1/security/beef/stop", post(beef_stop_handler))
        .route("/v1/security/beef/hooks", get(beef_hooks_handler))
        .route("/v1/security/beef/restart", post(beef_restart_handler))
        // Watch Skill Video Analysis API
        .route("/v1/watch/status", get(watch_status_handler))
        .route("/v1/watch/start", post(watch_start_handler))
        .route("/v1/watch/stop", post(watch_stop_handler))
        .route("/v1/watch/video", post(watch_video_handler))
        .route("/v1/watch/ask", post(watch_ask_handler))
        .route("/v1/watch/search", get(watch_search_handler))
        .route("/v1/watch/videos", get(watch_list_videos_handler))
        // Knowledge Graph API
        .route(
            "/v1/graph/:project",
            get(graph_query_handler).post(graph_analyze_handler),
        )
        .route("/v1/graph/:project/view", get(graph_view_handler))
        .route("/v1/projects", get(project_list_handler))
        // Credential Vault API
        .route("/v1/credentials/ui", get(credential_ui_handler))
        .route(
            "/v1/credentials",
            get(credential_list_handler).post(credential_create_handler),
        )
        .route(
            "/v1/credentials/:id",
            get(credential_get_handler)
                .put(credential_update_handler)
                .delete(credential_delete_handler),
        )
        .route("/v1/credentials/:id/token", get(credential_token_handler))
        .route(
            "/v1/credentials/:id/validate",
            post(credential_validate_handler),
        )
        .route(
            "/v1/credentials/providers",
            get(credential_providers_handler),
        )
                // Audit API — 审计日志查询
        .route("/v1/audit/logs", get(audit_query_handler))
// Tenant API (MT) -- 多租户管理
        .route("/v1/tenant/orgs", get(tenant_list_orgs_handler).post(tenant_create_org_handler))
        .route("/v1/tenant/orgs/:org_id", get(tenant_get_org_handler))
        .route("/v1/tenant/orgs/:org_id/projects", get(tenant_list_projects_handler).post(tenant_create_project_handler))
        .route("/v1/tenant/orgs/:org_id/projects/:project_id", get(tenant_get_project_handler))
        .route("/v1/tenant/orgs/:org_id/users", get(tenant_list_users_handler).post(tenant_invite_user_handler))
        .route("/v1/tenant/orgs/:org_id/users/:user_id", get(tenant_get_user_handler).delete(tenant_remove_user_handler))
        .route("/v1/tenant/stats", get(tenant_stats_handler))
        // Vault API (Secret Management)
        .route("/v1/vault/health", get(vault_health_handler))
        .route("/v1/vault/secrets", post(vault_write_handler).get(vault_list_handler))
        .route("/v1/vault/secrets/*path", get(vault_read_handler).delete(vault_delete_handler))
        .route("/v1/vault/encrypt", post(vault_encrypt_handler))
        .route("/v1/vault/decrypt", post(vault_decrypt_handler))
        .route("/v1/vault/dynamic-secret/*path", get(vault_dynamic_secret_handler))
        .route("/v1/vault/lease/:lease_id/renew", post(vault_renew_lease_handler))
        .route("/v1/vault/lease/:lease_id/revoke", post(vault_revoke_lease_handler))
        // TEE API (Confidential Computing)
        .route("/v1/tee/health", get(tee_health_handler))
        .route("/v1/tee/attest", post(tee_attest_handler))
        .route("/v1/tee/encrypted-memory", post(tee_encrypted_memory_store_handler).get(tee_encrypted_memory_list_handler))
        .route("/v1/tee/encrypted-memory/:id", get(tee_encrypted_memory_get_handler).delete(tee_encrypted_memory_delete_handler))
        .route("/v1/tee/policy", get(tee_policy_get_handler).put(tee_policy_update_handler))
        // Evaluator API (ES)
        .route("/v1/eval/run", post(eval_run_handler))
        .route("/v1/eval/result", get(eval_result_handler))
        .route("/v1/eval/regression", post(eval_regression_handler))
        // Federation API (Fed)
        .route("/v1/federation/status", get(federation_status_handler))
        .route("/v1/federation/nodes", get(federation_nodes_handler))
        .route("/v1/federation/execute", post(federation_execute_handler))
        // Channel Webhook API
        .route("/v1/channels/feishu/webhook", post(feishu_webhook_handler))
        .route("/v1/channels/qq/webhook", post(qq_webhook_handler))
        // Billing API (v4.3 Enterprise — Token Cost)
        .route("/v1/billing/stats", get(crate::api::billing::billing_stats_handler))
        .route("/v1/billing/report/:user_id", get(crate::api::billing::billing_report_handler))
        .route("/v1/billing/quota/:user_id", get(crate::api::billing::billing_quota_handler))
        .route("/v1/billing/usage", post(crate::api::billing::record_usage_handler))
        // Discovery API (v4.3 Enterprise — MCP Auto-Discovery)
        .route("/v1/discovery/servers", get(crate::api::discovery::discovery_list_handler))
        .route("/v1/discovery/health", get(crate::api::discovery::discovery_health_handler))
        // API Documentation
        .route("/docs", get(docs_handler))
        .route("/docs/openapi.json", get(openapi_json_handler))
        .route("/docs/swagger", get(swagger_ui_handler))
        // Admin Dashboard + WebUI (需认证)
        .merge(
            Router::new()
                .route("/admin", get(admin_handler))
                .nest_service(
                    "/webui",
                    tower_http::services::fs::ServeDir::new("webui/dist"),
                )
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    admin_auth_middleware,
                )),
        )
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        // Rate limiting middleware — wrap each request with a check
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state)
}

// ── Admin Auth Middleware ────────────────────────────

/// Admin/WebUI 认证中间件。
/// 从  或  Cookie 中提取 JWT，验证后放行。
async fn admin_auth_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, Redirect> {
    let token = extract_token_from_request(&req);

    match token {
        Some(token) => match state.jwt_service.authenticate(&token) {
            Ok(auth) => {
                let mut req = req;
                req.extensions_mut().insert(auth);
                Ok(next.run(req).await)
            }
            Err(_) => Err(Redirect::to("/login")),
        },
        None => Err(Redirect::to("/login")),
    }
}

/// 从请求中提取 JWT token。
fn extract_token_from_request(req: &axum::extract::Request) -> Option<String> {
    // 1. 检查 Authorization header
    if let Some(auth_val) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_val.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // 2. 检查 Cookie
    if let Some(cookie_val) = req.headers().get(header::COOKIE) {
        if let Ok(cookie_str) = cookie_val.to_str() {
            for pair in cookie_str.split(';') {
                let pair = pair.trim();
                if let Some(val) = pair.strip_prefix("lingshu_session=") {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
}

// ── Admin Password Verification ──────────────────────

fn verify_admin_password(username: &str, password: &str) -> bool {
    let expected_user = std::env::var("LS_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
    let expected_pass = std::env::var("LS_ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());

    let user_match = constant_time_eq(username.as_bytes(), expected_user.as_bytes());
    let pass_match = constant_time_eq(password.as_bytes(), expected_pass.as_bytes());
    user_match && pass_match
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

// ── Login / Logout Handlers ─────────────────────────

/// 登录页面 (GET).
async fn login_page_handler() -> Html<String> {
    Html(r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lingshu Login</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background: #0d1117; color: #c9d1d9; display: flex;
      align-items: center; justify-content: center; min-height: 100vh;
    }
    .login-box {
      background: #161b22; border: 1px solid #30363d; border-radius: 8px;
      padding: 2.5rem 2rem; width: 360px;
    }
    .login-box h1 { font-size: 1.3rem; margin-bottom: 0.3rem; color: #58a6ff; }
    .login-box p { font-size: 0.85rem; color: #8b949e; margin-bottom: 1.5rem; }
    .login-box label { display: block; font-size: 0.8rem; color: #8b949e; margin-bottom: 0.3rem; }
    .login-box input {
      width: 100%; padding: 0.6rem 0.8rem; margin-bottom: 1rem;
      background: #0d1117; border: 1px solid #30363d; border-radius: 6px;
      color: #c9d1d9; font-size: 0.9rem; outline: none;
    }
    .login-box input:focus { border-color: #58a6ff; }
    .login-box button {
      width: 100%; padding: 0.6rem; background: #238636; color: #fff;
      border: none; border-radius: 6px; font-size: 0.9rem; cursor: pointer;
    }
    .login-box button:hover { background: #2ea043; }
    .error { color: #f85149; font-size: 0.85rem; margin-bottom: 0.8rem; display: none; }
    .login-box .hint { font-size: 0.75rem; color: #6e7681; margin-top: 1rem; text-align: center; }
  </style>
</head>
<body>
  <div class="login-box">
    <h1>⚡ Lingshu</h1>
    <p>Admin Panel — 请输入管理员凭证</p>
    <div class="error" id="error-msg"></div>
    <form id="login-form">
      <label for="username">用户名</label>
      <input type="text" id="username" name="username" placeholder="admin" required autofocus>
      <label for="password">密码</label>
      <input type="password" id="password" name="password" placeholder="••••••••" required>
      <button type="submit">登录</button>
    </form>
    <div class="hint">默认凭证: admin / admin</div>
  </div>
  <script>
    document.getElementById('login-form').addEventListener('submit', async function(e) {
      e.preventDefault();
      const errEl = document.getElementById('error-msg');
      const username = document.getElementById('username').value;
      const password = document.getElementById('password').value;
      try {
        const res = await fetch('/login', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ username, password })
        });
        if (res.ok) {
          window.location.href = '/admin';
        } else {
          const data = await res.json();
          errEl.textContent = data.message || '登录失败';
          errEl.style.display = 'block';
        }
      } catch(e) {
        errEl.textContent = '网络错误: ' + e.message;
        errEl.style.display = 'block';
      }
    });
  </script>
</body>
</html>"##.to_string())
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    user_id: String,
    roles: Vec<String>,
}

/// 处理登录 (POST) — 验证凭证并签发 JWT。
async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<
    (StatusCode, [(&'static str, String); 2], Json<LoginResponse>),
    (StatusCode, Json<serde_json::Value>),
> {
    if !verify_admin_password(&req.username, &req.password) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"message": "用户名或密码错误"})),
        ));
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let token = state
        .jwt_service
        .issue(&req.username, &session_id, None, vec!["admin".to_string()])
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"message": format!("token issue failed: {e}")})),
            )
        })?;

    let body = LoginResponse {
        token: token.clone(),
        user_id: req.username,
        roles: vec!["admin".to_string()],
    };

    Ok((
        StatusCode::OK,
        [
            ("content-type", "application/json".to_string()),
            (
                "set-cookie",
                format!("lingshu_session={token}; Path=/; HttpOnly; SameSite=Lax"),
            ),
        ],
        Json(body),
    ))
}

/// 登出 (GET) — 清除 session cookie 并重定向到 /login。
async fn logout_handler() -> (StatusCode, [(&'static str, &'static str); 2], &'static str) {
    (
        StatusCode::FOUND,
        [
            ("location", "/login"),
            (
                "set-cookie",
                "lingshu_session=; Path=/; HttpOnly; Max-Age=0",
            ),
        ],
        "Redirecting to login...",
    )
}

/// 返回当前认证用户信息。
async fn auth_me_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Json<serde_json::Value> {
    // 尝试从 Authorization header 或 Cookie 中提取 token
    let token: Option<String> = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .and_then(|cookie_str| {
                    for pair in cookie_str.split(';') {
                        let pair = pair.trim();
                        if let Some(val) = pair.strip_prefix("lingshu_session=") {
                            return Some(val.to_string());
                        }
                    }
                    None
                })
        });

    match token {
        Some(t) => match state.jwt_service.authenticate(&t) {
            Ok(auth) => Json(
                serde_json::json!({"authenticated": true, "user_id": auth.user_id, "roles": auth.roles}),
            ),
            Err(_) => Json(serde_json::json!({"authenticated": false})),
        },
        None => Json(serde_json::json!({"authenticated": false})),
    }
}

// ── Rate Limiting Middleware ─────────────────────────

/// Rate limiting middleware — checks per-IP rate before processing request.
async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    // Extract client IP from headers or connection info
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    match state.runtime.rate_limiter.check_all(&client_ip).await {
        Ok(result) if !result.allowed => {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(
                    serde_json::json!({"error": "rate_limit_exceeded", "message": "Too many requests. Please slow down.", "retry_after": result.reset_at}),
                ),
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, ip = %client_ip, "rate limiter check failed, allowing request");
        }
        _ => {}
    }

    Ok(next.run(req).await)
}

// ── Handlers ────────────────────────────────────────

/// GET /health
async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let response = state.health_registry.check_all().await;

    let checks: Vec<HealthCheckItem> = response
        .checks
        .iter()
        .map(|s| HealthCheckItem {
            name: s.component.clone(),
            healthy: s.healthy,
            detail: s.message.clone(),
        })
        .collect();

    let all_healthy = checks.iter().all(|c| c.healthy);
    let status = if all_healthy {
        "ok".into()
    } else {
        "degraded".into()
    };

    Json(HealthResponse {
        status,
        version: response.version,
        uptime: response.checked_at.to_rfc3339(),
        checks,
    })
}

/// GET /metrics
async fn metrics_handler() -> (StatusCode, String) {
    let registry = lingshu_observability::metrics::MetricsRegistry::global();
    let text = registry.gather_text();
    (StatusCode::OK, text)
}


/// GET /v1/metrics — JSON format for WebUI real-time charts
async fn v1_metrics_handler() -> Json<MetricsJsonResponse> {
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu_usage = sys.global_cpu_usage() as f64;
    let total_mem_kb = sys.total_memory();
    let used_mem_kb = sys.used_memory();
    let memory_mb = if total_mem_kb > 0 {
        used_mem_kb as f64 / 1024.0
    } else {
        0.0
    };

    let registry = lingshu_observability::metrics::MetricsRegistry::global();
    let gathered = registry.gather();
    let mut llm_requests_total = 0u64;
    let mut llm_tokens_total = 0u64;
    for mf in &gathered {
        match mf.get_name() {
            "ls_llm_invocations_total" => {
                for m in mf.get_metric() {
                    llm_requests_total += m.get_counter().get_value() as u64;
                }
            }
            "ls_llm_tokens_total" => {
                for m in mf.get_metric() {
                    llm_tokens_total += m.get_counter().get_value() as u64;
                }
            }
            _ => {}
        }
    }

    Json(MetricsJsonResponse {
        cpu_usage,
        memory_mb,
        active_sessions: 0,
        total_agents: 0,
        llm_requests_total,
        llm_tokens_total,
        federation_nodes: 0,
        uptime_secs: 0,
        custom_metrics: vec![
            MetricSample {
                name: "ls_cpu_usage".into(),
                value: cpu_usage,
                labels: std::collections::HashMap::new(),
            },
            MetricSample {
                name: "ls_memory_mb".into(),
                value: memory_mb,
                labels: std::collections::HashMap::new(),
            },
        ],
    })
}
/// GET /version
async fn version_handler() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: "1.0.0".into(),
        build: env!("CARGO_PKG_VERSION").into(),
        rustc: "stable".into(),
    })
}

/// GET /v1/models
async fn models_handler(State(state): State<Arc<AppState>>) -> Json<Vec<ModelInfo>> {
    let default = &state.runtime.config.llm.default_model;
    Json(vec![
        ModelInfo {
            id: default.clone(),
            object: "model".into(),
            created: 1735689600,
            owned_by: "lingshu".into(),
        },
        ModelInfo {
            id: "gpt-4o".into(),
            object: "model".into(),
            created: 1735689600,
            owned_by: "openai".into(),
        },
        ModelInfo {
            id: "gpt-4o-mini".into(),
            object: "model".into(),
            created: 1735689600,
            owned_by: "openai".into(),
        },
        ModelInfo {
            id: "claude-3-5-sonnet-20241022".into(),
            object: "model".into(),
            created: 1735689600,
            owned_by: "anthropic".into(),
        },
        ModelInfo {
            id: "deepseek-chat".into(),
            object: "model".into(),
            created: 1735689600,
            owned_by: "deepseek".into(),
        },
    ])
}

/// POST /v1/chat/completions
async fn chat_completions_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> axum::response::Response {
    if req.stream.unwrap_or(false) {
        return handle_streaming_chat(state, req).await.into_response();
    }
    handle_non_streaming_chat(state.clone(), req)
        .await
        .into_response()
}

/// Handle non-streaming chat completion (supports tool calling)
async fn handle_non_streaming_chat(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
) -> Result<Json<ChatCompletionResp>, (StatusCode, Json<Value>)> {
    let runtime = &state.runtime;
    let llm = runtime.llm.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no LLM configured"})),
        )
    })?;

    let session_id = req
        .session_id
        .clone()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();
    let ctx = LsContext::with_session(session_id);
    let session_id_str = session_id.to_string();

    // Recall conversation history from memory
    let memory = state
        .runtime
        .memory_manager
        .get_or_create(&session_id_str)
        .await;
    let memory_result = memory
        .recall(&ctx, &lingshu_memory::MemoryQuery::default())
        .await
        .ok();

    // Build message list: memory history + request messages
    let mut messages: Vec<LlmMessage> = Vec::new();
    if let Some(result) = memory_result {
        for item in &result.items {
            let role = match item.role.as_str() {
                "system" => LlmRole::System,
                "assistant" => LlmRole::Assistant,
                _ => LlmRole::User,
            };
            messages.push(LlmMessage {
                role,
                content: item.content.clone(),
                content_parts: None,
                name: None,
                tool_calls: None,
            });
        }
    }

    let req_messages: Vec<LlmMessage> = req
        .messages
        .into_iter()
        .map(|m| {
            let role = match m.role.as_str() {
                "system" => LlmRole::System,
                "assistant" => LlmRole::Assistant,
                "tool" => LlmRole::Tool,
                _ => LlmRole::User,
            };
            LlmMessage {
                role,
                content: m.content,
                name: m.name,
                content_parts: None,
                tool_calls: None,
            }
        })
        .collect();
    messages.extend(req_messages.clone());

    if let Some(ref user) = req.user {
        let _ = runtime
            .session_mgr
            .create(&LsContext::with_session(session_id).with_user(user))
            .await;
    }

    // Store user/system messages from this request to memory
    for msg in &req_messages {
        let role_str = match msg.role {
            LlmRole::System => "system",
            LlmRole::Assistant => continue,
            LlmRole::User => "user",
            LlmRole::Tool => continue,
        };
        let _ = memory.store_message(&ctx, role_str, &msg.content).await;
    }

    // Tool calling loop: max 10 iterations
    let tools = req.tools.clone();
    for _ in 0..10 {
        let request = LlmRequest {
            model: req.model.clone(),
            messages: messages.clone(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            tools: tools.clone(),
            stream: false,
        };

        match llm.invoke(ctx.clone(), request).await {
            Ok(mut response) => {
                let has_tool_calls = response.message.tool_calls.is_some()
                    && response
                        .message
                        .tool_calls
                        .as_ref()
                        .map(|c| !c.is_empty())
                        .unwrap_or(false);

                if !has_tool_calls {
                    // Store assistant response to memory
                    let _ = memory
                        .store_message(&ctx, "assistant", &response.message.content)
                        .await;

                    let usage = UsageInfo {
                        prompt_tokens: response.usage.prompt_tokens,
                        completion_tokens: response.usage.completion_tokens,
                        total_tokens: response.usage.total_tokens,
                    };

                    // Audit log
                    let audit_entry = AuditEntry::new(
                        AuditEventType::ApiCall,
                        "chat.completion",
                        &req.user.clone().unwrap_or_else(|| "anonymous".into()),
                        "session",
                        &session_id_str,
                        &format!("model={} tokens={}", req.model, usage.total_tokens),
                    );
                    let _ = state.runtime.audit_log.append(audit_entry).await;

                    // Billing: track token usage
                    let _ = state
                        .runtime
                        .billing
                        .record_usage(
                            &req.user.clone().unwrap_or_else(|| "anonymous".into()),
                            &req.model,
                            usage.prompt_tokens,
                            usage.completion_tokens,
                        )
                        .await;

                    return Ok(Json(ChatCompletionResp {
                        id: format!("chatcmpl-{}", session_id),
                        object: "chat.completion".into(),
                        created: chrono::Utc::now().timestamp() as u64,
                        model: "lingshu-1".into(),
                        choices: vec![Choice {
                            index: 0,
                            message: RespMsg {
                                role: "assistant".into(),
                                content: response.message.content,
                            },
                            finish_reason: response.finish_reason,
                        }],
                        usage,
                    }));
                }

                // Add assistant message with tool_calls to history
                let tool_calls = response.message.tool_calls.take();
                messages.push(response.message);

                // Execute each tool call
                for tool_call in tool_calls.unwrap_or_default() {
                    let args: Value = serde_json::from_str(&tool_call.function.arguments)
                        .unwrap_or(json!({"error": "invalid args"}));

                    let tool_result = state
                        .runtime
                        .tool_registry
                        .read()
                        .await
                        .execute_unchecked(&ctx, &tool_call.function.name, args)
                        .await;

                    let result_content = match tool_result {
                        Ok(val) => val.to_string(),
                        Err(e) => format!("{{\"error\":\"{e}\"}}"),
                    };

                    messages.push(LlmMessage {
                        role: LlmRole::Tool,
                        content: result_content,
                        name: Some(tool_call.function.name),
                        content_parts: None,
                        tool_calls: None,
                    });
                }
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("{}", e)})),
                ));
            }
        }
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "tool call limit exceeded (max 10 iterations)"})),
    ))
}

/// Handle streaming chat completion (SSE) — supports tool calls and interrupt.
async fn handle_streaming_chat(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
) -> Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>> {
    let runtime = &state.runtime;

    let session_id = req
        .session_id
        .clone()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();
    let ctx = LsContext::with_session(session_id);
    let session_id_str = session_id.to_string();

    // Recall conversation history from memory
    let memory = state
        .runtime
        .memory_manager
        .get_or_create(&session_id_str)
        .await;
    let memory_result = memory
        .recall(&ctx, &lingshu_memory::MemoryQuery::default())
        .await
        .ok();

    let mut messages: Vec<LlmMessage> = Vec::new();
    if let Some(result) = memory_result {
        for item in &result.items {
            let role = match item.role.as_str() {
                "system" => LlmRole::System,
                "assistant" => LlmRole::Assistant,
                _ => LlmRole::User,
            };
            messages.push(LlmMessage {
                role,
                content: item.content.clone(),
                content_parts: None,
                name: None,
                tool_calls: None,
            });
        }
    }

    let req_messages: Vec<LlmMessage> = req
        .messages
        .into_iter()
        .map(|m| {
            let role = match m.role.as_str() {
                "system" => LlmRole::System,
                "assistant" => LlmRole::Assistant,
                "tool" => LlmRole::Tool,
                _ => LlmRole::User,
            };
            LlmMessage {
                role,
                content: m.content,
                name: m.name,
                content_parts: None,
                tool_calls: None,
            }
        })
        .collect();
    messages.extend(req_messages.clone());

    let tools = req.tools.clone();
    let llm = runtime.llm.clone();

    // Build the stream: a channel-based stream that can handle multi-turn tool calling
    let (global_tx, global_rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(256);

    tokio::spawn(async move {
        let mut current_messages = messages;
        let max_tool_rounds = 10;

        for _round in 0..max_tool_rounds {
            let request = LlmRequest {
                model: req.model.clone(),
                messages: current_messages.clone(),
                temperature: req.temperature,
                max_tokens: req.max_tokens,
                tools: tools.clone(),
                stream: true,
            };

            let llm = match &llm {
                Some(llm) => llm.clone(),
                None => {
                    let _ = global_tx
                        .send(Ok(
                            Event::default().data("{\"error\":\"no LLM configured\"}")
                        ))
                        .await;
                    return;
                }
            };

            match llm.invoke_stream(ctx.clone(), request).await {
                Ok(mut rx) => {
                    // Collect tool call deltas across chunks
                    // Each delta chunk produces ToolCall objects with partial fields;
                    // we accumulate by position (index) in the Vec.
                    let mut tool_call_builders: Vec<(String, String, String, String)> = Vec::new();
                    let mut full_content = String::new();
                    let mut has_tool_calls = false;

                    while let Some(chunk_result) = rx.recv().await {
                        match chunk_result {
                            Ok(chunk) => {
                                // Accumulate content
                                if let Some(ref text) = chunk.content {
                                    full_content.push_str(text);
                                }

                                // Accumulate tool call deltas
                                if let Some(ref tcs) = chunk.tool_calls {
                                    has_tool_calls = true;
                                    // Ensure vec is large enough
                                    while tool_call_builders.len() < tcs.len() {
                                        tool_call_builders.push((
                                            String::new(),
                                            String::new(),
                                            String::new(),
                                            String::new(),
                                        ));
                                    }
                                    for (i, tc) in tcs.iter().enumerate() {
                                        if i < tool_call_builders.len() {
                                            let entry = &mut tool_call_builders[i];
                                            if !tc.id.is_empty() {
                                                entry.0 = tc.id.clone();
                                            }
                                            if !tc.call_type.is_empty() {
                                                entry.1 = tc.call_type.clone();
                                            }
                                            if !tc.function.name.is_empty() {
                                                entry.2.push_str(&tc.function.name);
                                            }
                                            if !tc.function.arguments.is_empty() {
                                                entry.3.push_str(&tc.function.arguments);
                                            }
                                        }
                                    }
                                }

                                // Forward the chunk as SSE
                                let data = json!({
                                    "choices": [{
                                        "delta": {
                                            "content": chunk.content,
                                        },
                                        "index": 0,
                                        "finish_reason": chunk.finish_reason,
                                    }]
                                });
                                if global_tx
                                    .send(Ok(Event::default().data(data.to_string())))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }

                                // Check if this is the final chunk with tool_calls finish_reason
                                if let Some(ref reason) = chunk.finish_reason {
                                    if reason == "tool_calls" && has_tool_calls {
                                        break; // Exit chunk loop to process tools
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = global_tx
                                    .send(Ok(Event::default().data(format!("error: {}", e))))
                                    .await;
                                return;
                            }
                        }
                    }

                    // If tool calls were detected, execute them and continue the loop
                    if has_tool_calls && !tool_call_builders.is_empty() {
                        // Add assistant message with tool calls to history
                        let tool_calls: Vec<lingshu_traits::llm::ToolCall> = tool_call_builders
                            .into_iter()
                            .map(
                                |(id, call_type, name, args)| lingshu_traits::llm::ToolCall {
                                    id,
                                    call_type,
                                    function: lingshu_traits::llm::ToolCallFunction {
                                        name,
                                        arguments: args,
                                    },
                                },
                            )
                            .collect();

                        current_messages.push(LlmMessage {
                            role: LlmRole::Assistant,
                            content: full_content.clone(),
                            content_parts: None,
                            name: None,
                            tool_calls: Some(tool_calls.clone()),
                        });

                        // Execute each tool call (simplified: just record the call)
                        for tc in &tool_calls {
                            let result = format!(
                                "executed tool: {} with args: {}",
                                tc.function.name, tc.function.arguments
                            );
                            // Forward tool execution event
                            let tool_data = json!({
                                "type": "tool_call",
                                "id": tc.id,
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                                "result": result,
                            });
                            let _ = global_tx
                                .send(Ok(Event::default()
                                    .event("tool_call")
                                    .data(tool_data.to_string())))
                                .await;

                            // Add tool result to messages for next round
                            current_messages.push(LlmMessage {
                                role: LlmRole::Tool,
                                content: result,
                                content_parts: None,
                                name: None,
                                tool_calls: None,
                            });
                        }

                        // Continue loop to send tool results back to LLM
                        continue;
                    }

                    // Store final content to memory
                    if !full_content.is_empty() {
                        let _ = memory.store_message(&ctx, "assistant", &full_content).await;
                    }

                    // Send [DONE] signal
                    let _ = global_tx.send(Ok(Event::default().data("[DONE]"))).await;
                    return;
                }
                Err(e) => {
                    let _ = global_tx
                        .send(Ok(Event::default().data(format!("error: {}", e))))
                        .await;
                    return;
                }
            }
        }

        // Max rounds exceeded
        let _ = global_tx
            .send(Ok(
                Event::default().data("{\"error\":\"max tool call rounds exceeded\"}")
            ))
            .await;
    });

    Sse::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(
        global_rx,
    )))
}
async fn embeddings_handler(Json(req): Json<EmbedReq>) -> Json<EmbedResp> {
    let dims = 1536;
    let data: Vec<EmbedItem> = req
        .input
        .iter()
        .enumerate()
        .map(|(i, _)| EmbedItem {
            object: "embedding".into(),
            index: i,
            embedding: vec![0.0; dims],
        })
        .collect();

    Json(EmbedResp {
        object: "list".into(),
        data,
        model: req.model.unwrap_or_else(|| "text-embedding-3-small".into()),
        usage: UsageInfo {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    })
}

/// POST /v1/chat
pub async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatReq>,
) -> Result<Json<ChatResp>, (StatusCode, Json<Value>)> {
    let runtime = &state.runtime;
    let llm = runtime.llm.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no LLM configured"})),
        )
    })?;

    let session_id = req
        .session_id
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(LsId::new);
    let ctx = LsContext::with_session(session_id);
    let session_id_str = session_id.to_string();

    // Recall conversation history from memory
    let memory = state
        .runtime
        .memory_manager
        .get_or_create(&session_id_str)
        .await;
    let memory_result = memory
        .recall(&ctx, &lingshu_memory::MemoryQuery::default())
        .await
        .ok();

    let mut messages: Vec<LlmMessage> = Vec::new();
    if let Some(result) = memory_result {
        for item in &result.items {
            let role = match item.role.as_str() {
                "system" => LlmRole::System,
                "assistant" => LlmRole::Assistant,
                _ => LlmRole::User,
            };
            messages.push(LlmMessage {
                role,
                content: item.content.clone(),
                content_parts: None,
                name: None,
                tool_calls: None,
            });
        }
    }

    // Add current user prompt
    let prompt = req.prompt.clone();
    messages.push(LlmMessage {
        role: LlmRole::User,
        content: prompt.clone(),
        content_parts: None,
        name: None,
        tool_calls: None,
    });

    // Store user prompt to memory
    let _ = memory.store_message(&ctx, "user", &prompt).await;

    let request = LlmRequest {
        model: req
            .model
            .unwrap_or_else(|| runtime.config.llm.default_model.clone()),
        messages,
        temperature: Some(0.7),
        max_tokens: Some(runtime.config.llm.max_tokens),
        tools: None,
        stream: false,
    };

    match llm.invoke(ctx.clone(), request).await {
        Ok(response) => {
            // Store assistant response to memory
            let _ = memory
                .store_message(&ctx, "assistant", &response.message.content)
                .await;

            Ok(Json(ChatResp {
                session_id: session_id_str,
                message: response.message.content,
                usage: Some(UsageInfo {
                    prompt_tokens: response.usage.prompt_tokens,
                    completion_tokens: response.usage.completion_tokens,
                    total_tokens: response.usage.total_tokens,
                }),
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{}", e)})),
        )),
    }
}

/// POST /v1/embed
async fn embed_handler(Json(req): Json<EmbedInput>) -> Json<EmbedOutput> {
    let dims = req._text.len();
    Json(EmbedOutput {
        embedding: vec![0.0; dims],
        dimensions: dims,
    })
}

/// POST /v1/agent/run — 使用 DefaultAgent 执行任务
pub async fn agent_run_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentRunReq>,
) -> Result<Json<AgentRunResp>, (StatusCode, Json<Value>)> {
    let runtime = &state.runtime;
    let llm = runtime.llm.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no LLM configured"})),
        )
    })?;

    let session_id = req
        .session_id
        .clone()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(LsId::new);
    let ctx = LsContext::with_session(session_id);
    let session_id_str = session_id.to_string();
    let tools = runtime.tool_registry.clone();

    // Store agent input to memory
    let memory = state
        .runtime
        .memory_manager
        .get_or_create(&session_id_str)
        .await;
    let input_str = serde_json::to_string(&req.input).unwrap_or_default();
    let _ = memory
        .store_message(&ctx, "user", &format!("[Agent Input]: {}", input_str))
        .await;

    use lingshu_traits::agent::Agent;
    let config = lingshu_backends::AgentConfig {
        model: runtime.config.llm.default_model.clone(),
        max_tokens: runtime.config.llm.max_tokens,
        enable_memory: false,
        ..lingshu_backends::AgentConfig::default()
    };

    // 发布 Agent 启动事件
    let _ = runtime
        .event_bus
        .publish(
            ctx.child(),
            lingshu_traits::event_bus::Event {
                event_id: uuid::Uuid::now_v7().to_string(),
                topic: "ls.agent.run.started".into(),
                session_id: session_id_str.clone(),
                trace_id: ctx.trace_id.to_string(),
                payload: json!({"agent_id": config.model, "input": req.input}),
                timestamp: chrono::Utc::now(),
            },
        )
        .await;

    let model_name = config.model.clone();
    let mut agent = lingshu_backends::DefaultAgent::new(config, llm.clone(), tools, None);

    // 发布 Agent 启动事件到 Plugin EventBus
    state
        .plugin_event_bus
        .publish(&lingshu_plugin::event::Event::new(
            lingshu_plugin::event::EventType::AgentStarted,
            format!("agent:{}", session_id_str),
            serde_json::json!({"session_id": session_id_str, "model": model_name}),
        ))
        .await;

    match agent.run(ctx.clone(), req.input).await {
        Ok(output) => {
            // Store agent output to memory
            let output_str = serde_json::to_string(&output.data).unwrap_or_default();
            let _ = memory
                .store_message(
                    &ctx,
                    "assistant",
                    &format!("[Agent Output]: {}", output_str),
                )
                .await;

            // 发布 Agent 完成事件
            let _ = runtime
                .event_bus
                .publish(
                    ctx.child(),
                    lingshu_traits::event_bus::Event {
                        event_id: uuid::Uuid::now_v7().to_string(),
                        topic: "ls.agent.run.completed".into(),
                        session_id: session_id_str.clone(),
                        trace_id: ctx.trace_id.to_string(),
                        payload: json!({"agent_id": output.agent_id.to_string(), "status": format!("{:?}", output.status)}),
                        timestamp: chrono::Utc::now(),
                    },
                )
                .await;

            // Publish SSE event for real-time subscribers
            state.sse_broadcaster.publish(SseEvent::new(
                "agent.run.completed",
                json!({
                    "agent_id": output.agent_id.to_string(),
                    "status": format!("{:?}", output.status),
                }),
            ));

            // 发布 Agent 完成事件到 Plugin EventBus
            state.plugin_event_bus.publish(&lingshu_plugin::event::Event::new(
                lingshu_plugin::event::EventType::AgentCompleted,
                format!("agent:{}", output.agent_id),
                serde_json::json!({"agent_id": output.agent_id.to_string(), "status": format!("{:?}", output.status)}),
            )).await;
            let status = match output.status {
                lingshu_traits::agent::AgentStatus::Completed => "completed",
                lingshu_traits::agent::AgentStatus::Failed => "failed",
                _ => "other",
            };
            Ok(Json(AgentRunResp {
                agent_id: output.agent_id.to_string(),
                status: status.into(),
                output: output.data.unwrap_or(json!({"message": "no output"})),
            }))
        }
        Err(e) => {
            // 发布 Agent 失败事件
            let _ = runtime
                .event_bus
                .publish(
                    ctx.child(),
                    lingshu_traits::event_bus::Event {
                        event_id: uuid::Uuid::now_v7().to_string(),
                        topic: "ls.agent.run.failed".into(),
                        session_id: session_id_str.clone(),
                        trace_id: ctx.trace_id.to_string(),
                        payload: json!({"error": format!("{e}")}),
                        timestamp: chrono::Utc::now(),
                    },
                )
                .await;

            // 发布 Agent 失败事件到 Plugin EventBus
            state
                .plugin_event_bus
                .publish(&lingshu_plugin::event::Event::new(
                    lingshu_plugin::event::EventType::AgentFailed(format!("{}", e)),
                    format!("agent:{}", session_id_str),
                    serde_json::json!({"session_id": session_id_str, "error": format!("{}", e)}),
                ))
                .await;
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("{e}")})),
            ))
        }
    }
}

/// GET /ws — WebSocket streaming chat
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: ws::WebSocket, state: Arc<AppState>) {
    use futures::StreamExt;
    use tokio_stream::wrappers::ReceiverStream;

    let session_id = LsId::new();
    let ctx = LsContext::with_session(session_id);
    let session_id_str = session_id.to_string();

    let memory = state
        .runtime
        .memory_manager
        .get_or_create(&session_id_str)
        .await;

    let _ = socket
        .send(ws::Message::Text(
            json!({"type": "connected", "session_id": session_id_str.clone()})
                .to_string(),
        ))
        .await;

    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            ws::Message::Text(t) => t.to_string(),
            ws::Message::Close(_) => break,
            _ => continue,
        };

        let parsed: Value = serde_json::from_str(&text).unwrap_or(json!({"prompt": text}));
        let prompt = parsed
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if prompt.is_empty() {
            let _ = socket
                .send(ws::Message::Text(
                    json!({"type": "error", "message": "empty prompt"})
                        .to_string(),
                ))
                .await;
            continue;
        }

        // Store user prompt to memory
        let _ = memory.store_message(&ctx, "user", &prompt).await;

        if let Some(llm) = &state.runtime.llm {
            let child_ctx = ctx.child();

            // Recall history and prepend
            let memory_result = memory
                .recall(&ctx, &lingshu_memory::MemoryQuery::default())
                .await
                .ok();
            let mut hist_messages: Vec<LlmMessage> = Vec::new();
            if let Some(result) = memory_result {
                for item in &result.items {
                    let role = match item.role.as_str() {
                        "system" => LlmRole::System,
                        "assistant" => LlmRole::Assistant,
                        _ => LlmRole::User,
                    };
                    hist_messages.push(LlmMessage {
                        role,
                        content: item.content.clone(),
                        content_parts: None,
                        name: None,
                        tool_calls: None,
                    });
                }
            }
            hist_messages.push(LlmMessage {
                role: LlmRole::User,
                content: prompt.clone(),
                content_parts: None,
                name: None,
                tool_calls: None,
            });

            let request = LlmRequest {
                model: state.runtime.config.llm.default_model.clone(),
                messages: hist_messages,
                temperature: Some(0.7),
                max_tokens: Some(state.runtime.config.llm.max_tokens),
                tools: None,
                stream: true,
            };

            match llm.invoke_stream(child_ctx, request).await {
                Ok(rx) => {
                    let mut stream = ReceiverStream::new(rx);
                    let mut full_content = String::new();
                    let prompt_tokens = 0u64;
                    let mut completion_tokens = 0u64;

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                if let Some(content) = &chunk.content {
                                    full_content.push_str(content);
                                    completion_tokens += 1;
                                    let _ = socket
                                        .send(ws::Message::Text(
                                            json!({
                                                "type": "chunk",
                                                "content": content,
                                            })
                                            .to_string(),
                                        ))
                                        .await;
                                }
                                if let Some(reason) = &chunk.finish_reason {
                                    // Store assistant response to memory
                                    let _ = memory
                                        .store_message(&ctx, "assistant", &full_content)
                                        .await;

                                    let _ = socket.send(ws::Message::Text(json!({
                                        "type": "done",
                                        "content": full_content,
                                        "finish_reason": reason,
                                        "usage": {
                                            "prompt_tokens": prompt_tokens,
                                            "completion_tokens": completion_tokens,
                                            "total_tokens": prompt_tokens + completion_tokens,
                                        }
                                    }).to_string())).await;
                                }
                            }
                            Err(e) => {
                                let _ = socket.send(ws::Message::Text(
                                    json!({"type": "error", "message": format!("stream error: {e}")}).to_string()
                                )).await;
                            }
                        }
                    }

                    // Ensure done is sent if stream ended without finish_reason
                    if !full_content.is_empty() {
                        let _ = memory.store_message(&ctx, "assistant", &full_content).await;

                        let _ = socket
                            .send(ws::Message::Text(
                                json!({
                                    "type": "done",
                                    "content": full_content,
                                    "finish_reason": null,
                                    "usage": {
                                        "prompt_tokens": prompt_tokens,
                                        "completion_tokens": completion_tokens,
                                        "total_tokens": prompt_tokens + completion_tokens,
                                    }
                                })
                                .to_string(),
                            ))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = socket
                        .send(ws::Message::Text(
                            json!({"type": "error", "message": format!("{e}")})
                                .to_string(),
                        ))
                        .await;
                }
            }
        } else {
            let _ = socket
                .send(ws::Message::Text(
                    json!({"type": "error", "message": "no LLM configured"})
                        .to_string(),
                ))
                .await;
        }
    }

    info!(session_id = %session_id_str, "websocket disconnected");
}

// ── v2 Real-time Handlers ──────────────────────────

/// GET /v2/chat/stream — SSE streaming chat (v2)
async fn v2_chat_stream_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let ctx = LsContext::with_session(LsId::new());
    let request = LlmRequest {
        model: state.runtime.config.llm.default_model.clone(),
        messages: vec![LlmMessage {
            role: LlmRole::User,
            content: "Hello".to_string(),
            content_parts: None,
            name: None,
            tool_calls: None,
        }],
        temperature: Some(0.7),
        max_tokens: Some(state.runtime.config.llm.max_tokens),
        tools: None,
        stream: true,
    };

    if let Some(llm) = &state.runtime.llm {
        match llm.invoke_stream(ctx, request).await {
            Ok(rx) => {
                let stream = ReceiverStream::new(rx).then(|chunk_result| {
                    futures::future::ready(match chunk_result {
                        Ok(chunk) => Ok::<Event, Infallible>(
                            Event::default().data(
                                json!({
                                    "type": "chunk",
                                    "content": chunk.content.unwrap_or_default(),
                                    "index": 0u32,
                                })
                                .to_string(),
                            ),
                        ),
                        Err(e) => Ok(Event::default()
                            .data(json!({"type": "error", "message": format!("{e}")}).to_string())
                            .event("error")),
                    })
                });
                Sse::new(stream)
                    .keep_alive(
                        axum::response::sse::KeepAlive::new()
                            .interval(std::time::Duration::from_secs(15))
                            .text("keep-alive"),
                    )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("{e}")})),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no LLM configured"})),
        )
            .into_response()
    }
}

/// GET /v2/ws — WebSocket with ConnectionManager (v2)
async fn v2_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| v2_handle_ws(socket, state))
}

async fn v2_handle_ws(mut socket: ws::WebSocket, state: Arc<AppState>) {
    use futures::StreamExt;
    use tokio_stream::wrappers::ReceiverStream;

    let session_id = LsId::new();
    let user_id = "anonymous".to_string();
    let remote_addr = "unknown".to_string();
    let user_agent = "unknown".to_string();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let conn_manager = state.ws_manager.clone();
    let sid_str = session_id.to_string();

    conn_manager
        .register(Connection::new(
            sid_str.clone(),
            user_id,
            tx,
            remote_addr,
            user_agent,
        ))
        .await;

    let _ = socket
        .send(ws::Message::Text(
            json!({"type": "connected", "session_id": sid_str.clone()})
                .to_string(),
        ))
        .await;

    let ctx = LsContext::with_session(session_id);
    let mut streaming_content = String::new();

    // Initialize memory for this session
    let memory = state.runtime.memory_manager.get_or_create(&sid_str).await;

    loop {
        tokio::select! {
            // Forward broadcast messages to WebSocket
            maybe_msg = rx.recv() => {
                match maybe_msg {
                    Some(msg) => {
                        if socket.send(ws::Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            // Receive incoming WebSocket messages
            maybe_ws = socket.recv() => {
                let msg = match maybe_ws {
                    Some(Ok(msg)) => msg,
                    _ => break,
                };

                let text = match msg {
                    ws::Message::Text(t) => t.to_string(),
                    ws::Message::Close(_) => break,
                    _ => continue,
                };

                // Try to parse as ClientMessage
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                    match client_msg {
                        ClientMessage::Cancel => {
                            conn_manager.update_state(&sid_str, ConnectionState::Cancelling).await;
                            let _ = socket.send(ws::Message::Text(
                                json!({"type": "cancelled"}).to_string(),
                            )).await;
                            continue;
                        }
                        ClientMessage::Pong { .. } => {
                            conn_manager.update_activity(&sid_str).await;
                            continue;
                        }
                        ClientMessage::Close { .. } => break,
                        _ => {}
                    }
                }

                let prompt = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v.get("prompt").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .unwrap_or(text.trim().to_string());

                if prompt.is_empty() {
                    let _ = socket.send(ws::Message::Text(
                        json!({"type": "error", "message": "empty prompt"}).to_string(),
                    )).await;
                    continue;
                }

                // Store user prompt to memory
                let _ = memory.store_message(&ctx, "user", &prompt).await;

                conn_manager.update_state(&sid_str, ConnectionState::Streaming).await;
                streaming_content.clear();

                if let Some(llm) = &state.runtime.llm {
                    let child_ctx = ctx.child();

                    // Recall conversation history from memory
                    let memory_result = memory.recall(&ctx, &lingshu_memory::MemoryQuery::default()).await.ok();
                    let mut hist_messages: Vec<LlmMessage> = Vec::new();
                    if let Some(result) = memory_result {
                        for item in &result.items {
                            let role = match item.role.as_str() {
                                "system" => LlmRole::System,
                                "assistant" => LlmRole::Assistant,
                                _ => LlmRole::User,
                            };
                            hist_messages.push(LlmMessage {
                                role,
                                content: item.content.clone(),
                                content_parts: None,
        name: None,
                                tool_calls: None,
                            });
                        }
                    }
                    hist_messages.push(LlmMessage {
                        role: LlmRole::User,
                        content: prompt.clone(),
                        content_parts: None,
        name: None,
                        tool_calls: None,
                    });

                    let request = LlmRequest {
                        model: state.runtime.config.llm.default_model.clone(),
                        messages: hist_messages,
                        temperature: Some(0.7),
                        max_tokens: Some(state.runtime.config.llm.max_tokens),
                        tools: None,
                        stream: true,
                    };

                    match llm.invoke_stream(child_ctx, request).await {
                        Ok(rx) => {
                            let mut stream = ReceiverStream::new(rx);
                            let mut completion_tokens = 0u64;

                            while let Some(chunk_result) = stream.next().await {
                                // Check for cancel
                                if conn_manager.get_state(&sid_str).await == Some(ConnectionState::Cancelling) {
                                    break;
                                }

                                match chunk_result {
                                    Ok(chunk) => {
                                        if let Some(content) = &chunk.content {
                                            streaming_content.push_str(content);
                                            completion_tokens += 1;
                                            let _ = socket.send(ws::Message::Text(
                                                json!({"type": "chunk", "content": content}).to_string(),
                                            )).await;
                                        }
                                        if let Some(reason) = &chunk.finish_reason {
                                            // Store assistant response to memory
                                            let _ = memory.store_message(&ctx, "assistant", &streaming_content).await;

                                            let _ = socket.send(ws::Message::Text(json!({
                                                "type": "done",
                                                "content": streaming_content,
                                                "finish_reason": reason,
                                                "usage": {
                                                    "prompt_tokens": 0u64,
                                                    "completion_tokens": completion_tokens,
                                                    "total_tokens": completion_tokens,
                                                }
                                            }).to_string())).await;
                                        }
                                    }
                                    Err(e) => {
                                        let _ = socket.send(ws::Message::Text(
                                            json!({"type": "error", "message": format!("stream error: {e}")}).to_string()
                                        )).await;
                                    }
                                }
                            }
                            conn_manager.update_state(&sid_str, ConnectionState::Connected).await;
                        }
                        Err(e) => {
                            let _ = socket.send(ws::Message::Text(
                                json!({"type": "error", "message": format!("{e}")}).to_string(),
                            )).await;
                        }
                    }
                } else {
                    let _ = socket.send(ws::Message::Text(
                        json!({"type": "error", "message": "no LLM configured"}).to_string(),
                    )).await;
                }
            }
        }
    }

    conn_manager.unregister(&sid_str).await;
    info!(session_id = %sid_str, "v2 websocket disconnected");
}

/// GET /v2/events — SSE system event stream
async fn v2_events_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut rx = state.sse_broadcaster.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let sse = Event::default()
                        .event(event.event.clone())
                        .data(event.data.to_string());
                    if let Some(id) = &event.id {
                        yield Ok::<_, Infallible>(sse.id(id));
                    } else {
                        yield Ok::<_, Infallible>(sse);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    yield Ok::<_, Infallible>(
                        Event::default().event("lagged").data(json!({"skipped": n}).to_string())
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// GET /v1/plugins/events — SSE plugin lifecycle event stream
pub async fn plugin_events_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut rx = state.sse_broadcaster.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Only forward plugin:event type events
                    if event.event == "plugin:event" {
                        let sse = Event::default()
                            .event("plugin:event")
                            .data(event.data.to_string());
                        if let Some(id) = &event.id {
                            yield Ok::<_, Infallible>(sse.id(id));
                        } else {
                            yield Ok::<_, Infallible>(sse);
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    yield Ok::<_, Infallible>(
                        Event::default().event("lagged").data(json!({"skipped": n}).to_string())
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ── Agent Lifecycle Handlers ────────────────────────

/// GET /v1/agents — 列出所有 Agent
pub async fn agent_list_handler(State(state): State<Arc<AppState>>) -> Json<Vec<AgentSummaryResponse>> {
    let agents = state.runtime.agent_manager.list().await;
    let items: Vec<AgentSummaryResponse> = agents
        .iter()
        .map(|a| AgentSummaryResponse {
            agent_id: a.agent_id.to_string(),
            name: a.name.clone(),
            status: format!("{:?}", a.status),
            created_at: a.created_at.to_rfc3339(),
        })
        .collect();
    Json(items)
}

/// GET /v1/agents/:id — 获取 Agent 状态
pub async fn agent_status_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<AgentSummaryResponse>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    match state.runtime.agent_manager.status(&agent_id).await {
        Ok(status) => {
            let agents = state.runtime.agent_manager.list().await;
            let agent = agents
                .iter()
                .find(|a| a.agent_id == agent_id)
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(json!({"error": "agent not found"})),
                    )
                })?;
            Ok(Json(AgentSummaryResponse {
                agent_id: agent.agent_id.to_string(),
                name: agent.name.clone(),
                status: format!("{:?}", status),
                created_at: agent.created_at.to_rfc3339(),
            }))
        }
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
    }
}

/// POST /v1/agents/:id/pause — 暂停 Agent
pub async fn agent_pause_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    let ctx = LsContext::with_session(LsId::new());
    state
        .runtime
        .agent_manager
        .pause(&agent_id, &ctx)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
        })?;

    // Publish SSE event
    state.sse_broadcaster.publish(SseEvent::new(
        "agent.state_change",
        json!({
            "agent_id": agent_id.to_string(),
            "state": "paused",
        }),
    ));
    Ok(Json(
        json!({"status": "paused", "agent_id": agent_id.to_string()}),
    ))
}

/// POST /v1/agents/:id/resume — 恢复 Agent
pub async fn agent_resume_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    let ctx = LsContext::with_session(LsId::new());
    state
        .runtime
        .agent_manager
        .resume(&agent_id, &ctx)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
        })?;

    // Publish SSE event
    state.sse_broadcaster.publish(SseEvent::new(
        "agent.state_change",
        json!({
            "agent_id": agent_id.to_string(),
            "state": "resumed",
        }),
    ));
    Ok(Json(
        json!({"status": "resumed", "agent_id": agent_id.to_string()}),
    ))
}

/// POST /v1/agents/:id/cancel — 取消 Agent
pub async fn agent_cancel_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    let ctx = LsContext::with_session(LsId::new());
    state
        .runtime
        .agent_manager
        .cancel(&agent_id, &ctx)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
        })?;

    // Publish SSE event
    state.sse_broadcaster.publish(SseEvent::new(
        "agent.state_change",
        json!({
            "agent_id": agent_id.to_string(),
            "state": "cancelled",
        }),
    ));
    Ok(Json(
        json!({"status": "cancelled", "agent_id": agent_id.to_string()}),
    ))
}

/// POST /v1/agents/:id/restart — 重启 Agent
pub async fn agent_restart_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    let ctx = LsContext::with_session(LsId::new());
    state
        .runtime
        .agent_manager
        .restart(&agent_id, &ctx)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
        })?;

    state.sse_broadcaster.publish(SseEvent::new(
        "agent.state_change",
        json!({
            "agent_id": agent_id.to_string(),
            "state": "restarted",
        }),
    ));
    Ok(Json(
        json!({"status": "restarted", "agent_id": agent_id.to_string()}),
    ))
}

/// POST /v1/agent/:id/update — 热更新 Agent 配置
#[derive(Deserialize)]
pub struct AgentConfigUpdateReq {
    pub config: serde_json::Value,
}

pub async fn agent_update_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<AgentConfigUpdateReq>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    let ctx = LsContext::with_session(LsId::new());
    state
        .runtime
        .agent_manager
        .update_config(&agent_id, &ctx, req.config)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
        })?;

    state.sse_broadcaster.publish(SseEvent::new(
        "agent.state_change",
        json!({
            "agent_id": agent_id.to_string(),
            "state": "config_updated",
        }),
    ));
    Ok(Json(
        json!({"status": "config_updated", "agent_id": agent_id.to_string()}),
    ))
}

/// DELETE /v1/agent/:id — 删除 Agent
pub async fn agent_delete_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let agent_id: LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid agent id"})),
        )
    })?;

    state
        .runtime
        .agent_manager
        .remove(&agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
        })?;

    state.sse_broadcaster.publish(SseEvent::new(
        "agent.state_change",
        json!({
            "agent_id": agent_id.to_string(),
            "state": "deleted",
        }),
    ));
    Ok(Json(
        json!({"status": "deleted", "agent_id": agent_id.to_string()}),
    ))
}

#[derive(Serialize)]
pub struct AgentSummaryResponse {
    agent_id: String,
    name: String,
    status: String,
    created_at: String,
}


/// Detailed plugin response
#[derive(Serialize)]
#[allow(dead_code)]
pub struct PluginDetailResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub plugin_type: String,
    pub status: String,
    pub loaded_at: Option<String>,
    pub permissions: Vec<String>,
}

/// Market plugin entry
#[derive(Serialize)]
#[allow(dead_code)]
pub struct PluginMarketResponse {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub download_url: String,
    pub checksum: Option<String>,
}

// ── Plugin Types ────────────────────────────────────

#[derive(Serialize)]
pub struct PluginListResponse {
    plugins: Vec<PluginResponseItem>,
    total: usize,
}

#[derive(Serialize)]
pub struct PluginResponseItem {
    id: String,
    name: String,
    version: String,
    description: String,
    author: Option<String>,
    plugin_type: String,
    status: String,
    loaded_at: Option<String>,
}

#[derive(Deserialize)]
pub struct PluginInstallRequest {
    name: String,
    version: String,
    description: String,
    author: Option<String>,
    plugin_type: Option<String>,
    permissions: Option<Vec<lingshu_traits::plugin::PluginPermission>>,
}

#[derive(Serialize)]
pub struct PluginInstallResponse {
    id: String,
    name: String,
    status: String,
}

#[derive(Serialize)]
pub struct PluginActionResponse {
    id: String,
    name: String,
    status: String,
    message: String,
}

// ── Plugin Handlers ─────────────────────────────────

/// GET /v1/plugins — 列出所有插件
pub async fn plugin_list_handler(State(state): State<Arc<AppState>>) -> Json<PluginListResponse> {
    let plugins = state.plugin_registry.list().await;
    let items: Vec<PluginResponseItem> = plugins
        .iter()
        .map(|p| PluginResponseItem {
            id: p.plugin_id.to_string(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            plugin_type: p.manifest.plugin_type.clone(),
            status: format!("{:?}", p.status),
            loaded_at: p.loaded_at.map(|t| t.to_rfc3339()),
        })
        .collect();
    let total = items.len();
    Json(PluginListResponse {
        plugins: items,
        total,
    })
}

/// POST /v1/plugins — 安装一个静态插件
pub async fn plugin_install_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PluginInstallRequest>,
) -> Result<Json<PluginInstallResponse>, (StatusCode, Json<Value>)> {
    let manifest = lingshu_traits::plugin::PluginManifest {
        name: req.name.clone(),
        version: req.version.clone(),
        description: req.description.clone(),
        author: req.author,
        homepage: None,
        license: None,
        plugin_type: req.plugin_type.unwrap_or_else(|| "static".into()),
        entry_point: None,
        permissions: req.permissions.unwrap_or_default(),
        min_api_version: Some("1.0.0".into()),
    ..Default::default()
    };

    let plugin_id = lingshu_core::LsId::new();
    let info = lingshu_traits::plugin::PluginInfo {
        plugin_id,
        manifest,
        status: lingshu_traits::plugin::PluginStatus::Installed,
        loaded_at: None,
    };

    // 创建一个简单的内联插件
    let plugin = lingshu_plugin::StaticPlugin::new(info);

    match state.plugin_registry.register(Box::new(plugin), None).await {
        Ok(id) => {
            let info = state.plugin_registry.get_info(&id).await.unwrap();
            Ok(Json(PluginInstallResponse {
                id: id.to_string(),
                name: info.manifest.name.clone(),
                status: "installed".into(),
            }))
        }
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": format!("{e}")})))),
    }
}

/// GET /v1/plugins/:id — 获取插件详情
pub async fn plugin_get_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<PluginResponseItem>, (StatusCode, Json<Value>)> {
    let plugin_id: lingshu_core::LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid plugin id"})),
        )
    })?;

    match state.plugin_registry.get_info(&plugin_id).await {
        Ok(info) => Ok(Json(PluginResponseItem {
            id: info.plugin_id.to_string(),
            name: info.manifest.name,
            version: info.manifest.version,
            description: info.manifest.description,
            author: info.manifest.author,
            plugin_type: info.manifest.plugin_type,
            status: format!("{:?}", info.status),
            loaded_at: info.loaded_at.map(|t| t.to_rfc3339()),
        })),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

/// POST /v1/plugins/:id/start — 启动插件
pub async fn plugin_start_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<PluginActionResponse>, (StatusCode, Json<Value>)> {
    let plugin_id: lingshu_core::LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid plugin id"})),
        )
    })?;

    // 先初始化
    let ctx = state.runtime.root_ctx.child();
    if let Err(e) = state.plugin_registry.init_plugin(&plugin_id, &ctx).await {
        // Ignore if already initialized
        tracing::warn!(error = %e, "plugin init skipped (may already be initialized)");
    }

    match state.plugin_registry.start_plugin(&plugin_id, &ctx).await {
        Ok(()) => {
            let info = state.plugin_registry.get_info(&plugin_id).await.unwrap();
            let name = info.manifest.name.clone();
            Ok(Json(PluginActionResponse {
                id: plugin_id.to_string(),
                name: info.manifest.name,
                status: "running".into(),
                message: format!("plugin '{name}' started"),
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

/// POST /v1/plugins/:id/stop — 停止插件
pub async fn plugin_stop_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<PluginActionResponse>, (StatusCode, Json<Value>)> {
    let plugin_id: lingshu_core::LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid plugin id"})),
        )
    })?;

    let ctx = state.runtime.root_ctx.child();
    match state.plugin_registry.stop_plugin(&plugin_id, &ctx).await {
        Ok(()) => {
            let info = state.plugin_registry.get_info(&plugin_id).await.unwrap();
            let name = info.manifest.name.clone();
            Ok(Json(PluginActionResponse {
                id: plugin_id.to_string(),
                name: info.manifest.name,
                status: "stopped".into(),
                message: format!("plugin '{name}' stopped"),
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

/// DELETE /v1/plugins/:id — 卸载插件
pub async fn plugin_uninstall_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let plugin_id: lingshu_core::LsId = id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid plugin id"})),
        )
    })?;

    match state.plugin_registry.unregister(&plugin_id).await {
        Ok(()) => Ok(Json(
            json!({"status": "uninstalled", "id": plugin_id.to_string()}),
        )),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

// ── Plugin Market API ────────────────────────────────

#[derive(Serialize)]
pub struct MarketSearchResponse {
    query: String,
    total: usize,
    plugins: Vec<MarketPluginEntry>,
}

#[derive(Deserialize)]
pub struct MarketInstallRequest {
    name: String,
    version: String,
    download_url: String,
    checksum: Option<String>,
}

#[derive(Serialize)]
pub struct MarketInstallResponse {
    name: String,
    version: String,
    path: String,
    status: String,
}

#[derive(Deserialize)]
pub struct MarketAddSourceRequest {
    source_type: String,
    source_url: String,
}

#[derive(Serialize)]
pub struct MarketSourceItem {
    source_type: String,
    source_url: String,
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct HotReloadStatusResponse {
    running: bool,
    watch_dir: String,
}

/// GET /v1/plugins/market/search — 搜索市场插件
pub async fn market_search_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<MarketSearchResponse> {
    let query = params.get("q").map(|s| s.as_str()).unwrap_or("");
    let market = state.plugin_market.read().await;
    match market.search(query).await {
        Ok(result) => Json(MarketSearchResponse {
            query: result.query,
            total: result.total,
            plugins: result.plugins,
        }),
        Err(_e) => Json(MarketSearchResponse {
            query: query.to_string(),
            total: 0,
            plugins: vec![],
        }),
    }
}

/// GET /v1/plugins/market/list — 列出市场中所有可用插件
#[allow(dead_code)]
pub async fn market_list_handler(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let market = state.plugin_market.read().await;
    let registry = state.plugin_registry.list().await;
    let installed: Vec<String> = registry.iter().map(|p| p.manifest.name.clone()).collect();
    
    let mut plugins: Vec<serde_json::Value> = Vec::new();
    
    // 从 market index 读取已注册的插件信息
    for source in market.sources() {
        let _ = source;
    }
    
    // 返回本地 market 目录中的插件列表
    let market_dir = market.install_dir();
    if market_dir.exists() && market_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(market_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let plugin_json = path.join("plugin.json");
                    if plugin_json.exists() {
                        if let Ok(data) = std::fs::read_to_string(&plugin_json) {
                            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&data) {
                                let name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let version = manifest.get("version").and_then(|v| v.as_str()).unwrap_or("");
                                let desc = manifest.get("description").and_then(|v| v.as_str()).unwrap_or("");
                                let is_installed = installed.contains(&name.to_string());
                                plugins.push(serde_json::json!({
                                    "name": name,
                                    "version": version,
                                    "description": desc,
                                    "installed": is_installed,
                                    "path": path.to_string_lossy(),
                                }));
                            }
                        }
                    }
                }
            }
        }
    }
    
    Json(serde_json::json!({
        "total": plugins.len(),
        "plugins": plugins,
    }))
}

/// DELETE /v1/plugins/market/sources/:source_type — 移除市场源
#[allow(dead_code)]
pub async fn market_remove_source_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(source_type): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let removed = state.plugin_market.write().await.remove_source(&source_type);
    Json(serde_json::json!({
        "removed": removed,
        "source_type": source_type,
    }))
}

/// POST /v1/plugins/market/install — 从市场安装插件
pub async fn market_install_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MarketInstallRequest>,
) -> Result<Json<MarketInstallResponse>, (StatusCode, Json<Value>)> {
    let entry = MarketPluginEntry {
        id: format!("{}@{}", req.name, req.version),
        name: req.name.clone(),
        version: req.version.clone(),
        description: String::new(),
        author: None,
        categories: vec![],
        tags: vec![],
        download_url: req.download_url,
        checksum: req.checksum,
        size: None,
    };

    let install_dir = state.plugin_market.read().await.install_dir().to_path_buf();

    let options = InstallOptions {
        target_dir: install_dir,
        skip_verify: false,
        force: false,
    };

    match state
        .plugin_market
        .write()
        .await
        .install(&entry, &options)
        .await
    {
        Ok(path) => Ok(Json(MarketInstallResponse {
            name: req.name,
            version: req.version,
            path: path.to_string_lossy().to_string(),
            status: "installed".into(),
        })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

/// GET /v1/plugins/market/sources — 列出市场源
pub async fn market_sources_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<MarketSourceItem>> {
    let market = state.plugin_market.read().await;
    let sources = market.sources();
    let items: Vec<MarketSourceItem> = sources.iter().map(|s| MarketSourceItem {
        source_type: s.source_type().to_string(),
        source_url: s.source_url().to_string(),
    }).collect();
    Json(items)
}

/// POST /v1/plugins/market/sources — 添加市场源
pub async fn market_add_source_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MarketAddSourceRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let source = match req.source_type.as_str() {
        "github" => RegistrySource::GitHubReleases(req.source_url.clone()),
        "url" => RegistrySource::Url(req.source_url.clone()),
        "local" => RegistrySource::LocalDir(std::path::PathBuf::from(&req.source_url)),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("unknown source type: {}", req.source_type)})),
            ));
        }
    };

    state.plugin_market.write().await.add_source(source);
    Ok(Json(json!({"status": "source_added"})))
}

/// POST /v1/plugins/market/refresh — 刷新市场源
pub async fn market_refresh_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "message": "Market sources refreshed",
    }))
}


/// POST /v1/plugins/hotreload/start — 启动热重载监控
async fn hot_reload_start_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let ctx = state.runtime.root_ctx.child();
    match state
        .hot_reload_watcher
        .start(
            state.plugin_registry.clone(),
            std::sync::Arc::new(lingshu_plugin::loader::PluginLoader::new("1.0.0")),
            ctx,
            std::sync::Arc::new(|event| {
                tracing::info!("hot-reload event: {:?}", event);
            }),
        )
        .await
    {
        Ok(()) => Ok(Json(json!({"status": "hot_reload_started"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

/// POST /v1/plugins/hotreload/stop — 停止热重载监控
async fn hot_reload_stop_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.hot_reload_watcher.stop().await {
        Ok(()) => Ok(Json(json!({"status": "hot_reload_stopped"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}

// ── BeEF Security Testing API ───────────────────────

/// GET /v1/security/beef/status — 获取 BeEF 插件状态
async fn beef_status_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.beef_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let beef_status = manager.status().await;
            Ok(Json(json!({
                "plugin": "beef-plugin",
                "version": "1.0.0",
                "status": format!("{:?}", beef_status),
                "online": matches!(beef_status, lingshu_beef_plugin::BeefStatus::Running { .. }),
            })))
        }
        None => Ok(Json(json!({
            "plugin": "beef-plugin",
            "status": "NotRegistered",
            "online": false,
        }))),
    }
}

/// POST /v1/security/beef/start — 启动 BeEF
async fn beef_start_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.beef_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            manager
                .start()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
            let beef_status = manager.status().await;
            Ok(Json(json!({
                "status": "started",
                "beef": format!("{:?}", beef_status),
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "BeEF plugin not registered"})),
        )),
    }
}

/// POST /v1/security/beef/stop — 停止 BeEF
async fn beef_stop_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.beef_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            manager
                .stop()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
            Ok(Json(json!({"status": "stopped"})))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "BeEF plugin not registered"})),
        )),
    }
}

/// POST /v1/security/beef/restart — 重启 BeEF
async fn beef_restart_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.beef_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let _ = manager.stop().await;
            manager
                .start()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
            Ok(Json(json!({"status": "restarted"})))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "BeEF plugin not registered"})),
        )),
    }
}

/// GET /v1/security/beef/hooks — 获取被钩住的浏览器列表
async fn beef_hooks_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.beef_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let beef_status = manager.status().await;
            match beef_status {
                lingshu_beef_plugin::BeefStatus::Running { port, .. } => {
                    let base_url = format!("http://127.0.0.1:{}", port);
                    let mut client = lingshu_beef_plugin::BeefClient::new(&base_url);
                    match client.login("beef", "beef").await {
                        Ok(()) => match client.list_hooks().await {
                            Ok(browsers) => Ok(Json(json!({
                                "online": true,
                                "count": browsers.len(),
                                "hooks": browsers,
                            }))),
                            Err(e) => Err((
                                StatusCode::BAD_GATEWAY,
                                Json(json!({"error": format!("BeEF API error: {e}")})),
                            )),
                        },
                        Err(e) => Err((
                            StatusCode::UNAUTHORIZED,
                            Json(json!({"error": format!("BeEF auth failed: {e}")})),
                        )),
                    }
                }
                _ => Ok(Json(json!({
                    "online": false,
                    "count": 0,
                    "hooks": [],
                    "beef_status": format!("{:?}", beef_status),
                }))),
            }
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "BeEF plugin not registered"})),
        )),
    }
}

// ── File Upload & Analysis (多模态) ────────────────────

/// POST /v1/files/upload — 上传文件 (Base64 JSON)
async fn upload_file_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FileUploadRequest>,
) -> Result<Json<FileRecord>, (StatusCode, Json<Value>)> {
    let file_id = uuid::Uuid::now_v7().to_string();

    // 解码 Base64
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&req.data)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("base64 decode failed: {e}")})),
            )
        })?;

    let size_bytes = decoded.len() as u64;
    let mime_type = req
        .mime_type
        .clone()
        .unwrap_or_else(|| lingshu_multimodal::FileAnalyzer::guess_mime(&req.name, &decoded));
    let file_type = format!(
        "{:?}",
        lingshu_multimodal::FileAnalyzer::classify(&mime_type)
    );

    // 分析文件 (图像)
    let analysis = if mime_type.starts_with("image/") {
        lingshu_multimodal::ImageProcessor::analyze(&decoded)
            .ok()
            .map(|a| serde_json::to_value(a).unwrap_or_default())
    } else if mime_type.starts_with("audio/") {
        lingshu_multimodal::AudioProcessor::analyze(&decoded)
            .ok()
            .map(|a| serde_json::to_value(a).unwrap_or_default())
    } else {
        None
    };

    let now = chrono::Utc::now().to_rfc3339();
    let record = FileRecord {
        id: file_id.clone(),
        name: req.name.clone(),
        mime_type,
        size_bytes,
        file_type,
        data: req.data.clone(),
        analysis,
        created_at: now,
    };

    // 存储文件记录
    {
        let mut store = state.file_store.write().await;
        store.push(record.clone());
    }

    // Publish SSE event
    state
        .sse_broadcaster
        .publish(lingshu_websocket::SseEvent::new(
            "file.uploaded",
            json!({
                "file_id": file_id,
                "name": req.name,
                "size_bytes": size_bytes,
            }),
        ));

    Ok(Json(record))
}

/// POST /v1/files/analyze — 分析已上传文件
async fn analyze_file_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FileAnalyzeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let file_id = req.file_id;
    let record = {
        let store = state.file_store.read().await;
        store.iter().find(|f| f.id == file_id).cloned()
    }
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("file '{}' not found", file_id)})),
        )
    })?;

    // 解码并分析
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&record.data)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("base64 decode failed: {e}")})),
            )
        })?;

    let result = if record.mime_type.starts_with("image/") {
        let analysis = lingshu_multimodal::ImageProcessor::analyze(&decoded).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("image analysis failed: {e}")})),
            )
        })?;
        json!({
            "type": "image",
            "analysis": analysis,
        })
    } else if record.mime_type.starts_with("audio/") {
        let info = lingshu_multimodal::AudioProcessor::analyze(&decoded).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("audio analysis failed: {e}")})),
            )
        })?;
        json!({
            "type": "audio",
            "info": info,
        })
    } else {
        let file_info =
            lingshu_multimodal::FileAnalyzer::analyze(&record.name, &decoded).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("file analysis failed: {e}")})),
                )
            })?;
        json!({
            "type": "file",
            "info": file_info,
        })
    };

    Ok(Json(result))
}

/// GET /v1/files — 列出所有上传文件
async fn list_files_handler(State(state): State<Arc<AppState>>) -> Json<FileListResponse> {
    let store = state.file_store.read().await;
    let files: Vec<FileRecord> = store.clone();
    let total = files.len();
    Json(FileListResponse { files, total })
}

/// GET /v1/files/:id — 获取文件详情
async fn get_file_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<FileRecord>, (StatusCode, Json<Value>)> {
    let store = state.file_store.read().await;
    store
        .iter()
        .find(|f| f.id == id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("file '{}' not found", id)})),
            )
        })
        .map(Json)
}

/// POST /v1/chat/multimodal — 多模态聊天 (文本+图像)
async fn multimodal_chat_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MultimodalChatRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let runtime = &state.runtime;
    let llm = runtime.llm.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "no LLM configured"})),
        )
    })?;

    let session_id = req
        .session_id
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(lingshu_core::LsId::new);
    let ctx = lingshu_core::LsContext::with_session(session_id);
    let session_id_str = session_id.to_string();

    // 获取关联文件
    let mut image_parts: Vec<String> = Vec::new();
    {
        let store = state.file_store.read().await;
        for file_id in &req.file_ids {
            if let Some(record) = store.iter().find(|f| f.id == *file_id) {
                if record.mime_type.starts_with("image/") {
                    image_parts.push(format!("data:{};base64,{}", record.mime_type, record.data));
                }
            }
        }
    }

    // 构建多模态消息内容
    let content_parts =
        lingshu_multimodal::MultimodalRag::build_multimodal_content(&req.prompt, &image_parts);

    // Recall memory
    let memory = state
        .runtime
        .memory_manager
        .get_or_create(&session_id_str)
        .await;
    let mut messages: Vec<lingshu_traits::llm::LlmMessage> = Vec::new();
    if let Ok(mem_result) = memory
        .recall(&ctx, &lingshu_memory::MemoryQuery::default())
        .await
    {
        for item in &mem_result.items {
            let role = match item.role.as_str() {
                "system" => lingshu_traits::llm::LlmRole::System,
                "assistant" => lingshu_traits::llm::LlmRole::Assistant,
                _ => lingshu_traits::llm::LlmRole::User,
            };
            messages.push(lingshu_traits::llm::LlmMessage {
                role,
                content: item.content.clone(),
                content_parts: None,
                name: None,
                tool_calls: None,
            });
        }
    }

    messages.push(lingshu_traits::llm::LlmMessage {
        role: lingshu_traits::llm::LlmRole::User,
        content: req.prompt.clone(),
        content_parts: Some(content_parts),
        name: None,
        tool_calls: None,
    });

    let _ = memory.store_message(&ctx, "user", &req.prompt).await;

    let request = lingshu_traits::llm::LlmRequest {
        model: req
            .model
            .unwrap_or_else(|| runtime.config.llm.default_model.clone()),
        messages,
        temperature: Some(0.7),
        max_tokens: Some(runtime.config.llm.max_tokens),
        tools: None,
        stream: false,
    };

    match llm.invoke(ctx.clone(), request).await {
        Ok(response) => {
            let _ = memory
                .store_message(&ctx, "assistant", &response.message.content)
                .await;

            Ok(Json(json!({
                "session_id": session_id_str,
                "message": response.message.content,
                "usage": {
                    "prompt_tokens": response.usage.prompt_tokens,
                    "completion_tokens": response.usage.completion_tokens,
                    "total_tokens": response.usage.total_tokens,
                }
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("{e}")})),
        )),
    }
}
// ── Tests ───────────────────────────────────────────

// ── MCP (Model Context Protocol) ─────────────────────


// ── Knowledge Graph API ─────────────────────────────────

/// KnowledgeGraph JSON 查询.
async fn graph_query_handler(
    State(state): State<Arc<AppState>>,
    Path(project): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cache = state.runtime.graph_cache.read().await;
    match cache.get(&project) {
        Some(graph) => Ok(Json(serde_json::to_value(graph).unwrap_or_default())),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found", "project": project})),
        )),
    }
}

/// 项目列表（有图谱缓存的）.
async fn project_list_handler(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let cache = state.runtime.graph_cache.read().await;
    Json(cache.keys().cloned().collect())
}

/// 知识图谱可视化页面（内嵌 vis-network）.
async fn graph_view_handler(
    State(state): State<Arc<AppState>>,
    Path(project): Path<String>,
) -> Result<Html<String>, (StatusCode, String)> {
    let cache = state.runtime.graph_cache.read().await;
    let graph = cache.get(&project).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "Project ':project' not found".to_string(),
        )
    })?;

    let graph_json = serde_json::to_string_pretty(graph).unwrap_or_default();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Knowledge Graph — :project</title>
<style>
  * {{{{ margin:0; padding:0; box-sizing:border-box; }}}}
  body {{{{ font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif; background:#1a1a2e; color:#eee; }}}}
  #header {{{{ padding:16px 24px; background:#16213e; border-bottom:1px solid #0f3460; display:flex; align-items:center; gap:16px; }}}}
  #header h1 {{{{ font-size:18px; font-weight:600; }}}}
  #header .stats {{{{ font-size:13px; color:#8899aa; }}}}
  #header .stats span {{{{ margin-right:16px; }}}}
  #mynetwork {{{{ width:100vw; height:calc(100vh - 56px); }}}}
  .tag {{{{ display:inline-block; padding:2px 8px; border-radius:4px; font-size:11px; margin:2px; background:#0f3460; color:#aaccff; }}}}
</style>
</head>
<body>
<div id="header">
  <h1>🔍 :project</h1>
  <div class="stats">
    <span>⬡ <span id="node-count">0</span> nodes</span>
    <span>⬌ <span id="edge-count">0</span> edges</span>
    <span id="info-text"></span>
  </div>
</div>
<div id="mynetwork"></div>
<script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
<script>
  const graphData = {graph_json};

  // Convert lingshu graph → vis-network format
  const nodes = graphData.nodes.map(n => ({{{{
    id: n.id,
    label: n.name,
    title: `<b>${{{{n.name}}}}</b><br>${{{{n.summary}}}}<br><small>${{{{n.file_path || ''}}}}</small>${{{{n.tags?.map(t => `<span class="tag">${{{{t}}}}</span>`).join('') || ''}}}}`,
    group: n.node_type,
    shape: n.node_type === 'Function' || n.node_type === 'Class' ? 'box' :
           n.node_type === 'File' ? 'ellipse' :
           n.node_type === 'Module' || n.node_type === 'Package' ? 'database' : 'dot',
    size: n.complexity === 'Complex' || n.complexity === 'VeryComplex' ? 30 :
          n.complexity === 'Moderate' ? 22 : 16,
    color: n.node_type === 'Function' ? '#e94560' :
           n.node_type === 'Class' ? '#0f3460' :
           n.node_type === 'File' ? '#16213e' :
           n.node_type === 'Module' ? '#533483' :
           n.node_type === 'Package' ? '#e94560' : '#1a1a2e',
    font: {{{{ color: '#fff', size: 12 }}}}
  }}}}));

  const edges = graphData.edges.map(e => ({{{{
    from: e.source,
    to: e.target,
    label: e.description || e.edge_type,
    arrows: 'to',
    color: {{{{ color: e.direction === 'Bidirectional' ? '#e94560' : '#0f3460', opacity: 0.6 }}}},
    width: e.weight * 3 || 1,
    font: {{{{ size: 10, color: '#8899aa', strokeWidth: 0 }}}},
    smooth: {{{{ type: 'continuous' }}}}
  }}}}));

  document.getElementById('node-count').textContent = nodes.length;
  document.getElementById('edge-count').textContent = edges.length;

  const container = document.getElementById('mynetwork');
  const data = {{{{ nodes: new vis.DataSet(nodes), edges: new vis.DataSet(edges) }}}};
  const options = {{{{
    physics: {{{{ enabled: true, solver: 'forceAtlas2Based', forceAtlas2Based: {{{{ gravitationalConstant: -40, centralGravity: 0.005, springLength: 200, springConstant: 0.02 }}}} }}}},
    layout: {{{{ improvedLayout: true }}}},
    edges: {{{{ smooth: true }}}},
    interaction: {{{{ hover: true, tooltipDelay: 200, navigationButtons: true, keyboard: true }}}},
    groups: {{{{
      Function: {{{{ color: '#e94560' }}}},
      Class: {{{{ color: '#0f3460' }}}},
      File: {{{{ color: '#16213e' }}}},
      Module: {{{{ color: '#533483' }}}},
      Package: {{{{ color: '#e94560' }}}},
      Config: {{{{ color: '#e9a460' }}}},
      Document: {{{{ color: '#60a4e9' }}}},
      Table: {{{{ color: '#60e9a4' }}}},
      Resource: {{{{ color: '#a460e9' }}}}
    }}}}
  }}}};
  const network = new vis.Network(container, data, options);

  // SSE: Real-time updates
  if (window.EventSource) {{
    const evtSource = new EventSource('/v2/events');
    evtSource.addEventListener('graph.updated', function(e) {{
      try {{
        const msg = JSON.parse(e.data);
        if (msg.project === ':project') {{
          document.getElementById('info-text').textContent = 'Updating...';
          fetch('/v1/graph/:project')
            .then(r => r.json())
            .then(newData => {{
              const newNodes = (newData.nodes || []).map(n => ({{
                id: n.id, label: n.name,
                title: '<b>' + n.name + '</b><br>' + (n.summary || '') + '<br><small>' + (n.file_path || '') + '</small>',
                group: n.node_type,
                shape: n.node_type === 'Function' || n.node_type === 'Class' ? 'box' :
                       n.node_type === 'File' ? 'ellipse' :
                       n.node_type === 'Module' || n.node_type === 'Package' ? 'database' : 'dot',
                size: n.complexity === 'Complex' || n.complexity === 'VeryComplex' ? 30 :
                      n.complexity === 'Moderate' ? 22 : 16,
                color: n.node_type === 'Function' ? '#e94560' :
                       n.node_type === 'Class' ? '#0f3460' :
                       n.node_type === 'File' ? '#16213e' :
                       n.node_type === 'Module' ? '#533483' : '#1a1a2e',
                font: {{ color: '#fff', size: 12 }}
              }}));
              const newEdges = (newData.edges || []).map(e => ({{
                from: e.source, to: e.target,
                label: e.description || e.edge_type,
                arrows: 'to',
                color: {{ color: e.direction === 'Bidirectional' ? '#e94560' : '#0f3460', opacity: 0.6 }},
                width: e.weight * 3 || 1,
                font: {{ size: 10, color: '#8899aa', strokeWidth: 0 }},
                smooth: {{ type: 'continuous' }}
              }}));
              network.setData({{ nodes: new vis.DataSet(newNodes), edges: new vis.DataSet(newEdges) }});
              document.getElementById('node-count').textContent = newNodes.length;
              document.getElementById('edge-count').textContent = newEdges.length;
              document.getElementById('info-text').textContent = 'Updated ' + new Date().toLocaleTimeString();
            }});
        }}
      }} catch(e) {{ console.error('SSE error', e); }}
    }});
    evtSource.onerror = function() {{}};
  }}
</script>
</body>
</html>"#,
        graph_json = graph_json,
    );

    Ok(Html(html))
}

/// 凭证管理 WebUI 页面.
async fn credential_ui_handler() -> Result<Html<String>, (StatusCode, String)> {
    // Use string building to avoid format!() brace/quote conflicts
    let html = String::from(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>Credential Vault</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#1a1a2e;color:#eee}
#header{padding:16px 24px;background:#16213e;border-bottom:1px solid #0f3460;display:flex;align-items:center}
#header h1{font-size:18px;font-weight:600}
#header .actions{margin-left:auto}
.btn{padding:8px 16px;border:none;border-radius:6px;cursor:pointer;font-size:13px;display:inline-block}
.btn-primary{background:#0f3460;color:#fff}
.btn-danger{background:#e94560;color:#fff}
.btn-success{background:#2d8a4e;color:#fff}
.btn-sm{padding:4px 10px;font-size:12px}
.container{max-width:1100px;margin:20px auto;padding:0 20px}
.card{background:#16213e;border-radius:10px;padding:16px;margin-bottom:12px;border:1px solid #0f3460}
.card-header{display:flex;align-items:center;gap:12px;margin-bottom:10px}
.provider-badge{display:inline-flex;padding:3px 10px;border-radius:4px;font-size:11px;font-weight:600;color:#fff}
.card-title{font-size:15px;font-weight:600;flex:1}
.card-actions{display:flex;gap:6px;margin-left:auto}
.card-body{font-size:13px;color:#8899aa;line-height:1.6}
.card-body .row{display:flex;gap:8px;margin-bottom:4px}
.card-body .label{color:#667788;min-width:80px}
.token-display{font-family:monospace;font-size:12px;background:#0d1117;padding:4px 8px;border-radius:4px;color:#aaccff;cursor:pointer}
.empty{text-align:center;padding:60px;color:#667788}
.modal-overlay{display:none;position:fixed;inset:0;background:rgba(0,0,0,0.6);z-index:1000;align-items:center;justify-content:center}
.modal-overlay.active{display:flex}
.modal{background:#1a1a2e;border:1px solid #0f3460;border-radius:12px;padding:24px;width:90%;max-width:520px;max-height:80vh;overflow-y:auto}
.modal h2{font-size:16px;margin-bottom:16px}
.form-group{margin-bottom:12px}
.form-group label{display:block;font-size:12px;color:#8899aa;margin-bottom:4px}
.form-group input,.form-group select{width:100%;padding:8px 12px;border:1px solid #0f3460;border-radius:6px;background:#0d1117;color:#eee;font-size:13px}
.form-actions{display:flex;gap:8px;justify-content:flex-end;margin-top:16px}
.toast{position:fixed;bottom:20px;right:20px;padding:12px 20px;border-radius:8px;z-index:2000;opacity:0;transition:opacity 0.3s}
.toast.show{opacity:1}
.toast.success{background:#2d8a4e;color:#fff}
.toast.error{background:#e94560;color:#fff}
.tag{display:inline-block;padding:2px 8px;border-radius:4px;font-size:11px;background:#0f3460;color:#aaccff}
</style></head><body>
<div id="header">
<h1>Credential Vault</h1>
<div class="actions">
<button class="btn btn-primary" onclick="openCreate()">+ New</button>
<button class="btn btn-sm" onclick="refresh()" style="background:#0d1117;color:#8899aa;">Refresh</button>
</div></div>
<div class="container"><div id="list"></div></div>

<div class="modal-overlay" id="modal"><div class="modal">
<h2 id="modal-title">Credential</h2>
<input type="hidden" id="edit-id">
<div class="form-group"><label>Provider</label>
<select id="f-provider">
<option value="gitee">Gitee</option><option value="codeup">Codeup</option>
<option value="coding">CODING</option><option value="gitcode">GitCode</option><option value="cnb">CNB</option>
</select></div>
<div class="form-group"><label>Type</label>
<select id="f-type">
<option value="personal_access_token">PAT</option><option value="enterprise_token">Enterprise</option>
<option value="deployment_token">Deploy</option><option value="access_token">Access</option>
</select></div>
<div class="form-group"><label>Name</label><input id="f-name" placeholder="My Token"></div>
<div class="form-group"><label>Token</label><input id="f-token" type="password"></div>
<div class="form-group"><label>Description</label><input id="f-desc" placeholder="optional"></div>
<div class="form-group"><label>Scopes (csv)</label><input id="f-scopes" placeholder="projects,users"></div>
<div class="form-actions">
<button class="btn" onclick="closeModal()" style="background:#0d1117;color:#8899aa;">Cancel</button>
<button class="btn btn-primary" onclick="save()">Save</button>
</div></div></div>
<div class="toast" id="toast"></div>
<script>
var A="/v1/credentials";
var P={gitee:"#c71d23",codeup:"#ff6a00",coding:"#0abf53",gitcode:"#292e33",cnb:"#0052d9"};
var T={personal_access_token:"PAT",enterprise_token:"ENT",deployment_token:"DEP",access_token:"ACC"};
var D=[];
function $(i){return document.getElementById(i)}
function t(m,c){var e=$("toast");e.textContent=m;e.className="toast "+c+" show";setTimeout(function(){e.classList.remove("show")},3000)}
function esc(s){if(!s)return"";return String(s).replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;")}
function refresh(){fetch(A).then(function(r){return r.json()}).then(function(d){D=d;render()}).catch(function(e){t("Load:"+e,"error")})}
function render(){
 var el=$("list"),h="";
 if(!D.length){el.innerHTML='<div class="empty"><p>No credentials. Click + New to add.</p></div>';return}
 D.forEach(function(c){
  var cl=P[c.provider]||"#0f3460",sc=c.scopes||[];
  h+='<div class="card"><div class="card-header">'+
   '<span class="provider-badge" style="background:'+cl+'">'+c.provider.toUpperCase()+'</span>'+
   '<span class="tag">'+(T[c.credential_type]||c.credential_type)+'</span>'+
   '<div class="card-title">'+esc(c.name)+'</div>'+
   '<div class="card-actions">'+
   '<button class="btn btn-sm" onclick="v(\\''+c.id+'\\')" style="background:#2d8a4e;color:#fff;">V</button>'+
   '<button class="btn btn-sm" onclick="e(\\''+c.id+'\\')" style="background:#0f3460;color:#fff;">E</button>'+
   '<button class="btn btn-sm btn-danger" onclick="d(\\''+c.id+'\\')">X</button>'+
   '</div></div><div class="card-body">'+
   '<div class="row"><span class="label">Token:</span><span class="token-display" onclick="tok(\\''+c.id+'\\',this)" data-masked="1">'+esc(c.masked_token)+'</span></div>'+
   (c.description?'<div class="row"><span class="label">Desc:</span>'+esc(c.description)+'</div>':'')+
   (c.username?'<div class="row"><span class="label">User:</span>'+esc(c.username)+'</div>':'')+
   (sc.length?'<div class="row"><span class="label">Scopes:</span>'+sc.map(function(s){return '<span class="tag">'+esc(s)+'</span>'}).join(' ')+'</div>':'')+
   '<div class="row"><span class="label">Created:</span>'+new Date(c.created_at*1000).toLocaleString()+'</div></div></div>'
 });
 el.innerHTML=h;
}
function openCreate(){$("modal-title").textContent="New Credential";$("edit-id").value="";$("f-provider").value="gitee";$("f-type").value="personal_access_token";$("f-name").value="";$("f-token").value="";$("f-desc").value="";$("f-scopes").value="";$("modal").classList.add("active")}
function closeModal(){$("modal").classList.remove("active")}
function save(){
 var id=$("edit-id").value,data={provider:$("f-provider").value,credential_type:$("f-type").value,name:$("f-name").value,token:$("f-token").value,description:$("f-desc").value||undefined,scopes:$("f-scopes").value?$("f-scopes").value.split(",").map(function(s){return s.trim()}):undefined};
 if(!data.name||!data.token){t("Name+Token required","error");return}
 var m=id?"PUT":"POST",u=id?A+"/"+id:A;
 fetch(u,{method:m,headers:{"Content-Type":"application/json"},body:JSON.stringify(data)}).then(function(r){if(!r.ok)throw Error(r.status);t(id?"Updated":"Created","success");closeModal();refresh()}).catch(function(e){t("Err:"+e,"error")})
}
function e(id){fetch(A+"/"+id).then(function(r){return r.json()}).then(function(c){$("modal-title").textContent="Edit "+c.name;$("edit-id").value=id;$("f-provider").value=c.provider;$("f-type").value=c.credential_type;$("f-name").value=c.name;$("f-token").value="";$("f-desc").value=c.description||"";$("f-scopes").value=(c.scopes||[]).join(", ");$("modal").classList.add("active")}).catch(function(e){t("Load:"+e,"error")})}
function d(id){if(!confirm("Delete?"))return;fetch(A+"/"+id,{method:"DELETE"}).then(function(){t("Deleted","success");refresh()}).catch(function(e){t("Err:"+e,"error")})}
function v(id){fetch(A+"/"+id+"/validate",{method:"POST"}).then(function(r){return r.json()}).then(function(v){t(v.valid?"OK "+v.scopes_verified.length+" scopes":"BAD "+v.message,v.valid?"success":"error")}).catch(function(e){t("Err:"+e,"error")})}
function tok(id,el){if(el.dataset.masked==="0"){el.textContent=el.dataset.mt;el.dataset.masked="1";return}fetch(A+"/"+id+"/token").then(function(r){return r.json()}).then(function(e){el.dataset.mt=el.textContent;el.textContent=e.token;el.dataset.masked="0"}).catch(function(x){t("Err:"+x,"error")})}
refresh();
</script>
</body></html>"##,
    );
    Ok(Html(html))
}

async fn graph_analyze_handler(
    State(state): State<Arc<AppState>>,
    Path(project): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let project_root = payload
        .get("project_root")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing project_root"})),
            )
        })?;

    let max_files = payload
        .get("max_files")
        .and_then(|v| v.as_u64())
        .unwrap_or(5000) as usize;
    let modified_since = payload
        .get("modified_since")
        .and_then(|v| v.as_i64())
        .map(|secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64));

    // Build progress callback that emits SSE events
    let sse = state.sse_broadcaster.clone();
    let project_name = project.clone();
    let progress_callback: Option<std::sync::Arc<dyn Fn(usize, usize) + Send + Sync>> =
        Some(std::sync::Arc::new(move |scanned, total| {
            sse.publish(lingshu_websocket::SseEvent::new(
                "graph.scan_progress",
                serde_json::json!({"project": project_name, "scanned": scanned, "total": total}),
            ));
        }));

    // Build pipeline
    let config = lingshu_orchestrator::PipelineConfig {
        project_root: project_root.to_string(),
        project_name: project.clone(),
        enable_semantic_analysis: state.runtime.llm.is_some(),
        max_files,
        modified_since,
        progress_callback,
        ..Default::default()
    };

    let mut pipeline = lingshu_orchestrator::CodeUnderstandingPipeline::new(config);

    // If LLM is available, wire it in
    if let Some(ref llm) = state.runtime.llm {
        let llm_config = lingshu_code_analyzer::LlmAnalyzerConfig {
            model: state.runtime.config.llm.default_model.clone(),
            ..Default::default()
        };
        let analyzer = Arc::new(lingshu_code_analyzer::LlmAnalyzer::new(
            llm.clone(),
            llm_config,
        ));
        pipeline = pipeline.with_llm_analyzer(analyzer);
    }

    // Run pipeline
    let (graph, report, _enrichment_queue, _collector) = pipeline.run().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    // Store in cache + persist to SQLite
    {
        let mut cache = state.runtime.graph_cache.write().await;
        cache.insert(project.clone(), graph.clone());
    }
    // Persist to SQLite (async, best-effort)
    if let Err(e) = state.runtime.graph_store.save(&project, &graph).await {
        tracing::warn!(error = %e, project = %project, "failed to persist graph to store");
    }

    // Audit log
    {
        let entry = AuditEntry::new(
            AuditEventType::Custom("graph.analyzed".into()),
            "graph.analyzed",
            "api",
            "project",
            &project,
            &format!(
                "Analyzed project: {} files, {} nodes, {} edges",
                report.files_scanned, report.graph_nodes, report.graph_edges
            ),
        );
        let _ = state.runtime.audit_log.append(entry).await;
    }

    // Broadcast WS/SSE
    let event = lingshu_websocket::broadcast::events::graph_updated(
        &project,
        graph.nodes.len(),
        graph.edges.len(),
    );
    state.sse_broadcaster.publish(event);

    // Also broadcast via WS
    use lingshu_websocket::ServerMessage;
    state
        .ws_manager
        .broadcast(&ServerMessage::Event {
            event: "graph.updated".into(),
            data: serde_json::json!({
                "project": project,
                "node_count": graph.nodes.len(),
                "edge_count": graph.edges.len(),
                "nodes": graph.nodes,
                "edges": graph.edges,
            }),
        })
        .await;

    Ok(Json(serde_json::json!({
        "project": project,
        "files_scanned": report.files_scanned,
        "graph_nodes": report.graph_nodes,
        "graph_edges": report.graph_edges,
        "duration_ms": report.duration_ms,
        "enrichment_submitted": report.enrichment_submitted,
    })))
}

// ── Credential Vault Handlers ────────────────────────

/// 列出所有凭证（摘要）.
async fn credential_list_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<lingshu_credentials::CredentialSummary>>, (StatusCode, String)> {
    state
        .credential_manager
        .list()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// 创建凭证.
async fn credential_create_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<lingshu_credentials::CreateCredentialRequest>,
) -> Result<(StatusCode, Json<lingshu_credentials::CredentialSummary>), (StatusCode, String)> {
    state
        .credential_manager
        .create(req)
        .await
        .map(|c| (StatusCode::CREATED, Json(c)))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

/// 获取凭证摘要.
async fn credential_get_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<lingshu_credentials::CredentialSummary>, (StatusCode, String)> {
    match state.credential_manager.get_summary(&id) {
        Ok(summary) => Ok(Json(summary)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err((StatusCode::NOT_FOUND, msg))
            } else {
                Err((StatusCode::INTERNAL_SERVER_ERROR, msg))
            }
        }
    }
}

/// 更新凭证.
async fn credential_update_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<lingshu_credentials::UpdateCredentialRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    state
        .credential_manager
        .update(&id, req)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

/// 删除凭证.
async fn credential_delete_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .credential_manager
        .delete(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// 获取凭证 token（敏感操作）.
async fn credential_token_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<lingshu_credentials::CredentialEntry>, (StatusCode, String)> {
    match state.credential_manager.get_token(&id) {
        Ok(Some(entry)) => Ok(Json(entry)),
        Ok(None) => Err((StatusCode::NOT_FOUND, "credential not found: :id".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// 验证凭证.
async fn credential_validate_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<lingshu_credentials::CredentialValidation>, (StatusCode, String)> {
    state
        .credential_manager
        .validate(&id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

/// 列出支持的提供商.
async fn credential_providers_handler() -> Json<Vec<&'static str>> {
    Json(vec!["gitee", "codeup", "coding", "gitcode", "cnb"])
}

// ── Evaluator API ───────────────────────────────────

/// LLM-backed evaluable target for running test suites.
struct LlmEvaluable {
    llm: std::sync::Arc<dyn lingshu_traits::llm::Llm>,
    model: String,
}

#[async_trait::async_trait]
impl lingshu_evaluator::Evaluable for LlmEvaluable {
    async fn execute(
        &self,
        ctx: &LsContext,
        case: &lingshu_evaluator::TestCase,
    ) -> LsResult<lingshu_evaluator::ExecutedOutput> {
        let req = lingshu_traits::llm::LlmRequest {
            model: self.model.clone(),
            messages: vec![lingshu_traits::llm::LlmMessage {
                role: lingshu_traits::llm::LlmRole::User,
                content: case.input.to_string(),
                name: None,
                content_parts: None,
                tool_calls: None,
            }],
            temperature: None,
            max_tokens: Some(4096),
            stream: false,
            tools: None,
        };
        let start = std::time::Instant::now();
        let resp = self
            .llm
            .invoke(ctx.clone(), req)
            .await
            .map_err(|e| LsError::Internal(format!("LLM invoke error: {e}")))?;
        let latency = start.elapsed();
        Ok(lingshu_evaluator::ExecutedOutput {
            output: serde_json::json!(resp.message.content),
            latency,
            input_tokens: resp.usage.prompt_tokens,
            output_tokens: resp.usage.completion_tokens,
            cost: 0.0,
        })
    }

    fn target_name(&self) -> &str {
        &self.model
    }

    fn target_version(&self) -> &str {
        "1.0.0"
    }
}

/// 运行评测套件.
pub async fn eval_run_handler(
    State(state): State<Arc<AppState>>,
    Json(suite): Json<lingshu_evaluator::TestSuite>,
) -> Json<lingshu_evaluator::EvaluationResult> {
    let ctx = state.runtime.root_ctx.clone();
    match &state.runtime.llm {
        Some(llm) => {
            let target = std::sync::Arc::new(LlmEvaluable {
                llm: llm.clone(),
                model: state.runtime.config.llm.default_model.clone(),
            });
            let runner = lingshu_evaluator::EvalRunner::new(
                target,
                lingshu_evaluator::EvalConfig::default(),
            );
            let result = runner.run_suite(&suite, &ctx).await;
            *state.runtime.eval_store.write().await = Some(result.clone());
            result
        }
        None => lingshu_evaluator::EvaluationResult {
            id: LsId::new(),
            suite_name: suite.name,
            target_name: "none".into(),
            target_version: String::new(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: std::time::Duration::ZERO,
            total_cases: 0,
            passed_cases: 0,
            failed_cases: 0,
            overall_score: 0.0,
            weighted_score: 0.0,
            metrics: lingshu_evaluator::MetricsSummary::default(),
            case_results: vec![],
            metadata: std::collections::HashMap::new(),
        },
    }
    .into()
}

/// 获取最新评测结果.
pub async fn eval_result_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<lingshu_evaluator::EvaluationResult>, (StatusCode, String)> {
    let store = state.runtime.eval_store.read().await;
    store
        .clone()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "No evaluation results available".to_string(),
            )
        })
        .map(Json)
}

/// 回归检测 — 对比当前与基线结果.
pub async fn eval_regression_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegressionRequest>,
) -> Json<lingshu_evaluator::RegressionResult> {
    let current = state
        .runtime
        .eval_store
        .read()
        .await
        .clone()
        .unwrap_or_else(|| lingshu_evaluator::EvaluationResult {
            id: LsId::new(),
            suite_name: String::new(),
            target_name: String::new(),
            target_version: String::new(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: std::time::Duration::ZERO,
            total_cases: 0,
            passed_cases: 0,
            failed_cases: 0,
            overall_score: 0.0,
            weighted_score: 0.0,
            metrics: lingshu_evaluator::MetricsSummary::default(),
            case_results: vec![],
            metadata: std::collections::HashMap::new(),
        });
    let baseline = req.baseline.unwrap_or(current.clone());
    let thresholds = lingshu_evaluator::RegressionThresholds::default();
    Json(lingshu_evaluator::RegressionDetector::detect(
        &current,
        &baseline,
        &thresholds,
    ))
}

/// 回归检测请求体.
#[derive(serde::Deserialize)]
pub struct RegressionRequest {
    baseline: Option<lingshu_evaluator::EvaluationResult>,
}

// ── Federation API ─────────────────────────────────

/// 获取联邦状态.
pub async fn federation_status_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let stats = state.runtime.federation.stats().await;
    let nodes = state.runtime.federation.online_nodes().await;
    Json(serde_json::json!({
        "enabled": state.runtime.federation.config.enabled,
        "cluster_name": state.runtime.federation.config.cluster_name,
        "listen_addr": state.runtime.federation.config.listen_addr.to_string(),
        "topology": state.runtime.federation.config.topology.as_str(),
        "stats": {
            "connected_nodes": stats.connected_nodes,
            "total_nodes": stats.total_nodes,
            "active_links": stats.active_links,
            "total_messages": stats.total_messages,
            "total_errors": stats.total_errors,
            "uptime_seconds": stats.uptime_seconds,
        },
        "online_nodes": nodes.iter().map(|n| serde_json::json!({
            "cluster_id": n.cluster_id.to_string(),
            "name": n.name,
            "version": n.version,
            "addrs": n.addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            "status": format!("{:?}", n.status),
            "capabilities": n.capabilities.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

/// 在线节点列表.
pub async fn federation_nodes_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<serde_json::Value>> {
    let nodes = state.runtime.federation.online_nodes().await;
    Json(
        nodes
            .into_iter()
            .map(|n| {
                serde_json::json!({
                    "cluster_id": n.cluster_id.to_string(),
                    "name": n.name,
                    "version": n.version,
                    "addrs": n.addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                    "status": format!("{:?}", n.status),
                    "capabilities": n.capabilities.into_iter().map(|c| serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "version": c.version,
                        "description": c.description,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// 远程执行请求.
#[derive(serde::Deserialize)]
pub struct FederationExecRequest {
    target_cluster: String,
    target: String,
    payload: serde_json::Value,
    timeout_secs: Option<u64>,
}

/// 远程执行.
pub async fn federation_execute_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FederationExecRequest>,
) -> Result<Json<lingshu_federation::RemoteExecResponse>, (StatusCode, String)> {
    if !state.runtime.federation.config.enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Federation is not enabled".to_string(),
        ));
    }
    let result = state
        .runtime
        .federation
        .execute(
            &req.target_cluster,
            &req.target,
            req.payload,
            req.timeout_secs.unwrap_or(30),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(result))
}



// ── Audit API Handlers ────────────────────────────

/// GET /v1/audit/logs — 查询审计日志
async fn audit_query_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut qb = lingshu_audit::AuditQueryBuilder::new();
    if let Some(actor) = params.get("actor") {
        qb = qb.with_actor(actor);
    }
    if let Some(event_type) = params.get("event_type") {
        let et = match event_type.as_str() {
            "user_login" => lingshu_audit::AuditEventType::UserLogin,
            "user_logout" => lingshu_audit::AuditEventType::UserLogout,
            "api_call" => lingshu_audit::AuditEventType::ApiCall,
            "agent_execution" => lingshu_audit::AuditEventType::AgentExecution,
            "admin_action" => lingshu_audit::AuditEventType::AdminAction,
            "config_change" => lingshu_audit::AuditEventType::ConfigChange,
            "permission_change" => lingshu_audit::AuditEventType::PermissionChange,
            "system" => lingshu_audit::AuditEventType::System,
            _ => lingshu_audit::AuditEventType::Custom(event_type.clone()),
        };
        qb = qb.with_event_type(et);
    }
    if let Some(name) = params.get("event_name") {
        qb = qb.with_event_name(name);
    }
    if let Some(resource) = params.get("resource_type") {
        qb = qb.with_resource_type(resource);
    }
    if let Some(result) = params.get("result") {
        qb = qb.with_result(result);
    }
    if let Some(offset) = params.get("offset").and_then(|v| v.parse::<u64>().ok()) {
        qb = qb.with_offset(offset);
    }
    if let Some(limit) = params.get("limit").and_then(|v| v.parse::<u64>().ok()) {
        qb = qb.with_limit(limit);
    }

    let query = qb.build();
    let entries = state.runtime.audit_log.query(&query).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    Ok(Json(json!({
        "entries": entries,
        "total": entries.len(),
    })))
}

// ── Tenant API Handlers (Multi-Tenant) ─────────────

/// GET /v1/tenant/orgs — 列出所有组织
async fn tenant_list_orgs_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.list_organizations().await {
        Ok(orgs) => Ok(Json(json!(orgs))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))),
    }
}

/// POST /v1/tenant/orgs — 创建组织
async fn tenant_create_org_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<lingshu_tenant::CreateOrganizationRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.create_organization(&req.name, &req.slug, "system").await {
        Ok(org) => Ok(Json(json!(org))),
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": e.to_string()})))),
    }
}

/// GET /v1/tenant/orgs/:org_id — 获取组织详情
async fn tenant_get_org_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(org_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.get_organization(&org_id).await {
        Ok(org) => Ok(Json(json!(org))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()})))),
    }
}

/// GET /v1/tenant/orgs/:org_id/projects — 列出项目
async fn tenant_list_projects_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(org_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.list_projects(&org_id).await {
        Ok(projects) => Ok(Json(json!(projects))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()})))),
    }
}

/// POST /v1/tenant/orgs/:org_id/projects — 创建项目
async fn tenant_create_project_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(org_id): axum::extract::Path<String>,
    Json(req): Json<lingshu_tenant::CreateProjectRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.create_project(&org_id, &req.name, &req.description.unwrap_or_default()).await {
        Ok(project) => Ok(Json(json!(project))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()})))),
    }
}

/// GET /v1/tenant/orgs/:org_id/projects/:project_id — 获取项目
async fn tenant_get_project_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path((_org_id, project_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.get_project(&project_id).await {
        Ok(project) => Ok(Json(json!(project))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()})))),
    }
}

/// GET /v1/tenant/orgs/:org_id/users — 列出用户
async fn tenant_list_users_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(org_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.list_users(&org_id).await {
        Ok(users) => Ok(Json(json!(users))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()})))),
    }
}

/// POST /v1/tenant/orgs/:org_id/users — 邀请用户
async fn tenant_invite_user_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(org_id): axum::extract::Path<String>,
    Json(req): Json<lingshu_tenant::InviteUserRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.invite_user(&org_id, &req.email, req.role).await {
        Ok(user) => Ok(Json(json!(user))),
        Err(e) => Err((StatusCode::CONFLICT, Json(json!({"error": e.to_string()})))),
    }
}

/// GET /v1/tenant/orgs/:org_id/users/:user_id — 获取用户
async fn tenant_get_user_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path((_org_id, user_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.get_user(&user_id).await {
        Ok(user) => Ok(Json(json!(user))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()})))),
    }
}

/// DELETE /v1/tenant/orgs/:org_id/users/:user_id — 移除用户
async fn tenant_remove_user_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path((_org_id, user_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match state.tenant_manager.remove_user(&user_id).await {
        Ok(_) => Ok(Json(json!({"success": true}))),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(json!({"error": e.to_string()})))),
    }
}

/// GET /v1/tenant/stats — 租户统计
async fn tenant_stats_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let stats = state.tenant_manager.stats().await;
    Json(json!(stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_state() -> Arc<AppState> {
        use lingshu_config::settings::{LlmProvider, LsConfig};
        use lingshu_eventbus::bus::InMemoryEventBus;
        use lingshu_runtime::lifecycle::{LifecycleManager, LifecycleState};
        use lingshu_runtime::recovery::RecoveryManager;
        use lingshu_runtime::scheduler::InternalScheduler;
        use lingshu_runtime::session::SessionManager;
        use lingshu_storage::LocalStorage;

        let ctx = LsContext::with_session(LsId::new());
        let tmp = std::env::temp_dir().join("lingshu-test");
        std::fs::create_dir_all(&tmp).ok();

        let mut config = LsConfig::default();
        config.llm.provider = LlmProvider::Mock;
        config.llm.default_model = "mock".to_string();

        let lifecycle = LifecycleManager::new();
        let _ = lifecycle.transition(&ctx, LifecycleState::Initializing);
        let _ = lifecycle.transition(&ctx, LifecycleState::Running);

        let tmp_cred = tmp.join("credentials.db");
        let credential_store = std::sync::Arc::new(
            lingshu_credentials::CredentialStore::open(&tmp_cred, "test-master-key").unwrap(),
        );
        let credential_manager = std::sync::Arc::new(lingshu_credentials::CredentialManager::new(
            credential_store,
        ));

        let plugin_event_bus = Arc::new(lingshu_plugin::event::EventBus::new());
        let runtime = Arc::new(crate::LingshuRuntime {
            lifecycle,
            scheduler: InternalScheduler::new(16),
            session_mgr: SessionManager::new(3600),
            event_bus: Arc::new(InMemoryEventBus::new()),
            recovery: RecoveryManager::new(3),
            storage: LocalStorage::new(tmp),
            config,
            llm: Some(Arc::new(lingshu_backends::mock_llm::MockLlm::new())),
            service_key: None,
            root_ctx: ctx,
            tool_registry: Arc::new(tokio::sync::RwLock::new(
                lingshu_runtime::ToolRegistry::new(),
            )),
            agent_manager: lingshu_runtime::AgentManager::new(),
            memory_manager: lingshu_memory::SessionMemoryManager::default(),
            mcp_server: Arc::new(lingshu_mcp::McpServer::new()),
            graph_store: std::sync::Arc::new(
                lingshu_knowledge_graph::GraphStore::in_memory().unwrap(),
            ),
            graph_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            rate_limiter: std::sync::Arc::new(lingshu_ratelimit::MultiRateLimiter::new()),
            audit_log: std::sync::Arc::new(lingshu_audit::AuditLog::new()),
            prompt_registry: std::sync::Arc::new(lingshu_prompt::PromptRegistry::new()),
            billing: std::sync::Arc::new(
                lingshu_billing::BillingSystem::new(vec![]).unwrap_or_else(|_| {
                    let tracker = std::sync::Arc::new(lingshu_billing::UsageTracker::new());
                    let quota_mgr = std::sync::Arc::new(lingshu_billing::QuotaManager::new(vec![]));
                    let report_gen =
                        std::sync::Arc::new(lingshu_billing::ReportGenerator::new(tracker.clone()));
                    lingshu_billing::BillingSystem {
                        tracker,
                        quota_manager: quota_mgr,
                        report_generator: report_gen,
                    }
                }),
            ),
            credential_manager,
            eval_store: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            federation: std::sync::Arc::new(
                lingshu_federation::Federation::new(
                    lingshu_core::LsId::new(),
                    lingshu_federation::FederationConfig::default(),
                )
                .await,
            ),
            config_rx: {
                let (_, rx) = tokio::sync::broadcast::channel(16);
                rx
            },
            start_time: std::time::Instant::now(),
            chidori_recovery: None,
            autoagents: None,
            loong_adapter: None,
            channel_registry: std::sync::Arc::new(lingshu_channel::registry::ChannelRegistry::new()),
            agent_runtime: None,
        });
        let health_registry = Arc::new(lingshu_observability::health::HealthRegistry::new(
            "lingshu-test",
            "1.0.0",
        ));
        // Need credential store for test
        let tmp_cred_test = std::env::temp_dir()
            .join("lingshu-test")
            .join("credentials.db");
        std::fs::create_dir_all(tmp_cred_test.parent().unwrap()).ok();
        let test_cred_store = std::sync::Arc::new(
            lingshu_credentials::CredentialStore::open(&tmp_cred_test, "test-master-key").unwrap(),
        );
        Arc::new(AppState {
            runtime,
            plugin_event_bus: plugin_event_bus.clone(),
            plugin_registry: Arc::new(lingshu_plugin::PluginRegistry::with_event_bus(
                plugin_event_bus,
            )),
            plugin_market: tokio::sync::RwLock::new(lingshu_plugin::market::PluginMarket::new(
                vec![],
                std::path::PathBuf::from("/tmp/test-plugins"),
            )),
            hot_reload_watcher: lingshu_plugin::hot_reload::HotReloadWatcher::new(
                std::path::PathBuf::from("/tmp/test-plugins"),
            ),
            beef_manager: Arc::new(tokio::sync::RwLock::new(None)),
            watch_manager: Arc::new(tokio::sync::RwLock::new(None)),
            health_registry,
            ws_manager: Arc::new(ConnectionManager::new(300)),
            sse_broadcaster: Arc::new(SseBroadcaster::new(1024)),
            file_store: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            credential_manager: std::sync::Arc::new(lingshu_credentials::CredentialManager::new(
                test_cred_store,
            )),
            jwt_service: lingshu_security::auth::JwtService::new(
                "test-jwt-secret-for-unit-tests",
                3600,
            ),
            tenant_manager: std::sync::Arc::new(lingshu_tenant::TenantManager::new()),
            vault_client: std::sync::Arc::new(lingshu_vault::client::MockVaultClient::new()),
            tee_system: std::sync::Arc::new(
                lingshu_tee::TeeSystem::initialize().await.unwrap()
            ),
        })
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = test_state().await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], "1.0.0");
    }

    #[tokio::test]
    async fn test_version_endpoint() {
        let state = test_state().await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["version"], "1.0.0");
    }

    #[tokio::test]
    async fn test_models_endpoint() {
        let state = test_state().await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let models = body.as_array().unwrap();
        assert!(models.len() >= 4);
    }

    #[tokio::test]
    async fn test_chat_completions_handler() {
        let state = test_state().await;
        let app = build_router(state);

        let req_body = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let response = app
            .oneshot(
                Request::post("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_embeddings_handler() {
        let state = test_state().await;
        let app = build_router(state);

        let req_body = json!({
            "model": "text-embedding-3-small",
            "input": ["hello world"]
        });

        let response = app
            .oneshot(
                Request::post("/v1/embeddings")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_agent_run_handler() {
        let state = test_state().await;
        let app = build_router(state);

        let req_body = json!({
            "input": {"task": "test"}
        });

        let response = app
            .oneshot(
                Request::post("/v1/agent/run")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["status"], "completed");
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let state = test_state().await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        // Prometheus text format: empty line or HELP/TYPE/name lines
        assert!(body.is_empty() || body.contains("#") || body == "\n");
    }

    #[tokio::test]
    async fn test_plugin_list_empty() {
        let state = test_state().await;
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/plugins")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}

// ════════════════════════════════════════════════════════════════════════
// Watch Skill API Handlers
// ════════════════════════════════════════════════════════════════════════

/// GET /v1/watch/status — 获取 Watch Skill 插件状态
async fn watch_status_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let ws_status = manager.status().await;
            Ok(Json(json!({
                "plugin": "watch-plugin",
                "version": "1.0.0",
                "status": format!("{:?}", ws_status),
                "online": matches!(ws_status, lingshu_watch_plugin::WatchStatus::Running { .. }),
            })))
        }
        None => Ok(Json(json!({
            "plugin": "watch-plugin",
            "status": "NotRegistered",
            "online": false,
        }))),
    }
}

/// POST /v1/watch/start — 启动 Watch Skill API 服务
async fn watch_start_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            manager
                .start()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
            let status = manager.status().await;
            Ok(Json(json!({
                "status": "started",
                "watch_skill": format!("{:?}", status),
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Watch plugin not registered"})),
        )),
    }
}

/// POST /v1/watch/stop — 停止 Watch Skill API 服务
async fn watch_stop_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            manager
                .stop()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))))?;
            Ok(Json(json!({"status": "stopped"})))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Watch plugin not registered"})),
        )),
    }
}

/// POST /v1/watch/video — 观看视频
#[derive(Debug, Deserialize)]
struct WatchVideoRequest {
    source: String,
    question: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

async fn watch_video_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WatchVideoRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let client = lingshu_watch_plugin::WatchClient::new(manager.api_base_url());
            let watch_req = lingshu_watch_plugin::WatchRequest {
                source: req.source,
                question: req.question,
                start: req.start,
                end: req.end,
                budget: None,
                inline_frames: 2,
            };
            let resp = client
                .watch(&watch_req)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;
            Ok(Json(json!(resp)))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Watch plugin not registered or not started"})),
        )),
    }
}

/// POST /v1/watch/ask — 询问视频内容
#[derive(Debug, Deserialize)]
struct WatchAskRequest {
    video: String,
    question: String,
}

async fn watch_ask_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WatchAskRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let client = lingshu_watch_plugin::WatchClient::new(manager.api_base_url());
            let ask_req = lingshu_watch_plugin::AskRequest {
                video: req.video,
                question: req.question,
                max_frames: 6,
                inline_frames: 2,
            };
            let resp = client
                .ask(&ask_req)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;
            Ok(Json(json!(resp)))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Watch plugin not registered or not started"})),
        )),
    }
}

/// GET /v1/watch/search — 跨视频搜索
async fn watch_search_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let query = params.get("q").map(|s| s.as_str()).unwrap_or("");
    if query.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "query parameter 'q' is required"})),
        ));
    }
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let client = lingshu_watch_plugin::WatchClient::new(manager.api_base_url());
            let results = client
                .search(query)
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;
            Ok(Json(json!(results)))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Watch plugin not registered or not started"})),
        )),
    }
}

/// GET /v1/watch/videos — 列出已索引视频
async fn watch_list_videos_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let manager_lock = state.watch_manager.read().await;
    match &*manager_lock {
        Some(manager) => {
            let client = lingshu_watch_plugin::WatchClient::new(manager.api_base_url());
            let videos = client
                .list_videos()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({"error": e}))))?;
            Ok(Json(json!(videos)))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Watch plugin not registered or not started"})),
        )),
    }
}
/// 格式化 Duration 为人类可读字符串.
#[allow(dead_code)]
pub fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

// ── Vault API Handlers ─────────────────────────────

/// GET /v1/vault/health — Vault 健康检查
async fn vault_health_handler(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.vault_client.health().await {
        Ok(health) => Json(serde_json::json!({
            "success": true,
            "initialized": health.initialized,
            "sealed": health.sealed,
            "standby": health.standby,
            "cluster_name": health.cluster_name,
            "cluster_id": health.cluster_id,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// POST /v1/vault/secrets — 写入 Secret
async fn vault_write_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let path = params.get("path").map(|s| s.as_str()).unwrap_or("default");
    let data = payload.get("data")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    match state.vault_client.write_secret(path, data).await {
        Ok(resp) => Json(serde_json::json!({
            "success": true,
            "version": resp.metadata.version,
            "created_time": resp.metadata.created_time,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// GET /v1/vault/secrets/{path} — 读取 Secret
async fn vault_read_handler(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Json<serde_json::Value> {
    match state.vault_client.read_secret(&path).await {
        Ok(resp) => Json(serde_json::json!({
            "success": true,
            "data": resp.data.data,
            "version": resp.metadata.version,
            "created_time": resp.metadata.created_time,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// DELETE /v1/vault/secrets/{path} — 删除 Secret
async fn vault_delete_handler(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Json<serde_json::Value> {
    match state.vault_client.delete_secret(&path).await {
        Ok(()) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// GET /v1/vault/secrets — 列出 Secrets
async fn vault_list_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let path = params.get("path").map(|s| s.as_str()).unwrap_or("");
    match state.vault_client.list_secrets(path).await {
        Ok(keys) => Json(serde_json::json!({
            "success": true,
            "keys": keys,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// POST /v1/vault/encrypt — 加密数据
async fn vault_encrypt_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let key_name = payload.get("key_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let plaintext = payload.get("plaintext")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    use base64::Engine;
    let plaintext_b64 = base64::engine::general_purpose::STANDARD.encode(plaintext);

    match state.vault_client.encrypt(key_name, &plaintext_b64).await {
        Ok(ciphertext) => Json(serde_json::json!({
            "success": true,
            "ciphertext": ciphertext,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// POST /v1/vault/decrypt — 解密数据
async fn vault_decrypt_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let key_name = payload.get("key_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let ciphertext = payload.get("ciphertext")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match state.vault_client.decrypt(key_name, ciphertext).await {
        Ok(plaintext_b64) => {
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(&plaintext_b64) {
                Ok(decoded) => Json(serde_json::json!({
                    "success": true,
                    "plaintext": String::from_utf8_lossy(&decoded),
                })),
                Err(e) => Json(serde_json::json!({
                    "success": false,
                    "error": format!("base64 decode failed: {e}"),
                })),
            }
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// GET /v1/vault/dynamic-secret/{path} — 请求动态 Secret
async fn vault_dynamic_secret_handler(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> Json<serde_json::Value> {
    match state.vault_client.request_dynamic_secret(&path).await {
        Ok(secret) => Json(serde_json::json!({
            "success": true,
            "lease_id": secret.lease_id,
            "lease_duration": secret.lease_duration,
            "data": secret.data,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// POST /v1/vault/lease/:lease_id/renew — 续约 Lease
async fn vault_renew_lease_handler(
    State(state): State<Arc<AppState>>,
    Path(lease_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let increment = payload.get("increment").and_then(|v| v.as_u64()).unwrap_or(3600);
    match state.vault_client.renew_lease(&lease_id, increment).await {
        Ok(()) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// POST /v1/vault/lease/:lease_id/revoke — 撤销 Lease
async fn vault_revoke_lease_handler(
    State(state): State<Arc<AppState>>,
    Path(lease_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.vault_client.revoke_lease(&lease_id).await {
        Ok(()) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

// ── TEE API Handlers ───────────────────────────────

/// GET /v1/tee/health — TEE 健康状态
async fn tee_health_handler(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let tee = &state.tee_system;
    let platform = &tee.platform;

    Json(serde_json::json!({
        "success": true,
        "platform": platform,
        "platform_label": platform.label(),
        "available": platform.is_available(),
        "sgx_available": tee.sgx.is_some(),
        "tdx_available": tee.tdx.is_some(),
        "encrypted_memory_count": tee.encrypted_memory.list_ids().unwrap_or_default().len(),
        "policy_enforce": tee.policy_engine.read().unwrap().enforce,
    }))
}

/// POST /v1/tee/attest — 执行远程证明
async fn tee_attest_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let nonce = payload.get("nonce")
        .and_then(|v| v.as_str())
        .unwrap_or("default-nonce");

    match state.tee_system.attest(nonce).await {
        Ok(report) => Json(serde_json::json!({
            "success": true,
            "platform": report.platform,
            "nonce": report.nonce,
            "quote_hex": report.quote_hex,
            "is_valid": report.is_valid,
            "timestamp": report.timestamp,
            "details": report.details,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// POST /v1/tee/encrypted-memory — 加密存储数据
async fn tee_encrypted_memory_store_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("data");
    let data = payload.get("data").and_then(|v| v.as_str()).unwrap_or("");

    match state.tee_system.encrypted_memory.store(id, data.as_bytes()) {
        Ok(blob) => Json(serde_json::json!({
            "success": true,
            "id": blob.id,
            "key_id": blob.key_id,
            "created_at": blob.created_at,
            "ciphertext_size": blob.ciphertext.len(),
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// GET /v1/tee/encrypted-memory/:id — 解密读取
async fn tee_encrypted_memory_get_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.tee_system.encrypted_memory.retrieve(&id) {
        Ok(plaintext) => Json(serde_json::json!({
            "success": true,
            "id": id,
            "data": String::from_utf8_lossy(&plaintext),
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// DELETE /v1/tee/encrypted-memory/:id — 删除加密块
async fn tee_encrypted_memory_delete_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.tee_system.encrypted_memory.delete(&id) {
        Ok(()) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// GET /v1/tee/encrypted-memory — 列出加密块
async fn tee_encrypted_memory_list_handler(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.tee_system.encrypted_memory.list_ids() {
        Ok(ids) => Json(serde_json::json!({
            "success": true,
            "ids": ids,
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string(),
        })),
    }
}

/// GET /v1/tee/policy — 获取 TEE 策略配置
async fn tee_policy_get_handler(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let config = state.tee_system.policy_engine.read().unwrap().policy_config();
    Json(serde_json::json!({
        "success": true,
        "enforce": config.enforce,
        "required_operations": config.required_operations,
    }))
}
/// PUT /v1/tee/policy — 更新 TEE 策略
async fn tee_policy_update_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let enforce = payload.get("enforce").and_then(|v| v.as_bool()).unwrap_or(false);
    state.tee_system.policy_engine.write().unwrap().set_enforce(enforce);
    let config = state.tee_system.policy_engine.read().unwrap().policy_config();
    Json(serde_json::json!({"success": true, "enforce": config.enforce, "required_operations": config.required_operations}))
}
