//! SemanticWorkflow — 将 SemanticIndex 适配为 MemoryWorkflow

use async_trait::async_trait;
use chrono::Utc;
use lingshu_core::LsResult;
use lingshu_evidence_graph::{NodeId, Edge, EvidenceGraph, Node};
use lingshu_memory_workflow::{MemoryQuery, MemoryWorkflow};
use std::sync::Arc;

use crate::index::{SemanticIndex, TfIdfIndex};

/// SemanticWorkflow — 语义搜索工作流。
///
/// 将 SemanticIndex 的搜索结果包装为 EvidenceGraph，
/// 使其可以通过统一的 MemoryWorkflowRegistry 调用。
pub struct SemanticWorkflow {
    index: Arc<dyn SemanticIndex>,
    /// 搜索返回的最大结果数
    max_results: usize,
    /// 搜索结果相关度阈值
    score_threshold: f64,
}

impl SemanticWorkflow {
    pub fn new(index: Arc<dyn SemanticIndex>) -> Self {
        Self {
            index,
            max_results: 10,
            score_threshold: 0.05,
        }
    }

    /// 设置最大结果数。
    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    /// 设置相关度阈值。
    pub fn with_score_threshold(mut self, threshold: f64) -> Self {
        self.score_threshold = threshold;
        self
    }
}

#[async_trait]
impl MemoryWorkflow for SemanticWorkflow {
    fn name(&self) -> &str {
        "semantic"
    }

    async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
        let start = std::time::Instant::now();

        let results = self.index
            .search(&query.question, self.max_results)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        // 过滤低分结果
        let filtered: Vec<_> = results.into_iter()
            .filter(|r| r.score >= self.score_threshold)
            .collect();

        let query_time_ms = start.elapsed().as_millis() as u64;

        if filtered.is_empty() {
            let mut empty = EvidenceGraph::empty(&query.question);
            empty.metadata.build_time_ms = query_time_ms;
            empty.metadata.source = "semantic".into();
            return Ok(empty);
        }

        let mut graph = EvidenceGraph::empty(
            &query.question,
        );
        graph.metadata.source = "semantic".into();

        graph.metadata.build_time_ms = query_time_ms;
        let mut prev_node_id: Option<NodeId> = None;
        let mut all_entities = Vec::new();
        let mut time_span_start: Option<chrono::DateTime<Utc>> = None;
        let mut time_span_end: Option<chrono::DateTime<Utc>> = None;

        for result in &filtered {
            let mut node = Node::event(
                &result.episode.title,
                &format!("[相关度: {:.2}] {}", result.score, result.episode.summary),
                result.episode.timestamp,
            );

            // 添加命中的词项作为标签
            for term in &result.matched_terms {
                node = node.with_tag(term);
            }
            for entity in &result.episode.entities {
                node = node.with_tag(&format!("{}:{}", entity.kind, entity.name));
                all_entities.push(format!("{}:{}", entity.kind, entity.name));
            }
            for tag in &result.episode.tags {
                node = node.with_tag(tag);
            }

            let node_id = node.id;

            // 添加时间顺序边
            if let Some(prev_id) = prev_node_id {
                graph.add_edge(Edge::temporal(prev_id, node_id));
            }

            graph.add_node(node);
            prev_node_id = Some(node_id);

            // 更新时间范围
            if time_span_start.is_none() || result.episode.timestamp < time_span_start.unwrap() {
                time_span_start = Some(result.episode.timestamp);
            }
            if time_span_end.is_none() || result.episode.timestamp > time_span_end.unwrap() {
                time_span_end = Some(result.episode.timestamp);
            }
        }

        graph.metadata.node_count = graph.nodes.len();
        graph.metadata.edge_count = graph.edges.len();
        graph.metadata.entities = all_entities;
        graph.metadata.time_span_start = time_span_start;
        graph.metadata.time_span_end = time_span_end;

        Ok(graph)
    }
}

/// 创建带有 TfIdfIndex 的 SemanticWorkflow（便捷函数）。
pub fn create_semantic_workflow() -> (Arc<TfIdfIndex>, SemanticWorkflow) {
    let index = Arc::new(TfIdfIndex::new());
    let workflow = SemanticWorkflow::new(index.clone());
    (index, workflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemanticIndex;
    use lingshu_memory_episode::Episode;

    #[tokio::test]
    async fn test_semantic_workflow_empty() {
        let (_, workflow) = create_semantic_workflow();
        let query = MemoryQuery::new("test");
        let result = workflow.execute(query).await.unwrap();
        assert_eq!(result.nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_semantic_workflow_with_data() {
        let (index, workflow) = create_semantic_workflow();

        index.index_episode(&Episode::new("RAG技术", "RAG的原理和应用", Utc::now())).await.unwrap();
        index.index_episode(&Episode::new("项目A启动", "项目A正式启动", Utc::now())).await.unwrap();

        let query = MemoryQuery::new("RAG");
        let result = workflow.execute(query).await.unwrap();
        assert!(!result.nodes.is_empty(), "should find RAG episode");
        assert_eq!(result.metadata.source, "semantic");

        // 验证节点内容
        let node = &result.nodes[0];
        assert!(node.title.contains("RAG"), "top result should be RAG-related");
    }

    #[tokio::test]
    async fn test_semantic_workflow_score_threshold() {
        let index = Arc::new(TfIdfIndex::new());
        let workflow = SemanticWorkflow::new(index.clone())
            .with_score_threshold(0.9); // 极高阈值，几乎不会命中

        index.index_episode(&Episode::new("测试", "测试内容", Utc::now())).await.unwrap();

        let result = workflow.execute(MemoryQuery::new("不相关的内容")).await.unwrap();
        assert_eq!(result.nodes.len(), 0, "should be filtered by threshold");
    }

    #[tokio::test]
    async fn test_create_semantic_workflow() {
        let (index, workflow) = create_semantic_workflow();
        assert_eq!(workflow.name(), "semantic");
        assert_eq!(index.name(), "tfidf");
    }
}
