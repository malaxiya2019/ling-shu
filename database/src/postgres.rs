//! PostgresDatabase — PostgreSQL 后端实现.
//!
//! 基于 `sqlx` 的异步实现，用于生产环境.
//!
//! # Feature
//! `postgres` (需显式启用)

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::database::{Database, PaginatedResult, Pagination, QueryFilter};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;

/// PostgreSQL 数据库后端.
#[derive(Debug, Clone)]
pub struct PostgresDatabase {
    pool: sqlx::PgPool,
}

impl PostgresDatabase {
    /// 从连接字符串创建连接池并自动执行迁移.
    ///
    /// # 参数
    /// - `database_url`: PostgreSQL 连接字符串 (如 `postgres://user:pass@localhost/lingshu`)
    /// - `max_connections`: 最大连接数
    pub async fn new(database_url: &str, max_connections: u32) -> LsResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| LsError::Internal(format!("postgres connect failed: {e}")))?;

        Self::run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    /// 从现有连接池创建 (用于测试).
    pub fn from_pool(pool: sqlx::PgPool) -> LsResult<Self> {
        Ok(Self { pool })
    }

    /// 执行迁移.
    async fn run_migrations(pool: &sqlx::PgPool) -> LsResult<()> {
        let sql = include_str!("migrations/001_init.sql");

        // SQLite 语法和 PostgreSQL 语法不完全兼容，需要转换一些关键词
        let pg_sql = sql
            .replace("TEXT PRIMARY KEY", "VARCHAR(64) PRIMARY KEY")
            .replace(
                "INTEGER NOT NULL DEFAULT 1",
                "BOOLEAN NOT NULL DEFAULT TRUE",
            )
            .replace(
                "INTEGER NOT NULL DEFAULT 0",
                "BOOLEAN NOT NULL DEFAULT FALSE",
            )
            .replace("INTEGER", "BIGINT")
            .replace("REAL DEFAULT 0.0", "DOUBLE PRECISION DEFAULT 0.0")
            .replace("BLOB", "BYTEA")
            .replace("datetime('now')", "NOW()")
            .replace(
                "TEXT NOT NULL DEFAULT (datetime('now'))",
                "TIMESTAMPTZ NOT NULL DEFAULT NOW()",
            )
            .replace(
                "TEXT NOT NULL DEFAULT (NOW())",
                "TIMESTAMPTZ NOT NULL DEFAULT NOW()",
            );

        // 按语句分割并逐条执行
        for statement in pg_sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            // 跳过 IF NOT EXISTS 创建表和索引
            sqlx::query(trimmed)
                .execute(pool)
                .await
                .map_err(|e| LsError::Internal(format!("postgres migration failed: {e}")))?;
        }

        tracing::info!("database: PostgreSQL migrations applied successfully");
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

fn row_to_value(row: &sqlx::postgres::PgRow) -> Value {
    let id: String = row.get("id");
    let payload: String = row.get("payload");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    let mut val = from_json(&payload);
    if let Some(obj) = val.as_object_mut() {
        obj.insert("id".into(), Value::String(id));
        obj.insert("created_at".into(), Value::String(created_at.to_rfc3339()));
        obj.insert("updated_at".into(), Value::String(updated_at.to_rfc3339()));
    }
    val
}

// ── Database trait 实现 ────────────────────────────

#[async_trait]
impl Database for PostgresDatabase {
    async fn insert(&self, _ctx: LsContext, collection: &str, data: Value) -> LsResult<Value> {
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO documents (id, collection, payload, created_at, updated_at) VALUES ($1, $2, $3::jsonb, NOW(), NOW())",
        )
        .bind(&id)
        .bind(collection)
        .bind(to_json(&data))
        .execute(&self.pool)
        .await
        .map_err(|e| LsError::Internal(format!("postgres insert failed: {e}")))?;

        let mut result = data;
        if let Some(obj) = result.as_object_mut() {
            obj.insert("id".into(), Value::String(id));
        }
        Ok(result)
    }

    async fn get_by_id(
        &self,
        _ctx: LsContext,
        collection: &str,
        id: &str,
    ) -> LsResult<Option<Value>> {
        let row = sqlx::query(
            "SELECT id, payload::text, created_at, updated_at FROM documents WHERE collection = $1 AND id = $2",
        )
        .bind(collection)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| LsError::Internal(format!("postgres get_by_id failed: {e}")))?;

        Ok(row.map(|r| row_to_value(&r)))
    }

    async fn query(
        &self,
        _ctx: LsContext,
        collection: &str,
        _filters: Vec<QueryFilter>,
        pagination: Pagination,
    ) -> LsResult<PaginatedResult> {
        let offset = (pagination.page.saturating_sub(1)) * pagination.page_size;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE collection = $1")
            .bind(collection)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("postgres count failed: {e}")))?;

        let total = total as u64;
        let total_pages = total.div_ceil(pagination.page_size.max(1));

        let rows = sqlx::query(
            "SELECT id, payload::text, created_at, updated_at FROM documents \
             WHERE collection = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(collection)
        .bind(pagination.page_size as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LsError::Internal(format!("postgres query failed: {e}")))?;

        let items: Vec<Value> = rows.iter().map(row_to_value).collect();

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
        let result = sqlx::query(
            "UPDATE documents SET payload = $1::jsonb, updated_at = NOW() WHERE collection = $2 AND id = $3",
        )
        .bind(to_json(&data))
        .bind(collection)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| LsError::Internal(format!("postgres update failed: {e}")))?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        let mut result = data;
        if let Some(obj) = result.as_object_mut() {
            obj.insert("id".into(), Value::String(id.to_string()));
        }
        Ok(Some(result))
    }

    async fn delete(&self, _ctx: LsContext, collection: &str, id: &str) -> LsResult<bool> {
        let result = sqlx::query("DELETE FROM documents WHERE collection = $1 AND id = $2")
            .bind(collection)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("postgres delete failed: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    async fn begin_transaction(&self, _ctx: LsContext) -> LsResult<String> {
        // sqlx::Transaction is handled via pool-level tx, but for simplicity
        // we use a savepoint-like approach via explicit BEGIN
        sqlx::query("BEGIN")
            .execute(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("postgres begin failed: {e}")))?;

        Ok("pg_txn".into())
    }

    async fn commit_transaction(&self, _ctx: LsContext, _txn_id: &str) -> LsResult<()> {
        sqlx::query("COMMIT")
            .execute(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("postgres commit failed: {e}")))?;

        Ok(())
    }

    async fn rollback_transaction(&self, _ctx: LsContext, _txn_id: &str) -> LsResult<()> {
        sqlx::query("ROLLBACK")
            .execute(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("postgres rollback failed: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "postgres")]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use serde_json::json;

    // 这些测试需要正在运行的 PostgreSQL 实例.
    // 设置环境变量 PG_TEST_URL 或使用默认的本地连接.
    fn get_test_url() -> String {
        std::env::var("DATABASE_URL")
            .or_else(|_| std::env::var("PG_TEST_URL"))
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/lingshu_test".into())
    }

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    async fn test_db() -> PostgresDatabase {
        let url = get_test_url();
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        // 清理所有迁移表 (CASCADE 确保级联删除依赖对象)
        let tables = [
            "documents",
            "users",
            "sessions",
            "memories",
            "vectors",
            "events",
            "audit_logs",
            "plugins",
        ];
        for table in &tables {
            let sql = format!("DROP TABLE IF EXISTS {table} CASCADE");
            let _ = sqlx::query(&sql).execute(&pool).await;
        }
        drop(pool);
        PostgresDatabase::new(&url, 2).await.unwrap()
    }

    #[tokio::test]
    #[serial_test::serial]
    #[ignore = "requires local PostgreSQL"]
    async fn test_insert_and_get() {
        let db = test_db().await;
        let ctx = test_ctx();
        let data = json!({"name": "pg_test_user"});
        let inserted = db.insert(ctx.child(), "users", data).await.unwrap();
        let id = inserted
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        let found = db.get_by_id(ctx.child(), "users", &id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().get("name").unwrap(), "pg_test_user");
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL"]
    #[serial_test::serial]
    async fn test_pagination() {
        let db = test_db().await;
        let ctx = test_ctx();
        for i in 0..5 {
            db.insert(ctx.child(), "items", json!({"index": i}))
                .await
                .unwrap();
        }
        let result = db
            .query(
                ctx,
                "items",
                vec![],
                Pagination {
                    page: 1,
                    page_size: 3,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.total, 5);
    }
}
