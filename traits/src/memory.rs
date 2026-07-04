use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// 记忆条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub memory_id: LsId,
    pub session_id: LsId,
    pub content: Value,
    pub metadata: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ttl_seconds: Option<u64>,
}

/// 记忆检索结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub items: Vec<MemoryItem>,
    pub total: u64,
}

/// Memory — 记忆读写、检索、隔离与生命周期管理.
#[async_trait]
pub trait Memory: Send + Sync + 'static {
    /// 写入记忆.
    async fn write(&self, ctx: LsContext, item: MemoryItem) -> LsResult<LsId>;

    /// 批量写入记忆.
    async fn write_batch(&self, ctx: LsContext, items: Vec<MemoryItem>) -> LsResult<Vec<LsId>>;

    /// 按 ID 读取记忆.
    async fn read(&self, ctx: LsContext, memory_id: LsId) -> LsResult<MemoryItem>;

    /// 检索记忆（关键词/语义）.
    async fn search(&self, ctx: LsContext, query: &str, limit: u64) -> LsResult<MemorySearchResult>;

    /// 删除记忆.
    async fn delete(&self, ctx: LsContext, memory_id: LsId) -> LsResult<()>;

    /// 清理过期记忆.
    async fn clean_expired(&self, ctx: LsContext) -> LsResult<u64>;

    /// 清除会话全部记忆.
    async fn clear_session(&self, ctx: LsContext, session_id: LsId) -> LsResult<()>;
}

// ── Blanket impl: Box<dyn Memory> 也实现 Memory ────

#[async_trait]
impl<T: Memory + ?Sized> Memory for Box<T> {
    async fn write(&self, ctx: LsContext, item: MemoryItem) -> LsResult<LsId> {
        (**self).write(ctx, item).await
    }
    async fn write_batch(&self, ctx: LsContext, items: Vec<MemoryItem>) -> LsResult<Vec<LsId>> {
        (**self).write_batch(ctx, items).await
    }
    async fn read(&self, ctx: LsContext, memory_id: LsId) -> LsResult<MemoryItem> {
        (**self).read(ctx, memory_id).await
    }
    async fn search(&self, ctx: LsContext, query: &str, limit: u64) -> LsResult<MemorySearchResult> {
        (**self).search(ctx, query, limit).await
    }
    async fn delete(&self, ctx: LsContext, memory_id: LsId) -> LsResult<()> {
        (**self).delete(ctx, memory_id).await
    }
    async fn clean_expired(&self, ctx: LsContext) -> LsResult<u64> {
        (**self).clean_expired(ctx).await
    }
    async fn clear_session(&self, ctx: LsContext, session_id: LsId) -> LsResult<()> {
        (**self).clear_session(ctx, session_id).await
    }
}
