//! MCP Tool Adapter — 将 lingshu-traits::Tool 包装为 MCP 格式

use crate::types::{McpContent, McpTool, ToolsCallResult};
use lingshu_traits::tool::{Tool, ToolInfo};
use serde_json::Value;
use std::sync::Arc;

/// 将 ToolInfo 转换为 MCP Tool 定义
pub fn tool_info_to_mcp(info: &ToolInfo) -> McpTool {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in &info.parameters {
        let mut prop = serde_json::Map::new();
        prop.insert("type".into(), Value::String(param.param_type.clone()));
        prop.insert("description".into(), Value::String(param.description.clone()));
        properties.insert(param.name.clone(), Value::Object(prop));
        if param.required {
            required.push(param.name.clone());
        }
    }

    let input_schema = serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    });

    McpTool {
        name: info.name.clone(),
        description: info.description.clone(),
        input_schema: Some(input_schema),
    }
}

/// 执行工具调用并返回 MCP 格式结果
pub async fn execute_tool_to_mcp(
    tool: &dyn Tool,
    ctx: lingshu_core::LsContext,
    args: Value,
) -> ToolsCallResult {
    match tool.execute(ctx, args).await {
        Ok(result) => {
            let text = if result.is_object() || result.is_array() {
                serde_json::to_string_pretty(&result).unwrap_or_default()
            } else {
                result.to_string()
            };
            ToolsCallResult {
                content: vec![McpContent::Text { text }],
                is_error: false,
            }
        }
        Err(e) => ToolsCallResult {
            content: vec![McpContent::Text {
                text: format!("Error: {}", e),
            }],
            is_error: true,
        },
    }
}

/// 列出所有工具（MCP 格式）
pub fn list_tools(tools: &[Arc<dyn Tool>]) -> Vec<McpTool> {
    tools.iter().map(|t| tool_info_to_mcp(&t.info())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::{LsContext, LsId, LsResult};
    use lingshu_traits::tool::{ToolInfo, ToolParam};

    struct EchoTool;

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

        fn validate(&self, _input: &Value) -> LsResult<()> {
            Ok(())
        }

        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            Ok(input)
        }
    }

    #[test]
    fn test_tool_info_to_mcp() {
        let tool = EchoTool;
        let info = tool.info();
        let mcp = tool_info_to_mcp(&info);
        assert_eq!(mcp.name, "echo");
        assert!(mcp.input_schema.is_some());
    }
}
