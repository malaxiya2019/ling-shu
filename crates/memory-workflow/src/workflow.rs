//! MemoryWorkflow trait — Memory Engine 的统一入口。

use async_trait::async_trait;
use lingshu_core::LsResult;
use lingshu_evidence_graph::EvidenceGraph;
use serde::{Deserialize, Serialize};

/// MemoryQuery — 记忆查询的统一输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// 用户问题/查询文本
    pub question: String,
    /// 可选的会话 ID
    pub session_id: Option<String>,
    /// 可选的上下文
    pub context: Option<serde_json::Value>,
}

impl MemoryQuery {
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            session_id: None,
            context: None,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// MemoryWorkflow — 记忆工作流统一 trait。
///
/// 所有记忆检索方式（Timeline、EntitySearch、RAG 等）实现此接口。
/// Agent Runtime 只通过此接口访问记忆，不直接碰存储层。
#[async_trait]
pub trait MemoryWorkflow: Send + Sync {
    /// 工作流名称。
    fn name(&self) -> &str;

    /// 执行记忆查询，返回 EvidenceGraph。
    async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph>;
}

/// MemoryWorkflowRegistry — 工作流注册表。
///
/// 管理所有已注册的 MemoryWorkflow，支持按名称查找和执行。
pub struct MemoryWorkflowRegistry {
    workflows: Vec<Box<dyn MemoryWorkflow>>,
}

impl MemoryWorkflowRegistry {
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
        }
    }

    /// 注册一个 MemoryWorkflow。
    pub fn register(&mut self, workflow: Box<dyn MemoryWorkflow>) {
        self.workflows.push(workflow);
    }

    /// 按名称查找工作流。
    pub fn get(&self, name: &str) -> Option<&dyn MemoryWorkflow> {
        self.workflows.iter().find(|w| w.name() == name).map(|w| w.as_ref())
    }

    /// 列出所有已注册的工作流名称。
    pub fn list_names(&self) -> Vec<String> {
        self.workflows.iter().map(|w| w.name().to_string()).collect()
    }

    /// 在所有工作流中执行（按注册顺序，返回第一个有结果的）。
    pub async fn execute_first(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
        for workflow in &self.workflows {
            let result = workflow.execute(query.clone()).await?;
            if !result.nodes.is_empty() {
                return Ok(result);
            }
        }
        Ok(EvidenceGraph::empty(&query.question))
    }

    /// 在所有工作流中执行并合并结果。
    pub async fn execute_all(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
        let mut merged = EvidenceGraph::empty(&query.question);

        for workflow in &self.workflows {
            match workflow.execute(query.clone()).await {
                Ok(graph) => {
                    merged.merge(graph);
                }
                Err(e) => {
                    tracing::warn!(
                        workflow = workflow.name(),
                        error = %e,
                        "memory workflow failed, skipping"
                    );
                }
            }
        }

        Ok(merged)
    }
}

impl Default for MemoryWorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_evidence_graph::Node;

    struct TestWorkflow;

    #[async_trait]
    impl MemoryWorkflow for TestWorkflow {
        fn name(&self) -> &str {
            "test"
        }

        async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
            let mut graph = EvidenceGraph::empty(&query.question);
            graph.add_node(Node::fact("test result", format!("result for: {}", query.question)));
            Ok(graph)
        }
    }

    #[tokio::test]
    async fn test_registry() {
        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(TestWorkflow));

        assert_eq!(registry.list_names(), vec!["test"]);

        let result = registry
            .execute_first(MemoryQuery::new("测试"))
            .await
            .unwrap();
        assert!(!result.nodes.is_empty());
    }

    #[tokio::test]
    async fn test_execute_all_empty() {
        let registry = MemoryWorkflowRegistry::new();
        let result = registry
            .execute_all(MemoryQuery::new("test"))
            .await
            .unwrap();
        assert!(result.nodes.is_empty());
    }
}
