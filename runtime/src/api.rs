//! AgentRuntime API — 统一管理接口（HTTP + Event 双通道）.
//!
//! 提供 Agent Runtime 的标准化管理接口，支持：
//! - Agent 管理（注册/启动/暂停/恢复/取消）
//! - 会话管理（创建/查询/终止）
//! - 工具查询（列表/执行）
//! - 工作流管理（注册/执行/状态查询）
//! - Runtime 状态（心跳/健康检查/统计）
//!
//! 这些 API 被设计为与具体传输层无关（HTTP/WebSocket/IPC 均可适配）。
//!
//! # 指标记录 (v4.2.3)
//!
//! API Handler 内置 MetricsRegistry 集成：
//! - `handle_agent` / `handle_runtime` → 记录 `ls_agent_count` gauge
//! - `handle_tool` → 记录 `ls_tool_calls_total` counter
//! - `handle_session` / `handle_runtime` → 记录 `ls_session_count` gauge
//!
//! # 权限校验 (v4.2.4)
//!
//! 每个 API 端点关联一个 Permission 常量，由 API Server 中间件校验。
//! 权限格式: `ls.{domain}.{resource}.{action}`

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_observability::metrics::{MetricsRegistry, RuntimeMetricsCollector};
#[cfg(feature = "otel")]
use lingshu_observability::RuntimeOtelMetrics;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_runtime::AgentRuntime;

// ── 权限常量 (v4.2.4) ──

/// API 权限常量.
pub mod permissions {
    /// Agent 权限域.
    pub mod agent {
        pub const LIST: &str = "ls.runtime.agent.list";
        pub const CREATE: &str = "ls.runtime.agent.create";
        pub const GET: &str = "ls.runtime.agent.get";
        pub const DELETE: &str = "ls.runtime.agent.delete";
        pub const ACTION: &str = "ls.runtime.agent.action";
    }
    /// 会话权限域.
    pub mod session {
        pub const LIST: &str = "ls.runtime.session.list";
        pub const CREATE: &str = "ls.runtime.session.create";
        pub const DELETE: &str = "ls.runtime.session.delete";
    }
    /// 工具权限域.
    pub mod tool {
        pub const LIST: &str = "ls.runtime.tool.list";
        pub const EXECUTE: &str = "ls.runtime.tool.execute";
    }
    /// 工作流权限域.
    pub mod workflow {
        pub const LIST: &str = "ls.runtime.workflow.list";
        pub const EXECUTE: &str = "ls.runtime.workflow.execute";
        pub const STATUS: &str = "ls.runtime.workflow.status";
    }
    /// Runtime 权限域.
    pub mod runtime {
        pub const STATUS: &str = "ls.runtime.status";
        pub const HEALTH: &str = "ls.runtime.health";
        pub const STATS: &str = "ls.runtime.stats";
        pub const CONFIG: &str = "ls.runtime.config";
    }
}

// ── 请求 / 响应类型 ──

/// API 请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiRequest {
    /// 心跳/健康检查.
    Ping,
    /// Agent 操作.
    Agent(ApiAgentRequest),
    /// 会话操作.
    Session(ApiSessionRequest),
    /// 工具操作.
    Tool(ApiToolRequest),
    /// 工作流操作.
    Workflow(ApiWorkflowRequest),
    /// Runtime 操作.
    Runtime(ApiRuntimeRequest),
}

/// 统一错误结构.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// 错误码（如 `tool_not_found`, `invalid_argument`, `permission_denied`）.
    pub code: String,
    /// 人类可读的错误描述.
    pub message: String,
    /// 可选的附加信息.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// 通用 API 响应.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ApiResponse {
    pub fn ok(data: Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now(),
        }
    }

    /// 创建错误响应（带错误码）.
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
                details: None,
            }),
            timestamp: chrono::Utc::now(),
        }
    }

    /// 创建错误响应（带错误码和附加详情）.
    pub fn err_with_details(code: impl Into<String>, message: impl Into<String>, details: Value) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
                details: Some(details),
            }),
            timestamp: chrono::Utc::now(),
        }
    }

    /// 从 LsResult 创建响应.
    pub fn from_result(result: LsResult<Value>) -> Self {
        match result {
            Ok(data) => Self::ok(data),
            Err(e) => Self::from_ls_error(e),
        }
    }

    /// 从 LsError 创建响应.
    pub fn from_ls_error(e: LsError) -> Self {
        let (code, message, details) = ls_error_to_api_error(&e);
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                code,
                message,
                details,
            }),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// 将 LsError 映射为 (code, message, details).
fn ls_error_to_api_error(e: &LsError) -> (String, String, Option<Value>) {
    let msg = e.to_string();
    let message = msg.clone();
    match e {
        LsError::InvalidArgument(_) => ("invalid_argument".into(), message, None),
        LsError::NotFound(_) => ("not_found".into(), message, None),
        LsError::AlreadyExists(_) => ("already_exists".into(), message, None),
        LsError::PermissionDenied(_) => ("permission_denied".into(), message, None),
        LsError::AuthenticationFailed(_) => ("authentication_failed".into(), message, None),
        LsError::SessionNotFound(_) => ("session_not_found".into(), message, None),
        LsError::SessionExpired(_) => ("session_expired".into(), message, None),
        LsError::QuotaExceeded(_) => ("quota_exceeded".into(), message, None),
        LsError::Timeout(_) => ("timeout".into(), message, None),
        LsError::RuntimeNotInitialized => ("runtime_not_initialized".into(), message, None),
        LsError::RuntimeAlreadyInitialized => ("runtime_already_initialized".into(), message, None),
        LsError::RuntimeState(_) => ("runtime_state".into(), message, None),
        LsError::PluginNotFound(_) => ("plugin_not_found".into(), message, None),
        LsError::Plugin(_) => ("plugin_error".into(), message, None),
        LsError::Llm(_) => ("llm_error".into(), message, None),
        LsError::Embedding(_) => ("embedding_error".into(), message, None),
        LsError::Storage(_) => ("storage_error".into(), message, None),
        LsError::EventBus(_) => ("eventbus_error".into(), message, None),
        LsError::Config(_) => ("config_error".into(), message, None),
        LsError::Serialization(_) => ("serialization_error".into(), message, None),
        LsError::Validation(_) => ("validation_error".into(), message, None),
        LsError::External(_) => ("external_error".into(), message, None),
        LsError::NotImplemented(_) => ("not_implemented".into(), message, None),
        LsError::Internal(_) => ("internal_error".into(), message, None),
        // Catch-all for non-exhaustive variants
        _ => ("internal_error".into(), format!("{}: {}", msg, e), None),
    }
}

// ── Agent API ──

/// Agent 操作请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiAgentRequest {
    pub action: AgentAction,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub input: Option<Value>,
    pub agent_type: Option<String>,
}

/// Agent 操作类型.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentAction {
    Register,
    Run,
    Pause,
    Resume,
    Cancel,
    Status,
    List,
    Remove,
    Snapshot,
}

/// Agent 状态信息（API 响应格式）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiAgentInfo {
    pub agent_id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
}

// ── 会话 API ──

/// 会话操作请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSessionRequest {
    pub action: SessionAction,
    pub session_id: Option<String>,
}

/// 会话操作类型.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionAction {
    Create,
    Get,
    Renew,
    Terminate,
    List,
    Stats,
}

/// 会话信息（API 响应格式）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSessionInfo {
    pub session_id: String,
    pub user_id: Option<String>,
    pub state: String,
    pub created_at: String,
    pub expires_at: String,
}

// ── 工具 API ──

/// 工具操作请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolRequest {
    pub action: ToolAction,
    pub tool_name: Option<String>,
    pub args: Option<Value>,
    pub filter: Option<lingshu_tool::ToolFilter>,
}

/// 工具操作类型.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolAction {
    List,
    Get,
    Execute,
    Stats,
    Definitions,
}

// ── 工作流 API ──

/// 工作流操作请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiWorkflowRequest {
    pub action: WorkflowAction,
    pub workflow_name: Option<String>,
    pub input: Option<Value>,
}

/// 工作流操作类型.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowAction {
    List,
    Execute,
    Status,
}

// ── Runtime API ──

/// Runtime 操作请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRuntimeRequest {
    pub action: RuntimeAction,
}

/// Runtime 操作类型.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeAction {
    Status,
    Health,
    Stats,
    Config,
}

// ── API 处理器 ──

/// API 处理器 — 处理所有 ApiRequest，返回 ApiResponse.
///
/// # 指标集成 (v4.2.3)
///
/// 内置 `RuntimeMetricsCollector`，在处理请求时自动记录：
/// - Agent 列表/状态 → 更新 `ls_agent_count`
/// - 工具执行 → 递增 `ls_tool_calls_total`
/// - 会话创建/统计 → 更新 `ls_session_count`
/// - Runtime 状态 → 更新 `ls_agent_count` + `ls_session_count`
pub struct ApiHandler {
    runtime: AgentRuntime,
    /// Prometheus 指标收集器 (v4.2.3).
    metrics: RuntimeMetricsCollector,
    /// OTel 指标收集器 (v4.2.3, 可选).
    #[cfg(feature = "otel")]
    otel_metrics: Option<RuntimeOtelMetrics>,
}

impl ApiHandler {
    /// 创建新的 API 处理器.
    pub fn new(runtime: AgentRuntime) -> Self {
        Self {
            metrics: RuntimeMetricsCollector::default(),
            #[cfg(feature = "otel")]
            otel_metrics: Some(RuntimeOtelMetrics::new()),
            runtime,
        }
    }

    /// 使用自定义 MetricsRegistry 创建 API 处理器.
    pub fn with_metrics_registry(runtime: AgentRuntime, registry: &MetricsRegistry) -> Self {
        Self {
            metrics: RuntimeMetricsCollector::new(registry),
            #[cfg(feature = "otel")]
            otel_metrics: Some(RuntimeOtelMetrics::new()),
            runtime,
        }
    }

    /// 获取内部 Runtime 引用.
    pub fn runtime(&self) -> AgentRuntime {
        self.runtime.clone()
    }

    /// 获取 MetricsCollector 引用（供外部调用）.
    pub fn metrics_collector(&self) -> &RuntimeMetricsCollector {
        &self.metrics
    }

    /// 处理 API 请求.
    pub async fn handle(&self, request: ApiRequest, ctx: &LsContext) -> ApiResponse {
        match request {
            ApiRequest::Ping => self.handle_ping(ctx).await,
            ApiRequest::Agent(req) => self.handle_agent(req, ctx).await,
            ApiRequest::Session(req) => self.handle_session(req, ctx).await,
            ApiRequest::Tool(req) => self.handle_tool(req, ctx).await,
            ApiRequest::Workflow(req) => self.handle_workflow(req, ctx).await,
            ApiRequest::Runtime(req) => self.handle_runtime(req, ctx).await,
        }
    }

    async fn handle_ping(&self, _ctx: &LsContext) -> ApiResponse {
        ApiResponse::ok(serde_json::json!({
            "status": "pong",
            "runtime": "lingshu-agent",
            "version": "4.0.0",
        }))
    }

    async fn handle_agent(&self, req: ApiAgentRequest, _ctx: &LsContext) -> ApiResponse {
        match req.action {
            AgentAction::List => {
                let agents = self.runtime.list_agents().await;
                let count = agents.len();
                // 更新 Agent 数量指标
                self.metrics.set_agent_count(count as i64);
                #[cfg(feature = "otel")]
                if let Some(ref otel) = self.otel_metrics {
                    otel.set_agent_count(count as u64);
                }

                let api_agents: Vec<ApiAgentInfo> = agents
                    .into_iter()
                    .map(|a| ApiAgentInfo {
                        agent_id: a.agent_id.to_string(),
                        name: a.name,
                        status: format!("{:?}", a.status),
                        created_at: a.created_at.to_rfc3339(),
                    })
                    .collect();
                ApiResponse::ok(serde_json::json!({"agents": api_agents, "count": count}))
            }
            AgentAction::Status => {
                let agent_id = req.agent_id.and_then(|id| parse_ls_id(&id).ok())
                    .ok_or_else(|| LsError::InvalidArgument("missing valid agent_id".into()));
                match agent_id {
                    Ok(id) => {
                        match self.runtime.agent_status(&id).await {
                            Ok(status) => ApiResponse::ok(serde_json::json!({
                                "agent_id": id.to_string(),
                                "status": format!("{:?}", status),
                            })),
                            Err(e) => ApiResponse::from_ls_error(e),
                        }
                    }
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            AgentAction::Remove => {
                let agent_id = req.agent_id.and_then(|id| parse_ls_id(&id).ok())
                    .ok_or_else(|| LsError::InvalidArgument("missing valid agent_id".into()));
                match agent_id {
                    Ok(id) => {
                        match self.runtime.remove_agent(&id).await {
                            Ok(_) => {
                                // 更新 Agent 数量指标
                                let count = self.runtime.agent_count().await;
                                self.metrics.set_agent_count(count as i64);
                                #[cfg(feature = "otel")]
                                if let Some(ref otel) = self.otel_metrics {
                                    otel.set_agent_count(count as u64);
                                }
                                ApiResponse::ok(serde_json::json!({"removed": id.to_string()}))
                            }
                            Err(e) => ApiResponse::from_ls_error(e),
                        }
                    }
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            _ => ApiResponse::err("not_implemented", format!("agent action {:?} not implemented", req.action)),
        }
    }

    async fn handle_session(&self, req: ApiSessionRequest, ctx: &LsContext) -> ApiResponse {
        let sm = self.runtime.session_manager().await;

        match req.action {
            SessionAction::Create => {
                match sm.create(ctx).await {
                    Ok(info) => {
                        // 更新会话数指标
                        let count = sm.active_count().await;
                        self.metrics.set_session_count(count as i64);
                        #[cfg(feature = "otel")]
                        if let Some(ref otel) = self.otel_metrics {
                            otel.set_session_count(count as u64);
                        }
                        ApiResponse::ok(serde_json::json!({
                            "session_id": info.session_id.to_string(),
                            "state": format!("{:?}", info.state),
                            "created_at": info.created_at.to_rfc3339(),
                        }))
                    }
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            SessionAction::Get => {
                let session_id = req.session_id.and_then(|id| parse_ls_id(&id).ok())
                    .ok_or_else(|| LsError::InvalidArgument("missing valid session_id".into()));
                match session_id {
                    Ok(id) => match sm.get(id).await {
                        Ok(info) => ApiResponse::ok(serde_json::json!({
                            "session_id": info.session_id.to_string(),
                            "state": format!("{:?}", info.state),
                            "created_at": info.created_at.to_rfc3339(),
                        })),
                        Err(e) => ApiResponse::from_ls_error(e),
                    },
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            SessionAction::Stats => {
                let active = self.runtime.active_sessions().await;
                self.metrics.set_session_count(active as i64);
                #[cfg(feature = "otel")]
                if let Some(ref otel) = self.otel_metrics {
                    otel.set_session_count(active as u64);
                }
                ApiResponse::ok(serde_json::json!({
                    "active_sessions": active,
                }))
            }
            _ => ApiResponse::err("not_implemented", format!("session action {:?} not implemented", req.action)),
        }
    }

    async fn handle_tool(&self, req: ApiToolRequest, ctx: &LsContext) -> ApiResponse {
        match self.runtime.tool_registry().await {
            Some(registry) => {
                match req.action {
                    ToolAction::List => {
                        let tools = registry.list_tools().await;
                        let count = tools.len();
                        self.metrics.inc_tool_calls("__list__", "success");
                        #[cfg(feature = "otel")]
                        if let Some(ref otel) = self.otel_metrics {
                            otel.inc_tool_calls("__list__", "success");
                        }
                        ApiResponse::ok(serde_json::json!({
                            "tools": tools,
                            "count": count,
                        }))
                    }
                    ToolAction::Get => {
                        let name = req.tool_name.as_deref()
                            .ok_or_else(|| LsError::InvalidArgument("missing tool_name".into()));
                        match name {
                            Ok(n) => match registry.get(n).await {
                                Some(info) => ApiResponse::ok(serde_json::to_value(info).unwrap_or_default()),
                                None => {
                                    self.metrics.inc_tool_calls(n, "not_found");
                                    #[cfg(feature = "otel")]
                                    if let Some(ref otel) = self.otel_metrics {
                                        otel.inc_tool_calls(n, "not_found");
                                    }
                                    ApiResponse::err("tool_not_found", format!("tool '{}' not found", n))
                                }
                            },
                            Err(e) => ApiResponse::from_ls_error(e),
                        }
                    }
                    ToolAction::Stats => {
                        let stats = registry.stats().await;
                        ApiResponse::ok(serde_json::to_value(stats).unwrap_or_default())
                    }
                    ToolAction::Definitions => {
                        let defs = registry.get_tool_definitions().await;
                        ApiResponse::ok(serde_json::to_value(defs).unwrap_or_default())
                    }
                    ToolAction::Execute => {
                        let name = req.tool_name.as_deref()
                            .ok_or_else(|| LsError::InvalidArgument("missing tool_name".into()));
                        let args = req.args.unwrap_or(Value::Null);
                        match name {
                            Ok(n) => {
                                match registry.execute_unchecked(ctx, n, args).await {
                                    Ok(result) => {
                                        self.metrics.inc_tool_calls(n, "success");
                                        #[cfg(feature = "otel")]
                                        if let Some(ref otel) = self.otel_metrics {
                                            otel.inc_tool_calls(n, "success");
                                        }
                                        ApiResponse::ok(result)
                                    }
                                    Err(e) => {
                                        self.metrics.inc_tool_calls(n, "failed");
                                        #[cfg(feature = "otel")]
                                        if let Some(ref otel) = self.otel_metrics {
                                            otel.inc_tool_calls(n, "failed");
                                        }
                                        ApiResponse::from_ls_error(e)
                                    }
                                }
                            }
                            Err(e) => ApiResponse::from_ls_error(e),
                        }
                    }
                }
            }
            None => ApiResponse::err("tool_registry_not_configured", "ToolRegistry not configured"),
        }
    }

    async fn handle_workflow(&self, req: ApiWorkflowRequest, _ctx: &LsContext) -> ApiResponse {
        match req.action {
            WorkflowAction::List => {
                if !self.runtime.has_workflow_access().await {
                    return ApiResponse::err("not_configured", "WorkflowAccess not configured");
                }
                let workflows = self.runtime.list_workflows().await;
                ApiResponse::ok(serde_json::json!({ "workflows": workflows }))
            }
            WorkflowAction::Execute => {
                let name = match &req.workflow_name {
                    Some(n) => n.clone(),
                    None => return ApiResponse::err("invalid_argument", "workflow_name required"),
                };
                let input = req.input.clone().unwrap_or(serde_json::json!({}));
                match self.runtime.execute_workflow(&name, _ctx.clone(), input).await {
                    Ok(result) => ApiResponse::ok(serde_json::json!({ "result": result })),
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            WorkflowAction::Status => {
                let name = match &req.workflow_name {
                    Some(n) => n.clone(),
                    None => return ApiResponse::err("invalid_argument", "workflow_name required"),
                };
                match self.runtime.workflow_status(&name).await {
                    Ok(status) => ApiResponse::ok(serde_json::json!({ "status": status })),
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
        }
    }

    async fn handle_runtime(&self, req: ApiRuntimeRequest, _ctx: &LsContext) -> ApiResponse {
        match req.action {
            RuntimeAction::Status => {
                let state = self.runtime.lifecycle_state().await;
                let agent_count = self.runtime.agent_count().await;
                let session_count = self.runtime.active_sessions().await;
                // 更新指标
                self.metrics.set_agent_count(agent_count as i64);
                self.metrics.set_session_count(session_count as i64);
                #[cfg(feature = "otel")]
                if let Some(ref otel) = self.otel_metrics {
                    otel.set_agent_count(agent_count as u64);
                    otel.set_session_count(session_count as u64);
                }
                match state {
                    Ok(s) => ApiResponse::ok(serde_json::json!({
                        "state": format!("{:?}", s),
                        "name": "lingshu-agent",
                        "version": "4.0.0",
                        "agent_count": agent_count,
                        "session_count": session_count,
                    })),
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            RuntimeAction::Health => {
                let state = self.runtime.lifecycle_state().await;
                match state {
                    Ok(s) if s.is_running() => ApiResponse::ok(serde_json::json!({
                        "healthy": true,
                        "state": format!("{:?}", s),
                    })),
                    Ok(s) => ApiResponse::ok(serde_json::json!({
                        "healthy": false,
                        "state": format!("{:?}", s),
                    })),
                    Err(e) => ApiResponse::from_ls_error(e),
                }
            }
            RuntimeAction::Stats => {
                let agent_count = self.runtime.agent_count().await;
                let session_count = self.runtime.active_sessions().await;
                self.metrics.set_agent_count(agent_count as i64);
                self.metrics.set_session_count(session_count as i64);
                #[cfg(feature = "otel")]
                if let Some(ref otel) = self.otel_metrics {
                    otel.set_agent_count(agent_count as u64);
                    otel.set_session_count(session_count as u64);
                }
                ApiResponse::ok(serde_json::json!({
                    "agents": agent_count,
                    "sessions": session_count,
                }))
            }
            RuntimeAction::Config => {
                let config = self.runtime.config().await;
                ApiResponse::ok(serde_json::to_value(config).unwrap_or_default())
            }
        }
    }
}

fn parse_ls_id(s: &str) -> LsResult<LsId> {
    uuid::Uuid::parse_str(s)
        .map(LsId::from)
        .map_err(|e| LsError::InvalidArgument(format!("invalid LsId '{}': {}", s, e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::AgentRuntimeConfig;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_ping() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let handler = ApiHandler::new(runtime);
        let response = handler.handle(ApiRequest::Ping, &test_ctx()).await;
        assert!(response.success);
        assert_eq!(response.data.unwrap()["status"], "pong");
    }

    #[tokio::test]
    async fn test_runtime_status() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let handler = ApiHandler::new(runtime);
        let req = ApiRequest::Runtime(ApiRuntimeRequest {
            action: RuntimeAction::Status,
        });
        let response = handler.handle(req, &test_ctx()).await;
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_runtime_health() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        runtime.start().await.unwrap();
        let handler = ApiHandler::new(runtime);
        let req = ApiRequest::Runtime(ApiRuntimeRequest {
            action: RuntimeAction::Health,
        });
        let response = handler.handle(req, &test_ctx()).await;
        assert!(response.success);
        assert!(response.data.unwrap()["healthy"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_tool_list_no_registry() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let handler = ApiHandler::new(runtime);
        let req = ApiRequest::Tool(ApiToolRequest {
            action: ToolAction::List,
            tool_name: None,
            args: None,
            filter: None,
        });
        let response = handler.handle(req, &test_ctx()).await;
        assert!(!response.success);
        assert!(response.error.unwrap().message.contains("not configured"));
    }

    #[tokio::test]
    async fn test_session_create() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let handler = ApiHandler::new(runtime);
        let req = ApiRequest::Session(ApiSessionRequest {
            action: SessionAction::Create,
            session_id: None,
        });
        let response = handler.handle(req, &test_ctx()).await;
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_api_response_helpers() {
        let ok = ApiResponse::ok(serde_json::json!({"key": "value"}));
        assert!(ok.success);
        assert_eq!(ok.data.unwrap()["key"], "value");

        let err = ApiResponse::err("test_error", "something went wrong");
        assert!(!err.success);
        assert_eq!(err.error.unwrap().message, "something went wrong");
    }

    #[tokio::test]
    async fn test_runtime_metrics_recorded() {
        let runtime = AgentRuntime::new(AgentRuntimeConfig::default()).await.unwrap();
        let handler = ApiHandler::new(runtime);
        let ctx = test_ctx();

        // Session create → session_count metrics
        let resp = handler.handle(
            ApiRequest::Session(ApiSessionRequest {
                action: SessionAction::Create,
                session_id: None,
            }),
            &ctx,
        ).await;
        assert!(resp.success);

        // Runtime stats → agent_count + session_count metrics
        let resp = handler.handle(
            ApiRequest::Runtime(ApiRuntimeRequest {
                action: RuntimeAction::Stats,
            }),
            &ctx,
        ).await;
        assert!(resp.success);
        let data = resp.data.unwrap();
        assert_eq!(data["sessions"].as_u64().unwrap_or(0), 1);

        // Agent list → agent_count metrics
        let resp = handler.handle(
            ApiRequest::Agent(ApiAgentRequest {
                action: AgentAction::List,
                agent_id: None,
                agent_name: None,
                input: None,
                agent_type: None,
            }),
            &ctx,
        ).await;
        assert!(resp.success);
    }
}
