//! TimelineWorkflowAdapter — 将 TimelineWorkflow 适配为 MemoryWorkflow trait。

use async_trait::async_trait;
use lingshu_core::LsResult;
use lingshu_evidence_graph::{EvidenceGraph, EvidenceGraphBuilder};
use lingshu_workflow_memory::TimelineWorkflow;

use crate::{MemoryQuery, MemoryWorkflow};

/// TimelineWorkflowAdapter — TimelineWorkflow 的 MemoryWorkflow 适配器。
///
/// 将 TimelineWorkflow（搜索 Episode → 排序 → 构建时间线）包装为
/// MemoryWorkflow trait，使其可以通过统一的 MemoryWorkflowRegistry 调用。
pub struct TimelineWorkflowAdapter {
    inner: TimelineWorkflow,
}

impl TimelineWorkflowAdapter {
    pub fn new(inner: TimelineWorkflow) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl MemoryWorkflow for TimelineWorkflowAdapter {
    fn name(&self) -> &str {
        "timeline"
    }

    async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
        let start = std::time::Instant::now();

        // 调用 TimelineWorkflow
        let result = self.inner.execute(&query.question).await?;

        let _build_time_ms = start.elapsed().as_millis() as u64;

        // 将 TimelineWorkflow 结果转换为 EvidenceGraph
        let _graph = EvidenceGraphBuilder::from_episodes(
            // 由于 TimelineWorkflow 内部已经搜索了 Episode，但我们无法直接访问，
            // 这里我们从 Timeline 结果反推重建
            &[], // 在 TimelineWorkflow 中已处理
            &query.question,
            "timeline_workflow",
            result.execution_time_ms,
        );

        // 从 Timeline 结果构建节点
        let mut eg = EvidenceGraph::empty(&query.question);
        eg.metadata.build_time_ms = result.execution_time_ms;
        eg.metadata.source = "timeline_workflow".into();

        // 将 Timeline 中的节点转换成 EvidenceGraph
        let mut prev_node_id = None;
        for tl_node in &result.timeline.nodes {
            let mut enode = lingshu_evidence_graph::Node::event(
                &tl_node.title,
                &tl_node.summary,
                tl_node.timestamp,
            );
            for entity in &tl_node.entities {
                enode = enode.with_tag(&entity.name);
            }
            for tag in &tl_node.tags {
                enode = enode.with_tag(tag);
            }

            let node_id = enode.id;

            // 添加时间顺序边
            if let Some(prev_id) = prev_node_id {
                eg.add_edge(lingshu_evidence_graph::Edge::temporal(prev_id, node_id));
            }

            eg.add_node(enode);
            prev_node_id = Some(node_id);
        }

        // 设置元数据
        eg.metadata.node_count = eg.nodes.len();
        eg.metadata.edge_count = eg.edges.len();
        eg.metadata.time_span_start = result.timeline.span_start;
        eg.metadata.time_span_end = result.timeline.span_end;
        eg.metadata.entities = result
            .timeline
            .involved_entities
            .iter()
            .map(|e| format!("{}:{}", e.kind, e.name))
            .collect();
        eg.metadata.build_time_ms = result.execution_time_ms;

        Ok(eg)
    }
}
