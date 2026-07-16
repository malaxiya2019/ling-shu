//! EvidenceGraph 格式化输出。
//!
//! 提供多种输出格式：
//! - JSON（标准序列化）
//! - 文本摘要（人类可读）
//! - Debug（详细调试）

use crate::EvidenceGraph;

/// EvidenceGraph 格式化输出。
pub struct EvidenceGraphSerializer;

impl EvidenceGraphSerializer {
    /// 输出为格式化的 JSON 字符串。
    pub fn to_json(graph: &EvidenceGraph) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(graph)
    }

    /// 输出为紧凑 JSON 字符串。
    pub fn to_json_compact(graph: &EvidenceGraph) -> Result<String, serde_json::Error> {
        serde_json::to_string(graph)
    }

    /// 输出为人类可读的文本摘要。
    pub fn to_text_summary(graph: &EvidenceGraph, max_events: usize) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "证据图摘要: {} ({} 节点, {} 边)\n",
            graph.metadata.query, graph.metadata.node_count, graph.metadata.edge_count,
        ));

        if let Some(start) = graph.metadata.time_span_start {
            if let Some(end) = graph.metadata.time_span_end {
                output.push_str(&format!(
                    "时间跨度: {} ~ {}\n",
                    start.format("%Y-%m-%d"),
                    end.format("%Y-%m-%d"),
                ));
            }
        }

        if !graph.metadata.entities.is_empty() {
            output.push_str(&format!(
                "涉及实体: {}\n",
                graph.metadata.entities.join(", ")
            ));
        }

        output.push_str(&format!("数据来源: {}\n", graph.metadata.source));
        output.push_str(&format!("构建耗时: {}ms\n", graph.metadata.build_time_ms));

        // 时间线
        let events = graph.events_sorted();
        if !events.is_empty() {
            output.push_str("\n时间线:\n");
            for (i, event) in events.iter().take(max_events).enumerate() {
                if let Some(ts) = event.timestamp {
                    output.push_str(&format!(
                        "  [{}.] {} — {}\n",
                        i + 1,
                        ts.format("%Y-%m-%d %H:%M"),
                        event.title,
                    ));
                } else {
                    output.push_str(&format!("  [{}.] {}\n", i + 1, event.title));
                }
                if !event.description.is_empty() {
                    output.push_str(&format!("        {}\n", event.description));
                }
            }
            if events.len() > max_events {
                output.push_str(&format!(
                    "  ... 还有 {} 个事件\n",
                    events.len() - max_events
                ));
            }
        }

        output
    }

    /// 输出为调试用的详细文本。
    pub fn to_debug(graph: &EvidenceGraph) -> String {
        let mut output = String::new();

        output.push_str("=== EvidenceGraph Debug ===\n");
        output.push_str(&format!("Query: {}\n", graph.metadata.query));
        output.push_str(&format!("Nodes: {}\n", graph.nodes.len()));
        output.push_str(&format!("Edges: {}\n", graph.edges.len()));

        output.push_str("\n--- Nodes ---\n");
        for node in &graph.nodes {
            output.push_str(&format!(
                "  [{:?}] {} (confidence: {})\n",
                node.kind, node.title, node.confidence,
            ));
            if let Some(ts) = node.timestamp {
                output.push_str(&format!("         at {}\n", ts.format("%Y-%m-%d %H:%M:%S")));
            }
        }

        output.push_str("\n--- Edges ---\n");
        for edge in &graph.edges {
            let source_title = graph
                .nodes
                .iter()
                .find(|n| n.id == edge.source_id)
                .map(|n| n.title.as_str())
                .unwrap_or("?");
            let target_title = graph
                .nodes
                .iter()
                .find(|n| n.id == edge.target_id)
                .map(|n| n.title.as_str())
                .unwrap_or("?");
            output.push_str(&format!(
                "  [{:?}] {} --{}--> {} (confidence: {})\n",
                edge.kind, source_title, edge.label, target_title, edge.confidence,
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;
    use chrono::Duration;

    fn make_graph() -> EvidenceGraph {
        let mut g = EvidenceGraph::empty("测试");
        let n1 = Node::event("事件1", "描述1", chrono::Utc::now() - Duration::days(5));
        g.add_node(n1);
        let n2 = Node::event("事件2", "描述2", chrono::Utc::now());
        g.add_node(n2);
        g
    }

    #[test]
    fn test_json_serialization() {
        let g = make_graph();
        let json = EvidenceGraphSerializer::to_json(&g).unwrap();
        assert!(json.contains("事件1"));
        assert!(json.contains("事件2"));
    }

    #[test]
    fn test_text_summary() {
        let g = make_graph();
        let summary = EvidenceGraphSerializer::to_text_summary(&g, 10);
        assert!(summary.contains("事件1"));
        assert!(summary.contains("事件2"));
    }

    #[test]
    fn test_debug_output() {
        let g = make_graph();
        let debug = EvidenceGraphSerializer::to_debug(&g);
        assert!(debug.contains("EvidenceGraph Debug"));
        assert!(debug.contains("Event"));
    }
}
