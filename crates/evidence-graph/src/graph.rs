//! EvidenceGraph 主结构。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Edge, EdgeKind, Node, NodeId, NodeKind};

/// EvidenceGraph — 记忆输出的统一表示。
///
/// 所有记忆查询的最终输出格式。
///
/// # 设计原则
///
/// - 唯一输出接口：所有 Memory Workflow 返回 EvidenceGraph
/// - 自包含：包含所有节点、边和元数据
/// - 可追溯：每个节点和边都有 source_ref
/// - 可评估：包含置信度和冲突信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceGraph {
    /// 图中的所有节点
    pub nodes: Vec<Node>,
    /// 图中的所有边
    pub edges: Vec<Edge>,
    /// 查询元数据
    pub metadata: GraphMetadata,
}

/// 图元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetadata {
    /// 查询文本
    pub query: String,
    /// 图创建时间
    pub created_at: DateTime<Utc>,
    /// 节点数
    pub node_count: usize,
    /// 边数
    pub edge_count: usize,
    /// 涉及的实体列表
    pub entities: Vec<String>,
    /// 时间跨度起始
    pub time_span_start: Option<DateTime<Utc>>,
    /// 时间跨度结束
    pub time_span_end: Option<DateTime<Utc>>,
    /// 构建耗时（毫秒）
    pub build_time_ms: u64,
    /// 源信息
    pub source: String,
    /// 扩展属性
    pub attributes: HashMap<String, String>,
}

impl EvidenceGraph {
    /// 创建一个空的 EvidenceGraph。
    pub fn empty(query: impl Into<String>) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            metadata: GraphMetadata {
                query: query.into(),
                created_at: Utc::now(),
                node_count: 0,
                edge_count: 0,
                entities: Vec::new(),
                time_span_start: None,
                time_span_end: None,
                build_time_ms: 0,
                source: "empty".into(),
                attributes: HashMap::new(),
            },
        }
    }

    /// 添加一个节点。
    pub fn add_node(&mut self, node: Node) {
        self.metadata.node_count = self.nodes.len() + 1;
        self.nodes.push(node);
    }

    /// 添加一条边。
    pub fn add_edge(&mut self, edge: Edge) {
        self.metadata.edge_count = self.edges.len() + 1;
        self.edges.push(edge);
    }

    /// 获取指定类型的所有节点。
    pub fn nodes_by_kind(&self, kind: NodeKind) -> Vec<&Node> {
        self.nodes.iter().filter(|n| n.kind == kind).collect()
    }

    /// 获取与指定节点相连的所有边。
    pub fn edges_for_node(&self, node_id: NodeId) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| e.source_id == node_id || e.target_id == node_id)
            .collect()
    }

    /// 获取指定类型的所有边。
    pub fn edges_by_kind(&self, kind: EdgeKind) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.kind == kind).collect()
    }

    /// 按时间排序的事件节点。
    pub fn events_sorted(&self) -> Vec<&Node> {
        let mut events: Vec<&Node> = self
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Event && n.timestamp.is_some())
            .collect();
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        events
    }

    /// 合并另一个 EvidenceGraph 到当前图。
    pub fn merge(&mut self, other: EvidenceGraph) {
        // 合并节点（去重）
        let existing_ids: std::collections::HashSet<NodeId> =
            self.nodes.iter().map(|n| n.id).collect();
        for node in other.nodes {
            if !existing_ids.contains(&node.id) {
                self.nodes.push(node);
            }
        }

        // 合并边（去重）
        let existing_edge_ids: std::collections::HashSet<_> =
            self.edges.iter().map(|e| e.id).collect();
        for edge in other.edges {
            if !existing_edge_ids.contains(&edge.id) {
                self.edges.push(edge);
            }
        }

        // 更新元数据
        self.metadata.node_count = self.nodes.len();
        self.metadata.edge_count = self.edges.len();
    }

    /// 转化为 JSON 字符串。
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 从 JSON 字符串解析。
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;

    #[test]
    fn test_empty_graph() {
        let g = EvidenceGraph::empty("test query");
        assert_eq!(g.nodes.len(), 0);
        assert_eq!(g.edges.len(), 0);
        assert_eq!(g.metadata.query, "test query");
    }

    #[test]
    fn test_add_node_and_edge() {
        let mut g = EvidenceGraph::empty("test");
        let n1 = Node::event("事件1", "第一个事件", Utc::now());
        let n2 = Node::event("事件2", "第二个事件", Utc::now());
        let n1_id = n1.id;
        let n2_id = n2.id;

        g.add_node(n1);
        g.add_node(n2);
        g.add_edge(Edge::temporal(n1_id, n2_id));

        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.metadata.node_count, 2);
        assert_eq!(g.metadata.edge_count, 1);
    }

    #[test]
    fn test_events_sorted() {
        use chrono::Duration;

        let mut g = EvidenceGraph::empty("test");
        let n1 = Node::event("旧事件", "较旧", Utc::now() - Duration::days(10));
        let n2 = Node::event("新事件", "较新", Utc::now());

        let n1_id = n1.id;
        let n2_id = n2.id;

        g.add_node(n2);
        g.add_node(n1);
        g.add_edge(Edge::temporal(n1_id, n2_id));

        let sorted = g.events_sorted();
        assert_eq!(sorted[0].title, "旧事件");
        assert_eq!(sorted[1].title, "新事件");
    }

    #[test]
    fn test_merge() {
        let mut g1 = EvidenceGraph::empty("q1");
        let n1 = Node::event("A", "A事件", Utc::now());
        g1.add_node(n1.clone());

        let mut g2 = EvidenceGraph::empty("q2");
        let n2 = Node::event("B", "B事件", Utc::now());
        g2.add_node(n2);

        g1.merge(g2);
        assert_eq!(g1.nodes.len(), 2);
    }

    #[test]
    fn test_json_serialization() {
        let mut g = EvidenceGraph::empty("test");
        let n1 = Node::event("事件1", "描述1", Utc::now());
        let n2 = Node::event("事件2", "描述2", Utc::now());
        let n1_id = n1.id;
        let n2_id = n2.id;
        g.add_node(n1);
        g.add_node(n2);
        g.add_edge(Edge::temporal(n1_id, n2_id));

        let json = g.to_json_pretty().unwrap();
        let parsed = EvidenceGraph::from_json(&json).unwrap();
        assert_eq!(parsed.nodes.len(), 2);
        assert_eq!(parsed.edges.len(), 1);
    }
}
