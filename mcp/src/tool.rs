//! MCP Tool Adapter — 将 lingshu-traits::Tool 包装为 MCP 格式
//!
//! 提供工具信息转换、标准执行、以及支持进度回调的扩展执行。
use tracing::Instrument;

use crate::types::{McpContent, McpTool, ProgressContext, ToolsCallResult};
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
        prop.insert(
            "description".into(),
            Value::String(param.description.clone()),
        );
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

/// 执行工具调用并返回 MCP 格式结果（无进度）
pub async fn execute_tool_to_mcp(
    tool: &dyn Tool,
    ctx: lingshu_core::LsContext,
    args: Value,
) -> ToolsCallResult {
    let tool_name = tool.info().name.clone();
    let span = tracing::info_span!(
        "gen_ai",
        gen_ai.operation.name = "tool.call",
        gen_ai.tool.name = %tool_name,
        trace_id = %ctx.trace_id,
        session_id = %ctx.session_id,
    );
    match tool.execute(ctx, args).instrument(span).await {
        Ok(result) => {
            let text = if result.is_object() || result.is_array() {
                serde_json::to_string_pretty(&result).unwrap_or_default()
            } else {
                result.to_string()
            };
            ToolsCallResult {
                content: vec![McpContent::Text { text }],
                is_error: false,
                execution_id: None,
            }
        }
        Err(e) => ToolsCallResult {
            content: vec![McpContent::Text {
                text: format!("Error: {}", e),
            }],
            is_error: true,
            execution_id: None,
        },
    }
}

/// 执行工具调用并返回 MCP 格式结果（支持进度回调）
///
/// 若提供了 `ProgressContext`，则将其注入到工具的调用上下文中，
/// 工具可以通过 `ctx.metadata` 访问进度令牌，并在执行过程中调用进度回调。
///
/// `ProgressContext` 不直接传给 Tool trait（保持 trait 干净），
/// 而是通过包装的方式 — 如果调用方需要进度感知，使用本函数。
pub async fn execute_tool_to_mcp_with_progress(
    tool: &dyn Tool,
    ctx: lingshu_core::LsContext,
    args: Value,
    progress: Option<ProgressContext>,
) -> ToolsCallResult {
    // 如果有进度上下文，先报告开始
    if let Some(ref p) = progress {
        p.report(0.0, Some(100.0), Some("Starting...".into()));
    }

    // 如果工具自身实现了进度感知（通过 metadata 传递 progress token），
    // 把 progress token 注入到 context metadata 中
    let ctx = if let Some(ref p) = progress {
        ctx.with_metadata("progress_token", p.progress_token.clone())
    } else {
        ctx
    };

    let tool_name = tool.info().name.clone();
    let span = tracing::info_span!(
        "gen_ai",
        gen_ai.operation.name = "tool.call",
        gen_ai.tool.name = %tool_name,
        trace_id = %ctx.trace_id,
        session_id = %ctx.session_id,
    );
    let result = match tool.execute(ctx, args).instrument(span).await {
        Ok(result) => {
            // 报告 100% 完成
            if let Some(ref p) = progress {
                p.report(100.0, Some(100.0), Some("Completed".into()));
            }
            let text = if result.is_object() || result.is_array() {
                serde_json::to_string_pretty(&result).unwrap_or_default()
            } else {
                result.to_string()
            };
            ToolsCallResult {
                content: vec![McpContent::Text { text }],
                is_error: false,
                execution_id: None,
            }
        }
        Err(e) => {
            if let Some(ref p) = progress {
                p.report(100.0, Some(100.0), Some(format!("Failed: {}", e)));
            }
            ToolsCallResult {
                content: vec![McpContent::Text {
                    text: format!("Error: {}", e),
                }],
                is_error: true,
                execution_id: None,
            }
        }
    };

    result
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

    #[test]
    fn test_tool_info_to_mcp() {
        let tool = EchoTool;
        let info = tool.info();
        let mcp = tool_info_to_mcp(&info);
        assert_eq!(mcp.name, "echo");
        assert!(mcp.input_schema.is_some());
    }

    #[tokio::test]
    async fn test_execute_with_progress() {
        let tool = EchoTool;
        let ctx = LsContext::with_session(LsId::new());
        let args = serde_json::json!({ "message": "hello" });

        // 创建进度回调来捕获进度
        let progress_reported = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = progress_reported.clone();
        let callback: crate::ProgressCallback = std::sync::Arc::new(move |p, t, m| {
            let mut reports = captured.lock().unwrap();
            reports.push((p, t, m));
        });

        let progress = ProgressContext::new("test-token".into(), callback);

        let result = execute_tool_to_mcp_with_progress(&tool, ctx, args, Some(progress)).await;

        assert!(!result.is_error);

        // 验证进度被报告了（开始 0% 和完成 100%）
        let reports = progress_reported.lock().unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].0, 0.0); // 开始
        assert_eq!(reports[1].0, 100.0); // 完成
    }
}
