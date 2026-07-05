use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 事件.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub topic: String,
    pub session_id: String,
    pub trace_id: String,
    pub payload: Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 订阅者处理函数.
pub type EventHandler = Box<dyn Fn(Event) -> LsResult<()> + Send + Sync>;

/// 订阅信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub id: String,
    pub topic_pattern: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// EventBus — 事件发布、订阅、重试与持久化.
#[async_trait]
pub trait EventBus: Send + Sync + 'static {
    /// 发布事件.
    async fn publish(&self, ctx: LsContext, event: Event) -> LsResult<()>;

    /// 批量发布.
    async fn publish_batch(&self, ctx: LsContext, events: Vec<Event>) -> LsResult<()>;

    /// 订阅主题 (支持通配符).
    async fn subscribe(
        &self,
        ctx: LsContext,
        topic_pattern: &str,
        handler: EventHandler,
    ) -> LsResult<String>;

    /// 取消订阅.
    async fn unsubscribe(&self, ctx: LsContext, subscription_id: &str) -> LsResult<()>;

    /// 列出当前订阅.
    async fn list_subscriptions(&self, ctx: LsContext) -> LsResult<Vec<SubscriptionInfo>>;
}

// ── Blanket impl: Box<dyn EventBus> 也实现 EventBus ──

#[async_trait]
impl<T: EventBus + ?Sized> EventBus for Box<T> {
    async fn publish(&self, ctx: LsContext, event: Event) -> LsResult<()> {
        (**self).publish(ctx, event).await
    }
    async fn publish_batch(&self, ctx: LsContext, events: Vec<Event>) -> LsResult<()> {
        (**self).publish_batch(ctx, events).await
    }
    async fn subscribe(
        &self,
        ctx: LsContext,
        topic_pattern: &str,
        handler: EventHandler,
    ) -> LsResult<String> {
        (**self).subscribe(ctx, topic_pattern, handler).await
    }
    async fn unsubscribe(&self, ctx: LsContext, subscription_id: &str) -> LsResult<()> {
        (**self).unsubscribe(ctx, subscription_id).await
    }
    async fn list_subscriptions(&self, ctx: LsContext) -> LsResult<Vec<SubscriptionInfo>> {
        (**self).list_subscriptions(ctx).await
    }
}
