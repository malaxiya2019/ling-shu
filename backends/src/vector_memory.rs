//! VectorMemory — 基于 VectorStore + Embedding 的记忆实现.
//!
//! 将 `Memory` trait 桥接到 `VectorStore` 和 `Embedding`:
//! - `write`: 将 MemoryItem 转为 embedding → 存入 VectorStore
//! - `search`: 将查询文本转为 embedding → 相似度检索 → 返回 MemorySearchResult
//! - `read`: 直接按 ID 检索
//!
//! ## 端到端链路
//! ```text
//! Memory::search("query")
//!   → Embedding::embed(["query"])
//!   → VectorStore::search(collection, query_vector, top_k)
//!   → MemorySearchResult
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::embedding::{Embedding, EmbeddingRequest};
use lingshu_traits::memory::{Memory, MemoryItem, MemorySearchResult};
use lingshu_traits::vector_store::{VectorRecord, VectorSearchResult, VectorStore};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use tracing::{debug, info};

/// VectorMemory 的集合命名常量.
const MEMORY_COLLECTION_NAME: &str = "ls_memory";

/// 将 Memory 桥接到 VectorStore + Embedding 的实现.
pub struct VectorMemory {
    store: Box<dyn VectorStore>,
    embedder: Box<dyn Embedding>,
    /// 自动创建的集合 ID (延迟初始化).
    collection_id: Mutex<Option<LsId>>,
    /// 是否已初始化.
    initialized: AtomicBool,
}

impl VectorMemory {
    /// 创建 VectorMemory 实例.
    pub fn new(store: Box<dyn VectorStore>, embedder: Box<dyn Embedding>) -> Self {
        Self {
            store,
            embedder,
            collection_id: Mutex::new(None),
            initialized: AtomicBool::new(false),
        }
    }

    /// 获取或创建集合.
    async fn ensure_collection(&self, ctx: &LsContext) -> LsResult<LsId> {
        let mut lock = self.collection_id.lock().await;
        if let Some(id) = *lock {
            return Ok(id);
        }

        let dimensions = self.embedder.dimensions();
        debug!(
            trace_id = %ctx.trace_id,
            collection = MEMORY_COLLECTION_NAME,
            dimensions = dimensions,
            "creating memory collection"
        );
        let id = self
            .store
            .create_collection(ctx.child(), MEMORY_COLLECTION_NAME, dimensions)
            .await?;

        *lock = Some(id);
        self.initialized.store(true, Ordering::Release);
        info!(
            trace_id = %ctx.trace_id,
            collection_id = %id,
            dimensions = dimensions,
            "memory collection initialized"
        );
        Ok(id)
    }

    /// 将 MemoryItem 转为 VectorRecord (不含向量).
    fn item_to_record(item: &MemoryItem) -> LsResult<VectorRecord> {
        let content = match &item.content {
            Value::String(s) => s.clone(),
            other => {
                serde_json::to_string(other).map_err(|e| LsError::Serialization(e.to_string()))?
            }
        };

        let mut meta = serde_json::json!({
            "memory_id": item.memory_id.to_string(),
            "session_id": item.session_id.to_string(),
            "content": content,
            "content_type": "text",
            "created_at": item.created_at.to_rfc3339(),
        });

        if let Some(meta_obj) = meta.as_object_mut() {
            for (k, v) in &item.metadata {
                meta_obj.insert(k.clone(), Value::String(v.clone()));
            }
        }

        Ok(VectorRecord {
            id: item.memory_id,
            vector: vec![],
            metadata: meta,
            score: None,
        })
    }

    /// 将 VectorSearchResult 转为 MemorySearchResult.
    fn search_result_to_memory(result: VectorSearchResult) -> MemorySearchResult {
        let items: Vec<MemoryItem> = result
            .records
            .into_iter()
            .map(|rec| {
                let memory_id = rec.id;
                let session_id = rec
                    .metadata
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(LsId::nil());

                let content = rec
                    .metadata
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| Value::String(s.to_string()))
                    .unwrap_or(Value::Null);

                let created_at = rec
                    .metadata
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                let mut metadata = HashMap::new();
                if let Some(obj) = rec.metadata.as_object() {
                    for (k, v) in obj {
                        if k != "memory_id"
                            && k != "session_id"
                            && k != "content"
                            && k != "content_type"
                            && k != "created_at"
                        {
                            if let Some(s) = v.as_str() {
                                metadata.insert(k.clone(), s.to_string());
                            }
                        }
                    }
                }

                MemoryItem {
                    memory_id,
                    session_id,
                    content,
                    metadata,
                    created_at,
                    ttl_seconds: None,
                }
            })
            .collect();

        let total = result.total;
        MemorySearchResult { items, total }
    }
}

#[async_trait]
impl Memory for VectorMemory {
    async fn write(&self, ctx: LsContext, item: MemoryItem) -> LsResult<LsId> {
        let col_id = self.ensure_collection(&ctx).await?;

        let content = match &item.content {
            Value::String(s) => s.clone(),
            other => {
                serde_json::to_string(other).map_err(|e| LsError::Serialization(e.to_string()))?
            }
        };

        let embed_resp = self
            .embedder
            .embed(
                ctx.child(),
                EmbeddingRequest {
                    input: vec![content],
                    model: None,
                },
            )
            .await?;

        let vector = embed_resp
            .vectors
            .into_iter()
            .next()
            .ok_or_else(|| LsError::Internal("embedding returned empty result".into()))?
            .values;

        let mut record = Self::item_to_record(&item)?;
        record.vector = vector;

        self.store.upsert(ctx.child(), col_id, vec![record]).await?;
        Ok(item.memory_id)
    }

    async fn write_batch(&self, ctx: LsContext, items: Vec<MemoryItem>) -> LsResult<Vec<LsId>> {
        let mut ids = Vec::with_capacity(items.len());
        for item in items {
            ids.push(self.write(ctx.child(), item).await?);
        }
        Ok(ids)
    }

    async fn read(&self, ctx: LsContext, memory_id: LsId) -> LsResult<MemoryItem> {
        let col_id = self.ensure_collection(&ctx).await?;
        let dimensions = self.embedder.dimensions();
        let dummy = vec![0.0; dimensions];

        let result = self.store.search(ctx.child(), col_id, dummy, 10000).await?;
        let record = result
            .records
            .into_iter()
            .find(|r| r.id == memory_id)
            .ok_or_else(|| LsError::NotFound(format!("memory {memory_id}")))?;

        let items = Self::search_result_to_memory(VectorSearchResult {
            records: vec![record],
            total: 1,
        });

        items
            .items
            .into_iter()
            .next()
            .ok_or_else(|| LsError::Internal("conversion returned empty".into()))
    }

    async fn search(
        &self,
        ctx: LsContext,
        query: &str,
        limit: u64,
    ) -> LsResult<MemorySearchResult> {
        let col_id = self.ensure_collection(&ctx).await?;

        let embed_resp = self
            .embedder
            .embed(
                ctx.child(),
                EmbeddingRequest {
                    input: vec![query.to_string()],
                    model: None,
                },
            )
            .await?;

        let query_vector = embed_resp
            .vectors
            .into_iter()
            .next()
            .ok_or_else(|| LsError::Internal("embedding returned empty result".into()))?
            .values;

        let result = self
            .store
            .search(ctx.child(), col_id, query_vector, limit)
            .await?;
        Ok(Self::search_result_to_memory(result))
    }

    async fn delete(&self, ctx: LsContext, memory_id: LsId) -> LsResult<()> {
        let col_id = self.ensure_collection(&ctx).await?;
        let dimensions = self.embedder.dimensions();
        let dummy = vec![0.0; dimensions];

        let result = self.store.search(ctx.child(), col_id, dummy, 10000).await?;

        // 过滤掉要删除的记录
        let remaining: Vec<VectorRecord> = result
            .records
            .into_iter()
            .filter(|r| r.id != memory_id)
            .collect();

        // 重建集合: 删除 + 重新创建 + 写入剩余记录
        let name = MEMORY_COLLECTION_NAME;
        self.store.delete_collection(ctx.child(), col_id).await?;
        let new_id = self
            .store
            .create_collection(ctx.child(), name, dimensions)
            .await?;

        if !remaining.is_empty() {
            self.store.upsert(ctx.child(), new_id, remaining).await?;
        }

        // 更新缓存的集合 ID
        let mut lock = self.collection_id.lock().await;
        *lock = Some(new_id);

        Ok(())
    }

    async fn clean_expired(&self, _ctx: LsContext) -> LsResult<u64> {
        // 当前 VectorStore trait 不支持按时间过滤删除
        Ok(0)
    }

    async fn clear_session(&self, ctx: LsContext, session_id: LsId) -> LsResult<()> {
        let col_id = self.ensure_collection(&ctx).await?;
        let dimensions = self.embedder.dimensions();
        let dummy = vec![0.0; dimensions];

        let result = self.store.search(ctx.child(), col_id, dummy, 10000).await?;

        let remaining: Vec<VectorRecord> = result
            .records
            .into_iter()
            .filter(|r| {
                r.metadata
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<LsId>().ok())
                    != Some(session_id)
            })
            .collect();

        // 重建集合
        let name = MEMORY_COLLECTION_NAME;
        self.store.delete_collection(ctx.child(), col_id).await?;
        let new_id = self
            .store
            .create_collection(ctx.child(), name, dimensions)
            .await?;

        if !remaining.is_empty() {
            self.store.upsert(ctx.child(), new_id, remaining).await?;
        }

        let mut lock = self.collection_id.lock().await;
        *lock = Some(new_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryVectorStore;

    /// 模拟 Embedder: 将文本转为固定的 4 维向量.
    struct MockEmbedder;

    #[async_trait]
    impl Embedding for MockEmbedder {
        async fn embed(
            &self,
            _ctx: LsContext,
            request: EmbeddingRequest,
        ) -> LsResult<lingshu_traits::embedding::EmbeddingResponse> {
            Ok(lingshu_traits::embedding::EmbeddingResponse {
                vectors: request
                    .input
                    .iter()
                    .map(|text| {
                        let hash: u64 = text
                            .bytes()
                            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
                        let values: Vec<f32> = (0..4)
                            .map(|i| ((hash >> (i * 16)) & 0xFFFF) as f32 / 65536.0)
                            .collect();
                        lingshu_traits::embedding::EmbeddingVector {
                            dimensions: 4,
                            values,
                        }
                    })
                    .collect(),
                model: "mock-embedding".into(),
                usage: lingshu_traits::embedding::EmbeddingUsage { total_tokens: 0 },
            })
        }

        fn validate_dimensions(
            &self,
            vector: &lingshu_traits::embedding::EmbeddingVector,
        ) -> LsResult<()> {
            if vector.dimensions != 4 {
                return Err(LsError::InvalidArgument(format!(
                    "expected 4 dimensions, got {}",
                    vector.dimensions
                )));
            }
            Ok(())
        }

        fn dimensions(&self) -> usize {
            4
        }
    }

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    fn make_memory() -> VectorMemory {
        VectorMemory::new(Box::new(InMemoryVectorStore::new()), Box::new(MockEmbedder))
    }

    #[tokio::test]
    async fn test_write_and_search() {
        let memory = make_memory();
        let ctx = test_ctx();

        let item = MemoryItem {
            memory_id: LsId::new(),
            session_id: ctx.session_id,
            content: Value::String("今天天气很好".into()),
            metadata: [("source".into(), "chat".into())].into(),
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
    async fn test_search_other_session_preserved() {
        let memory = make_memory();
        let ctx1 = test_ctx();
        let ctx2 = LsContext::with_session(LsId::new());

        // 写入两条属于不同会话的记忆
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

        // 清除会话1
        memory
            .clear_session(ctx1.child(), ctx1.session_id)
            .await
            .unwrap();

        // 按 ID 验证：会话1的记录应无法读取
        let read1 = memory.read(ctx1.child(), id1).await;
        assert!(read1.is_err(), "session1's memory should be deleted");

        // 会话2的记录应可正常读取
        let read2 = memory.read(ctx2.child(), id2).await;
        assert!(read2.is_ok(), "session2's memory should be preserved");
        assert_eq!(read2.unwrap().content, Value::String("会话2的内容".into()));

        // 搜索仍能找到会话2的内容（集合中还有1条记录）
        let result = memory
            .search(ctx2.child(), "会话2的内容", 10)
            .await
            .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].content, Value::String("会话2的内容".into()));
    }
}
