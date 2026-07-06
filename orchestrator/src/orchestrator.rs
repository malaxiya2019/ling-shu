//! Orchestrator — 编排器主引擎.
//!
//! 管理智能体团队 (Team)，支持任务委派、团队协作和委派追踪。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::comm::{AgentMessage, InterAgentComm};
use crate::registry::{AgentInfo, AgentRegistry};
use crate::scheduler::{AgentScheduler, ScheduleStrategy};

/// 团队配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// 团队名称
    pub name: String,
    /// 团队描述
    pub description: Option<String>,
    /// 所需能力组合
    pub required_capabilities: Vec<String>,
    /// 调度策略
    pub strategy: ScheduleStrategy,
}

/// 编排器配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// 默认调度策略
    pub default_strategy: ScheduleStrategy,
    /// 请求超时秒数
    pub request_timeout_seconds: u64,
    /// 最大委派深度
    pub max_delegation_depth: u32,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            default_strategy: ScheduleStrategy::RoundRobin,
            request_timeout_seconds: 30,
            max_delegation_depth: 5,
        }
    }
}

/// 委派结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResult {
    pub task_id: String,
    pub agent_id: LsId,
    pub team: String,
    pub output: serde_json::Value,
    pub duration_ms: u64,
    pub depth: u32,
}

/// Orchestrator — 多智能体编排器.
pub struct Orchestrator {
    pub registry: Arc<AgentRegistry>,
    pub scheduler: AgentScheduler,
    pub comm: Arc<InterAgentComm>,
    config: RwLock<OrchestratorConfig>,
    /// 团队配置
    teams: RwLock<HashMap<String, TeamConfig>>,
    /// 活跃委派追踪
    delegations: RwLock<HashMap<String, DelegationResult>>,
}

impl Orchestrator {
    /// 创建编排器.
    pub fn new(config: OrchestratorConfig) -> Self {
        let registry = Arc::new(AgentRegistry::new());
        Self {
            scheduler: AgentScheduler::new(registry.clone(), config.default_strategy),
            registry,
            comm: Arc::new(InterAgentComm::new()),
            config: RwLock::new(config),
            teams: RwLock::new(HashMap::new()),
            delegations: RwLock::new(HashMap::new()),
        }
    }

    /// 创建并注册一个团队.
    pub async fn create_team(&self, config: TeamConfig) -> LsResult<()> {
        let mut teams = self.teams.write().await;
        if teams.contains_key(&config.name) {
            return Err(LsError::AlreadyExists(format!("team {}", config.name)));
        }
        teams.insert(config.name.clone(), config.clone());
        info!(team = %config.name, "team created");
        Ok(())
    }

    /// 将智能体加入团队.
    pub async fn join_team(&self, team_name: &str, agent_id: &LsId) -> LsResult<()> {
        let teams = self.teams.read().await;
        let team = teams
            .get(team_name)
            .ok_or_else(|| LsError::NotFound(format!("team {team_name}")))?;

        // 检查能力是否满足
        let agent_info = self.registry.get(agent_id).await?;
        let agent_caps: Vec<String> = agent_info
            .capabilities
            .iter()
            .map(|c| c.name.clone())
            .collect();

        for required in &team.required_capabilities {
            if !agent_caps.contains(required) {
                return Err(LsError::InvalidArgument(format!(
                    "agent {agent_id} lacks required capability '{required}'"
                )));
            }
        }

        // 写入 team 标签
        self.registry
            .add_tag(agent_id, "team".into(), team_name.into())
            .await?;

        info!(agent_id = %agent_id, team = %team_name, "agent joined team");
        Ok(())
    }

    /// 向团队委派任务.
    pub async fn delegate(
        &self,
        team_name: &str,
        task: serde_json::Value,
        ctx: &LsContext,
    ) -> LsResult<DelegationResult> {
        // 获取团队配置
        let first_capability = {
            let teams = self.teams.read().await;
            let team = teams
                .get(team_name)
                .ok_or_else(|| LsError::NotFound(format!("team {team_name}")))?;
            team.required_capabilities.first().cloned()
        };

        // 找出团队中可用的智能体
        let candidates: Vec<AgentInfo> = self
            .registry
            .find_by_tag("team", team_name)
            .await
            .into_iter()
            .filter(|a| a.status == lingshu_traits::agent::AgentStatus::Idle)
            .collect();

        if candidates.is_empty() {
            return Err(LsError::QuotaExceeded(format!(
                "no available agent in team {team_name}"
            )));
        }

        // 选择智能体
        let assignment = self
            .scheduler
            .select_agent(first_capability.as_deref(), ctx)
            .await?;

        self.scheduler.task_started(&assignment.agent_id).await;

        let start = std::time::Instant::now();

        // 通过通信层发送任务
        let msg = AgentMessage {
            id: assignment.task_id.clone(),
            source: LsId::new(), // orchestrator 自身 ID
            destination: Some(assignment.agent_id),
            message_type: "command".into(),
            payload: task.clone(),
            correlation_id: Some(assignment.task_id.clone()),
            timestamp: chrono::Utc::now(),
            ttl_seconds: Some(self.config.read().await.request_timeout_seconds),
        };

        match self.comm.send(&assignment.agent_id, msg).await {
            Ok(_) => {
                let duration = start.elapsed().as_millis() as u64;
                let result = DelegationResult {
                    task_id: assignment.task_id.clone(),
                    agent_id: assignment.agent_id,
                    team: team_name.to_string(),
                    output: serde_json::json!({"status": "delegated", "task_id": assignment.task_id}),
                    duration_ms: duration,
                    depth: 0,
                };

                let mut delegations = self.delegations.write().await;
                delegations.insert(assignment.task_id.clone(), result.clone());

                self.scheduler.task_completed(&assignment.agent_id).await;
                info!(
                    task_id = %result.task_id,
                    agent_id = %result.agent_id,
                    team = %team_name,
                    "task delegated"
                );
                Ok(result)
            }
            Err(e) => {
                self.scheduler.task_completed(&assignment.agent_id).await;
                Err(e)
            }
        }
    }

    /// 查询委派结果.
    pub async fn delegation_result(&self, task_id: &str) -> LsResult<DelegationResult> {
        let delegations = self.delegations.read().await;
        delegations
            .get(task_id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("delegation {task_id}")))
    }

    /// 列出所有委派.
    pub async fn list_delegations(&self) -> Vec<DelegationResult> {
        self.delegations.read().await.values().cloned().collect()
    }

    /// 获取编排器配置.
    pub async fn config(&self) -> OrchestratorConfig {
        self.config.read().await.clone()
    }

    /// 更新编排器配置.
    pub async fn update_config(&self, config: OrchestratorConfig) {
        *self.config.write().await = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::make_info;

    #[tokio::test]
    async fn test_create_team() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        let config = TeamConfig {
            name: "writers".into(),
            description: Some("Writing team".into()),
            required_capabilities: vec!["writing".into()],
            strategy: ScheduleStrategy::RoundRobin,
        };
        orch.create_team(config).await.unwrap();

        // Duplicate should fail
        let dup = TeamConfig {
            name: "writers".into(),
            description: None,
            required_capabilities: vec![],
            strategy: ScheduleStrategy::RoundRobin,
        };
        assert!(orch.create_team(dup).await.is_err());
    }

    #[tokio::test]
    async fn test_agent_join_team() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        orch.create_team(TeamConfig {
            name: "coders".into(),
            description: None,
            required_capabilities: vec!["code".into()],
            strategy: ScheduleStrategy::RoundRobin,
        })
        .await
        .unwrap();

        let id = LsId::new();
        orch.registry
            .register(make_info(id, "coder-agent", vec!["code"]))
            .await
            .unwrap();

        orch.join_team("coders", &id).await.unwrap();

        let info = orch.registry.get(&id).await.unwrap();
        assert_eq!(info.tags.get("team").unwrap(), "coders");
    }

    #[tokio::test]
    async fn test_agent_join_team_missing_capability() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        orch.create_team(TeamConfig {
            name: "writers".into(),
            description: None,
            required_capabilities: vec!["writing".into()],
            strategy: ScheduleStrategy::RoundRobin,
        })
        .await
        .unwrap();

        let id = LsId::new();
        orch.registry
            .register(make_info(id, "coder", vec!["code"]))
            .await
            .unwrap();

        assert!(orch.join_team("writers", &id).await.is_err());
    }

    #[tokio::test]
    async fn test_delegate_task() {
        let orch = Orchestrator::new(OrchestratorConfig::default());
        orch.create_team(TeamConfig {
            name: "workers".into(),
            description: None,
            required_capabilities: vec!["work".into()],
            strategy: ScheduleStrategy::RoundRobin,
        })
        .await
        .unwrap();

        let id = LsId::new();
        orch.registry
            .register(make_info(id, "worker-1", vec!["work"]))
            .await
            .unwrap();
        // 注册 inbox
        let _rx = orch.comm.register_agent(id).await;

        orch.join_team("workers", &id).await.unwrap();

        let ctx = LsContext::with_session(LsId::new());
        let result = orch
            .delegate("workers", serde_json::json!({"task": "do something"}), &ctx)
            .await
            .unwrap();

        assert_eq!(result.team, "workers");
        assert_eq!(result.agent_id, id);

        // 委派应该被记录
        let stored = orch.delegation_result(&result.task_id).await.unwrap();
        assert_eq!(stored.task_id, result.task_id);
    }
}
