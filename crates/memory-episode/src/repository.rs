//! EpisodeRepository trait — 事件存储抽象接口。

use async_trait::async_trait;
use lingshu_core::LsResult;

use crate::{Episode, EpisodeId, EpisodeQuery, QueryStats};

/// EpisodeRepository — 事件存储抽象。
///
/// 所有 Episode 存储后端（内存 / SQLite / PostgreSQL）实现此 trait。
///
/// # 设计原则
///
/// - 接口极简：只做 CRUD + 按维度查询
/// - 不要泛型：用 Box<dyn> 而非 <T: Store>
/// - 不要事务：第一版不做跨事件一致性保证
#[async_trait]
pub trait EpisodeRepository: Send + Sync {
    /// 存储一个新事件。
    async fn store(&self, episode: Episode) -> LsResult<EpisodeId>;

    /// 批量存储事件。
    async fn store_batch(&self, episodes: Vec<Episode>) -> LsResult<Vec<EpisodeId>>;

    /// 通过 ID 获取事件。
    async fn get(&self, id: EpisodeId) -> LsResult<Episode>;

    /// 查询事件列表。
    async fn query(&self, query: EpisodeQuery) -> LsResult<Vec<Episode>>;

    /// 查询事件并返回统计信息。
    async fn query_with_stats(&self, query: EpisodeQuery) -> LsResult<(Vec<Episode>, QueryStats)>;

    /// 按实体删除事件。
    async fn delete_by_entity(&self, entity: &str) -> LsResult<usize>;

    /// 删除单个事件。
    async fn delete(&self, id: EpisodeId) -> LsResult<bool>;

    /// 获取事件总数。
    async fn count(&self) -> LsResult<usize>;

    /// 获取所有已知实体名称。
    async fn list_entities(&self) -> LsResult<Vec<String>>;

    /// 清空所有事件。
    async fn clear(&self) -> LsResult<()>;
}

// ─── Arc<T> 的 EpisodeRepository 实现 ──────────────────

#[async_trait]
impl<T: EpisodeRepository + ?Sized> EpisodeRepository for std::sync::Arc<T> {
    async fn store(&self, episode: Episode) -> LsResult<EpisodeId> {
        (**self).store(episode).await
    }

    async fn store_batch(&self, episodes: Vec<Episode>) -> LsResult<Vec<EpisodeId>> {
        (**self).store_batch(episodes).await
    }

    async fn get(&self, id: EpisodeId) -> LsResult<Episode> {
        (**self).get(id).await
    }

    async fn query(&self, query: EpisodeQuery) -> LsResult<Vec<Episode>> {
        (**self).query(query).await
    }

    async fn query_with_stats(&self, query: EpisodeQuery) -> LsResult<(Vec<Episode>, QueryStats)> {
        (**self).query_with_stats(query).await
    }

    async fn delete_by_entity(&self, entity: &str) -> LsResult<usize> {
        (**self).delete_by_entity(entity).await
    }

    async fn delete(&self, id: EpisodeId) -> LsResult<bool> {
        (**self).delete(id).await
    }

    async fn count(&self) -> LsResult<usize> {
        (**self).count().await
    }

    async fn list_entities(&self) -> LsResult<Vec<String>> {
        (**self).list_entities().await
    }

    async fn clear(&self) -> LsResult<()> {
        (**self).clear().await
    }
}
