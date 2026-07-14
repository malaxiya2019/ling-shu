//! 🌉 MCP Bridge Tool — 将 agent-device 的 MCP 工具包装为 Lingshu Tool.
//!
//! 通过 `McpStdioClient` 与 agent-device 子进程通信，
//! 将远程 MCP 工具暴露为 Lingshu ToolRegistry 中的标准工具。

use std::sync::Arc;

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_mcp::rmcp_stdio_client::{McpContent, McpStdioClient, McpToolResult};
use lingshu_traits::tool::{
    PermissionLevel, SandboxConfig, Tool, ToolCategory, ToolInfo, ToolMetadata, ToolParam,
};
use serde_json::Value;

/// MCP 桥接工具 — 将远端的 MCP 工具调用包装为 Lingshu Tool.
pub struct McpBridgeTool {
    info: ToolInfo,
    /// 共享的 MCP stdio 客户端引用.
    client: Arc<McpStdioClient>,
    /// 输出格式（optimized/json）
    output_format: String,
}

impl McpBridgeTool {
    /// 创建一个新的 MCP 桥接工具.
    pub fn new(
        name: String,
        description: String,
        input_schema: Value,
        client: Arc<McpStdioClient>,
        output_format: String,
    ) -> Self {
        // 从 input_schema 提取参数
        let params = extract_params(&input_schema);
        let tool_timeout = 120_000; // 默认 120s

        let info = ToolInfo {
            tool_id: LsId::new(),
            name: format!("device:{}", name),
            description,
            parameters: params,
            metadata: ToolMetadata {
                category: ToolCategory::System,
                tags: vec![
                    "device".into(),
                    "automation".into(),
                    "mcp".into(),
                    "agent-device".into(),
                ],
                permission_level: PermissionLevel::Admin,
                timeout_ms: Some(tool_timeout),
                sandbox_config: Some(SandboxConfig {
                    max_execution_ms: tool_timeout,
                    max_output_bytes: 10_000_000, // 10MB (截图等大输出)
                    network_isolated: false,
                    fs_isolated: false,
                    max_memory_mb: None,
                    special_permissions: vec!["device".into()],
                }),
                version: "2.0.0".into(),
                author: "agent-device".into(),
            },
        };

        Self {
            info,
            client,
            output_format,
        }
    }

    /// 将 MCP 结果转换为 JSON Value.
    fn mcp_result_to_value(result: McpToolResult) -> Value {
        let mut texts = Vec::new();
        for content in &result.content {
            match content {
                McpContent::Text { text } => texts.push(text.clone()),
                McpContent::Image { data, mime_type } => {
                    texts.push(format!("[image: {} ({} bytes)]", mime_type, data.len()));
                }
                McpContent::Resource { uri, text, .. } => {
                    if let Some(t) = text {
                        texts.push(format!("[resource: {uri}]\n{t}"));
                    } else {
                        texts.push(format!("[resource: {uri}]"));
                    }
                }
            }
        }

        serde_json::json!({
            "content": texts,
            "is_error": result.is_error,
            "text": texts.join("\n"),
        })
    }
}

#[async_trait]
impl Tool for McpBridgeTool {
    fn info(&self) -> ToolInfo {
        self.info.clone()
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        // 基本参数校验：检查必需参数
        for param in &self.info.parameters {
            if param.required && !input.has_key(&param.name) {
                // 忽略 MCP 内部参数
                if param.name != "stateDir" && param.name != "session" {
                    return Err(LsError::Validation(format!(
                        "missing required parameter '{}' for tool '{}'",
                        param.name, self.info.name
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        // 提取工具名称（去掉 device: 前缀）
        let tool_name = self
            .info
            .name
            .strip_prefix("device:")
            .unwrap_or(&self.info.name);

        // 注入 outputFormat（如果不为 optimized）
        let enhanced_input = if self.output_format != "optimized" {
            let mut map = match input {
                Value::Object(m) => m,
                _ => return Err(LsError::Validation("input must be an object".into())),
            };
            map.insert(
                "outputFormat".to_string(),
                Value::String(self.output_format.clone()),
            );
            Value::Object(map)
        } else {
            input
        };

        // 调用 MCP
        let result = self.client.call_tool(tool_name, enhanced_input).await?;
        Ok(Self::mcp_result_to_value(result))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(Self {
            info: self.info.clone(),
            client: self.client.clone(),
            output_format: self.output_format.clone(),
        })
    }
}

/// 从 JSON Schema 中提取 ToolParam 列表.
fn extract_params(schema: &Value) -> Vec<ToolParam> {
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

        // 跳过 MCP 内部配置字段
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

            params.push(ToolParam {
                name: name.clone(),
                description,
                required: required.contains(name),
                param_type,
            });
        }
    }

    params
}

/// 为 serde_json::Value 添加 has_key 辅助方法（简化使用）。
trait ValueExt {
    fn has_key(&self, key: &str) -> bool;
}

impl ValueExt for Value {
    fn has_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

use lingshu_core::LsError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_params() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "platform": {
                    "type": "string",
                    "description": "Target platform (ios, android)"
                },
                "bundleId": {
                    "type": "string",
                    "description": "App bundle identifier"
                }
            },
            "required": ["platform"]
        });

        let params = extract_params(&schema);
        assert_eq!(params.len(), 2);
        assert!(params.iter().any(|p| p.name == "platform" && p.required));
        assert!(params.iter().any(|p| p.name == "bundleId" && !p.required));
    }

    #[test]
    fn test_skip_config_fields() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string" },
                "stateDir": { "type": "string" }
            },
            "required": []
        });

        let params = extract_params(&schema);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "platform");
    }

    #[test]
    fn test_mcp_result_to_value_text() {
        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "Hello, world!".into(),
            }],
            is_error: false,
        };

        let value = McpBridgeTool::mcp_result_to_value(result);
        assert_eq!(value["text"], "Hello, world!");
        assert!(!value["is_error"].as_bool().unwrap());
    }

    #[test]
    fn test_mcp_result_to_value_multiple() {
        let result = McpToolResult {
            content: vec![
                McpContent::Text {
                    text: "Line 1".into(),
                },
                McpContent::Text {
                    text: "Line 2".into(),
                },
            ],
            is_error: false,
        };

        let value = McpBridgeTool::mcp_result_to_value(result);
        assert_eq!(value["text"], "Line 1\nLine 2");
    }

    #[test]
    fn test_new_tool_with_output_format() {
        let schema = serde_json::json!({"type": "object", "properties": {}});
        let client = Arc::new(McpStdioClient::new(
            lingshu_mcp::rmcp_stdio_client::McpStdioConfig::default(),
        ));

        let tool_optimized = McpBridgeTool::new(
            "snapshot".into(),
            "test".into(),
            schema.clone(),
            client.clone(),
            "optimized".into(),
        );
        assert_eq!(tool_optimized.output_format, "optimized");

        let tool_json = McpBridgeTool::new(
            "snapshot".into(),
            "test".into(),
            schema.clone(),
            client.clone(),
            "json".into(),
        );
        assert_eq!(tool_json.output_format, "json");
    }

    #[test]
    fn test_tool_name_format() {
        let schema = serde_json::json!({"type": "object", "properties": {}});
        let client = Arc::new(McpStdioClient::new(
            lingshu_mcp::rmcp_stdio_client::McpStdioConfig::default(),
        ));

        let tool = McpBridgeTool::new(
            "snapshot".into(),
            "Take a snapshot".into(),
            schema,
            client,
            "optimized".into(),
        );
        assert_eq!(tool.info.name, "device:snapshot");
        assert_eq!(tool.info.metadata.version, "2.0.0");
    }
}
