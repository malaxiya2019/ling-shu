//! MemorySQLite — 基于 SQLite 的持久化记忆实现.
//!
//! 提供完整的 `Memory` trait 实现，数据持久化到 SQLite 文件。
//! 支持:
//! - 会话隔离 (每个 session_id 独立命名空间)
//! - TTL 自动过期清理
//! - 关键词搜索 (LIKE)
//! - 批量写入

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::memory::{Memory, MemoryItem, MemorySearchResult};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// 基于 SQLite 的记忆存储.
pub struct MemorySQLite {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl MemorySQLite {
    /// 创建或打开 SQLite 记忆数据库.
    pub fn new(path: impl AsRef<Path>) -> LsResult<Self> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| LsError::Internal(format!("failed to open SQLite memory db: {e}")))?;
        let db = Self::init_db(conn)?;
        info!("MemorySQLite initialized");
        Ok(Self { db })
    }

    /// 创建纯内存数据库 (测试用).
    pub fn in_memory() -> LsResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| LsError::Internal(format!("failed to open in-memory SQLite: {e}")))?;
        let db = Self::init_db(conn)?;
        Ok(Self { db })
    }

    fn init_db(conn: rusqlite::Connection) -> LsResult<Arc<Mutex<rusqlite::Connection>>> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id          TEXT PRIMARY KEY,
                session_id  TEXT NOT NULL,
                content     TEXT NOT NULL,
                content_type TEXT NOT NULL DEFAULT 'text',
                metadata    TEXT NOT NULL DEFAULT '{}',
                ttl_seconds INTEGER,
                created_at  TEXT NOT NULL,
                expires_at  TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_memories_session
                ON memories(session_id);

            CREATE INDEX IF NOT EXISTS idx_memories_expires
                ON memories(expires_at);
            ",
        )
        .map_err(|e| LsError::Internal(format!("failed to init memory schema: {e}")))?;

        Ok(Arc::new(Mutex::new(conn)))
    }

    fn build_insert(
        item: &MemoryItem,
    ) -> LsResult<(
        String,
        String,
        String,
        String,
        Option<u64>,
        String,
        Option<String>,
    )> {
        let expires_at = item.ttl_seconds.map(|ttl| {
            let exp = item.created_at + chrono::Duration::seconds(ttl as i64);
            exp.to_rfc3339()
        });

        let metadata_str = serde_json::to_string(&item.metadata)
            .map_err(|e| LsError::Serialization(format!("metadata serialization: {e}")))?;

        let content_str = match &item.content {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        Ok((
            item.memory_id.to_string(),
            item.session_id.to_string(),
            content_str,
            metadata_str,
            item.ttl_seconds,
            item.created_at.to_rfc3339(),
            expires_at,
        ))
    }

    fn row_to_item(
        id: String,
        session_id: String,
        content: String,
        metadata_str: String,
        ttl_seconds: Option<u64>,
        created_at_str: String,
        _expires_at_str: Option<String>,
    ) -> LsResult<MemoryItem> {
        let memory_id = id
            .parse()
            .map_err(|e| LsError::Internal(format!("invalid memory_id in db: {e}")))?;
        let session_id = session_id
            .parse()
            .map_err(|e| LsError::Internal(format!("invalid session_id in db: {e}")))?;

        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| LsError::Internal(format!("invalid created_at in db: {e}")))?
            .with_timezone(&chrono::Utc);

        let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)
            .map_err(|e| LsError::Serialization(format!("metadata deserialization: {e}")))?;

        Ok(MemoryItem {
            memory_id,
            session_id,
            content: Value::String(content),
            metadata,
            created_at,
            ttl_seconds,
        })
    }
}

#[async_trait]
impl Memory for MemorySQLite {
    async fn write(&self, _ctx: LsContext, item: MemoryItem) -> LsResult<LsId> {
        let db = self.db.lock().await;
        let (id, sid, content, meta, ttl, created, expires) = Self::build_insert(&item)?;

        db.execute(
            "INSERT INTO memories (id, session_id, content, metadata, ttl_seconds, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, sid, content, meta, ttl, created, expires],
        )
        .map_err(|e| LsError::Internal(format!("failed to insert memory: {e}")))?;

        debug!(memory_id = %item.memory_id, "memory written");
        Ok(item.memory_id)
    }

    async fn write_batch(&self, _ctx: LsContext, items: Vec<MemoryItem>) -> LsResult<Vec<LsId>> {
        let mut db = self.db.lock().await;
        let mut ids = Vec::with_capacity(items.len());

        let tx = db
            .transaction()
            .map_err(|e| LsError::Internal(format!("failed to start transaction: {e}")))?;

        for item in &items {
            let (id, sid, content, meta, ttl, created, expires) = Self::build_insert(item)?;

            tx.execute(
                "INSERT INTO memories (id, session_id, content, metadata, ttl_seconds, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![id, sid, content, meta, ttl, created, expires],
            )
            .map_err(|e| LsError::Internal(format!("failed to insert memory in batch: {e}")))?;

            ids.push(item.memory_id);
        }

        tx.commit()
            .map_err(|e| LsError::Internal(format!("failed to commit batch: {e}")))?;

        debug!(count = items.len(), "memories batch written");
        Ok(ids)
    }

    async fn read(&self, _ctx: LsContext, memory_id: LsId) -> LsResult<MemoryItem> {
        let db = self.db.lock().await;
        let id_str = memory_id.to_string();

        let result = db.query_row(
            "SELECT id, session_id, content, metadata, ttl_seconds, created_at, expires_at
             FROM memories WHERE id = ?1",
            rusqlite::params![id_str],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let content: String = row.get(2)?;
                let meta: String = row.get(3)?;
                let ttl: Option<u64> = row.get(4)?;
                let created: String = row.get(5)?;
                let expires: Option<String> = row.get(6)?;
                Ok((id, sid, content, meta, ttl, created, expires))
            },
        );

        match result {
            Ok((id, sid, content, meta, ttl, created, expires)) => {
                Self::row_to_item(id, sid, content, meta, ttl, created, expires)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(LsError::NotFound(format!("memory {memory_id}")))
            }
            Err(e) => Err(LsError::Internal(format!(
                "failed to read memory '{memory_id}': {e}"
            ))),
        }
    }

    async fn search(
        &self,
        _ctx: LsContext,
        query: &str,
        limit: u64,
    ) -> LsResult<MemorySearchResult> {
        let db = self.db.lock().await;
        let limit_val = if limit == 0 { 10 } else { limit as i64 };
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));

        let mut stmt = db
            .prepare(
                "SELECT id, session_id, content, metadata, ttl_seconds, created_at, expires_at
                 FROM memories
                 WHERE content LIKE ?1 ESCAPE '\\'
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| LsError::Internal(format!("LIKE prepare error: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![pattern, limit_val], |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let content: String = row.get(2)?;
                let meta: String = row.get(3)?;
                let ttl: Option<u64> = row.get(4)?;
                let created: String = row.get(5)?;
                let expires: Option<String> = row.get(6)?;
                Ok((id, sid, content, meta, ttl, created, expires))
            })
            .map_err(|e| LsError::Internal(format!("LIKE query error: {e}")))?;

        let mut items: Vec<MemoryItem> = Vec::new();
        for row in rows {
            match row {
                Ok((id, sid, content, meta, ttl, created, expires)) => {
                    match Self::row_to_item(id, sid, content, meta, ttl, created, expires) {
                        Ok(item) => items.push(item),
                        Err(e) => debug!("row parse error: {e}"),
                    }
                }
                Err(e) => debug!("query row error: {e}"),
            }
        }

        let total = items.len() as u64;
        debug!(total, query = %query, "memory search completed");

        Ok(MemorySearchResult { items, total })
    }

    async fn delete(&self, _ctx: LsContext, memory_id: LsId) -> LsResult<()> {
        let db = self.db.lock().await;
        let id_str = memory_id.to_string();

        let affected = db
            .execute(
                "DELETE FROM memories WHERE id = ?1",
                rusqlite::params![id_str],
            )
            .map_err(|e| {
                LsError::Internal(format!("failed to delete memory '{memory_id}': {e}"))
            })?;

        if affected == 0 {
            return Err(LsError::NotFound(format!("memory {memory_id}")));
        }
        debug!(memory_id = %memory_id, "memory deleted");
        Ok(())
    }

    async fn clean_expired(&self, _ctx: LsContext) -> LsResult<u64> {
        let db = self.db.lock().await;
        let now = chrono::Utc::now().to_rfc3339();

        let count = db
            .execute(
                "DELETE FROM memories WHERE expires_at IS NOT NULL AND expires_at < ?1",
                rusqlite::params![now],
            )
            .map_err(|e| LsError::Internal(format!("failed to clean expired memories: {e}")))?;

        if count > 0 {
            info!(expired_count = count, "expired memories cleaned");
        }
        Ok(count as u64)
    }

    async fn clear_session(&self, _ctx: LsContext, session_id: LsId) -> LsResult<()> {
        let db = self.db.lock().await;
        let sid = session_id.to_string();

        let count = db
            .execute(
                "DELETE FROM memories WHERE session_id = ?1",
                rusqlite::params![sid],
            )
            .map_err(|e| LsError::Internal(format!("failed to clear session: {e}")))?;

        info!(session_id = %session_id, deleted = count, "session memories cleared");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    fn make_memory() -> MemorySQLite {
        MemorySQLite::in_memory().unwrap()
    }

    #[tokio::test]
    async fn test_write_and_search() {
        let memory = make_memory();
        let ctx = test_ctx();

        let item = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("今天天气很好".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };

        let id = memory.write(ctx.child(), item).await.unwrap();
        assert!(!id.is_nil());

        let result = memory.search(ctx.child(), "天气", 10).await.unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(
            result.items[0].content,
            Value::String("今天天气很好".into())
        );
    }

    #[tokio::test]
    async fn test_write_batch() {
        let memory = make_memory();
        let ctx = test_ctx();

        let items: Vec<MemoryItem> = (0..3)
            .map(|i| MemoryItem {
                memory_id: LsId::new(),
                session_id: ctx.session_id,
                content: Value::String(format!("记忆条目 {}", i)),
                metadata: HashMap::new(),
                created_at: chrono::Utc::now(),
                ttl_seconds: None,
            })
            .collect();

        let ids = memory.write_batch(ctx.child(), items).await.unwrap();
        assert_eq!(ids.len(), 3);

        let result = memory.search(ctx.child(), "记忆", 10).await.unwrap();
        assert_eq!(result.items.len(), 3);
    }

    #[tokio::test]
    async fn test_read() {
        let memory = make_memory();
        let ctx = test_ctx();

        let item = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("可读取的内容".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };

        let id = memory.write(ctx.child(), item).await.unwrap();
        let read_back = memory.read(ctx.child(), id).await.unwrap();
        assert_eq!(read_back.content, Value::String("可读取的内容".into()));
    }

    #[tokio::test]
    async fn test_delete() {
        let memory = make_memory();
        let ctx = test_ctx();

        let item = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("将被删除".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };

        let id = memory.write(ctx.child(), item).await.unwrap();
        memory.delete(ctx.child(), id).await.unwrap();

        let result = memory.search(ctx.child(), "删除", 10).await.unwrap();
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn test_clear_session() {
        let memory = make_memory();
        let ctx = test_ctx();

        let item = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("会话记忆".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };

        memory.write(ctx.child(), item).await.unwrap();
        memory
            .clear_session(ctx.child(), ctx.session_id)
            .await
            .unwrap();

        let result = memory.search(ctx.child(), "会话", 10).await.unwrap();
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn test_clean_expired() {
        let memory = make_memory();
        let ctx = test_ctx();

        let expired = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("过期记忆".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now() - chrono::Duration::hours(2),
            ttl_seconds: Some(1),
        };

        memory.write(ctx.child(), expired).await.unwrap();

        let valid = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("有效记忆".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: Some(3600),
        };

        memory.write(ctx.child(), valid).await.unwrap();

        let cleaned = memory.clean_expired(ctx.child()).await.unwrap();
        assert_eq!(cleaned, 1);

        let result = memory.search(ctx.child(), "记忆", 10).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].content, Value::String("有效记忆".into()));
    }

    #[tokio::test]
    async fn test_search_other_session_preserved() {
        let memory = make_memory();
        let ctx1 = test_ctx();
        let ctx2 = LsContext::with_session(LsId::new());

        let item1 = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx1.session_id,
            content: Value::String("会话1的内容".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };
        let item2 = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx2.session_id,
            content: Value::String("会话2的内容".into()),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            ttl_seconds: None,
        };

        let id1 = memory.write(ctx1.child(), item1).await.unwrap();
        let id2 = memory.write(ctx2.child(), item2).await.unwrap();

        memory
            .clear_session(ctx1.child(), ctx1.session_id)
            .await
            .unwrap();

        let read1 = memory.read(ctx1.child(), id1).await;
        assert!(read1.is_err(), "session1's memory should be deleted");

        let read2 = memory.read(ctx2.child(), id2).await;
        assert!(read2.is_ok(), "session2's memory should be preserved");
        assert_eq!(read2.unwrap().content, Value::String("会话2的内容".into()));

        let result = memory
            .search(ctx2.child(), "会话2的内容", 10)
            .await
            .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].content, Value::String("会话2的内容".into()));
    }
}
