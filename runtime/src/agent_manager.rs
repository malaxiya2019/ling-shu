//! AgentManager — 智能体生命周期管理.
//!
//! 管理运行中的 Agent 实例，支持 pause/resume/cancel/status 操作.

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::agent::{Agent, AgentSnapshot, AgentStatus};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::info;

/// Agent 运行时条目.
struct AgentEntry {
    agent_id: LsId,
    name: String,
    agent: Box<dyn Agent>,
    status: AgentStatus,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// AgentManager — 线程安全的 Agent 注册与生命周期管理.
pub struct AgentManager {
    agents: RwLock<HashMap<LsId, AgentEntry>>,
}

impl AgentManager {
    /// 创建新的 AgentManager.
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
        }
    }

    /// 注册一个 Agent.
    pub async fn register(&self, agent_id: LsId, name: String, agent: Box<dyn Agent>) {
        let mut agents = self.agents.write().await;
        agents.insert(
            agent_id,
            AgentEntry {
                agent_id,
                name,
                agent,
                status: AgentStatus::Idle,
                created_at: chrono::Utc::now(),
            },
        );
        info!(agent_id = %agent_id, "agent registered");
    }

    /// 获取 Agent 状态.
    pub async fn status(&self, agent_id: &LsId) -> LsResult<AgentStatus> {
        let agents = self.agents.read().await;
        agents
            .get(agent_id)
            .map(|e| e.status.clone())
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))
    }

    /// 暂停 Agent.
    pub async fn pause(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents.get_mut(agent_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("agent {agent_id}"))
        })?;
        entry.agent.pause(ctx.clone()).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent paused");
        Ok(())
    }

    /// 恢复 Agent.
    pub async fn resume(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents.get_mut(agent_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("agent {agent_id}"))
        })?;
        entry.agent.resume(ctx.clone()).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent resumed");
        Ok(())
    }

    /// 取消 Agent.
    pub async fn cancel(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents.get_mut(agent_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("agent {agent_id}"))
        })?;
        entry.agent.cancel(ctx.clone()).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent cancelled");
        Ok(())
    }

    /// 获取快照.
    pub async fn snapshot(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<AgentSnapshot> {
        let agents = self.agents.read().await;
        let entry = agents.get(agent_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("agent {agent_id}"))
        })?;
        entry.agent.snapshot(ctx.clone()).await
    }

    /// 从快照恢复.
    pub async fn restore(
        &self,
        agent_id: &LsId,
        ctx: &LsContext,
        snapshot: AgentSnapshot,
    ) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents.get_mut(agent_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("agent {agent_id}"))
        })?;
        entry.agent.restore(ctx.clone(), snapshot).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent restored");
        Ok(())
    }

    /// 列出所有 Agent 摘要.
    pub async fn list(&self) -> Vec<AgentSummary> {
        let agents = self.agents.read().await;
        agents
            .values()
            .map(|e| AgentSummary {
                agent_id: e.agent_id,
                name: e.name.clone(),
                status: e.status.clone(),
                created_at: e.created_at,
            })
            .collect()
    }

    /// 移除 Agent.
    pub async fn remove(&self, agent_id: &LsId) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        agents.remove(agent_id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("agent {agent_id}"))
        })?;
        info!(agent_id = %agent_id, "agent removed");
        Ok(())
    }

    /// Agent 数量.
    pub async fn count(&self) -> usize {
        self.agents.read().await.len()
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent 摘要信息.
#[derive(Debug, Clone)]
pub struct AgentSummary {
    pub agent_id: LsId,
    pub name: String,
    pub status: AgentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::LsResult;
    use lingshu_traits::agent::{AgentOutput, AgentSnapshot, AgentStatus};
    use serde_json::Value;

    struct TestAgent {
        id: LsId,
        status: AgentStatus,
    }

    #[async_trait]
    impl Agent for TestAgent {
        fn id(&self) -> LsId {
            self.id
        }

        async fn run(&mut self, _ctx: LsContext, _input: Value) -> LsResult<AgentOutput> {
            self.status = AgentStatus::Completed;
            Ok(AgentOutput {
                agent_id: self.id,
                status: AgentStatus::Completed,
                data: Some(Value::Null),
                error: None,
            })
        }

        async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> {
            self.status = AgentStatus::Paused;
            Ok(())
        }

        async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> {
            self.status = AgentStatus::Running;
            Ok(())
        }

        async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> {
            self.status = AgentStatus::Idle;
            Ok(())
        }

        async fn snapshot(&self, _ctx: LsContext) -> LsResult<AgentSnapshot> {
            Ok(AgentSnapshot {
                    agent_id: self.id,
                    status: self.status.clone(),
                    context: LsContext::with_session(LsId::new()),
                    state: Vec::new(),
                    created_at: chrono::Utc::now(),
                })
        }

        async fn restore(&mut self, _ctx: LsContext, _snap: AgentSnapshot) -> LsResult<()> {
            Ok(())
        }

        async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> {
            Ok(self.status.clone())
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let mgr = AgentManager::new();
        assert_eq!(mgr.count().await, 0);

        let id = LsId::new();
        mgr.register(id, "test".into(), Box::new(TestAgent {
            id,
            status: AgentStatus::Idle,
        })).await;

        assert_eq!(mgr.count().await, 1);
        let list = mgr.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test");
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let mgr = AgentManager::new();
        let id = LsId::new();
        mgr.register(id, "test".into(), Box::new(TestAgent {
            id,
            status: AgentStatus::Running,
        })).await;

        let ctx = LsContext::with_session(LsId::new());
        mgr.pause(&id, &ctx).await.unwrap();
        let status = mgr.status(&id).await.unwrap();
        assert_eq!(status, AgentStatus::Paused);

        mgr.resume(&id, &ctx).await.unwrap();
        let status = mgr.status(&id).await.unwrap();
        assert_eq!(status, AgentStatus::Running);
    }

    #[tokio::test]
    async fn test_remove() {
        let mgr = AgentManager::new();
        let id = LsId::new();
        mgr.register(id, "test".into(), Box::new(TestAgent {
            id,
            status: AgentStatus::Idle,
        })).await;

        mgr.remove(&id).await.unwrap();
        assert_eq!(mgr.count().await, 0);
    }

    #[tokio::test]
    async fn test_not_found() {
        let mgr = AgentManager::new();
        let ctx = LsContext::with_session(LsId::new());
        let id = LsId::new();
        assert!(mgr.status(&id).await.is_err());
        assert!(mgr.pause(&id, &ctx).await.is_err());
        assert!(mgr.remove(&id).await.is_err());
    }
}
