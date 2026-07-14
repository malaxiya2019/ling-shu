//! API Server — 基于 axum 的 HTTP API 服务器.
//!
//! 封装 ApiHandler 为 RESTful HTTP 接口。
//! 需要启用 `api-server` feature。
//!
//! # 权限校验 (v4.2.4)
//!
//! 每个 API 端点通过 `require_permission` 路由层声明所需权限。
//! 使用 JWT Bearer Token 认证 + PermissionChecker 校验。
//!
//! ## 权限格式
//! `ls.runtime.{domain}.{action}` — 见 `api::permissions` 模块。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{header, Request, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::Stream;
use lingshu_core::{LsContext, LsId};
use lingshu_security::{auth::JwtService, permission::PermissionChecker, Permission};
use serde_json::Value;
use tower::{Layer, Service};
use tracing::{info, warn};

use crate::agent_runtime::AgentRuntime;
use crate::api::*;

// ═══════════════════════════════════════════════════
// 认证 / 权限中间件 (v4.2.4)
// ═══════════════════════════════════════════════════

/// 路由权限配置.
#[derive(Debug, Clone)]
struct RoutePermission {
    /// 该路由所需的权限字符串.
    permission: &'static str,
    /// 是否绕过认证（如健康检查）.
    skip_auth: bool,
}

/// 从请求路径和方法推断所需权限.
fn route_permission(method: &axum::http::Method, path: &str) -> RoutePermission {
    match (method.as_str(), path) {
        // 健康检查 — 无需认证
        ("GET", "/health") => RoutePermission {
            permission: "",
            skip_auth: true,
        },

        // Agent 管理
        ("GET", "/api/v1/agents") => RoutePermission {
            permission: permissions::agent::LIST,
            skip_auth: false,
        },
        ("POST", "/api/v1/agents") => RoutePermission {
            permission: permissions::agent::CREATE,
            skip_auth: false,
        },
        ("GET", p) if p.starts_with("/api/v1/agents/") && !p.contains("/action") => {
            RoutePermission {
                permission: permissions::agent::GET,
                skip_auth: false,
            }
        }
        ("DELETE", p) if p.starts_with("/api/v1/agents/") => RoutePermission {
            permission: permissions::agent::DELETE,
            skip_auth: false,
        },
        ("POST", p) if p.starts_with("/api/v1/agents/") && p.contains("/action") => {
            RoutePermission {
                permission: permissions::agent::ACTION,
                skip_auth: false,
            }
        }

        // 会话管理
        ("GET", "/api/v1/sessions") => RoutePermission {
            permission: permissions::session::LIST,
            skip_auth: false,
        },
        ("POST", "/api/v1/sessions") => RoutePermission {
            permission: permissions::session::CREATE,
            skip_auth: false,
        },
        ("DELETE", p) if p.starts_with("/api/v1/sessions/") => RoutePermission {
            permission: permissions::session::DELETE,
            skip_auth: false,
        },

        // 工具
        ("GET", "/api/v1/tools") => RoutePermission {
            permission: permissions::tool::LIST,
            skip_auth: false,
        },
        ("POST", p) if p.starts_with("/api/v1/tools/") && p.contains("/execute") => {
            RoutePermission {
                permission: permissions::tool::EXECUTE,
                skip_auth: false,
            }
        }

        // 工作流
        ("GET", "/api/v1/workflows") => RoutePermission {
            permission: permissions::workflow::LIST,
            skip_auth: false,
        },
        ("POST", p) if p.starts_with("/api/v1/workflows/") && p.contains("/execute") => {
            RoutePermission {
                permission: permissions::workflow::EXECUTE,
                skip_auth: false,
            }
        }
        ("GET", p) if p.starts_with("/api/v1/workflows/") && p.contains("/status") => {
            RoutePermission {
                permission: permissions::workflow::STATUS,
                skip_auth: false,
            }
        }

        // Runtime
        ("GET", "/api/v1/runtime/status") => RoutePermission {
            permission: permissions::runtime::STATUS,
            skip_auth: false,
        },
        ("GET", "/api/v1/runtime/health") => RoutePermission {
            permission: permissions::runtime::HEALTH,
            skip_auth: false,
        },
        ("GET", "/api/v1/runtime/config") => RoutePermission {
            permission: permissions::runtime::CONFIG,
            skip_auth: false,
        },
        ("GET", "/api/v1/runtime/stats") => RoutePermission {
            permission: permissions::runtime::STATS,
            skip_auth: false,
        },

        // 实时事件流 — 需要会话认证
        ("GET", "/api/v1/events") => RoutePermission {
            permission: permissions::runtime::STATUS,
            skip_auth: false,
        },
        ("GET", "/api/v1/ws") => RoutePermission {
            permission: permissions::runtime::STATUS,
            skip_auth: false,
        },

        _ => RoutePermission {
            permission: "",
            skip_auth: true,
        },
    }
}

/// 从 HTTP 请求中提取 Bearer Token.
fn extract_bearer_token(req: &Request<Body>) -> Option<String> {
    let auth_header = req.headers().get(header::AUTHORIZATION)?;
    let auth_str = auth_header.to_str().ok()?;
    if auth_str.len() > 7 && auth_str[..7].eq_ignore_ascii_case("Bearer ") {
        Some(auth_str[7..].to_string())
    } else {
        None
    }
}

/// 认证层 — JWT 校验 + 权限检查.
#[derive(Clone)]
pub struct AuthLayer {
    jwt_service: JwtService,
    permission_checker: Arc<PermissionChecker>,
}

impl AuthLayer {
    /// 创建认证层.
    pub fn new(jwt_service: JwtService, permission_checker: PermissionChecker) -> Self {
        Self {
            jwt_service,
            permission_checker: Arc::new(permission_checker),
        }
    }

    /// 从环境变量创建默认认证层.
    pub fn from_env() -> Self {
        let jwt_service = JwtService::from_env_or("lingshu-default-jwt-secret", 3600);
        Self {
            jwt_service,
            permission_checker: Arc::new(PermissionChecker::new()),
        }
    }
}

/// 认证中间件服务.
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    layer: AuthLayer,
}

/// 注入到请求扩展中的认证上下文.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub session_id: String,
    pub tenant_id: Option<String>,
    pub roles: Vec<String>,
}

impl<S> Service<Request<Body>> for AuthMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Send + Clone + 'static,
    S::Future: Send,
    S::Error: Into<Infallible>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // 注意: 不能在闭包中直接 move self, 先克隆需要的部分
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let route_perm = route_permission(&method, &path);
        let layer = self.layer.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // 跳过认证的路由
            if route_perm.skip_auth {
                return inner.call(req).await;
            }

            // 提取 Bearer Token
            let token = match extract_bearer_token(&req) {
                Some(t) => t,
                None => {
                    warn!("auth: missing Authorization header for {}", path);
                    let resp = (
                        StatusCode::UNAUTHORIZED,
                        serde_json::to_string(&serde_json::json!({
                            "error": "missing_authorization",
                            "message": "Authorization header with Bearer token required"
                        }))
                        .unwrap_or_default(),
                    )
                        .into_response();
                    return Ok(resp);
                }
            };

            // 验证 JWT
            let auth_result = match layer.jwt_service.authenticate(&token) {
                Ok(r) => r,
                Err(e) => {
                    warn!("auth: JWT verification failed for {}: {}", path, e);
                    let resp = (
                        StatusCode::UNAUTHORIZED,
                        serde_json::to_string(&serde_json::json!({
                            "error": "authentication_failed",
                            "message": e.to_string()
                        }))
                        .unwrap_or_default(),
                    )
                        .into_response();
                    return Ok(resp);
                }
            };

            // 检查权限
            let required_perm = match Permission::parse(route_perm.permission) {
                Ok(p) => p,
                Err(_) => {
                    warn!(
                        "auth: invalid permission format '{}' for {}",
                        route_perm.permission, path
                    );
                    let resp = (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        serde_json::to_string(&serde_json::json!({
                            "error": "invalid_permission_config",
                            "message": "server permission configuration error"
                        }))
                        .unwrap_or_default(),
                    )
                        .into_response();
                    return Ok(resp);
                }
            };

            // 将用户角色转换为 Permission 列表
            let granted_perms: Vec<Permission> = auth_result
                .roles
                .iter()
                .map(|role| Permission::new("runtime", role, "access"))
                .collect();

            if let Err(e) = layer
                .permission_checker
                .check(&granted_perms, &required_perm)
            {
                warn!(
                    "auth: permission denied for user {} on {}: {}",
                    auth_result.user_id, path, e
                );
                let resp = (
                    StatusCode::FORBIDDEN,
                    serde_json::to_string(&serde_json::json!({
                        "error": "permission_denied",
                        "message": e.to_string()
                    }))
                    .unwrap_or_default(),
                )
                    .into_response();
                return Ok(resp);
            }

            info!(
                "auth: granted {} access to {} (roles: {:?})",
                auth_result.user_id, path, auth_result.roles
            );

            // 将认证信息注入请求扩展
            let mut req = req;
            req.extensions_mut().insert(AuthContext {
                user_id: auth_result.user_id,
                session_id: auth_result.session_id,
                tenant_id: auth_result.tenant_id,
                roles: auth_result.roles,
            });

            inner.call(req).await
        })
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            layer: self.clone(),
        }
    }
}

// ═══════════════════════════════════════════════════
// API Server 状态
// ═══════════════════════════════════════════════════

/// API Server 状态.
struct ApiServerState {
    handler: ApiHandler,
}

/// 创建 API 路由器.
pub fn create_router(runtime: AgentRuntime) -> Router {
    let state = Arc::new(ApiServerState {
        handler: ApiHandler::new(runtime),
    });

    // 创建认证层
    let auth_layer = AuthLayer::from_env();

    Router::new()
        // 健康检查 (无需认证)
        .route("/health", get(health_check))
        // Agent 管理
        .route("/api/v1/agents", get(list_agents).post(create_agent))
        .route(
            "/api/v1/agents/:id",
            get(get_agent).delete(delete_agent).post(agent_action),
        )
        // 会话管理
        .route("/api/v1/sessions", get(list_sessions).post(create_session))
        .route("/api/v1/sessions/:id", delete(delete_session))
        // 工具
        .route("/api/v1/tools", get(list_tools))
        .route("/api/v1/tools/:name/execute", post(execute_tool))
        // 工作流
        .route("/api/v1/workflows", get(list_workflows))
        .route("/api/v1/workflows/:name/execute", post(execute_workflow))
        .route("/api/v1/workflows/:name/status", get(workflow_status))
        // Runtime
        .route("/api/v1/runtime/status", get(runtime_status))
        .route("/api/v1/runtime/health", get(runtime_health))
        .route("/api/v1/runtime/config", get(runtime_config))
        .route("/api/v1/runtime/stats", get(runtime_stats))
        // 实时事件流 (SSE)
        .route("/api/v1/events", get(sse_events))
        // WebSocket 实时事件流
        .route("/api/v1/ws", get(ws_handler))
        // 应用认证中间件
        .layer(auth_layer)
        .with_state(state)
}

/// 启动 HTTP 服务器.
pub async fn serve(
    runtime: AgentRuntime,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = create_router(runtime);
    info!("API Server starting on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ── 辅助：从请求扩展中提取 AuthContext ──

/// 从 axum 请求的扩展中提取认证上下文，回退到匿名会话.
#[allow(dead_code)]
fn extract_auth_context(req: &axum::extract::Request) -> LsContext {
    if let Some(auth) = req.extensions().get::<AuthContext>() {
        let session_id = uuid::Uuid::parse_str(&auth.session_id)
            .map(LsId::from)
            .unwrap_or_else(|_| LsId::new());
        let mut ctx = LsContext::with_session(session_id).with_user(&auth.user_id);
        if let Some(ref tid) = auth.tenant_id {
            ctx = ctx.with_tenant(tid);
        }
        // 角色列表存入 metadata
        ctx.metadata
            .insert("roles".to_string(), auth.roles.join(","));
        ctx
    } else {
        LsContext::with_session(LsId::new())
    }
}

// ── 处理函数 ──

async fn health_check() -> Json<Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_agents(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Agent(ApiAgentRequest {
                action: AgentAction::List,
                agent_id: None,
                agent_name: None,
                input: None,
                agent_type: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn create_agent(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Agent(ApiAgentRequest {
                action: AgentAction::Register,
                agent_id: None,
                agent_name: Some("default".to_string()),
                input: None,
                agent_type: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn get_agent(
    State(state): State<Arc<ApiServerState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Agent(ApiAgentRequest {
                action: AgentAction::Status,
                agent_id: Some(id),
                agent_name: None,
                input: None,
                agent_type: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn delete_agent(
    State(state): State<Arc<ApiServerState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Agent(ApiAgentRequest {
                action: AgentAction::Remove,
                agent_id: Some(id),
                agent_name: None,
                input: None,
                agent_type: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn agent_action(
    State(state): State<Arc<ApiServerState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Agent(ApiAgentRequest {
                action: AgentAction::Pause,
                agent_id: Some(id),
                agent_name: None,
                input: None,
                agent_type: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn list_sessions(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Session(ApiSessionRequest {
                action: SessionAction::List,
                session_id: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn create_session(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Session(ApiSessionRequest {
                action: SessionAction::Create,
                session_id: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn delete_session(
    State(state): State<Arc<ApiServerState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Session(ApiSessionRequest {
                action: SessionAction::Terminate,
                session_id: Some(id),
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn list_tools(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Tool(ApiToolRequest {
                action: ToolAction::List,
                tool_name: None,
                args: None,
                filter: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn execute_tool(
    State(state): State<Arc<ApiServerState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Tool(ApiToolRequest {
                action: ToolAction::Execute,
                tool_name: Some(name),
                args: None,
                filter: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn list_workflows(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Workflow(ApiWorkflowRequest {
                action: WorkflowAction::List,
                workflow_name: None,
                input: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn execute_workflow(
    State(state): State<Arc<ApiServerState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Workflow(ApiWorkflowRequest {
                action: WorkflowAction::Execute,
                workflow_name: Some(name),
                input: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn workflow_status(
    State(state): State<Arc<ApiServerState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Workflow(ApiWorkflowRequest {
                action: WorkflowAction::Status,
                workflow_name: Some(name),
                input: None,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn runtime_status(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Runtime(ApiRuntimeRequest {
                action: RuntimeAction::Status,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn runtime_health(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Runtime(ApiRuntimeRequest {
                action: RuntimeAction::Health,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn runtime_config(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Runtime(ApiRuntimeRequest {
                action: RuntimeAction::Config,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

async fn runtime_stats(
    State(state): State<Arc<ApiServerState>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ctx = LsContext::with_session(LsId::new());
    let resp = state
        .handler
        .handle(
            ApiRequest::Runtime(ApiRuntimeRequest {
                action: RuntimeAction::Stats,
            }),
            &ctx,
        )
        .await;
    api_response_to_json(resp)
}

// ── 实时事件流 (SSE) ──

/// SSE 事件流端点 — 客户端订阅实时 Runtime 事件.
async fn sse_events(
    State(state): State<Arc<ApiServerState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::broadcast::channel::<String>(1024);

    // 尝试订阅 Runtime EventBus，将事件转发到 SSE broadcast channel
    let runtime = state.handler.runtime();
    if let Some(bus) = runtime.event_bus().await {
        let ctx = LsContext::with_session(LsId::new());
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let _ = bus
                .subscribe(
                    ctx,
                    "ls.*",
                    Box::new(move |event| {
                        if let Ok(json) = serde_json::to_string(&event) {
                            let _ = tx2.send(json);
                        }
                        Ok(())
                    }),
                )
                .await;
        });
    }

    // 使用 futures::stream::unfold 从 broadcast::Receiver 创建流
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(data) => Some((Ok(Event::default().data(data)), rx)),
            Err(_) => {
                // 如果所有发送者都断开，尝试重新连接
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                Some((Ok(Event::default().data("")), rx))
            }
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ── WebSocket 事件流 ──

/// WebSocket 端点 — 双向实时通信.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiServerState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<ApiServerState>) {
    // 发送连接成功消息
    let welcome = serde_json::json!({
        "type": "connected",
        "message": "LingShu Runtime v4.2",
        "protocol": "json",
    });
    let _ = socket.send(Message::Text(welcome.to_string())).await;

    // 创建一个 channel 用于 EventBus → WS 转发
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let runtime = state.handler.runtime();

    // 订阅 EventBus
    if let Some(bus) = runtime.event_bus().await {
        let ctx = LsContext::with_session(LsId::new());
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let _ = bus
                .subscribe(
                    ctx,
                    "ls.*",
                    Box::new(move |event| {
                        if let Ok(json) = serde_json::to_string(&event) {
                            let _ = tx.send(json);
                        }
                        Ok(())
                    }),
                )
                .await;
        });
    }

    // 主循环：同时处理 WS 接收和 EventBus 转发
    loop {
        tokio::select! {
            // 从 EventBus 收到事件，推送到 WS
            Some(event_json) = event_rx.recv() => {
                let _ = socket.send(Message::Text(event_json)).await;
            }
            // 从 WS 收到客户端消息
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if text.contains(r#""type":"ping""#) {
                            let pong = serde_json::json!({
                                "type": "pong",
                                "timestamp": chrono::Utc::now().timestamp(),
                            });
                            let _ = socket.send(Message::Text(pong.to_string())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

// ── 响应辅助函数 ──

/// 将 ApiResponse 转换为 HTTP JSON 响应（包含结构化错误）.
fn api_response_to_json(resp: ApiResponse) -> Result<Json<Value>, (StatusCode, String)> {
    if resp.success {
        Ok(Json(serde_json::json!({
            "success": true,
            "data": resp.data.unwrap_or(Value::Null),
            "timestamp": resp.timestamp,
        })))
    } else {
        let status = if let Some(ref err) = resp.error {
            match err.code.as_str() {
                "not_found" | "session_not_found" | "tool_not_found" => StatusCode::NOT_FOUND,
                "permission_denied" | "authentication_failed" => StatusCode::FORBIDDEN,
                "invalid_argument" | "validation_error" => StatusCode::BAD_REQUEST,
                "timeout" => StatusCode::GATEWAY_TIMEOUT,
                _ => StatusCode::BAD_REQUEST,
            }
        } else {
            StatusCode::BAD_REQUEST
        };
        // 返回结构化 JSON 错误
        let body = serde_json::json!({
            "success": false,
            "error": resp.error,
            "timestamp": resp.timestamp,
        });
        Err((status, serde_json::to_string(&body).unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::AgentRuntimeConfig;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_check_no_auth() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default())
            .await
            .unwrap();
        let app = create_router(runtime);

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
    }

    #[tokio::test]
    async fn test_auth_required_for_protected_route() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default())
            .await
            .unwrap();
        let app = create_router(runtime);

        // 不带 Authorization header 请求 /api/v1/runtime/status
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/runtime/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
