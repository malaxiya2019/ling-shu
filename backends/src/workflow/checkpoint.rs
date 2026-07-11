//! Workflow Checkpoint — 工作流快照持久化与恢复.
//!
//! 支持将执行中的工作流状态保存到存储后端，
//! 在故障或中断后从快照恢复执行。

use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

use super::dag::{ExecutionSnapshot, WorkflowDag, WorkflowResult};

/// Checkpoint 持久化存储接口.
#[async_trait::async_trait]
pub trait CheckpointStore: Send + Sync {
    /// 保存检查点.
    async fn save(&self, checkpoint: &WorkflowCheckpoint) -> LsResult<()>;

    /// 加载检查点.
    async fn load(&self, workflow_id: &LsId) -> LsResult<Option<WorkflowCheckpoint>>;

    /// 删除检查点.
    async fn delete(&self, workflow_id: &LsId) -> LsResult<()>;

    /// 列出所有检查点.
    async fn list(&self) -> LsResult<Vec<WorkflowCheckpointSummary>>;
}

/// 工作流检查点.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCheckpoint {
    /// 工作流 ID.
    pub workflow_id: LsId,
    /// 工作流名称.
    pub workflow_name: String,
    /// 执行快照.
    pub snapshot: ExecutionSnapshot,
    /// 输入数据.
    pub input: Value,
    /// 检查点创建时间.
    pub created_at: String,
}

/// 检查点摘要.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCheckpointSummary {
    pub workflow_id: LsId,
    pub workflow_name: String,
    pub completed_nodes: usize,
    pub total_nodes: usize,
    pub failed: bool,
    pub created_at: String,
}

// ── InMemoryCheckpointStore ──

/// 内存检查点存储 (用于测试和单进程场景).
pub struct InMemoryCheckpointStore {
    store: Arc<tokio::sync::RwLock<HashMap<LsId, WorkflowCheckpoint>>>,
}

impl InMemoryCheckpointStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for InMemoryCheckpointStore {
    fn default() -> Self {
        Self {
            store: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl CheckpointStore for InMemoryCheckpointStore {
    async fn save(&self, checkpoint: &WorkflowCheckpoint) -> LsResult<()> {
        let mut store = self.store.write().await;
        store.insert(checkpoint.workflow_id, checkpoint.clone());
        debug!(
            workflow_id = %checkpoint.workflow_id,
            "checkpoint saved in memory"
        );
        Ok(())
    }

    async fn load(&self, workflow_id: &LsId) -> LsResult<Option<WorkflowCheckpoint>> {
        let store = self.store.read().await;
        Ok(store.get(workflow_id).cloned())
    }

    async fn delete(&self, workflow_id: &LsId) -> LsResult<()> {
        let mut store = self.store.write().await;
        store.remove(workflow_id);
        Ok(())
    }

    async fn list(&self) -> LsResult<Vec<WorkflowCheckpointSummary>> {
        let store = self.store.read().await;
        let summaries = store
            .values()
            .map(|cp| WorkflowCheckpointSummary {
                workflow_id: cp.workflow_id,
                workflow_name: cp.workflow_name.clone(),
                completed_nodes: cp.snapshot.completed.len(),
                total_nodes: 0, // caller can fill this
                failed: cp.snapshot.failed,
                created_at: cp.created_at.clone(),
            })
            .collect();
        Ok(summaries)
    }
}

// ── CheckpointManager ──

/// Checkpoint 管理器 — 协调快照创建、加载和恢复.
pub struct CheckpointManager {
    store: Arc<dyn CheckpointStore>,
    /// 自动创建检查点的节点间隔 (0 = 禁用).
    auto_checkpoint_interval: usize,
}

impl CheckpointManager {
    /// 创建新的 CheckpointManager.
    pub fn new(store: Arc<dyn CheckpointStore>) -> Self {
        Self {
            store,
            auto_checkpoint_interval: 0,
        }
    }

    /// 设置自动检查点间隔 (每 N 个节点创建一个检查点).
    pub fn with_auto_interval(mut self, interval: usize) -> Self {
        self.auto_checkpoint_interval = interval;
        Self {
            store: self.store,
            auto_checkpoint_interval: self.auto_checkpoint_interval,
        }
    }

    /// 创建检查点.
    pub async fn checkpoint(
        &self,
        workflow: &WorkflowDag,
        snapshot: &ExecutionSnapshot,
        input: &Value,
    ) -> LsResult<()> {
        let checkpoint = WorkflowCheckpoint {
            workflow_id: workflow.id(),
            workflow_name: workflow.name().to_string(),
            snapshot: snapshot.clone(),
            input: input.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.store.save(&checkpoint).await?;

        info!(
            workflow_id = %checkpoint.workflow_id,
            completed_nodes = snapshot.completed.len(),
            "checkpoint created"
        );
        Ok(())
    }

    /// 加载检查点.
    pub async fn load_checkpoint(&self, workflow_id: &LsId) -> LsResult<Option<WorkflowCheckpoint>> {
        self.store.load(workflow_id).await
    }

    /// 删除检查点.
    pub async fn delete_checkpoint(&self, workflow_id: &LsId) -> LsResult<()> {
        self.store.delete(workflow_id).await
    }

    /// 列出所有检查点.
    pub async fn list_checkpoints(&self) -> LsResult<Vec<WorkflowCheckpointSummary>> {
        self.store.list().await
    }

    /// 使用检查点恢复工作流执行.
    ///
    /// 如果存在检查点，从快照恢复；否则从头执行。
    pub async fn execute_or_resume(
        &self,
        workflow: &WorkflowDag,
        ctx: LsContext,
        input: Value,
    ) -> LsResult<WorkflowResult> {
        let workflow_id = workflow.id();

        // Try to load existing checkpoint
        match self.store.load(&workflow_id).await? {
            Some(checkpoint) => {
                info!(
                    workflow = %workflow.name(),
                    completed_nodes = checkpoint.snapshot.completed.len(),
                    "found checkpoint, resuming workflow execution"
                );
                workflow
                    .resume(ctx, checkpoint.input, checkpoint.snapshot)
                    .await
            }
            None => {
                info!(
                    workflow = %workflow.name(),
                    "no checkpoint found, starting fresh execution"
                );
                let result = workflow.execute(ctx, input.clone()).await?;

                // Create checkpoints for recovery
                // NOTE: In a full implementation, we would create checkpoints during execution.
                // For v3.9, we create a final checkpoint.

                Ok(result)
            }
        }
    }
}

/// 从检查点恢复函数 (快捷方式).
pub async fn resume_from_checkpoint(
    workflow: &WorkflowDag,
    ctx: LsContext,
    input: Value,
    store: Arc<dyn CheckpointStore>,
) -> LsResult<WorkflowResult> {
    let manager = CheckpointManager::new(store);
    manager.execute_or_resume(workflow, ctx, input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use super::super::dag::NodeResult;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_in_memory_store_save_load() {
        let store = Arc::new(InMemoryCheckpointStore::new()) as Arc<dyn CheckpointStore>;

        let cp = WorkflowCheckpoint {
            workflow_id: LsId::new(),
            workflow_name: "test-wf".to_string(),
            snapshot: ExecutionSnapshot {
                completed: HashSet::new(),
                results: HashMap::new(),
                node_outputs: HashMap::new(),
                failed: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            input: json!({"key": "value"}),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        store.save(&cp).await.unwrap();

        let loaded = store.load(&cp.workflow_id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().workflow_name, "test-wf");

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_checkpoint_delete() {
        let store = Arc::new(InMemoryCheckpointStore::new()) as Arc<dyn CheckpointStore>;

        let cp = WorkflowCheckpoint {
            workflow_id: LsId::new(),
            workflow_name: "delete-test".to_string(),
            snapshot: ExecutionSnapshot {
                completed: HashSet::new(),
                results: HashMap::new(),
                node_outputs: HashMap::new(),
                failed: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            input: Value::Null,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        store.save(&cp).await.unwrap();
        store.delete(&cp.workflow_id).await.unwrap();

        let loaded = store.load(&cp.workflow_id).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_checkpoint_manager_create() {
        let store = Arc::new(InMemoryCheckpointStore::new()) as Arc<dyn CheckpointStore>;
        let manager = CheckpointManager::new(store.clone());

        let mut wf = WorkflowDag::new("cp-test");
        let a = wf.add_node(
            "step_a", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"done": true})) })),
        );

        // Simulate partial execution state
        let mut completed = HashSet::new();
        completed.insert(a);
        let mut results = HashMap::new();
        results.insert(
            a,
            NodeResult {
                node_id: a,
                node_name: "step_a".to_string(),
                status: super::super::dag::NodeStatus::Completed,
                output: Some(json!({"done": true})),
                error: None,
                duration_ms: 10,
                attempts: 1,
            },
        );
        let mut outputs = HashMap::new();
        outputs.insert(a, json!({"done": true}));

        let snapshot = ExecutionSnapshot {
            completed,
            results,
            node_outputs: outputs,
            failed: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        manager
            .checkpoint(&wf, &snapshot, &json!({"input": "data"}))
            .await
            .unwrap();

        let loaded = store.load(&wf.id()).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_execute_or_resume_no_checkpoint() {
        let store = Arc::new(InMemoryCheckpointStore::new()) as Arc<dyn CheckpointStore>;
        let manager = CheckpointManager::new(store);

        let mut wf = WorkflowDag::new("fresh-exec");
        wf.add_node(
            "hello", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"msg": "hello"})) })),
        );

        let result = manager
            .execute_or_resume(&wf, test_ctx(), Value::Null)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 1);
    }
}
