//! SqliteAuditLog — SQLite 持久化的审计日志存储.
//!
//! 将 `AuditEntry` 持久化到 SQLite 数据库，支持自动建表、
//! 按「查询条件」过滤，以及 WAL 并发模式。
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lingshu_audit::sqlite::SqliteAuditLog;
//!
//! let store = SqliteAuditLog::new("audit.db")?;
//! store.append(entry).await?;
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::{LsError, LsResult};
use rusqlite::params;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::log::{AuditEntry, AuditEventType, AuditLogStore};
use crate::query::AuditQuery;

/// SQLite 持久化的审计日志存储.
pub struct SqliteAuditLog {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteAuditLog {
    /// 打开或创建指定路径的 SQLite 审计数据库，自动建表。
    pub fn new(path: impl AsRef<Path>) -> LsResult<Self> {
        let conn = Self::open_and_init(path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 从已有连接创建（用于 `:memory:` 测试）。
    pub fn from_connection(conn: rusqlite::Connection) -> LsResult<Self> {
        Self::ensure_table(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 创建内存数据库（用于测试）。
    pub fn in_memory() -> LsResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| LsError::Internal(format!("sqlite in-memory open: {e}")))?;
        Self::from_connection(conn)
    }

    // ── 内部：打开 + 初始化表 ──

    fn open_and_init(path: impl AsRef<Path>) -> LsResult<rusqlite::Connection> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| LsError::Internal(format!("sqlite open failed: {e}")))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA synchronous=NORMAL;",
        )
        .map_err(|e| LsError::Internal(format!("sqlite pragma failed: {e}")))?;
        Self::ensure_table(&conn)?;
        Ok(conn)
    }

    fn ensure_table(conn: &rusqlite::Connection) -> LsResult<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_entries (
                id              TEXT PRIMARY KEY NOT NULL,
                timestamp       TEXT NOT NULL,
                event_type      TEXT NOT NULL,
                event_name      TEXT NOT NULL,
                actor           TEXT NOT NULL,
                resource_type   TEXT NOT NULL,
                resource_id     TEXT NOT NULL,
                detail          TEXT NOT NULL DEFAULT '{}',
                trace_id        TEXT,
                source          TEXT,
                result          TEXT NOT NULL DEFAULT 'success'
            );

            CREATE INDEX IF NOT EXISTS idx_ae_timestamp  ON audit_entries(timestamp);
            CREATE INDEX IF NOT EXISTS idx_ae_event_type ON audit_entries(event_type);
            CREATE INDEX IF NOT EXISTS idx_ae_actor      ON audit_entries(actor);
            CREATE INDEX IF NOT EXISTS idx_ae_result     ON audit_entries(result);
            CREATE INDEX IF NOT EXISTS idx_ae_resource   ON audit_entries(resource_type, resource_id);",
        )
        .map_err(|e| LsError::Internal(format!("create audit_entries table: {e}")))?;
        Ok(())
    }

    // ── 辅助：AuditEntry ↔ SQLite 行 ──


    fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<AuditEntry> {
        let id_str: String = row.get(0)?;
        let ts_str: String = row.get(1)?;
        let et_str: String = row.get(2)?;
        let event_type: AuditEventType = serde_json::from_str(&et_str)
            .unwrap_or(AuditEventType::Custom(et_str));

        Ok(AuditEntry {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::nil()),
            timestamp: DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            event_type,
            event_name: row.get(3)?,
            actor: row.get(4)?,
            resource_type: row.get(5)?,
            resource_id: row.get(6)?,
            detail: row.get(7)?,
            trace_id: row.get(8)?,
            source: row.get(9)?,
            result: row.get(10)?,
        })
    }
}

#[async_trait]
impl AuditLogStore for SqliteAuditLog {
    async fn append(&self, entry: AuditEntry) -> LsResult<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO audit_entries (id, timestamp, event_type, event_name, actor,
                                        resource_type, resource_id, detail,
                                        trace_id, source, result)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                entry.id.to_string(),
                entry.timestamp.to_rfc3339(),
                serde_json::to_value(&entry.event_type)
                    .map_err(|e| LsError::Internal(format!("serialize event_type: {e}")))?
                    .to_string(),
                entry.event_name,
                entry.actor,
                entry.resource_type,
                entry.resource_id,
                entry.detail,
                entry.trace_id,
                entry.source,
                entry.result,
            ],
        )
        .map_err(|e| LsError::Internal(format!("append audit entry: {e}")))?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> LsResult<AuditEntry> {
        let conn = self.conn.lock().await;
        let id_str = id.to_string();
        conn.query_row(
            "SELECT id, timestamp, event_type, event_name, actor,
                    resource_type, resource_id, detail,
                    trace_id, source, result
             FROM audit_entries WHERE id = ?1",
            params![id_str],
            Self::row_to_entry,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                LsError::NotFound(format!("audit entry {id}"))
            }
            other => LsError::Internal(format!("get_by_id: {other}")),
        })
    }

    async fn query(&self, q: &AuditQuery) -> LsResult<Vec<AuditEntry>> {
        let conn = self.conn.lock().await;

        let mut sql = String::from(
            "SELECT id, timestamp, event_type, event_name, actor,
                    resource_type, resource_id, detail,
                    trace_id, source, result
             FROM audit_entries WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref actor) = q.actor {
            sql.push_str(" AND actor = ?");
            param_values.push(Box::new(actor.clone()));
        }
        if let Some(ref event_type) = q.event_type {
            sql.push_str(" AND event_type = ?");
            let et_str =
                serde_json::to_value(event_type).map(|v| v.to_string()).unwrap_or_default();
            param_values.push(Box::new(et_str));
        }
        if let Some(ref event_name) = q.event_name {
            sql.push_str(" AND event_name = ?");
            param_values.push(Box::new(event_name.clone()));
        }
        if let Some(ref resource_type) = q.resource_type {
            sql.push_str(" AND resource_type = ?");
            param_values.push(Box::new(resource_type.clone()));
        }
        if let Some(ref resource_id) = q.resource_id {
            sql.push_str(" AND resource_id = ?");
            param_values.push(Box::new(resource_id.clone()));
        }
        if let Some(ref start) = q.start_time {
            sql.push_str(" AND timestamp >= ?");
            param_values.push(Box::new(start.to_rfc3339()));
        }
        if let Some(ref end) = q.end_time {
            sql.push_str(" AND timestamp <= ?");
            param_values.push(Box::new(end.to_rfc3339()));
        }
        if let Some(ref result) = q.result {
            sql.push_str(" AND result = ?");
            param_values.push(Box::new(result.clone()));
        }

        // 按时间降序
        sql.push_str(" ORDER BY timestamp DESC");

        // 分页
        let limit = q.limit.unwrap_or(100);
        let offset = q.offset.unwrap_or(0);
        sql.push_str(&format!(" LIMIT {limit} OFFSET {offset}"));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|v| v.as_ref()).collect();

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| LsError::Internal(format!("query prepare: {e}")))?;

        let rows = stmt
            .query_map(params_refs.as_slice(), Self::row_to_entry)
            .map_err(|e| LsError::Internal(format!("query execute: {e}")))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(
                row.map_err(|e| LsError::Internal(format!("query row: {e}")))?,
            );
        }
        Ok(entries)
    }

    async fn count(&self, q: &AuditQuery) -> LsResult<u64> {
        let conn = self.conn.lock().await;

        let mut sql = String::from("SELECT COUNT(*) FROM audit_entries WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref actor) = q.actor {
            sql.push_str(" AND actor = ?");
            param_values.push(Box::new(actor.clone()));
        }
        if let Some(ref event_type) = q.event_type {
            sql.push_str(" AND event_type = ?");
            let et_str =
                serde_json::to_value(event_type).map(|v| v.to_string()).unwrap_or_default();
            param_values.push(Box::new(et_str));
        }
        if let Some(ref event_name) = q.event_name {
            sql.push_str(" AND event_name = ?");
            param_values.push(Box::new(event_name.clone()));
        }
        if let Some(ref resource_type) = q.resource_type {
            sql.push_str(" AND resource_type = ?");
            param_values.push(Box::new(resource_type.clone()));
        }
        if let Some(ref resource_id) = q.resource_id {
            sql.push_str(" AND resource_id = ?");
            param_values.push(Box::new(resource_id.clone()));
        }
        if let Some(ref start) = q.start_time {
            sql.push_str(" AND timestamp >= ?");
            param_values.push(Box::new(start.to_rfc3339()));
        }
        if let Some(ref end) = q.end_time {
            sql.push_str(" AND timestamp <= ?");
            param_values.push(Box::new(end.to_rfc3339()));
        }
        if let Some(ref result) = q.result {
            sql.push_str(" AND result = ?");
            param_values.push(Box::new(result.clone()));
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|v| v.as_ref()).collect();

        conn.query_row(&sql, params_refs.as_slice(), |row| row.get::<_, i64>(0))
            .map(|c| c as u64)
            .map_err(|e| LsError::Internal(format!("count query: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::AuditEventType;
    use crate::query::AuditQueryBuilder;

    fn test_store() -> SqliteAuditLog {
        SqliteAuditLog::in_memory().unwrap()
    }

    fn make_entry(event_type: AuditEventType, event_name: &str, actor: &str) -> AuditEntry {
        AuditEntry::new(event_type, event_name, actor, "test", "res-1", r#"{}"#)
    }

    #[tokio::test]
    async fn test_append_and_query_all() {
        let store = test_store();
        store
            .append(make_entry(AuditEventType::UserLogin, "user.login", "alice"))
            .await
            .unwrap();

        let q = AuditQueryBuilder::new().build();
        let results = store.query(&q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, "alice");
    }

    #[tokio::test]
    async fn test_get_by_id() {
        let store = test_store();
        let entry = make_entry(AuditEventType::System, "system.startup", "system");
        let id = entry.id;
        store.append(entry).await.unwrap();

        let found = store.get_by_id(id).await.unwrap();
        assert_eq!(found.id, id);
    }

    #[tokio::test]
    async fn test_not_found() {
        let store = test_store();
        let result = store.get_by_id(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filter_by_event_type() {
        let store = test_store();
        store
            .append(make_entry(AuditEventType::UserLogin, "user.login", "alice"))
            .await
            .unwrap();
        store
            .append(make_entry(
                AuditEventType::AdminAction,
                "admin.delete",
                "admin",
            ))
            .await
            .unwrap();

        let q = AuditQueryBuilder::new()
            .with_event_type(AuditEventType::AdminAction)
            .build();
        let results = store.query(&q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_name, "admin.delete");
    }

    #[tokio::test]
    async fn test_filter_by_actor() {
        let store = test_store();
        store
            .append(make_entry(AuditEventType::UserLogin, "login", "alice"))
            .await
            .unwrap();
        store
            .append(make_entry(AuditEventType::UserLogin, "login", "bob"))
            .await
            .unwrap();

        let q = AuditQueryBuilder::new().with_actor("bob").build();
        let results = store.query(&q).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_filter_by_result() {
        let store = test_store();
        let mut e1 = make_entry(AuditEventType::UserLogin, "login", "alice");
        e1.result = "success".into();
        let mut e2 = make_entry(AuditEventType::UserLogin, "login", "alice");
        e2.result = "failure".into();
        store.append(e1).await.unwrap();
        store.append(e2).await.unwrap();

        let q = AuditQueryBuilder::new()
            .with_result("failure")
            .build();
        let results = store.query(&q).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_pagination() {
        let store = test_store();
        for i in 0..10 {
            let entry = AuditEntry::new(
                AuditEventType::System,
                &format!("event.{i}"),
                "system",
                "test",
                &format!("res-{i}"),
                "{}",
            );
            store.append(entry).await.unwrap();
        }

        let q = AuditQueryBuilder::new().with_limit(3).build();
        let results = store.query(&q).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_count() {
        let store = test_store();
        for i in 0..5 {
            let entry = make_entry(AuditEventType::System, &format!("ev.{i}"), "system");
            store.append(entry).await.unwrap();
        }

        let q = AuditQueryBuilder::new().build();
        let count = store.count(&q).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_order_by_time_desc() {
        use std::time::Duration;
        let store = test_store();
        let entry1 = make_entry(AuditEventType::System, "first", "system");
        store.append(entry1).await.unwrap();
        // small delay for different timestamps
        tokio::time::sleep(Duration::from_millis(10)).await;
        let entry2 = make_entry(AuditEventType::System, "second", "system");
        store.append(entry2).await.unwrap();

        let q = AuditQueryBuilder::new().build();
        let results = store.query(&q).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].event_name, "second");
        assert_eq!(results[1].event_name, "first");
    }

    #[tokio::test]
    async fn test_persistence_across_instances() {
        // 测试数据实际写入磁盘文件
        let dir = std::env::temp_dir();
        let db_path = dir.join(format!("test_audit_{}.db", Uuid::new_v4()));

        let id;
        {
            let store = SqliteAuditLog::new(&db_path).unwrap();
            let entry = make_entry(AuditEventType::UserLogin, "persist_test", "alice");
            id = entry.id;
            store.append(entry).await.unwrap();
        }

        // 新实例读取同一文件
        {
            let store = SqliteAuditLog::new(&db_path).unwrap();
            let found = store.get_by_id(id).await.unwrap();
            assert_eq!(found.actor, "alice");
        }

        // 清理
        let _ = std::fs::remove_file(&db_path);
        let wal = db_path.with_extension("db-wal");
        let _ = std::fs::remove_file(wal);
    }
}
