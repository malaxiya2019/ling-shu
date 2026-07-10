//! Workflow Tools — 将 WorkflowDag 暴露为 Agent 可调用的 Tool.
//!
//! 提供两个 Tool:
//! - `execute_workflow` — 执行已注册的工作流
//! - `list_workflows` — 列出可用工作流

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam, ToolMetadata, ToolCategory, PermissionLevel, SandboxConfig};
use serde_json::Value;
use std::sync::Arc;
use tracing::info;

use super::registry::WorkflowRegistry;

/// 工作流执行工具 — 将 WorkflowDag 暴露为 Agent 可调用的 Tool.
pub struct WorkflowExecuteTool {
    registry: Arc<tokio::sync::RwLock<WorkflowRegistry>>,
}

impl WorkflowExecuteTool {
    pub fn new(registry: Arc<tokio::sync::RwLock<WorkflowRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for WorkflowExecuteTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "execute_workflow".into(),
            description: "执行一个已注册的工作流。工作流是一个有向无环图(DAG)，包含多个步骤(节点)，支持依赖编排、并行执行、超时控制、重试和条件跳过。参数: name(工作流名称), input(可选输入数据)。".into(),
            parameters: vec![
                ToolParam {
                    name: "name".into(),
                    description: "工作流名称（必须先通过 list_workflows 查询）".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "input".into(),
                    description: "工作流输入数据（可选）".into(),
                    required: false,
                    param_type: "object".into(),
                },
            ],
            metadata: ToolMetadata {
                category: ToolCategory::Custom("workflow".into()),
                tags: vec!["workflow".into(), "automation".into()],
                permission_level: PermissionLevel::User,
                timeout_ms: Some(300_000),
                sandbox_config: Some(SandboxConfig {
                    max_execution_ms: 300_000,
                    max_output_bytes: 10_000_000,
                    network_isolated: false,
                    fs_isolated: false,
                    max_memory_mb: None,
                    special_permissions: Vec::new(),
                }),
                version: "1.0.0".into(),
                author: "lingshu".into(),
            },
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("name").and_then(|v| v.as_str()).is_none() {
            return Err(LsError::Validation("missing required field: 'name'".into()));
        }
        Ok(())
    }

    async fn execute(&self, ctx: LsContext, args: Value) -> LsResult<Value> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::InvalidArgument("missing 'name' field".into()))?;

        let input = args.get("input").cloned().unwrap_or(Value::Null);

        let registry = self.registry.read().await;
        let dag = registry
            .get(name)
            .await
            .ok_or_else(|| LsError::NotFound(format!("workflow '{name}' not found")))?;

        info!(
            workflow = %name,
            "workflow tool: executing workflow"
        );

        let result = dag.execute(ctx.child(), input).await?;

        Ok(serde_json::json!({
            "workflow": name,
            "success": result.success,
            "total_duration_ms": result.total_duration_ms,
            "node_count": result.node_results.len(),
            "results": result.node_results,
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(WorkflowExecuteTool {
            registry: self.registry.clone(),
        })
    }
}

/// 工作流列表工具 — 列出所有可用的工作流.
pub struct ListWorkflowsTool {
    registry: Arc<tokio::sync::RwLock<WorkflowRegistry>>,
}

impl ListWorkflowsTool {
    pub fn new(registry: Arc<tokio::sync::RwLock<WorkflowRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ListWorkflowsTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "list_workflows".into(),
            description: "列出所有已注册的工作流，包括名称、节点数、边数。".into(),
            parameters: vec![],
            metadata: ToolMetadata {
                category: ToolCategory::Custom("workflow".into()),
                tags: vec!["workflow".into()],
                permission_level: PermissionLevel::Public,
                timeout_ms: Some(10_000),
                sandbox_config: None,
                version: "1.0.0".into(),
                author: "lingshu".into(),
            },
        }
    }

    fn validate(&self, _input: &Value) -> LsResult<()> {
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, _args: Value) -> LsResult<Value> {
        let registry = self.registry.read().await;
        let entries = registry.list().await;

        Ok(serde_json::json!({
            "workflows": entries,
            "count": entries.len(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(ListWorkflowsTool {
            registry: self.registry.clone(),
        })
    }
}

/// 注册工作流工具到 ToolRegistry.
pub async fn register_workflow_tools(
    tool_registry: &lingshu_tool::ToolRegistry,
    workflow_registry: Arc<tokio::sync::RwLock<WorkflowRegistry>>,
) {
    tool_registry
        .register(Box::new(WorkflowExecuteTool::new(workflow_registry.clone())))
        .await;
    tool_registry
        .register(Box::new(ListWorkflowsTool::new(workflow_registry)))
        .await;
}

/// 创建 Agent 节点处理器 — 允许 WorkflowDag 中的节点通过 Agent 执行任务.
pub fn create_agent_node_handler(
    _agent_id: LsId,
    _agent_manager: Arc<tokio::sync::RwLock<super::dag::NodeHandler>>,
    default_handler: super::dag::NodeHandler,
) -> super::dag::NodeHandler {
    Arc::new(move |ctx: LsContext, input: Value| {
        let handler = default_handler.clone();
        Box::pin(async move {
            handler(ctx, input).await
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;

    #[tokio::test]
    async fn test_list_workflows_empty() {
        let registry = Arc::new(tokio::sync::RwLock::new(WorkflowRegistry::new()));
        let tool = ListWorkflowsTool::new(registry);

        let ctx = LsContext::with_session(LsId::new());
        let result = tool.execute(ctx, Value::Null).await.unwrap();
        assert_eq!(result["count"], 0);
    }

    #[tokio::test]
    async fn test_execute_workflow_not_found() {
        let registry = Arc::new(tokio::sync::RwLock::new(WorkflowRegistry::new()));
        let tool = WorkflowExecuteTool::new(registry);

        let ctx = LsContext::with_session(LsId::new());
        let result = tool
            .execute(ctx, serde_json::json!({"name": "nonexistent"}))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_tool_info() {
        let registry = Arc::new(tokio::sync::RwLock::new(WorkflowRegistry::new()));
        let tool = WorkflowExecuteTool::new(registry);
        let info = tool.info();
        assert_eq!(info.name, "execute_workflow");
    }

    #[test]
    fn test_list_workflows_tool_info() {
        let registry = Arc::new(tokio::sync::RwLock::new(WorkflowRegistry::new()));
        let tool = ListWorkflowsTool::new(registry);
        let info = tool.info();
        assert_eq!(info.name, "list_workflows");
    }
}
