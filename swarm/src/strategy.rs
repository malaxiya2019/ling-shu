//! AgentSwarm — 群体智能协作策略
//!
//! 提供多种群体决策机制：
//! - Voting: 投票制，多数决定
//! - Consensus: 共识制，寻求全体/多数同意
//! - Hierarchical: 层级制，Leader 决策
//! - Democratic: 民主制，平等参与
//! - Bidding: 竞标制，最优报价胜出
//! - Hybrid: 混合制，根据任务动态切换

use crate::types::*;
use async_trait::async_trait;
use lingshu_core::LsResult;
use tracing::debug;

// ── Strategy Trait ──────────────────────────────────

/// 群体决策策略接口
#[async_trait]
pub trait SwarmDecisionStrategy: Send + Sync {
    /// 策略名称
    fn name(&self) -> &'static str;
    /// 策略类型
    fn strategy_type(&self) -> SwarmStrategy;
    /// 选择一个 Agent 执行任务
    async fn select_agent(
        &self,
        task: &SwarmTask,
        agents: &[SwarmAgent],
        bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>>;
    /// 对结果进行群体评估
    async fn evaluate_result(
        &self,
        task: &SwarmTask,
        results: &[SwarmTaskResult],
        agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult>;
    /// 是否需要竞标环节
    fn needs_bidding(&self) -> bool {
        false
    }
}

// ── 投票制策略 ─────────────────────────────────────

/// 投票制策略：所有相关 Agent 投票，多数通过
pub struct VotingStrategy {
    threshold: f64, // 通过阈值，如 0.5 表示简单多数
}

impl VotingStrategy {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl Default for VotingStrategy {
    fn default() -> Self {
        Self { threshold: 0.5 }
    }
}

#[async_trait]
impl SwarmDecisionStrategy for VotingStrategy {
    fn name(&self) -> &'static str {
        "voting"
    }

    fn strategy_type(&self) -> SwarmStrategy {
        SwarmStrategy::Voting
    }

    async fn select_agent(
        &self,
        task: &SwarmTask,
        agents: &[SwarmAgent],
        _bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>> {
        // 找到最适合的 Agent（基于能力评分 + 专长匹配）
        let available: Vec<&SwarmAgent> = agents.iter().filter(|a| a.is_available()).collect();
        if available.is_empty() {
            return Ok(None);
        }

        let best = available
            .into_iter()
            .max_by(|a, b| {
                let a_score = Self::calculate_fitness(a, task);
                let b_score = Self::calculate_fitness(b, task);
                a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
            });

        Ok(best.cloned())
    }

    async fn evaluate_result(
        &self,
        _task: &SwarmTask,
        results: &[SwarmTaskResult],
        agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult> {
        if results.is_empty() || agents.is_empty() {
            return Ok(ConsensusResult {
                achieved: false,
                total_votes: 0,
                yes_votes: 0,
                no_votes: 0,
                abstain_votes: 0,
                approval_ratio: 0.0,
                decision: serde_json::Value::Null,
                minority_opinions: Vec::new(),
            });
        }

        // Agent 根据置信度投票
        let mut yes = 0usize;
        let mut no = 0usize;
        let mut abstain = 0usize;
        let mut minority_opinions = Vec::new();

        for result in results {
            if result.confidence >= 0.7 {
                yes += 1;
            } else if result.confidence >= 0.4 {
                abstain += 1;
            } else {
                no += 1;
                if let Some(ref err) = result.error {
                    minority_opinions.push(format!("Agent {}: {}", result.agent_name, err));
                }
            }
        }

        let total = yes + no + abstain;
        let ratio = if total > 0 { yes as f64 / total as f64 } else { 0.0 };
        let achieved = ratio >= self.threshold;

        // 选择置信度最高的结果作为决策
        let decision = results
            .iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .map(|r| r.output.clone())
            .unwrap_or(serde_json::Value::Null);

        Ok(ConsensusResult {
            achieved,
            total_votes: total,
            yes_votes: yes,
            no_votes: no,
            abstain_votes: abstain,
            approval_ratio: ratio,
            decision,
            minority_opinions,
        })
    }
}

impl VotingStrategy {
    fn calculate_fitness(agent: &SwarmAgent, task: &SwarmTask) -> f64 {
        let mut score = agent.capability_score;
        // 专长匹配加分
        for expertise in &task.required_expertise {
            if let Some(expertise_score) = agent.expertise.get(expertise) {
                score += expertise_score * 0.2;
            }
        }
        // 角色匹配加分
        if let Some(ref required_role) = task.required_role {
            if agent.role == *required_role {
                score += 0.3;
            } else if agent.alternative_roles.contains(required_role) {
                score += 0.15;
            }
        }
        score.min(1.0)
    }
}

// ── 共识制策略 ─────────────────────────────────────

/// 共识制策略：寻求 2/3 多数同意
pub struct ConsensusStrategy {
    threshold: f64,
}

impl ConsensusStrategy {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl Default for ConsensusStrategy {
    fn default() -> Self {
        Self { threshold: 0.67 }
    }
}

#[async_trait]
impl SwarmDecisionStrategy for ConsensusStrategy {
    fn name(&self) -> &'static str {
        "consensus"
    }

    fn strategy_type(&self) -> SwarmStrategy {
        SwarmStrategy::Consensus
    }

    async fn select_agent(
        &self,
        _task: &SwarmTask,
        agents: &[SwarmAgent],
        _bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>> {
        // Consensus 策略下，优先选成功率高的 Agent
        let available: Vec<&SwarmAgent> = agents.iter().filter(|a| a.is_available()).collect();
        if available.is_empty() {
            return Ok(None);
        }

        let best = available
            .into_iter()
            .max_by(|a, b| {
                let a_score = a.success_rate * 0.6 + a.capability_score * 0.4;
                let b_score = b.success_rate * 0.6 + b.capability_score * 0.4;
                a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
            });

        Ok(best.cloned())
    }

    async fn evaluate_result(
        &self,
        _task: &SwarmTask,
        results: &[SwarmTaskResult],
        agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult> {
        if results.is_empty() || agents.is_empty() {
            return Ok(ConsensusResult {
                achieved: false,
                total_votes: 0,
                yes_votes: 0,
                no_votes: 0,
                abstain_votes: 0,
                approval_ratio: 0.0,
                decision: serde_json::Value::Null,
                minority_opinions: Vec::new(),
            });
        }

        // 基于置信度和结果一致性计算共识
        let mut yes = 0usize;
        let mut no = 0usize;
        let mut abstain = 0usize;

        // 分析结果相似度——如果 Agent 输出相似，增加共识概率
        let output_strs: Vec<String> = results
            .iter()
            .map(|r| serde_json::to_string(&r.output).unwrap_or_default())
            .collect();

        for (i, result) in results.iter().enumerate() {
            // 检查与其他结果的一致性
            let mut agreements = 0;
            for (j, other) in output_strs.iter().enumerate() {
                if i != j && output_strs[i] == *other {
                    agreements += 1;
                }
            }
            let consistency = if results.len() > 1 {
                agreements as f64 / (results.len() - 1) as f64
            } else {
                1.0
            };

            // 综合置信度和一致性得出投票
            let combined = result.confidence * 0.6 + consistency * 0.4;
            if combined >= 0.7 {
                yes += 1;
            } else if combined >= 0.4 {
                abstain += 1;
            } else {
                no += 1;
            }
        }

        let total = yes + no + abstain;
        let ratio = if total > 0 { yes as f64 / total as f64 } else { 0.0 };
        let achieved = ratio >= self.threshold;

        let decision = results
            .iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .map(|r| r.output.clone())
            .unwrap_or(serde_json::Value::Null);

        let minority_opinions: Vec<String> = results
            .iter()
            .filter(|r| r.confidence < 0.4)
            .map(|r| format!("Agent {} (confidence={:.2})", r.agent_name, r.confidence))
            .collect();

        Ok(ConsensusResult {
            achieved,
            total_votes: total,
            yes_votes: yes,
            no_votes: no,
            abstain_votes: abstain,
            approval_ratio: ratio,
            decision,
            minority_opinions,
        })
    }
}

// ── 层级制策略 ─────────────────────────────────────

/// 层级制策略：Leader 决策，Worker 执行
pub struct HierarchicalStrategy;

#[async_trait]
impl SwarmDecisionStrategy for HierarchicalStrategy {
    fn name(&self) -> &'static str {
        "hierarchical"
    }

    fn strategy_type(&self) -> SwarmStrategy {
        SwarmStrategy::Hierarchical
    }

    async fn select_agent(
        &self,
        task: &SwarmTask,
        agents: &[SwarmAgent],
        _bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>> {
        // 层级制：找 Planner 或 Router 分配任务给 Executor
        // 这里直接找能力最强的可用 Agent
        let available: Vec<&SwarmAgent> = agents.iter().filter(|a| a.is_available()).collect();
        if available.is_empty() {
            return Ok(None);
        }

        // 优先匹配所需角色
        if let Some(ref required_role) = task.required_role {
            if let Some(matched) = available.iter().find(|a| a.role == *required_role) {
                return Ok(Some((*matched).clone()));
            }
        }

        // 否则取能力最强的
        let best = available
            .into_iter()
            .max_by(|a, b| {
                a.capability_score
                    .partial_cmp(&b.capability_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        Ok(best.cloned())
    }

    async fn evaluate_result(
        &self,
        _task: &SwarmTask,
        results: &[SwarmTaskResult],
        _agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult> {
        // 层级制：Leader 直接接受结果，无需投票
        if results.is_empty() {
            return Ok(ConsensusResult {
                achieved: false,
                total_votes: 0,
                yes_votes: 0,
                no_votes: 0,
                abstain_votes: 0,
                approval_ratio: 0.0,
                decision: serde_json::Value::Null,
                minority_opinions: Vec::new(),
            });
        }

        // 取置信度最高的结果
        let best = results
            .iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        Ok(ConsensusResult {
            achieved: best.success,
            total_votes: 1,
            yes_votes: if best.success { 1 } else { 0 },
            no_votes: if best.success { 0 } else { 1 },
            abstain_votes: 0,
            approval_ratio: if best.success { 1.0 } else { 0.0 },
            decision: best.output.clone(),
            minority_opinions: Vec::new(),
        })
    }
}

// ── 民主制策略 ─────────────────────────────────────

/// 民主制策略：所有 Agent 平等参与
pub struct DemocraticStrategy;

#[async_trait]
impl SwarmDecisionStrategy for DemocraticStrategy {
    fn name(&self) -> &'static str {
        "democratic"
    }

    fn strategy_type(&self) -> SwarmStrategy {
        SwarmStrategy::Democratic
    }

    async fn select_agent(
        &self,
        _task: &SwarmTask,
        agents: &[SwarmAgent],
        bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>> {
        // 民主制：用竞标结果选择 Agent
        if !bids.is_empty() {
            let best_bid = bids
                .iter()
                .max_by(|a, b| a.bid_score.partial_cmp(&b.bid_score).unwrap_or(std::cmp::Ordering::Equal));

            if let Some(bid) = best_bid {
                if let Some(agent) = agents.iter().find(|a| a.id == bid.agent_id) {
                    return Ok(Some(agent.clone()));
                }
            }
        }

        // Fallback: 轮询（Round Robin）
        let available: Vec<&SwarmAgent> = agents.iter().filter(|a| a.is_available()).collect();
        if available.is_empty() {
            return Ok(None);
        }

        // 取任务最少的 Agent（负载均衡）
        let best = available
            .into_iter()
            .min_by(|a, b| {
                a.tasks_completed.cmp(&b.tasks_completed)
            });

        Ok(best.cloned())
    }

    fn needs_bidding(&self) -> bool {
        true
    }

    async fn evaluate_result(
        &self,
        task: &SwarmTask,
        results: &[SwarmTaskResult],
        agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult> {
        // 民主制：简单多数投票
        let voter = VotingStrategy::new(0.5);
        voter.evaluate_result(task, results, agents).await
    }
}

// ── 竞标制策略 ─────────────────────────────────────

/// 竞标制策略：Agent 竞标，最优报价胜出
pub struct BiddingStrategy;

#[async_trait]
impl SwarmDecisionStrategy for BiddingStrategy {
    fn name(&self) -> &'static str {
        "bidding"
    }

    fn strategy_type(&self) -> SwarmStrategy {
        SwarmStrategy::Bidding
    }

    async fn select_agent(
        &self,
        _task: &SwarmTask,
        _agents: &[SwarmAgent],
        bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>> {
        if bids.is_empty() {
            return Ok(None);
        }

        // 综合评分：bid_score * (1 - estimated_ms/max_ms)
        let max_ms = bids.iter().map(|b| b.estimated_ms).max().unwrap_or(1).max(1);
        let best = bids
            .iter()
            .max_by(|a, b| {
                let a_score = a.bid_score * (1.0 - a.estimated_ms as f64 / max_ms as f64);
                let b_score = b.bid_score * (1.0 - b.estimated_ms as f64 / max_ms as f64);
                a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
            });

        // 注意：返回的 Agent 信息只能从 bid 构建，因为 agents 可能在远程
        // 这里返回 None，由 Coordinator 根据 agent_id 查找完整 Agent
        Ok(best.map(|b| SwarmAgent {
            id: b.agent_id.clone(),
            name: b.agent_name.clone(),
            role: SwarmAgentRole::Executor,
            alternative_roles: Vec::new(),
            status: SwarmAgentStatus::Idle,
            capability_score: b.bid_score,
            success_rate: 0.5,
            tasks_completed: 0,
            avg_execution_ms: 0.0,
            expertise: std::collections::HashMap::new(),
            last_active: b.timestamp,
            joined_at: b.timestamp,
            metadata: std::collections::HashMap::new(),
        }))
    }

    fn needs_bidding(&self) -> bool {
        true
    }

    async fn evaluate_result(
        &self,
        _task: &SwarmTask,
        results: &[SwarmTaskResult],
        _agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult> {
        // 竞标制：执行结果即为最终结果
        if results.is_empty() {
            return Ok(ConsensusResult {
                achieved: false,
                total_votes: 0,
                yes_votes: 0,
                no_votes: 0,
                abstain_votes: 0,
                approval_ratio: 0.0,
                decision: serde_json::Value::Null,
                minority_opinions: Vec::new(),
            });
        }

        let result = &results[0];
        Ok(ConsensusResult {
            achieved: result.success,
            total_votes: 1,
            yes_votes: if result.success { 1 } else { 0 },
            no_votes: if result.success { 0 } else { 1 },
            abstain_votes: 0,
            approval_ratio: if result.success { 1.0 } else { 0.0 },
            decision: result.output.clone(),
            minority_opinions: Vec::new(),
        })
    }
}

// ── 混合制策略 ─────────────────────────────────────

/// 混合制策略：根据任务类型动态选择最佳策略
pub struct HybridStrategy {
    strategies: Vec<Box<dyn SwarmDecisionStrategy>>,
}

impl HybridStrategy {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
        }
    }

    pub fn with_strategy(mut self, strategy: Box<dyn SwarmDecisionStrategy>) -> Self {
        self.strategies.push(strategy);
        self
    }

    fn select_strategy(&self, task: &SwarmTask) -> &dyn SwarmDecisionStrategy {
        // 根据任务特征选择策略：
        // - 高优先级 + 需要创意 → Bidding
        // - 需要精确验证 → Voting/Consensus
        // - 简单执行 → Hierarchical
        // - 默认 → Democratic

        if task.priority >= 8 && task.required_role == Some(SwarmAgentRole::Creator) {
            self.strategies
                .iter()
                .find(|s| s.strategy_type() == SwarmStrategy::Bidding)
                .map(|s| s.as_ref())
                .unwrap_or_else(|| self.strategies.first().map(|s| s.as_ref()).unwrap())
        } else if task.required_role == Some(SwarmAgentRole::Validator) || task.required_role == Some(SwarmAgentRole::Tester) {
            self.strategies
                .iter()
                .find(|s| s.strategy_type() == SwarmStrategy::Consensus)
                .map(|s| s.as_ref())
                .unwrap_or_else(|| self.strategies.first().map(|s| s.as_ref()).unwrap())
        } else if task.priority <= 3 {
            self.strategies
                .iter()
                .find(|s| s.strategy_type() == SwarmStrategy::Hierarchical)
                .map(|s| s.as_ref())
                .unwrap_or_else(|| self.strategies.first().map(|s| s.as_ref()).unwrap())
        } else {
            self.strategies
                .iter()
                .find(|s| s.strategy_type() == SwarmStrategy::Democratic)
                .map(|s| s.as_ref())
                .unwrap_or_else(|| self.strategies.first().map(|s| s.as_ref()).unwrap())
        }
    }
}

impl Default for HybridStrategy {
    fn default() -> Self {
        Self {
            strategies: vec![
                Box::new(VotingStrategy::default()),
                Box::new(ConsensusStrategy::default()),
                Box::new(HierarchicalStrategy),
                Box::new(DemocraticStrategy),
                Box::new(BiddingStrategy),
            ],
        }
    }
}

#[async_trait]
impl SwarmDecisionStrategy for HybridStrategy {
    fn name(&self) -> &'static str {
        "hybrid"
    }

    fn strategy_type(&self) -> SwarmStrategy {
        SwarmStrategy::Hybrid
    }

    async fn select_agent(
        &self,
        task: &SwarmTask,
        agents: &[SwarmAgent],
        bids: &[SwarmBid],
    ) -> LsResult<Option<SwarmAgent>> {
        let strategy = self.select_strategy(task);
        debug!("hybrid strategy selected: {}", strategy.name());
        strategy.select_agent(task, agents, bids).await
    }

    async fn evaluate_result(
        &self,
        task: &SwarmTask,
        results: &[SwarmTaskResult],
        agents: &[SwarmAgent],
    ) -> LsResult<ConsensusResult> {
        let strategy = self.select_strategy(task);
        strategy.evaluate_result(task, results, agents).await
    }

    fn needs_bidding(&self) -> bool {
        true // Hybrid 可能使用 Bidding
    }
}

// ── Strategy Factory ────────────────────────────────

/// 策略工厂 — 根据配置创建对应策略实例
pub fn create_strategy(config: &SwarmConfig) -> Box<dyn SwarmDecisionStrategy> {
    match config.strategy {
        SwarmStrategy::Voting => Box::new(VotingStrategy::new(config.consensus_threshold)),
        SwarmStrategy::Consensus => Box::new(ConsensusStrategy::new(config.consensus_threshold)),
        SwarmStrategy::Hierarchical => Box::new(HierarchicalStrategy),
        SwarmStrategy::Democratic => Box::new(DemocraticStrategy),
        SwarmStrategy::Bidding => Box::new(BiddingStrategy),
        SwarmStrategy::Hybrid => Box::new(HybridStrategy::default()),
    }
}

#[cfg(test)]
mod tests {
    use lingshu_core::LsId;
    use super::*;

    fn create_test_agent(name: &str, role: SwarmAgentRole, score: f64) -> SwarmAgent {
        let mut agent = SwarmAgent::new(name, role);
        agent.capability_score = score;
        agent.expertise.insert("general".to_string(), score);
        agent
    }

    fn create_test_task() -> SwarmTask {
        SwarmTask::new("test", "test task", serde_json::json!({"input": "test"}))
            .with_priority(5)
    }

    #[tokio::test]
    async fn test_voting_strategy() {
        let strategy = VotingStrategy::default();
        assert_eq!(strategy.name(), "voting");

        let agents = vec![
            create_test_agent("agent-a", SwarmAgentRole::Executor, 0.9),
            create_test_agent("agent-b", SwarmAgentRole::Executor, 0.7),
        ];
        let task = create_test_task();
        let selected = strategy.select_agent(&task, &agents, &[]).await.unwrap();
        assert!(selected.is_some());
        let selected_agent = selected.unwrap();
        assert_eq!(selected_agent.name, "agent-a", "Expected agent-a (capability 0.9) but got {}", selected_agent.name);
    }

    #[tokio::test]
    async fn test_voting_evaluate() {
        let strategy = VotingStrategy::new(0.5);
        let results = vec![
            SwarmTaskResult {
                task_id: LsId::new(),
                agent_id: LsId::new(),
                agent_name: "a".into(),
                output: serde_json::json!("result_a"),
                success: true,
                execution_ms: 100,
                confidence: 0.9,
                error: None,
                started_at: 0,
                completed_at: 100,
            },
            SwarmTaskResult {
                task_id: LsId::new(),
                agent_id: LsId::new(),
                agent_name: "b".into(),
                output: serde_json::json!("result_b"),
                success: true,
                execution_ms: 100,
                confidence: 0.8,
                error: None,
                started_at: 0,
                completed_at: 100,
            },
        ];

        let agents = vec![
            create_test_agent("a", SwarmAgentRole::Executor, 0.9),
            create_test_agent("b", SwarmAgentRole::Executor, 0.8),
        ];

        let task = create_test_task();
        let result = strategy.evaluate_result(&task, &results, &agents).await.unwrap();
        assert!(result.achieved);
        assert_eq!(result.yes_votes, 2);
        assert!(result.approval_ratio >= 0.5);
    }

    #[tokio::test]
    async fn test_consensus_strategy() {
        let strategy = ConsensusStrategy::default();
        assert_eq!(strategy.name(), "consensus");
        assert!(strategy.strategy_type() == SwarmStrategy::Consensus);
    }

    #[tokio::test]
    async fn test_hierarchical_strategy() {
        let strategy = HierarchicalStrategy;
        let agent = create_test_agent("leader-agent", SwarmAgentRole::Executor, 0.95);
        let agents = vec![agent.clone()];
        let task = create_test_task();
        let selected = strategy.select_agent(&task, &agents, &[]).await.unwrap();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name, "leader-agent");
    }

    #[tokio::test]
    async fn test_democratic_strategy() {
        let strategy = DemocraticStrategy;
        assert!(strategy.needs_bidding());
    }

    #[tokio::test]
    async fn test_bidding_strategy() {
        let strategy = BiddingStrategy;
        let bids = vec![
            SwarmBid {
                agent_id: LsId::new(),
                agent_name: "fast-agent".into(),
                bid_score: 0.8,
                estimated_ms: 50,
                rationale: "I'm fast".into(),
                timestamp: 0,
            },
            SwarmBid {
                agent_id: LsId::new(),
                agent_name: "accurate-agent".into(),
                bid_score: 0.95,
                estimated_ms: 200,
                rationale: "I'm accurate".into(),
                timestamp: 0,
            },
        ];
        let task = create_test_task();
        let selected = strategy.select_agent(&task, &[], &bids).await.unwrap();
        assert!(selected.is_some());
        // combined_score: accurate-agent should win due to higher bid_score
    }

    #[test]
    fn test_strategy_factory() {
        let config = SwarmConfig {
            strategy: SwarmStrategy::Voting,
            consensus_threshold: 0.6,
            ..SwarmConfig::default()
        };
        let strategy = create_strategy(&config);
        assert_eq!(strategy.name(), "voting");
        assert!(strategy.strategy_type() == SwarmStrategy::Voting);
    }

    #[test]
    fn test_hybrid_default() {
        let hybrid = HybridStrategy::default();
        assert_eq!(hybrid.name(), "hybrid");

        let config = SwarmConfig {
            strategy: SwarmStrategy::Hybrid,
            ..SwarmConfig::default()
        };
        let strategy = create_strategy(&config);
        assert_eq!(strategy.name(), "hybrid");
    }

    #[test]
    fn test_vote_value_display() {
        assert_eq!(format!("{:?}", VoteValue::Yes), "Yes");
        assert_eq!(format!("{:?}", VoteValue::No), "No");
        assert_eq!(format!("{:?}", VoteValue::Abstain), "Abstain");
    }
}
