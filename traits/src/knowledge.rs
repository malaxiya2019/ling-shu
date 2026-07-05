use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// 知识条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub entry_id: LsId,
    pub source: String,
    pub content: Value,
    pub version: u64,
    pub metadata: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 数据源配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub source_id: LsId,
    pub name: String,
    pub source_type: String,
    pub config: Value,
}

/// 知识检索结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub entries: Vec<KnowledgeEntry>,
    pub total: u64,
}

/// Knowledge — 数据源管理、知识条目检索、版本追溯.
#[async_trait]
pub trait Knowledge: Send + Sync + 'static {
    /// 注册数据源.
    async fn register_source(&self, ctx: LsContext, source: DataSource) -> LsResult<LsId>;

    /// 注销数据源.
    async fn unregister_source(&self, ctx: LsContext, source_id: LsId) -> LsResult<()>;

    /// 同步知识条目.
    async fn sync(&self, ctx: LsContext, source_id: LsId) -> LsResult<u64>;

    /// 检索知识.
    async fn search(
        &self,
        ctx: LsContext,
        query: &str,
        limit: u64,
    ) -> LsResult<KnowledgeSearchResult>;

    /// 按 ID 获取知识条目.
    async fn get_entry(&self, ctx: LsContext, entry_id: LsId) -> LsResult<KnowledgeEntry>;

    /// 获取指定条目的历史版本.
    async fn get_entry_history(
        &self,
        ctx: LsContext,
        entry_id: LsId,
    ) -> LsResult<Vec<KnowledgeEntry>>;
}
