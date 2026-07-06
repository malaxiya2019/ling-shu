//! 事件溯源 — 通过存储的事件流重建系统状态.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 溯源事件.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub version: u64,
    pub data: serde_json::Value,
    pub metadata: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// 事件存储 trait.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// 持久化一个事件.
    async fn save(&self, event: StoredEvent) -> LsResult<()>;

    /// 获取聚合根的所有事件（按版本升序）.
    async fn get_events(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> LsResult<Vec<StoredEvent>>;

    /// 获取聚合根的当前版本.
    async fn get_current_version(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> LsResult<u64>;
}

/// 内存事件存储.
#[derive(Clone)]
pub struct InMemoryEventStore {
    events: Arc<RwLock<Vec<StoredEvent>>>,
}

impl InMemoryEventStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn save(&self, event: StoredEvent) -> LsResult<()> {
        self.events.write().await.push(event);
        Ok(())
    }

    async fn get_events(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> LsResult<Vec<StoredEvent>> {
        let events = self.events.read().await;
        let mut filtered: Vec<_> = events
            .iter()
            .filter(|e| e.aggregate_type == aggregate_type && e.aggregate_id == aggregate_id)
            .cloned()
            .collect();
        filtered.sort_by_key(|e| e.version);
        Ok(filtered)
    }

    async fn get_current_version(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> LsResult<u64> {
        let events = self.get_events(aggregate_type, aggregate_id).await?;
        Ok(events.last().map(|e| e.version).unwrap_or(0))
    }
}

/// 事件溯源器 — 基于事件重建状态.
pub struct EventSourcer {
    store: Arc<dyn EventStore>,
}

impl EventSourcer {
    pub fn new(store: Arc<dyn EventStore>) -> Self {
        Self { store }
    }

    /// 记录新事件.
    pub async fn record_event(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
        event_type: &str,
        data: serde_json::Value,
    ) -> LsResult<StoredEvent> {
        let current_version = self
            .store
            .get_current_version(aggregate_type, aggregate_id)
            .await?;

        let event = StoredEvent {
            id: Uuid::new_v4(),
            aggregate_type: aggregate_type.to_string(),
            aggregate_id: aggregate_id.to_string(),
            event_type: event_type.to_string(),
            version: current_version + 1,
            data,
            metadata: serde_json::json!({
                "recorded_by": "lingshu-audit",
                "recorded_at": Utc::now().to_rfc3339(),
            }),
            timestamp: Utc::now(),
        };

        self.store.save(event.clone()).await?;
        Ok(event)
    }

    /// 重放事件流.
    pub async fn replay_events(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> LsResult<Vec<StoredEvent>> {
        self.store.get_events(aggregate_type, aggregate_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> EventSourcer {
        let store = Arc::new(InMemoryEventStore::new());
        EventSourcer::new(store)
    }

    #[tokio::test]
    async fn test_record_and_replay() {
        let sourcer = setup();

        let event = sourcer
            .record_event(
                "agent",
                "agent-123",
                "AgentCreated",
                serde_json::json!({"name": "test-agent"}),
            )
            .await
            .unwrap();

        assert_eq!(event.version, 1);
        assert_eq!(event.aggregate_type, "agent");
        assert_eq!(event.aggregate_id, "agent-123");

        let events = sourcer.replay_events("agent", "agent-123").await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "AgentCreated");
    }

    #[tokio::test]
    async fn test_version_increments() {
        let sourcer = setup();

        sourcer
            .record_event("session", "sess-1", "Started", serde_json::json!({}))
            .await
            .unwrap();
        sourcer
            .record_event("session", "sess-1", "MessageAdded", serde_json::json!({}))
            .await
            .unwrap();
        sourcer
            .record_event("session", "sess-1", "Ended", serde_json::json!({}))
            .await
            .unwrap();

        let events = sourcer.replay_events("session", "sess-1").await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].version, 1);
        assert_eq!(events[1].version, 2);
        assert_eq!(events[2].version, 3);
    }

    #[tokio::test]
    async fn test_isolated_aggregates() {
        let sourcer = setup();

        sourcer
            .record_event("agent", "a-1", "Created", serde_json::json!({}))
            .await
            .unwrap();
        sourcer
            .record_event("agent", "a-2", "Created", serde_json::json!({}))
            .await
            .unwrap();

        let events = sourcer.replay_events("agent", "a-1").await.unwrap();
        assert_eq!(events.len(), 1);
    }
}
