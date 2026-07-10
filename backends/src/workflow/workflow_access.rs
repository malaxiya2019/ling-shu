//! WorkflowAccess impl — 为 WorkflowRegistry 实现 Runtime 的 WorkflowAccess trait.


use lingshu_core::{LsContext, LsResult};
use lingshu_runtime::agent_runtime::WorkflowAccess;
use serde_json::Value;

use super::WorkflowRegistry;

#[async_trait::async_trait]
impl WorkflowAccess for WorkflowRegistry {
    /// 列出所有工作流.
    async fn list_workflows(&self) -> Vec<Value> {
        let entries = self.list().await;
        entries
            .into_iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "id": e.id.to_string(),
                    "node_count": e.node_count,
                    "edge_count": e.edge_count,
                })
            })
            .collect()
    }

    /// 执行工作流.
    async fn execute_workflow(&self, name: &str, ctx: LsContext, input: Value) -> LsResult<Value> {
        let result = self.execute(name, ctx, input).await?;
        Ok(serde_json::to_value(result).unwrap_or_default())
    }

    /// 查询工作流状态.
    async fn workflow_status(&self, name: &str) -> LsResult<Value> {
        if !self.contains(name).await {
            return Err(lingshu_core::LsError::NotFound(format!("workflow '{name}' not found")));
        }
        // Use get() to return workflow info (clone-safe since handlers are reset)
        let dag = self.get(name).await
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("workflow '{name}' not found")))?;
        let info = dag.info();
        Ok(serde_json::json!({
            "name": info.name,
            "id": info.id.to_string(),
            "node_count": info.node_count,
            "edge_count": info.edge_count,
            "status": "registered",
        }))
    }
}
