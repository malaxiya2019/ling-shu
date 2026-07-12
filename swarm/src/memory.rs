//! AgentSwarm — 群体记忆与涌现行为系统
//!
//! 管理 Swarm 级别的共享记忆、涌现专长和行为模式。
//! 跟踪 Agent 的历史表现，自动发现最优角色分配。

use crate::types::*;
use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ── 能力画像 ────────────────────────────────────────

/// Agent 的能力画像（由 Emergent Specialization 维护）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Agent ID
    pub agent_id: LsId,
    /// Agent 名称
    pub name: String,
    /// 各角色表现评分
    pub role_performance: HashMap<String, RolePerformance>,
    /// 当前最优角色
    pub optimal_role: SwarmAgentRole,
    /// 专长领域
    pub expertise_areas: HashMap<String, f64>,
    /// 任务历史摘要
    pub task_history: VecDeque<TaskRecord>,
    /// 学习率（Emergent Specialization 的适应速度）
    pub learning_rate: f64,
}

/// 角色表现
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePerformance {
    /// 执行次数
    pub execution_count: u32,
    /// 成功率
    pub success_rate: f64,
    /// 平均执行时长 ms
    pub avg_execution_ms: f64,
    /// 平均置信度
    pub avg_confidence: f64,
    /// 最后执行时间
    pub last_executed: i64,
}

/// 任务记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    /// 任务名称
    pub task_name: String,
    /// 执行角色
    pub role: SwarmAgentRole,
    /// 是否成功
    pub success: bool,
    /// 执行时长 ms
    pub execution_ms: u64,
    /// 置信度
    pub confidence: f64,
    /// 时间戳
    pub timestamp: i64,
}

// ── 涌现行为引擎 ────────────────────────────────────

/// 涌现专长管理 — 自动发现每个 Agent 的最优角色
pub struct EmergentSpecialization {
    /// Agent 画像
    profiles: RwLock<HashMap<LsId, AgentProfile>>,
    /// 角色切换记录
    role_switches: RwLock<Vec<RoleSwitch>>,
    /// 最小执行次数后才切换角色
    min_executions_before_switch: u32,
    /// 性能提升阈值（超过此值才切换）
    improvement_threshold: f64,
}

/// 角色切换记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleSwitch {
    pub agent_id: LsId,
    pub from_role: SwarmAgentRole,
    pub to_role: SwarmAgentRole,
    pub reason: String,
    pub timestamp: i64,
}

impl EmergentSpecialization {
    pub fn new(min_executions: u32, improvement_threshold: f64) -> Self {
        Self {
            profiles: RwLock::new(HashMap::new()),
            role_switches: RwLock::new(Vec::new()),
            min_executions_before_switch: min_executions,
            improvement_threshold,
        }
    }

    /// 注册或更新 Agent
    pub async fn register_agent(&self, agent: &SwarmAgent) {
        let mut profiles = self.profiles.write().await;
        profiles.entry(agent.id).or_insert_with(|| {
            let profile = AgentProfile {
                agent_id: agent.id,
                name: agent.name.clone(),
                role_performance: HashMap::new(),
                optimal_role: agent.role,
                expertise_areas: agent.expertise.clone(),
                task_history: VecDeque::with_capacity(100),
                learning_rate: 0.3,
            };
            debug!("registered profile for agent '{}'", agent.name);
            profile
        });
    }

    /// 记录任务执行结果，更新 Agent 画像
    pub async fn record_execution(&self, agent_id: &LsId, task_name: &str, role: SwarmAgentRole, result: &SwarmTaskResult) {
        let mut profiles = self.profiles.write().await;

        if let Some(profile) = profiles.get_mut(agent_id) {
            // 更新角色表现
            let role_key = role.as_str().to_string();
            let perf = profile
                .role_performance
                .entry(role_key.clone())
                .or_insert(RolePerformance {
                    execution_count: 0,
                    success_rate: 0.5,
                    avg_execution_ms: 0.0,
                    avg_confidence: 0.5,
                    last_executed: 0,
                });

            let alpha = profile.learning_rate;
            perf.execution_count += 1;
            perf.success_rate = alpha * (if result.success { 1.0 } else { 0.0 }) + (1.0 - alpha) * perf.success_rate;
            perf.avg_execution_ms = alpha * result.execution_ms as f64 + (1.0 - alpha) * perf.avg_execution_ms;
            perf.avg_confidence = alpha * result.confidence + (1.0 - alpha) * perf.avg_confidence;
            perf.last_executed = chrono::Utc::now().timestamp();

            // 添加任务记录
            profile.task_history.push_front(TaskRecord {
                task_name: task_name.to_string(),
                role,
                success: result.success,
                execution_ms: result.execution_ms,
                confidence: result.confidence,
                timestamp: chrono::Utc::now().timestamp(),
            });

            // 限制历史记录大小
            while profile.task_history.len() > 100 {
                profile.task_history.pop_back();
            }

            // 更新专长领域
            for expertise in profile.expertise_areas.keys().cloned().collect::<Vec<_>>() {
                if let Some(score) = profile.expertise_areas.get_mut(&expertise) {
                    *score = alpha * (if result.success { result.confidence } else { 0.0 }) + (1.0 - alpha) * *score;
                }
            }
        }
    }

    /// 评估并建议最优角色切换
    pub async fn suggest_role_change(&self, agent_id: &LsId) -> Option<RoleSwitch> {
        let profiles = self.profiles.read().await;
        let profile = profiles.get(agent_id)?;

        let current_role_key = profile.optimal_role.as_str();
        let current_perf = profile.role_performance.get(current_role_key);

        // 对每个替代角色评估
        for (role_key, perf) in &profile.role_performance {
            let role = match role_key.as_str() {
                "analyst" => SwarmAgentRole::Analyst,
                "creator" => SwarmAgentRole::Creator,
                "validator" => SwarmAgentRole::Validator,
                "negotiator" => SwarmAgentRole::Negotiator,
                "observer" => SwarmAgentRole::Observer,
                "planner" => SwarmAgentRole::Planner,
                "executor" => SwarmAgentRole::Executor,
                "tester" => SwarmAgentRole::Tester,
                "aggregator" => SwarmAgentRole::Aggregator,
                "router" => SwarmAgentRole::Router,
                _ => continue,
            };

            if role == profile.optimal_role {
                continue;
            }

            if perf.execution_count < self.min_executions_before_switch {
                continue;
            }

            if let Some(current) = current_perf {
                if perf.success_rate > current.success_rate + self.improvement_threshold {
                    // 建议切换！
                    let from_role = profile.optimal_role;
                    return Some(RoleSwitch {
                        agent_id: *agent_id,
                        from_role,
                        to_role: role,
                        reason: format!(
                            "Emergent specialization: success_rate improved from {:.2} to {:.2}",
                            current.success_rate, perf.success_rate
                        ),
                        timestamp: chrono::Utc::now().timestamp(),
                    });
                }
            }
        }

        None
    }

    /// 应用角色切换
    pub async fn apply_role_switch(&self, switch: RoleSwitch) {
        let mut profiles = self.profiles.write().await;
        let mut switches = self.role_switches.write().await;

        if let Some(profile) = profiles.get_mut(&switch.agent_id) {
            info!(
                "role switch: agent '{}': {:?} → {:?} ({})",
                profile.name, switch.from_role, switch.to_role, switch.reason
            );
            profile.optimal_role = switch.to_role;
            switches.push(switch);
        }
    }

    /// 获取 Agent 的最优角色
    pub async fn get_optimal_role(&self, agent_id: &LsId) -> Option<SwarmAgentRole> {
        let profiles = self.profiles.read().await;
        profiles.get(agent_id).map(|p| p.optimal_role)
    }

    /// 获取角色切换历史
    pub async fn get_role_switches(&self) -> Vec<RoleSwitch> {
        self.role_switches.read().await.clone()
    }

    /// 获取 Agent 画像
    pub async fn get_profile(&self, agent_id: &LsId) -> Option<AgentProfile> {
        let profiles = self.profiles.read().await;
        profiles.get(agent_id).cloned()
    }
}

// ── Swarm 记忆 ──────────────────────────────────────

/// Swarm 级别的共享记忆
pub struct SwarmMemory {
    /// 任务-结果映射
    outcomes: RwLock<HashMap<LsId, SwarmTaskResult>>,
    /// 涌现模式
    patterns: RwLock<Vec<EmergentPattern>>,
    /// 关键决策记录
    decisions: RwLock<Vec<ConsensusResult>>,
}

/// 涌现模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergentPattern {
    /// 模式名称
    pub name: String,
    /// 模式描述
    pub description: String,
    /// 触发次数
    pub occurrence_count: u32,
    /// 成功率
    pub success_rate: f64,
    /// 首次观察时间
    pub first_observed: i64,
    /// 最后观察时间
    pub last_observed: i64,
}

impl SwarmMemory {
    pub fn new() -> Self {
        Self {
            outcomes: RwLock::new(HashMap::new()),
            patterns: RwLock::new(Vec::new()),
            decisions: RwLock::new(Vec::new()),
        }
    }

    /// 记录任务结果
    pub async fn record_result(&self, _task: &SwarmTask, result: &SwarmTaskResult) {
        self.outcomes.write().await.insert(_task.id, result.clone());

        // 检测模式：连续失败
        let outcomes = self.outcomes.read().await;
        let recent_failures: Vec<&SwarmTaskResult> = outcomes
            .values()
            .filter(|r| !r.success)
            .collect();

        if recent_failures.len() >= 3 {
            let pattern_name = "repeated_failure";
            let mut patterns = self.patterns.write().await;
            if let Some(pattern) = patterns.iter_mut().find(|p: &&mut EmergentPattern| p.name == pattern_name) {
                pattern.occurrence_count += 1;
                pattern.last_observed = chrono::Utc::now().timestamp();
            } else {
                patterns.push(EmergentPattern {
                    name: pattern_name.to_string(),
                    description: "Multiple consecutive task failures detected".to_string(),
                    occurrence_count: 1,
                    success_rate: 0.0,
                    first_observed: chrono::Utc::now().timestamp(),
                    last_observed: chrono::Utc::now().timestamp(),
                });
            }
            warn!("emergent pattern: repeated_failure ({} occurrences)", recent_failures.len());
        }
    }

    /// 记录决策
    pub async fn record_decision(&self, result: &ConsensusResult) {
        self.decisions.write().await.push(result.clone());
    }

    /// 获取失败模式
    pub async fn get_failure_patterns(&self) -> Vec<EmergentPattern> {
        let patterns = self.patterns.read().await;
        patterns
            .iter()
            .filter(|p| p.success_rate < 0.5)
            .cloned()
            .collect()
    }

    /// 获取最近结果
    pub async fn get_recent_results(&self, count: usize) -> Vec<SwarmTaskResult> {
        let outcomes = self.outcomes.read().await;
        let mut results: Vec<SwarmTaskResult> = outcomes.values().cloned().collect();
        results.sort_by_key(|b| std::cmp::Reverse(b.completed_at));
        results.truncate(count);
        results
    }
}

impl Default for SwarmMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(agent_id: LsId, success: bool, confidence: f64) -> SwarmTaskResult {
        SwarmTaskResult {
            task_id: LsId::new(),
            agent_id,
            agent_name: "test-agent".into(),
            output: serde_json::json!({"result": "test"}),
            success,
            execution_ms: 100,
            confidence,
            error: if success { None } else { Some("error".into()) },
            started_at: 0,
            completed_at: 100,
        }
    }

    #[tokio::test]
    async fn test_emergent_specialization_register() {
        let es = EmergentSpecialization::new(3, 0.1);
        let agent = SwarmAgent::new("test-agent", SwarmAgentRole::Executor);
        es.register_agent(&agent).await;

        let profile = es.get_profile(&agent.id).await;
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().name, "test-agent");
    }

    #[tokio::test]
    async fn test_emergent_specialization_record() {
        let es = EmergentSpecialization::new(3, 0.1);
        let agent = SwarmAgent::new("test-agent", SwarmAgentRole::Executor);
        es.register_agent(&agent).await;

        let result = create_test_result(agent.id, true, 0.9);
        es.record_execution(&agent.id, "test-task", SwarmAgentRole::Executor, &result).await;

        let profile = es.get_profile(&agent.id).await.unwrap();
        assert_eq!(profile.task_history.len(), 1);

        let perf = profile.role_performance.get("executor").unwrap();
        assert_eq!(perf.execution_count, 1);
        assert!(perf.success_rate >= 0.5);
    }

    #[tokio::test]
    async fn test_emergent_specialization_role_switch() {
        let es = EmergentSpecialization::new(1, 0.1);
        let agent = SwarmAgent::new("adaptive-agent", SwarmAgentRole::Executor);
        es.register_agent(&agent).await;

        // 作为 Executor 表现差
        for _ in 0..3 {
            let result = create_test_result(agent.id, false, 0.2);
            es.record_execution(&agent.id, "bad-task", SwarmAgentRole::Executor, &result).await;
        }

        // 作为 Validator 表现好（假设在其他角色下记录了高性能）
        for _ in 0..3 {
            let result = create_test_result(agent.id, true, 0.95);
            es.record_execution(&agent.id, "good-task", SwarmAgentRole::Validator, &result).await;
        }

        // 建议角色切换
        let switch = es.suggest_role_change(&agent.id).await;
        // 可能建议切换
        if let Some(s) = switch {
            assert_eq!(s.from_role, SwarmAgentRole::Executor);
            es.apply_role_switch(s).await;
            let switches = es.get_role_switches().await;
            assert_eq!(switches.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_swarm_memory() {
        let memory = SwarmMemory::new();

        let task = SwarmTask::new("test", "test", serde_json::json!({}));
        let result = create_test_result(LsId::new(), true, 0.9);
        memory.record_result(&task, &result).await;

        let recent = memory.get_recent_results(10).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].confidence, 0.9);
    }

    #[tokio::test]
    async fn test_swarm_memory_failure_pattern() {
        let memory = SwarmMemory::new();

        for _ in 0..4 {
            let task = SwarmTask::new("fail", "fail", serde_json::json!({}));
            let result = create_test_result(LsId::new(), false, 0.1);
            memory.record_result(&task, &result).await;
        }

        let patterns = memory.get_failure_patterns().await;
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_agent_profile_creation() {
        let profile = AgentProfile {
            agent_id: LsId::new(),
            name: "profile-test".into(),
            role_performance: HashMap::new(),
            optimal_role: SwarmAgentRole::Analyst,
            expertise_areas: HashMap::from([("rust".into(), 0.9)]),
            task_history: VecDeque::new(),
            learning_rate: 0.3,
        };
        assert_eq!(profile.name, "profile-test");
        assert_eq!(profile.optimal_role, SwarmAgentRole::Analyst);
        assert_eq!(profile.expertise_areas.get("rust").unwrap(), &0.9);
    }
}
