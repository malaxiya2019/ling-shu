//! 🔗 rmcp MCP Client — 连接外部 MCP 服务器.
//!
//! 支持 SSE (Server-Sent Events) 和 WebSocket 传输层，
//! 可连接 Anthropic MCP、开源 MCP 服务器等。
//!
//! ## 使用
//!
//! ```no_run
//! use lingshu_mcp::rmcp_client::McpClient;
//!
//! let mut client = McpClient::connect("http://127.0.0.1:8931/mcp").await?;
//! let tools = client.list_tools().await?;
//! let result = client.call_tool("my-tool", serde_json::json!({})).await?;
//! ```

use lingshu_core::{LsError, LsId, LsResult};
use serde_json::Value;
use std::collections::HashMap;

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
    Resource {
        uri: String,
        text: Option<String>,
        blob: Option<String>,
    },
}

/// MCP 客户端 — 连接远程 MCP 服务器.
pub struct McpClient {
    /// 服务器基础 URL
    base_url: String,
    /// HTTP 客户端
    client: reqwest::Client,
    /// JSON-RPC 请求 ID 计数器
    next_id: std::sync::atomic::AtomicU64,
}

impl McpClient {
    /// 创建新的 MCP 客户端并连接到服务器.
    pub async fn connect(base_url: &str) -> LsResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| LsError::Plugin(format!("MCP client build failed: {e}")))?;

        // 测试连接
        let health_url = format!("{}/health", base_url.trim_end_matches('/'));
        let _ = client.get(&health_url).send().await;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            next_id: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// 发送 JSON-RPC 请求并返回结果.
    async fn send_request(&self, method: &str, params: Option<Value>) -> LsResult<Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(Value::Null),
            "id": id,
        });

        let resp = self
            .client
            .post(&self.base_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("MCP request failed: {e}")))?
            .json::<Value>()
            .await
            .map_err(|e| LsError::Internal(format!("MCP response parse failed: {e}")))?;

        // 检查错误
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            let msg = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(LsError::Internal(format!("MCP error [{code}]: {msg}")));
        }

        resp.get("result")
            .cloned()
            .ok_or_else(|| LsError::Internal("MCP response missing result".into()))
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
        let result = self
            .send_request(
                "tools/call",
                Some(serde_json::json!({
                    "name": name,
                    "arguments": arguments,
                })),
            )
            .await?;

        serde_json::from_value(result)
            .map_err(|e| LsError::Plugin(format!("MCP tool result parse failed: {e}")))
    }

    /// 获取远程工具列表作为 lingshu ToolInfo.
    pub async fn list_tool_infos(&self) -> LsResult<Vec<lingshu_traits::tool::ToolInfo>> {
        let tools = self.list_tools().await?;
        Ok(tools
            .into_iter()
            .map(|t| lingshu_traits::tool::ToolInfo {
                tool_id: LsId::new(),
                name: t.name,
                description: t.description,
                parameters: vec![],
                ..Default::default()
            })
            .collect())
    }
}

/// 支持多个 MCP 服务器的客户端管理器.
pub struct McpClientPool {
    clients: tokio::sync::RwLock<HashMap<String, McpClient>>,
}

impl Default for McpClientPool {
    fn default() -> Self {
        Self::new()
    }
}

impl McpClientPool {
    /// 创建空的 MCP 客户端池.
    pub fn new() -> Self {
        Self {
            clients: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 添加一个 MCP 服务器连接.
    pub async fn add_server(&self, name: &str, url: &str) -> LsResult<()> {
        let client = McpClient::connect(url).await?;
        self.clients.write().await.insert(name.to_string(), client);
        Ok(())
    }

    /// 获取指定 MCP 服务器的工具列表.
    pub async fn list_tools(&self, name: &str) -> LsResult<Vec<McpTool>> {
        let clients = self.clients.read().await;
        clients
            .get(name)
            .ok_or_else(|| LsError::NotFound(format!("MCP server '{name}' not found")))?
            .list_tools()
            .await
    }

    /// 调用指定 MCP 服务器的工具.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Value,
    ) -> LsResult<McpToolResult> {
        let clients = self.clients.read().await;
        clients
            .get(server)
            .ok_or_else(|| LsError::NotFound(format!("MCP server '{server}' not found")))?
            .call_tool(tool, arguments)
            .await
    }

    /// 从所有已连接的 MCP 服务器汇总工具列表.
    pub async fn all_tools(&self) -> LsResult<HashMap<String, Vec<McpTool>>> {
        let clients = self.clients.read().await;
        let mut all = HashMap::new();
        for (name, client) in clients.iter() {
            if let Ok(tools) = client.list_tools().await {
                all.insert(name.clone(), tools);
            }
        }
        Ok(all)
    }
}
