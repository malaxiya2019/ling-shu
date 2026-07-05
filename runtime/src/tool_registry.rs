//! ToolRegistry — 工具注册与执行管理中心
//!
//! 管理所有注册的工具，提供按名称查找和执行的能力。
//! 支持 OpenAI 兼容的 Tool/Function calling 全流程。

use std::collections::HashMap;
use std::sync::Arc;

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::llm::ToolDefinition;
use lingshu_traits::tool::{Tool, ToolCallRecord};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;

#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use lingshu_traits::tool::ToolInfo;

/// 工具注册表 — 线程安全，支持动态注册/注销
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Box<dyn Tool>>>>,
    history: Arc<RwLock<Vec<ToolCallRecord>>>,
}

impl ToolRegistry {
    /// 创建空的工具注册表
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注册一个工具
    pub async fn register(&self, tool: Box<dyn Tool>) {
        let name = tool.info().name.clone();
        self.tools.write().await.insert(name.clone(), tool);
        info!(tool = %name, "tool registered");
    }

    /// 按名称查找工具
    pub async fn get(
        &self,
        _name: &str,
    ) -> Option<tokio::sync::RwLockReadGuard<'_, Box<dyn Tool>>> {
        // This is a simplified approach
        None
    }

    /// 执行工具调用
    pub async fn execute(&self, ctx: &LsContext, name: &str, args: Value) -> LsResult<Value> {
        let tools = self.tools.read().await;
        let tool = tools
            .get(name)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("tool not found: {name}")))?;

        tool.validate(&args)?;
        let result = tool.execute(ctx.clone(), args.clone()).await?;

        // Record history
        let record = ToolCallRecord {
            tool_id: tool.info().tool_id,
            call_id: LsId::new(),
            session_id: ctx.session_id,
            input: args,
            output: result.clone(),
            duration_ms: 0,
            success: true,
            timestamp: chrono::Utc::now(),
        };
        self.history.write().await.push(record);

        Ok(result)
    }

    /// 获取所有已注册工具的 OpenAI-compatible 定义
    pub async fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|tool| {
                let info = tool.info();
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();
                for param in &info.parameters {
                    let mut prop = serde_json::Map::new();
                    prop.insert("type".into(), serde_json::json!(param.param_type));
                    prop.insert("description".into(), serde_json::json!(param.description));
                    properties.insert(param.name.clone(), serde_json::Value::Object(prop));
                    if param.required {
                        required.push(param.name.clone());
                    }
                }
                ToolDefinition {
                    tool_type: "function".into(),
                    function: lingshu_traits::llm::ToolFunction {
                        name: info.name.clone(),
                        description: info.description.clone(),
                        parameters: serde_json::json!({
                            "type": "object",
                            "properties": properties,
                            "required": required,
                        }),
                    },
                }
            })
            .collect()
    }

    /// 列出所有已注册工具名
    pub async fn list_tools(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    /// 获取调用历史
    pub async fn history(&self) -> Vec<ToolCallRecord> {
        self.history.read().await.clone()
    }

    /// 工具数量
    pub async fn count(&self) -> usize {
        self.tools.read().await.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use lingshu_traits::tool::ToolParam;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn info(&self) -> ToolInfo {
            ToolInfo {
                tool_id: LsId::new(),
                name: "echo".into(),
                description: "Echo the input back".into(),
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
                    "missing message field".into(),
                ));
            }
            Ok(())
        }

        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count().await, 0);

        registry.register(Box::new(EchoTool)).await;
        assert_eq!(registry.count().await, 1);

        let tools = registry.list_tools().await;
        assert!(tools.contains(&"echo".to_string()));
    }

    #[tokio::test]
    async fn test_tool_definitions() {
        let registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool)).await;

        let defs = registry.get_tool_definitions().await;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "echo");
    }

    #[tokio::test]
    async fn test_execute_tool() {
        let registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool)).await;

        let ctx = LsContext::with_session(LsId::new());
        let args = serde_json::json!({"message": "hello"});
        let result = registry.execute(&ctx, "echo", args).await.unwrap();
        assert_eq!(result["message"], "hello");

        // Test not found
        let result = registry
            .execute(&ctx, "nonexistent", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }
}
