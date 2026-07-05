use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};

/// 内部任务描述.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub task_id: LsId,
    pub session_id: LsId,
    pub priority: u8,
    pub status: TaskState,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// 运行时内部调度器.
#[derive(Debug)]
pub struct InternalScheduler {
    max_concurrent: usize,
    paused: AtomicBool,
    semaphore: Arc<Semaphore>,
    tasks: RwLock<HashMap<LsId, ScheduledTask>>,
    completed_count: AtomicU64,
    failed_count: AtomicU64,
}

impl InternalScheduler {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent,
            paused: AtomicBool::new(false),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            tasks: RwLock::new(HashMap::new()),
            completed_count: AtomicU64::new(0),
            failed_count: AtomicU64::new(0),
        }
    }

    /// 提交任务.
    pub async fn submit<F, T>(&self, ctx: &LsContext, task_id: LsId, _future: F) -> LsResult<()>
    where
        F: Future<Output = LsResult<T>> + Send + 'static,
        T: Send + 'static,
    {
        if self.paused.load(Ordering::Acquire) {
            tracing::warn!(
                trace_id = %ctx.trace_id,
                task_id = %task_id,
                "task rejected: scheduler paused"
            );
            return Err(LsError::RuntimeState("scheduler is paused".into()));
        }

        let task = ScheduledTask {
            task_id,
            session_id: ctx.session_id,
            priority: 0,
            status: TaskState::Pending,
            created_at: chrono::Utc::now(),
        };

        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id, task);
        }

        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| LsError::Internal(format!("semaphore error: {e}")))?;

        tracing::debug!(
            trace_id = %ctx.trace_id,
            task_id = %task_id,
            max_concurrent = self.max_concurrent,
            "task submitted"
        );

        Ok(())
    }

    pub async fn mark_completed(&self, task_id: LsId) -> LsResult<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(&task_id) {
            task.status = TaskState::Completed;
            self.completed_count.fetch_add(1, Ordering::Release);
            tracing::debug!(task_id = %task_id, "task completed");
            Ok(())
        } else {
            tracing::warn!(task_id = %task_id, "task not found for completion");
            Err(LsError::NotFound(format!("task {task_id}")))
        }
    }

    pub async fn mark_failed(&self, task_id: LsId, error: String) -> LsResult<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(&task_id) {
            task.status = TaskState::Failed(error.clone());
            self.failed_count.fetch_add(1, Ordering::Release);
            tracing::warn!(task_id = %task_id, error = %error, "task failed");
            Ok(())
        } else {
            tracing::warn!(task_id = %task_id, "task not found for failure mark");
            Err(LsError::NotFound(format!("task {task_id}")))
        }
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
        tracing::info!("scheduler paused");
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        tracing::info!("scheduler resumed");
    }

    pub async fn active_count(&self) -> usize {
        self.max_concurrent - self.semaphore.available_permits()
    }

    pub async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    pub fn completed_total(&self) -> u64 {
        self.completed_count.load(Ordering::Acquire)
    }

    pub fn failed_total(&self) -> u64 {
        self.failed_count.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_submit_and_count() {
        let sched = InternalScheduler::new(16);
        let ctx = LsContext::with_session(LsId::new());
        let task_id = LsId::new();

        sched.submit(&ctx, task_id, async { Ok(()) }).await.unwrap();
        assert_eq!(sched.task_count().await, 1);
    }

    #[tokio::test]
    async fn test_mark_completed() {
        let sched = InternalScheduler::new(16);
        let ctx = LsContext::with_session(LsId::new());
        let task_id = LsId::new();

        sched.submit(&ctx, task_id, async { Ok(()) }).await.unwrap();
        sched.mark_completed(task_id).await.unwrap();
        assert_eq!(sched.completed_total(), 1);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let sched = InternalScheduler::new(16);
        let ctx = LsContext::with_session(LsId::new());
        let task_id = LsId::new();

        sched.submit(&ctx, task_id, async { Ok(()) }).await.unwrap();
        sched
            .mark_failed(task_id, "test error".into())
            .await
            .unwrap();
        assert_eq!(sched.failed_total(), 1);
    }

    #[tokio::test]
    async fn test_pause_rejects_submit() {
        let sched = InternalScheduler::new(16);
        sched.pause();
        let ctx = LsContext::with_session(LsId::new());
        let err = sched
            .submit(&ctx, LsId::new(), async { Ok(()) })
            .await
            .unwrap_err();
        assert!(matches!(err, LsError::RuntimeState(_)));
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let sched = InternalScheduler::new(16);
        sched.pause();
        let ctx = LsContext::with_session(LsId::new());
        assert!(sched
            .submit(&ctx, LsId::new(), async { Ok(()) })
            .await
            .is_err());

        sched.resume();
        assert!(sched
            .submit(&ctx, LsId::new(), async { Ok(()) })
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_active_count() {
        let sched = InternalScheduler::new(2);
        assert_eq!(sched.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_mark_completed_not_found() {
        let sched = InternalScheduler::new(16);
        let err = sched.mark_completed(LsId::new()).await.unwrap_err();
        assert!(matches!(err, LsError::NotFound(_)));
    }
}
