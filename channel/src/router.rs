//! 🚦 多通道消息路由 — ChannelRouter.
//!
//! 负责将 Agent 回复自动路由回来源通道。
//!
//! ## 工作流程
//!
//! 1. 入站事件 (InboundEvent) → SessionStore.upsert_from_event() 记录会话路由
//! 2. Agent 处理完成后产生 ReplyPayload
//! 3. ChannelRouter.reply() 根据会话键查找来源通道 → 发送回复
//!
//! ## 使用
//!
//! ```rust,no_run
//! use lingshu_channel::router::ChannelRouter;
//! use lingshu_channel::session_store::SessionStore;
//! use lingshu_channel::registry::ChannelRegistry;
//! use lingshu_traits::database::Database;
//!
//! # async fn example() {
//! let registry = std::sync::Arc::new(ChannelRegistry::new());
//! # let db: std::sync::Arc<dyn Database> = unimplemented!();
//! let store = SessionStore::new(db);
//! let router = ChannelRouter::new(registry, std::sync::Arc::new(store));
//! # }
//! ```

use std::sync::Arc;
use crate::registry::ChannelRegistry;
use crate::session_store::SessionStore;
use crate::types::*;
use crate::traits::MessageChannel;
use lingshu_core::{LsContext, LsError, LsResult};

/// 多通道消息路由器.
///
/// 将 Agent 回复自动路由到正确的平台通道.
pub struct ChannelRouter {
    /// 通道注册表.
    registry: Arc<ChannelRegistry>,
    /// 会话持久化存储.
    session_store: Arc<SessionStore>,
}

impl ChannelRouter {
    /// 创建路由器.
    pub fn new(registry: Arc<ChannelRegistry>, session_store: Arc<SessionStore>) -> Self {
        Self {
            registry,
            session_store,
        }
    }

    /// 通过会话键回复消息.
    ///
    /// `session_key` 格式: `channel:{channel_id}:{peer_id}`
    /// 自动从 SessionStore 查找路由信息，将回复发送到对应通道.
    pub async fn reply_by_session(
        &self,
        ctx: &LsContext,
        session_key: &str,
        payload: ReplyPayload,
    ) -> LsResult<SendReceipt> {
        // 查找会话路由
        let route = self
            .session_store
            .get_by_session_key(ctx, session_key)
            .await?
            .ok_or_else(|| {
                LsError::Plugin(format!("会话路由未找到: {session_key}"))
            })?;

        // 查找通道
        let channel = self
            .registry
            .get(&route.channel_id)
            .await
            .ok_or_else(|| {
                LsError::Plugin(format!("通道未注册: {}", route.channel_id))
            })?;

        // 确定目标
        let target = if route.chat_type == "group" {
            route.chat_id.unwrap_or_else(|| route.sender_id.clone())
        } else {
            route.sender_id.clone()
        };

        self.send_reply(&*channel, &target, payload, None).await
    }

    /// 通过 InboundEvent 回复消息.
    ///
    /// 自动记录/更新会话路由，然后发送回复.
    pub async fn reply_to_event(
        &self,
        ctx: &LsContext,
        event: &InboundEvent,
        payload: ReplyPayload,
    ) -> LsResult<SendReceipt> {
        // 先更新会话路由
        let _route = self.session_store.upsert_from_event(ctx, event).await?;

        // 查找通道
        let channel = self
            .registry
            .get(&event.channel_id)
            .await
            .ok_or_else(|| {
                LsError::Plugin(format!("通道未注册: {}", event.channel_id))
            })?;

        // 确定发送目标
        let target = match event.chat_type {
            ChatType::Group => event.chat_id.clone().unwrap_or_else(|| {
                event.sender_id.clone().unwrap_or_default()
            }),
            ChatType::Direct | ChatType::Channel => {
                event.sender_id.clone().unwrap_or_default()
            }
        };

        if target.is_empty() {
            return Err(LsError::Plugin("回复目标为空，无法发送".into()));
        }

        self.send_reply(&*channel, &target, payload, event.message_id.as_deref())
            .await
    }

    /// 发送回复到指定目标.
    async fn send_reply(
        &self,
        channel: &dyn MessageChannel,
        target: &str,
        payload: ReplyPayload,
        reply_to: Option<&str>,
    ) -> LsResult<SendReceipt> {
        // 根据载荷类型选择合适的发送方法
        if let Some(text) = &payload.text {
            // 如果有媒体，使用 send_payload
            if payload.media_urls.as_ref().is_some_and(|v| !v.is_empty()) {
                channel
                    .send_payload(SendPayloadContext {
                        to: target.into(),
                        payload,
                        reply_to_id: reply_to.map(|s| s.to_string()),
                        thread_id: None,
                        account_id: None,
                    })
                    .await
            } else {
                // 纯文本
                channel
                    .send_text(SendTextContext {
                        to: target.into(),
                        text: text.clone(),
                        reply_to_id: reply_to.map(|s| s.to_string()),
                        thread_id: None,
                        silent: false,
                        account_id: None,
                    })
                    .await
            }
        } else if payload.media_urls.as_ref().is_some_and(|v| !v.is_empty()) {
            // 仅媒体
            let first_media = payload.media_urls.as_ref().unwrap()[0].clone();
            channel
                .send_media(SendMediaContext {
                    to: target.into(),
                    text: None,
                    media_url: first_media,
                    audio_as_voice: payload.audio_as_voice.unwrap_or(false),
                    reply_to_id: reply_to.map(|s| s.to_string()),
                    thread_id: None,
                    account_id: None,
                })
                .await
        } else {
            Err(LsError::Plugin("回复载荷为空".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_store::SessionStore;
    use crate::registry::ChannelRegistry;
    use std::sync::Arc;
    use std::collections::HashMap;
    use std::sync::RwLock;

    // 简易内存 MockDatabase (用于测试)
    struct MockDb {
        store: RwLock<HashMap<String, serde_json::Value>>,
    }
    impl MockDb {
        fn new() -> Self { Self { store: RwLock::new(HashMap::new()) } }
    }
    #[async_trait]
    impl lingshu_traits::database::Database for MockDb {
        async fn insert(&self, _ctx: lingshu_core::LsContext, _collection: &str, value: serde_json::Value) -> lingshu_core::LsResult<serde_json::Value> {
            let id = uuid::Uuid::new_v4().to_string();
            let mut full = value.clone();
            full["id"] = serde_json::json!(id);
            let sk = full["session_key"].as_str().unwrap_or("").to_string();
            self.store.write().unwrap().insert(sk, full.clone());
            Ok(full)
        }
        async fn get_by_id(&self, _ctx: lingshu_core::LsContext, _collection: &str, id: &str) -> lingshu_core::LsResult<Option<serde_json::Value>> {
            Ok(self.store.read().unwrap().get(id).cloned())
        }
        async fn query(&self, _ctx: lingshu_core::LsContext, _collection: &str, _filters: Vec<lingshu_traits::database::QueryFilter>, _pagination: lingshu_traits::database::Pagination) -> lingshu_core::LsResult<lingshu_traits::database::PaginatedResult> {
            let items: Vec<serde_json::Value> = self.store.read().unwrap().values().cloned().collect();
            let total = items.len() as u64;
            Ok(lingshu_traits::database::PaginatedResult { items, total, page: 1, page_size: total, total_pages: 1 })
        }
        async fn update(&self, _ctx: lingshu_core::LsContext, _collection: &str, id: &str, value: serde_json::Value) -> lingshu_core::LsResult<Option<serde_json::Value>> {
            let old = self.store.read().unwrap().get(id).cloned();
            self.store.write().unwrap().insert(id.to_string(), value.clone());
            Ok(old)
        }
        async fn delete(&self, _ctx: lingshu_core::LsContext, _collection: &str, id: &str) -> lingshu_core::LsResult<bool> {
            Ok(self.store.write().unwrap().remove(id).is_some())
        }
        async fn begin_transaction(&self, _ctx: lingshu_core::LsContext) -> lingshu_core::LsResult<String> { Ok("txn".into()) }
        async fn commit_transaction(&self, _ctx: lingshu_core::LsContext, _txn_id: &str) -> lingshu_core::LsResult<()> { Ok(()) }
        async fn rollback_transaction(&self, _ctx: lingshu_core::LsContext, _txn_id: &str) -> lingshu_core::LsResult<()> { Ok(()) }
    }

    fn create_test_store() -> SessionStore {
        let db = Arc::new(MockDb::new());
        SessionStore::new(db as Arc<dyn lingshu_traits::database::Database>)
    }

    // Simple mock channel for testing
    struct MockChannel {
        id: &'static str,
    }

    use async_trait::async_trait;
    #[async_trait]
    impl MessageChannel for MockChannel {
        fn id(&self) -> &'static str { self.id }
        fn meta(&self) -> ChannelMeta {
            ChannelMeta {
                label: "Mock",
                description: "mock",
                docs_url: None,
                aliases: &["mock"],
            }
        }
        fn capabilities(&self) -> ChannelCapabilities {
            ChannelCapabilities {
                chat_types: vec![ChatType::Direct, ChatType::Group],
                supports_media: true,
                supports_polls: false,
                supports_threads: false,
                supports_edit: false,
                supports_reply: true,
            }
        }
        async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
            Ok(SendReceipt {
                message_id: format!("mock-{}", ctx.to),
                thread_id: None,
                timestamp: chrono::Utc::now().timestamp(),
                raw: None,
            })
        }
        async fn send_media(&self, _ctx: SendMediaContext) -> LsResult<SendReceipt> {
            Ok(SendReceipt { message_id: "media-mock".into(), thread_id: None, timestamp: 0, raw: None })
        }
        async fn send_payload(&self, _ctx: SendPayloadContext) -> LsResult<SendReceipt> {
            Ok(SendReceipt { message_id: "payload-mock".into(), thread_id: None, timestamp: 0, raw: None })
        }
        async fn handle_inbound(&self, _event: InboundEvent) -> LsResult<()> { Ok(()) }
        async fn health_check(&self) -> LsResult<HealthStatus> {
            Ok(HealthStatus { healthy: true, latency_ms: None, error: None, connected_at: None })
        }
        fn parse_target(&self, raw: &str) -> LsResult<MessagingTarget> {
            Ok(MessagingTarget {
                normalized: raw.into(),
                kind: MessagingTargetKind::User,
                id: raw.into(),
                raw: raw.into(),
            })
        }
    }

    use lingshu_core::LsId;

    async fn setup() -> (ChannelRouter, Arc<ChannelRegistry>) {
        let registry = Arc::new(ChannelRegistry::new());
        let mock = Arc::new(MockChannel { id: "test-channel" });
        let store = create_test_store();
        let router = ChannelRouter::new(registry.clone(), Arc::new(store));
        registry.register(mock).await;
        (router, registry)
    }
    #[tokio::test]
    async fn test_reply_to_event() {
        let (router, _) = setup().await;
        let ctx = LsContext::with_session(LsId::new());

        let event = InboundEvent {
            channel_id: "test-channel".into(),
            message_id: Some("msg_1".into()),
            sender_id: Some("user_123".into()),
            sender_name: Some("Alice".into()),
            chat_type: ChatType::Direct,
            chat_id: None,
            text: Some("hello".into()),
            media_urls: vec![],
            reply_to_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: None,
        };

        let receipt = router
            .reply_to_event(&ctx, &event, ReplyPayload::text("Hi there!"))
            .await;
        assert!(receipt.is_ok(), "reply should succeed: {:?}", receipt.err());
    }

    #[tokio::test]
    async fn test_reply_by_session() {
        let (router, _) = setup().await;
        let ctx = LsContext::with_session(LsId::new());

        // Create a session first
        let event = InboundEvent {
            channel_id: "test-channel".into(),
            message_id: Some("msg_2".into()),
            sender_id: Some("user_456".into()),
            sender_name: Some("Bob".into()),
            chat_type: ChatType::Group,
            chat_id: Some("group_789".into()),
            text: Some("group hello".into()),
            media_urls: vec![],
            reply_to_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: None,
        };

        let route = router.session_store.upsert_from_event(&ctx, &event).await.unwrap();

        let receipt = router
            .reply_by_session(
                &ctx,
                &route.session_key,
                ReplyPayload::text("Group reply!"),
            )
            .await;
        assert!(receipt.is_ok(), "session reply should succeed: {:?}", receipt.err());
    }
}
