//! ConsolidationStrategy trait 及内置策略实现

use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;
use lingshu_memory_episode::{EntityRef, Episode};
use std::collections::HashMap;

// ─── ConsolidationStrategy Trait ────────────────────────

#[async_trait]
pub trait ConsolidationStrategy: Send + Sync {
    fn name(&self) -> &str;
    async fn consolidate(&self, episodes: &[Episode]) -> Result<Vec<ConsolidatedMemory>, ConsolidationError>;
    fn description(&self) -> &str { "" }
}

// ─── SummarizeStrategy ──────────────────────────────────

pub struct SummarizeStrategy {
    max_source_episodes: usize,
}

impl SummarizeStrategy {
    pub fn new() -> Self { Self { max_source_episodes: 100 } }
    pub fn with_max_sources(mut self, max: usize) -> Self { self.max_source_episodes = max; self }
}

impl Default for SummarizeStrategy { fn default() -> Self { Self::new() } }

#[async_trait]
impl ConsolidationStrategy for SummarizeStrategy {
    fn name(&self) -> &str { "summarize" }
    fn description(&self) -> &str { "将同一实体/时间窗口下的多个 Episode 合并为一条带时间线摘要的巩固记忆" }

    async fn consolidate(&self, episodes: &[Episode]) -> Result<Vec<ConsolidatedMemory>, ConsolidationError> {
        if episodes.is_empty() {
            return Ok(Vec::new());
        }

        let mut sorted: Vec<&Episode> = episodes.iter().collect();
        sorted.sort_by_key(|e| e.timestamp);

        let subset: Vec<&&Episode> = sorted.iter().take(self.max_source_episodes).collect();
        if subset.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_entities = Vec::new();
        let mut all_tags = Vec::new();
        let mut episodes_text: Vec<String> = Vec::new();
        let mut source_ids: Vec<String> = Vec::new();
        let mut earliest: Option<chrono::DateTime<Utc>> = None;
        let mut latest: Option<chrono::DateTime<Utc>> = None;

        for ep in subset {
            for entity in &ep.entities {
                if !all_entities.contains(entity) {
                    all_entities.push(entity.clone());
                }
            }
            for tag in &ep.tags {
                if !all_tags.contains(tag) {
                    all_tags.push(tag.to_string());
                }
            }
            episodes_text.push(format!(
                "[{}] {}: {}",
                ep.timestamp.format("%Y-%m-%d %H:%M"),
                ep.title, ep.summary
            ));
            source_ids.push(ep.id.to_string());

            if earliest.is_none() || ep.timestamp < earliest.unwrap() {
                earliest = Some(ep.timestamp);
            }
            if latest.is_none() || ep.timestamp > latest.unwrap() {
                latest = Some(ep.timestamp);
            }
        }

        let title = if episodes.len() == 1 {
            format!("记忆: {}", sorted[0].title)
        } else {
            format!(
                "事件总结 ({}条, {} ~ {})",
                episodes.len(),
                earliest.map(|t| t.format("%m-%d").to_string()).unwrap_or_default(),
                latest.map(|t| t.format("%m-%d").to_string()).unwrap_or_default()
            )
        };

        let summary = format!(
            "共 {} 条相关事件的时间线总结：\n{}",
            episodes.len(), episodes_text.join("\n")
        );

        let mut memory = ConsolidatedMemory::new(&title, &summary, "summarize")
            .with_confidence(if episodes.len() >= 3 { 0.9 } else { 0.7 });

        for entity in all_entities { memory = memory.with_entity(entity); }
        for tag in all_tags { memory = memory.with_tag(tag); }
        for id in source_ids { memory = memory.with_source(id); }
        if let (Some(start), Some(end)) = (earliest, latest) {
            memory = memory.with_time_span(start, end);
        }

        Ok(vec![memory])
    }
}

// ─── DedupStrategy ──────────────────────────────────────

pub struct DedupStrategy {
    time_threshold_secs: i64,
    title_similarity_threshold: f64,
}

impl DedupStrategy {
    pub fn new() -> Self {
        Self { time_threshold_secs: 300, title_similarity_threshold: 1.0 }
    }
    pub fn with_time_threshold(mut self, secs: i64) -> Self { self.time_threshold_secs = secs; self }
    pub fn with_similarity_threshold(mut self, threshold: f64) -> Self {
        self.title_similarity_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    fn title_similarity(a: &str, b: &str) -> f64 {
        if a == b { return 1.0; }
        let set_a: std::collections::HashSet<char> = a.chars().collect();
        let set_b: std::collections::HashSet<char> = b.chars().collect();
        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();
        if union == 0 { 1.0 } else { intersection as f64 / union as f64 }
    }
}

impl Default for DedupStrategy { fn default() -> Self { Self::new() } }

#[async_trait]
impl ConsolidationStrategy for DedupStrategy {
    fn name(&self) -> &str { "dedup" }
    fn description(&self) -> &str { "检测并合并标题和时间接近的重复 Episode" }

    async fn consolidate(&self, episodes: &[Episode]) -> Result<Vec<ConsolidatedMemory>, ConsolidationError> {
        if episodes.len() < 2 { return Ok(Vec::new()); }

        let mut merged: Vec<ConsolidatedMemory> = Vec::new();
        let mut used: Vec<bool> = vec![false; episodes.len()];

        for i in 0..episodes.len() {
            if used[i] { continue; }
            used[i] = true;
            let mut duplicates: Vec<&Episode> = vec![&episodes[i]];

            for j in (i + 1)..episodes.len() {
                if used[j] { continue; }
                let time_diff = (episodes[i].timestamp - episodes[j].timestamp)
                    .num_seconds().abs();
                let sim = Self::title_similarity(&episodes[i].title, &episodes[j].title);

                if time_diff <= self.time_threshold_secs && sim >= self.title_similarity_threshold {
                    duplicates.push(&episodes[j]);
                    used[j] = true;
                }
            }

            if duplicates.len() > 1 {
                let primary = duplicates[0];
                let mut memory = ConsolidatedMemory::new(
                    &primary.title,
                    format!(
                        "去重合并: {} 条相似事件 ({} ~ {})",
                        duplicates.len(),
                        duplicates.first().map(|e| e.timestamp.format("%H:%M:%S").to_string()).unwrap_or_default(),
                        duplicates.last().map(|e| e.timestamp.format("%H:%M:%S").to_string()).unwrap_or_default(),
                    ),
                    "dedup",
                ).with_confidence(0.95);

                for ep in &duplicates {
                    for entity in &ep.entities {
                        memory = memory.with_entity(entity.clone());
                    }
                    for tag in &ep.tags {
                        memory = memory.with_tag(tag);
                    }
                    memory = memory.with_source(ep.id.to_string());
                }
                if let (Some(first), Some(last)) = (duplicates.first(), duplicates.last()) {
                    memory = memory.with_time_span(first.timestamp, last.timestamp);
                }
                merged.push(memory);
            }
        }
        Ok(merged)
    }
}

// ─── ProfileStrategy ────────────────────────────────────

pub struct ProfileStrategy { min_episodes: usize }

impl ProfileStrategy {
    pub fn new() -> Self { Self { min_episodes: 2 } }
    pub fn with_min_episodes(mut self, min: usize) -> Self { self.min_episodes = min; self }
}

impl Default for ProfileStrategy { fn default() -> Self { Self::new() } }

#[async_trait]
impl ConsolidationStrategy for ProfileStrategy {
    fn name(&self) -> &str { "profile" }
    fn description(&self) -> &str { "从多个 Episode 中提取实体的完整画像" }

    async fn consolidate(&self, episodes: &[Episode]) -> Result<Vec<ConsolidatedMemory>, ConsolidationError> {
        if episodes.len() < self.min_episodes { return Ok(Vec::new()); }

        let mut entity_episodes: HashMap<String, Vec<&Episode>> = HashMap::new();
        for ep in episodes {
            for entity in &ep.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                entity_episodes.entry(key).or_default().push(ep);
            }
        }

        let mut results: Vec<ConsolidatedMemory> = Vec::new();
        for (entity_key, entity_eps) in &entity_episodes {
            if entity_eps.len() < self.min_episodes { continue; }

            let mut sorted: Vec<&&Episode> = entity_eps.iter().collect();
            sorted.sort_by_key(|e| e.timestamp);

            let mut source_ids: Vec<String> = Vec::new();
            let mut timeline: Vec<String> = Vec::new();
            let earliest = sorted.first().map(|e| e.timestamp);
            let latest = sorted.last().map(|e| e.timestamp);

            for ep in &sorted {
                timeline.push(format!(
                    "- [{}] {}: {}",
                    ep.timestamp.format("%Y-%m-%d %H:%M"), ep.title, ep.summary
                ));
                source_ids.push(ep.id.to_string());
            }

            let parts: Vec<&str> = entity_key.splitn(2, ':').collect();
            let e_kind = parts.first().copied().unwrap_or("entity");
            let e_name = parts.get(1).copied().unwrap_or(entity_key);

            let mut memory = ConsolidatedMemory::new(
                format!("实体画像: {}", entity_key),
                format!("{} 的完整事件时间线（共 {} 条事件）：\n{}", entity_key, sorted.len(), timeline.join("\n")),
                "profile",
            )
            .with_entity(EntityRef::new(e_kind, e_name))
            .with_tag("profile").with_tag("consolidated")
            .with_confidence(0.85);

            for id in source_ids { memory = memory.with_source(id); }
            if let (Some(start), Some(end)) = (earliest, latest) {
                memory = memory.with_time_span(start, end);
            }
            results.push(memory);
        }
        Ok(results)
    }
}

// ─── 内置策略工厂 ───────────────────────────────────────

pub fn default_strategies() -> Vec<Box<dyn ConsolidationStrategy>> {
    vec![
        Box::new(SummarizeStrategy::new()),
        Box::new(DedupStrategy::new()),
        Box::new(ProfileStrategy::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_memory_episode::Episode;

    fn make_episode(title: &str, summary: &str, hours_ago: i64) -> Episode {
        let time = Utc::now() - chrono::Duration::hours(hours_ago);
        Episode::new(title, summary, time)
    }

    #[tokio::test]
    async fn test_summarize_single() {
        let s = SummarizeStrategy::new();
        let r = s.consolidate(&[make_episode("项目A启动", "项目A正式启动", 1)]).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].source_episode_ids.len(), 1);
    }

    #[tokio::test]
    async fn test_summarize_multi() {
        let s = SummarizeStrategy::new();
        let eps = vec![
            make_episode("启动项目A", "项目A启动", 48),
            make_episode("供应商退出", "供应商退出", 24),
            make_episode("暂停项目A", "项目A暂停", 1),
        ];
        let r = s.consolidate(&eps).await.unwrap();
        assert_eq!(r.len(), 1);
        assert!(r[0].title.contains("3条"));
        assert_eq!(r[0].source_episode_ids.len(), 3);
    }

    #[tokio::test]
    async fn test_summarize_empty() {
        assert!(SummarizeStrategy::new().consolidate(&[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_dedup_duplicates() {
        let s = DedupStrategy::new();
        let t = Utc::now();
        let eps = vec![
            Episode::new("会议记录", "项目A讨论", t),
            Episode::new("会议记录", "项目A讨论", t + chrono::Duration::seconds(60)),
        ];
        let r = s.consolidate(&eps).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].source_episode_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_dedup_no_duplicates() {
        let r = DedupStrategy::new().consolidate(&[
            make_episode("事件A", "描述A", 48),
            make_episode("事件B", "描述B", 1),
        ]).await.unwrap();
        assert_eq!(r.len(), 0);
    }

    #[tokio::test]
    async fn test_dedup_single() {
        assert!(DedupStrategy::new().consolidate(&[make_episode("测试", "测试描述", 1)]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_dedup_custom_threshold() {
        let s = DedupStrategy::new().with_time_threshold(3600).with_similarity_threshold(0.5);
        let t = Utc::now();
        let eps = vec![
            Episode::new("项目A状态更新", "进行中", t),
            Episode::new("项目A状态变更", "已暂停", t + chrono::Duration::minutes(30)),
        ];
        assert_eq!(s.consolidate(&eps).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_profile_basic() {
        let s = ProfileStrategy::new();
        let mut ep1 = make_episode("项目启动", "项目A启动", 48);
        ep1.entities.push(EntityRef::new("project", "项目A"));
        let mut ep2 = make_episode("供应商退出", "供应商退出", 24);
        ep2.entities.push(EntityRef::new("project", "项目A"));
        let mut ep3 = make_episode("项目暂停", "项目A暂停", 1);
        ep3.entities.push(EntityRef::new("project", "项目A"));

        let r = s.consolidate(&[ep1, ep2, ep3]).await.unwrap();
        assert!(!r.is_empty());
        assert!(r[0].title.contains("项目A"));
    }

    #[tokio::test]
    async fn test_profile_min_episodes() {
        let s = ProfileStrategy::new().with_min_episodes(3);
        let mut ep1 = make_episode("事件", "描述", 1);
        ep1.entities.push(EntityRef::new("person", "张三"));
        assert!(s.consolidate(&[ep1]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_default_strategies() {
        let s = default_strategies();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].name(), "summarize");
        assert_eq!(s[1].name(), "dedup");
        assert_eq!(s[2].name(), "profile");
    }

    #[test]
    fn test_title_similarity() {
        assert!((DedupStrategy::title_similarity("项目A启动", "项目A启动") - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_memory_builder() {
        let m = ConsolidatedMemory::new("测试", "摘要", "test")
            .with_entity(EntityRef::new("project", "项目A"))
            .with_tag("important")
            .with_source("ep-001")
            .with_confidence(0.8);
        assert_eq!(m.entities.len(), 1);
        assert_eq!(m.source_episode_ids.len(), 1);
    }

    #[test]
    fn test_memory_to_episode() {
        let m = ConsolidatedMemory::new("标题", "摘要", "summarize").with_tag("consolidated");
        let ep = m.to_episode();
        assert_eq!(ep.title, "标题");
        assert!(ep.tags.contains(&"consolidated".to_string()));
    }

    #[test]
    fn test_report_empty() {
        let r = ConsolidationReport::empty();
        assert!(r.success);
        assert_eq!(r.processed_count, 0);
    }

    #[test]
    fn test_group_key_display() {
        assert_eq!(format!("{}", GroupKey::Entity("项目A".into())), "entity:项目A");
        assert_eq!(format!("{}", GroupKey::TimeWindow("2026-07".into())), "time:2026-07");
        assert_eq!(format!("{}", GroupKey::Tag("project".into())), "tag:project");
    }
}
