//! PgVector — 基于 PostgreSQL 的持久化向量存储.
//!
//! 使用 `vector_records` 表存储向量数据，支持:
//! - 集合管理 (create/delete collection)
//! - 向量写入 (upsert, 按 id 去重)
//! - 余弦相似度检索 (Top-K)
//!
//! 在生产环境中建议配合 pgvector 扩展使用原生 `<=>` 运算符提升性能。
//!
//! # Feature
//! `vector-store-pg` (需显式启用)

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::vector_store::*;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;

/// PostgreSQL 持久化向量存储.
#[derive(Debug, Clone)]
pub struct PgVector {
    pool: sqlx::PgPool,
}

impl PgVector {
    /// 从连接字符串创建连接池并自动执行迁移.
    pub async fn new(database_url: &str, max_connections: u32) -> LsResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| LsError::Internal(format!("pgvector connect failed: {e}")))?;

        Self::run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    async fn run_migrations(pool: &sqlx::PgPool) -> LsResult<()> {
        // 创建 pgvector 扩展 (如果可用)
        let _ = sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(pool)
            .await;

        // 向量存储表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS vector_collections (
                id          VARCHAR(64) PRIMARY KEY,
                name        VARCHAR(255) NOT NULL UNIQUE,
                dimensions  BIGINT NOT NULL,
                metadata    JSONB DEFAULT '{}',
                created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| LsError::Internal(format!("pgvector migration failed: {e}")))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS vector_records (
                id          VARCHAR(64) PRIMARY KEY,
                collection  VARCHAR(64) NOT NULL REFERENCES vector_collections(id) ON DELETE CASCADE,
                vector      BYTEA NOT NULL,
                payload     JSONB DEFAULT '{}',
                metadata    JSONB DEFAULT '{}',
                created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| LsError::Internal(format!("pgvector migration failed: {e}")))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_vector_records_collection_pg ON vector_records(collection)",
        )
        .execute(pool)
        .await
        .map_err(|e| LsError::Internal(format!("pgvector index failed: {e}")))?;

        tracing::info!("vector_store: PostgreSQL schema ready");
        Ok(())
    }

    /// 将 f32 向量序列化为字节 BLOB.
    fn vector_to_blob(v: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(v.len() * 4);
        for val in v {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes
    }

    /// 从字节 BLOB 反序列化 f32 向量.
    fn blob_to_vector(blob: &[u8]) -> Vec<f32> {
        blob.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// 余弦相似度.
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot / (norm_a * norm_b)) as f64
    }
}

#[async_trait]
impl VectorStore for PgVector {
    async fn create_collection(
        &self,
        _ctx: LsContext,
        name: &str,
        dimensions: usize,
    ) -> LsResult<LsId> {
        let id = LsId::new().to_string();

        sqlx::query(
            "INSERT INTO vector_collections (id, name, dimensions, metadata) VALUES ($1, $2, $3, '{}')",
        )
        .bind(&id)
        .bind(name)
        .bind(dimensions as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| LsError::Internal(format!("create_collection failed: {e}")))?;

        Ok(LsId::from(uuid::Uuid::parse_str(&id).unwrap()))
    }

    async fn delete_collection(&self, _ctx: LsContext, collection_id: LsId) -> LsResult<()> {
        let cid = collection_id.to_string();

        // 级联删除记录 (由 ON DELETE CASCADE 处理)
        sqlx::query("DELETE FROM vector_collections WHERE id = $1")
            .bind(&cid)
            .execute(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("delete_collection failed: {e}")))?;

        Ok(())
    }

    async fn upsert(
        &self,
        _ctx: LsContext,
        collection_id: LsId,
        records: Vec<VectorRecord>,
    ) -> LsResult<()> {
        let cid = collection_id.to_string();

        // 验证集合存在并获取维度
        let dimensions: i64 =
            sqlx::query_scalar("SELECT dimensions FROM vector_collections WHERE id = $1")
                .bind(&cid)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| LsError::Internal(format!("get dimensions failed: {e}")))?
                .ok_or_else(|| LsError::NotFound(format!("collection {cid}")))?;

        for rec in &records {
            if rec.vector.len() != dimensions as usize {
                return Err(LsError::InvalidArgument(format!(
                    "vector dimension {} != expected {}",
                    rec.vector.len(),
                    dimensions
                )));
            }

            let blob = Self::vector_to_blob(&rec.vector);
            let payload = serde_json::to_string(&rec.metadata).unwrap_or_else(|_| "{}".into());
            let rid = rec.id.to_string();

            sqlx::query(
                "INSERT INTO vector_records (id, collection, vector, payload, metadata) \
                 VALUES ($1, $2, $3, $4::jsonb, '{}') \
                 ON CONFLICT (id) DO UPDATE SET vector = $3, payload = $4::jsonb",
            )
            .bind(&rid)
            .bind(&cid)
            .bind(&blob)
            .bind(&payload)
            .execute(&self.pool)
            .await
            .map_err(|e| LsError::Internal(format!("upsert failed: {e}")))?;
        }

        Ok(())
    }

    async fn search(
        &self,
        _ctx: LsContext,
        collection_id: LsId,
        query: Vec<f32>,
        top_k: u64,
    ) -> LsResult<VectorSearchResult> {
        let cid = collection_id.to_string();

        // 验证集合
        let _dimensions: i64 =
            sqlx::query_scalar("SELECT dimensions FROM vector_collections WHERE id = $1")
                .bind(&cid)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| LsError::Internal(format!("get dimensions failed: {e}")))?
                .ok_or_else(|| LsError::NotFound(format!("collection {cid}")))?;

        // 加载集合中所有向量
        let rows: Vec<(String, Vec<u8>, String)> = sqlx::query_as(
            "SELECT id, vector, payload::text FROM vector_records WHERE collection = $1",
        )
        .bind(&cid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LsError::Internal(format!("search failed: {e}")))?;

        let mut scored: Vec<(f64, LsId, Vec<f32>, Value)> = Vec::new();

        for (id_str, blob, payload_str) in rows {
            let vector = Self::blob_to_vector(&blob);
            let score = Self::cosine_similarity(&query, &vector);
            let metadata: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
            let lid = LsId::from(uuid::Uuid::parse_str(&id_str).unwrap_or(uuid::Uuid::nil()));

            scored.push((score, lid, vector, metadata));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let total = scored.len() as u64;
        let records: Vec<VectorRecord> = scored
            .into_iter()
            .take(top_k as usize)
            .map(|(score, id, vector, metadata)| VectorRecord {
                id,
                vector,
                metadata,
                score: Some(score),
            })
            .collect();

        Ok(VectorSearchResult { records, total })
    }
}
