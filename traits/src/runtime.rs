use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};

/// Runtime 运行状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeStatus {
    Uninitialized,
    Initializing,
    Running,
    Paused,
    ShuttingDown,
    Stopped,
}

/// Runtime 全局统计.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStats {
    pub uptime_seconds: u64,
    pub active_sessions: u64,
    pub active_tasks: u64,
    pub total_tasks_completed: u64,
    pub total_tasks_failed: u64,
}

/// Runtime — 运行时启停、会话管理、全局状态查询.
#[async_trait]
pub trait Runtime: Send + Sync + 'static {
    /// 初始化运行时.
    async fn initialize(&self, ctx: LsContext) -> LsResult<()>;

    /// 启动运行时.
    async fn start(&self, ctx: LsContext) -> LsResult<()>;

    /// 暂停运行时 (排空后暂停).
    async fn pause(&self, ctx: LsContext) -> LsResult<()>;

    /// 关闭运行时.
    async fn shutdown(&self, ctx: LsContext) -> LsResult<()>;

    /// 查询运行时状态.
    async fn status(&self, ctx: LsContext) -> LsResult<RuntimeStatus>;

    /// 查询运行时统计信息.
    async fn stats(&self, ctx: LsContext) -> LsResult<RuntimeStats>;

    /// 获取或创建会话.
    async fn get_or_create_session(&self, ctx: LsContext, session_id: LsId) -> LsResult<LsContext>;
}
