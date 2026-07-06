//! 审计查询构建器 — 链式构建审计查询条件.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::log::AuditEventType;

/// 审计查询条件.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditQuery {
    pub actor: Option<String>,
    pub event_type: Option<AuditEventType>,
    pub event_name: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub result: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

/// 审计查询构建器.
#[derive(Debug, Default)]
pub struct AuditQueryBuilder {
    query: AuditQuery,
}

impl AuditQueryBuilder {
    pub fn new() -> Self {
        Self {
            query: AuditQuery::default(),
        }
    }

    pub fn with_actor(mut self, actor: &str) -> Self {
        self.query.actor = Some(actor.to_string());
        self
    }

    pub fn with_event_type(mut self, event_type: AuditEventType) -> Self {
        self.query.event_type = Some(event_type);
        self
    }

    pub fn with_event_name(mut self, event_name: &str) -> Self {
        self.query.event_name = Some(event_name.to_string());
        self
    }

    pub fn with_resource_type(mut self, resource_type: &str) -> Self {
        self.query.resource_type = Some(resource_type.to_string());
        self
    }

    pub fn with_resource_id(mut self, resource_id: &str) -> Self {
        self.query.resource_id = Some(resource_id.to_string());
        self
    }

    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.query.start_time = Some(start);
        self.query.end_time = Some(end);
        self
    }

    pub fn with_result(mut self, result: &str) -> Self {
        self.query.result = Some(result.to_string());
        self
    }

    pub fn with_offset(mut self, offset: u64) -> Self {
        self.query.offset = Some(offset);
        self
    }

    pub fn with_limit(mut self, limit: u64) -> Self {
        self.query.limit = Some(limit);
        self
    }

    pub fn build(self) -> AuditQuery {
        self.query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let q = AuditQueryBuilder::new().build();
        assert!(q.actor.is_none());
        assert!(q.limit.is_none());
    }

    #[test]
    fn test_builder_with_all_fields() {
        let _now = Utc::now();
        let q = AuditQueryBuilder::new()
            .with_actor("alice")
            .with_event_type(AuditEventType::ApiCall)
            .with_event_name("api.chat")
            .with_resource_type("model")
            .with_resource_id("gpt-4")
            .with_result("success")
            .with_offset(10)
            .with_limit(50)
            .build();

        assert_eq!(q.actor, Some("alice".to_string()));
        assert_eq!(q.event_type, Some(AuditEventType::ApiCall));
        assert_eq!(q.event_name, Some("api.chat".to_string()));
        assert_eq!(q.resource_type, Some("model".to_string()));
        assert_eq!(q.resource_id, Some("gpt-4".to_string()));
        assert_eq!(q.result, Some("success".to_string()));
        assert_eq!(q.offset, Some(10));
        assert_eq!(q.limit, Some(50));
        assert!(q.start_time.is_none());
        assert!(q.end_time.is_none());
    }
}
