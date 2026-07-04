use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 任务优先级.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// 任务状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 任务描述.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: LsId,
    pub session_id: LsId,
    pub priority: Priority,
    pub status: TaskStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tags: HashMap<String, String>,
}

/// 调度配额.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub max_concurrent: u32,
    pub current_concurrent: u32,
    pub max_queue_size: u32,
    pub current_queue_size: u32,
}

/// Scheduler — 任务提交、调度控制、配额与统计查询.
#[async_trait]
pub trait Scheduler: Send + Sync + 'static {
    /// 提交一个异步任务.
    async fn submit(&self, ctx: LsContext, task: Box<dyn FnOnce() + Send>) -> LsResult<LsId>;

    /// 取消指定任务.
    async fn cancel(&self, ctx: LsContext, task_id: LsId) -> LsResult<()>;

    /// 查询任务信息.
    async fn get_task(&self, ctx: LsContext, task_id: LsId) -> LsResult<TaskInfo>;

    /// 暂停调度器（不再接受新任务）.
    async fn pause(&self, ctx: LsContext) -> LsResult<()>;

    /// 恢复调度器.
    async fn resume(&self, ctx: LsContext) -> LsResult<()>;

    /// 查询配额与负载信息.
    async fn quota(&self, ctx: LsContext) -> LsResult<QuotaInfo>;
}
