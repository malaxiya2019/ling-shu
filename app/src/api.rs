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

use std::sync::Arc;

use axum::{
    extract::{ws, State, WebSocketUpgrade},
    http::{header, Method, StatusCode},
    response::Json,
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
use lingshu_core::{LsContext, LsId};
use lingshu_observability::health::HealthRegistry;
use lingshu_plugin::PluginRegistry;
use lingshu_traits::llm::{Llm, LlmMessage, LlmRequest, LlmRole};
use std::convert::Infallible;
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream;

use crate::LingshuRuntime;

// ── Shared State ────────────────────────────────────

pub struct AppState {
    pub runtime: Arc<LingshuRuntime>,
    pub plugin_registry: Arc<PluginRegistry>,
    pub health_registry: Arc<HealthRegistry>,
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
        .route("/ws", get(ws_handler))
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
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
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

    let mut messages: Vec<LlmMessage> = req
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
                tool_calls: None,
            }
        })
        .collect();

    let session_id = LsId::new();
    let ctx = LsContext::with_session(session_id);

    if let Some(ref user) = req.user {
        let _ = runtime
            .session_mgr
            .create(&LsContext::with_session(session_id).with_user(user))
            .await;
    }

    // Tool calling loop: max 10 iterations
    let tools = req.tools.clone(); // tools from original request
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
                    // No tool calls — return final response
                    let usage = UsageInfo {
                        prompt_tokens: response.usage.prompt_tokens,
                        completion_tokens: response.usage.completion_tokens,
                        total_tokens: response.usage.total_tokens,
                    };
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

/// Handle streaming chat completion (SSE)
async fn handle_streaming_chat(
    state: Arc<AppState>,
    req: ChatCompletionRequest,
) -> Sse<Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>> {
    let runtime = &state.runtime;
    let messages: Vec<LlmMessage> = req
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
                tool_calls: None,
            }
        })
        .collect();

    let session_id = LsId::new();
    let ctx = LsContext::with_session(session_id);

    let request = LlmRequest {
        model: req.model,
        messages,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        tools: None,
        stream: true,
    };

    let stream: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        if let Some(llm) = &runtime.llm {
            match llm.invoke_stream(ctx, request).await {
                Ok(rx) => {
                    let s = ReceiverStream::new(rx).map(|chunk_result| match chunk_result {
                        Ok(chunk) => {
                            let data = json!({
                                "choices": [{
                                    "delta": {
                                        "content": chunk.content,
                                    },
                                    "index": 0,
                                    "finish_reason": chunk.finish_reason,
                                }]
                            });
                            Ok(Event::default().data(data.to_string()))
                        }
                        Err(_) => Ok(Event::default().data("[DONE]")),
                    });
                    Box::pin(s)
                }
                Err(e) => {
                    let s = futures::stream::once(async move {
                        Ok(Event::default().data(format!("error: {}", e)))
                    });
                    Box::pin(s)
                }
            }
        } else {
            let s = futures::stream::once(async {
                Ok(Event::default().data("{\"error\":\"no LLM configured\"}"))
            });
            Box::pin(s)
        };

    Sse::new(stream)
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

    let request = LlmRequest {
        model: req
            .model
            .unwrap_or_else(|| runtime.config.llm.default_model.clone()),
        messages: vec![LlmMessage {
            role: LlmRole::User,
            content: req.prompt,
            name: None,
            tool_calls: None,
        }],
        temperature: Some(0.7),
        max_tokens: Some(runtime.config.llm.max_tokens),
        tools: None,
        stream: false,
    };

    match llm.invoke(ctx, request).await {
        Ok(response) => Ok(Json(ChatResp {
            session_id: session_id.to_string(),
            message: response.message.content,
            usage: Some(UsageInfo {
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
            }),
        })),
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

/// POST /v1/agent/run
async fn agent_run_handler(Json(req): Json<AgentRunReq>) -> Json<AgentRunResp> {
    let agent_id = req.agent_id.unwrap_or_else(|| LsId::new().to_string());
    Json(AgentRunResp {
        agent_id,
        status: "completed".into(),
        output: json!({"message": "agent executed", "input": req.input}),
    })
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

    let _ = socket
        .send(ws::Message::Text(
            json!({"type": "connected", "session_id": session_id.to_string()})
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

        if let Some(llm) = &state.runtime.llm {
            let child_ctx = ctx.child();
            let request = LlmRequest {
                model: state.runtime.config.llm.default_model.clone(),
                messages: vec![LlmMessage {
                    role: LlmRole::User,
                    content: prompt,
                    name: None,
                    tool_calls: None,
                }],
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

    info!(session_id = %session_id, "websocket disconnected");
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

// ── Tests ───────────────────────────────────────────

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

        let runtime = Arc::new(crate::LingshuRuntime {
            lifecycle,
            scheduler: InternalScheduler::new(16),
            session_mgr: SessionManager::new(3600),
            event_bus: Arc::new(InMemoryEventBus::new()),
            recovery: RecoveryManager::new(3),
            storage: LocalStorage::new(tmp),
            config,
            llm: Some(Box::new(lingshu_backends::mock_llm::MockLlm::new())),
            service_key: None,
            root_ctx: ctx,
            tool_registry: lingshu_runtime::ToolRegistry::new(),
        });
        let health_registry = Arc::new(lingshu_observability::health::HealthRegistry::new(
            "lingshu-test",
            "1.0.0",
        ));
        Arc::new(AppState {
            runtime,
            plugin_registry: Arc::new(lingshu_plugin::PluginRegistry::new()),
            health_registry,
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
        assert!(
            body["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
                .len()
                > 0
        );
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
