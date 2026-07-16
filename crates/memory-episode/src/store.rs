//! InMemoryEpisodeStore — 内存级事件存储实现。

use async_trait::async_trait;
use lingshu_core::{LsError, LsResult};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::{
    Episode, EpisodeId, EpisodeQuery, QueryStats, SortOrder,
};

/// InMemoryEpisodeStore — 无持久化的内存事件存储。
///
/// 用于开发、测试和 single-node 场景。
///
/// # 线程安全
///
/// 内部使用 `Arc<RwLock<HashMap>>`，支持多线程并发读写。
#[derive(Debug, Clone)]
pub struct InMemoryEpisodeStore {
    episodes: Arc<RwLock<HashMap<EpisodeId, Episode>>>,
    entity_index: Arc<RwLock<HashMap<String, Vec<EpisodeId>>>>,
    tag_index: Arc<RwLock<HashMap<String, Vec<EpisodeId>>>>,
}

impl Default for InMemoryEpisodeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEpisodeStore {
    /// 创建一个新的内存事件存储。
    pub fn new() -> Self {
        Self {
            episodes: Arc::new(RwLock::new(HashMap::new())),
            entity_index: Arc::new(RwLock::new(HashMap::new())),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 重建索引（用于测试/恢复）。
    pub async fn reindex(&self) -> LsResult<()> {
        let episodes = self.episodes.read().await;
        let mut entity_idx = HashMap::new();
        let mut tag_idx = HashMap::new();

        for (id, ep) in episodes.iter() {
            for entity in &ep.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                entity_idx.entry(key).or_insert_with(Vec::new).push(*id);
            }
            for tag in &ep.tags {
                tag_idx.entry(tag.clone()).or_insert_with(Vec::new).push(*id);
            }
        }

        *self.entity_index.write().await = entity_idx;
        *self.tag_index.write().await = tag_idx;
        Ok(())
    }
}

#[async_trait]
impl super::EpisodeRepository for InMemoryEpisodeStore {
    async fn store(&self, episode: Episode) -> LsResult<EpisodeId> {
        let id = episode.id;
        let entities = episode.entities.clone();
        let tags = episode.tags.clone();

        // 存储事件
        {
            let mut episodes = self.episodes.write().await;
            episodes.insert(id, episode);
        }

        // 更新实体索引
        {
            let mut entity_idx = self.entity_index.write().await;
            for entity in &entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                entity_idx.entry(key).or_insert_with(Vec::new).push(id);
            }
        }

        // 更新标签索引
        {
            let mut tag_idx = self.tag_index.write().await;
            for tag in &tags {
                tag_idx.entry(tag.clone()).or_insert_with(Vec::new).push(id);
            }
        }

        debug!(episode_id = %id, "episode stored");
        Ok(id)
    }

    async fn store_batch(&self, episodes: Vec<Episode>) -> LsResult<Vec<EpisodeId>> {
        let mut ids = Vec::with_capacity(episodes.len());
        for episode in episodes {
            let id = self.store(episode).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    async fn get(&self, id: EpisodeId) -> LsResult<Episode> {
        let episodes = self.episodes.read().await;
        episodes
            .get(&id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("episode {} not found", id)))
    }

    async fn query(&self, query: EpisodeQuery) -> LsResult<Vec<Episode>> {
        let (results, _) = self.query_with_stats(query).await?;
        Ok(results)
    }

    async fn query_with_stats(&self, query: EpisodeQuery) -> LsResult<(Vec<Episode>, QueryStats)> {
        let start = std::time::Instant::now();
        let episodes = self.episodes.read().await;

        // 收集所有匹配的事件
        let mut matched: Vec<Episode> = episodes
            .values()
            .filter(|ep| {
                // 实体过滤
                if !query.entities.is_empty() {
                    let entity_keys: HashSet<String> = ep
                        .entities
                        .iter()
                        .map(|e| format!("{}:{}", e.kind, e.name))
                        .collect();
                    let query_keys: HashSet<String> = query
                        .entities
                        .iter()
                        .map(|e| format!("{}:{}", e.kind, e.name))
                        .collect();
                    if query_keys.intersection(&entity_keys).next().is_none() {
                        return false;
                    }
                }

                // 标签过滤
                if !query.tags.is_empty() {
                    let ep_tags: HashSet<&str> =
                        ep.tags.iter().map(|t| t.as_str()).collect();
                    let query_tags: HashSet<&str> =
                        query.tags.iter().map(|t| t.as_str()).collect();
                    if query_tags.intersection(&ep_tags).next().is_none() {
                        return false;
                    }
                }

                // 时间范围过滤
                if let Some(from) = query.time_from {
                    if ep.timestamp < from {
                        return false;
                    }
                }
                if let Some(to) = query.time_to {
                    if ep.timestamp > to {
                        return false;
                    }
                }

                // 会话过滤
                if let Some(ref sid) = query.session_id {
                    if ep.session_id.as_deref() != Some(sid.as_str()) {
                        return false;
                    }
                }

                // 来源引用过滤
                if let Some(ref src) = query.source_ref {
                    if ep.source_ref.as_deref() != Some(src.as_str()) {
                        return false;
                    }
                }

                // 关键词搜索（标题 + 摘要模糊匹配）
                if let Some(ref text) = query.search_text {
                    let lower = text.to_lowercase();
                    let title_match = ep.title.to_lowercase().contains(&lower);
                    let summary_match = ep.summary.to_lowercase().contains(&lower);
                    if !title_match && !summary_match {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        let total_matched = matched.len();

        // 排序
        match query.sort_order {
            SortOrder::Ascending => matched.sort_by_key(|e| e.timestamp),
            SortOrder::Descending => {
                matched.sort_by(|a, b| b.timestamp.cmp(&a.timestamp))
            }
        }

        // 分页
        let returned = matched
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();

        let elapsed = start.elapsed().as_millis() as u64;

        Ok((
            returned,
            QueryStats {
                total_matched,
                returned: query.limit.min(total_matched.saturating_sub(query.offset)),
                query_time_ms: elapsed,
            },
        ))
    }

    async fn delete_by_entity(&self, entity: &str) -> LsResult<usize> {
        let mut count = 0;
        let mut to_remove = Vec::new();

        {
            let episodes = self.episodes.read().await;
            for (id, ep) in episodes.iter() {
                if ep.entities.iter().any(|e| {
                    format!("{}:{}", e.kind, e.name).to_lowercase()
                        == entity.to_lowercase()
                }) {
                    to_remove.push(*id);
                }
            }
        }

        for id in &to_remove {
            let ep = { self.episodes.write().await.remove(id) };
            if let Some(episode) = ep {
                // 清理索引
                let mut entity_idx = self.entity_index.write().await;
                for entity in &episode.entities {
                    let key = format!("{}:{}", entity.kind, entity.name);
                    if let Some(ids) = entity_idx.get_mut(&key) {
                        ids.retain(|eid| eid != id);
                        if ids.is_empty() {
                            entity_idx.remove(&key);
                        }
                    }
                }
                let mut tag_idx = self.tag_index.write().await;
                for tag in &episode.tags {
                    if let Some(ids) = tag_idx.get_mut(tag) {
                        ids.retain(|eid| eid != id);
                        if ids.is_empty() {
                            tag_idx.remove(tag);
                        }
                    }
                }
                count += 1;
            }
        }

        Ok(count)
    }

    async fn delete(&self, id: EpisodeId) -> LsResult<bool> {
        let episode = { self.episodes.write().await.remove(&id) };
        if let Some(episode) = episode {
            let mut entity_idx = self.entity_index.write().await;
            for entity in &episode.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                if let Some(ids) = entity_idx.get_mut(&key) {
                    ids.retain(|eid| *eid != id);
                    if ids.is_empty() {
                        entity_idx.remove(&key);
                    }
                }
            }
            let mut tag_idx = self.tag_index.write().await;
            for tag in &episode.tags {
                if let Some(ids) = tag_idx.get_mut(tag) {
                    ids.retain(|eid| *eid != id);
                    if ids.is_empty() {
                        tag_idx.remove(tag);
                    }
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn count(&self) -> LsResult<usize> {
        let episodes = self.episodes.read().await;
        Ok(episodes.len())
    }

    async fn list_entities(&self) -> LsResult<Vec<String>> {
        let entity_idx = self.entity_index.read().await;
        let mut entities: Vec<String> = entity_idx.keys().cloned().collect();
        entities.sort();
        Ok(entities)
    }

    async fn clear(&self) -> LsResult<()> {
        self.episodes.write().await.clear();
        self.entity_index.write().await.clear();
        self.tag_index.write().await.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::repository::EpisodeRepository;
    use super::*;
    use crate::{Episode, EntityRef};
    use chrono::{Duration, Utc};

    fn make_episode(
        title: &str,
        summary: &str,
        days_ago: i64,
        entities: Vec<(&str, &str)>,
        tags: Vec<&str>,
    ) -> Episode {
        let mut ep = Episode::new(
            title,
            summary,
            Utc::now() - Duration::days(days_ago),
        );
        for (kind, name) in entities {
            ep = ep.with_entity(EntityRef::new(kind, name));
        }
        for tag in tags {
            ep = ep.with_tag(tag);
        }
        ep
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let store = InMemoryEpisodeStore::new();
        let ep = make_episode("测试事件", "这是一个测试", 1, vec![("project", "测试项目")], vec!["test"]);
        let id = store.store(ep).await.unwrap();
        let retrieved = store.get(id).await.unwrap();
        assert_eq!(retrieved.title, "测试事件");
    }

    #[tokio::test]
    async fn test_query_by_entity() {
        let store = InMemoryEpisodeStore::new();
        store
            .store(make_episode("事件A", "项目A启动", 5, vec![("project", "项目A")], vec!["launch"]))
            .await
            .unwrap();
        store
            .store(make_episode("事件B", "项目B启动", 3, vec![("project", "项目B")], vec!["launch"]))
            .await
            .unwrap();

        let results = store
            .query(
                EpisodeQuery::default()
                    .with_entity(EntityRef::new("project", "项目A")),
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "事件A");
    }

    #[tokio::test]
    async fn test_query_by_tag() {
        let store = InMemoryEpisodeStore::new();
        store
            .store(make_episode("里程碑1", "第一个里程碑", 10, vec![], vec!["milestone"]))
            .await
            .unwrap();
        store
            .store(make_episode("日常更新", "日常", 1, vec![], vec!["daily"]))
            .await
            .unwrap();

        let results = store
            .query(EpisodeQuery::default().with_tag("milestone"))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "里程碑1");
    }

    #[tokio::test]
    async fn test_query_time_range() {
        let store = InMemoryEpisodeStore::new();
        store
            .store(make_episode("旧事件", "很旧", 100, vec![], vec![]))
            .await
            .unwrap();
        store
            .store(make_episode("新事件", "最近", 1, vec![], vec![]))
            .await
            .unwrap();

        let from = Utc::now() - Duration::days(7);
        let results = store
            .query(EpisodeQuery::default().with_time_range(from, Utc::now()))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "新事件");
    }

    #[tokio::test]
    async fn test_sort_order() {
        let store = InMemoryEpisodeStore::new();
        store
            .store(make_episode("最早", "最早的事件", 10, vec![], vec![]))
            .await
            .unwrap();
        store
            .store(make_episode("最新", "最新的事件", 1, vec![], vec![]))
            .await
            .unwrap();

        let asc = store
            .query(EpisodeQuery::default().with_sort(SortOrder::Ascending))
            .await
            .unwrap();
        assert_eq!(asc[0].title, "最早");

        let desc = store
            .query(EpisodeQuery::default().with_sort(SortOrder::Descending))
            .await
            .unwrap();
        assert_eq!(desc[0].title, "最新");
    }

    #[tokio::test]
    async fn test_delete() {
        let store = InMemoryEpisodeStore::new();
        let ep = make_episode("待删除", "将被删除", 1, vec![("project", "项目X")], vec!["temp"]);
        let id = store.store(ep).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        let deleted = store.delete(id).await.unwrap();
        assert!(deleted);
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_search_text() {
        let store = InMemoryEpisodeStore::new();
        store
            .store(make_episode("项目A暂停", "因为供应商问题暂停", 5, vec![], vec![]))
            .await
            .unwrap();
        store
            .store(make_episode("项目B完成", "全部完成", 3, vec![], vec![]))
            .await
            .unwrap();

        let results = store
            .query(EpisodeQuery::default().with_search("暂停"))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "项目A暂停");
    }

    #[tokio::test]
    async fn test_list_entities() {
        let store = InMemoryEpisodeStore::new();
        store
            .store(make_episode("事件1", "desc", 1, vec![("project", "项目A"), ("person", "张三")], vec![]))
            .await
            .unwrap();
        store
            .store(make_episode("事件2", "desc", 1, vec![("project", "项目B"), ("person", "李四")], vec![]))
            .await
            .unwrap();

        let entities = store.list_entities().await.unwrap();
        assert!(entities.contains(&"person:张三".to_string()));
        assert!(entities.contains(&"person:李四".to_string()));
        assert!(entities.contains(&"project:项目A".to_string()));
        assert!(entities.contains(&"project:项目B".to_string()));
    }
}
