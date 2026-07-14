//! 🔌 MCP Stdio Client — 通过 stdin/stdout 连接外部 MCP 服务器.
//!
//! 用于连接使用 stdio 传输层的 MCP 服务器（如 agent-device mcp）。
//! 通过启动子进程并通过 stdin/stdout 进行 JSON-RPC 2.0 通信。
//!
//! ## 使用
//!
//! ```no_run
//! use lingshu_mcp::rmcp_stdio_client::{McpStdioClient, McpStdioConfig};
//!
//! let config = McpStdioConfig {
//!     command: "agent-device".into(),
//!     args: vec!["mcp".into()],
//!     ..Default::default()
//! };
//!
//! let mut client = McpStdioClient::spawn(config).await?;
//! let tools = client.list_tools().await?;
//! let result = client.call_tool("snapshot", serde_json::json!({"interactive": true})).await?;
//! client.shutdown().await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use lingshu_core::{LsError, LsResult, LsId};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// MCP 工具定义 (从远程服务器获取).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// MCP 工具调用结果.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}

/// MCP 内容块.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { uri: String, text: Option<String>, blob: Option<String> },
}

/// Stdio MCP 客户端配置.
#[derive(Debug, Clone)]
pub struct McpStdioConfig {
    /// 要执行的命令 (如 "agent-device" / "npx").
    pub command: String,
    /// 命令参数 (如 ["mcp"]).
    pub args: Vec<String>,
    /// 工作目录.
    pub work_dir: Option<String>,
    /// 环境变量.
    pub env: HashMap<String, String>,
    /// 子进程启动超时（毫秒）.
    pub spawn_timeout_ms: u64,
    /// 工具调用默认超时（毫秒）.
    pub tool_timeout_ms: u64,
    /// 重启最大次数.
    pub max_restarts: u32,
}

impl Default for McpStdioConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            work_dir: None,
            env: HashMap::new(),
            spawn_timeout_ms: 10_000,
            tool_timeout_ms: 120_000,
            max_restarts: 3,
        }
    }
}

/// 待处理的请求.
struct PendingResponse {
    /// 响应发送通知.
    sender: tokio::sync::oneshot::Sender<LsResult<Value>>,
}

/// Stdio MCP 客户端 — 通过子进程的 stdin/stdout 进行 MCP 通信.
pub struct McpStdioClient {
    /// 子进程
    child: Mutex<Option<Child>>,
    /// stdin 写入端
    stdin: Mutex<Option<tokio::process::ChildStdin>>,
    /// 待处理请求映射 (Arc<RwLock<...>> 可用于 clone)
    pending: Arc<RwLock<HashMap<u64, PendingResponse>>>,
    /// JSON-RPC ID 计数器
    next_id: AtomicU64,
    /// 配置
    config: McpStdioConfig,
    /// 是否已启动
    started: AtomicBool,
    /// 重启计数
    restart_count: AtomicU32,
}

impl McpStdioClient {
    /// 创建一个新的 stdio MCP 客户端（尚未启动子进程）.
    pub fn new(config: McpStdioConfig) -> Self {
        Self {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            pending: Arc::new(RwLock::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            config,
            started: AtomicBool::new(false),
            restart_count: AtomicU32::new(0),
        }
    }

    /// 启动子进程并连接 MCP 服务器.
    pub async fn spawn(config: McpStdioConfig) -> LsResult<Self> {
        let client = Self::new(config);
        client.start_process().await?;
        Ok(client)
    }

    /// 启动（或重启）子进程.
    async fn start_process(&self) -> LsResult<()> {
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(dir) = &self.config.work_dir {
            cmd.current_dir(dir);
        }

        for (k, v) in &self.config.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(|e| {
            LsError::Plugin(format!(
                "failed to spawn MCP server '{}': {e}",
                self.config.command
            ))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            LsError::Plugin("failed to capture child stdin".into())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            LsError::Plugin("failed to capture child stdout".into())
        })?;

        let stderr = child.stderr.take();

        // 启动 stdout 读取任务
        let pending = self.pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        debug!("MCP stdio client: stdout closed");
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        // 尝试解析 JSON-RPC 响应
                        match serde_json::from_str::<Value>(trimmed) {
                            Ok(resp) => {
                                let id = resp.get("id").and_then(|v| v.as_u64());
                                if let Some(id) = id {
                                    let mut pending_map = pending.write().await;
                                    if let Some(pending_resp) = pending_map.remove(&id) {
                                        // 检查是否有错误
                                        if let Some(err) = resp.get("error") {
                                            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                                            let msg = err
                                                .get("message")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown error");
                                            let _ = pending_resp.sender.send(Err(
                                                LsError::Internal(format!("MCP error [{code}]: {msg}"))
                                            ));
                                        } else if let Some(result) = resp.get("result") {
                                            let _ = pending_resp.sender.send(Ok(result.clone()));
                                        } else {
                                            let _ = pending_resp.sender.send(Ok(resp));
                                        }
                                    }
                                }
                                // 忽略通知（无 id 的消息）
                            }
                            Err(e) => {
                                warn!("MCP stdio: failed to parse response: {e}, line: {trimmed}");
                            }
                        }
                    }
                    Err(e) => {
                        error!("MCP stdio: read error: {e}");
                        break;
                    }
                }
            }
        });

        // 启动 stderr 读取任务（只记录日志）
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                debug!("MCP server (stderr): {trimmed}");
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        *self.stdin.lock().await = Some(stdin);
        *self.child.lock().await = Some(child);
        self.started.store(true, Ordering::SeqCst);

        info!(
            command = %self.config.command,
            args = ?self.config.args,
            "MCP stdio client started"
        );

        Ok(())
    }

    /// 发送 JSON-RPC 请求并等待响应.
    async fn send_request(&self, method: &str, params: Option<Value>) -> LsResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
            "id": id,
        });

        // 创建 oneshot channel 用于接收响应
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending_map = self.pending.write().await;
            pending_map.insert(id, PendingResponse { sender: tx });
        }

        // 写入 stdin
        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard.as_mut().ok_or_else(|| {
                LsError::Plugin("MCP stdin not available".into())
            })?;

            let msg = serde_json::to_string(&request)
                .map_err(|e| LsError::Internal(format!("MCP serialization failed: {e}")))?;
            let mut msg_bytes = msg.into_bytes();
            msg_bytes.push(b'\n');

            if let Err(e) = stdin.write_all(&msg_bytes).await {
                self.pending.write().await.remove(&id);
                return Err(LsError::Plugin(format!("MCP write to stdin failed: {e}")));
            }

            // 刷新以确保发送
            if let Err(e) = stdin.flush().await {
                self.pending.write().await.remove(&id);
                return Err(LsError::Plugin(format!("MCP flush failed: {e}")));
            }
        }

        // 等待响应（带超时）
        let timeout = std::time::Duration::from_millis(self.config.tool_timeout_ms);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.pending.write().await.remove(&id);
                Err(LsError::Internal("MCP response channel closed".into()))
            }
            Err(_) => {
                self.pending.write().await.remove(&id);
                Err(LsError::Timeout(format!(
                    "MCP tool call '{}' timed out after {}ms",
                    method, self.config.tool_timeout_ms
                )))
            }
        }
    }

    /// tools/list — 列出远程服务器上的工具.
    pub async fn list_tools(&self) -> LsResult<Vec<McpTool>> {
        let result = self.send_request("tools/list", None).await?;
        let tools: Vec<McpTool> =
            serde_json::from_value(result.get("tools").cloned().unwrap_or(Value::Array(vec![])))
                .map_err(|e| LsError::Plugin(format!("MCP tools parse failed: {e}")))?;
        Ok(tools)
    }

    /// tools/call — 调用远程服务器上的工具.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> LsResult<McpToolResult> {
        let result = self.send_request(
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments,
            })),
        ).await?;

        serde_json::from_value(result)
            .map_err(|e| LsError::Plugin(format!("MCP tool result parse failed: {e}")))
    }

    /// 获取远程工具列表作为 lingshu ToolInfo.
    pub async fn list_tool_infos(&self) -> LsResult<Vec<lingshu_traits::tool::ToolInfo>> {
        let tools = self.list_tools().await?;
        Ok(tools
            .into_iter()
            .map(|t| {
                // 从 input_schema 提取参数
                let params = if let Some(schema) = t.input_schema.as_object() {
                    extract_params_from_schema(&Value::Object(schema.clone()))
                } else {
                    vec![]
                };

                lingshu_traits::tool::ToolInfo {
                    tool_id: LsId::new(),
                    name: t.name,
                    description: t.description,
                    parameters: params,
                    metadata: lingshu_traits::tool::ToolMetadata {
                        category: lingshu_traits::tool::ToolCategory::System,
                        tags: vec!["device".into(), "mobile".into(), "mcp".into()],
                        permission_level: lingshu_traits::tool::PermissionLevel::Admin,
                        timeout_ms: Some(self.config.tool_timeout_ms),
                        sandbox_config: None,
                        version: "1.0.0".into(),
                        author: "agent-device".into(),
                    },
                }
            })
            .collect())
    }

    /// 关闭子进程.
    pub async fn shutdown(&self) -> LsResult<()> {
        self.started.store(false, Ordering::SeqCst);

        let mut stdin_guard = self.stdin.lock().await;
        *stdin_guard = None; // 关闭 stdin

        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            // 尝试优雅关闭
            let _ = child.start_kill();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        info!("MCP stdio client shutdown");
        Ok(())
    }

    /// 检查是否已启动.
    pub fn is_running(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// 重启子进程.
    pub async fn restart(&self) -> LsResult<()> {
        let count = self.restart_count.fetch_add(1, Ordering::SeqCst);
        if count >= self.config.max_restarts {
            return Err(LsError::Plugin(format!(
                "MCP stdio client exceeded max restarts ({})",
                self.config.max_restarts
            )));
        }

        // 先关闭现有进程
        let _ = self.shutdown().await;

        // 短暂等待
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // 重新启动
        self.start_process().await
    }

    /// 健康检查 — 尝试调用 tools/list 确认服务器仍在运行.
    pub async fn health_check(&self) -> LsResult<()> {
        self.list_tools().await?;
        Ok(())
    }
}

/// 从 JSON Schema 提取 ToolParam 列表.
fn extract_params_from_schema(schema: &Value) -> Vec<lingshu_traits::tool::ToolParam> {
    let mut params = Vec::new();

    if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
        let required = schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();

        // 跳过 MCP 配置字段
        let skip_fields = ["stateDir", "session", "outputFormat", "responseLevel"];

        for (name, prop) in properties {
            if skip_fields.contains(&name.as_str()) {
                continue;
            }

            let description = prop
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let param_type = prop
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("string")
                .to_string();

            params.push(lingshu_traits::tool::ToolParam {
                name: name.clone(),
                description,
                required: required.contains(name),
                param_type,
            });
        }
    }

    params
}


/// 同步清理 — 确保子进程在 Drop 时被终止.
impl Drop for McpStdioClient {
    fn drop(&mut self) {
        if self.started.load(Ordering::SeqCst) {
            self.started.store(false, Ordering::SeqCst);

            // 尝试同步杀死子进程
            if let Ok(mut child_opt) = self.child.try_lock() {
                if let Some(mut child) = child_opt.take() {
                    let _ = child.start_kill();
                    // 不能 .wait() 因为那是异步的，但 start_kill 会发送信号
                    tracing::debug!("MCP stdio client: child process killed on drop");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_params_from_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "platform": {
                    "type": "string",
                    "description": "Target platform"
                },
                "interactive": {
                    "type": "boolean",
                    "description": "Show only interactive elements"
                }
            },
            "required": ["platform"]
        });

        let params = extract_params_from_schema(&schema);
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "platform" && p.required));
        assert!(params.iter().any(|p| p.name == "interactive" && !p.required));
    }

    #[test]
    fn test_skip_mcp_config_fields() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string" },
                "stateDir": { "type": "string" },
                "session": { "type": "string" }
            },
            "required": []
        });

        let params = extract_params_from_schema(&schema);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "platform");
    }

    #[test]
    fn test_mcp_tool_serde() {
        let tool = McpTool {
            name: "snapshot".into(),
            description: "Take a snapshot of the current screen".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "interactive": { "type": "boolean" }
                }
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let parsed: McpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "snapshot");
    }

    #[test]
    fn test_mcp_result_serde() {
        let result = McpToolResult {
            content: vec![McpContent::Text { text: "Hello".into() }],
            is_error: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: McpToolResult = serde_json::from_str(&json).unwrap();
        assert!(!parsed.is_error);
        assert_eq!(parsed.content.len(), 1);
    }
}
