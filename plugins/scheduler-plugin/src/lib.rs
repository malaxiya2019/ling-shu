//! ⏰ Scheduler Plugin — 定时任务调度插件
//!
//! 让 Agent 能够创建和管理定时任务，支持:
//! - **一次性任务**: 在指定时间执行一次
//! - **间隔任务**: 每 N 秒/分钟执行一次
//! - **Cron 任务**: 使用标准 cron 表达式调度

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

// ===========================================================================
// 任务类型
// ===========================================================================

/// 调度类型.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ScheduleType {
    Once(DateTime<Utc>),
    Interval(u64),
    Cron(String),
}

/// 定时任务.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub schedule: ScheduleType,
    pub instruction: String,
    pub target_channel: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub enabled: bool,
}

impl ScheduledTask {
    fn new(name: String, schedule: ScheduleType, instruction: String, target_channel: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            schedule,
            instruction,
            target_channel,
            created_at: Utc::now(),
            last_run: None,
            run_count: 0,
            enabled: true,
        }
    }
}

// ===========================================================================
// 插件实现
// ===========================================================================

pub struct SchedulerPlugin {
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    total_scheduled: AtomicU64,
    total_executed: Arc<AtomicU64>,
    running: Arc<AtomicU64>,
    info: PluginInfo,
}

impl SchedulerPlugin {
    pub fn new() -> Self {
        let manifest = PluginManifest {
            name: "scheduler".into(),
            version: "1.0.0".into(),
            description: "定时任务调度 — 支持 cron/间隔/一次性任务".into(),
            author: Some("Lingshu Team".into()),
            homepage: Some("https://github.com/malaxiya2019/ling-shu".into()),
            license: Some("MIT".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![
                PluginPermission {
                    resource: "timer".into(),
                    actions: vec!["schedule".into(), "cancel".into(), "list".into()],
                },
                PluginPermission {
                    resource: "agent".into(),
                    actions: vec!["invoke".into()],
                },
            ],
            min_api_version: Some("1.0.0".into()),
        };

        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            total_scheduled: AtomicU64::new(0),
            total_executed: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicU64::new(0)),
            info: PluginInfo {
                plugin_id: LsId::new(),
                manifest,
                status: PluginStatus::Loaded,
                loaded_at: Some(chrono::Utc::now()),
            },
        }
    }

    pub async fn add_task(
        &self,
        name: String,
        schedule: ScheduleType,
        instruction: String,
        target_channel: Option<String>,
    ) -> LsResult<String> {
        let task = ScheduledTask::new(name, schedule, instruction, target_channel);
        let task_id = task.id.clone();
        self.total_scheduled.fetch_add(1, Ordering::SeqCst);
        let mut tasks = self.tasks.write().await;
        tasks.insert(task_id.clone(), task);
        tracing::info!(task_id = %task_id, "定时任务已添加");
        Ok(task_id)
    }

    pub async fn cancel_task(&self, task_id: &str) -> LsResult<bool> {
        let mut tasks = self.tasks.write().await;
        Ok(tasks.remove(task_id).is_some())
    }

    pub async fn list_tasks(&self) -> Vec<ScheduledTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_scheduled": self.total_scheduled.load(Ordering::SeqCst),
            "total_executed": self.total_executed.load(Ordering::SeqCst),
            "status": if self.running.load(Ordering::SeqCst) > 0 { "running" } else { "stopped" },
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst) > 0
    }
}

impl Default for SchedulerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for SchedulerPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "scheduler", "Scheduler plugin initialized");
        Ok(())
    }

    async fn start(&self, ctx: LsContext) -> LsResult<()> {
        tracing::info!(plugin = "scheduler", session = %ctx.session_id, "Scheduler plugin started");

        let tasks = self.tasks.clone();
        let running = self.running.clone();
        let total_executed = self.total_executed.clone();

        tokio::spawn(async move {
            scheduler_loop(tasks, running, total_executed).await;
        });

        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        self.running.store(0, Ordering::SeqCst);
        tracing::info!(plugin = "scheduler", "Scheduler plugin stopped");
        Ok(())
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        self.info.manifest.permissions.clone()
    }
}

// ===========================================================================
// 后台调度循环 (模块级独立函数)
// ===========================================================================

async fn scheduler_loop(
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    running: Arc<AtomicU64>,
    total_executed: Arc<AtomicU64>,
) {
    running.store(1, Ordering::SeqCst);
    tracing::info!("调度器循环已启动");

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let mut tick_count: u64 = 0;

    loop {
        interval.tick().await;
        tick_count += 1;

        if running.load(Ordering::SeqCst) == 0 {
            tracing::info!("调度器已停止");
            break;
        }

        let check_cron = tick_count.is_multiple_of(12);
        let now = Utc::now();
        let mut tasks_to_run: Vec<String> = Vec::new();
        let mut cron_tasks: Vec<(String, String)> = Vec::new();

        {
            let task_map = tasks.read().await;
            for (id, task) in task_map.iter() {
                if !task.enabled { continue; }
                match &task.schedule {
                    ScheduleType::Once(at) => {
                        if now >= *at { tasks_to_run.push(id.clone()); }
                    }
                    ScheduleType::Interval(secs) => {
                        let last = task.last_run.unwrap_or(task.created_at);
                        let elapsed = (now - last).num_seconds() as u64;
                        if elapsed >= *secs { tasks_to_run.push(id.clone()); }
                    }
                    ScheduleType::Cron(expr) => {
                        if check_cron { cron_tasks.push((id.clone(), expr.clone())); }
                    }
                }
            }
        }

        if check_cron {
            for (id, expr) in cron_tasks {
                if let Ok(sched) = expr.parse::<cron::Schedule>() {
                    let should_run = sched.upcoming(Utc).take(1).any(|next| {
                        let diff = (next - now).num_seconds();
                        (0..=5).contains(&diff)
                    });
                    if should_run { tasks_to_run.push(id); }
                }
            }
        }

        for task_id in tasks_to_run {
            let instruction = {
                let task_map = tasks.read().await;
                task_map.get(&task_id).map(|t| t.instruction.clone())
            };

            if let Some(_instr) = instruction {
                tracing::info!(task_id = %task_id, "执行定时任务");
                let mut task_map = tasks.write().await;
                if let Some(task) = task_map.get_mut(&task_id) {
                    task.last_run = Some(Utc::now());
                    task.run_count += 1;
                    if matches!(task.schedule, ScheduleType::Once(_)) {
                        task.enabled = false;
                    }
                }
                total_executed.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_task_creation() {
        let task = ScheduledTask::new(
            "test".into(), ScheduleType::Interval(60),
            "do something".into(), None,
        );
        assert_eq!(task.name, "test");
        assert!(task.enabled);
        assert_eq!(task.run_count, 0);
    }

    #[test]
    fn test_plugin_info() {
        let plugin = SchedulerPlugin::new();
        let info = plugin.info();
        assert_eq!(info.manifest.name, "scheduler");
    }

    #[tokio::test]
    async fn test_add_and_list_tasks() {
        let plugin = SchedulerPlugin::new();
        let id = plugin.add_task(
            "test".into(), ScheduleType::Interval(30),
            "hello".into(), None,
        ).await.unwrap();
        let tasks = plugin.list_tasks().await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, id);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let plugin = SchedulerPlugin::new();
        let id = plugin.add_task(
            "test".into(), ScheduleType::Interval(30),
            "msg".into(), None,
        ).await.unwrap();
        assert!(plugin.cancel_task(&id).await.unwrap());
        assert!(!plugin.cancel_task(&id).await.unwrap());
    }

    #[test]
    fn test_stats() {
        let plugin = SchedulerPlugin::new();
        let stats = plugin.stats();
        assert_eq!(stats["total_scheduled"], 0);
        assert_eq!(stats["total_executed"], 0);
    }
}
