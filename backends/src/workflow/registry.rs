//! WorkflowRegistry — 工作流注册表.
//!
//! 管理多个 WorkflowDag 的注册、查找、删除和执行。
//! 线程安全，支持并发的注册和执行。

use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use super::dag::{WorkflowDag, WorkflowResult};
use super::checkpoint::{CheckpointManager, CheckpointStore, WorkflowCheckpointSummary};

/// 工作流注册表 — 管理多个工作流的生命周期.
pub struct WorkflowRegistry {
    workflows: Arc<RwLock<HashMap<String, WorkflowDag>>>,
    checkpoint_manager: Option<Arc<CheckpointManager>>,
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            checkpoint_manager: None,
        }
    }
}

impl WorkflowRegistry {
    /// 创建新的工作流注册表.
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 CheckpointManager.
    pub fn with_checkpoint(mut self, store: Arc<dyn CheckpointStore + 'static>) -> Self {
        self.checkpoint_manager = Some(Arc::new(CheckpointManager::new(store)));
        self
    }

    /// 注册工作流.
    pub async fn register(&self, name: impl Into<String>, workflow: WorkflowDag) {
        let name = name.into();
        let mut workflows = self.workflows.write().await;
        workflows.insert(name.clone(), workflow);
        info!(workflow = %name, "workflow registered");
    }

    /// 注销工作流.
    pub async fn unregister(&self, name: &str) -> LsResult<()> {
        let mut workflows = self.workflows.write().await;
        workflows.remove(name);
        info!(workflow = %name, "workflow unregistered");
        Ok(())
    }

    /// 获取工作流.
    pub async fn get(&self, name: &str) -> Option<WorkflowDag> {
        let workflows = self.workflows.read().await;
        // Return a clone (handlers will be reset)
        workflows.get(name).cloned()
    }

    /// 检查工作流是否存在.
    pub async fn contains(&self, name: &str) -> bool {
        let workflows = self.workflows.read().await;
        workflows.contains_key(name)
    }

    /// 列出所有注册的工作流.
    pub async fn list(&self) -> Vec<WorkflowRegistryEntry> {
        let workflows = self.workflows.read().await;
        workflows
            .iter()
            .map(|(name, dag)| {
                let info = dag.info();
                WorkflowRegistryEntry {
                    name: name.clone(),
                    id: info.id,
                    node_count: info.node_count,
                    edge_count: info.edge_count,
                }
            })
            .collect()
    }

    /// 执行工作流 (如果注册了).
    pub async fn execute(
        &self,
        name: &str,
        ctx: LsContext,
        input: Value,
    ) -> LsResult<WorkflowResult> {
        let workflows = self.workflows.read().await;
        let dag = workflows
            .get(name)
            .ok_or_else(|| {
                lingshu_core::LsError::NotFound(format!("workflow '{name}' not found"))
            })?;

        // If checkpoint manager is configured, use execute_or_resume
        if let Some(ref cm) = self.checkpoint_manager {
            return cm.execute_or_resume(dag, ctx, input).await;
        }

        dag.execute(ctx, input).await
    }

    /// 执行工作流并创建检查点.
    pub async fn execute_with_checkpoint(
        &self,
        name: &str,
        ctx: LsContext,
        input: Value,
    ) -> LsResult<WorkflowResult> {
        let workflows = self.workflows.read().await;
        let dag = workflows
            .get(name)
            .ok_or_else(|| {
                lingshu_core::LsError::NotFound(format!("workflow '{name}' not found"))
            })?;

        match self.checkpoint_manager {
            Some(ref cm) => cm.execute_or_resume(dag, ctx, input).await,
            None => dag.execute(ctx, input).await,
        }
    }

    /// 获取工作流数量.
    pub async fn len(&self) -> usize {
        let workflows = self.workflows.read().await;
        workflows.len()
    }

    /// 注册表是否为空.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// 获取检查点摘要列表.
    pub async fn list_checkpoints(&self) -> LsResult<Vec<WorkflowCheckpointSummary>> {
        match self.checkpoint_manager {
            Some(ref cm) => cm.list_checkpoints().await,
            None => Ok(Vec::new()),
        }
    }

    /// 删除检查点.
    pub async fn delete_checkpoint(&self, workflow_id: &LsId) -> LsResult<()> {
        match self.checkpoint_manager {
            Some(ref cm) => cm.delete_checkpoint(workflow_id).await,
            None => Ok(()),
        }
    }
}

/// 工作流注册条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRegistryEntry {
    pub name: String,
    pub id: LsId,
    pub node_count: usize,
    pub edge_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;
    use std::sync::Arc;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    fn make_hello_handler() -> crate::workflow::dag::NodeHandler {
        Arc::new(|_ctx: LsContext, _input: Value| {
            Box::pin(async { Ok(json!({"msg": "hello"})) })
        })
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let registry = WorkflowRegistry::new();
        let mut dag = WorkflowDag::new("hello_wf");
        dag.add_node("hello", "", Value::Null, make_hello_handler());

        registry.register("hello_wf", dag).await;
        assert_eq!(registry.len().await, 1);

        let entries = registry.list().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello_wf");
        assert_eq!(entries[0].node_count, 1);
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = WorkflowRegistry::new();
        let mut dag = WorkflowDag::new("test_wf");
        dag.add_node("node_a", "", Value::Null, make_hello_handler());

        registry.register("test_wf", dag).await;

        let loaded = registry.get("test_wf").await;
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name(), "test_wf");
    }

    #[tokio::test]
    async fn test_execute_registered_workflow() {
        let registry = WorkflowRegistry::new();
        let mut dag = WorkflowDag::new("exec-test");
        dag.add_node("hello", "", Value::Null, make_hello_handler());

        registry.register("exec-test", dag).await;

        let result = registry
            .execute("exec-test", test_ctx(), Value::Null)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_not_found() {
        let registry = WorkflowRegistry::new();
        let result = registry
            .execute("nonexistent", test_ctx(), Value::Null)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = WorkflowRegistry::new();
        let dag = WorkflowDag::new("temp_wf");
        registry.register("temp_wf", dag).await;
        assert_eq!(registry.len().await, 1);

        registry.unregister("temp_wf").await.unwrap();
        assert_eq!(registry.len().await, 0);
    }

    #[tokio::test]
    async fn test_double_register_overwrites() {
        let registry = WorkflowRegistry::new();
        let dag1 = WorkflowDag::new("same_name");
        registry.register("dup", dag1).await;

        let mut dag2 = WorkflowDag::new("same_name");
        dag2.add_node("node_a", "", Value::Null, make_hello_handler());
        registry.register("dup", dag2).await;

        let loaded = registry.get("dup").await.unwrap();
        assert_eq!(loaded.node_count(), 1);
    }

    #[tokio::test]
    async fn test_contains() {
        let registry = WorkflowRegistry::new();
        let dag = WorkflowDag::new("exists");
        registry.register("exists", dag).await;

        assert!(registry.contains("exists").await);
        assert!(!registry.contains("missing").await);
    }

    #[tokio::test]
    async fn test_is_empty() {
        let registry = WorkflowRegistry::new();
        assert!(registry.is_empty().await);

        let dag = WorkflowDag::new("wf");
        registry.register("wf", dag).await;
        assert!(!registry.is_empty().await);
    }
}
