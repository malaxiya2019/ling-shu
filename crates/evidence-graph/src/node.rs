//! EvidenceGraph 节点类型。

use chrono::{DateTime, Utc};
use lingshu_memory_episode::EntityRef;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 节点 ID。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct NodeId(Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(u: Uuid) -> Self {
        Self(u)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 节点类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeKind {
    /// 事件：发生在特定时间点的事实
    Event,
    /// 事实：静态陈述，无时间关联
    Fact,
    /// 实体：人、项目、组织等
    Entity,
}

/// EvidenceGraph 中的节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// 节点唯一标识
    pub id: NodeId,
    /// 节点类型
    pub kind: NodeKind,
    /// 节点标题
    pub title: String,
    /// 节点详细描述
    pub description: String,
    /// 事件时间戳（仅 Event 类型）
    pub timestamp: Option<DateTime<Utc>>,
    /// 实体引用（仅 Entity 类型）
    pub entity: Option<EntityRef>,
    /// 源引用（如 Episode ID、文档 ID）
    pub source_ref: Option<String>,
    /// 可信度 (0.0 ~ 1.0)
    pub confidence: f64,
    /// 与查询的相关性评分 (0.0 ~ 1.0)，由 RankingScorer 计算
    pub relevance_score: f64,
    /// 标签
    pub tags: Vec<String>,
    /// 原始元数据
    pub metadata: serde_json::Value,
}

impl Node {
    /// 创建一个事件节点。
    pub fn event(
        title: impl Into<String>,
        description: impl Into<String>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: NodeId::new(),
            kind: NodeKind::Event,
            title: title.into(),
            description: description.into(),
            timestamp: Some(timestamp),
            entity: None,
            source_ref: None,
            confidence: 1.0,
            relevance_score: 0.0,
            tags: Vec::new(),
            metadata: serde_json::json!({}),
        }
    }

    /// 创建一个事实节点。
    pub fn fact(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: NodeId::new(),
            kind: NodeKind::Fact,
            title: title.into(),
            description: description.into(),
            timestamp: None,
            entity: None,
            source_ref: None,
            confidence: 1.0,
            relevance_score: 0.0,
            tags: Vec::new(),
            metadata: serde_json::json!({}),
        }
    }

    /// 创建一个实体节点。
    pub fn entity(entity: EntityRef) -> Self {
        Self {
            id: NodeId::new(),
            kind: NodeKind::Entity,
            title: format!("{}:{}", entity.kind, entity.name),
            description: String::new(),
            timestamp: None,
            entity: Some(entity),
            source_ref: None,
            confidence: 1.0,
            relevance_score: 0.0,
            tags: Vec::new(),
            metadata: serde_json::json!({}),
        }
    }

    /// 设置可信度。
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// 设置源引用。
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source_ref = Some(source.into());
        self
    }

    /// 设置相关性评分。
    pub fn with_relevance(mut self, score: f64) -> Self {
        self.relevance_score = score.clamp(0.0, 1.0);
        self
    }

    /// 添加标签。
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_node() {
        let node = Node::event("启动项目", "项目启动", Utc::now());
        assert_eq!(node.kind, NodeKind::Event);
        assert!(node.timestamp.is_some());
    }

    #[test]
    fn test_fact_node() {
        let node = Node::fact("事实陈述", "这是一个事实");
        assert_eq!(node.kind, NodeKind::Fact);
        assert!(node.timestamp.is_none());
    }

    #[test]
    fn test_entity_node() {
        let entity = EntityRef::new("project", "项目A");
        let node = Node::entity(entity.clone());
        assert_eq!(node.kind, NodeKind::Entity);
        assert_eq!(node.entity, Some(entity));
    }

    #[test]
    fn test_confidence_clamping() {
        let node = Node::fact("test", "desc").with_confidence(1.5);
        assert_eq!(node.confidence, 1.0);

        let node = Node::fact("test", "desc").with_confidence(-0.5);
        assert_eq!(node.confidence, 0.0);
    }
}
