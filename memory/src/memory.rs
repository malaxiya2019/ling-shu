//! Memory — Unified memory system combining short-term buffer and long-term vector storage
//!
//! v4.1 增强：新增记忆摘要（summarization）和记忆合并（consolidation）能力。

use crate::buffer::ChatBuffer;
use crate::consolidation::{ConsolidationResult, LongTermStore, MemoryConsolidator};
use crate::summarization::{MemorySummarizer, MemorySummary};
use crate::types::{MemoryConfig, MemoryItem, MemoryQuery, MemoryResult};
use crate::vector::VectorMemory;
use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

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

    /// Get the current summary (if any)
    async fn get_summary(&self) -> LsResult<Option<MemorySummary>>;

    /// Trigger memory summarization manually
    async fn trigger_summarization(&self, ctx: &LsContext) -> LsResult<Option<MemorySummary>>;

    /// Trigger memory consolidation manually
    async fn trigger_consolidation(&self, ctx: &LsContext) -> LsResult<ConsolidationResult>;
}

// ── Implementation ──────────────────────────────────

/// Default memory implementation with summarization and consolidation support
pub struct DefaultMemory {
    buffer: ChatBuffer,
    vector_store: Option<Arc<dyn VectorMemory>>,
    long_term_store: Option<Arc<dyn LongTermStore>>,
    summarizer: Option<Arc<MemorySummarizer>>,
    consolidator: Option<Arc<MemoryConsolidator>>,
    summary: Arc<RwLock<Option<MemorySummary>>>,
    #[allow(dead_code)]
    config: MemoryConfig,
    session_id: String,
}

impl DefaultMemory {
    pub fn new(session_id: &str, config: MemoryConfig) -> Self {
        Self {
            buffer: ChatBuffer::new(session_id, config.buffer_capacity),
            vector_store: None,
            long_term_store: None,
            summarizer: None,
            consolidator: None,
            summary: Arc::new(RwLock::new(None)),
            config,
            session_id: session_id.to_string(),
        }
    }

    pub fn with_vector_store(mut self, store: Arc<dyn VectorMemory>) -> Self {
        self.vector_store = Some(store);
        self
    }

    pub fn with_long_term_store(mut self, store: Arc<dyn LongTermStore>) -> Self {
        self.long_term_store = Some(store.clone());
        // 也设置到 consolidator
        if let Some(ref cons) = self.consolidator {
            let new_cons = MemoryConsolidator::new(cons.policy().clone()).with_store(store);
            self.consolidator = Some(Arc::new(new_cons));
        } else {
            self.consolidator = Some(Arc::new(MemoryConsolidator::default().with_store(store)));
        }
        self
    }

    pub fn with_summarizer(mut self, summarizer: Arc<MemorySummarizer>) -> Self {
        self.summarizer = Some(summarizer);
        self
    }

    pub fn with_consolidator(mut self, consolidator: Arc<MemoryConsolidator>) -> Self {
        self.consolidator = Some(consolidator);
        self
    }

    pub fn buffer(&self) -> &ChatBuffer {
        &self.buffer
    }

    /// 尝试自动摘要（如果条件满足）
    async fn try_auto_summarize(&self, ctx: &LsContext) {
        let summarizer = match &self.summarizer {
            Some(s) => s,
            None => return,
        };

        let buffer_len = self.buffer.len().await;
        if !summarizer.should_summarize(buffer_len) {
            return;
        }

        debug!("auto-summarize triggered: buffer_len={}", buffer_len);

        // 获取需要摘要的条目（除 keep_recent 之外的旧条目）
        let all_items = self.buffer.all().await;
        let keep = summarizer.config().keep_recent;
        let to_summarize: Vec<MemoryItem> = if all_items.len() > keep {
            all_items[..all_items.len() - keep].to_vec()
        } else {
            all_items.clone()
        };

        if to_summarize.is_empty() {
            return;
        }

        let prev_summary = self.summary.read().await.clone();

        match summarizer
            .summarize(ctx, &self.session_id, &to_summarize, prev_summary.as_ref())
            .await
        {
            Ok(Some(new_summary)) => {
                let mut summary = self.summary.write().await;
                *summary = Some(new_summary);
                debug!("auto-summarize complete");
            }
            Ok(None) => {}
            Err(e) => {
                warn!("auto-summarize failed: {}", e);
            }
        }
    }

    /// 尝试自动合并（如果条件满足）
    async fn try_auto_consolidate(&self, ctx: &LsContext) {
        let consolidator = match &self.consolidator {
            Some(c) => c,
            None => return,
        };

        let buffer_len = self.buffer.len().await;
        if !consolidator.should_consolidate(buffer_len).await {
            return;
        }

        let all_items = self.buffer.all().await;
        if let Err(e) = consolidator
            .consolidate(ctx, &self.session_id, &all_items)
            .await
        {
            warn!("auto-consolidate failed: {}", e);
        }
    }
}

#[async_trait]
impl Memory for DefaultMemory {
    async fn store(&self, ctx: &LsContext, item: MemoryItem) -> LsResult<()> {
        self.buffer.add(item.clone()).await;

        // 同步到向量存储（如果可用）
        if let Some(vs) = &self.vector_store {
            let _ = vs.store(item, vec![]).await;
        }

        debug!("Memory stored (buffer size: {})", self.buffer.len().await);

        // 触发自动摘要和合并
        self.try_auto_summarize(ctx).await;
        self.try_auto_consolidate(ctx).await;

        Ok(())
    }

    async fn store_message(&self, ctx: &LsContext, role: &str, content: &str) -> LsResult<()> {
        let item = MemoryItem::new(&self.session_id, role, content);
        self.store(ctx, item).await
    }

    async fn recall(&self, _ctx: &LsContext, query: &MemoryQuery) -> LsResult<MemoryResult> {
        let start = std::time::Instant::now();
        let mut items = self.buffer.recent(query.limit).await;
        let total = self.buffer.len().await;

        // 如果有摘要，在开头注入摘要信息
        if let Some(ref summary) = *self.summary.read().await {
            let summary_item = MemoryItem {
                id: format!("summary:{}", summary.id),
                session_id: self.session_id.clone(),
                role: "system".to_string(),
                content: format!(
                    "[会话摘要 - {} 条历史记录]\n{}",
                    summary.original_count, summary.summary
                ),
                timestamp: summary.created_at,
                metadata: serde_json::json!({
                    "type": "summary",
                    "original_count": summary.original_count,
                }),
            };
            items.insert(0, summary_item);
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(MemoryResult {
            items,
            total,
            query_time_ms: elapsed,
        })
    }

    async fn search(&self, _ctx: &LsContext, query: &MemoryQuery) -> LsResult<MemoryResult> {
        let start = std::time::Instant::now();

        // 优先从长期存储搜索
        if let Some(ref store) = self.long_term_store {
            let query_text = query.query.as_deref().unwrap_or("");
            let results = store
                .search(_ctx, &self.session_id, query_text, query.limit)
                .await?;
            let elapsed = start.elapsed().as_millis() as u64;
            let total = results.len();
            return Ok(MemoryResult {
                items: results,
                total,
                query_time_ms: elapsed,
            });
        }

        // 回退到向量存储搜索
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
                // 最终回退到缓冲关键词搜索
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

        // 清除摘要
        {
            let mut summary = self.summary.write().await;
            *summary = None;
        }

        // 清除长期存储
        if let Some(ref store) = self.long_term_store {
            let _ = store.clear_session(_ctx, _session_id).await;
        }

        Ok(())
    }

    async fn get_summary(&self) -> LsResult<Option<MemorySummary>> {
        Ok(self.summary.read().await.clone())
    }

    async fn trigger_summarization(&self, ctx: &LsContext) -> LsResult<Option<MemorySummary>> {
        let summarizer = match &self.summarizer {
            Some(s) => s,
            None => return Ok(None),
        };

        let all_items = self.buffer.all().await;
        let prev_summary = self.summary.read().await.clone();

        let result = summarizer
            .summarize(ctx, &self.session_id, &all_items, prev_summary.as_ref())
            .await?;

        if let Some(ref new_summary) = result {
            let mut summary = self.summary.write().await;
            *summary = Some(new_summary.clone());
        }

        Ok(result)
    }

    async fn trigger_consolidation(&self, ctx: &LsContext) -> LsResult<ConsolidationResult> {
        let consolidator = match &self.consolidator {
            Some(c) => c,
            None => {
                return Ok(ConsolidationResult {
                    consolidated: 0,
                    skipped: self.buffer.len().await,
                    errors: 0,
                });
            }
        };

        let all_items = self.buffer.all().await;
        consolidator
            .consolidate(ctx, &self.session_id, &all_items)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarization::SummarizerLlm;
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
        let mem = DefaultMemory::new("test-session", MemoryConfig::default()).with_vector_store(vs);

        mem.store_message(&ctx, "user", "I like apples")
            .await
            .unwrap();
        mem.store_message(&ctx, "user", "I like dogs")
            .await
            .unwrap();

        let query = MemoryQuery {
            query: Some("apple".into()),
            limit: 10,
            ..Default::default()
        };
        let result = mem.search(&ctx, &query).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(result.items[0].content.contains("apple"));
    }

    #[tokio::test]
    async fn test_get_summary_empty() {
        let _ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mem = DefaultMemory::new("test-session", MemoryConfig::default());
        let summary = mem.get_summary().await.unwrap();
        assert!(summary.is_none());
    }

    #[tokio::test]
    async fn test_trigger_summarization_no_llm() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mem = DefaultMemory::new("test-session", MemoryConfig::default());
        mem.store_message(&ctx, "user", "Hello").await.unwrap();
        let result = mem.trigger_summarization(&ctx).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_clear_session() {
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mem = DefaultMemory::new("test-session", MemoryConfig::default());
        mem.store_message(&ctx, "user", "Hello").await.unwrap();
        mem.store_message(&ctx, "assistant", "Hi!").await.unwrap();

        mem.clear_session(&ctx, "test-session").await.unwrap();
        assert!(mem.buffer.is_empty().await);
        let summary = mem.get_summary().await.unwrap();
        assert!(summary.is_none());
    }

    #[tokio::test]
    async fn test_recall_with_summary() {
        struct MockLlm;
        #[async_trait]
        impl SummarizerLlm for MockLlm {
            async fn generate(
                &self,
                _ctx: &LsContext,
                _prompt: &str,
                _model: &str,
            ) -> LsResult<String> {
                Ok("**摘要**：模拟摘要内容。".to_string())
            }
        }

        let summarizer = Arc::new(MemorySummarizer::default().with_llm(Arc::new(MockLlm)));

        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let mem = DefaultMemory::new(
            "test-session",
            MemoryConfig {
                buffer_capacity: 10,
                ..Default::default()
            },
        )
        .with_summarizer(summarizer);

        mem.store_message(&ctx, "user", "今天天气怎么样？")
            .await
            .unwrap();
        mem.store_message(&ctx, "assistant", "今天天气很好。")
            .await
            .unwrap();

        // 手动触发摘要
        mem.trigger_summarization(&ctx).await.unwrap();

        // recall 时应该包含摘要条目
        let result = mem.recall(&ctx, &MemoryQuery::default()).await.unwrap();
        assert!(result.items.len() >= 2);
        // 第一条应该是摘要
        assert!(
            result.items[0].content.contains("会话摘要")
                || result.items[0].content.contains("摘要")
        );
    }
}
