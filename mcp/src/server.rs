//! MCP Server — JSON-RPC 2.0 请求分发器
//!
//! 接收 JSON-RPC 请求，路由到对应处理方法，返回 JSON-RPC 响应。
//! 与 HTTP 层解耦，可被 axum handler 调用。

use crate::tool;
use crate::types::*;
use lingshu_core::LsContext;
use lingshu_traits::tool::Tool;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

/// MCP 服务器 — 处理 JSON-RPC 请求
pub struct McpServer {
    /// 已注册的 MCP 工具
    tools: Vec<Arc<dyn Tool>>,
}

impl McpServer {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
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

    /// 处理 JSON-RPC 请求，返回 JSON-RPC 响应
    pub async fn handle_request(
        &self,
        ctx: &LsContext,
        body: Value,
    ) -> JsonRpcResponse<Value> {
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
            METHOD_TOOLS_CALL => {
                self.handle_tools_call(ctx, request_id, params).await
            }
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
    async fn handle_tools_list(
        &self,
        id: u64,
        _params: Value,
    ) -> JsonRpcResponse<Value> {
        let tools: Vec<McpTool> = self.tools.iter().map(|t| tool::tool_info_to_mcp(&t.info())).collect();

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

    /// `tools/call` — 调用指定工具
    async fn handle_tools_call(
        &self,
        ctx: &LsContext,
        id: u64,
        params: Value,
    ) -> JsonRpcResponse<Value> {
        // 解析参数
        let name = params.get("name").and_then(|v| v.as_str());
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        let name = match name {
            Some(n) => n,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcError::new(INVALID_PARAMS, "name is required")),
                    id,
                };
            }
        };

        // 查找工具
        let tool = match self.tools.iter().find(|t| t.info().name == name) {
            Some(t) => t.clone(),
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcError::new(
                        INVALID_PARAMS,
                        format!("tool not found: {name}"),
                    )),
                    id,
                };
            }
        };

        // 校验参数
        if let Err(e) = tool.validate(&arguments) {
            return JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(JsonRpcError::with_data(
                    INVALID_PARAMS,
                    format!("invalid params: {e}"),
                    arguments,
                )),
                id,
            };
        }

        // 执行
        let call_result = tool::execute_tool_to_mcp(tool.as_ref(), ctx.clone(), arguments).await;

        let result = serde_json::to_value(&call_result).unwrap_or_default();

        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    /// 获取已注册工具数量
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::{LsId, LsResult};
    use lingshu_traits::tool::{ToolInfo, ToolParam};

    struct EchoTool;
    struct AddTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                tool_id: LsId::new(),
                name: "echo".into(),
                description: "Echo input back".into(),
                parameters: vec![ToolParam {
                    name: "message".into(),
                    description: "Message to echo".into(),
                    required: true,
                    param_type: "string".into(),
                }],
            }
        }

        fn validate(&self, input: &Value) -> LsResult<()> {
            if input.get("message").and_then(|v| v.as_str()).is_none() {
                return Err(lingshu_core::LsError::Validation(
                    "missing message".into(),
                ));
            }
            Ok(())
        }

        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            Ok(input)
        }
    }

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
            }
        }

        fn validate(&self, input: &Value) -> LsResult<()> {
            if !input.get("a").and_then(|v| v.as_f64()).is_some() {
                return Err(lingshu_core::LsError::Validation("missing a".into()));
            }
            if !input.get("b").and_then(|v| v.as_f64()).is_some() {
                return Err(lingshu_core::LsError::Validation("missing b".into()));
            }
            Ok(())
        }

        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            let a = input.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = input.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(serde_json::json!({ "result": a + b }))
        }
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
        assert_eq!(result.tools.len(), 2);
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
        assert!(resp.result.is_some(), "expected result, got error: {:?}", resp.error);

        let result: ToolsCallResult = serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] { McpContent::Text { text } => text.as_str(), _ => "" }; assert!(text.contains("7"), "expected result containing 7, got: {text}");
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let server = McpServer::new();
        let ctx = LsContext::with_session(LsId::new());
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "unknown",
            "params": {},
            "id": 3,
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
            "id": 4,
        });

        let resp = server.handle_request(&ctx, body).await;
        assert!(resp.error.is_some());
    }
}
