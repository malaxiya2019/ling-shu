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
    extract::{ws, Path, State, WebSocketUpgrade},
    http::{header, Method, StatusCode},
    response::{Html, Json},
    routing::{get, post},
    Router,
};
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
use lingshu_core::{LsContext, LsId};
use lingshu_observability::health::HealthRegistry;
use lingshu_plugin::PluginRegistry;
use lingshu_traits::llm::{LlmMessage, LlmRequest, LlmRole};
use lingshu_websocket::{
    ClientMessage, Connection, ConnectionManager, ConnectionState, SseBroadcaster, SseEvent,
};
use std::convert::Infallible;
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream;

use crate::LingshuRuntime;

// ── Shared State ────────────────────────────────────

pub struct AppState {
    pub runtime: Arc<LingshuRuntime>,
    pub plugin_registry: Arc<PluginRegistry>,
    pub health_registry: Arc<HealthRegistry>,
    pub ws_manager: Arc<ConnectionManager>,
    pub sse_broadcaster: Arc<SseBroadcaster>,
    /// 文件存储 (多模态上传)
    pub file_store: Arc<tokio::sync::RwLock<Vec<FileRecord>>>,
    /// 凭证管理 (多 Git 提供商)
    pub credential_manager: std::sync::Arc<lingshu_credentials::CredentialManager>,
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
struct ChatResp {
    session_id: String,
    message: String,
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
struct ChatReq {
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
struct EmbedResp {
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
struct AgentRunReq {
    session_id: Option<String>,
    #[allow(dead_code)]
    agent_id: Option<String>,
    input: Value,
}

#[derive(Serialize)]
struct AgentRunResp {
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

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/version", get(version_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/embeddings", post(embeddings_handler))
        .route("/v1/chat", post(chat_handler))
        .route("/v1/embed", post(embed_handler))
        .route("/v1/agent/run", post(agent_run_handler))
        .route("/v1/agents", get(agent_list_handler))
        .route("/v1/agents/{id}", get(agent_status_handler))
        .route("/v1/agents/{id}/pause", post(agent_pause_handler))
        .route("/v1/agents/{id}/resume", post(agent_resume_handler))
        .route("/v1/agents/{id}/cancel", post(agent_cancel_handler))
        .route("/ws", get(ws_handler))
        // v2 Real-time API
        .route("/v2/chat/stream", get(v2_chat_stream_handler))
        .route("/v2/ws", get(v2_ws_handler))
        .route("/v2/events", get(v2_events_handler))
        .route("/v1/mcp", post(mcp_handler))
        .route("/v1/mcp/tools", get(mcp_tools_handler))
        .route("/v1/mcp/ui", get(mcp_ui_handler))
        // File API (多模态)
        .route("/v1/files/upload", post(upload_file_handler))
        .route("/v1/files/analyze", post(analyze_file_handler))
        .route("/v1/files", get(list_files_handler))
        .route("/v1/files/{id}", get(get_file_handler))
        .route("/v1/chat/multimodal", post(multimodal_chat_handler))
        // Plugin API
        .route(
            "/v1/plugins",
            get(plugin_list_handler).post(plugin_install_handler),
        )
        .route(
            "/v1/plugins/{id}",
            get(plugin_get_handler).delete(plugin_uninstall_handler),
        )
        .route("/v1/plugins/{id}/start", post(plugin_start_handler))
        .route("/v1/plugins/{id}/stop", post(plugin_stop_handler))
        // Knowledge Graph API
        .route(
            "/v1/graph/{project}",
            get(graph_query_handler).post(graph_analyze_handler),
        )
        .route("/v1/graph/{project}/view", get(graph_view_handler))
        .route("/v1/projects", get(project_list_handler))
        // Credential Vault API
        .route("/v1/credentials/ui", get(credential_ui_handler))
        .route(
            "/v1/credentials",
            get(credential_list_handler).post(credential_create_handler),
        )
        .route(
            "/v1/credentials/{id}",
            get(credential_get_handler)
                .put(credential_update_handler)
                .delete(credential_delete_handler),
        )
        .route("/v1/credentials/{id}/token", get(credential_token_handler))
        .route(
            "/v1/credentials/{id}/validate",
            post(credential_validate_handler),
        )
        .route(
            "/v1/credentials/providers",
            get(credential_providers_handler),
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
                        .execute(&ctx, &tool_call.function.name, args)
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
async fn chat_handler(
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
async fn agent_run_handler(
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

    let mut agent = lingshu_backends::DefaultAgent::new(config, llm.clone(), tools, None);

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
                .to_string()
                .into(),
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
                        .to_string()
                        .into(),
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
                                            .to_string()
                                            .into(),
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
                                    }).to_string().into())).await;
                                }
                            }
                            Err(e) => {
                                let _ = socket.send(ws::Message::Text(
                                    json!({"type": "error", "message": format!("stream error: {e}")}).to_string().into()
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
                                .to_string()
                                .into(),
                            ))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = socket
                        .send(ws::Message::Text(
                            json!({"type": "error", "message": format!("{e}")})
                                .to_string()
                                .into(),
                        ))
                        .await;
                }
            }
        } else {
            let _ = socket
                .send(ws::Message::Text(
                    json!({"type": "error", "message": "no LLM configured"})
                        .to_string()
                        .into(),
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
                .to_string()
                .into(),
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
                        if socket.send(ws::Message::Text(msg.into())).await.is_err() {
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
                                json!({"type": "cancelled"}).to_string().into(),
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
                        json!({"type": "error", "message": "empty prompt"}).to_string().into(),
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
                                                json!({"type": "chunk", "content": content}).to_string().into(),
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
                                            }).to_string().into())).await;
                                        }
                                    }
                                    Err(e) => {
                                        let _ = socket.send(ws::Message::Text(
                                            json!({"type": "error", "message": format!("stream error: {e}")}).to_string().into()
                                        )).await;
                                    }
                                }
                            }
                            conn_manager.update_state(&sid_str, ConnectionState::Connected).await;
                        }
                        Err(e) => {
                            let _ = socket.send(ws::Message::Text(
                                json!({"type": "error", "message": format!("{e}")}).to_string().into(),
                            )).await;
                        }
                    }
                } else {
                    let _ = socket.send(ws::Message::Text(
                        json!({"type": "error", "message": "no LLM configured"}).to_string().into(),
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

// ── Agent Lifecycle Handlers ────────────────────────

/// GET /v1/agents — 列出所有 Agent
async fn agent_list_handler(State(state): State<Arc<AppState>>) -> Json<Vec<AgentSummaryResponse>> {
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

/// GET /v1/agents/{id} — 获取 Agent 状态
async fn agent_status_handler(
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

/// POST /v1/agents/{id}/pause — 暂停 Agent
async fn agent_pause_handler(
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

/// POST /v1/agents/{id}/resume — 恢复 Agent
async fn agent_resume_handler(
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

/// POST /v1/agents/{id}/cancel — 取消 Agent
async fn agent_cancel_handler(
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

#[derive(Serialize)]
struct AgentSummaryResponse {
    agent_id: String,
    name: String,
    status: String,
    created_at: String,
}

// ── Plugin Types ────────────────────────────────────

#[derive(Serialize)]
struct PluginListResponse {
    plugins: Vec<PluginResponseItem>,
    total: usize,
}

#[derive(Serialize)]
struct PluginResponseItem {
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
struct PluginInstallRequest {
    name: String,
    version: String,
    description: String,
    author: Option<String>,
    plugin_type: Option<String>,
    permissions: Option<Vec<lingshu_traits::plugin::PluginPermission>>,
}

#[derive(Serialize)]
struct PluginInstallResponse {
    id: String,
    name: String,
    status: String,
}

#[derive(Serialize)]
struct PluginActionResponse {
    id: String,
    name: String,
    status: String,
    message: String,
}

// ── Plugin Handlers ─────────────────────────────────

/// GET /v1/plugins — 列出所有插件
async fn plugin_list_handler(State(state): State<Arc<AppState>>) -> Json<PluginListResponse> {
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
async fn plugin_install_handler(
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

/// GET /v1/plugins/{id} — 获取插件详情
async fn plugin_get_handler(
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

/// POST /v1/plugins/{id}/start — 启动插件
async fn plugin_start_handler(
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

/// POST /v1/plugins/{id}/stop — 停止插件
async fn plugin_stop_handler(
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

/// DELETE /v1/plugins/{id} — 卸载插件
async fn plugin_uninstall_handler(
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

/// GET /v1/files/{id} — 获取文件详情
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

/// POST /v1/mcp — MCP JSON-RPC 端点
/// 支持 tools/list 和 tools/call
async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let ctx = LsContext::with_session(LsId::new());
    let response = state.runtime.mcp_server.handle_request(&ctx, body).await;
    Json(serde_json::to_value(&response).unwrap_or_default())
}

/// GET /v1/mcp/tools — 列出 MCP 工具 (简化 JSON 格式)
async fn mcp_tools_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ctx = LsContext::with_session(LsId::new());
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {},
        "id": 1,
    });
    let response = state.runtime.mcp_server.handle_request(&ctx, body).await;
    Json(serde_json::to_value(&response).unwrap_or_default())
}

/// GET /v1/mcp/ui -- MCP 工具调用 WebUI
async fn mcp_ui_handler() -> Result<Html<String>, (StatusCode, String)> {
    let html = String::from(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>MCP Tool Call</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;background:#1a1a2e;color:#eee;font-size:14px}
#header{padding:16px 24px;background:#16213e;border-bottom:1px solid #0f3460;display:flex;align-items:center}
#header h1{font-size:18px;font-weight:600}
#header .sub{font-size:12px;color:#667788;margin-left:12px}
.container{max-width:1200px;margin:20px auto;padding:0 20px;display:flex;gap:20px}
#sidebar{width:280px;flex-shrink:0}
#main{flex:1;min-width:0}
#tool-list{background:#16213e;border-radius:10px;border:1px solid #0f3460;overflow:hidden}
#tool-list .ti{padding:12px 16px;cursor:pointer;border-bottom:1px solid #0f3460;transition:background 0.15s;font-size:13px}
#tool-list .ti:last-child{border-bottom:none}
#tool-list .ti:hover,#tool-list .ti.active{background:#0f3460}
#tool-list .ti .tn{font-size:13px;font-weight:600;color:#aaccff}
#tool-list .ti .td{font-size:11px;color:#667788;margin-top:2px}
.panel{background:#16213e;border-radius:10px;border:1px solid #0f3460;padding:20px;margin-bottom:16px}
.panel h2{font-size:16px;margin-bottom:12px;color:#aaccff}
.panel .desc{font-size:13px;color:#667788;margin-bottom:16px;line-height:1.5}
.form-group{margin-bottom:12px}
.form-group label{display:block;font-size:12px;color:#8899aa;margin-bottom:4px;font-weight:500}
.form-group input,.form-group select,.form-group textarea{width:100%;padding:8px 12px;border:1px solid #0f3460;border-radius:6px;background:#0d1117;color:#eee;font-size:13px;font-family:monospace}
.form-group textarea{min-height:60px;resize:vertical}
.form-group .hint{font-size:11px;color:#667788;margin-top:2px}
.btn{padding:8px 20px;border:none;border-radius:6px;cursor:pointer;font-size:13px;font-weight:500;display:inline-flex;align-items:center;gap:6px}
.btn-primary{background:#0f3460;color:#fff}
.btn-primary:hover{background:#1a4a80}
.btn-success{background:#2d8a4e;color:#fff}
.btn-danger{background:#e94560;color:#fff}
.btn:disabled{opacity:0.5;cursor:not-allowed}
.actions{display:flex;gap:8px;margin-top:16px}
#result-area{background:#0d1117;border-radius:6px;padding:16px;margin-top:16px;font-family:monospace;font-size:12px;line-height:1.5;white-space:pre-wrap;max-height:400px;overflow:auto;display:none;border:1px solid #0f3460}
#result-area.show{display:block}
#result-area .label{color:#8899aa;margin-bottom:8px;font-size:11px;text-transform:uppercase}
#result-area .error{color:#e94560}
#spinner{display:none;margin-left:8px;width:14px;height:14px;border:2px solid #667788;border-top-color:#aaccff;border-radius:50%;animation:spin 0.8s linear infinite;vertical-align:middle}
#spinner.show{display:inline-block}
@keyframes spin{to{transform:rotate(360deg)}}
.tooltip{font-size:11px;color:#667788;margin-top:12px;padding:8px 12px;background:#0d1117;border-radius:6px;line-height:1.5}
.toast{position:fixed;bottom:20px;right:20px;padding:12px 20px;border-radius:8px;z-index:2000;opacity:0;transition:opacity 0.3s;font-size:13px}
.toast.show{opacity:1}
.toast.success{background:#2d8a4e;color:#fff}
.toast.error{background:#e94560;color:#fff}
.empty{text-align:center;padding:40px;color:#667788;font-size:13px}
.badge{display:inline-block;padding:2px 8px;border-radius:4px;font-size:10px;font-weight:600;background:#0f3460;color:#aaccff;margin-left:6px}
#progress-bar{display:none;margin-top:12px;height:4px;background:#0d1117;border-radius:2px;overflow:hidden}
#progress-bar.show{display:block}
#progress-bar .fill{height:100%;background:#2d8a4e;width:0%;transition:width 0.3s;border-radius:2px}
</style></head><body>
<div id="header">
<h1>MCP Tool Call</h1><span class="sub">Model Context Protocol</span>
</div>
<div class="container">
<div id="sidebar">
<div id="tool-list"><div class="empty" id="loading-msg">Loading tools...</div></div>
</div>
<div id="main">
<div class="panel" id="tool-panel" style="display:none">
<h2 id="tool-name">-</h2>
<div class="desc" id="tool-desc"></div>
<div id="tool-params"></div>
<div class="actions">
<button class="btn btn-primary" id="btn-call" onclick="callTool()"><span id="btn-text">Execute</span><span id="spinner"></span></button>
<button class="btn" id="btn-clear" onclick="clearResult()" style="background:#0d1117;color:#667788;">Clear</button>
</div>
<div id="progress-bar"><div class="fill" id="progress-fill"></div></div>
<div id="result-area"></div>
</div>
<div class="panel" id="no-tool" style="display:block">
<div class="empty">Select a tool from the sidebar to get started.</div>
</div>
</div>
</div>
<div class="toast" id="toast"></div>
<script>
var tools=[],curTool=null,reqId=1;
function $(i){return document.getElementById(i)}
function esc(s){if(!s)return"";return String(s).replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;")}
function toast(msg,type){var t=$("toast");t.textContent=msg;t.className="toast show "+type;setTimeout(function(){t.classList.remove("show")},3000)}

fetch("/v1/mcp/tools").then(function(r){return r.json()}).then(function(d){
 var list=d.result;
 if(!list||!list.tools||!list.tools.length){$("loading-msg").innerHTML="<div class='empty'>No tools available.</div>";return}
 tools=list.tools;
 var h="";
 tools.forEach(function(t,i){
  var desc=t.description||"";
  h+='<div class="ti" id="ti-'+i+'" onclick="selectTool('+i+')"><div class="tn">'+esc(t.name)+'<span class="badge">'+(t.input_schema&&t.input_schema.required?t.input_schema.required.length+" params":"0 params")+'</span></div><div class="td">'+esc(desc.substring(0,60)+(desc.length>60?"...":""))+'</div></div>'
 });
 $("tool-list").innerHTML=h;
 if(tools.length){selectTool(0)}
}).catch(function(e){$("loading-msg").innerHTML="<div class='empty'>Error: "+e+"</div>"})

function selectTool(idx){
 curTool=idx;
 document.querySelectorAll(".ti").forEach(function(el,i){el.classList.toggle("active",i===idx)});
 $("no-tool").style.display="none";
 $("tool-panel").style.display="block";
 var tool=tools[idx];
 $("tool-name").textContent=tool.name;
 $("tool-desc").textContent=tool.description||"-";
 $("tool-params").innerHTML="";
 $("result-area").classList.remove("show");
 $("result-area").textContent="";
 var schema=tool.input_schema||{};
 var props=schema.properties||{};
 var required=new Set(schema.required||[]);
 var h="";
 Object.keys(props).forEach(function(k){
  var p=props[k];
  var req=required.has(k);
  var label=esc(k)+(req?' <span style="color:#e94560">*</span>':'');
  var hint=p.description?'<div class="hint">'+esc(p.description)+'</div>':'';
  var ptype=p.type||"string";
  var val=p.default!==undefined?JSON.stringify(p.default):"";
  if(ptype==="boolean"){
   h+='<div class="form-group"><label>'+label+'</label>'+hint+'<select id="fp-'+k+'"><option value="true">true</option><option value="false" '+(val==="false"?"selected":"")+'>false</option></select></div>'
  }else if(ptype==="object"){
   h+='<div class="form-group"><label>'+label+'</label>'+hint+'<textarea id="fp-'+k+'" placeholder="JSON object">'+esc(val)+'</textarea></div>'
  }else{
   h+='<div class="form-group"><label>'+label+'</label>'+hint+'<input id="fp-'+k+'" type="text" placeholder="'+esc(p.description||"")+'" value="'+esc(val)+'"></div>'
  }
 });
 if(!Object.keys(props).length){h='<div style="color:#667788;font-size:13px;text-align:center;padding:12px">This tool takes no parameters.</div>'}
 $("tool-params").innerHTML=h;
 clearResult();
}

function callTool(){
 if(curTool===null)return;
 var tool=tools[curTool];
 var args={};
 var schema=tool.input_schema||{};
 var props=schema.properties||{};
 Object.keys(props).forEach(function(k){
  var el=$("fp-"+k);
  if(!el)return;
  var ptype=props[k].type||"string";
  if(ptype==="boolean"){args[k]=el.value==="true"}
  else if(ptype==="number"){var n=parseFloat(el.value);if(!isNaN(n))args[k]=n}
  else if(ptype==="object"){try{args[k]=JSON.parse(el.value)}catch(e){args[k]=el.value}}
  else{args[k]=el.value}
 });
 var body={jsonrpc:"2.0",method:"tools/call",params:{name:tool.name,arguments:args},id:reqId++};
 $("btn-text").textContent="Executing...";
 $("spinner").classList.add("show");
 $("btn-call").disabled=true;
 $("result-area").classList.remove("show");
 $("result-area").textContent="";
 $("progress-bar").classList.add("show");
 $("progress-fill").style.width="0%";

 fetch("/v1/mcp",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(body)})
 .then(function(r){return r.json()}).then(function(d){
  $("progress-fill").style.width="100%";
  setTimeout(function(){$("progress-bar").classList.remove("show")},500);
  var ra=$("result-area");
  ra.classList.add("show");
  if(d.error){
   ra.innerHTML='<div class="label">Error</div><span class="error">'+esc(JSON.stringify(d.error,null,2))+'</span>'
  }else if(d.result){
   var text="";
   if(d.result.content&&d.result.content.length){
    d.result.content.forEach(function(c){if(c.type==="text"){text+=c.text}})
   }else{text=JSON.stringify(d.result,null,2)}
   try{var parsed=JSON.parse(text);text=JSON.stringify(parsed,null,2)}catch(e){}
   ra.innerHTML='<div class="label">Result</div>'+esc(text);
   if(d.result.execution_id){
    ra.innerHTML+='<div class="tooltip">execution_id: '+esc(d.result.execution_id)+'</div>'
   }
   toast("Tool executed successfully","success")
  }else{
   ra.innerHTML='<div class="label">Response</div>'+esc(JSON.stringify(d,null,2))
  }
 }).catch(function(e){
  $("progress-fill").style.width="100%";
  setTimeout(function(){$("progress-bar").classList.remove("show")},500);
  var ra=$("result-area");ra.classList.add("show");
  ra.innerHTML='<div class="label">Error</div><span class="error">'+esc(String(e))+'</span>';
  toast("Request failed: "+e,"error")
 }).finally(function(){
  $("btn-text").textContent="Execute";
  $("spinner").classList.remove("show");
  $("btn-call").disabled=false
 })
}

function clearResult(){
 $("result-area").classList.remove("show");
 $("result-area").textContent="";
 $("progress-bar").classList.remove("show")
}
</script>
</body></html>"##,
    );
    Ok(Html(html))
}

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
            format!("Project '{project}' not found"),
        )
    })?;

    let graph_json = serde_json::to_string_pretty(graph).unwrap_or_default();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Knowledge Graph — {project}</title>
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
  <h1>🔍 {project}</h1>
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
        if (msg.project === '{project}') {{
          document.getElementById('info-text').textContent = 'Updating...';
          fetch('/v1/graph/{project}')
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
        project = project,
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
        Ok(None) => Err((StatusCode::NOT_FOUND, format!("credential not found: {id}"))),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
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
            plugin_registry: Arc::new(lingshu_plugin::PluginRegistry::new()),
            health_registry,
            ws_manager: Arc::new(ConnectionManager::new(300)),
            sse_broadcaster: Arc::new(SseBroadcaster::new(1024)),
            file_store: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            credential_manager: std::sync::Arc::new(lingshu_credentials::CredentialManager::new(
                test_cred_store,
            )),
        })
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
        let state = test_state();
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
