//! AgentManager — 智能体生命周期管理.
//!
//! 管理运行中的 Agent 实例，支持 pause/resume/cancel/status 操作.
//! 通过可选的 Plugin EventBus 发布 Agent 生命周期事件.

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::agent::{Agent, AgentSnapshot, AgentStatus};
use std::collections::HashMap;
use std::sync::Arc;
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
    /// 可选的 Plugin EventBus（RwLock 支持 &self 方法）.
    plugin_event_bus: RwLock<Option<Arc<lingshu_plugin::event::EventBus>>>,
}

impl AgentManager {
    /// 创建新的 AgentManager.
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            plugin_event_bus: RwLock::new(None),
        }
    }

    /// 创建 AgentManager 并绑定 Plugin EventBus.
    pub fn with_event_bus(event_bus: Arc<lingshu_plugin::event::EventBus>) -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            plugin_event_bus: RwLock::new(Some(event_bus)),
        }
    }

    /// 返回当前活跃 agent 数量.
    pub fn active_agent_count(&self) -> usize {
        self.agents.try_read().map(|g| g.len()).unwrap_or(0)
    }

    /// 设置 Plugin EventBus（运行时注入，&self 安全）.
    pub async fn set_event_bus(&self, event_bus: Arc<lingshu_plugin::event::EventBus>) {
        *self.plugin_event_bus.write().await = Some(event_bus);
    }

    /// 获取 Plugin EventBus 引用（如有）.
    pub async fn event_bus(&self) -> Option<Arc<lingshu_plugin::event::EventBus>> {
        self.plugin_event_bus.read().await.clone()
    }

    /// 发布 Agent 事件到 Plugin EventBus.
    async fn emit_event(
        &self,
        event_type: lingshu_plugin::event::EventType,
        agent_id: &LsId,
        name: &str,
        payload: serde_json::Value,
    ) {
        if let Some(ref bus) = *self.plugin_event_bus.read().await {
            let event = lingshu_plugin::event::Event::new(
                event_type,
                format!("agent:{}", agent_id),
                serde_json::json!({
                    "agent_id": agent_id.to_string(),
                    "name": name,
                    "payload": payload,
                }),
            );
            bus.publish(&event).await;
        }
    }

    /// 注册一个 Agent.
    pub async fn register(&self, agent_id: LsId, name: String, agent: Box<dyn Agent>) {
        let mut agents = self.agents.write().await;
        agents.insert(
            agent_id,
            AgentEntry {
                agent_id,
                name: name.clone(),
                agent,
                status: AgentStatus::Idle,
                created_at: chrono::Utc::now(),
            },
        );
        info!(agent_id = %agent_id, "agent registered");
        self.emit_event(
            lingshu_plugin::event::EventType::AgentCreated,
            &agent_id,
            &name,
            serde_json::json!({"status": "Idle"}),
        )
        .await;
    }

    /// 获取 Agent 状态.
    pub async fn status(&self, agent_id: &LsId) -> LsResult<AgentStatus> {
        let agents = self.agents.read().await;
        agents
            .get(agent_id)
            .map(|e| e.status)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))
    }

    /// 暂停 Agent.
    pub async fn pause(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))?;
        entry.agent.pause(ctx.clone()).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent paused");
        Ok(())
    }

    /// 恢复 Agent.
    pub async fn resume(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))?;
        entry.agent.resume(ctx.clone()).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent resumed");
        Ok(())
    }

    /// 取消 Agent.
    pub async fn cancel(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))?;
        let name = entry.name.clone();
        entry.agent.cancel(ctx.clone()).await?;
        entry.status = entry.agent.status(ctx.clone()).await?;
        info!(agent_id = %agent_id, status = ?entry.status, "agent cancelled");
        self.emit_event(
            lingshu_plugin::event::EventType::AgentFailed("cancelled".into()),
            agent_id,
            &name,
            serde_json::json!({"status": "cancelled"}),
        )
        .await;
        Ok(())
    }

    /// 获取快照.
    pub async fn snapshot(&self, agent_id: &LsId, ctx: &LsContext) -> LsResult<AgentSnapshot> {
        let agents = self.agents.read().await;
        let entry = agents
            .get(agent_id)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))?;
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
        let entry = agents
            .get_mut(agent_id)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))?;
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
                status: e.status,
                created_at: e.created_at,
            })
            .collect()
    }

    /// 移除 Agent.
    pub async fn remove(&self, agent_id: &LsId) -> LsResult<()> {
        let mut agents = self.agents.write().await;
        let entry = agents
            .remove(agent_id)
            .ok_or_else(|| lingshu_core::LsError::NotFound(format!("agent {agent_id}")))?;
        info!(agent_id = %agent_id, "agent removed");
        self.emit_event(
            lingshu_plugin::event::EventType::AgentFailed("removed".into()),
            agent_id,
            &entry.name,
            serde_json::json!({"status": "removed"}),
        )
        .await;
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
                status: self.status,
                context: LsContext::with_session(LsId::new()),
                state: Vec::new(),
                created_at: chrono::Utc::now(),
            })
        }
        async fn restore(&mut self, _ctx: LsContext, _snap: AgentSnapshot) -> LsResult<()> {
            Ok(())
        }
        async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> {
            Ok(self.status)
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let mgr = AgentManager::new();
        assert_eq!(mgr.count().await, 0);
        let id = LsId::new();
        mgr.register(
            id,
            "test".into(),
            Box::new(TestAgent {
                id,
                status: AgentStatus::Idle,
            }),
        )
        .await;
        assert_eq!(mgr.count().await, 1);
        let list = mgr.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test");
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let mgr = AgentManager::new();
        let id = LsId::new();
        mgr.register(
            id,
            "test".into(),
            Box::new(TestAgent {
                id,
                status: AgentStatus::Running,
            }),
        )
        .await;
        let ctx = LsContext::with_session(LsId::new());
        mgr.pause(&id, &ctx).await.unwrap();
        assert_eq!(mgr.status(&id).await.unwrap(), AgentStatus::Paused);
        mgr.resume(&id, &ctx).await.unwrap();
        assert_eq!(mgr.status(&id).await.unwrap(), AgentStatus::Running);
    }

    #[tokio::test]
    async fn test_remove() {
        let mgr = AgentManager::new();
        let id = LsId::new();
        mgr.register(
            id,
            "test".into(),
            Box::new(TestAgent {
                id,
                status: AgentStatus::Idle,
            }),
        )
        .await;
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

    #[tokio::test]
    async fn test_with_event_bus() {
        #[allow(unused_imports)]
        use lingshu_plugin::event::{EventBus, EventType};
        let bus = Arc::new(lingshu_plugin::event::EventBus::new());
        let mgr = AgentManager::with_event_bus(bus.clone());
        let id = LsId::new();
        mgr.register(
            id,
            "event-test".into(),
            Box::new(TestAgent {
                id,
                status: AgentStatus::Idle,
            }),
        )
        .await;
        assert_eq!(mgr.count().await, 1);
    }
}

// ── Agent 执行追踪 ─────────────────────────────────────

/// 使用 OTel GenAI span 包裹 agent 执行。
///
/// 返回 `(agent_output, span)` 以允许调用方进一步记录 span 属性。
pub async fn traced_agent_run(
    agent: &mut dyn lingshu_traits::agent::Agent,
    ctx: LsContext,
    input: serde_json::Value,
    agent_name: &str,
) -> LsResult<lingshu_traits::agent::AgentOutput> {
    let span = tracing::info_span!(
        "gen_ai",
        gen_ai.operation.name = "agent.run",
        gen_ai.agent.name = agent_name,
        trace_id = %ctx.trace_id,
        session_id = %ctx.session_id,
    );
    let _guard = span.enter();
    let start = std::time::Instant::now();

    let result = agent.run(ctx, input).await;

    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    match &result {
        Ok(output) => {
            tracing::debug!(
                duration_ms,
                agent_id = %output.agent_id,
                status = ?output.status,
                "Agent run completed",
            );
        }
        Err(e) => {
            tracing::warn!(duration_ms, error = %e, "Agent run failed");
        }
    }

    result
}
