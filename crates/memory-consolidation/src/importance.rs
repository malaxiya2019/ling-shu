//! ImportanceScorer — 记忆重要性评分器。
//!
//! 对 ConsolidatedMemory 和 Episode 进行多维重要性评估，
//! 用于记忆优先级排序、自动归档和遗忘决策。

use crate::types::*;
use chrono::Utc;
use lingshu_memory_episode::Episode;

/// 重要性等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImportanceLevel {
    /// 关键记忆（> 0.85）
    Critical,
    /// 重要记忆（> 0.65）
    Important,
    /// 普通记忆（> 0.40）
    Normal,
    /// 低价值记忆（<= 0.40）
    LowValue,
}

impl ImportanceLevel {
    pub fn from_score(score: f64) -> Self {
        if score > 0.85 {
            Self::Critical
        } else if score > 0.65 {
            Self::Important
        } else if score > 0.40 {
            Self::Normal
        } else {
            Self::LowValue
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Important => "important",
            Self::Normal => "normal",
            Self::LowValue => "low_value",
        }
    }
}

/// ImportanceScorer — 多维重要性评分器。
///
/// 评分维度：
/// - **时效性**（Recency）：越近的事件得分越高，30 天衰减
/// - **实体丰富度**（Entity Richness）：关联实体越多越重要
/// - **源数量**（Source Count）：更多源事件意味着信息更可靠
/// - **置信度**（Confidence）：合并算法的置信度
/// - **状态变更**（State Changes）：有状态变更的事件更重要
/// - **策略权重**（Strategy Weight）：不同 consolidation 策略的基准权重
pub struct ImportanceScorer {
    /// 时效性权重 (default: 0.30)
    pub recency_weight: f64,
    /// 实体丰富度权重 (default: 0.15)
    pub entity_weight: f64,
    /// 源数量权重 (default: 0.15)
    pub source_weight: f64,
    /// 置信度权重 (default: 0.15)
    pub confidence_weight: f64,
    /// 状态变更加成 (default: 0.15)
    pub state_change_bonus: f64,
    /// 策略权重映射
    pub strategy_weights: std::collections::HashMap<String, f64>,
    /// 评分衰减半衰期（小时, default: 168 = 7 天）
    pub half_life_hours: f64,
}

impl Default for ImportanceScorer {
    fn default() -> Self {
        let mut strategy_weights = std::collections::HashMap::new();
        strategy_weights.insert("profile".to_string(), 0.10);
        strategy_weights.insert("summarize".to_string(), 0.05);
        strategy_weights.insert("dedup".to_string(), 0.02);

        Self {
            recency_weight: 0.30,
            entity_weight: 0.15,
            source_weight: 0.15,
            confidence_weight: 0.15,
            state_change_bonus: 0.15,
            strategy_weights,
            half_life_hours: 168.0, // 7 天
        }
    }
}

impl ImportanceScorer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算 ConsolidatedMemory 的重要性分数 (0.0 ~ 1.0)。
    pub fn score(&self, memory: &ConsolidatedMemory) -> f64 {
        let mut score = 0.0;

        // 1. 时效性分数 (指数衰减)
        let age_hours = (Utc::now() - memory.created_at)
            .num_hours()
            .max(0) as f64;
        let recency = (-age_hours / self.half_life_hours).exp(); // 指数衰减
        score += self.recency_weight * recency;

        // 2. 实体丰富度
        let entity_factor = (memory.entities.len() as f64 / 5.0).min(1.0);
        score += self.entity_weight * entity_factor;

        // 3. 源事件数量
        let source_factor = (memory.source_episode_ids.len() as f64 / 10.0).min(1.0);
        score += self.source_weight * source_factor;

        // 4. 置信度
        score += self.confidence_weight * memory.confidence;

        // 5. 策略权重加成
        if let Some(w) = self.strategy_weights.get(&memory.strategy) {
            score += w;
        }

        score.clamp(0.0, 1.0)
    }

    /// 计算 Episode 的重要性分数 (0.0 ~ 1.0)。
    pub fn score_episode(&self, episode: &Episode) -> f64 {
        let mut score = 0.20; // baseline

        // 1. 时效性
        let age_hours = (Utc::now() - episode.timestamp)
            .num_hours()
            .max(0) as f64;
        let recency = (-age_hours / self.half_life_hours).exp();
        score += self.recency_weight * recency;

        // 2. 实体丰富度
        let entity_factor = (episode.entities.len() as f64 / 5.0).min(1.0);
        score += self.entity_weight * entity_factor;

        // 3. 状态变更加成
        if !episode.state_changes.is_empty() {
            score += self.state_change_bonus;
        }

        // 4. consolidated 降权（原始事件比 consolidated 摘要更重要）
        if episode.tags.contains(&"consolidated".to_string()) {
            score *= 0.7;
        }

        score.clamp(0.0, 1.0)
    }

    /// 批量评分一组 ConsolidatedMemory。
    pub fn score_batch(&self, memories: &[ConsolidatedMemory]) -> Vec<(usize, f64, ImportanceLevel)> {
        memories
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let score = self.score(m);
                (i, score, ImportanceLevel::from_score(score))
            })
            .collect()
    }

    /// 归一化分数到 0.0~1.0 范围。
    pub fn normalize(scores: &[f64]) -> Vec<f64> {
        let max = scores.iter().cloned().fold(0.0_f64, f64::max);
        if max <= 0.0 {
            return vec![0.0; scores.len()];
        }
        scores.iter().map(|s| s / max).collect()
    }

    /// 按重要性排序 consolidated memories（降序）。
    pub fn sort_by_importance(&self, memories: &mut Vec<ConsolidatedMemory>) {
        let scores: Vec<f64> = memories.iter().map(|m| self.score(m)).collect();
        let mut indices: Vec<usize> = (0..memories.len()).collect();
        indices.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));

        let sorted: Vec<ConsolidatedMemory> = indices.iter().map(|&i| memories[i].clone()).collect();
        *memories = sorted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_memory_episode::EntityRef;

    fn make_memory(days_ago: i64, entities: usize, sources: usize, confidence: f64, strategy: &str) -> ConsolidatedMemory {
        let time = Utc::now() - chrono::Duration::days(days_ago);
        let mut mem = ConsolidatedMemory::new("测试记忆", "测试摘要", strategy)
            .with_confidence(confidence);
        mem.created_at = time;

        for i in 0..entities {
            mem = mem.with_entity(EntityRef::new("test", format!("实体{}", i)));
        }
        for i in 0..sources {
            mem = mem.with_source(format!("src-{}", i));
        }
        mem
    }

    fn make_episode(days_ago: i64, entities: usize, has_state_change: bool) -> Episode {
        let time = Utc::now() - chrono::Duration::days(days_ago);
        let mut ep = Episode::new("测试事件", "测试描述", time);

        for i in 0..entities {
            ep = ep.with_entity(EntityRef::new("test", format!("实体{}", i)));
        }
        if has_state_change {
            ep = ep.with_state_change(lingshu_memory_episode::StateChange::new(
                EntityRef::new("entity", "测试"),
                "status",
                None,
                "changed",
            ));
        }
        ep
    }

    #[test]
    fn test_importance_level_from_score() {
        assert_eq!(ImportanceLevel::from_score(0.9), ImportanceLevel::Critical);
        assert_eq!(ImportanceLevel::from_score(0.75), ImportanceLevel::Important);
        assert_eq!(ImportanceLevel::from_score(0.5), ImportanceLevel::Normal);
        assert_eq!(ImportanceLevel::from_score(0.3), ImportanceLevel::LowValue);
    }

    #[test]
    fn test_level_as_str() {
        assert_eq!(ImportanceLevel::Critical.as_str(), "critical");
        assert_eq!(ImportanceLevel::LowValue.as_str(), "low_value");
    }

    #[test]
    fn test_recent_memory_scores_higher() {
        let scorer = ImportanceScorer::new();
        let recent = make_memory(1, 2, 2, 0.9, "summarize");
        let old = make_memory(365, 2, 2, 0.9, "summarize");

        let recent_score = scorer.score(&recent);
        let old_score = scorer.score(&old);
        assert!(
            recent_score > old_score,
            "recent memory ({:.3}) should score higher than old ({:.3})",
            recent_score,
            old_score
        );
    }

    #[test]
    fn test_more_entities_scores_higher() {
        let scorer = ImportanceScorer::new();
        let rich = make_memory(7, 5, 2, 0.8, "summarize");
        let poor = make_memory(7, 0, 2, 0.8, "summarize");
        assert!(scorer.score(&rich) > scorer.score(&poor));
    }

    #[test]
    fn test_profile_strategy_weight() {
        let scorer = ImportanceScorer::new();
        let profile = make_memory(7, 2, 2, 0.8, "profile");
        let dedup = make_memory(7, 2, 2, 0.8, "dedup");
        // profile has higher strategy weight
        assert!(scorer.score(&profile) > scorer.score(&dedup));
    }

    #[test]
    fn test_score_range() {
        let scorer = ImportanceScorer::new();
        for days in [0, 1, 7, 30, 90, 365] {
            let mem = make_memory(days, 3, 5, 0.8, "summarize");
            let score = scorer.score(&mem);
            assert!(
                (0.0..=1.0).contains(&score),
                "score {:.3} for {} days ago should be in [0,1]",
                score,
                days
            );
        }
    }

    #[test]
    fn test_episode_state_change_bonus() {
        let scorer = ImportanceScorer::new();
        let with_change = make_episode(7, 2, true);
        let without_change = make_episode(7, 2, false);

        assert!(scorer.score_episode(&with_change) > scorer.score_episode(&without_change));
    }

    #[test]
    fn test_episode_consolidated_penalty() {
        let scorer = ImportanceScorer::new();
        let mut ep = make_episode(7, 2, false);
        ep.tags.push("consolidated".to_string());
        let raw = make_episode(7, 2, false);

        assert!(
            scorer.score_episode(&raw) >= scorer.score_episode(&ep),
            "consolidated episode should not score higher"
        );
    }

    #[test]
    fn test_batch_scoring() {
        let scorer = ImportanceScorer::new();
        let memories = vec![
            make_memory(1, 5, 10, 0.95, "profile"),
            make_memory(30, 2, 2, 0.7, "summarize"),
            make_memory(365, 0, 0, 0.5, "dedup"),
        ];
        let results = scorer.score_batch(&memories);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].1, scorer.score(&memories[0]));
    }

    #[test]
    fn test_normalization() {
        let scores = vec![0.5, 0.8, 0.0, 1.0];
        let normalized = ImportanceScorer::normalize(&scores);
        assert!((normalized[3] - 1.0).abs() < 0.01);
        assert!((normalized[0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_sort_by_importance() {
        let scorer = ImportanceScorer::new();
        let mut memories = vec![
            make_memory(365, 0, 0, 0.5, "dedup"),  // lowest
            make_memory(1, 5, 10, 0.95, "profile"),  // highest
            make_memory(30, 2, 2, 0.7, "summarize"), // middle
        ];

        let _scores_before: Vec<f64> = memories.iter().map(|m| scorer.score(m)).collect();
        scorer.sort_by_importance(&mut memories);
        let scores_after: Vec<f64> = memories.iter().map(|m| scorer.score(m)).collect();

        // After sorting, scores should be descending
        for i in 0..scores_after.len() - 1 {
            assert!(
                scores_after[i] >= scores_after[i + 1],
                "scores should be sorted descending"
            );
        }
    }

    #[test]
    fn test_zero_entity_memory() {
        let scorer = ImportanceScorer::new();
        let mem = make_memory(7, 0, 0, 0.5, "dedup");
        let score = scorer.score(&mem);
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn test_critical_threshold() {
        let scorer = ImportanceScorer::new();
        let critical = make_memory(0, 5, 10, 0.99, "profile");
        let score = scorer.score(&critical);
        assert!(score > 0.80, "critical memory should score high, got {:.3}", score);
    }
}
