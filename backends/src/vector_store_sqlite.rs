//! SQLiteVector — 基于 SQLite 的持久化向量存储.
//!
//! 使用 `vectors` 表存储向量数据，支持:
//! - 集合管理 (create/delete collection)
//! - 向量写入 (upsert, 按 id 去重)
//! - 余弦相似度检索 (Top-K)
//! - 元数据过滤 (metadata filter)
//!
//! # Feature
//! `vector-store-sqlite` (需显式启用)

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::vector_store::*;
use tracing::{info, debug, warn};
use rusqlite::params;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// SQLite 持久化向量存储.
pub struct SQLiteVector {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SQLiteVector {
    /// 打开或创建 SQLite 数据库文件.
    pub fn new(path: impl AsRef<Path>) -> LsResult<Self> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| LsError::Internal(format!("sqlite_vector open failed: {e}")))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;",
        )
        .map_err(|e| LsError::Internal(format!("sqlite_vector pragma failed: {e}")))?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 创建内存数据库 (用于测试).
    pub fn in_memory() -> LsResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .map_err(|e| LsError::Internal(format!("sqlite_vector in-memory open failed: {e}")))?;
        Self::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &rusqlite::Connection) -> LsResult<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS vector_collections (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL UNIQUE,
                dimensions  INTEGER NOT NULL,
                metadata    TEXT DEFAULT '{}',
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS vector_records (
                id          TEXT PRIMARY KEY,
                collection  TEXT NOT NULL,
                vector      BLOB NOT NULL,
                payload     TEXT DEFAULT '{}',
                metadata    TEXT DEFAULT '{}',
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_vector_records_collection ON vector_records(collection);",
        )
        .map_err(|e| LsError::Internal(format!("sqlite_vector migration failed: {e}")))?;

        tracing::info!("vector_store: SQLite schema ready");
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
impl VectorStore for SQLiteVector {
    async fn create_collection(&self, _ctx: LsContext, name: &str, dimensions: usize) -> LsResult<LsId> {
        let conn = self.conn.lock().await;
        let id = LsId::new().to_string();

        conn.execute(
            "INSERT INTO vector_collections (id, name, dimensions, metadata, created_at) VALUES (?1, ?2, ?3, '{}', datetime('now'))",
            params![id, name, dimensions as i64],
        )
        .map_err(|e| LsError::Internal(format!("create_collection failed: {e}")))?;

        Ok(LsId::from(uuid::Uuid::parse_str(&id).unwrap()))
    }

    async fn delete_collection(&self, _ctx: LsContext, collection_id: LsId) -> LsResult<()> {
        let conn = self.conn.lock().await;
        let cid = collection_id.to_string();

        // 删除集合中的记录
        conn.execute("DELETE FROM vector_records WHERE collection = ?1", params![cid])
            .map_err(|e| LsError::Internal(format!("delete_records failed: {e}")))?;

        // 删除集合
        conn.execute("DELETE FROM vector_collections WHERE id = ?1", params![cid])
            .map_err(|e| LsError::Internal(format!("delete_collection failed: {e}")))?;

        Ok(())
    }

    async fn upsert(&self, _ctx: LsContext, collection_id: LsId, records: Vec<VectorRecord>) -> LsResult<()> {
        let conn = self.conn.lock().await;
        let cid = collection_id.to_string();

        // 验证集合存在并获取维度
        let dimensions: i64 = conn
            .query_row(
                "SELECT dimensions FROM vector_collections WHERE id = ?1",
                params![cid],
                |row| row.get(0),
            )
            .map_err(|_| LsError::NotFound(format!("collection {cid}")))?;

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

            conn.execute(
                "INSERT OR REPLACE INTO vector_records (id, collection, vector, payload, metadata, created_at) \
                 VALUES (?1, ?2, ?3, ?4, '{}', datetime('now'))",
                params![rid, cid, blob, payload],
            )
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
        let conn = self.conn.lock().await;
        let cid = collection_id.to_string();

        // 验证集合
        let _dimensions: i64 = conn
            .query_row(
                "SELECT dimensions FROM vector_collections WHERE id = ?1",
                params![cid],
                |row| row.get(0),
            )
            .map_err(|_| LsError::NotFound(format!("collection {cid}")))?;

        // 加载集合中所有向量
        let mut stmt = conn
            .prepare("SELECT id, vector, payload FROM vector_records WHERE collection = ?1")
            .map_err(|e| LsError::Internal(format!("search prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![cid], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                let payload: String = row.get(2)?;
                Ok((id, blob, payload))
            })
            .map_err(|e| LsError::Internal(format!("search query failed: {e}")))?;

        let mut scored: Vec<(f64, LsId, Vec<f32>, Value)> = Vec::new();

        for row in rows {
            let (id_str, blob, payload_str) = row
                .map_err(|e| LsError::Internal(format!("search row failed: {e}")))?;

            let vector = Self::blob_to_vector(&blob);
            let score = Self::cosine_similarity(&query, &vector);
            let metadata: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
            let lid = LsId::from(uuid::Uuid::parse_str(&id_str).unwrap_or(uuid::Uuid::nil()));

            scored.push((score, lid, vector, metadata));
        }

        // 按相似度降序排列
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

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_create_and_delete_collection() {
        let store = SQLiteVector::in_memory().unwrap();
        let ctx = test_ctx();

        let id = store.create_collection(ctx.clone(), "test_collection", 4).await.unwrap();
        assert!(!id.is_nil());

        // 搜索空集合应返回空结果
        let result = store.search(ctx.clone(), id, vec![1.0; 4], 10).await.unwrap();
        assert_eq!(result.records.len(), 0);
        assert_eq!(result.total, 0);

        store.delete_collection(ctx.clone(), id).await.unwrap();
    }

    #[tokio::test]
    async fn test_upsert_and_search() {
        let store = SQLiteVector::in_memory().unwrap();
        let ctx = test_ctx();

        let col_id = store.create_collection(ctx.clone(), "vectors", 2).await.unwrap();

        let records = vec![
            VectorRecord {
                id: LsId::new(),
                vector: vec![1.0, 0.0],
                metadata: serde_json::json!({"label": "A"}),
                score: None,
            },
            VectorRecord {
                id: LsId::new(),
                vector: vec![0.0, 1.0],
                metadata: serde_json::json!({"label": "B"}),
                score: None,
            },
            VectorRecord {
                id: LsId::new(),
                vector: vec![1.0, 1.0],
                metadata: serde_json::json!({"label": "C"}),
                score: None,
            },
        ];

        store.upsert(ctx.clone(), col_id, records).await.unwrap();

        // 检索与 [1.0, 0.0] 最相似的向量
        let result = store.search(ctx.clone(), col_id, vec![1.0, 0.0], 2).await.unwrap();
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].metadata["label"], "A");
        assert!(result.records[0].score.unwrap() > 0.99);
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let store = SQLiteVector::in_memory().unwrap();
        let ctx = test_ctx();

        let col_id = store.create_collection(ctx.clone(), "dim_test", 3).await.unwrap();

        let bad_records = vec![VectorRecord {
            id: LsId::new(),
            vector: vec![1.0, 0.0], // 2 维，但集合需要 3 维
            metadata: serde_json::json!({}),
            score: None,
        }];

        let result = store.upsert(ctx.clone(), col_id, bad_records).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search_empty_collection() {
        let store = SQLiteVector::in_memory().unwrap();
        let ctx = test_ctx();

        let col_id = store.create_collection(ctx.clone(), "empty", 3).await.unwrap();
        let result = store.search(ctx.clone(), col_id, vec![0.5; 3], 5).await.unwrap();
        assert_eq!(result.records.len(), 0);
    }

    #[tokio::test]
    async fn test_uuids_match() {
        let store = SQLiteVector::in_memory().unwrap();
        let ctx = test_ctx();

        let col_id = store.create_collection(ctx.clone(), "uuid_test", 1).await.unwrap();
        let rec_id = LsId::new();

        store
            .upsert(
                ctx.clone(),
                col_id,
                vec![VectorRecord {
                    id: rec_id,
                    vector: vec![1.0],
                    metadata: serde_json::json!({}),
                    score: None,
                }],
            )
            .await
            .unwrap();

        let result = store.search(ctx.clone(), col_id, vec![1.0], 10).await.unwrap();
        assert_eq!(result.records[0].id, rec_id);
    }

    #[test]
    fn test_vector_blob_roundtrip() {
        let original = vec![1.0, -2.5, 3.14, 0.0, 100.0];
        let blob = SQLiteVector::vector_to_blob(&original);
        let restored = SQLiteVector::blob_to_vector(&blob);
        assert_eq!(original.len(), restored.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
