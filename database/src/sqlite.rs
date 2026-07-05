//! SqliteDatabase — SQLite 后端实现.
//!
//! 使用 `rusqlite` 的异步包装, 支持 WAL 模式, 自动执行迁移.
//!
//! # Feature
//! `sqlite` (默认启用)

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::database::{Database, PaginatedResult, Pagination, QueryFilter};
use rusqlite::params;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// SQLite 数据库后端.
pub struct SqliteDatabase {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteDatabase {
    /// 创建或打开 SQLite 数据库, 并自动执行迁移.
    pub fn new(path: impl AsRef<Path>) -> LsResult<Self> {
        let conn = Self::open_and_migrate(path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 从现有连接创建 (用于 `:memory:` 测试).
    pub fn from_connection(conn: rusqlite::Connection) -> LsResult<Self> {
        Self::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 创建内存数据库 (用于测试).
    pub fn in_memory() -> LsResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| LsError::Internal(format!("sqlite in-memory open failed: {e}")))?;
        Self::from_connection(conn)
    }

    // ── 内部: 打开 + 迁移 (同步) ──

    fn open_and_migrate(path: impl AsRef<Path>) -> LsResult<rusqlite::Connection> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| LsError::Internal(format!("sqlite open failed: {e}")))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| LsError::Internal(format!("sqlite pragma failed: {e}")))?;
        Self::run_migrations(&conn)?;
        Ok(conn)
    }

    fn run_migrations(conn: &rusqlite::Connection) -> LsResult<()> {
        let sql = include_str!("migrations/001_init.sql");
        conn.execute_batch(sql)
            .map_err(|e| LsError::Internal(format!("migration failed: {e}")))?;
        tracing::info!("database: SQLite migrations applied successfully");
        Ok(())
    }
}

// ── 辅助函数 ───────────────────────────────────────

fn to_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

fn from_json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::Null)
}

/// 将数据库行 (payload, id, created_at, updated_at) 转为 Value.
fn row_to_value(row: &rusqlite::Row) -> rusqlite::Result<Value> {
    let id: String = row.get(0)?;
    let payload: String = row.get(1)?;
    let created_at: String = row.get(2)?;
    let updated_at: String = row.get(3)?;
    let mut val = from_json(&payload);
    if let Some(obj) = val.as_object_mut() {
        obj.insert("id".into(), Value::String(id));
        obj.insert("created_at".into(), Value::String(created_at));
        obj.insert("updated_at".into(), Value::String(updated_at));
    }
    Ok(val)
}

/// 统计集合中记录总数.
fn count_collection(conn: &rusqlite::Connection, collection: &str) -> rusqlite::Result<u64> {
    conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE collection = ?1",
        params![collection],
        |row| row.get(0),
    )
}

/// 分页查询集合.
fn query_collection(
    conn: &rusqlite::Connection,
    collection: &str,
    page: u64,
    page_size: u64,
) -> rusqlite::Result<Vec<Value>> {
    let offset = (page.saturating_sub(1)) * page_size;
    let mut stmt = conn.prepare(
        "SELECT id, payload, created_at, updated_at FROM documents \
         WHERE collection = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![collection, page_size, offset], row_to_value)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

// ── Database trait 实现 ────────────────────────────

#[async_trait]
impl Database for SqliteDatabase {
    async fn insert(&self, _ctx: LsContext, collection: &str, data: Value) -> LsResult<Value> {
        let conn = self.conn.lock().await;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO documents (id, collection, payload, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, collection, to_json(&data), now, now],
        )
        .map_err(|e| LsError::Internal(format!("insert failed: {e}")))?;

        let mut result = data;
        if let Some(obj) = result.as_object_mut() {
            obj.insert("id".into(), Value::String(id));
            obj.insert("created_at".into(), Value::String(now));
        }
        Ok(result)
    }

    async fn get_by_id(
        &self,
        _ctx: LsContext,
        collection: &str,
        id: &str,
    ) -> LsResult<Option<Value>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT id, payload, created_at, updated_at FROM documents WHERE collection = ?1 AND id = ?2",
            )
            .map_err(|e| LsError::Internal(format!("get_by_id prepare failed: {e}")))?;

        let result = stmt.query_row(params![collection, id], row_to_value);
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(LsError::Internal(format!("get_by_id failed: {e}"))),
        }
    }

    async fn query(
        &self,
        _ctx: LsContext,
        collection: &str,
        _filters: Vec<QueryFilter>,
        pagination: Pagination,
    ) -> LsResult<PaginatedResult> {
        let conn = self.conn.lock().await;
        let total = count_collection(&conn, collection)
            .map_err(|e| LsError::Internal(format!("query count failed: {e}")))?;

        let total_pages = if pagination.page_size > 0 {
            (total + pagination.page_size - 1) / pagination.page_size
        } else {
            1
        };

        let items = query_collection(&conn, collection, pagination.page, pagination.page_size)
            .map_err(|e| LsError::Internal(format!("query failed: {e}")))?;

        Ok(PaginatedResult {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
            total_pages,
        })
    }

    async fn update(
        &self,
        _ctx: LsContext,
        collection: &str,
        id: &str,
        data: Value,
    ) -> LsResult<Option<Value>> {
        let conn = self.conn.lock().await;
        let now = chrono::Utc::now().to_rfc3339();

        let affected = conn
            .execute(
                "UPDATE documents SET payload = ?1, updated_at = ?2 WHERE collection = ?3 AND id = ?4",
                params![to_json(&data), now, collection, id],
            )
            .map_err(|e| LsError::Internal(format!("update failed: {e}")))?;

        if affected == 0 {
            return Ok(None);
        }

        let mut result = data;
        if let Some(obj) = result.as_object_mut() {
            obj.insert("id".into(), Value::String(id.to_string()));
            obj.insert("updated_at".into(), Value::String(now));
        }
        Ok(Some(result))
    }

    async fn delete(&self, _ctx: LsContext, collection: &str, id: &str) -> LsResult<bool> {
        let conn = self.conn.lock().await;
        let affected = conn
            .execute(
                "DELETE FROM documents WHERE collection = ?1 AND id = ?2",
                params![collection, id],
            )
            .map_err(|e| LsError::Internal(format!("delete failed: {e}")))?;
        Ok(affected > 0)
    }

    async fn begin_transaction(&self, _ctx: LsContext) -> LsResult<String> {
        let conn = self.conn.lock().await;
        conn.execute("BEGIN TRANSACTION", [])
            .map_err(|e| LsError::Internal(format!("begin transaction failed: {e}")))?;
        Ok("txn_default".into())
    }

    async fn commit_transaction(&self, _ctx: LsContext, _txn_id: &str) -> LsResult<()> {
        let conn = self.conn.lock().await;
        conn.execute("COMMIT", [])
            .map_err(|e| LsError::Internal(format!("commit failed: {e}")))?;
        Ok(())
    }

    async fn rollback_transaction(&self, _ctx: LsContext, _txn_id: &str) -> LsResult<()> {
        let conn = self.conn.lock().await;
        conn.execute("ROLLBACK", [])
            .map_err(|e| LsError::Internal(format!("rollback failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use serde_json::json;

    fn test_db() -> SqliteDatabase {
        SqliteDatabase::in_memory().unwrap()
    }

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let db = test_db();
        let ctx = test_ctx();
        let data = json!({"name": "test_user", "email": "test@example.com"});
        let inserted = db.insert(ctx.child(), "users", data).await.unwrap();
        let id = inserted
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let found = db.get_by_id(ctx.child(), "users", &id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().get("name").unwrap(), "test_user");
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let db = test_db();
        let ctx = test_ctx();
        let result = db.get_by_id(ctx, "users", "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update() {
        let db = test_db();
        let ctx = test_ctx();
        let data = json!({"value": 1});
        let inserted = db.insert(ctx.child(), "test_items", data).await.unwrap();
        let id = inserted
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let updated = db
            .update(ctx.child(), "test_items", &id, json!({"value": 42}))
            .await
            .unwrap();
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().get("value").unwrap(), 42);
    }

    #[tokio::test]
    async fn test_delete() {
        let db = test_db();
        let ctx = test_ctx();
        let data = json!({"name": "to_delete"});
        let inserted = db.insert(ctx.child(), "test_items", data).await.unwrap();
        let id = inserted
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let deleted = db.delete(ctx.child(), "test_items", &id).await.unwrap();
        assert!(deleted);
        let found = db.get_by_id(ctx, "test_items", &id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_pagination() {
        let db = test_db();
        let ctx = test_ctx();
        for i in 0..10 {
            db.insert(ctx.child(), "test_items", json!({"index": i}))
                .await
                .unwrap();
        }
        let result = db
            .query(
                ctx,
                "test_items",
                vec![],
                Pagination {
                    page: 1,
                    page_size: 5,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.items.len(), 5);
        assert_eq!(result.total, 10);
        assert_eq!(result.total_pages, 2);
    }

    #[tokio::test]
    async fn test_transaction() {
        let db = test_db();
        let ctx = test_ctx();
        let txn_id = db.begin_transaction(ctx.child()).await.unwrap();
        db.insert(ctx.child(), "users", json!({"name": "txn_user"}))
            .await
            .unwrap();
        db.rollback_transaction(ctx.child(), &txn_id).await.unwrap();
        let result = db
            .query(
                ctx,
                "users",
                vec![],
                Pagination {
                    page: 1,
                    page_size: 10,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.total, 0);
    }

    #[tokio::test]
    async fn test_commit_transaction() {
        let db = test_db();
        let ctx = test_ctx();
        let txn_id = db.begin_transaction(ctx.child()).await.unwrap();
        db.insert(ctx.child(), "users", json!({"name": "committed"}))
            .await
            .unwrap();
        db.commit_transaction(ctx.child(), &txn_id).await.unwrap();
        let count = db
            .query(
                ctx,
                "users",
                vec![],
                Pagination {
                    page: 1,
                    page_size: 100,
                },
            )
            .await
            .unwrap();
        assert_eq!(count.total, 1);
    }
}
