use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};

/// Agent 状态机.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Running,
    Paused,
    Completed,
    Failed,
}

/// Agent 快照，用于恢复.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub agent_id: LsId,
    pub status: AgentStatus,
    pub context: LsContext,
    pub state: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Agent 执行结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent_id: LsId,
    pub status: AgentStatus,
    pub data: Option<serde_json::Value>,
    pub error: Option<LsError>,
}

/// Agent — 智能体生命周期、执行控制、快照与恢复.
#[async_trait]
pub trait Agent: Send + Sync + 'static {
    /// 返回智能体唯一 ID.
    fn id(&self) -> LsId;

    /// 启动执行.
    async fn run(&mut self, ctx: LsContext, input: serde_json::Value) -> LsResult<AgentOutput>;

    /// 暂停执行.
    async fn pause(&mut self, ctx: LsContext) -> LsResult<()>;

    /// 恢复执行.
    async fn resume(&mut self, ctx: LsContext) -> LsResult<()>;

    /// 取消执行.
    async fn cancel(&mut self, ctx: LsContext) -> LsResult<()>;

    /// 创建当前快照.
    async fn snapshot(&self, ctx: LsContext) -> LsResult<AgentSnapshot>;

    /// 从快照恢复.
    async fn restore(&mut self, ctx: LsContext, snapshot: AgentSnapshot) -> LsResult<()>;

    /// 查询当前状态.
    async fn status(&self, ctx: LsContext) -> LsResult<AgentStatus>;

    /// 重启 Agent（默认实现返回不支持错误）.
    async fn restart(&mut self, ctx: LsContext) -> LsResult<()> {
        let _ = ctx;
        Err(LsError::Unsupported("restart not implemented".into()))
    }

    /// 热更新 Agent 配置（默认实现返回不支持错误）.
    async fn update_config(&mut self, ctx: LsContext, config: serde_json::Value) -> LsResult<()> {
        let _ = (ctx, config);
        Err(LsError::Unsupported("update_config not implemented".into()))
    }
}

// ── Blanket impl: Box<dyn Agent> 也实现 Agent ──────

#[async_trait]
impl<T: Agent + ?Sized> Agent for Box<T> {
    fn id(&self) -> LsId {
        (**self).id()
    }
    async fn run(&mut self, ctx: LsContext, input: serde_json::Value) -> LsResult<AgentOutput> {
        (**self).run(ctx, input).await
    }
    async fn pause(&mut self, ctx: LsContext) -> LsResult<()> {
        (**self).pause(ctx).await
    }
    async fn resume(&mut self, ctx: LsContext) -> LsResult<()> {
        (**self).resume(ctx).await
    }
    async fn cancel(&mut self, ctx: LsContext) -> LsResult<()> {
        (**self).cancel(ctx).await
    }
    async fn snapshot(&self, ctx: LsContext) -> LsResult<AgentSnapshot> {
        (**self).snapshot(ctx).await
    }
    async fn restore(&mut self, ctx: LsContext, snapshot: AgentSnapshot) -> LsResult<()> {
        (**self).restore(ctx, snapshot).await
    }
    async fn status(&self, ctx: LsContext) -> LsResult<AgentStatus> {
        (**self).status(ctx).await
    }
    async fn restart(&mut self, ctx: LsContext) -> LsResult<()> {
        (**self).restart(ctx).await
    }
    async fn update_config(&mut self, ctx: LsContext, config: serde_json::Value) -> LsResult<()> {
        (**self).update_config(ctx, config).await
    }
}
