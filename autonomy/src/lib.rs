//! LSAutonomy — Self-Reflection & Self-Evolution Engine (v5.0.3)
//!
//! Agent 自治 Runtime，提供自我反思和自我进化能力：
//!
//! # 核心概念
//!
//! - **ExperienceStore**: 经验存储，记录 Agent 的所有执行经验
//! - **ReflectionEngine**: 自我反思引擎，分析经验生成洞察
//! - **EvolutionEngine**: 自我进化引擎，根据洞察自动调整策略和参数
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │              AutonomyEngine                    │
//! │  ┌──────────────┐  ┌──────────────────┐     │
//! │  │ Reflection    │  │ Evolution         │     │
//! │  │ Engine        │──│ Engine            │     │
//! │  │ (反思)        │  │ (进化)            │     │
//! │  └──────┬───────┘  └────────┬─────────┘     │
//! │         │                    │                │
//! │         ▼                    ▼                │
//! │  ┌──────────────────────────────────┐        │
//! │  │         ExperienceStore           │        │
//! │  │         (经验存储)                │        │
//! │  └──────────────────────────────────┘        │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # 使用方法
//!
//! ```ignore
//! use lingshu_autonomy::*;
//!
//! let store = Arc::new(ExperienceStore::new(1000));
//! let reflection = Arc::new(ReflectionEngine::new(
//!     ReflectionConfig::default(), store.clone()));
//! let evolution = EvolutionEngine::new(
//!     EvolutionConfig::default(), store.clone(), reflection.clone());
//!
//! // 注册 Agent 参数
//! evolution.register_agent("agent-1", AgentParameters::new("agent-1")).await;
//!
//! // 存储经验
//! store.store(ExperienceEntry::new("agent-1", ExperienceType::TaskExecution,
//!     "任务完成", "ok", ExperienceOutcome::Success)).await;
//!
//! // 执行反思
//! let report = reflection.reflect("agent-1").await;
//!
//! // 执行进化
//! let outcomes = evolution.evolve("agent-1").await;
//! ```

pub mod evolution;
pub mod experience;
pub mod reflection;

pub use evolution::{
    AgentParameters, EvolutionAction, EvolutionConfig, EvolutionEngine, EvolutionOutcome,
    EvolutionPlan,
};
pub use experience::{
    ExperienceEntry, ExperienceOutcome, ExperienceSeverity, ExperienceStore, ExperienceSummary,
    ExperienceType,
};
pub use reflection::{
    InsightType, ReflectionConfig, ReflectionEngine, ReflectionInsight, ReflectionReport,
};

use std::sync::Arc;

/// 自治引擎版本
pub const VERSION: &str = "5.0.3";
pub const NAME: &str = "lingshu-autonomy";

/// AutonomyEngine — 自治引擎顶层入口
///
/// 整合经验存储、反思、进化为单一入口。
pub struct AutonomyEngine {
    /// 经验存储
    pub experience_store: Arc<ExperienceStore>,
    /// 反思引擎
    pub reflection_engine: Arc<ReflectionEngine>,
    /// 进化引擎
    pub evolution_engine: EvolutionEngine,
}

impl AutonomyEngine {
    /// 创建新的自治引擎
    pub fn new(
        reflection_config: ReflectionConfig,
        evolution_config: EvolutionConfig,
        max_experiences_per_agent: usize,
    ) -> Self {
        let store = Arc::new(ExperienceStore::new(max_experiences_per_agent));
        let reflection = Arc::new(ReflectionEngine::new(reflection_config, store.clone()));
        let evolution = EvolutionEngine::new(evolution_config, store.clone(), reflection.clone());

        Self {
            experience_store: store,
            reflection_engine: reflection,
            evolution_engine: evolution,
        }
    }

    /// 单步自治周期：存储经验 → 反思 → 进化
    pub async fn autonomy_cycle(
        &self,
        agent_id: &str,
        entry: ExperienceEntry,
    ) -> Vec<EvolutionOutcome> {
        // 1. 存储经验
        self.experience_store.store(entry).await;

        // 2. 反思（自动分析经验）
        let _report = self.reflection_engine.reflect(agent_id).await;

        // 3. 进化（基于洞察自动调整）
        self.evolution_engine.evolve(agent_id).await
    }

    /// 仅执行反思（不自动进化）
    pub async fn reflect_only(&self, agent_id: &str) -> ReflectionReport {
        self.reflection_engine.reflect(agent_id).await
    }

    /// 仅执行进化（基于已缓存的洞察）
    pub async fn evolve_only(&self, agent_id: &str) -> Vec<EvolutionOutcome> {
        self.evolution_engine.evolve(agent_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_autonomy_engine_creation() {
        let engine = AutonomyEngine::new(
            ReflectionConfig::default(),
            EvolutionConfig {
                auto_apply_threshold: 1,
                cooldown: std::time::Duration::from_secs(0),
                ..EvolutionConfig::default()
            },
            100,
        );

        assert!(engine
            .reflection_engine
            .reflect("test-agent")
            .await
            .insights
            .is_empty());
    }

    #[tokio::test]
    async fn test_autonomy_cycle() {
        let mut evo_config = EvolutionConfig::default();
        evo_config.auto_apply_threshold = 1;
        evo_config.cooldown = std::time::Duration::from_secs(0);

        let engine = AutonomyEngine::new(ReflectionConfig::default(), evo_config, 100);

        // Register agent
        engine
            .evolution_engine
            .register_agent("agent-1", AgentParameters::new("agent-1"))
            .await;

        // Add multiple failure experiences
        for i in 0..5 {
            let entry = ExperienceEntry::new(
                "agent-1",
                ExperienceType::TaskExecution,
                format!("fail-{}", i),
                "error",
                ExperienceOutcome::Failure("timeout".into()),
            )
            .with_tag("network-error");
            engine.experience_store.store(entry).await;
        }

        // Run autonomy cycle on the latest
        let entry = ExperienceEntry::new(
            "agent-1",
            ExperienceType::TaskExecution,
            "latest-fail",
            "error",
            ExperienceOutcome::Failure("timeout".into()),
        )
        .with_tag("network-error");

        let outcomes = engine.autonomy_cycle("agent-1", entry).await;
        assert!(!outcomes.is_empty());
    }

    #[test]
    fn test_version_constants() {
        assert_eq!(VERSION, "5.0.3");
        assert_eq!(NAME, "lingshu-autonomy");
    }
}
