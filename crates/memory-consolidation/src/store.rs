//! ConsolidatedMemoryRepository — 巩固记忆持久化层。
//!
//! 基于已有 EpisodeRepository 实现的 ConsolidatedMemory 存储，
//! 将 ConsolidatedMemory 序列化为 Episode 存入底层存储。
//!
//! # 设计
//!
//! 每个 ConsolidatedMemory 被存储为一个带有 "consolidated" 标签的 Episode：
//! - title / summary 直接映射
//! - 实体信息映射为 Episode.entities
//! - consolidation 元数据存储在 Episode.metadata HashMap 中
//!
//! 这样不需要引入新的 SQLite 依赖，复用已有 episode 存储体系。

use crate::types::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::LsResult;
use lingshu_memory_episode::{Episode, EpisodeQuery, EpisodeRepository};
use std::sync::Arc;

// ─── ConsolidatedMemoryRepository trait ─────────────────

/// 巩固记忆持久化接口。
#[async_trait]
pub trait ConsolidatedMemoryRepository: Send + Sync {
    /// 存储一条巩固记忆。
    async fn store_consolidated(&self, memory: &ConsolidatedMemory) -> LsResult<()>;

    /// 按 ID 查找巩固记忆。
    async fn get_consolidated(&self, id: &str) -> LsResult<Option<ConsolidatedMemory>>;

    /// 列出巩固记忆（分页）。
    async fn list_consolidated(&self, limit: usize, offset: usize) -> LsResult<Vec<ConsolidatedMemory>>;

    /// 按策略类型列出巩固记忆。
    async fn list_by_strategy(&self, strategy: &str, limit: usize, offset: usize) -> LsResult<Vec<ConsolidatedMemory>>;

    /// 按重要性级别列出巩固记忆（需要 ImportanceScorer）。
    async fn list_by_importance(
        &self,
        min_score: f64,
        limit: usize,
        offset: usize,
        scorer: &crate::importance::ImportanceScorer,
    ) -> LsResult<Vec<ConsolidatedMemory>>;

    /// 统计巩固记忆总数。
    async fn count_consolidated(&self) -> LsResult<usize>;

    /// 删除一条巩固记忆。
    async fn delete_consolidated(&self, id: &str) -> LsResult<bool>;

    /// 归档一条巩固记忆（软删除）。
    ///
    /// 为记忆添加归档标签，而非物理删除。
    /// 后续可以通过移除归档标签恢复。
    async fn archive_consolidated(&self, id: &str, archive_tag: &str) -> LsResult<bool>;
}

// ─── EpisodeBackedConsolidatedStore ─────────────────────

/// 基于 EpisodeRepository 的 ConsolidatedMemory 存储实现。
///
/// 将 ConsolidatedMemory 序列化为带 "consolidated" 标签的 Episode
/// 存入已有的 Episode 存储体系（InMemory 或 SQLite）。
pub struct EpisodeBackedConsolidatedStore {
    store: Arc<dyn EpisodeRepository>,
}

impl EpisodeBackedConsolidatedStore {
    pub fn new(store: Arc<dyn EpisodeRepository>) -> Self {
        Self { store }
    }

    /// 将 ConsolidatedMemory 转换为 Episode。
    fn memory_to_episode(memory: &ConsolidatedMemory) -> Episode {
        let mut episode = Episode::new(&memory.title, &memory.summary, memory.created_at)
            .with_tag("consolidated")
            .with_tag(&memory.strategy)
            .with_metadata("consolidated_id", &memory.id)
            .with_metadata("consolidated_strategy", &memory.strategy)
            .with_metadata("consolidated_confidence", memory.confidence.to_string())
            .with_metadata(
                "consolidated_source_ids",
                memory.source_episode_ids.join(","),
            );

        // 时间跨度
        if let Some(start) = memory.time_span_start {
            episode = episode.with_metadata("time_span_start", start.to_rfc3339());
        }
        if let Some(end) = memory.time_span_end {
            episode = episode.with_metadata("time_span_end", end.to_rfc3339());
        }

        // 实体
        for entity in &memory.entities {
            episode = episode.with_entity(entity.clone());
        }

        // 标签（排除内部标签）
        for tag in &memory.tags {
            if tag != "consolidated" {
                episode = episode.with_tag(tag);
            }
        }

        episode
    }

    /// 将 Episode 转换回 ConsolidatedMemory。
    fn episode_to_memory(episode: &Episode) -> Option<ConsolidatedMemory> {
        if !episode.tags.contains(&"consolidated".to_string()) {
            return None;
        }

        let id = episode
            .metadata
            .get("consolidated_id")
            .cloned()
            .unwrap_or_else(|| episode.id.to_string());

        let strategy = episode
            .metadata
            .get("consolidated_strategy")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let confidence = episode
            .metadata
            .get("consolidated_confidence")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.8);

        let source_ids: Vec<String> = episode
            .metadata
            .get("consolidated_source_ids")
            .map(|v| {
                if v.is_empty() {
                    Vec::new()
                } else {
                    v.split(',').map(|s| s.to_string()).collect()
                }
            })
            .unwrap_or_default();

        let time_span_start = episode
            .metadata
            .get("time_span_start")
            .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let time_span_end = episode
            .metadata
            .get("time_span_end")
            .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // 使用 ConsolidationMemory 构造 + 手动设置 id
        let mut memory = ConsolidatedMemory::new(&episode.title, &episode.summary, &strategy)
            .with_confidence(confidence);

        memory.id = id;
        memory.source_episode_ids = source_ids;
        memory.time_span_start = time_span_start;
        memory.time_span_end = time_span_end;
        memory.created_at = episode.timestamp;

        for entity in &episode.entities {
            memory = memory.with_entity(entity.clone());
        }
        for tag in &episode.tags {
            if tag != "consolidated" && tag != &strategy {
                memory = memory.with_tag(tag);
            }
        }

        Some(memory)
    }
}

#[async_trait]
impl ConsolidatedMemoryRepository for EpisodeBackedConsolidatedStore {
    async fn store_consolidated(&self, memory: &ConsolidatedMemory) -> LsResult<()> {
        let episode = Self::memory_to_episode(memory);
        self.store.store(episode).await?;
        Ok(())
    }

    async fn get_consolidated(&self, id: &str) -> LsResult<Option<ConsolidatedMemory>> {
        // EpisodeRepository 默认按 title/entity 等查询，没有直接 ID 查询
        // 我们用 consolidated_id metadata + query 方式查找
        let query = EpisodeQuery::default().with_tag("consolidated").with_limit(1000);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        for ep in &episodes {
            if ep.metadata.get("consolidated_id").map(|s| s == id).unwrap_or(false) {
                return Ok(Self::episode_to_memory(ep));
            }
        }
        Ok(None)
    }

    async fn list_consolidated(&self, limit: usize, offset: usize) -> LsResult<Vec<ConsolidatedMemory>> {
        let query = EpisodeQuery::default().with_tag("consolidated").with_limit(limit + offset);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        Ok(episodes
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(Self::episode_to_memory)
            .collect())
    }

    async fn list_by_strategy(
        &self,
        strategy: &str,
        limit: usize,
        offset: usize,
    ) -> LsResult<Vec<ConsolidatedMemory>> {
        // 用 consolidated 标签查所有，再用 strategy 内存过滤
        // 因为 EpisodeQuery 的 tag 是 OR 逻辑
        let query = EpisodeQuery::default()
            .with_tag("consolidated")
            .with_limit(1000);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        Ok(episodes
            .iter()
            .filter(|ep| ep.tags.contains(&strategy.to_string()))
            .skip(offset)
            .take(limit)
            .filter_map(Self::episode_to_memory)
            .collect())
    }

    async fn list_by_importance(
        &self,
        min_score: f64,
        limit: usize,
        offset: usize,
        scorer: &crate::importance::ImportanceScorer,
    ) -> LsResult<Vec<ConsolidatedMemory>> {
        let query = EpisodeQuery::default().with_tag("consolidated").with_limit(1000);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        let mut memories: Vec<ConsolidatedMemory> = episodes
            .iter()
            .filter_map(Self::episode_to_memory)
            .filter(|m| scorer.score(m) >= min_score)
            .collect();

        // 按重要性降序排列
        scorer.sort_by_importance(&mut memories);

        Ok(memories.into_iter().skip(offset).take(limit).collect())
    }

    async fn count_consolidated(&self) -> LsResult<usize> {
        // 从 EpisodeRepository 查询 consolidated 标签的数量
        let query = EpisodeQuery::default().with_tag("consolidated").with_limit(10000);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
        Ok(episodes.len())
    }

    async fn delete_consolidated(&self, id: &str) -> LsResult<bool> {
        // 先找到匹配的 episode
        let query = EpisodeQuery::default().with_tag("consolidated").with_limit(10000);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        for ep in &episodes {
            if ep.metadata.get("consolidated_id").map(|s| s == id).unwrap_or(false) {
                // episode id 作为字符串
                self.store
                    .delete(ep.id)
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn archive_consolidated(&self, id: &str, archive_tag: &str) -> LsResult<bool> {
        let query = EpisodeQuery::default().with_tag("consolidated").with_limit(10000);
        let episodes = self
            .store
            .query(query)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        for ep in &episodes {
            if ep.metadata.get("consolidated_id").map(|s| s == id).unwrap_or(false) {
                let mut episode = ep.clone();
                episode = episode.with_tag(format!("archived:{}", archive_tag));
                self.store.store(episode).await
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

}

// ─── InMemoryConsolidatedStore ──────────────────────────

/// 内存版 ConsolidatedMemory 存储（用于测试）。
pub struct InMemoryConsolidatedStore {
    memories: tokio::sync::RwLock<Vec<ConsolidatedMemory>>,
}

impl InMemoryConsolidatedStore {
    pub fn new() -> Self {
        Self {
            memories: tokio::sync::RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryConsolidatedStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConsolidatedMemoryRepository for InMemoryConsolidatedStore {
    async fn store_consolidated(&self, memory: &ConsolidatedMemory) -> LsResult<()> {
        let mut store = self.memories.write().await;
        // 去重：相同的 id 替换
        if let Some(pos) = store.iter().position(|m| m.id == memory.id) {
            store[pos] = memory.clone();
        } else {
            store.push(memory.clone());
        }
        Ok(())
    }

    async fn get_consolidated(&self, id: &str) -> LsResult<Option<ConsolidatedMemory>> {
        let store = self.memories.read().await;
        Ok(store.iter().find(|m| m.id == id).cloned())
    }

    async fn list_consolidated(&self, limit: usize, offset: usize) -> LsResult<Vec<ConsolidatedMemory>> {
        let store = self.memories.read().await;
        Ok(store.iter().skip(offset).take(limit).cloned().collect())
    }

    async fn list_by_strategy(
        &self,
        strategy: &str,
        limit: usize,
        offset: usize,
    ) -> LsResult<Vec<ConsolidatedMemory>> {
        let store = self.memories.read().await;
        Ok(store
            .iter()
            .filter(|m| m.strategy == strategy)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect())
    }

    async fn list_by_importance(
        &self,
        min_score: f64,
        limit: usize,
        offset: usize,
        scorer: &crate::importance::ImportanceScorer,
    ) -> LsResult<Vec<ConsolidatedMemory>> {
        let store = self.memories.read().await;
        let mut memories: Vec<ConsolidatedMemory> = store
            .iter()
            .filter(|m| scorer.score(m) >= min_score)
            .cloned()
            .collect();
        scorer.sort_by_importance(&mut memories);
        Ok(memories.into_iter().skip(offset).take(limit).collect())
    }

    async fn count_consolidated(&self) -> LsResult<usize> {
        let store = self.memories.read().await;
        Ok(store.len())
    }

    async fn delete_consolidated(&self, id: &str) -> LsResult<bool> {
        let mut store = self.memories.write().await;
        if let Some(pos) = store.iter().position(|m| m.id == id) {
            store.remove(pos);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn archive_consolidated(&self, id: &str, archive_tag: &str) -> LsResult<bool> {
        let mut store = self.memories.write().await;
        if let Some(mem) = store.iter_mut().find(|m| m.id == id) {
            let tag = format!("archived:{}", archive_tag);
            if !mem.tags.contains(&tag) {
                mem.tags.push(tag);
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importance::ImportanceScorer;
    use lingshu_memory_episode::{EntityRef, InMemoryEpisodeStore};

    fn make_test_memory(id: &str, strategy: &str, days_ago: i64) -> ConsolidatedMemory {
        let time = Utc::now() - chrono::Duration::days(days_ago);
        let mut mem = ConsolidatedMemory::new(format!("测试 {}", id), format!("摘要 {}", id), strategy)
            .with_entity(EntityRef::new("project", "项目A"))
            .with_source("ep-001")
            .with_confidence(0.8);
        mem.id = id.to_string();
        mem.created_at = time;
        mem
    }

    #[tokio::test]
    async fn test_episode_store_roundtrip() {
        let episode_store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let store = EpisodeBackedConsolidatedStore::new(episode_store);

        let mem = make_test_memory("test-1", "summarize", 1);
        store.store_consolidated(&mem).await.unwrap();

        let found = store.get_consolidated("test-1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "测试 test-1");
    }

    #[tokio::test]
    async fn test_episode_store_list() {
        let episode_store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let store = EpisodeBackedConsolidatedStore::new(episode_store);

        store.store_consolidated(&make_test_memory("a", "summarize", 1)).await.unwrap();
        store.store_consolidated(&make_test_memory("b", "profile", 2)).await.unwrap();
        store.store_consolidated(&make_test_memory("c", "dedup", 3)).await.unwrap();

        let all = store.list_consolidated(10, 0).await.unwrap();
        assert_eq!(all.len(), 3);

        let summarized = store.list_by_strategy("summarize", 10, 0).await.unwrap();
        assert_eq!(summarized.len(), 1);
    }

    #[tokio::test]
    async fn test_episode_store_count_and_delete() {
        let episode_store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let store = EpisodeBackedConsolidatedStore::new(episode_store);

        store.store_consolidated(&make_test_memory("x", "summarize", 1)).await.unwrap();
        assert_eq!(store.count_consolidated().await.unwrap(), 1);

        assert!(store.delete_consolidated("x").await.unwrap());
        assert_eq!(store.count_consolidated().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_in_memory_store_basic() {
        let store = InMemoryConsolidatedStore::new();

        store.store_consolidated(&make_test_memory("m1", "summarize", 1)).await.unwrap();
        store.store_consolidated(&make_test_memory("m2", "profile", 2)).await.unwrap();

        assert_eq!(store.count_consolidated().await.unwrap(), 2);

        let found = store.get_consolidated("m1").await.unwrap();
        assert!(found.is_some());

        assert!(store.delete_consolidated("m1").await.unwrap());
        assert_eq!(store.count_consolidated().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_in_memory_list_by_importance() {
        let store = InMemoryConsolidatedStore::new();
        let scorer = ImportanceScorer::new();

        let mut recent = make_test_memory("recent", "profile", 0);
        recent.confidence = 0.99;

        store.store_consolidated(&recent).await.unwrap();
        store.store_consolidated(&make_test_memory("old", "dedup", 365)).await.unwrap();

        let important = store.list_by_importance(0.5, 10, 0, &scorer).await.unwrap();
        assert!(!important.is_empty(), "should have at least one important memory");
        // recent should be first
        assert_eq!(important[0].id, "recent");
    }

    #[tokio::test]
    async fn test_episode_store_by_importance() {
        let episode_store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let store = EpisodeBackedConsolidatedStore::new(episode_store);
        let scorer = ImportanceScorer::new();

        let mut recent = make_test_memory("important", "profile", 0);
        recent.confidence = 0.99;

        store.store_consolidated(&recent).await.unwrap();
        store.store_consolidated(&make_test_memory("old", "dedup", 365)).await.unwrap();

        let important = store.list_by_importance(0.3, 10, 0, &scorer).await.unwrap();
        assert!(!important.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_dedup_on_store() {
        let store = InMemoryConsolidatedStore::new();
        store.store_consolidated(&make_test_memory("same", "summarize", 1)).await.unwrap();
        store.store_consolidated(&make_test_memory("same", "summarize", 1)).await.unwrap();
        // same id should replace (dedup)
        assert_eq!(store.count_consolidated().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_episode_store_get_not_found() {
        let episode_store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let store = EpisodeBackedConsolidatedStore::new(episode_store);
        let found = store.get_consolidated("nonexistent").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_in_memory_strategy_filter() {
        let store = InMemoryConsolidatedStore::new();
        store.store_consolidated(&make_test_memory("a", "summarize", 1)).await.unwrap();
        store.store_consolidated(&make_test_memory("b", "profile", 1)).await.unwrap();
        store.store_consolidated(&make_test_memory("c", "summarize", 1)).await.unwrap();

        assert_eq!(store.list_by_strategy("summarize", 10, 0).await.unwrap().len(), 2);
        assert_eq!(store.list_by_strategy("profile", 10, 0).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_serialization_roundtrip() {
        let episode_store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let store = EpisodeBackedConsolidatedStore::new(episode_store);

        let mut mem = ConsolidatedMemory::new("完整测试", "完整序列化/反序列化测试", "profile")
            .with_entity(EntityRef::new("person", "张三"))
            .with_entity(EntityRef::new("project", "项目A"))
            .with_source("ep-001")
            .with_source("ep-002")
            .with_tag("test_tag")
            .with_confidence(0.85);
        mem.id = "complete-test".to_string();
        mem.created_at = Utc::now() - chrono::Duration::days(7);
        mem.time_span_start = Some(Utc::now() - chrono::Duration::days(10));
        mem.time_span_end = Some(Utc::now() - chrono::Duration::days(5));

        store.store_consolidated(&mem).await.unwrap();

        let loaded = store.get_consolidated("complete-test").await.unwrap().unwrap();
        assert_eq!(loaded.title, "完整测试");
        assert_eq!(loaded.entities.len(), 2);
        assert_eq!(loaded.source_episode_ids.len(), 2);
        assert!((loaded.confidence - 0.85).abs() < 0.01);
        assert!(loaded.time_span_start.is_some());
        assert!(loaded.time_span_end.is_some());
    }
}
