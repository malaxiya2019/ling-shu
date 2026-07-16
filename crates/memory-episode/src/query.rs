//! Episode 查询类型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::EntityRef;

/// EpisodeQuery — 事件查询参数。
///
/// 支持按时间范围、实体、标签、全文搜索等维度过滤。
/// 不涉及语义相似度 — 这是 L2 RAG 的职责。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeQuery {
    /// 关联实体过滤
    pub entities: Vec<EntityRef>,
    /// 标签过滤（任一匹配）
    pub tags: Vec<String>,
    /// 时间范围起始
    pub time_from: Option<DateTime<Utc>>,
    /// 时间范围结束
    pub time_to: Option<DateTime<Utc>>,
    /// 标题/摘要关键词搜索
    pub search_text: Option<String>,
    /// 会话 ID 过滤
    pub session_id: Option<String>,
    /// 来源引用过滤
    pub source_ref: Option<String>,
    /// 返回数量上限
    pub limit: usize,
    /// 偏移量
    pub offset: usize,
    /// 排序方向
    pub sort_order: SortOrder,
}

/// 排序方向。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortOrder {
    /// 时间升序（旧→新）
    Ascending,
    /// 时间降序（新→旧，默认）
    Descending,
}

impl Default for EpisodeQuery {
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            tags: Vec::new(),
            time_from: None,
            time_to: None,
            search_text: None,
            session_id: None,
            source_ref: None,
            limit: 100,
            offset: 0,
            sort_order: SortOrder::Descending,
        }
    }
}

impl EpisodeQuery {
    /// 按时间范围过滤。
    pub fn with_time_range(mut self, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        self.time_from = Some(from);
        self.time_to = Some(to);
        self
    }

    /// 按实体过滤。
    pub fn with_entity(mut self, entity: EntityRef) -> Self {
        self.entities.push(entity);
        self
    }

    /// 按标签过滤。
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 按关键词搜索。
    pub fn with_search(mut self, text: impl Into<String>) -> Self {
        self.search_text = Some(text.into());
        self
    }

    /// 按会话过滤。
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// 设置返回上限。
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// 设置排序方向。
    pub fn with_sort(mut self, order: SortOrder) -> Self {
        self.sort_order = order;
        self
    }
}

/// 查询结果统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    pub total_matched: usize,
    pub returned: usize,
    pub query_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_defaults() {
        let q = EpisodeQuery::default();
        assert_eq!(q.limit, 100);
        assert_eq!(q.sort_order, SortOrder::Descending);
        assert!(q.entities.is_empty());
    }

    #[test]
    fn test_query_builder() {
        let q = EpisodeQuery::default()
            .with_entity(EntityRef::new("project", "项目A"))
            .with_tag("milestone")
            .with_limit(20);

        assert_eq!(q.entities.len(), 1);
        assert_eq!(q.tags.len(), 1);
        assert_eq!(q.limit, 20);
    }
}
