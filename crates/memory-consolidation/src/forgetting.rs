//! ForgettingEngine — 记忆衰减与遗忘引擎。
//!
//! 模拟人脑的遗忘曲线：长期不用的记忆会逐渐衰减，
//! 低重要性的记忆会被自动清理或归档。
//!
//! # 架构
//!
//! ```text
//! ConsolidatedMemory 池
//!       │
//!       ▼
//!  DecayStrategy ← 可配置衰减策略
//!       │
//!   ┌───┴───┐
//!   │       │
//! 衰减    阈值
//! 评分    过滤
//!   │       │
//!   └───┬───┘
//!       ▼
//!  ForgettingEngine
//!       │
//!   ┌───┴───┐
//!   │       │
//!  清除    归档/压缩
//! (硬删除) (软删除)
//!       │
//!       ▼
//!  ForgettingReport
//! ```
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lingshu_memory_consolidation::{
//!     ForgettingEngine, ForgettingConfig, DecayStrategy,
//!     ImportanceScorer,
//! };
//!
//! let engine = ForgettingEngine::new(
//!     store.clone(),
//!     ForgettingConfig::default(),
//!     ImportanceScorer::new(),
//! );
//! let report = engine.run_forgetting().await.unwrap();
//! println!("遗忘了 {} 条记忆", report.evicted_count);
//! ```

use chrono::{DateTime, Utc};
use std::sync::Arc;
use tracing::{info, warn};

use crate::importance::ImportanceScorer;
use crate::store::ConsolidatedMemoryRepository;
use crate::types::ConsolidatedMemory;

// ═════════════════════════════════════════════════════════
// DecayStrategy
// ═════════════════════════════════════════════════════════

/// 衰减策略 — 决定哪些记忆应该被遗忘。
#[derive(Debug, Clone)]
pub enum DecayStrategy {
    /// 时间衰减：超过半衰期后重要性自然下降
    TimeDecay {
        /// 半衰期（小时）
        half_life_hours: f64,
    },
    /// 重要性阈值：低于此分数的记忆被遗忘
    ImportanceThreshold {
        /// 最低保留分数
        min_score: f64,
    },
    /// 访问频率：超过 N 天未被访问的记忆被遗忘
    AccessFrequency {
        /// 未访问 N 天后遗忘
        decay_if_unused_days: i64,
    },
    /// 上限淘汰：记忆总数超过上限时淘汰最低分的
    CapacityLimit {
        /// 最大记忆数
        max_count: usize,
    },
    /// 组合策略：满足任一条件即遗忘
    AnyOf(Vec<DecayStrategy>),
    /// 组合策略：满足所有条件才遗忘
    AllOf(Vec<DecayStrategy>),
}

impl DecayStrategy {
    /// 判断一条记忆是否应当被遗忘。
    pub fn should_forget(
        &self,
        memory: &ConsolidatedMemory,
        scorer: &ImportanceScorer,
        all_memories: &[ConsolidatedMemory],
    ) -> bool {
        match self {
            DecayStrategy::TimeDecay { half_life_hours } => {
                let age_hours = (Utc::now() - memory.created_at)
                    .num_hours()
                    .max(0) as f64;
                let decay_factor = (-age_hours / half_life_hours).exp();
                decay_factor < 0.05 // 衰减到 5% 以下
            }
            DecayStrategy::ImportanceThreshold { min_score } => {
                let score = scorer.score(memory);
                score < *min_score
            }
            DecayStrategy::AccessFrequency { decay_if_unused_days } => {
                let last_access = memory.metadata_last_access
                    .unwrap_or(memory.created_at);
                let days_since_access = (Utc::now() - last_access)
                    .num_days()
                    .max(0);
                days_since_access >= *decay_if_unused_days
            }
            DecayStrategy::CapacityLimit { max_count } => {
                if all_memories.len() <= *max_count {
                    return false;
                }
                // 按分数降序排列，保留前 max_count 条
                let mut scored: Vec<(usize, f64)> = all_memories
                    .iter()
                    .enumerate()
                    .map(|(i, m)| (i, scorer.score(m)))
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                // 如果 memory 不在 top-N 中，应被遗忘
                !scored
                    .iter()
                    .take(*max_count)
                    .any(|(idx, _)| all_memories[*idx].id == memory.id)
            }
            DecayStrategy::AnyOf(strategies) => {
                strategies
                    .iter()
                    .any(|s| s.should_forget(memory, scorer, all_memories))
            }
            DecayStrategy::AllOf(strategies) => {
                strategies
                    .iter()
                    .all(|s| s.should_forget(memory, scorer, all_memories))
            }
        }
    }
}

// ═════════════════════════════════════════════════════════
// ForgettingConfig
// ═════════════════════════════════════════════════════════

/// 遗忘引擎配置。
#[derive(Debug, Clone)]
pub struct ForgettingConfig {
    /// 衰减策略
    pub strategy: DecayStrategy,
    /// 是否启用归档（软删除）而非硬删除
    pub archive_enabled: bool,
    /// 归档标签前缀
    pub archive_tag_prefix: String,
    /// 每次运行最大处理数
    pub max_process: usize,
    /// 是否在巩固后自动运行遗忘
    pub auto_forget: bool,
}

impl Default for ForgettingConfig {
    fn default() -> Self {
        Self {
            strategy: DecayStrategy::AnyOf(vec![
                DecayStrategy::TimeDecay {
                    half_life_hours: 720.0, // 30 天半衰期
                },
                DecayStrategy::ImportanceThreshold {
                    min_score: 0.15,
                },
                DecayStrategy::CapacityLimit {
                    max_count: 10_000,
                },
            ]),
            archive_enabled: true,
            archive_tag_prefix: "archived".to_string(),
            max_process: 1000,
            auto_forget: true,
        }
    }
}

// ═════════════════════════════════════════════════════════
// ForgettingReport
// ═════════════════════════════════════════════════════════

/// 遗忘操作报告。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForgettingReport {
    /// 处理前的记忆总数
    pub total_before: usize,
    /// 处理后的记忆总数
    pub total_after: usize,
    /// 被淘汰（硬删除）的数量
    pub evicted_count: usize,
    /// 被归档（软删除）的数量
    pub archived_count: usize,
    /// 被压缩合并的数量
    pub compressed_count: usize,
    /// 处理前的平均重要性
    pub avg_importance_before: f64,
    /// 处理后的平均重要性
    pub avg_importance_after: f64,
    /// 执行耗时 (ms)
    pub execution_time_ms: u64,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 执行时间
    pub executed_at: DateTime<Utc>,
}

impl ForgettingReport {
    /// 创建空报告（无操作）。
    pub fn empty() -> Self {
        Self {
            total_before: 0,
            total_after: 0,
            evicted_count: 0,
            archived_count: 0,
            compressed_count: 0,
            avg_importance_before: 0.0,
            avg_importance_after: 0.0,
            execution_time_ms: 0,
            success: true,
            error: None,
            executed_at: Utc::now(),
        }
    }
}

// ═════════════════════════════════════════════════════════
// ForgettingEngine
// ═════════════════════════════════════════════════════════

/// 遗忘引擎 — 管理记忆生命周期。
pub struct ForgettingEngine {
    store: Arc<dyn ConsolidatedMemoryRepository>,
    config: ForgettingConfig,
    scorer: ImportanceScorer,
}

impl ForgettingEngine {
    /// 创建遗忘引擎。
    pub fn new(
        store: Arc<dyn ConsolidatedMemoryRepository>,
        config: ForgettingConfig,
        scorer: ImportanceScorer,
    ) -> Self {
        Self {
            store,
            config,
            scorer,
        }
    }

    /// 获取配置引用。
    pub fn config(&self) -> &ForgettingConfig {
        &self.config
    }

    /// 获取评分器引用。
    pub fn scorer(&self) -> &ImportanceScorer {
        &self.scorer
    }

    /// 运行一次遗忘流程。
    ///
    /// 1. 加载所有 consolidated memories
    /// 2. 计算每条记忆的重要性分数
    /// 3. 根据策略决定哪些应该被遗忘
    /// 4. 执行删除或归档
    pub async fn run_forgetting(&self) -> ForgettingReport {
        let start = std::time::Instant::now();

        let memories = match self.store.list_consolidated(self.config.max_process, 0).await {
            Ok(m) => m,
            Err(e) => {
                warn!("Forgetting: 无法加载记忆: {}", e);
                return ForgettingReport {
                    success: false,
                    error: Some(e.to_string()),
                    ..ForgettingReport::empty()
                };
            }
        };

        if memories.is_empty() {
            return ForgettingReport::empty();
        }

        let total_before = match self.store.count_consolidated().await {
            Ok(c) => c,
            Err(e) => {
                warn!("Forgetting: 无法获取记忆总数: {}", e);
                return ForgettingReport {
                    success: false,
                    error: Some(e.to_string()),
                    ..ForgettingReport::empty()
                };
            }
        };

        // 计算平均重要性（处理前）
        let avg_importance_before = if memories.is_empty() {
            0.0
        } else {
            memories.iter().map(|m| self.scorer.score(m)).sum::<f64>() / memories.len() as f64
        };

        // 逐条判断
        let mut evicted_count = 0usize;
        let mut archived_count = 0usize;

        for memory in &memories {
            if !self.config.strategy.should_forget(memory, &self.scorer, &memories) {
                continue;
            }

            if self.config.archive_enabled {
                // 归档：软删除，打上归档标签
                if let Err(e) = self
                    .store
                    .archive_consolidated(
                        &memory.id,
                        &format!("{}_{}", self.config.archive_tag_prefix, Utc::now().date_naive()),
                    )
                    .await
                {
                    warn!("Forgetting: 归档记忆 {} 失败: {}", memory.id, e);
                } else {
                    archived_count += 1;
                }
            } else {
                // 硬删除
                if let Err(e) = self.store.delete_consolidated(&memory.id).await {
                    warn!("Forgetting: 删除记忆 {} 失败: {}", memory.id, e);
                } else {
                    evicted_count += 1;
                }
            }
        }

        // 计算处理后的平均重要性（重新加载一次）
        let remaining = self.store.list_consolidated(self.config.max_process, 0)
            .await
            .unwrap_or_default();
        let total_after = total_before.saturating_sub(evicted_count + archived_count);
        let avg_importance_after = if remaining.is_empty() {
            0.0
        } else {
            remaining.iter().map(|m| self.scorer.score(m)).sum::<f64>() / remaining.len() as f64
        };

        info!(
            "Forgetting: 处理前 {} 条, 删除 {} 条, 归档 {} 条, 处理后 {} 条, 耗时 {}ms",
            total_before,
            evicted_count,
            archived_count,
            total_after,
            start.elapsed().as_millis(),
        );

        ForgettingReport {
            total_before,
            total_after,
            evicted_count,
            archived_count,
            compressed_count: 0,
            avg_importance_before,
            avg_importance_after,
            execution_time_ms: start.elapsed().as_millis() as u64,
            success: true,
            error: None,
            executed_at: Utc::now(),
        }
    }

    /// 预估遗忘结果（只算不改）。
    pub fn estimate_forgetting(
        &self,
        memories: &[ConsolidatedMemory],
    ) -> ForgettingEstimate {
        let total = memories.len();
        let to_forget: Vec<&ConsolidatedMemory> = memories
            .iter()
            .filter(|m| self.config.strategy.should_forget(m, &self.scorer, memories))
            .collect();

        let avg_importance: f64 = if memories.is_empty() {
            0.0
        } else {
            memories.iter().map(|m| self.scorer.score(m)).sum::<f64>() / memories.len() as f64
        };

        let forget_avg_importance: f64 = if to_forget.is_empty() {
            0.0
        } else {
            to_forget.iter().map(|m| self.scorer.score(m)).sum::<f64>() / to_forget.len() as f64
        };

        ForgettingEstimate {
            total_count: total,
            to_forget_count: to_forget.len(),
            to_remain_count: total - to_forget.len(),
            avg_importance,
            forget_avg_importance,
            forget_ids: to_forget.iter().map(|m| m.id.clone()).collect(),
        }
    }
}

/// 遗忘预估结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForgettingEstimate {
    /// 记忆总数
    pub total_count: usize,
    /// 将被遗忘的数量
    pub to_forget_count: usize,
    /// 保留的数量
    pub to_remain_count: usize,
    /// 总体平均重要性
    pub avg_importance: f64,
    /// 被遗忘部分平均重要性
    pub forget_avg_importance: f64,
    /// 将被遗忘的记忆 ID
    pub forget_ids: Vec<String>,
}

// ═════════════════════════════════════════════════════════
// 测试
// ═════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryConsolidatedStore;
    use crate::types::ConsolidatedMemory;

    fn make_memory(id: &str, days_ago: i64, confidence: f64) -> ConsolidatedMemory {
        let mut mem = ConsolidatedMemory::new(
            format!("记忆 {}", id),
            "测试记忆摘要",
            "summarize",
        )
        .with_confidence(confidence);
        mem.id = id.to_string();
        mem.created_at = Utc::now() - chrono::Duration::days(days_ago);
        mem
    }

    fn make_store() -> InMemoryConsolidatedStore {
        InMemoryConsolidatedStore::new()
    }

    // ── DecayStrategy 测试 ──

    #[test]
    fn test_time_decay_recent() {
        let strategy = DecayStrategy::TimeDecay { half_life_hours: 24.0 };
        let scorer = ImportanceScorer::new();
        let recent = make_memory("recent", 1, 0.9);
        assert!(!strategy.should_forget(&recent, &scorer, std::slice::from_ref(&recent)));
    }

    #[test]
    fn test_time_decay_very_old() {
        let strategy = DecayStrategy::TimeDecay { half_life_hours: 24.0 };
        let scorer = ImportanceScorer::new();
        let old = make_memory("old", 365, 0.9);
        // 365 天 >> 24 小时半衰期 → 衰减到接近 0
        assert!(strategy.should_forget(&old, &scorer, std::slice::from_ref(&old)));
    }

    #[test]
    fn test_importance_threshold() {
        let strategy = DecayStrategy::ImportanceThreshold { min_score: 0.3 };
        let scorer = ImportanceScorer::new();
        let high = make_memory("high", 1, 0.95);
        let low = make_memory("low", 365, 0.3);
        assert!(!strategy.should_forget(&high, &scorer, std::slice::from_ref(&high)));
        assert!(strategy.should_forget(&low, &scorer, std::slice::from_ref(&low)));
    }

    #[test]
    fn test_capacity_limit() {
        let strategy = DecayStrategy::CapacityLimit { max_count: 1 };
        let scorer = ImportanceScorer::new();

        let high = make_memory("high", 0, 0.99);
        let low = make_memory("low", 365, 0.3);

        let all = vec![high.clone(), low.clone()];
        // 容量 1，应淘汰分数最低的
        assert!(!strategy.should_forget(&high, &scorer, &all));
        assert!(strategy.should_forget(&low, &scorer, &all));
    }

    #[test]
    fn test_any_of() {
        // ImportanceThreshold(0.05) → only very low scores forgotten
        // TimeDecay(24h half life) → very old memories forgotten
        // AnyOf means: either condition triggers forget
        let strategy = DecayStrategy::AnyOf(vec![
            DecayStrategy::ImportanceThreshold { min_score: 0.15 },
            DecayStrategy::TimeDecay { half_life_hours: 24.0 * 30.0 },
        ]);
        let scorer = ImportanceScorer::new();
        let old_low = make_memory("ol", 365, 0.1);    // 旧 + 低分 → 两种都满足 → 遗忘
        let old_high = make_memory("oh", 365, 0.9);   // 旧（衰减到接近 0）→ 遗忘
        let recent_high = make_memory("rh", 1, 0.9);  // 新 + 高分 → 保留
        assert!(strategy.should_forget(&old_low, &scorer, &[old_low.clone(), old_high.clone(), recent_high.clone()]));
        assert!(strategy.should_forget(&old_high, &scorer, &[old_low.clone(), old_high.clone(), recent_high.clone()]));
        assert!(!strategy.should_forget(&recent_high, &scorer, &[old_low.clone(), old_high.clone(), recent_high.clone()]));
    }

    #[test]
    fn test_all_of() {
        // AllOf: 必须同时满足低分 AND 旧才遗忘
        let strategy = DecayStrategy::AllOf(vec![
            DecayStrategy::ImportanceThreshold { min_score: 0.15 },
            DecayStrategy::TimeDecay { half_life_hours: 24.0 * 30.0 },
        ]);
        let scorer = ImportanceScorer::new();
        let old_low = make_memory("ol", 365, 0.1);    // 既低分又旧 → 遗忘
        let old_high = make_memory("oh", 365, 0.9);   // 旧但高分 → 保留
        let recent_low = make_memory("rl", 1, 0.1);   // 低分但新 → 保留
        // AllOf: 必须同时低于 0.1 AND 旧
        // old_high 的 ImportanceScorer 评分约 0.185（仅 confidence + strategy）
        // recent_low 的 ImportanceScorer 评分约 0.185（recency 弥补了低 confidence）
        // old_low 的评分约 0.05（既旧又低 confidence）
        assert!(strategy.should_forget(&old_low, &scorer, &[old_low.clone(), old_high.clone(), recent_low.clone()]));
        assert!(!strategy.should_forget(&old_high, &scorer, &[old_low.clone(), old_high.clone(), recent_low.clone()]));
        assert!(!strategy.should_forget(&recent_low, &scorer, &[old_low.clone(), old_high.clone(), recent_low.clone()]));
    }

    // ── ForgettingEngine 测试 ──

    #[tokio::test]
    async fn test_empty_store() {
        let store = Arc::new(make_store()) as Arc<dyn ConsolidatedMemoryRepository>;
        let engine = ForgettingEngine::new(
            store,
            ForgettingConfig::default(),
            ImportanceScorer::new(),
        );
        let report = engine.run_forgetting().await;
        assert!(report.success);
        assert_eq!(report.total_before, 0);
    }

    #[tokio::test]
    async fn test_no_forget_needed() {
        let store = Arc::new(make_store()) as Arc<dyn ConsolidatedMemoryRepository>;
        store.store_consolidated(&make_memory("m1", 0, 0.99)).await.unwrap();
        store.store_consolidated(&make_memory("m2", 1, 0.95)).await.unwrap();

        let config = ForgettingConfig {
            strategy: DecayStrategy::ImportanceThreshold { min_score: 0.1 },
            archive_enabled: false,
            ..Default::default()
        };
        let engine = ForgettingEngine::new(store, config, ImportanceScorer::new());
        let report = engine.run_forgetting().await;
        assert_eq!(report.evicted_count, 0);
        assert_eq!(report.total_after, report.total_before);
    }

    #[tokio::test]
    async fn test_evict_low_value_no_archive() {
        let store = Arc::new(make_store()) as Arc<dyn ConsolidatedMemoryRepository>;
        store.store_consolidated(&make_memory("good", 0, 0.99)).await.unwrap();
        store.store_consolidated(&make_memory("bad", 365, 0.1)).await.unwrap();

        let config = ForgettingConfig {
            strategy: DecayStrategy::ImportanceThreshold { min_score: 0.3 },
            archive_enabled: false,
            ..Default::default()
        };
        let engine = ForgettingEngine::new(store.clone(), config, ImportanceScorer::new());
        let report = engine.run_forgetting().await;

        assert_eq!(report.evicted_count, 1);
        assert_eq!(report.archived_count, 0);
        assert_eq!(report.total_after, 1);

        // 确认 bad 已被删除
        let found = store.get_consolidated("bad").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_archive_instead_of_delete() {
        let store = Arc::new(make_store()) as Arc<dyn ConsolidatedMemoryRepository>;
        store.store_consolidated(&make_memory("old", 365, 0.1)).await.unwrap();

        let config = ForgettingConfig {
            strategy: DecayStrategy::TimeDecay { half_life_hours: 24.0 },
            archive_enabled: true,
            archive_tag_prefix: "archived".to_string(),
            ..Default::default()
        };
        let engine = ForgettingEngine::new(store.clone(), config, ImportanceScorer::new());
        let report = engine.run_forgetting().await;

        assert_eq!(report.archived_count, 1);
        assert_eq!(report.evicted_count, 0);

        // 确认 old 已被归档（依然存在但带归档标签）
        let found = store.get_consolidated("old").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_estimate_forgetting() {
        let store = InMemoryConsolidatedStore::new();
        store.store_consolidated(&make_memory("a", 0, 0.99)).await.unwrap();
        store.store_consolidated(&make_memory("b", 30, 0.6)).await.unwrap();
        store.store_consolidated(&make_memory("c", 365, 0.1)).await.unwrap();

        let all = store.list_consolidated(100, 0).await.unwrap();

        let engine = ForgettingEngine::new(
            Arc::new(store),
            ForgettingConfig::default(),
            ImportanceScorer::new(),
        );
        let estimate = engine.estimate_forgetting(&all);

        assert_eq!(estimate.total_count, 3);
        assert!(estimate.to_forget_count > 0);
        assert!(estimate.forget_avg_importance < estimate.avg_importance);
    }

    #[tokio::test]
    async fn test_capacity_limit_eviction() {
        let store = Arc::new(make_store()) as Arc<dyn ConsolidatedMemoryRepository>;
        for i in 0..10 {
            store.store_consolidated(&make_memory(
                &format!("m{}", i),
                i as i64 * 30, // m0 = 现在, m9 = 270 天前
                0.9 - (i as f64 * 0.08), // 越来越低
            )).await.unwrap();
        }

        let config = ForgettingConfig {
            strategy: DecayStrategy::CapacityLimit { max_count: 8 },
            archive_enabled: false,
            ..Default::default()
        };
        let engine = ForgettingEngine::new(store.clone(), config, ImportanceScorer::new());
        let _report = engine.run_forgetting().await;

        let remaining = store.list_consolidated(100, 0).await.unwrap();
        assert!(remaining.len() <= 10, "总数应减少或不变: {}", remaining.len());
    }

    #[tokio::test]
    async fn test_forgetting_idempotent() {
        let store = Arc::new(make_store()) as Arc<dyn ConsolidatedMemoryRepository>;
        store.store_consolidated(&make_memory("keep", 0, 0.99)).await.unwrap();
        store.store_consolidated(&make_memory("forget", 365, 0.1)).await.unwrap();

        let config = ForgettingConfig {
            strategy: DecayStrategy::ImportanceThreshold { min_score: 0.3 },
            archive_enabled: false,
            ..Default::default()
        };
        let engine = ForgettingEngine::new(store.clone(), config, ImportanceScorer::new());

        let r1 = engine.run_forgetting().await;
        assert_eq!(r1.evicted_count, 1);

        // 第二次运行不应再删除任何内容
        let r2 = engine.run_forgetting().await;
        assert_eq!(r2.evicted_count, 0);
    }

    #[test]
    fn test_estimate_with_empty() {
        let store = InMemoryConsolidatedStore::new();
        let engine = ForgettingEngine::new(
            Arc::new(store),
            ForgettingConfig::default(),
            ImportanceScorer::new(),
        );
        let estimate = engine.estimate_forgetting(&[]);
        assert_eq!(estimate.total_count, 0);
        assert_eq!(estimate.to_forget_count, 0);
    }

    #[test]
    fn test_access_frequency() {
        let strategy = DecayStrategy::AccessFrequency { decay_if_unused_days: 90 };
        let scorer = ImportanceScorer::new();

        let mut recently_used = make_memory("recent", 0, 0.9);
        recently_used.metadata_last_access = Some(Utc::now() - chrono::Duration::days(1));

        let mut unused = make_memory("unused", 0, 0.9);
        unused.metadata_last_access = Some(Utc::now() - chrono::Duration::days(180));

        assert!(!strategy.should_forget(&recently_used, &scorer, &[recently_used.clone(), unused.clone()]));
        assert!(strategy.should_forget(&unused, &scorer, &[recently_used.clone(), unused.clone()]));
    }

    #[test]
    fn test_estimate_order() {
        let strategy = DecayStrategy::ImportanceThreshold { min_score: 0.5 };
        let scorer = ImportanceScorer::new();

        let memories = vec![
            make_memory("a", 0, 0.99),
            make_memory("b", 1, 0.6),
            make_memory("c", 365, 0.1),
        ];

        let estimate = {
            let engine = ForgettingEngine::new(
                Arc::new(InMemoryConsolidatedStore::new()),
                ForgettingConfig { strategy, archive_enabled: false, ..Default::default() },
                scorer,
            );
            engine.estimate_forgetting(&memories)
        };

        assert_eq!(estimate.total_count, 3);
        assert_eq!(estimate.to_remain_count, estimate.total_count - estimate.to_forget_count);
    }
}
