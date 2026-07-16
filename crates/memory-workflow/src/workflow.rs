//! MemoryWorkflow trait — Memory Engine 的统一入口。

use async_trait::async_trait;
use lingshu_core::LsResult;
use lingshu_evidence_graph::{
    EvidenceGraph, WeightedGraphMerger, WeightedMergeResult, WeightedWorkflowOutput,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    /// 从注册表获取所有权。
    pub fn into_workflows(self) -> Vec<Box<dyn MemoryWorkflow>> {
        self.workflows
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

    /// 在所有工作流中执行并合并结果（简单 ID 去重合并）。
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

    /// 加权执行：并行执行所有 workflow，用 WeightedGraphMerger 合并。
    ///
    /// # 参数
    ///
    /// * `query` - 记忆查询
    /// * `weights` - workflow 名称 → 权重映射（来自 ProbabilisticRouter）
    ///
    /// # 返回
    ///
    /// WeightedMergeResult 包含合并后的图、统计信息和冲突列表。
    pub async fn execute_weighted(
        &self,
        query: MemoryQuery,
        weights: HashMap<String, f64>,
    ) -> LsResult<WeightedMergeResult> {
        if self.workflows.is_empty() || weights.is_empty() {
            return Ok(WeightedMergeResult {
                graph: EvidenceGraph::empty(&query.question),
                merge_stats: Default::default(),
                conflicts: Vec::new(),
                source_map: HashMap::new(),
            });
        }

        // 并行执行所有 workflow，收集有结果的
        let mut futures = Vec::new();
        for workflow in &self.workflows {
            let wf_name = workflow.name().to_string();
            if !weights.contains_key(&wf_name) {
                continue;
            }
            let q = query.clone();
            futures.push(async move {
                let result = workflow.execute(q).await;
                (wf_name, result)
            });
        }

        let results: Vec<(String, LsResult<EvidenceGraph>)> =
            futures::future::join_all(futures).await;

        // 构建 WeightedWorkflowOutput 列表
        let mut outputs = Vec::new();
        for (wf_name, result) in results {
            let weight = *weights.get(&wf_name).unwrap_or(&0.0);
            match result {
                Ok(graph) => {
                    if !graph.nodes.is_empty() {
                        outputs.push(WeightedWorkflowOutput {
                            workflow_name: wf_name,
                            weight,
                            graph,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        workflow = wf_name,
                        error = %e,
                        "weighted workflow execution failed, skipping"
                    );
                }
            }
        }

        // 使用 WeightedGraphMerger 合并
        let merger = WeightedGraphMerger::new();
        let mut result = merger.merge(outputs);

        // 设置查询
        result.graph.metadata.query = query.question.clone();

        Ok(result)
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

    struct TimelineWorkflow;

    #[async_trait]
    impl MemoryWorkflow for TimelineWorkflow {
        fn name(&self) -> &str {
            "timeline"
        }

        async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
            let mut graph = EvidenceGraph::empty(&query.question);
            graph.add_node(Node::event("事件A", "项目启动", chrono::Utc::now()));
            graph.add_node(Node::event("事件B", "项目暂停", chrono::Utc::now()));
            Ok(graph)
        }
    }

    struct SemanticWorkflow;

    #[async_trait]
    impl MemoryWorkflow for SemanticWorkflow {
        fn name(&self) -> &str {
            "semantic"
        }

        async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
            let mut graph = EvidenceGraph::empty(&query.question);
            graph.add_node(Node::fact("语义结果", format!("关于: {}", query.question)));
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

    #[tokio::test]
    async fn test_execute_weighted_empty() {
        let registry = MemoryWorkflowRegistry::new();
        let result = registry
            .execute_weighted(MemoryQuery::new("test"), HashMap::new())
            .await
            .unwrap();
        assert!(result.graph.nodes.is_empty());
        assert_eq!(result.merge_stats.total_workflows, 0);
    }

    #[tokio::test]
    async fn test_execute_weighted_single() {
        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(TestWorkflow));

        let mut weights = HashMap::new();
        weights.insert("test".to_string(), 1.0);

        let result = registry
            .execute_weighted(MemoryQuery::new("测试"), weights)
            .await
            .unwrap();

        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.merge_stats.nodes_added, 1);
    }

    #[tokio::test]
    async fn test_execute_weighted_multi() {
        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(TimelineWorkflow));
        registry.register(Box::new(SemanticWorkflow));

        let mut weights = HashMap::new();
        weights.insert("timeline".to_string(), 0.7);
        weights.insert("semantic".to_string(), 0.3);

        let result = registry
            .execute_weighted(MemoryQuery::new("项目A"), weights)
            .await
            .unwrap();

        // timeline has 2 nodes, semantic has 1
        assert_eq!(result.graph.nodes.len(), 3);
        assert_eq!(result.merge_stats.nodes_added, 3);
        assert_eq!(result.merge_stats.total_workflows, 2);
    }

    #[tokio::test]
    async fn test_execute_weighted_with_missing_workflow() {
        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(TimelineWorkflow));

        let mut weights = HashMap::new();
        weights.insert("timeline".to_string(), 0.7);
        weights.insert("nonexistent".to_string(), 0.3); // should be ignored

        let result = registry
            .execute_weighted(MemoryQuery::new("项目A"), weights)
            .await
            .unwrap();

        assert_eq!(result.graph.nodes.len(), 2);
        assert_eq!(result.merge_stats.total_workflows, 1);
    }

    #[tokio::test]
    async fn test_into_workflows() {
        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(TestWorkflow));
        registry.register(Box::new(TimelineWorkflow));

        let workflows = registry.into_workflows();
        assert_eq!(workflows.len(), 2);
    }
}
