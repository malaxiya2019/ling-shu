//! 🏗️ rmcp MCP Server — 标准 MCP 协议服务端.
//!
//! 将 lingshu 的 Tool Registry 包装为标准 MCP 协议服务，
//! 兼容 Anthropic, Claude Code, Cursor 等 MCP 客户端。
//!
//! ## 使用
//!
//! ```no_run
//! use lingshu_mcp::rmcp_server::McpServerBridge;
//!
//! // 创建桥接服务器
//! let bridge = McpServerBridge::new();
//!
//! // 注册 lingshu 工具
//! bridge.register_tool(my_tool);
//!
//! // 启动 MCP 服务器 (stdio 模式)
//! bridge.serve_stdio().await.unwrap();
//! ```

use axum::{extract::State, routing, Router};
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::tool::Tool;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// JSON-RPC 方法枚举.
#[derive(Debug, Clone)]
enum McpMethod {
    ListTools,
    CallTool,
    ListResources,
    ListPrompts,
    Unknown(String),
}

impl From<&str> for McpMethod {
    fn from(s: &str) -> Self {
        match s {
            "tools/list" => Self::ListTools,
            "tools/call" => Self::CallTool,
            "resources/list" => Self::ListResources,
            "prompts/list" => Self::ListPrompts,
            other => Self::Unknown(other.to_string()),
        }
    }
}

/// JSON-RPC 请求.
#[derive(Debug, Clone, serde::Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Option<Value>,
    id: Option<Value>,
}

/// JSON-RPC 响应.
#[derive(Debug, Clone, serde::Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: Value,
}

#[derive(Debug, Clone, serde::Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// MCP 服务桥接 — 将 lingshu 工具暴露为标准 MCP 协议.
pub struct McpServerBridge {
    tools: Arc<RwLock<HashMap<String, Box<dyn Tool>>>>,
}

impl Default for McpServerBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServerBridge {
    /// 创建新的 MCP 桥接服务器.
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册一个 lingshu 工具.
    pub async fn register_tool(&self, tool: Box<dyn Tool>) {
        let info = tool.info();
        self.tools.write().await.insert(info.name.clone(), tool);
    }

    /// 批量注册工具.
    pub async fn register_tools(&self, tools: Vec<Box<dyn Tool>>) {
        for tool in tools {
            self.register_tool(tool).await;
        }
    }

    /// 处理单个 JSON-RPC 请求.
    pub async fn handle_request(&self, request_str: &str) -> String {
        let request: JsonRpcRequest = match serde_json::from_str(request_str) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::to_string(&JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                        data: None,
                    }),
                    id: Value::Null,
                })
                .unwrap_or_default();
            }
        };

        let id = request.id.clone().unwrap_or(Value::Null);
        let method = McpMethod::from(request.method.as_str());

        match method {
            McpMethod::ListTools => self.handle_list_tools(id).await,
            McpMethod::CallTool => {
                self.handle_call_tool(id, request.params.unwrap_or(Value::Null))
                    .await
            }
            McpMethod::ListResources => self.handle_list_resources(id).await,
            McpMethod::ListPrompts => self.handle_list_prompts(id).await,
            McpMethod::Unknown(m) => {
                json_rpc_error(id, -32601, format!("Method not found: {m}"), None)
            }
        }
    }

    /// tools/list — 列出所有已注册工具.
    async fn handle_list_tools(&self, id: Value) -> String {
        let tools = self.tools.read().await;
        let tool_list: Vec<Value> = tools
            .values()
            .map(|t| {
                let info = t.info();
                serde_json::json!({
                    "name": info.name,
                    "description": info.description,
                    "inputSchema": {
                        "type": "object",
                        "properties": info.parameters.iter().map(|p| {
                            (p.name.clone(), serde_json::json!({
                                "type": p.param_type,
                                "description": p.description,
                                "required": p.required,
                            }))
                        }).collect::<serde_json::Map<_, _>>(),
                        "required": info.parameters.iter()
                            .filter(|p| p.required)
                            .map(|p| p.name.clone())
                            .collect::<Vec<_>>(),
                    },
                })
            })
            .collect();

        json_rpc_result(id, serde_json::json!({ "tools": tool_list }))
    }

    /// tools/call — 调用指定工具.
    async fn handle_call_tool(&self, id: Value, params: Value) -> String {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        let tools = self.tools.read().await;
        match tools.get(name) {
            Some(tool) => {
                let ctx = LsContext::with_session(LsId::new());
                match tool.execute(ctx, arguments).await {
                    Ok(result) => json_rpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": result.to_string(),
                            }],
                            "isError": false,
                        }),
                    ),
                    Err(e) => json_rpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Error: {e}"),
                            }],
                            "isError": true,
                        }),
                    ),
                }
            }
            None => json_rpc_error(id, -32602, format!("Tool not found: {name}"), None),
        }
    }

    /// resources/list — 列出可用资源 (预留).
    async fn handle_list_resources(&self, id: Value) -> String {
        json_rpc_result(id, serde_json::json!({ "resources": [] }))
    }

    /// prompts/list — 列出可用提示 (预留).
    async fn handle_list_prompts(&self, id: Value) -> String {
        json_rpc_result(id, serde_json::json!({ "prompts": [] }))
    }

    /// 以 stdio 模式运行 MCP 服务器 (逐行读取 stdin).
    pub async fn serve_stdio(&self) -> LsResult<()> {
        use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

        let reader = BufReader::new(io::stdin());
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let response = self.handle_request(&line).await;
            let mut stdout = io::stdout();
            stdout.write_all(response.as_bytes()).await.unwrap();
            stdout.write_all(b"\n").await.unwrap();
            stdout.flush().await.unwrap();
        }

        Ok(())
    }

    /// 以 HTTP 模式运行 MCP 服务器.
    pub async fn serve_http(&self, addr: &str) -> LsResult<()> {
        let bridge = Arc::new(McpServerBridge {
            tools: self.tools.clone(),
        });

        let app = Router::new()
            .route("/mcp", routing::post(handle_mcp_post))
            .with_state(bridge);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| lingshu_core::LsError::Plugin(format!("MCP HTTP bind failed: {e}")))?;

        axum::serve(listener, app.into_make_service())
            .await
            .map_err(|e| lingshu_core::LsError::Plugin(format!("MCP HTTP serve failed: {e}")))?;

        Ok(())
    }
}

/// axum handler for MCP POST requests.
async fn handle_mcp_post(State(bridge): State<Arc<McpServerBridge>>, body: String) -> String {
    bridge.handle_request(&body).await
}

// ── Helpers ─────────────────────────────────────────

fn json_rpc_result(id: Value, result: Value) -> String {
    serde_json::to_string(&JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: Some(result),
        error: None,
        id,
    })
    .unwrap_or_default()
}

fn json_rpc_error(id: Value, code: i64, message: String, data: Option<Value>) -> String {
    serde_json::to_string(&JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data,
        }),
        id,
    })
    .unwrap_or_default()
}
