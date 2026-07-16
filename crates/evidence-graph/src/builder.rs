//! EvidenceGraph 构建器。
//!
//! 提供从各种数据源构建 EvidenceGraph 的工具函数。

use chrono::Utc;
use lingshu_memory_episode::Episode;
use tracing::debug;

use crate::{Edge, EvidenceGraph, GraphMetadata, Node, NodeId};
use std::collections::HashMap;

/// EvidenceGraphBuilder — 从不同数据源构建 EvidenceGraph。
pub struct EvidenceGraphBuilder;

impl EvidenceGraphBuilder {
    /// 从 Episode 列表构建证据图。
    ///
    /// 逻辑：
    /// - 每个 Episode → 一个 Event 节点
    /// - 每个 Entity → 一个 Entity 节点（如果还没有）
    /// - 按时间先后添加 Temporal 边
    /// - Episode 中的 StateChange → StateChange 边
    pub fn from_episodes(
        episodes: &[Episode],
        query: &str,
        source_name: &str,
        build_time_ms: u64,
    ) -> EvidenceGraph {
        let _start = std::time::Instant::now();
        let mut graph = EvidenceGraph {
            nodes: Vec::with_capacity(episodes.len() * 2),
            edges: Vec::with_capacity(episodes.len() * 2),
            metadata: GraphMetadata {
                query: query.to_string(),
                created_at: Utc::now(),
                node_count: 0,
                edge_count: 0,
                entities: Vec::new(),
                time_span_start: episodes.first().map(|e| e.timestamp),
                time_span_end: episodes.last().map(|e| e.timestamp),
                build_time_ms,
                source: source_name.to_string(),
                attributes: HashMap::new(),
            },
        };

        let mut entity_node_map: HashMap<String, NodeId> = HashMap::new();
        let mut event_nodes: Vec<(NodeId, chrono::DateTime<chrono::Utc>)> = Vec::new();

        for ep in episodes {
            // 为 Episode 创建 Entity 节点
            for entity in &ep.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                if !entity_node_map.contains_key(&key) {
                    let enode = Node::entity(entity.clone())
                        .with_source(ep.id.to_string());
                    let id = enode.id;
                    entity_node_map.insert(key, id);
                    graph.add_node(enode);
                    graph.metadata.entities.push(format!("{}:{}", entity.kind, entity.name));
                }
            }

            // 创建 Event 节点
            let mut event_node = Node::event(&ep.title, &ep.summary, ep.timestamp)
                .with_source(ep.id.to_string())
                .with_confidence(1.0);
            for tag in &ep.tags {
                event_node = event_node.with_tag(tag);
            }
            let event_id = event_node.id;
            graph.add_node(event_node);
            event_nodes.push((event_id, ep.timestamp));

            // 连接事件到实体
            for entity in &ep.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                if let Some(entity_id) = entity_node_map.get(&key) {
                    graph.add_edge(Edge::related(
                        event_id,
                        *entity_id,
                        format!("涉及{}:{}", entity.kind, entity.name),
                    ));
                }
            }

            // 状态变更边
            for sc in &ep.state_changes {
                let entity_key = format!("{}:{}", sc.entity.kind, sc.entity.name);
                if let Some(entity_id) = entity_node_map.get(&entity_key) {
                    graph.add_edge(
                        Edge::state_change(
                            event_id,
                            *entity_id,
                            format!("{}: {} → {}", sc.change_type, sc.from.as_deref().unwrap_or("?"), sc.to),
                        )
                        .with_confidence(0.9),
                    );
                }
            }
        }

        // 添加 Temporal 边
        for i in 0..event_nodes.len().saturating_sub(1) {
            let (from_id, _) = event_nodes[i];
            let (to_id, _) = event_nodes[i + 1];
            graph.add_edge(Edge::temporal(from_id, to_id));
        }

        // 如果事件节点超过2个，连接首尾
        if event_nodes.len() > 2 {
            let (first_id, _) = event_nodes.first().unwrap();
            let (last_id, _) = event_nodes.last().unwrap();
            graph.add_edge(
                Edge::temporal(*first_id, *last_id)
                    .with_confidence(0.8),
            );
        }

        // 更新元数据
        graph.metadata.node_count = graph.nodes.len();
        graph.metadata.edge_count = graph.edges.len();

        debug!(
            nodes = graph.nodes.len(),
            edges = graph.edges.len(),
            entities = graph.metadata.entities.len(),
            "EvidenceGraph built from episodes"
        );

        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use lingshu_memory_episode::{EntityRef, Episode, EpisodeId, StateChange};

    fn make_episode(title: &str, summary: &str, days_ago: i64, entities: Vec<(&str, &str)>, tags: Vec<&str>) -> Episode {
        let mut ep = Episode::new(title, summary, Utc::now() - Duration::days(days_ago));
        for (kind, name) in entities {
            ep = ep.with_entity(EntityRef::new(kind, name));
        }
        for tag in tags {
            ep = ep.with_tag(tag);
        }
        ep
    }

    #[test]
    fn test_builder_from_episodes() {
        let episodes = vec![
            make_episode("启动项目A", "项目开始", 60, vec![("project", "项目A"), ("person", "张三")], vec!["launch"]),
            make_episode("暂停项目A", "项目暂停", 15, vec![("project", "项目A")], vec!["decision"]),
        ];

        let graph = EvidenceGraphBuilder::from_episodes(&episodes, "项目A历史", "test", 10);

        assert!(graph.nodes.len() >= 2);  // 至少2个事件节点
        assert!(graph.edges.len() >= 3);  // 至少2个related + 1个temporal

        // 应该有实体节点
        let entity_nodes = graph.nodes_by_kind(crate::NodeKind::Entity);
        assert!(entity_nodes.len() >= 1);
    }
}
