//! MCP Server — JSON-RPC 2.0 请求分发器
//!
//! 接收 JSON-RPC 请求，路由到对应处理方法，返回 JSON-RPC 响应。
//! 与 HTTP 层解耦，可被 axum handler 调用。
//!
//! ## 扩展能力
//!
//! - **流式进度通知**: 若 tools/call 请求携带 progress_token，工具执行期间
//!   可通过 ProgressContext 发送 notifications/progress 通知。
//! - **状态查询**: 内置 mcp_status 工具，可查询服务器状态与工具执行记录。
//! - **SSE 桥接**: 可通过 set_progress_sender 将进度通知转发到 SSE/WebSocket。

use crate::tool;
use crate::types::*;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tracing::debug;

// ── 工具执行追踪 ────────────────────────────────────

/// 工具执行追踪器 — 记录每次工具调用的状态与进度
#[derive(Debug, Clone)]
pub struct ExecutionTracker {
    pub(crate) records: Arc<RwLock<Vec<ToolExecutionRecord>>>,
    pub(crate) total_calls: Arc<RwLock<u64>>,
    pub(crate) total_failures: Arc<RwLock<u64>>,
    pub(crate) started_at: u64,
}

impl ExecutionTracker {
    fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            total_calls: Arc::new(RwLock::new(0)),
            total_failures: Arc::new(RwLock::new(0)),
            started_at: now_ms(),
        }
    }

    /// 创建新的执行记录
    pub(crate) async fn create_record(
        &self,
        execution_id: String,
        tool_name: String,
        session_id: String,
    ) {
        let record = ToolExecutionRecord {
            execution_id,
            tool_name,
            state: ToolExecutionState::Running,
            started_at: now_ms(),
            completed_at: None,
            progress_pct: 0.0,
            progress_message: None,
            session_id,
        };
        self.records.write().await.push(record);
        *self.total_calls.write().await += 1;
    }

    /// 更新执行进度
    pub(crate) async fn update_progress(
        &self,
        execution_id: &str,
        progress_pct: f64,
        message: Option<String>,
    ) {
        let records = &mut *self.records.write().await;
        if let Some(record) = records.iter_mut().find(|r| r.execution_id == execution_id) {
            record.progress_pct = progress_pct;
            if let Some(msg) = message {
                record.progress_message = Some(msg);
            }
        }
    }

    /// 标记执行完成
    pub(crate) async fn complete_record(&self, execution_id: &str, success: bool) {
        let records = &mut *self.records.write().await;
        if let Some(record) = records.iter_mut().find(|r| r.execution_id == execution_id) {
            record.state = if success {
                ToolExecutionState::Completed
            } else {
                ToolExecutionState::Failed
            };
            record.completed_at = Some(now_ms());
            record.progress_pct = if success { 100.0 } else { record.progress_pct };
        }
        if !success {
            *self.total_failures.write().await += 1;
        }
    }

    /// 获取活跃（运行中）的执行记录
    pub(crate) async fn active_executions(&self) -> Vec<ToolExecutionRecord> {
        let records = self.records.read().await;
        records
            .iter()
            .filter(|r| {
                matches!(
                    r.state,
                    ToolExecutionState::Running | ToolExecutionState::Pending
                )
            })
            .cloned()
            .collect()
    }

    /// 获取统计信息
    pub(crate) async fn stats(&self) -> (u64, u64) {
        let calls = *self.total_calls.read().await;
        let failures = *self.total_failures.read().await;
        (calls, failures)
    }

    /// 按 execution_id 查询单条记录
    pub(crate) async fn get_execution(&self, execution_id: &str) -> Option<ToolExecutionRecord> {
        let records = self.records.read().await;
        records
            .iter()
            .find(|r| r.execution_id == execution_id)
            .cloned()
    }

    /// 按 session_id 查询记录
    pub(crate) async fn get_executions_by_session(
        &self,
        session_id: &str,
    ) -> Vec<ToolExecutionRecord> {
        let records = self.records.read().await;
        records
            .iter()
            .filter(|r| r.session_id == session_id)
            .cloned()
            .collect()
    }

    /// 获取全部记录
    pub(crate) async fn get_all_records(&self) -> Vec<ToolExecutionRecord> {
        self.records.read().await.clone()
    }

    /// 服务器启动时长（毫秒）
    pub(crate) fn uptime_ms(&self) -> u64 {
        now_ms() - self.started_at
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── MCP 服务器 ──────────────────────────────────────

/// MCP 服务器 — 处理 JSON-RPC 请求，支持进度通知与状态追踪
pub struct McpServer {
    /// 已注册的 MCP 工具
    tools: Vec<Arc<dyn Tool>>,
    /// 工具执行追踪器
    tracker: ExecutionTracker,
    /// 进度通知外部发送器（如 SSE/WebSocket 桥接）
    progress_sender: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<ProgressNotification>>>>,
}

impl McpServer {
    pub fn new() -> Self {
        let mut server = Self {
            tools: Vec::new(),
            tracker: ExecutionTracker::new(),
            progress_sender: Arc::new(Mutex::new(None)),
        };
        server.register_builtin_tools();
        server
    }

    /// 返回已注册工具列表 (名称).
    pub fn list_tools(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.info().name.clone()).collect()
    }

    /// 设置进度通知发送器 — 用于将进度通知桥接到 SSE/WebSocket
    pub fn set_progress_sender(
        &self,
        sender: tokio::sync::mpsc::UnboundedSender<ProgressNotification>,
    ) {
        if let Ok(mut guard) = self.progress_sender.try_lock() {
            *guard = Some(sender);
        }
    }

    /// 注册一个工具
    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        debug!(name = %tool.info().name, "mcp tool registered");
        self.tools.push(tool);
    }

    /// 批量注册工具
    pub fn register_tools(&mut self, tools: Vec<Arc<dyn Tool>>) {
        for tool in tools {
            self.register_tool(tool);
        }
    }

    /// 注册内置工具（mcp_status 等）
    fn register_builtin_tools(&mut self) {
        let status_tool = McpStatusTool::new(self.tracker.clone());
        self.register_tool(Arc::new(status_tool));
        debug!("built-in mcp_status tool registered");
    }

    /// 处理 JSON-RPC 请求，返回 JSON-RPC 响应
    pub async fn handle_request(&self, ctx: &LsContext, body: Value) -> JsonRpcResponse<Value> {
        let request_id = body.get("id").and_then(|v| v.as_u64()).unwrap_or(0);

        // 校验 jsonrpc 版本
        if body.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
            return JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError::new(
                    INVALID_REQUEST,
                    "jsonrpc version must be 2.0",
                )),
                id: request_id,
            };
        }

        // 提取 method
        let method = match body.get("method").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcError::new(INVALID_REQUEST, "method is required")),
                    id: request_id,
                };
            }
        };

        let params = body.get("params").cloned().unwrap_or(Value::Null);

        match method {
            METHOD_TOOLS_LIST => self.handle_tools_list(request_id, params).await,
            METHOD_TOOLS_CALL => self.handle_tools_call(ctx, request_id, params).await,
            METHOD_MCP_STATUS => self.handle_mcp_status(request_id).await,
            METHOD_RESOURCES_LIST => self.handle_resources_list(request_id, params).await,
            METHOD_PROMPTS_LIST => self.handle_prompts_list(request_id, params).await,
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError::new(
                    METHOD_NOT_FOUND,
                    format!("unknown method: {method}"),
                )),
                id: request_id,
            },
        }
    }

    /// `tools/list` — 列出所有可用工具
    async fn handle_tools_list(&self, id: u64, _params: Value) -> JsonRpcResponse<Value> {
        let tools: Vec<McpTool> = self
            .tools
            .iter()
            .map(|t| tool::tool_info_to_mcp(&t.info()))
            .collect();

        let result = ToolsListResult {
            tools,
            next_cursor: None,
        };

        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
            id,
        }
    }

    /// `tools/call` — 调用指定工具（支持进度令牌）
    async fn handle_tools_call(
        &self,
        ctx: &LsContext,
        id: u64,
        params: Value,
    ) -> JsonRpcResponse<Value> {
        // 解析参数
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcError::new(INVALID_PARAMS, "name is required")),
                    id,
                };
            }
        };
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
        let progress_token = params
            .get("progress_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 查找工具
        let tool = match self.tools.iter().find(|t| t.info().name == name) {
            Some(t) => t.clone(),
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcError::new(
                        METHOD_NOT_FOUND,
                        format!("tool not found: {name}"),
                    )),
                    id,
                };
            }
        };

        // 获取 session_id
        let session_id = ctx.session_id.to_string();

        // 生成 execution_id
        let execution_id = uuid::Uuid::new_v4().to_string();

        // 创建执行记录
        self.tracker
            .create_record(execution_id.clone(), name.clone(), session_id)
            .await;

        // 如果有 progress_token，配置进度通知
        let callback = if let Some(token) = progress_token {
            let sender = self.progress_sender.clone();
            let token_for_cb = token.clone();
            let tracker = self.tracker.clone();
            let eid = execution_id.clone();
            Some(ProgressContext::new(
                token,
                Arc::new(move |progress, total, message| {
                    // 更新内部的 ExecutionTracker
                    let t = tracker.clone();
                    let e = eid.clone();
                    let msg = message.clone();
                    tokio::spawn(async move {
                        t.update_progress(&e, progress, msg).await;
                    });

                    // 发送 SSE 通知
                    let notification =
                        ProgressNotification::new(token_for_cb.clone(), progress, total, message);
                    if let Ok(guard) = sender.try_lock() {
                        if let Some(ref tx) = *guard {
                            let _ = tx.send(notification);
                        }
                    }
                }),
            ))
        } else {
            None
        };

        // 校验参数
        if let Err(e) = tool.validate(&arguments) {
            self.tracker.complete_record(&execution_id, false).await;
            return JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError::new(INVALID_PARAMS, e.to_string())),
                id,
            };
        }

        // 执行工具（支持进度回调）
        let result = if let Some(progress) = callback {
            tool::execute_tool_to_mcp_with_progress(&*tool, ctx.clone(), arguments, Some(progress))
                .await
        } else {
            tool::execute_tool_to_mcp(&*tool, ctx.clone(), arguments).await
        };

        // 记录完成状态
        self.tracker
            .complete_record(&execution_id, !result.is_error)
            .await;

        // 注入 execution_id 到结果中，方便后续查询执行状态
        let mut result = result;
        result.execution_id = Some(execution_id.clone());

        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
            id,
        }
    }

    /// `mcp_status` — 查询服务器状态与工具执行信息
    async fn handle_mcp_status(&self, id: u64) -> JsonRpcResponse<Value> {
        let active = self.tracker.active_executions().await;
        let (total_calls, total_failures) = self.tracker.stats().await;

        let result = McpStatusResult {
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            registered_tools: self.tools.len(),
            active_executions: active,
            server_uptime_ms: self.tracker.uptime_ms(),
            total_tool_calls: total_calls,
            total_failures,
        };

        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
            id,
        }
    }

    /// `resources/list` — 列出可用资源（当前为空）
    async fn handle_resources_list(&self, id: u64, _params: Value) -> JsonRpcResponse<Value> {
        let result = ResourcesListResult {
            resources: Vec::new(),
            next_cursor: None,
        };
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
            id,
        }
    }

    /// `prompts/list` — 列出可用提示（当前为空）
    async fn handle_prompts_list(&self, id: u64, _params: Value) -> JsonRpcResponse<Value> {
        let result = PromptsListResult {
            prompts: Vec::new(),
            next_cursor: None,
        };
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
            id,
        }
    }

    /// 获取 tracker 的进度发送器（用于外部桥接配置）
    pub fn progress_sender_clone(
        &self,
    ) -> Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<ProgressNotification>>>> {
        self.progress_sender.clone()
    }

    /// 桥接到 SSE 广播器
    ///
    /// 所有 notifications/progress 通知将自动通过 SseBroadcaster 转发。
    /// 需要在 McpServer 初始化后调用一次。
    pub fn bridge_sse(&self, broadcaster: std::sync::Arc<lingshu_websocket::SseBroadcaster>) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        self.set_progress_sender(tx);

        tokio::spawn(async move {
            while let Some(notification) = rx.recv().await {
                if let Ok(data) = serde_json::to_value(&notification) {
                    let sse_event = lingshu_websocket::SseEvent::new("mcp.progress", data);
                    broadcaster.publish(sse_event);
                }
            }
        });
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

// ── 内置 MCP 工具 ────────────────────────────────────

/// MCP 内置状态查询工具 — 允许 AI 通过 tools/list 发现并查询执行状态
///
/// 参数:
/// - `execution_id` (可选): 按执行 ID 查询单条记录
/// - `session_id` (可选): 按会话 ID 查询记录列表
/// - 不提供参数时返回全局状态
pub(crate) struct McpStatusTool {
    tracker: ExecutionTracker,
}

impl McpStatusTool {
    pub fn new(tracker: ExecutionTracker) -> Self {
        Self { tracker }
    }
}

#[async_trait::async_trait]
impl Tool for McpStatusTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "mcp_status".into(),
            description: "查询 MCP 服务器状态与工具执行记录。可指定 execution_id 查询单条，或 session_id 查询会话全部记录。不传参返回全局状态。".into(),
            parameters: vec![
                ToolParam {
                    name: "execution_id".into(),
                    description: "按执行 ID 查询单条执行记录".into(),
                    required: false,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "session_id".into(),
                    description: "按会话 ID 查询执行记录列表".into(),
                    required: false,
                    param_type: "string".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if !input.is_object() {
            return Err(LsError::Validation("input must be a JSON object".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let execution_id = input
            .get("execution_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let (total_calls, total_failures) = self.tracker.stats().await;

        // 根据参数查询执行记录
        let records: Vec<ToolExecutionRecord> = if let Some(ref eid) = execution_id {
            self.tracker.get_execution(eid).await.into_iter().collect()
        } else if let Some(ref sid) = session_id {
            self.tracker.get_executions_by_session(sid).await
        } else {
            self.tracker.get_all_records().await
        };

        // 过滤出活跃的执行
        let active: Vec<ToolExecutionRecord> = self.tracker.active_executions().await;

        let result = serde_json::json!({
            "server_version": env!("CARGO_PKG_VERSION"),
            "server_uptime_ms": self.tracker.uptime_ms(),
            "total_tool_calls": total_calls,
            "total_failures": total_failures,
            "active_count": active.len(),
            "active_executions": active,
            "total_records": records.len(),
            "records": records,
            "query": {
                "execution_id": execution_id,
                "session_id": session_id,
            },
        });

        Ok(result)
    }
    fn duplicate(&self) -> Box<dyn lingshu_traits::Tool> {
        Box::new(McpStatusTool { tracker: self.tracker.clone() })
    }
}

// ── 测试 ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::LsId;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                tool_id: LsId::new(),
                name: "echo".into(),
                description: "Echo input".into(),
                parameters: vec![ToolParam {
                    name: "message".into(),
                    description: "Message".into(),
                    required: true,
                    param_type: "string".into(),
                }],
            ..Default::default()
            }
        }

        fn validate(&self, _input: &Value) -> LsResult<()> {
            Ok(())
        }

        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            Ok(input)
        }
    
    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(EchoTool)
    }
}

    struct AddTool;

    #[async_trait]
    impl Tool for AddTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                tool_id: LsId::new(),
                name: "add".into(),
                description: "Add two numbers".into(),
                parameters: vec![
                    ToolParam {
                        name: "a".into(),
                        description: "First number".into(),
                        required: true,
                        param_type: "number".into(),
                    },
                    ToolParam {
                        name: "b".into(),
                        description: "Second number".into(),
                        required: true,
                        param_type: "number".into(),
                    },
                ],
            ..Default::default()
            }
        }

        fn validate(&self, input: &Value) -> LsResult<()> {
            if input.get("a").and_then(|v| v.as_f64()).is_none() {
                return Err(LsError::Validation("missing a".into()));
            }
            if input.get("b").and_then(|v| v.as_f64()).is_none() {
                return Err(LsError::Validation("missing b".into()));
            }
            Ok(())
        }

        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            let a = input.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = input.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(serde_json::json!({ "result": a + b }))
        }
        fn duplicate(&self) -> Box<dyn lingshu_traits::Tool> { Box::new(AddTool) }
    }

    #[tokio::test]
    async fn test_tools_list() {
        let mut server = McpServer::new();
        server.register_tool(Arc::new(EchoTool));
        server.register_tool(Arc::new(AddTool));

        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": 1,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.id, 1);

        let result: ToolsListResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        // 2 registered + 1 built-in (mcp_status) = 3
        assert_eq!(result.tools.len(), 3);
    }

    #[tokio::test]
    async fn test_tools_call() {
        let mut server = McpServer::new();
        server.register_tool(Arc::new(AddTool));

        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": "add", "arguments": { "a": 3, "b": 4 } },
            "id": 2,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let result: ToolsCallResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] {
            McpContent::Text { text } => text.as_str(),
            _ => "",
        };
        assert!(
            text.contains("7"),
            "expected result containing 7, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_tools_call_with_progress_token() {
        let mut server = McpServer::new();
        server.register_tool(Arc::new(EchoTool));

        // 捕获进度通知
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        server.set_progress_sender(tx);

        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "echo",
                "arguments": { "message": "hello" },
                "progress_token": "test-token-001"
            },
            "id": 3,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        // 验证进度通知被发送
        if let Ok(notification) = rx.try_recv() {
            assert_eq!(notification.params.progress_token, "test-token-001");
        }
    }

    #[tokio::test]
    async fn test_mcp_status() {
        let mut server = McpServer::new();
        server.register_tool(Arc::new(EchoTool));

        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "mcp_status",
            "params": {},
            "id": 4,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let status: McpStatusResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        // EchoTool + built-in mcp_status
        assert_eq!(status.registered_tools, 2);
        assert!(!status.server_version.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_status_tool_call() {
        let mut server = McpServer::new();

        // 先调用一个工具来产生执行记录
        server.register_tool(Arc::new(EchoTool));
        let ctx = LsContext::with_session(LsId::new());
        let call_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": "echo", "arguments": { "message": "hi" } },
            "id": 1,
        });
        server.handle_request(&ctx, call_body).await;

        // 调用 mcp_status 工具查状态
        let status_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "mcp_status",
                "arguments": {}
            },
            "id": 2,
        });

        let resp = server.handle_request(&ctx, status_body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let result: ToolsCallResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] {
            McpContent::Text { text } => text.as_str(),
            _ => "",
        };
        // Should contain total_tool_calls and records
        assert!(
            text.contains("total_tool_calls"),
            "expected stats in response, got: {text}"
        );
        assert!(
            text.contains("records"),
            "expected records in response, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let server = McpServer::new();
        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "unknown",
            "params": {},
            "id": 5,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tool_not_found() {
        let server = McpServer::new();
        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": "nonexistent", "arguments": {} },
            "id": 6,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(resp.error.is_some());
    }

    #[tokio::test]
    async fn test_resources_list() {
        let server = McpServer::new();
        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "resources/list",
            "params": {},
            "id": 7,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let result: ResourcesListResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(result.resources.is_empty());
    }

    #[tokio::test]
    async fn test_prompts_list() {
        let server = McpServer::new();
        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "prompts/list",
            "params": {},
            "id": 8,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let result: PromptsListResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(result.prompts.is_empty());
    }

    #[tokio::test]
    async fn test_tools_call_returns_execution_id() {
        let mut server = McpServer::new();
        server.register_tool(Arc::new(EchoTool));

        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": "echo", "arguments": { "message": "hi" } },
            "id": 9,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let result: ToolsCallResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(!result.is_error);
        assert!(
            result.execution_id.is_some(),
            "tools/call should return execution_id"
        );
        let eid = result.execution_id.unwrap();
        assert!(!eid.is_empty(), "execution_id should not be empty");

        // 验证可以用这个 execution_id 通过 mcp_status 工具查询
        let status_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "mcp_status",
                "arguments": { "execution_id": eid }
            },
            "id": 10,
        });

        let status_resp = server.handle_request(&ctx, status_body).await;
        assert!(
            status_resp.result.is_some(),
            "expected result, got error: {:?}",
            status_resp.error
        );

        let status_result: ToolsCallResult =
            serde_json::from_value(status_resp.result.unwrap()).unwrap();
        assert!(!status_result.is_error);
        let text = match &status_result.content[0] {
            McpContent::Text { text } => text.as_str(),
            _ => "",
        };
        assert!(
            text.contains(&eid),
            "response should contain the execution_id: {text}"
        );
    }

    #[tokio::test]
    async fn test_credential_tools_in_tools_list() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");
        let store = std::sync::Arc::new(
            lingshu_credentials::CredentialStore::open(&db_path, "test-master-key-for-testing")
                .expect("open store"),
        );
        let mgr = std::sync::Arc::new(lingshu_credentials::CredentialManager::new(store));
        let mut server = McpServer::new();
        let credential_tools = crate::credential_tools::create_credential_tools(mgr.clone());
        server.register_tools(credential_tools);

        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": 1,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(
            resp.result.is_some(),
            "expected result, got error: {:?}",
            resp.error
        );

        let result: ToolsListResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        let tool_names: Vec<&str> = result.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            tool_names.contains(&"credential_list"),
            "credential_list should be in tools list"
        );
        assert!(
            tool_names.contains(&"credential_create"),
            "credential_create should be in tools list"
        );
        assert!(
            tool_names.contains(&"credential_get"),
            "credential_get should be in tools list"
        );
        assert!(
            tool_names.contains(&"credential_update"),
            "credential_update should be in tools list"
        );
        assert!(
            tool_names.contains(&"credential_delete"),
            "credential_delete should be in tools list"
        );
        assert!(
            tool_names.contains(&"credential_validate"),
            "credential_validate should be in tools list"
        );
    }

    #[tokio::test]
    async fn test_credential_create_and_list_via_mcp() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test2.db");
        let store = std::sync::Arc::new(
            lingshu_credentials::CredentialStore::open(&db_path, "test-master-key-for-testing")
                .expect("open store"),
        );
        let mgr = std::sync::Arc::new(lingshu_credentials::CredentialManager::new(store));
        let mut server = McpServer::new();
        let credential_tools = crate::credential_tools::create_credential_tools(mgr.clone());
        server.register_tools(credential_tools);

        let ctx = LsContext::with_session(LsId::new());

        // Create a credential via tools/call
        let create_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "credential_create",
                "arguments": {
                    "provider": "gitee",
                    "credential_type": "personal_access_token",
                    "name": "test-credential",
                    "token": "test-token-value-12345",
                    "description": "test credential via MCP"
                }
            },
            "id": 2,
        });

        let create_resp = server.handle_request(&ctx, create_body).await;
        assert!(
            create_resp.result.is_some(),
            "create failed: {:?}",
            create_resp.error
        );
        let create_result: ToolsCallResult =
            serde_json::from_value(create_resp.result.unwrap()).unwrap();
        assert!(
            !create_result.is_error,
            "create should not error: {:?}",
            create_result.content
        );

        // List credentials via tools/call
        let list_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "credential_list",
                "arguments": {}
            },
            "id": 3,
        });

        let list_resp = server.handle_request(&ctx, list_body).await;
        assert!(
            list_resp.result.is_some(),
            "list failed: {:?}",
            list_resp.error
        );
        let list_result: ToolsCallResult =
            serde_json::from_value(list_resp.result.unwrap()).unwrap();
        assert!(!list_result.is_error, "list should not error");
        let text = match &list_result.content[0] {
            McpContent::Text { text } => text.as_str(),
            _ => "",
        };
        assert!(
            text.contains("test-credential"),
            "list should contain created credential: {text}"
        );
        assert!(
            text.contains("gitee"),
            "list should contain provider gitee: {text}"
        );
    }
}
