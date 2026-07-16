//! Consolidation 类型定义

use chrono::{DateTime, Utc};
use lingshu_memory_episode::{EntityRef, Episode};
use serde::{Deserialize, Serialize};

// ─── ConsolidatedMemory ─────────────────────────────────

/// 巩固后的记忆单元。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedMemory {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub entities: Vec<EntityRef>,
    pub tags: Vec<String>,
    pub source_episode_ids: Vec<String>,
    pub time_span_start: Option<DateTime<Utc>>,
    pub time_span_end: Option<DateTime<Utc>>,
    pub strategy: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
}

impl ConsolidatedMemory {
    pub fn new(title: impl Into<String>, summary: impl Into<String>, strategy: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
            title: title.into(),
            summary: summary.into(),
            entities: Vec::new(),
            tags: Vec::new(),
            source_episode_ids: Vec::new(),
            time_span_start: None,
            time_span_end: None,
            strategy: strategy.into(),
            confidence: 1.0,
            created_at: Utc::now(),
        }
    }

    pub fn with_entity(mut self, entity: EntityRef) -> Self {
        if !self.entities.contains(&entity) {
            self.entities.push(entity);
        }
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        let tag = tag.into();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
        self
    }

    pub fn with_source(mut self, episode_id: impl Into<String>) -> Self {
        self.source_episode_ids.push(episode_id.into());
        self
    }

    pub fn with_time_span(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.time_span_start = Some(start);
        self.time_span_end = Some(end);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.max(0.0).min(1.0);
        self
    }

    /// 转换为 Episode（可写回 Store）。
    pub fn to_episode(&self) -> Episode {
        let mut episode = Episode::new(&self.title, &self.summary, self.created_at)
            .with_tag("consolidated")
            .with_tag(&self.strategy);

        for entity in &self.entities {
            episode = episode.with_entity(entity.clone());
        }
        for tag in &self.tags {
            episode = episode.with_tag(tag);
        }
        episode
    }
}

// ─── ConsolidationConfig ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    pub max_episodes_per_job: usize,
    pub min_episodes_for_entity: usize,
    pub min_window_hours: i64,
    pub max_window_hours: i64,
    pub dedup_time_threshold_secs: i64,
    pub auto_write_episodes: bool,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            max_episodes_per_job: 1000,
            min_episodes_for_entity: 2,
            min_window_hours: 1,
            max_window_hours: 24 * 7,
            dedup_time_threshold_secs: 300,
            auto_write_episodes: true,
        }
    }
}

// ─── ConsolidationReport ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub processed_count: usize,
    pub consolidated_count: usize,
    pub strategy_stats: Vec<StrategyStat>,
    pub execution_time_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyStat {
    pub strategy_name: String,
    pub processed: usize,
    pub produced: usize,
    pub time_ms: u64,
}

impl ConsolidationReport {
    pub fn empty() -> Self {
        Self {
            processed_count: 0,
            consolidated_count: 0,
            strategy_stats: Vec::new(),
            execution_time_ms: 0,
            success: true,
            error: None,
        }
    }
}

// ─── GroupKey ───────────────────────────────────────────

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum GroupKey {
    Entity(String),
    TimeWindow(String),
    Tag(String),
    Custom(String),
}

impl std::fmt::Display for GroupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupKey::Entity(v) => write!(f, "entity:{}", v),
            GroupKey::TimeWindow(v) => write!(f, "time:{}", v),
            GroupKey::Tag(v) => write!(f, "tag:{}", v),
            GroupKey::Custom(v) => write!(f, "custom:{}", v),
        }
    }
}

// ─── ConsolidationError ─────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ConsolidationError {
    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("策略错误: {0}")]
    StrategyError(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("内部错误: {0}")]
    Internal(String),
}
