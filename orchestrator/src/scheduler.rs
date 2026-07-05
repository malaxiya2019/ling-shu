//! AgentScheduler — 智能体任务调度与负载均衡.
//!
//! 支持多种调度策略:
//! - **RoundRobin** — 轮询分配
//! - **LeastBusy** — 最少任务优先
//! - **CapabilityFirst** — 按能力匹配度分配

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::AgentStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

use crate::registry::{AgentInfo, AgentRegistry};

/// 调度策略.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleStrategy {
    /// 轮询分配
    RoundRobin,
    /// 最少任务优先 (需追踪任务数)
    LeastBusy,
    /// 按能力匹配度优先
    CapabilityFirst,
}

/// 任务分配结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    /// 分配的任务 ID
    pub task_id: String,
    /// 目标智能体 ID
    pub agent_id: LsId,
    /// 分配的策略
    pub strategy: ScheduleStrategy,
    /// 匹配度分数 (仅 CapabilityFirst)
    pub score: Option<f64>,
}

/// AgentScheduler — 任务调度器.
pub struct AgentScheduler {
    registry: std::sync::Arc<AgentRegistry>,
    strategy: RwLock<ScheduleStrategy>,
    /// 轮询计数器
    round_robin_counter: AtomicU64,
    /// 各智能体当前任务数追踪
    task_counts: RwLock<HashMap<LsId, u64>>,
}

impl AgentScheduler {
    /// 创建调度器.
    pub fn new(registry: std::sync::Arc<AgentRegistry>, strategy: ScheduleStrategy) -> Self {
        Self {
            registry,
            strategy: RwLock::new(strategy),
            round_robin_counter: AtomicU64::new(0),
            task_counts: RwLock::new(HashMap::new()),
        }
    }

    /// 设置调度策略.
    pub async fn set_strategy(&self, strategy: ScheduleStrategy) {
        *self.strategy.write().await = strategy;
    }

    /// 获取当前策略.
    pub async fn strategy(&self) -> ScheduleStrategy {
        *self.strategy.read().await
    }

    /// 选择一个智能体来执行任务.
    pub async fn select_agent(
        &self,
        required_capability: Option<&str>,
        _ctx: &LsContext,
    ) -> LsResult<TaskAssignment> {
        let candidates = match required_capability {
            Some(cap) => {
                let mut agents = self.registry.find_by_capability(cap).await;
                agents.retain(|a| a.status == AgentStatus::Idle);
                agents
            }
            None => {
                let mut agents = self.registry.list().await;
                agents.retain(|a| a.status == AgentStatus::Idle);
                agents
            }
        };

        if candidates.is_empty() {
            return Err(LsError::QuotaExceeded(
                "no available agent for task".into(),
            ));
        }

        let strategy = *self.strategy.read().await;
        let task_id = uuid::Uuid::now_v7().to_string();

        match strategy {
            ScheduleStrategy::RoundRobin => self.round_robin(candidates, task_id),
            ScheduleStrategy::LeastBusy => self.least_busy(candidates, task_id).await,
            ScheduleStrategy::CapabilityFirst => {
                self.capability_first(candidates, task_id, required_capability)
            }
        }
    }

    /// 轮询分配.
    fn round_robin(
        &self,
        candidates: Vec<AgentInfo>,
        task_id: String,
    ) -> LsResult<TaskAssignment> {
        let idx = self.round_robin_counter.fetch_add(1, Ordering::Relaxed) as usize % candidates.len();
        let agent = &candidates[idx];
        Ok(TaskAssignment {
            task_id,
            agent_id: agent.agent_id,
            strategy: ScheduleStrategy::RoundRobin,
            score: None,
        })
    }

    /// 最少任务分配.
    async fn least_busy(
        &self,
        candidates: Vec<AgentInfo>,
        task_id: String,
    ) -> LsResult<TaskAssignment> {
        let counts = self.task_counts.read().await;
        let agent = candidates
            .iter()
            .min_by_key(|a| counts.get(&a.agent_id).copied().unwrap_or(0))
            .ok_or_else(|| LsError::QuotaExceeded("no candidates".into()))?;

        Ok(TaskAssignment {
            task_id,
            agent_id: agent.agent_id,
            strategy: ScheduleStrategy::LeastBusy,
            score: None,
        })
    }

    /// 按能力匹配度分配.
    fn capability_first(
        &self,
        candidates: Vec<AgentInfo>,
        task_id: String,
        required_capability: Option<&str>,
    ) -> LsResult<TaskAssignment> {
        let required = required_capability.unwrap_or("");
        let (agent, score) = candidates
            .iter()
            .map(|a| {
                let match_count = a
                    .capabilities
                    .iter()
                    .filter(|c| c.name == required)
                    .count() as f64;
                let total = a.capabilities.len() as f64;
                // 匹配度 = 拥有该能力的个数 / 总能力数 (通常为 1/1 = 1.0)
                let score = if total > 0.0 { match_count / total } else { 0.0 };
                (a, score)
            })
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .ok_or_else(|| LsError::QuotaExceeded("no candidates".into()))?;

        Ok(TaskAssignment {
            task_id,
            agent_id: agent.agent_id,
            strategy: ScheduleStrategy::CapabilityFirst,
            score: Some(score),
        })
    }

    /// 标记任务开始 (更新任务计数).
    pub async fn task_started(&self, agent_id: &LsId) {
        let mut counts = self.task_counts.write().await;
        *counts.entry(*agent_id).or_insert(0) += 1;
    }

    /// 标记任务完成 (更新任务计数).
    pub async fn task_completed(&self, agent_id: &LsId) {
        let mut counts = self.task_counts.write().await;
        if let Some(c) = counts.get_mut(agent_id) {
            if *c > 0 {
                *c -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::registry::make_info;

    #[tokio::test]
    async fn test_round_robin() {
        let reg = AgentRegistry::new();
        reg.register(make_info(LsId::new(), "a", vec!["code"]))
            .await
            .unwrap();
        reg.register(make_info(LsId::new(), "b", vec!["code"]))
            .await
            .unwrap();

        let sched = AgentScheduler::new(Arc::new(reg), ScheduleStrategy::RoundRobin);
        let ctx = LsContext::with_session(LsId::new());

        let t1 = sched.select_agent(Some("code"), &ctx).await.unwrap();
        let t2 = sched.select_agent(Some("code"), &ctx).await.unwrap();
        // RoundRobin 分配应不同 (除非只有 2 个, 2 次不同)
        assert_ne!(t1.agent_id, t2.agent_id);
    }

    #[tokio::test]
    async fn test_no_available_agent() {
        let reg = AgentRegistry::new();
        // 注册但状态不是 Idle
        let id = LsId::new();
        let mut info = make_info(id, "busy", vec!["test"]);
        info.status = AgentStatus::Running;
        reg.register(info).await.unwrap();

        let sched = AgentScheduler::new(Arc::new(reg), ScheduleStrategy::RoundRobin);
        let ctx = LsContext::with_session(LsId::new());
        assert!(sched.select_agent(Some("test"), &ctx).await.is_err());
    }

    #[tokio::test]
    async fn test_capability_mismatch() {
        let reg = AgentRegistry::new();
        reg.register(make_info(LsId::new(), "coder", vec!["code"]))
            .await
            .unwrap();

        let sched = AgentScheduler::new(Arc::new(reg), ScheduleStrategy::RoundRobin);
        let ctx = LsContext::with_session(LsId::new());
        // 没有写能力
        assert!(sched.select_agent(Some("writing"), &ctx).await.is_err());
    }

    #[tokio::test]
    async fn test_task_bookkeeping() {
        let reg = AgentRegistry::new();
        let id = LsId::new();
        reg.register(make_info(id, "worker", vec!["task"]))
            .await
            .unwrap();

        let sched = AgentScheduler::new(Arc::new(reg), ScheduleStrategy::LeastBusy);
        sched.task_started(&id).await;
        let counts = sched.task_counts.read().await;
        assert_eq!(counts.get(&id), Some(&1));
        drop(counts);

        sched.task_completed(&id).await;
        let counts = sched.task_counts.read().await;
        assert_eq!(counts.get(&id), Some(&0));
    }

    #[tokio::test]
    async fn test_strategy_switch() {
        let reg = AgentRegistry::new();
        let sched = AgentScheduler::new(Arc::new(reg), ScheduleStrategy::RoundRobin);
        assert_eq!(sched.strategy().await, ScheduleStrategy::RoundRobin);
        sched.set_strategy(ScheduleStrategy::LeastBusy).await;
        assert_eq!(sched.strategy().await, ScheduleStrategy::LeastBusy);
    }
}
