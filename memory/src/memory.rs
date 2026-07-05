//! Memory — Unified memory system combining short-term buffer and long-term vector storage

use async_trait::async_trait;
use crate::buffer::ChatBuffer;
use crate::types::{MemoryConfig, MemoryItem, MemoryQuery, MemoryResult};
use crate::vector::VectorMemory;
use lingshu_core::{LsContext, LsResult};
use std::sync::Arc;
use tracing::debug;

/// The unified Memory trait
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a memory item (goes to buffer, optionally to vector store)
    async fn store(&self, ctx: &LsContext, item: MemoryItem) -> LsResult<()>;

    /// Store a simple chat message
    async fn store_message(&self, ctx: &LsContext, role: &str, content: &str) -> LsResult<()>;

    /// Retrieve recent conversation history
    async fn recall(&self, ctx: &LsContext, query: &MemoryQuery) -> LsResult<MemoryResult>;

    /// Search long-term memory semantically
    async fn search(&self, ctx: &LsContext, query: &MemoryQuery) -> LsResult<MemoryResult>;

    /// Clear all memory for a session
    async fn clear_session(&self, ctx: &LsContext, session_id: &str) -> LsResult<()>;
}

// ── Implementation ──────────────────────────────────

/// Default memory implementation
pub struct DefaultMemory {
    buffer: ChatBuffer,
    vector_store: Option<Arc<dyn VectorMemory>>,
    #[allow(dead_code)]
    config: MemoryConfig,
}

impl DefaultMemory {
    pub fn new(session_id: &str, config: MemoryConfig) -> Self {
        Self {
            buffer: ChatBuffer::new(session_id, config.buffer_capacity),
            vector_store: None,
            config,
        }
    }

    pub fn with_vector_store(mut self, store: Arc<dyn VectorMemory>) -> Self {
        self.vector_store = Some(store);
        self
    }

    pub fn buffer(&self) -> &ChatBuffer {
        &self.buffer
    }
}

#[async_trait]
impl Memory for DefaultMemory {
    async fn store(&self, _ctx: &LsContext, item: MemoryItem) -> LsResult<()> {
        self.buffer.add(item.clone()).await;

        // Also store in vector if available
        if let Some(vs) = &self.vector_store {
            // Use empty embedding — real embedding would come from an embedding model
            let _ = vs.store(item, vec![]).await;
        }

        debug!("Memory stored (buffer size: {})", self.buffer.len().await);
        Ok(())
    }

    async fn store_message(&self, ctx: &LsContext, role: &str, content: &str) -> LsResult<()> {
        let item = MemoryItem::new(&self.buffer.capacity().to_string(), role, content);
        self.store(ctx, item).await
    }

    async fn recall(&self, _ctx: &LsContext, query: &MemoryQuery) -> LsResult<MemoryResult> {
        let start = std::time::Instant::now();
        let items = self.buffer.recent(query.limit).await;
        let elapsed = start.elapsed().as_millis() as u64;

        Ok(MemoryResult {
            items,
            total: self.buffer.len().await,
            query_time_ms: elapsed,
        })
    }

    async fn search(&self, _ctx: &LsContext, query: &MemoryQuery) -> LsResult<MemoryResult> {
        let start = std::time::Instant::now();

        match &self.vector_store {
            Some(vs) => {
                let records = vs.search_by_text(query, query.limit).await?;
                let elapsed = start.elapsed().as_millis() as u64;
                let items: Vec<MemoryItem> = records.into_iter().map(|r| r.item).collect();
                let total = items.len();
                Ok(MemoryResult {
                    items,
                    total,
                    query_time_ms: elapsed,
                })
            }
            None => {
                // Fallback to buffer keyword search
                let items = self.buffer.all().await;
                let query_lower = query.query.as_deref().unwrap_or("").to_lowercase();
                let filtered: Vec<MemoryItem> = if query_lower.is_empty() {
                    items
                } else {
                    items
                        .into_iter()
                        .filter(|i| i.content.to_lowercase().contains(&query_lower))
                        .collect()
                };
                let elapsed = start.elapsed().as_millis() as u64;
                let total = filtered.len();
                let items: Vec<MemoryItem> = filtered.into_iter().take(query.limit).collect();
                Ok(MemoryResult {
                    items,
                    total,
                    query_time_ms: elapsed,
                })
            }
        }
    }

    async fn clear_session(&self, _ctx: &LsContext, _session_id: &str) -> LsResult<()> {
        self.buffer.clear().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::InMemoryVectorStore;

    #[tokio::test]
    async fn test_store_and_recall() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mem = DefaultMemory::new("test-session", MemoryConfig::default());

        mem.store_message(&ctx, "user", "Hello").await.unwrap();
        mem.store_message(&ctx, "assistant", "Hi!").await.unwrap();

        let result = mem.recall(&ctx, &MemoryQuery::default()).await.unwrap();
        assert_eq!(result.total, 2);
    }

    #[tokio::test]
    async fn test_search_with_vector_store() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let vs = Arc::new(InMemoryVectorStore::new());
        let mem = DefaultMemory::new("test-session", MemoryConfig::default())
            .with_vector_store(vs);

        mem.store_message(&ctx, "user", "I like apples").await.unwrap();
        mem.store_message(&ctx, "user", "I like dogs").await.unwrap();

        let query = MemoryQuery {
            query: Some("apple".into()),
            limit: 10,
            ..Default::default()
        };
        let result = mem.search(&ctx, &query).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(result.items[0].content.contains("apple"));
    }
}
