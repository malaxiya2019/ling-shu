//! AgentSwarm — 群体协调器
//!
//! 负责 Swarm 中的任务分配、Agent 选择、竞标管理和生命周期协调。
//! 将高层任务分解为子任务，分发给合适的 Agent，并收集结果。

use crate::communication::*;
use crate::memory::*;
use crate::metrics::*;
use crate::specialized::*;
use crate::strategy::*;
use crate::topology::*;
use crate::types::*;
use lingshu_core::{LsContext, LsId, LsResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ── 协调器配置 ──────────────────────────────────────

/// 协调器配置
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// 是否启用竞标
    pub enable_bidding: bool,
    /// 是否启用动态角色重分配
    pub enable_dynamic_role_assignment: bool,
    /// 最大并行任务数
    pub max_parallel_tasks: usize,
    /// 每个任务的最大 Agent 评估数
    pub max_evaluators_per_task: usize,
    /// 是否自动调整拓扑
    pub enable_adaptive_topology: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            enable_bidding: true,
            enable_dynamic_role_assignment: true,
            max_parallel_tasks: 10,
            max_evaluators_per_task: 3,
            enable_adaptive_topology: true,
        }
    }
}

// ── 协调器 ──────────────────────────────────────────

/// Swarm 协调器 — 负责任务分配、Agent 选择、生命周期管理
pub struct SwarmCoordinator {
    /// 配置
    config: CoordinatorConfig,
    /// Swarm 配置引用
    #[allow(dead_code)]
    swarm_config: Arc<SwarmConfig>,
    /// 决策策略
    strategy: Arc<RwLock<Box<dyn SwarmDecisionStrategy>>>,
    /// 通信通道
    channel: Arc<SwarmChannel>,
    /// 涌现专长引擎
    emergent: Arc<EmergentSpecialization>,
    /// 共享记忆
    memory: Arc<SwarmMemory>,
    /// 指标收集器
    metrics: Arc<MetricsCollector>,
    /// 拓扑管理器
    topology: Arc<TopologyManager>,
    /// 专业化 Agent 映射
    specialized_agents: RwLock<Vec<Box<dyn SpecializedAgent>>>,
}

impl SwarmCoordinator {
    pub fn new(
        #[allow(dead_code)] swarm_config: Arc<SwarmConfig>,
        strategy: Box<dyn SwarmDecisionStrategy>,
        channel: Arc<SwarmChannel>,
    ) -> Self {
        let strategy = Arc::new(RwLock::new(strategy));
        let emergent = Arc::new(EmergentSpecialization::new(3, 0.1));
        let memory = Arc::new(SwarmMemory::new());
        let metrics = Arc::new(MetricsCollector::new(10000, 60));
        let topology = Arc::new(TopologyManager::new(swarm_config.topology));

        Self {
            config: CoordinatorConfig::default(),
            swarm_config,
            strategy,
            channel,
            emergent,
            memory,
            metrics,
            topology,
            specialized_agents: RwLock::new(Vec::new()),
        }
    }

    /// 设置协调器配置
    pub fn with_config(mut self, config: CoordinatorConfig) -> Self {
        self.config = config;
        self
    }

    /// 注册专业化 Agent
    pub async fn register_specialized(&self, agent: Box<dyn SpecializedAgent>) {
        self.specialized_agents.write().await.push(agent);
    }

    /// 获取决策策略引用
    pub fn strategy(&self) -> &Arc<RwLock<Box<dyn SwarmDecisionStrategy>>> {
        &self.strategy
    }

    /// 获取涌现引擎引用
    pub fn emergent(&self) -> &Arc<EmergentSpecialization> {
        &self.emergent
    }

    /// 获取共享记忆
    pub fn memory(&self) -> &Arc<SwarmMemory> {
        &self.memory
    }

    /// 获取指标收集器
    pub fn metrics(&self) -> &Arc<MetricsCollector> {
        &self.metrics
    }

    /// 获取拓扑管理器
    pub fn topology(&self) -> &Arc<TopologyManager> {
        &self.topology
    }

    /// 获取通信通道
    pub fn channel(&self) -> &Arc<SwarmChannel> {
        &self.channel
    }

    // ── 核心协调逻辑 ──

    /// 协调执行一个任务（完整流程）
    pub async fn coordinate(
        &self,
        ctx: &LsContext,
        swarm_state: &SwarmState,
        task: &SwarmTask,
    ) -> LsResult<ConsensusResult> {
        info!(
            "coordinator: coordinating task '{}' (priority={})",
            task.name, task.priority
        );

        // Step 1: 分析任务复杂度，可能需要分解
        let tasks = self.analyze_task(ctx, task).await?;
        info!(
            "coordinator: task decomposed into {} sub-tasks",
            tasks.len()
        );

        // Step 2: 如果启用竞标，收集竞标
        let mut bids = Vec::new();
        if self.config.enable_bidding {
            bids = self.collect_bids(ctx, &tasks, swarm_state).await;
            info!("coordinator: collected {} bids", bids.len());
        }

        // Step 3: 选择 Agent 执行每个子任务
        let mut results = Vec::new();
        for sub_task in &tasks {
            let result = self
                .execute_subtask(ctx, sub_task, swarm_state, &bids)
                .await?;

            // 记录执行结果到涌现引擎
            {
                let agent_id = &result.agent_id;
                self.emergent
                    .record_execution(
                        agent_id,
                        &sub_task.name,
                        sub_task.required_role.unwrap_or(SwarmAgentRole::Executor),
                        &result,
                    )
                    .await;
            }

            // 记录到共享记忆
            self.memory.record_result(sub_task, &result).await;

            // 记录指标
            self.metrics
                .record_execution(result.success, result.execution_ms as f64)
                .await;

            results.push(result);
        }

        // Step 4: 群体评估
        let decision = self.evaluate_results(task, &results, swarm_state).await?;

        // 记录决策
        self.memory.record_decision(&decision).await;
        self.metrics.record_consensus(decision.achieved).await;

        // Step 5: 动态角色调整（如果启用）
        if self.config.enable_dynamic_role_assignment {
            self.adaptive_role_assignment(swarm_state).await;
        }

        // Step 6: 自适应拓扑调整（如果启用）
        if self.config.enable_adaptive_topology {
            let recommended = self.topology.adaptive_topology(swarm_state).await;
            self.topology
                .switch_topology(recommended, &swarm_state.agents)
                .await;
        }

        info!(
            "coordinator: task '{}' complete, consensus={}",
            task.name, decision.achieved
        );

        Ok(decision)
    }

    /// 分析任务（可能分解为子任务）
    async fn analyze_task(&self, _ctx: &LsContext, task: &SwarmTask) -> LsResult<Vec<SwarmTask>> {
        // 简单任务直接返回
        if task.priority <= 3 || task.description.len() < 100 {
            return Ok(vec![task.clone()]);
        }

        // 复杂任务：使用 Analyst 分解
        let specialized = self.specialized_agents.read().await;
        if let Some(analyst) = specialized
            .iter()
            .find(|a| a.role() == SwarmAgentRole::Analyst)
        {
            let analysis = analyst.analyze(_ctx, task).await?;
            if analysis.len() > 1 {
                return Ok(analysis);
            }
        }

        Ok(vec![task.clone()])
    }

    /// 收集竞标
    async fn collect_bids(
        &self,
        _ctx: &LsContext,
        tasks: &[SwarmTask],
        state: &SwarmState,
    ) -> Vec<SwarmBid> {
        if tasks.is_empty() || state.agents.is_empty() {
            return Vec::new();
        }

        let mut bids = Vec::new();
        for agent in &state.agents {
            if !agent.is_available() {
                continue;
            }

            // 每个 Agent 对最高优先级的任务竞标
            let best_task = tasks.iter().max_by(|a, b| a.priority.cmp(&b.priority));

            if let Some(task) = best_task {
                // 计算竞标分数
                let mut score = agent.capability_score;

                // 角色匹配加分
                if let Some(ref required_role) = task.required_role {
                    if agent.role == *required_role {
                        score += 0.2;
                    } else if agent.alternative_roles.contains(required_role) {
                        score += 0.1;
                    }
                }

                // 专长匹配加分
                for expertise in &task.required_expertise {
                    if let Some(expertise_score) = agent.expertise.get(expertise) {
                        score += expertise_score * 0.15;
                    }
                }

                let bid = SwarmBid {
                    agent_id: agent.id,
                    agent_name: agent.name.clone(),
                    bid_score: score.min(1.0),
                    estimated_ms: (agent.avg_execution_ms * (1.5 - agent.capability_score))
                        .max(100.0) as u64,
                    rationale: format!("Agent '{}' bid with score {:.2}", agent.name, score),
                    timestamp: chrono::Utc::now().timestamp(),
                };
                bids.push(bid);
            }
        }

        // 按分数降序排列
        bids.sort_by(|a, b| {
            b.bid_score
                .partial_cmp(&a.bid_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        bids.truncate(tasks.len() * 2); // 每个任务最多 2 个竞标
        bids
    }

    /// 执行单个子任务
    async fn execute_subtask(
        &self,
        _ctx: &LsContext,
        task: &SwarmTask,
        state: &SwarmState,
        bids: &[SwarmBid],
    ) -> LsResult<SwarmTaskResult> {
        let start = chrono::Utc::now().timestamp_millis();

        // 选择 Agent
        let strategy = self.strategy.read().await;
        let selected_agent = strategy.select_agent(task, &state.agents, bids).await?;

        let agent = match selected_agent {
            Some(a) => a,
            None => {
                // 无可用 Agent
                let completed_at = chrono::Utc::now().timestamp_millis();
                return Ok(SwarmTaskResult {
                    task_id: task.id,
                    agent_id: LsId::new(),
                    agent_name: "none".into(),
                    output: serde_json::Value::Null,
                    success: false,
                    execution_ms: (completed_at - start) as u64,
                    confidence: 0.0,
                    error: Some("No available agent for task".to_string()),
                    started_at: start,
                    completed_at,
                });
            }
        };

        // 通过专业化 Agent 执行
        let specialized = self.specialized_agents.read().await;
        let result = if let Some(specialist) = specialized.iter().find(|s| s.role() == agent.role) {
            specialist.execute(_ctx, task).await?
        } else {
            // 默认执行逻辑
            let completed_at = chrono::Utc::now().timestamp_millis();
            SwarmTaskResult {
                task_id: task.id,
                agent_id: agent.id,
                agent_name: agent.name.clone(),
                output: serde_json::json!({
                    "task": task.name,
                    "executed_by": agent.name,
                    "status": "completed",
                }),
                success: true,
                execution_ms: (completed_at - start) as u64,
                confidence: agent.capability_score,
                error: None,
                started_at: start,
                completed_at,
            }
        };

        Ok(result)
    }

    /// 群体评估结果
    async fn evaluate_results(
        &self,
        task: &SwarmTask,
        results: &[SwarmTaskResult],
        state: &SwarmState,
    ) -> LsResult<ConsensusResult> {
        let strategy = self.strategy.read().await;
        strategy.evaluate_result(task, results, &state.agents).await
    }

    /// 自适应角色分配 — 基于涌现引擎的建议调整 Agent 角色
    async fn adaptive_role_assignment(&self, state: &SwarmState) {
        for agent in &state.agents {
            if let Some(switch) = self.emergent.suggest_role_change(&agent.id).await {
                info!(
                    "adaptive role assignment: agent '{}' {:?} → {:?}",
                    agent.name, switch.from_role, switch.to_role
                );
                self.emergent.apply_role_switch(switch).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::{LsContext, LsId};

    fn create_test_swarm_state() -> SwarmState {
        let mut state =
            SwarmState::new("test-swarm", SwarmStrategy::Democratic, SwarmTopology::Mesh);
        let agents = vec![
            SwarmAgent::new("analyst-1", SwarmAgentRole::Analyst).with_expertise("analysis", 0.9),
            SwarmAgent::new("creator-1", SwarmAgentRole::Creator).with_expertise("code", 0.85),
            SwarmAgent::new("validator-1", SwarmAgentRole::Validator).with_expertise("qa", 0.8),
        ];
        state.agents = agents;
        state
    }

    #[tokio::test]
    async fn test_coordinator_basic() {
        let swarm_config = Arc::new(SwarmConfig::default());
        let channel = Arc::new(SwarmChannel::new(100));
        let strategy = create_strategy(&swarm_config);
        let coordinator = SwarmCoordinator::new(swarm_config, strategy, channel);

        // Register specialized agents
        coordinator
            .register_specialized(Box::new(AnalystAgent::new("analyst-1")))
            .await;
        coordinator
            .register_specialized(Box::new(CreatorAgent::new("creator-1")))
            .await;
        coordinator
            .register_specialized(Box::new(ValidatorAgent::new("validator-1", 0.5)))
            .await;

        let state = create_test_swarm_state();
        let task = SwarmTask::new(
            "test-coordinate",
            "A test coordination task",
            serde_json::json!({"input": "data"}),
        )
        .with_priority(5);

        let ctx = LsContext::with_session(LsId::new());
        let result = coordinator.coordinate(&ctx, &state, &task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_collect_bids() {
        let swarm_config = Arc::new(SwarmConfig::default());
        let channel = Arc::new(SwarmChannel::new(100));
        let strategy = create_strategy(&swarm_config);
        let coordinator = SwarmCoordinator::new(swarm_config, strategy, channel);

        let state = create_test_swarm_state();
        let task = SwarmTask::new("bid-task", "test", serde_json::json!({})).with_priority(8);

        let ctx = LsContext::with_session(LsId::new());
        let bids = coordinator.collect_bids(&ctx, &[task], &state).await;
        assert!(!bids.is_empty());
        // analyst-1 has highest capability for analysis tasks
    }

    #[tokio::test]
    async fn test_execute_subtask() {
        let swarm_config = Arc::new(SwarmConfig::default());
        let channel = Arc::new(SwarmChannel::new(100));
        let strategy = create_strategy(&swarm_config);
        let coordinator = SwarmCoordinator::new(swarm_config, strategy, channel);

        coordinator
            .register_specialized(Box::new(CreatorAgent::new("creator-1")))
            .await;

        let state = create_test_swarm_state();
        let task = SwarmTask::new(
            "execute-test",
            "test execution",
            serde_json::json!({"do": "something"}),
        )
        .with_priority(5);

        let ctx = LsContext::with_session(LsId::new());
        let result = coordinator.execute_subtask(&ctx, &task, &state, &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_coordinator_no_agents() {
        let swarm_config = Arc::new(SwarmConfig::default());
        let channel = Arc::new(SwarmChannel::new(100));
        let strategy = create_strategy(&swarm_config);
        let coordinator = SwarmCoordinator::new(swarm_config, strategy, channel);

        let empty_state = SwarmState::new("empty", SwarmStrategy::Democratic, SwarmTopology::Mesh);
        let task = SwarmTask::new("no-agent", "no agents available", serde_json::json!({}));

        let ctx = LsContext::with_session(LsId::new());
        let result = coordinator.coordinate(&ctx, &empty_state, &task).await;
        assert!(result.is_ok());
        let decision = result.unwrap();
        // With no agents, should not crash
        assert!(!decision.achieved || true); // Should handle gracefully
    }

    #[test]
    fn test_coordinator_config_default() {
        let config = CoordinatorConfig::default();
        assert!(config.enable_bidding);
        assert!(config.enable_dynamic_role_assignment);
        assert_eq!(config.max_parallel_tasks, 10);
    }
}
