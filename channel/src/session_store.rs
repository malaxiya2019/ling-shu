//! 🗃️ 通道会话持久化 — Channel Session Store.
//!
//! 存储通道会话路由，确保 Agent 回复能自动路由回来源通道。
//! 使用 Database trait 作为后端，支持 SQLite (dev) / PostgreSQL (prod)。

use crate::types::*;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::database::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 持久化的通道会话路由.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSessionRoute {
    /// 全局唯一会话键 (格式: `channel:{channel_id}:{peer_id}`).
    pub session_key: String,
    /// 来源通道 ID.
    pub channel_id: String,
    /// 聊天类型.
    pub chat_type: String,
    /// 发送者 ID (用户/机器人).
    pub sender_id: String,
    /// 发送者名称.
    pub sender_name: Option<String>,
    /// 聊天 ID.
    pub chat_id: Option<String>,
    /// 线程/主题 ID.
    pub thread_id: Option<String>,
    /// 最近一条消息的时间戳.
    pub last_message_at: i64,
    /// 会话创建时间.
    pub created_at: i64,
    /// 扩展元数据.
    pub metadata: Option<serde_json::Value>,
}

/// 通道会话持久化存储.
pub struct SessionStore {
    db: Arc<dyn Database>,
    /// 集合名称 (对应 documents 表的 collection 字段).
    collection: &'static str,
}

impl SessionStore {
    /// 创建新的会话存储.
    pub fn new(db: Arc<dyn Database>) -> Self {
        Self {
            db,
            collection: "channel_sessions",
        }
    }

    /// 从 InboundEvent 创建或更新会话路由.
    pub async fn upsert_from_event(
        &self,
        ctx: &LsContext,
        event: &InboundEvent,
    ) -> LsResult<StoredSessionRoute> {
        let peer_id = event.sender_id.as_deref().unwrap_or("unknown");
        let session_key = format!("channel:{}:{}", event.channel_id, peer_id);

        let now = chrono::Utc::now().timestamp();
        let route = StoredSessionRoute {
            session_key: session_key.clone(),
            channel_id: event.channel_id.clone(),
            chat_type: event.chat_type.to_string(),
            sender_id: event.sender_id.clone().unwrap_or_default(),
            sender_name: event.sender_name.clone(),
            chat_id: event.chat_id.clone(),
            thread_id: None,
            last_message_at: now,
            created_at: now,
            metadata: None,
        };

        // 检查是否已存在
        let existing = self.get_by_session_key(ctx, &session_key).await?;
        if let Some(_existing) = existing {
            // 更新最新时间
            let updated = StoredSessionRoute {
                last_message_at: now,
                sender_name: route.sender_name.clone(),
                .._existing
            };
            let value = serde_json::to_value(&updated)
                .map_err(|e| LsError::Plugin(format!("Session serialize: {e}")))?;
            self.db
                .update(ctx.clone(), self.collection, &session_key, value)
                .await?;
            Ok(updated)
        } else {
            let value = serde_json::to_value(&route)
                .map_err(|e| LsError::Plugin(format!("Session serialize: {e}")))?;
            self.db
                .insert(ctx.clone(), self.collection, value)
                .await?;
            Ok(route)
        }
    }

    /// 根据会话键查找路由.
    pub async fn get_by_session_key(
        &self,
        ctx: &LsContext,
        session_key: &str,
    ) -> LsResult<Option<StoredSessionRoute>> {
        let result = self
            .db
            .get_by_id(ctx.clone(), self.collection, session_key)
            .await?;
        match result {
            Some(value) => {
                let route: StoredSessionRoute = serde_json::from_value(value)
                    .map_err(|e| LsError::Plugin(format!("Session deserialize: {e}")))?;
                Ok(Some(route))
            }
            None => Ok(None),
        }
    }

    /// 根据通道 ID 查询所有会话.
    pub async fn get_by_channel(
        &self,
        ctx: &LsContext,
        channel_id: &str,
    ) -> LsResult<Vec<StoredSessionRoute>> {
        use lingshu_traits::database::{Pagination, QueryFilter};
        let filter = QueryFilter {
            field: "channel_id".into(),
            operator: "eq".into(),
            value: serde_json::Value::String(channel_id.into()),
        };
        let result = self
            .db
            .query(
                ctx.clone(),
                self.collection,
                vec![filter],
                Pagination {
                    page: 1,
                    page_size: 1000,
                },
            )
            .await?;
        let routes: Vec<StoredSessionRoute> = result
            .items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(routes)
    }

    /// 根据发送者 ID 查找会话.
    pub async fn get_by_sender(
        &self,
        ctx: &LsContext,
        sender_id: &str,
    ) -> LsResult<Option<StoredSessionRoute>> {
        use lingshu_traits::database::{Pagination, QueryFilter};
        let filter = QueryFilter {
            field: "sender_id".into(),
            operator: "eq".into(),
            value: serde_json::Value::String(sender_id.into()),
        };
        let result = self
            .db
            .query(
                ctx.clone(),
                self.collection,
                vec![filter],
                Pagination {
                    page: 1,
                    page_size: 1,
                },
            )
            .await?;
        Ok(result.items.into_iter().next().and_then(|v| serde_json::from_value(v).ok()))
    }

    /// 删除会话路由.
    pub async fn delete(&self, ctx: &LsContext, session_key: &str) -> LsResult<bool> {
        self.db
            .delete(ctx.clone(), self.collection, session_key)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::LsId;
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
    impl Database for MockDb {
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
        async fn query(&self, _ctx: lingshu_core::LsContext, _collection: &str, filters: Vec<lingshu_traits::database::QueryFilter>, _pagination: lingshu_traits::database::Pagination) -> lingshu_core::LsResult<lingshu_traits::database::PaginatedResult> {
            let items: Vec<serde_json::Value> = self.store.read().unwrap().values()
                .filter(|v| {
                    filters.iter().all(|f| {
                        v.get(&f.field).and_then(|v| v.as_str())
                            == f.value.as_str()
                    })
                })
                .cloned()
                .collect();
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
        SessionStore::new(db as Arc<dyn Database>)
    }

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    fn make_event(channel: &str, sender: &str, chat: ChatType) -> InboundEvent {
        InboundEvent {
            channel_id: channel.into(),
            message_id: Some("msg_1".into()),
            sender_id: Some(sender.into()),
            sender_name: Some("TestUser".into()),
            chat_type: chat,
            chat_id: Some("chat_1".into()),
            text: Some("hello".into()),
            media_urls: vec![],
            reply_to_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: None,
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let store = create_test_store();
        let ctx = test_ctx();
        let event = make_event("qq", "ou_123", ChatType::Direct);
        let route = store.upsert_from_event(&ctx, &event).await.unwrap();
        assert_eq!(route.channel_id, "qq");
        assert_eq!(route.sender_id, "ou_123");

        let found = store.get_by_session_key(&ctx, &route.session_key).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().sender_id, "ou_123");
    }

    #[tokio::test]
    async fn test_upsert_updates_timestamp() {
        let store = create_test_store();
        let ctx = test_ctx();
        let event = make_event("feishu", "ou_456", ChatType::Group);
        let first = store.upsert_from_event(&ctx, &event).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let second = store.upsert_from_event(&ctx, &event).await.unwrap();
        assert!(second.last_message_at >= first.last_message_at);
    }

    #[tokio::test]
    async fn test_get_by_channel() {
        let store = create_test_store();
        let ctx = test_ctx();
        store.upsert_from_event(&ctx, &make_event("qq", "u1", ChatType::Direct)).await.unwrap();
        store.upsert_from_event(&ctx, &make_event("qq", "u2", ChatType::Direct)).await.unwrap();
        store.upsert_from_event(&ctx, &make_event("telegram", "u3", ChatType::Direct)).await.unwrap();

        let qq_sessions = store.get_by_channel(&ctx, "qq").await.unwrap();
        assert_eq!(qq_sessions.len(), 2);

        let tg_sessions = store.get_by_channel(&ctx, "telegram").await.unwrap();
        assert_eq!(tg_sessions.len(), 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let store = create_test_store();
        let ctx = test_ctx();
        let route = store.upsert_from_event(&ctx, &make_event("qq", "ou_del", ChatType::Direct)).await.unwrap();
        assert!(store.delete(&ctx, &route.session_key).await.unwrap());
        let found = store.get_by_session_key(&ctx, &route.session_key).await.unwrap();
        assert!(found.is_none());
    }
}
