//! EpisodeAnalyzer — 对 Episode 进行分组和预分析

use crate::types::*;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use lingshu_memory_episode::{Episode, EpisodeQuery, EpisodeRepository};
use std::collections::HashMap;
use std::sync::Arc;

/// Episode 分析器 — 对 Episode 进行多维分组。
pub struct EpisodeAnalyzer {
    store: Arc<dyn EpisodeRepository>,
    config: ConsolidationConfig,
}

impl EpisodeAnalyzer {
    pub fn new(store: Arc<dyn EpisodeRepository>, config: ConsolidationConfig) -> Self {
        Self { store, config }
    }

    /// 获取所有 Episode（通过 query 替代不存在的 list）。
    async fn all_episodes(&self) -> Result<Vec<Episode>, ConsolidationError> {
        let query = EpisodeQuery::default().with_limit(self.config.max_episodes_per_job);
        self.store.query(query).await.map_err(|e| ConsolidationError::StorageError(e.to_string()))
    }

    /// 按实体分组。
    pub async fn group_by_entity(&self) -> Result<Vec<(GroupKey, Vec<Episode>)>, ConsolidationError> {
        let all = self.all_episodes().await?;
        let mut groups: HashMap<String, Vec<Episode>> = HashMap::new();

        for episode in all {
            for entity in &episode.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                groups.entry(key).or_default().push(episode.clone());
            }
        }

        Ok(groups
            .into_iter()
            .filter(|(_, eps)| eps.len() >= self.config.min_episodes_for_entity)
            .map(|(key, eps)| (GroupKey::Entity(key), eps))
            .collect())
    }

    /// 按时间窗口分组。
    pub async fn group_by_time_window(&self, window_hours: i64) -> Result<Vec<(GroupKey, Vec<Episode>)>, ConsolidationError> {
        let w = window_hours.max(self.config.min_window_hours).min(self.config.max_window_hours);
        let all = self.all_episodes().await?;
        let mut groups: HashMap<String, Vec<Episode>> = HashMap::new();

        for episode in all {
            let key = time_window_key(&episode.timestamp, w);
            groups.entry(key).or_default().push(episode);
        }

        let mut result: Vec<(GroupKey, Vec<Episode>)> = groups
            .into_iter()
            .map(|(key, eps)| (GroupKey::TimeWindow(key), eps))
            .collect();
        result.sort_by(|a, b| format!("{}", b.0).cmp(&format!("{}", a.0)));
        Ok(result)
    }

    /// 按标签分组。
    pub async fn group_by_tag(&self) -> Result<Vec<(GroupKey, Vec<Episode>)>, ConsolidationError> {
        let all = self.all_episodes().await?;
        let mut groups: HashMap<String, Vec<Episode>> = HashMap::new();
        for episode in all {
            for tag in &episode.tags {
                groups.entry(tag.clone()).or_default().push(episode.clone());
            }
        }
        Ok(groups.into_iter().map(|(k, v)| (GroupKey::Tag(k), v)).collect())
    }

    /// 获取待巩固的候选（旧且未标记 consolidated 的 Episode）。
    pub async fn get_consolidation_candidates(&self, older_than_hours: i64) -> Result<Vec<Episode>, ConsolidationError> {
        let cutoff = Utc::now() - Duration::hours(older_than_hours);
        let all = self.all_episodes().await?;
        Ok(all.into_iter().filter(|ep| ep.timestamp < cutoff && !ep.tags.contains(&"consolidated".to_string())).collect())
    }

    /// 统计未巩固的 Episode 数量。
    pub async fn count_unconsolidated(&self) -> Result<usize, ConsolidationError> {
        let all = self.all_episodes().await?;
        Ok(all.into_iter().filter(|ep| !ep.tags.contains(&"consolidated".to_string())).count())
    }

    /// 获取巩固状态统计。
    pub async fn consolidation_stats(&self) -> Result<ConsolidationStats, ConsolidationError> {
        let all = self.all_episodes().await?;
        let total = all.len();
        let consolidated = all.iter().filter(|ep| ep.tags.contains(&"consolidated".to_string())).count();
        let mut entities = std::collections::HashSet::new();
        for ep in &all {
            for entity in &ep.entities {
                entities.insert(format!("{}:{}", entity.kind, entity.name));
            }
        }
        Ok(ConsolidationStats {
            total_episodes: total,
            consolidated_count: consolidated,
            unconsolidated_count: total - consolidated,
            entity_count: entities.len(),
            oldest_episode: all.iter().map(|e| e.timestamp).min(),
            newest_episode: all.iter().map(|e| e.timestamp).max(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ConsolidationStats {
    pub total_episodes: usize,
    pub consolidated_count: usize,
    pub unconsolidated_count: usize,
    pub entity_count: usize,
    pub oldest_episode: Option<DateTime<Utc>>,
    pub newest_episode: Option<DateTime<Utc>>,
}

fn time_window_key(time: &DateTime<Utc>, window_hours: i64) -> String {
    if window_hours >= 24 * 30 {
        format!("{}-{:02}", time.year(), time.month())
    } else if window_hours >= 24 * 7 {
        format!("{}-W{:02}", time.iso_week().year(), time.iso_week().week())
    } else if window_hours >= 24 {
        format!("{}-{:02}-{:02}", time.year(), time.month(), time.day())
    } else {
        format!("{}-{:02}-{:02} {:02}:00", time.year(), time.month(), time.day(), time.hour())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_memory_episode::{EntityRef, InMemoryEpisodeStore};

    fn setup_store() -> Arc<dyn EpisodeRepository> {
        Arc::new(InMemoryEpisodeStore::new())
    }

    async fn add_test_episodes(store: &Arc<dyn EpisodeRepository>) {
        let mut ep1 = Episode::new("项目A启动", "项目A正式启动", Utc::now() - Duration::hours(48));
        ep1.entities.push(EntityRef::new("project", "项目A"));
        ep1.entities.push(EntityRef::new("person", "张三"));
        ep1.tags.push("launch".to_string());

        let mut ep2 = Episode::new("供应商退出", "核心供应商退出", Utc::now() - Duration::hours(24));
        ep2.entities.push(EntityRef::new("project", "项目A"));
        ep2.entities.push(EntityRef::new("organization", "供应商X"));
        ep2.tags.push("risk".to_string());

        let mut ep3 = Episode::new("项目A暂停", "因供应商问题暂停", Utc::now() - Duration::hours(1));
        ep3.entities.push(EntityRef::new("project", "项目A"));
        ep3.tags.push("decision".to_string());

        let mut ep4 = Episode::new("项目B启动", "项目B开始", Utc::now() - Duration::hours(72));
        ep4.entities.push(EntityRef::new("project", "项目B"));
        ep4.entities.push(EntityRef::new("person", "李四"));
        ep4.tags.push("launch".to_string());

        store.store(ep1).await.unwrap();
        store.store(ep2).await.unwrap();
        store.store(ep3).await.unwrap();
        store.store(ep4).await.unwrap();
    }

    #[tokio::test]
    async fn test_group_by_entity() {
        let store = setup_store();
        add_test_episodes(&store).await;

        let config = ConsolidationConfig { min_episodes_for_entity: 2, ..Default::default() };
        let analyzer = EpisodeAnalyzer::new(store, config);
        let groups = analyzer.group_by_entity().await.unwrap();

        let pa = groups.iter().find(|(k, _)| matches!(k, GroupKey::Entity(v) if v == "project:项目A"));
        assert!(pa.is_some());
        assert_eq!(pa.unwrap().1.len(), 3);

        let pb = groups.iter().find(|(k, _)| matches!(k, GroupKey::Entity(v) if v == "project:项目B"));
        assert!(pb.is_none());
    }

    #[tokio::test]
    async fn test_group_by_tag() {
        let store = setup_store();
        add_test_episodes(&store).await;

        let analyzer = EpisodeAnalyzer::new(store, ConsolidationConfig::default());
        let groups = analyzer.group_by_tag().await.unwrap();

        let launch = groups.iter().find(|(k, _)| matches!(k, GroupKey::Tag(v) if v == "launch"));
        assert!(launch.is_some());
        assert_eq!(launch.unwrap().1.len(), 2);
    }

    #[tokio::test]
    async fn test_consolidation_candidates() {
        let store = setup_store();
        add_test_episodes(&store).await;

        let analyzer = EpisodeAnalyzer::new(store, ConsolidationConfig::default());
        let candidates = analyzer.get_consolidation_candidates(12).await.unwrap();

        assert!(candidates.iter().any(|e| e.title == "项目A启动"));
        assert!(!candidates.iter().any(|e| e.title == "项目A暂停"));
    }

    #[tokio::test]
    async fn test_stats() {
        let store = setup_store();
        add_test_episodes(&store).await;

        let analyzer = EpisodeAnalyzer::new(store, ConsolidationConfig::default());
        let stats = analyzer.consolidation_stats().await.unwrap();

        assert_eq!(stats.total_episodes, 4);
        assert_eq!(stats.unconsolidated_count, 4);
    }

    #[tokio::test]
    async fn test_group_by_time_window() {
        let store = setup_store();
        add_test_episodes(&store).await;

        let analyzer = EpisodeAnalyzer::new(store, ConsolidationConfig::default());
        let groups = analyzer.group_by_time_window(24).await.unwrap();
        assert!(!groups.is_empty());
    }

    #[tokio::test]
    async fn test_count_unconsolidated() {
        let store = setup_store();
        add_test_episodes(&store).await;

        let analyzer = EpisodeAnalyzer::new(store.clone(), ConsolidationConfig::default());
        assert_eq!(analyzer.count_unconsolidated().await.unwrap(), 4);
    }

    #[test]
    fn test_time_window_key() {
        let time = Utc::now();
        assert!(time_window_key(&time, 1).contains(":00"));
        assert!(!time_window_key(&time, 24).contains(':'));
        assert!(time_window_key(&time, 24 * 7).contains('W'));
    }
}
