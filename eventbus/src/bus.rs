//! InMemoryEventBus — 基于内存的事件总线实现.
//!
//! 支持:
//! - 发布/订阅 (带通配符主题匹配)
//! - 事件历史记录
//! - 订阅管理

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use lingshu_traits::event_bus::{Event, EventBus, EventHandler, SubscriptionInfo};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

type SubscriberEntry = (String, String, EventHandler);

/// 内存事件总线 — 基于 RwLock + Vec 的轻量实现.
pub struct InMemoryEventBus {
    subscribers: RwLock<Vec<SubscriberEntry>>,
    published: Arc<Mutex<Vec<Event>>>,
}

impl InMemoryEventBus {
    /// 创建新的内存事件总线.
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(Vec::new()),
            published: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 获取已发布事件的历史记录.
    pub async fn history(&self) -> Vec<Event> {
        self.published.lock().await.clone()
    }

    /// 通配符主题匹配:
    /// - `*` 匹配所有
    /// - `ls.*` 匹配 `ls.` 前缀
    /// - `ls.agent.run.completed` 精确匹配
    fn topic_matches(pattern: &str, topic: &str) -> bool {
        if pattern == "*" || pattern == topic {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix(".*") {
            return topic.starts_with(prefix);
        }
        false
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, ctx: LsContext, event: Event) -> LsResult<()> {
        tracing::debug!(
            trace_id = %ctx.trace_id,
            session_id = %ctx.session_id,
            topic = %event.topic,
            event_id = %event.event_id,
            "event published"
        );
        self.published.lock().await.push(event.clone());
        let subscribers = self.subscribers.read().await;
        let matched = subscribers
            .iter()
            .filter(|(_, pattern, _)| Self::topic_matches(pattern, &event.topic))
            .count();
        for (_, pattern, handler) in subscribers.iter() {
            if Self::topic_matches(pattern, &event.topic) {
                let _ = handler(event.clone());
            }
        }
        if matched > 0 {
            tracing::trace!(
                topic = %event.topic,
                matched_subscribers = matched,
                "event delivered"
            );
        }
        Ok(())
    }

    async fn publish_batch(&self, ctx: LsContext, events: Vec<Event>) -> LsResult<()> {
        tracing::debug!(
            trace_id = %ctx.trace_id,
            batch_size = events.len(),
            "batch event publish"
        );
        for event in events {
            self.publish(ctx.clone(), event).await?;
        }
        Ok(())
    }

    async fn subscribe(
        &self,
        ctx: LsContext,
        topic_pattern: &str,
        handler: EventHandler,
    ) -> LsResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.subscribers
            .write()
            .await
            .push((id.clone(), topic_pattern.to_string(), handler));

        tracing::debug!(
            trace_id = %ctx.trace_id,
            subscription_id = %id,
            topic_pattern = %topic_pattern,
            "event subscription created"
        );

        Ok(id)
    }

    async fn unsubscribe(&self, ctx: LsContext, subscription_id: &str) -> LsResult<()> {
        let mut subs = self.subscribers.write().await;
        subs.retain(|(id, _, _)| id != subscription_id);
        tracing::debug!(
            trace_id = %ctx.trace_id,
            subscription_id = %subscription_id,
            "event subscription removed"
        );
        Ok(())
    }

    async fn list_subscriptions(&self, _ctx: LsContext) -> LsResult<Vec<SubscriptionInfo>> {
        let subs = self.subscribers.read().await;
        Ok(subs
            .iter()
            .map(|(id, pattern, _)| SubscriptionInfo {
                id: id.clone(),
                topic_pattern: pattern.clone(),
                created_at: chrono::Utc::now(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use serde_json::json;
    use std::sync::Mutex as StdMutex;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = InMemoryEventBus::new();
        let received: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let received_clone = received.clone();

        let sub_id = bus
            .subscribe(
                test_ctx(),
                "ls.test.event",
                Box::new(move |evt| {
                    let mut data = received_clone.lock().unwrap();
                    data.push(evt.topic.clone());
                    Ok(())
                }),
            )
            .await
            .unwrap();

        let event = Event {
            event_id: "evt_001".into(),
            topic: "ls.test.event".into(),
            session_id: "sess_001".into(),
            trace_id: "trace_001".into(),
            payload: json!({"key": "value"}),
            timestamp: chrono::Utc::now(),
        };

        bus.publish(test_ctx(), event).await.unwrap();

        let data = received.lock().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], "ls.test.event");

        let _ = bus.unsubscribe(test_ctx(), &sub_id).await;
    }

    #[tokio::test]
    async fn test_wildcard_subscribe() {
        let bus = InMemoryEventBus::new();
        let received: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let r = received.clone();

        bus.subscribe(
            test_ctx(),
            "ls.agent.*",
            Box::new(move |evt| {
                r.lock().unwrap().push(evt.topic);
                Ok(())
            }),
        )
        .await
        .unwrap();

        bus.publish(
            test_ctx(),
            Event {
                event_id: "e1".into(),
                topic: "ls.agent.run.completed".into(),
                session_id: "s1".into(),
                trace_id: "t1".into(),
                payload: json!({}),
                timestamp: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();

        bus.publish(
            test_ctx(),
            Event {
                event_id: "e2".into(),
                topic: "ls.unknown.event".into(),
                session_id: "s1".into(),
                trace_id: "t1".into(),
                payload: json!({}),
                timestamp: chrono::Utc::now(),
            },
        )
        .await
        .unwrap();

        let data = received.lock().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], "ls.agent.run.completed");
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let bus = InMemoryEventBus::new();
        let sub_id = bus
            .subscribe(test_ctx(), "*", Box::new(|_| Ok(())))
            .await
            .unwrap();

        let subs = bus.list_subscriptions(test_ctx()).await.unwrap();
        assert_eq!(subs.len(), 1);

        bus.unsubscribe(test_ctx(), &sub_id).await.unwrap();
        let subs = bus.list_subscriptions(test_ctx()).await.unwrap();
        assert_eq!(subs.len(), 0);
    }

    #[tokio::test]
    async fn test_publish_batch() {
        let bus = InMemoryEventBus::new();
        let events: Vec<Event> = (0..3)
            .map(|i| Event {
                event_id: format!("e{i}"),
                topic: "ls.test.batch".into(),
                session_id: "s1".into(),
                trace_id: "t1".into(),
                payload: json!({"idx": i}),
                timestamp: chrono::Utc::now(),
            })
            .collect();

        bus.publish_batch(test_ctx(), events).await.unwrap();
        let history = bus.history().await;
        assert_eq!(history.len(), 3);
    }

    #[tokio::test]
    async fn test_history() {
        let bus = InMemoryEventBus::new();
        assert!(bus.history().await.is_empty());

        let event = Event {
            event_id: "evt_hist".into(),
            topic: "ls.test.history".into(),
            session_id: "s1".into(),
            trace_id: "t1".into(),
            payload: json!({"data": 1}),
            timestamp: chrono::Utc::now(),
        };
        bus.publish(test_ctx(), event.clone()).await.unwrap();

        let history = bus.history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_id, "evt_hist");
    }
}
