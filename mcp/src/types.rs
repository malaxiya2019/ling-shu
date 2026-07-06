//! MCP (Model Context Protocol) — JSON-RPC 2.0 协议类型
//!
//! 参考: https://spec.modelcontextprotocol.io/
//!
//! 核心方法:
//! - `tools/list`              — 列出可用工具
//! - `tools/call`              — 调用指定工具（支持进度令牌）
//! - `notifications/progress`  — 工具执行进度通知
//! - `resources/list`          — 列出可用资源 (预留)
//! - `prompts/list`            — 列出可用提示 (预留)
//! - `mcp_status`              — 内置状态查询工具

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

// ── JSON-RPC 2.0 基础类型 ──────────────────────────

/// JSON-RPC 2.0 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest<P: Serialize> {
    pub jsonrpc: String,
    pub method: String,
    pub params: P,
    pub id: u64,
}

/// JSON-RPC 2.0 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse<R: Serialize> {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

/// JSON-RPC 2.0 通知（无 id）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification<P: Serialize> {
    pub jsonrpc: String,
    pub method: String,
    pub params: P,
}

/// JSON-RPC 2.0 错误
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: i64, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

/// 标准 JSON-RPC 错误码
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

// ── MCP 工具类型 ────────────────────────────────────

/// MCP Tool 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
}

/// `tools/list` 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// `tools/list` 响应结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<McpTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `tools/call` 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallParams {
    pub name: String,
    pub arguments: Value,
    /// 可选的进度令牌 — 若提供，工具执行期间会发送 progress 通知
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<String>,
}

/// MCP 调用结果内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: McpResource },
}

/// MCP 资源引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub mime_type: String,
    pub text: Option<String>,
}

/// `tools/call` 响应结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallResult {
    pub content: Vec<McpContent>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[serde(default)]
    pub is_error: bool,
    /// 执行 ID，用于后续通过 mcp_status 按 ID 查询执行状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
}

// ── 进度通知类型 ────────────────────────────────────

/// `notifications/progress` 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNotificationParams {
    /// 进度令牌（取自 tools/call 的请求参数）
    pub progress_token: String,
    /// 当前进度值
    pub progress: f64,
    /// 总进度值（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// 进度描述（可选，如 "Analyzing file 3/10"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 进度通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: ProgressNotificationParams,
}

impl ProgressNotification {
    pub fn new(
        progress_token: String,
        progress: f64,
        total: Option<f64>,
        message: Option<String>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: METHOD_NOTIFICATIONS_PROGRESS.into(),
            params: ProgressNotificationParams {
                progress_token,
                progress,
                total,
                message,
            },
        }
    }
}

/// 进度回调函数签名 — 工具在执行期间调用以报告进度
pub type ProgressCallback = Arc<dyn Fn(f64, Option<f64>, Option<String>) + Send + Sync>;

/// 进度上下文 — 传递给工具的可选进度报告能力
#[derive(Clone)]
pub struct ProgressContext {
    pub progress_token: String,
    callback: ProgressCallback,
}

impl ProgressContext {
    pub fn new(progress_token: String, callback: ProgressCallback) -> Self {
        Self {
            progress_token,
            callback,
        }
    }

    /// 报告进度
    pub fn report(&self, progress: f64, total: Option<f64>, message: Option<String>) {
        (self.callback)(progress, total, message);
    }

    /// 报告百分比进度（基于 total）
    pub fn report_percent(&self, current: f64, total: f64, message: Option<String>) {
        let pct = if total > 0.0 {
            (current / total) * 100.0
        } else {
            0.0
        };
        (self.callback)(pct, Some(100.0), message);
    }
}

// ── 状态查询类型 ────────────────────────────────────

/// 工具执行状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 单个工具执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    /// 执行唯一 ID
    pub execution_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 状态
    pub state: ToolExecutionState,
    /// 开始时间（Unix 时间戳，毫秒）
    pub started_at: u64,
    /// 完成时间（可选）
    pub completed_at: Option<u64>,
    /// 进度百分比（0-100）
    pub progress_pct: f64,
    /// 进度消息
    pub progress_message: Option<String>,
    /// 关联的 session_id
    pub session_id: String,
}

/// `mcp_status` 工具 — 查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStatusResult {
    /// 服务器版本
    pub server_version: String,
    /// 已注册工具数量
    pub registered_tools: usize,
    /// 当前/最近的工具执行
    pub active_executions: Vec<ToolExecutionRecord>,
    /// 服务器启动时间
    pub server_uptime_ms: u64,
    /// 累计工具调用次数
    pub total_tool_calls: u64,
    /// 累计失败次数
    pub total_failures: u64,
}

// ── 资源 / 提示类型（预留）───────────────────────────

/// `resources/list` 响应结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesListResult {
    pub resources: Vec<ResourceDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 资源定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

/// `prompts/list` 响应结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsListResult {
    pub prompts: Vec<PromptDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 提示定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDefinition {
    pub name: String,
    pub description: String,
}


// ── MCP 方法名常量 ──────────────────────────────────

pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";
pub const METHOD_NOTIFICATIONS_PROGRESS: &str = "notifications/progress";
pub const METHOD_RESOURCES_LIST: &str = "resources/list";
pub const METHOD_PROMPTS_LIST: &str = "prompts/list";
pub const METHOD_MCP_STATUS: &str = "mcp_status";
