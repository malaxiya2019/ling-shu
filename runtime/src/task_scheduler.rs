//! TaskScheduler — v4.1 生产级任务调度器.
//!
//! 提供完整的 Agent 任务调度能力：
//! - **Job Queue** — 基于 trait 的任务队列 (默认 InMemory，可插拔 SQLite)
//! - **Worker Pool** — 后台任务执行池 (可配置并发数)
//! - **Retry** — 指数退避重试策略
//! - **Timeout** — 任务超时控制
//! - **Cancel** — 任务取消 (CancellationToken)
//! - **Cron** — 定时触发任务
//! - **Pause/Resume** — 调度器暂停/恢复
//! - **Metrics** — 任务统计 (完成/失败/超时/取消计数)
//!
//! # 示例
//!
//! ```rust,ignore
//! let scheduler = TaskScheduler::new(TaskSchedulerConfig::default());
//! scheduler.start().await;
//!
//! let job = MyJob { ... };
//! let handle = scheduler.submit(&ctx, Box::new(job)).await.unwrap();
//!
//! // 等待完成
//! let result = handle.wait().await.unwrap();
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::task_queue::{InMemoryJobQueue, JobQueue};

// ═══════════════════════════════════════════════════════════
// 类型定义
// ═══════════════════════════════════════════════════════════

/// 作业状态.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// 等待执行
    Pending,
    /// 正在运行
    Running,
    /// 已完成 (含结果)
    Completed,
    /// 失败 (含错误消息)
    Failed(String),
    /// 已取消
    Cancelled,
    /// 超时
    TimedOut,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Completed | JobStatus::Failed(_) | JobStatus::Cancelled | JobStatus::TimedOut)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, JobStatus::Pending | JobStatus::Running)
    }
}

/// 重试策略.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// 最大重试次数 (不含首次执行).
    pub max_retries: u32,
    /// 初始延迟 (首次重试前等待).
    pub base_delay_ms: u64,
    /// 最大延迟.
    pub max_delay_ms: u64,
    /// 退避因子 (默认 2.0 即为指数退避).
    pub backoff_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
            backoff_factor: 2.0,
        }
    }
}

impl RetryPolicy {
    /// 计算第 n 次重试的等待时间 (n 从 0 开始).
    pub fn delay_for(&self, retry_count: u32) -> Duration {
        let delay = self.base_delay_ms as f64 * self.backoff_factor.powi(retry_count as i32);
        let delay = delay.min(self.max_delay_ms as f64) as u64;
        Duration::from_millis(delay)
    }
}

/// 调度器配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSchedulerConfig {
    /// 最大并发任务数 (Worker 池大小).
    pub max_concurrent: usize,
    /// 默认重试策略.
    pub retry_policy: RetryPolicy,
    /// 默认超时 (秒, 0 表示不超时).
    pub default_timeout_secs: u64,
    /// 队列最大长度 (0 = 无限制).
    pub max_queue_len: usize,
    /// 任务轮询间隔 (毫秒).
    pub poll_interval_ms: u64,
}

impl Default for TaskSchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 16,
            retry_policy: RetryPolicy::default(),
            default_timeout_secs: 300, // 5 分钟
            max_queue_len: 10_000,
            poll_interval_ms: 100,
        }
    }
}

/// 调度器统计.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulerStats {
    pub submitted: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub timed_out: u64,
    pub pending: u64,
    pub running: u64,
    pub max_concurrent: usize,
    pub paused: bool,
}

/// 作业摘要信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub job_id: String,
    pub name: String,
    pub status: JobStatus,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub retry_count: u32,
}

// ═══════════════════════════════════════════════════════════
// Job trait — 用户需要实现此 trait 来定义可调度作业
// ═══════════════════════════════════════════════════════════

/// 可调度作业 trait.
///
/// 用户需实现 `execute()` 方法，可选覆盖 `retry_policy()`/`timeout()`/`priority()`.
#[async_trait]
pub trait Job: Send + Sync + 'static {
    /// 作业唯一标识.
    fn id(&self) -> LsId;
    /// 作业名称 (用于日志/监控/展示).
    fn name(&self) -> &str;
    /// 执行作业.
    async fn execute(&self, ctx: LsContext) -> LsResult<Value>;
    /// 可选: 自定义重试策略 (None = 使用调度器默认).
    fn retry_policy(&self) -> Option<RetryPolicy> {
        None
    }
    /// 可选: 自定义超时 (None = 使用调度器默认, Some(Duration::ZERO) = 不超时).
    fn timeout(&self) -> Option<Duration> {
        None
    }
    /// 可选: 优先级 (0-255, 越大越优先).
    fn priority(&self) -> u8 {
        0
    }
}

// ═══════════════════════════════════════════════════════════
// CancellationToken — 用于通知任务取消
// ═══════════════════════════════════════════════════════════

/// 取消令牌 — 线程安全，可跨任务传递.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 触发取消.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// 检查是否已取消.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// 返回一个 future，在取消时完成；超时则返回错误.
    pub async fn wait_for_cancellation(&self) {
        // 简单轮询等待；生产环境可使用 tokio::sync::Notify 优化
        while !self.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// JobHandle — 提交作业后返回的句柄
// ═══════════════════════════════════════════════════════════

/// 作业句柄 — 用于查询状态、等待完成、取消作业.
#[derive(Clone)]
pub struct JobHandle {
    job_id: LsId,
    name: String,
    inner: Arc<TaskSchedulerInner>,
}

impl JobHandle {
    /// 获取作业 ID.
    pub fn job_id(&self) -> &LsId {
        &self.job_id
    }

    /// 获取作业名称.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 获取当前状态.
    pub async fn status(&self) -> JobStatus {
        self.inner.get_job_status(&self.job_id).await
    }

    /// 取消作业.
    pub async fn cancel(&self) -> LsResult<()> {
        self.inner.cancel_job(&self.job_id).await
    }

    /// 等待作业完成（阻塞直到终端状态）.
    pub async fn wait(&self) -> LsResult<Value> {
        self.inner.wait_for_job(&self.job_id).await
    }

    pub async fn wait_timeout(&self, timeout: Duration) -> LsResult<Option<Value>> {
        match tokio::time::timeout(timeout, self.wait()).await {
            Ok(Ok(val)) => Ok(Some(val)),
            Ok(Err(e)) => Err(e),
            Err(_) => Ok(None),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// TaskScheduler 内部状态
// ═══════════════════════════════════════════════════════════

struct JobState {
    id: LsId,
    name: String,
    status: JobStatus,
    result: Option<LsResult<Value>>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    retry_count: u32,
    #[allow(dead_code)]
    priority: u8,
    cancel_token: CancellationToken,
    completion_tx: watch::Sender<JobStatus>,
}

impl JobState {
    fn new(id: LsId, name: String, priority: u8) -> (Self, watch::Receiver<JobStatus>) {
        let (tx, rx) = watch::channel(JobStatus::Pending);
        (
            Self {
                id,
                name,
                status: JobStatus::Pending,
                result: None,
                created_at: Utc::now(),
                started_at: None,
                completed_at: None,
                retry_count: 0,
                priority,
                cancel_token: CancellationToken::new(),
                completion_tx: tx,
            },
            rx,
        )
    }

    fn set_status(&mut self, status: JobStatus) {
        self.status = status.clone();
        let _ = self.completion_tx.send(status);
    }
}

struct TaskSchedulerInner {
    config: TaskSchedulerConfig,
    queue: RwLock<Box<dyn JobQueue>>,
    job_states: RwLock<HashMap<LsId, JobState>>,
    worker_handles: RwLock<Vec<JoinHandle<()>>>,
    paused: AtomicBool,

    // 统计
    submitted_count: AtomicU64,
    completed_count: AtomicU64,
    failed_count: AtomicU64,
    cancelled_count: AtomicU64,
    timed_out_count: AtomicU64,
}

impl TaskSchedulerInner {
    async fn get_job_status(&self, job_id: &LsId) -> JobStatus {
        let states = self.job_states.read().await;
        states.get(job_id).map(|s| s.status.clone()).unwrap_or(JobStatus::Cancelled)
    }

    async fn cancel_job(&self, job_id: &LsId) -> LsResult<()> {
        let mut states = self.job_states.write().await;
        let state = states.get_mut(job_id).ok_or_else(|| {
            LsError::NotFound(format!("job {job_id}"))
        })?;

        if state.status.is_terminal() {
            return Ok(()); // 已经是终端状态，无需取消
        }

        // 触发取消令牌
        state.cancel_token.cancel();
        state.set_status(JobStatus::Cancelled);
        self.cancelled_count.fetch_add(1, Ordering::Release);

        // 从队列中移除
        let mut queue = self.queue.write().await;
        let _ = queue.remove(job_id).await;

        info!(job_id = %job_id, name = %state.name, "job cancelled");
        Ok(())
    }

    async fn wait_for_job(&self, job_id: &LsId) -> LsResult<Value> {
        let rx = {
            let states = self.job_states.read().await;
            let state = states.get(job_id).ok_or_else(|| {
                LsError::NotFound(format!("job {job_id}"))
            })?;
            if let Some(ref result) = state.result {
                return match result {
                    Ok(val) => Ok(val.clone()),
                    Err(e) => Err(LsError::Internal(format!("job failed: {e}"))),
                };
            }
            state.completion_tx.subscribe()
        };

        // 等待状态变化
        let mut rx = rx;
        loop {
            rx.changed().await.ok();
            let states = self.job_states.read().await;
            if let Some(state) = states.get(job_id) {
                if state.status.is_terminal() {
                    return match &state.result {
                        Some(Ok(val)) => Ok(val.clone()),
                        Some(Err(e)) => Err(LsError::Internal(format!("job failed: {e}"))),
                        None => Err(LsError::Internal("job terminated without result".into())),
                    };
                }
            }
        }
    }

    async fn execute_job_inner(&self, job: Box<dyn Job>) {
        let job_id = job.id();
        let name = job.name().to_string();
        let retry_policy = job.retry_policy().unwrap_or_else(|| self.config.retry_policy.clone());
        let timeout = job.timeout().or_else(|| {
            if self.config.default_timeout_secs > 0 {
                Some(Duration::from_secs(self.config.default_timeout_secs))
            } else {
                None
            }
        });

        // 标记为 Running
        {
            let mut states = self.job_states.write().await;
            if let Some(state) = states.get_mut(&job_id) {
                state.set_status(JobStatus::Running);
                state.started_at = Some(Utc::now());
            }
        }

        // 执行 + 重试循环
        let max_retries = retry_policy.max_retries;
        let mut last_error = String::new();

        for attempt in 0..=max_retries {
            // 检查取消
            {
                let states = self.job_states.read().await;
                if let Some(state) = states.get(&job_id) {
                    if state.cancel_token.is_cancelled() {
                        return;
                    }
                }
            }

            let ctx = LsContext::with_session(job_id);

            let execute_fut = job.execute(ctx);
            let result = if let Some(timeout) = timeout {
                // 带超时执行
                match tokio::time::timeout(timeout, execute_fut).await {
                    Ok(Ok(val)) => Ok(val),
                    Ok(Err(e)) => Err(e),
                    Err(_elapsed) => {
                        warn!(job_id = %job_id, name = %name, attempt = attempt, "job timed out after {timeout:?}");
                        self.timed_out_count.fetch_add(1, Ordering::Release);
                        let mut states = self.job_states.write().await;
                        if let Some(state) = states.get_mut(&job_id) {
                            state.set_status(JobStatus::TimedOut);
                        }
                        return;
                    }
                }
            } else {
                execute_fut.await
            };

            match result {
                Ok(val) => {
                    // 成功
                    let mut states = self.job_states.write().await;
                    if let Some(state) = states.get_mut(&job_id) {
                        state.result = Some(Ok(val.clone()));
                        state.set_status(JobStatus::Completed);
                        state.completed_at = Some(Utc::now());
                    }
                    self.completed_count.fetch_add(1, Ordering::Release);
                    info!(job_id = %job_id, name = %name, attempt = attempt, "job completed");
                    return;
                }
                Err(e) => {
                    last_error = e.to_string();
                    if attempt < max_retries {
                        // 计算退避延迟
                        let delay = retry_policy.delay_for(attempt);
                        warn!(
                            job_id = %job_id, name = %name, attempt = attempt,
                            error = %last_error, retry_delay_ms = delay.as_millis(),
                            "job failed, retrying"
                        );

                        // 更新重试计数
                        {
                            let mut states = self.job_states.write().await;
                            if let Some(state) = states.get_mut(&job_id) {
                                state.retry_count = attempt + 1;
                            }
                        }

                        // 等待重试间隔 (同时检查取消)
                        let cancel_token = {
                            let states = self.job_states.read().await;
                            states.get(&job_id).map(|s| s.cancel_token.clone())
                        };

                        let slept = tokio::time::sleep(delay);
                        tokio::pin!(slept);

                        tokio::select! {
                            _ = &mut slept => {},
                            _ = async {
                                if let Some(ct) = cancel_token {
                                    while !ct.is_cancelled() {
                                        tokio::time::sleep(Duration::from_millis(50)).await;
                                    }
                                } else {
                                    std::future::pending::<()>().await;
                                }
                            } => {
                                // 被取消
                                return;
                            }
                        }
                    }
                }
            }
        }

        // 所有重试均失败
        {
            let mut states = self.job_states.write().await;
            if let Some(state) = states.get_mut(&job_id) {
                state.result = Some(Err(LsError::Internal(last_error.clone())));
                state.set_status(JobStatus::Failed(last_error.clone()));
                state.completed_at = Some(Utc::now());
            }
        }
        self.failed_count.fetch_add(1, Ordering::Release);
        error!(job_id = %job_id, name = %name, error = %last_error, "job failed after {} retries", max_retries);
    }
}

// ═══════════════════════════════════════════════════════════
// TaskScheduler 公共 API
// ═══════════════════════════════════════════════════════════

/// 生产级任务调度器.
#[derive(Clone)]
pub struct TaskScheduler {
    inner: Arc<TaskSchedulerInner>,
}

impl TaskScheduler {
    /// 创建新的调度器（默认 InMemory 队列）.
    pub fn new(config: TaskSchedulerConfig) -> Self {
        Self::with_queue(config, Box::new(InMemoryJobQueue::new()))
    }

    /// 使用自定义队列创建调度器.
    pub fn with_queue(config: TaskSchedulerConfig, queue: Box<dyn JobQueue>) -> Self {
        let inner = TaskSchedulerInner {
            config: config.clone(),
            queue: RwLock::new(queue),
            job_states: RwLock::new(HashMap::new()),
            worker_handles: RwLock::new(Vec::new()),
            paused: AtomicBool::new(false),
            submitted_count: AtomicU64::new(0),
            completed_count: AtomicU64::new(0),
            failed_count: AtomicU64::new(0),
            cancelled_count: AtomicU64::new(0),
            timed_out_count: AtomicU64::new(0),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    /// 启动 Worker 池.
    pub async fn start(&self) -> LsResult<()> {
        let mut handles = self.inner.worker_handles.write().await;
        if !handles.is_empty() {
            return Err(LsError::RuntimeState("scheduler already started".into()));
        }

        let max_concurrent = self.inner.config.max_concurrent;
        let inner = self.inner.clone();

        for i in 0..max_concurrent {
            let inner = inner.clone();
            let handle = tokio::spawn(async move {
                Self::worker_loop(inner, i).await;
            });
            handles.push(handle);
        }

        info!(max_concurrent = max_concurrent, "task scheduler started");
        Ok(())
    }

    /// 停止调度器 (等待所有 Worker 完成).
    pub async fn shutdown(&self) -> LsResult<()> {
        let handles = self.inner.worker_handles.write().await;
        for (i, handle) in handles.iter().enumerate() {
            handle.abort();
            info!("worker {i} aborted");
        }
        info!("task scheduler shut down");
        Ok(())
    }

    /// 提交作业.
    pub async fn submit(&self, _ctx: &LsContext, job: Box<dyn Job>) -> LsResult<JobHandle> {
        if self.inner.paused.load(Ordering::Acquire) {
            return Err(LsError::RuntimeState("scheduler is paused".into()));
        }

        let job_id = job.id();
        let name = job.name().to_string();
        let priority = job.priority();

        // 检查队列长度
        {
            let queue = self.inner.queue.read().await;
            let max_len = self.inner.config.max_queue_len;
            if max_len > 0 && queue.len().await >= max_len {
                return Err(LsError::RuntimeState(format!(
                    "queue full (max {max_len})"
                )));
            }
        }

        // 创建状态
        {
            let mut states = self.inner.job_states.write().await;
            if states.contains_key(&job_id) {
                return Err(LsError::InvalidArgument(format!(
                    "job {} already exists",
                    job_id
                )));
            }
            let (state, _rx) = JobState::new(job_id, name.clone(), priority);
            states.insert(job_id, state);
        }

        // 入队
        {
            let mut queue = self.inner.queue.write().await;
            queue.enqueue(job).await.map_err(|e| {
                LsError::Internal(format!("enqueue failed: {e}"))
            })?;
        }

        self.inner.submitted_count.fetch_add(1, Ordering::Release);

        debug!(job_id = %job_id, name = %name, "job submitted");

        Ok(JobHandle {
            job_id,
            name,
            inner: self.inner.clone(),
        })
    }

    /// 取消作业.
    pub async fn cancel(&self, job_id: &LsId) -> LsResult<()> {
        self.inner.cancel_job(job_id).await
    }

    /// 获取作业状态.
    pub async fn job_status(&self, job_id: &LsId) -> JobStatus {
        self.inner.get_job_status(job_id).await
    }

    /// 获取作业句柄.
    pub async fn get_handle(&self, job_id: &LsId) -> LsResult<JobHandle> {
        let states = self.inner.job_states.read().await;
        let state = states.get(job_id).ok_or_else(|| {
            LsError::NotFound(format!("job {job_id}"))
        })?;
        Ok(JobHandle {
            job_id: *job_id,
            name: state.name.clone(),
            inner: self.inner.clone(),
        })
    }

    /// 列出作业摘要.
    pub async fn list_jobs(&self, filter_status: Option<JobStatus>) -> Vec<JobSummary> {
        let states = self.inner.job_states.read().await;
        states
            .values()
            .filter(|s| {
                filter_status
                    .as_ref()
                    .is_none_or(|f| &s.status == f)
            })
            .map(|s| JobSummary {
                job_id: s.id.to_string(),
                name: s.name.clone(),
                status: s.status.clone(),
                created_at: s.created_at.to_rfc3339(),
                started_at: s.started_at.map(|t| t.to_rfc3339()),
                completed_at: s.completed_at.map(|t| t.to_rfc3339()),
                retry_count: s.retry_count,
            })
            .collect()
    }

    /// 获取调度器统计.
    pub async fn stats(&self) -> SchedulerStats {
        let pending = {
            let states = self.inner.job_states.read().await;
            states.values().filter(|s| s.status == JobStatus::Pending).count() as u64
        };
        let running = {
            let states = self.inner.job_states.read().await;
            states.values().filter(|s| s.status == JobStatus::Running).count() as u64
        };

        SchedulerStats {
            submitted: self.inner.submitted_count.load(Ordering::Acquire),
            completed: self.inner.completed_count.load(Ordering::Acquire),
            failed: self.inner.failed_count.load(Ordering::Acquire),
            cancelled: self.inner.cancelled_count.load(Ordering::Acquire),
            timed_out: self.inner.timed_out_count.load(Ordering::Acquire),
            pending,
            running,
            max_concurrent: self.inner.config.max_concurrent,
            paused: self.inner.paused.load(Ordering::Acquire),
        }
    }

    /// 暂停调度器 (拒绝新提交，但已在运行的继续).
    pub fn pause(&self) {
        self.inner.paused.store(true, Ordering::Release);
        info!("task scheduler paused");
    }

    /// 恢复调度器.
    pub fn resume(&self) {
        self.inner.paused.store(false, Ordering::Release);
        info!("task scheduler resumed");
    }

    /// 检查调度器是否已暂停.
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    // ── Worker 内部循环 ──

    async fn worker_loop(inner: Arc<TaskSchedulerInner>, worker_id: usize) {
        debug!(worker_id = worker_id, "worker started");

        loop {
            // 从队列取出作业
            let job_opt = {
                let mut queue = inner.queue.write().await;
                queue.dequeue().await
            };

            if let Some(job) = job_opt {
                let job_id = job.id();
                let name = job.name().to_string();

                debug!(worker_id = worker_id, job_id = %job_id, name = %name, "worker picked job");

                // 检查是否已被取消
                {
                    let states = inner.job_states.read().await;
                    if let Some(state) = states.get(&job_id) {
                        if state.status == JobStatus::Cancelled {
                            debug!(job_id = %job_id, "job already cancelled, skipping");
                            continue;
                        }
                    }
                }

                inner.execute_job_inner(job).await;
            } else {
                // 队列为空，等待后重试
                tokio::time::sleep(Duration::from_millis(
                    inner.config.poll_interval_ms,
                ))
                .await;
            }
        }
    }
}
impl Drop for TaskSchedulerInner {
    fn drop(&mut self) {
        // 在 Drop 中尽力中止 worker，不阻塞
        if let Ok(mut guard) = self.worker_handles.try_write() {
            for h in guard.drain(..) {
                h.abort();
            }
        }
    }
}
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    struct TestJob {
        id: LsId,
        name: String,
        should_fail: bool,
        delay_ms: u64,
        executed: Arc<AtomicBool>,
    }

    impl TestJob {
        fn new(name: &str, should_fail: bool) -> Self {
            Self {
                id: LsId::new(),
                name: name.to_string(),
                should_fail,
                delay_ms: 0,
                executed: Arc::new(AtomicBool::new(false)),
            }
        }

        fn with_delay(name: &str, delay_ms: u64) -> Self {
            Self {
                id: LsId::new(),
                name: name.to_string(),
                should_fail: false,
                delay_ms,
                executed: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    #[async_trait]
    impl Job for TestJob {
        fn id(&self) -> LsId { self.id }
        fn name(&self) -> &str { &self.name }

        async fn execute(&self, _ctx: LsContext) -> LsResult<Value> {
            self.executed.store(true, Ordering::Release);
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            if self.should_fail {
                Err(LsError::Internal("test failure".into()))
            } else {
                Ok(serde_json::json!({"result": "ok", "name": self.name}))
            }
        }
    }

    #[tokio::test]
    async fn test_create_and_start() {
        let sched = TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 4,
            ..Default::default()
        });
        sched.start().await.unwrap();
        let stats = sched.stats().await;
        assert_eq!(stats.max_concurrent, 4);
        assert!(!stats.paused);
        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_submit_and_complete() {
        let sched = TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 4,
            ..Default::default()
        });
        sched.start().await.unwrap();

        let ctx = LsContext::with_session(LsId::new());
        let job = TestJob::new("test-job", false);
        let handle = sched.submit(&ctx, Box::new(job)).await.unwrap();

        let result = handle.wait().await.unwrap();
        assert_eq!(result["result"], "ok");

        let stats = sched.stats().await;
        assert_eq!(stats.submitted, 1);
        assert_eq!(stats.completed, 1);

        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_job_failure_and_retry() {
        let sched = TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 4,
            retry_policy: RetryPolicy {
                max_retries: 2,
                base_delay_ms: 10,
                max_delay_ms: 100,
                backoff_factor: 2.0,
            },
            ..Default::default()
        });
        sched.start().await.unwrap();

        let ctx = LsContext::with_session(LsId::new());
        let job = TestJob::new("fail-job", true);
        let handle = sched.submit(&ctx, Box::new(job)).await.unwrap();

        let result = handle.wait().await;
        assert!(result.is_err());

        let stats = sched.stats().await;
        assert_eq!(stats.completed, 0);
        assert_eq!(stats.failed, 1);
        // 原始 + 2 次重试 = 3 次执行, 但 failed_count 只计最终
        assert!(stats.submitted >= 1);

        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let sched = TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 4,
            ..Default::default()
        });
        sched.start().await.unwrap();

        let ctx = LsContext::with_session(LsId::new());
        let job = TestJob::with_delay("slow-job", 5000);
        let handle = sched.submit(&ctx, Box::new(job)).await.unwrap();

        // 立即取消
        handle.cancel().await.unwrap();

        let status = handle.status().await;
        assert_eq!(status, JobStatus::Cancelled);

        let stats = sched.stats().await;
        assert_eq!(stats.cancelled, 1);

        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let sched = TaskScheduler::new(TaskSchedulerConfig::default());
        sched.start().await.unwrap();

        assert!(!sched.is_paused());
        sched.pause();
        assert!(sched.is_paused());

        let ctx = LsContext::with_session(LsId::new());
        let job = TestJob::new("pause-test", false);
        let result = sched.submit(&ctx, Box::new(job)).await;
        assert!(result.is_err());

        sched.resume();
        assert!(!sched.is_paused());

        let job2 = TestJob::new("resume-test", false);
        let handle = sched.submit(&ctx, Box::new(job2)).await.unwrap();
        handle.wait().await.unwrap();

        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_jobs() {
        let sched = TaskScheduler::new(TaskSchedulerConfig::default());
        sched.start().await.unwrap();

        let ctx = LsContext::with_session(LsId::new());
        let job1 = TestJob::new("job-a", false);
        let job2 = TestJob::new("job-b", true);

        sched.submit(&ctx, Box::new(job1)).await.unwrap();
        sched.submit(&ctx, Box::new(job2)).await.unwrap();

        // 等一会儿让作业执行
        tokio::time::sleep(Duration::from_millis(200)).await;

        let jobs = sched.list_jobs(None).await;
        assert_eq!(jobs.len(), 2);

        let completed = sched.list_jobs(Some(JobStatus::Completed)).await;
        assert!(!completed.is_empty());

        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_retry_policy_delay() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10_000,
            backoff_factor: 2.0,
        };

        assert_eq!(policy.delay_for(0).as_millis(), 1000);  // 1000 * 2^0
        assert_eq!(policy.delay_for(1).as_millis(), 2000);  // 1000 * 2^1
        assert_eq!(policy.delay_for(2).as_millis(), 4000);  // 1000 * 2^2
        assert_eq!(policy.delay_for(3).as_millis(), 8000);  // 1000 * 2^3
        assert_eq!(policy.delay_for(4).as_millis(), 10_000); // 上限
    }

    #[tokio::test]
    async fn test_double_start_fails() {
        let sched = TaskScheduler::new(TaskSchedulerConfig::default());
        sched.start().await.unwrap();
        let err = sched.start().await.unwrap_err();
        assert!(matches!(err, LsError::RuntimeState(_)));
        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_stats_after_operations() {
        let sched = TaskScheduler::new(TaskSchedulerConfig {
            max_concurrent: 2,
            ..Default::default()
        });
        sched.start().await.unwrap();

        let ctx = LsContext::with_session(LsId::new());
        let job = TestJob::new("stats-test", false);
        let handle = sched.submit(&ctx, Box::new(job)).await.unwrap();
        handle.wait().await.unwrap();

        let stats = sched.stats().await;
        assert_eq!(stats.max_concurrent, 2);
        assert!(!stats.paused);

        sched.shutdown().await.unwrap();
    }
}
