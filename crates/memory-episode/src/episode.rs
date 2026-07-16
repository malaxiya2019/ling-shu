//! 核心 Episode 数据结构。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 实体引用 — 表示一个被提及的人、项目、文档等。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityRef {
    /// 实体类型，如 "person", "project", "document", "tool"
    pub kind: String,
    /// 实体名称或标识
    pub name: String,
}

impl EntityRef {
    pub fn new(kind: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
        }
    }
}

/// 状态变更 — 事件导致的实体状态变化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// 受影响的实体
    pub entity: EntityRef,
    /// 变更前的状态（可选）
    pub from: Option<String>,
    /// 变更后的状态
    pub to: String,
    /// 变更类型，如 "status", "phase", "ownership"
    pub change_type: String,
}

impl StateChange {
    pub fn new(
        entity: EntityRef,
        change_type: impl Into<String>,
        from: Option<String>,
        to: impl Into<String>,
    ) -> Self {
        Self {
            entity,
            from,
            to: to.into(),
            change_type: change_type.into(),
        }
    }
}

/// EpisodeId — 事件唯一标识。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct EpisodeId(Uuid);

impl EpisodeId {
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

impl Default for EpisodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EpisodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Episode — 单个事件记录。
///
/// 代表一个发生在特定时间点的、有明确主体和状态变更的事实。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// 事件唯一标识
    pub id: EpisodeId,
    /// 事件发生时间
    pub timestamp: DateTime<Utc>,
    /// 事件标题（简短概括）
    pub title: String,
    /// 事件详细描述
    pub summary: String,
    /// 关联的实体列表
    pub entities: Vec<EntityRef>,
    /// 标签（用于快速过滤）
    pub tags: Vec<String>,
    /// 状态变更记录
    pub state_changes: Vec<StateChange>,
    /// 会话 ID（来源会话）
    pub session_id: Option<String>,
    /// 可选的源引用（如消息 ID、文档 ID）
    pub source_ref: Option<String>,
    /// 自定义元数据
    pub metadata: HashMap<String, String>,
}

impl Episode {
    /// 创建一个新事件。
    pub fn new(
        title: impl Into<String>,
        summary: impl Into<String>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: EpisodeId::new(),
            timestamp,
            title: title.into(),
            summary: summary.into(),
            entities: Vec::new(),
            tags: Vec::new(),
            state_changes: Vec::new(),
            session_id: None,
            source_ref: None,
            metadata: HashMap::new(),
        }
    }

    /// 添加实体。
    pub fn with_entity(mut self, entity: EntityRef) -> Self {
        self.entities.push(entity);
        self
    }

    /// 添加标签。
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 添加状态变更。
    pub fn with_state_change(mut self, change: StateChange) -> Self {
        self.state_changes.push(change);
        self
    }

    /// 设置会话 ID。
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 设置源引用。
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source_ref = Some(source.into());
        self
    }

    /// 添加元数据。
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_episode_creation() {
        let ep = Episode::new(
            "启动项目A",
            "团队决定启动项目A的开发",
            Utc::now(),
        )
        .with_entity(EntityRef::new("project", "项目A"))
        .with_entity(EntityRef::new("person", "张三"))
        .with_tag("project")
        .with_tag("launch");

        assert_eq!(ep.title, "启动项目A");
        assert_eq!(ep.entities.len(), 2);
        assert_eq!(ep.tags.len(), 2);
        assert!(!ep.id.to_string().is_empty());
    }

    #[test]
    fn test_state_change() {
        let change = StateChange::new(
            EntityRef::new("project", "项目A"),
            "status",
            Some("planning".to_string()),
            "active",
        );
        assert_eq!(change.change_type, "status");
        assert_eq!(change.from, Some("planning".to_string()));
        assert_eq!(change.to, "active");
    }

    #[test]
    fn test_episode_id_unique() {
        let id1 = EpisodeId::new();
        let id2 = EpisodeId::new();
        assert_ne!(id1, id2);
    }
}
