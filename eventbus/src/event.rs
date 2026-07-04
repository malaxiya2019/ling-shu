use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::topic::EventTopic;

/// 标准事件结构，遵循 LSCode v1.0.0 规范必带字段.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsEvent {
    pub event_id: String,
    pub topic: String,
    pub session_id: String,
    pub trace_id: String,
    pub timestamp: DateTime<Utc>,
    pub payload: Value,
    /// 事件版本号，用于数据演进.
    #[serde(default = "default_event_version")]
    pub version: u8,
    /// 是否已脱敏.
    #[serde(default)]
    pub sanitized: bool,
}

fn default_event_version() -> u8 {
    1
}

impl LsEvent {
    /// 构造标准事件.
    pub fn new(
        topic: EventTopic,
        session_id: impl Into<String>,
        trace_id: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            topic: topic.to_string(),
            session_id: session_id.into(),
            trace_id: trace_id.into(),
            timestamp: Utc::now(),
            payload,
            version: 1,
            sanitized: false,
        }
    }

    /// 标记为已脱敏.
    pub fn mark_sanitized(mut self) -> Self {
        self.sanitized = true;
        self
    }
}

/// 事件投递状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
    DeadLettered,
}

/// 死信队列条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    pub event: LsEvent,
    pub failure_reason: String,
    pub retry_count: u32,
    pub failed_at: DateTime<Utc>,
}
