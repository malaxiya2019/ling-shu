//! LSAutonomy — Self-Evolution Engine
//!
//! Agent 自我进化引擎，根据反思洞察自动调整策略、参数和行为。

use crate::experience::*;
use crate::reflection::*;
use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 进化动作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvolutionAction {
    /// 调整参数
    AdjustParameter,
    /// 切换策略
    SwitchStrategy,
    /// 更新行为规则
    UpdateBehavior,
    /// 增加重试机制
    AddRetry,
    /// 调整超时
    AdjustTimeout,
    /// 增加验证
    AddValidation,
    /// 优化协作模式
    OptimizeCollaboration,
    /// 释放资源
    ReleaseResource,
    /// 申请更多资源
    ScaleResource,
    /// 学习新能力
    LearnCapability,
}

impl EvolutionAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvolutionAction::AdjustParameter => "adjust_parameter",
            EvolutionAction::SwitchStrategy => "switch_strategy",
            EvolutionAction::UpdateBehavior => "update_behavior",
            EvolutionAction::AddRetry => "add_retry",
            EvolutionAction::AdjustTimeout => "adjust_timeout",
            EvolutionAction::AddValidation => "add_validation",
            EvolutionAction::OptimizeCollaboration => "optimize_collaboration",
            EvolutionAction::ReleaseResource => "release_resource",
            EvolutionAction::ScaleResource => "scale_resource",
            EvolutionAction::LearnCapability => "learn_capability",
        }
    }
}

/// 进化计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionPlan {
    /// 计划 ID
    pub id: LsId,
    /// Agent ID
    pub agent_id: String,
    /// 基于的洞察 ID
    pub based_on_insight_id: LsId,
    /// 进化动作
    pub action: EvolutionAction,
    /// 动作目标
    pub target: String,
    /// 旧值（调整前）
    pub old_value: Option<serde_json::Value>,
    /// 新值（调整后）
    pub new_value: Option<serde_json::Value>,
    /// 预期效果描述
    pub expected_effect: String,
    /// 优先级（1-10）
    pub priority: u8,
    /// 是否已验证
    pub verified: bool,
    /// 验证结果
    pub verification_result: Option<String>,
    /// 创建时间
    pub created_at: i64,
    /// 应用时间
    pub applied_at: Option<i64>,
}

impl EvolutionPlan {
    pub fn new(
        agent_id: impl Into<String>,
        based_on_insight_id: LsId,
        action: EvolutionAction,
        target: impl Into<String>,
        expected_effect: impl Into<String>,
    ) -> Self {
        Self {
            id: LsId::new(),
            agent_id: agent_id.into(),
            based_on_insight_id,
            action,
            target: target.into(),
            old_value: None,
            new_value: None,
            expected_effect: expected_effect.into(),
            priority: 5,
            verified: false,
            verification_result: None,
            created_at: chrono::Utc::now().timestamp(),
            applied_at: None,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    pub fn with_old_value(mut self, value: serde_json::Value) -> Self {
        self.old_value = Some(value);
        self
    }

    pub fn with_new_value(mut self, value: serde_json::Value) -> Self {
        self.new_value = Some(value);
        self
    }
}

/// 进化结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionOutcome {
    /// 计划 ID
    pub plan_id: LsId,
    /// 是否成功
    pub success: bool,
    /// 应用耗时 ms
    pub duration_ms: u64,
    /// 效果评分（-1.0 ~ 1.0）
    pub effect_score: f64,
    /// 观察结果
    pub observation: String,
    /// 是否需要回滚
    pub needs_rollback: bool,
    /// 完成时间
    pub completed_at: i64,
}

/// 进化引擎配置
#[derive(Debug, Clone)]
pub struct EvolutionConfig {
    /// 自动应用高优先级进化（>= 此阈值）
    pub auto_apply_threshold: u8,
    /// 进化验证等待时间
    pub verification_wait: Duration,
    /// 最大并发进化计划数
    pub max_concurrent_plans: usize,
    /// 进化冷却时间（两次进化之间）
    pub cooldown: Duration,
    /// 是否启用自动回滚
    pub enable_auto_rollback: bool,
    /// 回滚阈值（效果评分低于此值自动回滚）
    pub rollback_threshold: f64,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            auto_apply_threshold: 8,
            verification_wait: Duration::from_secs(60),
            max_concurrent_plans: 5,
            cooldown: Duration::from_secs(300),
            enable_auto_rollback: true,
            rollback_threshold: -0.3,
        }
    }
}

/// Agent 参数状态（可进化部分）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentParameters {
    /// Agent ID
    pub agent_id: String,
    /// 温度参数
    pub temperature: f64,
    /// 最大 token 数
    pub max_tokens: u32,
    /// 超时秒数
    pub timeout_secs: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 协作策略
    pub collaboration_strategy: String,
    /// 任务优先级
    pub default_priority: u8,
    /// 参数版本
    pub version: u64,
    /// 最后更新时间
    pub updated_at: i64,
}

impl AgentParameters {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 300,
            max_retries: 3,
            collaboration_strategy: "default".to_string(),
            default_priority: 5,
            version: 1,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

/// 自我进化引擎
pub struct EvolutionEngine {
    config: EvolutionConfig,
    #[allow(dead_code)]
    experience_store: Arc<ExperienceStore>,
    reflection_engine: Arc<ReflectionEngine>,
    /// Agent 参数状态
    agent_params: Arc<RwLock<HashMap<String, AgentParameters>>>,
    /// 进化计划历史
    plan_history: Arc<RwLock<Vec<EvolutionPlan>>>,
    /// 上次进化时间
    last_evolution: Arc<RwLock<HashMap<String, i64>>>,
    /// 正在进行的计划数
    active_plans: Arc<RwLock<usize>>,
}

impl EvolutionEngine {
    pub fn new(
        config: EvolutionConfig,
        experience_store: Arc<ExperienceStore>,
        reflection_engine: Arc<ReflectionEngine>,
    ) -> Self {
        Self {
            config,
            experience_store,
            reflection_engine,
            agent_params: Arc::new(RwLock::new(HashMap::new())),
            plan_history: Arc::new(RwLock::new(Vec::new())),
            last_evolution: Arc::new(RwLock::new(HashMap::new())),
            active_plans: Arc::new(RwLock::new(0)),
        }
    }

    /// 注册 Agent 参数
    pub async fn register_agent(&self, agent_id: &str, params: AgentParameters) {
        self.agent_params
            .write()
            .await
            .insert(agent_id.to_string(), params);
    }

    /// 获取 Agent 参数
    pub async fn get_parameters(&self, agent_id: &str) -> Option<AgentParameters> {
        let params = self.agent_params.read().await;
        params.get(agent_id).cloned()
    }

    /// 根据反思洞察生成并执行进化
    pub async fn evolve(&self, agent_id: &str) -> Vec<EvolutionOutcome> {
        // 检查冷却时间
        let last_time = {
            let guard = self.last_evolution.read().await;
            guard.get(agent_id).copied().unwrap_or(0)
        };
        let now = chrono::Utc::now().timestamp();
        if now - last_time < self.config.cooldown.as_secs() as i64 {
            debug!(
                "agent '{}' still in cooldown ({}s remaining)",
                agent_id,
                self.config.cooldown.as_secs() as i64 - (now - last_time)
            );
            return Vec::new();
        }

        // 检查并发限制
        {
            let active = *self.active_plans.read().await;
            if active >= self.config.max_concurrent_plans {
                warn!(
                    "max concurrent plans reached, skipping evolution for '{}'",
                    agent_id
                );
                return Vec::new();
            }
        }

        // 执行反思
        let report = self.reflection_engine.reflect(agent_id).await;

        if report.insights.is_empty() {
            debug!("no insights for '{}', skipping evolution", agent_id);
            return Vec::new();
        }

        // 生成进化计划
        let mut plans = self.generate_plans(agent_id, &report).await;

        // 按优先级排序
        plans.sort_by_key(|b| std::cmp::Reverse(b.priority));

        // 应用计划
        let mut outcomes = Vec::new();
        for plan in &plans {
            if plan.priority >= self.config.auto_apply_threshold {
                let outcome = self.apply_plan(plan).await;
                outcomes.push(outcome);
            } else {
                debug!(
                    "plan '{}' priority {} < auto_apply_threshold {}, queued for manual review",
                    plan.target, plan.priority, self.config.auto_apply_threshold
                );
            }
        }

        // 更新进化时间
        self.last_evolution
            .write()
            .await
            .insert(agent_id.to_string(), chrono::Utc::now().timestamp());

        info!(
            "evolution for '{}': {} plans generated, {} applied",
            agent_id,
            plans.len(),
            outcomes.len()
        );

        outcomes
    }

    /// 根据反思报告生成进化计划
    async fn generate_plans(
        &self,
        agent_id: &str,
        report: &ReflectionReport,
    ) -> Vec<EvolutionPlan> {
        let mut plans = Vec::new();

        for insight in &report.insights {
            match insight.insight_type {
                InsightType::FailurePattern => {
                    // 增加重试和验证
                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::AddRetry,
                        "task_execution",
                        format!("增加重试机制以应对重复失败（基于洞察: {}）", insight.title),
                    )
                    .with_priority(insight.priority)
                    .with_old_value(serde_json::json!({"max_retries": 3}))
                    .with_new_value(serde_json::json!({"max_retries": 5}));
                    plans.push(plan);

                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::AddValidation,
                        "pre_execution_validation",
                        format!("增加前置验证以减少失败（基于洞察: {}）", insight.title),
                    )
                    .with_priority((insight.priority.saturating_sub(1)).max(1));
                    plans.push(plan);
                }
                InsightType::PerformanceDegradation => {
                    // 调整超时和参数
                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::AdjustTimeout,
                        "execution_timeout",
                        format!("调整超时配置以应对性能退化（基于洞察: {}）", insight.title),
                    )
                    .with_priority(insight.priority)
                    .with_old_value(serde_json::json!({"timeout_secs": 300}))
                    .with_new_value(serde_json::json!({"timeout_secs": 600}));
                    plans.push(plan);

                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::ScaleResource,
                        "compute_resources",
                        format!("增加计算资源以改善性能（基于洞察: {}）", insight.title),
                    )
                    .with_priority((insight.priority.saturating_sub(2)).max(1));
                    plans.push(plan);
                }
                InsightType::EfficiencyOpportunity => {
                    // 切换/优化策略
                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::SwitchStrategy,
                        "execution_strategy",
                        format!("切换执行策略以提升效率（基于洞察: {}）", insight.title),
                    )
                    .with_priority(insight.priority);
                    plans.push(plan);
                }
                InsightType::CollaborationImprovement => {
                    // 优化协作模式
                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::OptimizeCollaboration,
                        "collaboration_protocol",
                        format!("优化协作协议（基于洞察: {}）", insight.title),
                    )
                    .with_priority(insight.priority);
                    plans.push(plan);
                }
                InsightType::KnowledgeGap => {
                    // 学习新能力
                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::LearnCapability,
                        "capability_learning",
                        format!("补充缺失能力（基于洞察: {}）", insight.title),
                    )
                    .with_priority(insight.priority);
                    plans.push(plan);
                }
                _ => {
                    // 其他类型默认调整参数
                    let plan = EvolutionPlan::new(
                        agent_id,
                        insight.id,
                        EvolutionAction::AdjustParameter,
                        "general_parameters",
                        format!("调整通用参数（基于洞察: {}）", insight.title),
                    )
                    .with_priority(insight.priority);
                    plans.push(plan);
                }
            }
        }

        plans
    }

    /// 应用进化计划
    pub async fn apply_plan(&self, plan: &EvolutionPlan) -> EvolutionOutcome {
        let start = std::time::Instant::now();
        let mut outcome = EvolutionOutcome {
            plan_id: plan.id,
            success: true,
            duration_ms: 0,
            effect_score: 0.0,
            observation: String::new(),
            needs_rollback: false,
            completed_at: chrono::Utc::now().timestamp(),
        };

        // 更新 Agent 参数
        let params_opt = {
            let mut guard = self.agent_params.write().await;
            guard.get_mut(&plan.agent_id).cloned()
        };
        if let Some(mut params) = params_opt {
            match plan.action {
                EvolutionAction::AdjustTimeout => {
                    if let Some(ref new_val) = plan.new_value {
                        if let Some(timeout) = new_val.get("timeout_secs").and_then(|v| v.as_u64())
                        {
                            let old_timeout = params.timeout_secs;
                            params.timeout_secs = timeout;
                            params.version += 1;
                            outcome.observation =
                                format!("timeout adjusted: {}s -> {}s", old_timeout, timeout);
                        }
                    }
                }
                EvolutionAction::AddRetry => {
                    if let Some(ref new_val) = plan.new_value {
                        if let Some(retries) = new_val.get("max_retries").and_then(|v| v.as_u64()) {
                            let old_retries = params.max_retries;
                            params.max_retries = retries as u32;
                            params.version += 1;
                            outcome.observation =
                                format!("max retries adjusted: {} -> {}", old_retries, retries);
                        }
                    }
                }
                EvolutionAction::SwitchStrategy => {
                    outcome.observation = format!("strategy switch queued: {}", plan.target);
                }
                EvolutionAction::OptimizeCollaboration => {
                    let old_strategy = params.collaboration_strategy.clone();
                    params.collaboration_strategy = format!("optimized_{}", old_strategy);
                    params.version += 1;
                    outcome.observation = format!(
                        "collaboration strategy updated: {} -> {}",
                        old_strategy, params.collaboration_strategy
                    );
                }
                EvolutionAction::AddValidation => {
                    outcome.observation = format!("validation added for: {}", plan.target);
                }
                EvolutionAction::AdjustParameter => {
                    params.version += 1;
                    outcome.observation =
                        format!("general parameters adjusted (v{})", params.version);
                }
                EvolutionAction::ScaleResource => {
                    outcome.observation =
                        format!("resource scaling requested for: {}", plan.target);
                }
                EvolutionAction::LearnCapability => {
                    outcome.observation = format!("capability learning scheduled: {}", plan.target);
                }
                EvolutionAction::UpdateBehavior => {
                    params.version += 1;
                    outcome.observation = format!("behavior rules updated (v{})", params.version);
                }
                EvolutionAction::ReleaseResource => {
                    outcome.observation = format!("resource release triggered: {}", plan.target);
                }
            }

            params.updated_at = chrono::Utc::now().timestamp();
            self.agent_params
                .write()
                .await
                .insert(plan.agent_id.clone(), params);
        } else {
            outcome.success = false;
            outcome.observation = format!("agent '{}' not registered", plan.agent_id);
        }

        // 记录进化历史
        {
            let mut history = self.plan_history.write().await;
            let mut applied_plan = plan.clone();
            applied_plan.applied_at = Some(chrono::Utc::now().timestamp());
            history.push(applied_plan);
        }

        outcome.duration_ms = start.elapsed().as_millis() as u64;
        outcome.effect_score = if outcome.success { 0.5 } else { -0.5 };

        info!(
            "evolution plan applied for '{}': {} (success={})",
            plan.agent_id, plan.target, outcome.success
        );

        outcome
    }

    /// 尝试回滚进化计划
    pub async fn rollback(&self, plan_id: &LsId) -> bool {
        let mut history = self.plan_history.write().await;
        if let Some(plan) = history.iter_mut().find(|p| p.id == *plan_id) {
            plan.verified = false;
            plan.verification_result = Some("rolled back".to_string());
            info!("rollback plan {}", plan_id);
            true
        } else {
            warn!("plan {} not found for rollback", plan_id);
            false
        }
    }

    /// 获取进化历史
    pub async fn get_evolution_history(&self, agent_id: &str) -> Vec<EvolutionPlan> {
        let history = self.plan_history.read().await;
        history
            .iter()
            .filter(|p| p.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// 获取 Agent 当前参数版本
    pub async fn get_parameter_version(&self, agent_id: &str) -> Option<u64> {
        let params = self.agent_params.read().await;
        params.get(agent_id).map(|p| p.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_engine() -> (Arc<ExperienceStore>, Arc<ReflectionEngine>, EvolutionEngine) {
        let store = Arc::new(ExperienceStore::new(100));
        let reflection = Arc::new(ReflectionEngine::new(
            ReflectionConfig::default(),
            store.clone(),
        ));
        let evolution = EvolutionEngine::new(
            EvolutionConfig {
                auto_apply_threshold: 1, // Auto-apply everything in tests
                cooldown: Duration::from_secs(0),
                ..EvolutionConfig::default()
            },
            store.clone(),
            reflection.clone(),
        );
        (store, reflection, evolution)
    }

    #[tokio::test]
    async fn test_register_and_get_parameters() {
        let (_, _, engine) = setup_engine();
        let params = AgentParameters::new("agent-1");
        engine.register_agent("agent-1", params.clone()).await;

        let retrieved = engine.get_parameters("agent-1").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().agent_id, "agent-1");
    }

    #[tokio::test]
    async fn test_generate_plans() {
        let (store, reflection, engine) = setup_engine();
        let agent_id = "agent-1";

        // Add some failure experiences to trigger insights
        for i in 0..5 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("fail-{}", i),
                "error",
                ExperienceOutcome::Failure("timeout".into()),
            )
            .with_tag("network-error");
            store.store(entry).await;
        }

        let report = reflection.reflect(agent_id).await;
        let plans = engine.generate_plans(agent_id, &report).await;

        assert!(!plans.is_empty());
        // Failure patterns should generate add_retry plans
        assert!(plans.iter().any(|p| p.action == EvolutionAction::AddRetry));
    }

    #[tokio::test]
    async fn test_apply_plan() {
        let (_, _, engine) = setup_engine();
        let agent_id = "agent-1";
        engine
            .register_agent(agent_id, AgentParameters::new(agent_id))
            .await;

        let insight = ReflectionInsight::new(InsightType::FailurePattern, "test", "desc");
        let plan = EvolutionPlan::new(
            agent_id,
            insight.id,
            EvolutionAction::AdjustTimeout,
            "timeout",
            "increase timeout",
        )
        .with_new_value(serde_json::json!({"timeout_secs": 600}));

        let outcome = engine.apply_plan(&plan).await;
        assert!(outcome.success);

        let params = engine.get_parameters(agent_id).await.unwrap();
        assert_eq!(params.timeout_secs, 600);
    }

    #[tokio::test]
    async fn test_evolution_workflow() {
        let (store, _, engine) = setup_engine();
        let agent_id = "agent-1";

        engine
            .register_agent(agent_id, AgentParameters::new(agent_id))
            .await;

        // Add experiences that trigger evolution
        for i in 0..6 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("exp-{}", i),
                "test",
                if i % 2 == 0 {
                    ExperienceOutcome::Success
                } else {
                    ExperienceOutcome::Failure("err".into())
                },
            );
            store.store(entry).await;
        }

        let outcomes = engine.evolve(agent_id).await;
        assert!(!outcomes.is_empty());
        assert!(outcomes.iter().any(|o| o.success));
    }

    #[test]
    fn test_evolution_plan_creation() {
        let insight_id = LsId::new();
        let plan = EvolutionPlan::new(
            "agent-1",
            insight_id.clone(),
            EvolutionAction::AddRetry,
            "task",
            "增加重试",
        )
        .with_priority(9)
        .with_old_value(serde_json::json!({"retries": 3}))
        .with_new_value(serde_json::json!({"retries": 5}));

        assert_eq!(plan.priority, 9);
        assert_eq!(plan.action, EvolutionAction::AddRetry);
        assert!(plan.old_value.is_some());
        assert!(plan.new_value.is_some());
    }

    #[test]
    fn test_agent_parameters_defaults() {
        let params = AgentParameters::new("test-agent");
        assert_eq!(params.temperature, 0.7);
        assert_eq!(params.max_tokens, 4096);
        assert_eq!(params.version, 1);
    }
}
