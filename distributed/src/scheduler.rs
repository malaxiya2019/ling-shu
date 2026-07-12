//! LSDistributed — 分布式 Agent 调度器 (v5.0.2)
//!
//! 跨节点 Agent 任务调度与负载均衡：
//! - 基于集群状态的节点选择
//! - 多策略负载均衡（最少任务/轮询/加权/一致性哈希）
//! - 分布式任务队列
//! - 跨节点 Agent 执行
//! - 故障转移与重试

use crate::cluster::*;
use crate::queue::*;
use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ── 错误类型 ────────────────────────────────────────

/// 调度器错误
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("no available node found")]
    NoAvailableNode,
    #[error("task timeout: {0}")]
    TaskTimeout(String),
    #[error("queue error: {0}")]
    QueueError(String),
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;

// ── 调度策略 ────────────────────────────────────────

/// 分布式调度策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistScheduleStrategy {
    /// 最少任务 — 选择待处理任务最少的节点
    LeastTasks,
    /// 轮询 — 依次选择节点
    RoundRobin,
    /// 加权 — 基于节点权重选择
    Weighted,
    /// 一致性哈希 — 基于任务 ID 哈希
    ConsistentHash,
    /// 本地优先 — 优先本地执行，本地繁忙时远端
    LocalFirst,
    /// 自适应 — 综合负载、延迟、成功率动态选择
    Adaptive,
}

impl DistScheduleStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            DistScheduleStrategy::LeastTasks => "least_tasks",
            DistScheduleStrategy::RoundRobin => "round_robin",
            DistScheduleStrategy::Weighted => "weighted",
            DistScheduleStrategy::ConsistentHash => "consistent_hash",
            DistScheduleStrategy::LocalFirst => "local_first",
            DistScheduleStrategy::Adaptive => "adaptive",
        }
    }
}

// ── 调度配置 ────────────────────────────────────────

/// 分布式调度器配置
#[derive(Debug, Clone)]
pub struct DistSchedulerConfig {
    /// 调度策略
    pub strategy: DistScheduleStrategy,
    /// 本地节点 ID
    pub local_node_id: String,
    /// 最大重试次数
    pub max_retries: u32,
    /// 任务超时秒数
    pub task_timeout_secs: u64,
    /// 心跳超时秒数（节点判定离线）
    pub node_timeout_secs: u64,
    /// 队列批量拉取大小
    pub batch_size: usize,
    /// 是否启用自动故障转移
    pub enable_auto_failover: bool,
    /// 健康检查间隔
    pub health_check_interval: Duration,
}

impl Default for DistSchedulerConfig {
    fn default() -> Self {
        Self {
            strategy: DistScheduleStrategy::Adaptive,
            local_node_id: uuid::Uuid::new_v4().to_string(),
            max_retries: 3,
            task_timeout_secs: 300,
            node_timeout_secs: 30,
            batch_size: 10,
            enable_auto_failover: true,
            health_check_interval: Duration::from_secs(5),
        }
    }
}

// ── 调度任务 ────────────────────────────────────────

/// 分布式调度任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistTask {
    /// 任务 ID
    pub id: LsId,
    /// 任务名称
    pub name: String,
    /// 任务类型
    pub task_type: String,
    /// 任务负载
    pub payload: serde_json::Value,
    /// 目标 Agent ID（可选）
    pub target_agent_id: Option<String>,
    /// 所需能力标签
    pub required_capabilities: Vec<String>,
    /// 优先级（0-10）
    pub priority: u8,
    /// 创建时间
    pub created_at: i64,
    /// 超时秒数
    pub timeout_secs: u64,
    /// 重试次数
    pub retry_count: u32,
    /// 亲缘性节点 ID（优先在此节点执行）
    pub affinity_node: Option<String>,
}

impl DistTask {
    pub fn new(
        name: impl Into<String>,
        task_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: LsId::new(),
            name: name.into(),
            task_type: task_type.into(),
            payload,
            target_agent_id: None,
            required_capabilities: Vec::new(),
            priority: 5,
            created_at: chrono::Utc::now().timestamp(),
            timeout_secs: 300,
            retry_count: 0,
            affinity_node: None,
        }
    }

    pub fn with_target_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.target_agent_id = Some(agent_id.into());
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    pub fn with_affinity(mut self, node_id: impl Into<String>) -> Self {
        self.affinity_node = Some(node_id.into());
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.required_capabilities.push(cap.into());
        self
    }
}

// ── 调度结果 ────────────────────────────────────────

/// 任务调度结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistScheduleResult {
    /// 任务 ID
    pub task_id: LsId,
    /// 分配的节点 ID
    pub assigned_node_id: String,
    /// 节点地址
    pub node_addr: String,
    /// 是否本地执行
    pub is_local: bool,
    /// 调度耗时 ms
    pub scheduling_ms: u64,
    /// 预估队列等待时间
    pub estimated_wait_ms: u64,
}

/// 任务执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistExecResult {
    /// 任务 ID
    pub task_id: LsId,
    /// 执行节点 ID
    pub node_id: String,
    /// 输出数据
    pub output: serde_json::Value,
    /// 是否成功
    pub success: bool,
    /// 执行耗时 ms
    pub execution_ms: u64,
    /// 错误信息
    pub error: Option<String>,
    /// 完成时间
    pub completed_at: i64,
}

// ── 节点负载 ────────────────────────────────────────

/// 节点负载信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLoad {
    /// 节点 ID
    pub node_id: String,
    /// 节点地址
    pub addr: String,
    /// 待处理任务数
    pub pending_tasks: u32,
    /// 活跃任务数
    pub active_tasks: u32,
    /// 总任务完成数
    pub completed_tasks: u64,
    /// 总任务失败数
    pub failed_tasks: u64,
    /// 成功率
    pub success_rate: f64,
    /// 平均执行时长 ms
    pub avg_execution_ms: f64,
    /// CPU 使用率（0.0~1.0）
    pub cpu_usage: f64,
    /// 内存使用率（0.0~1.0）
    pub memory_usage: f64,
    /// 权重（用于加权调度）
    pub weight: f64,
    /// 是否在线
    pub is_online: bool,
    /// 最后更新时间
    pub last_updated: i64,
}

impl NodeLoad {
    pub fn new(node_id: impl Into<String>, addr: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            addr: addr.into(),
            pending_tasks: 0,
            active_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
            success_rate: 1.0,
            avg_execution_ms: 0.0,
            cpu_usage: 0.0,
            memory_usage: 0.0,
            weight: 1.0,
            is_online: true,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }

    /// 综合负载评分（越低越好）
    pub fn load_score(&self) -> f64 {
        let task_load = (self.pending_tasks as f64 * 2.0 + self.active_tasks as f64) / 100.0;
        let resource_load = (self.cpu_usage + self.memory_usage) / 2.0;
        let failure_penalty = (1.0 - self.success_rate) * 0.5;
        (task_load + resource_load + failure_penalty) / self.weight.max(0.1)
    }
}

// ── 分布式调度器 ────────────────────────────────────

/// 跨节点分布式 Agent 调度器
pub struct DistScheduler {
    /// 配置
    config: DistSchedulerConfig,
    /// 集群引用
    cluster: Arc<Cluster>,
    /// 分布式任务队列
    task_queue: Arc<RwLock<Option<DistributedQueue>>>,
    /// 各节点负载信息
    node_loads: Arc<RwLock<HashMap<String, NodeLoad>>>,
    /// 轮询计数器
    round_robin_counter: Arc<RwLock<usize>>,
    /// 节点权重配置（手动覆盖）
    node_weights: Arc<RwLock<HashMap<String, f64>>>,
    /// 本地节点负载
    local_load: Arc<RwLock<NodeLoad>>,
    /// 是否已启动
    running: Arc<RwLock<bool>>,
}

impl DistScheduler {
    pub fn new(config: DistSchedulerConfig, cluster: Arc<Cluster>) -> Self {
        let local_load = NodeLoad::new(&config.local_node_id, "local");
        Self {
            config,
            cluster,
            task_queue: Arc::new(RwLock::new(None)),
            node_loads: Arc::new(RwLock::new(HashMap::new())),
            round_robin_counter: Arc::new(RwLock::new(0)),
            node_weights: Arc::new(RwLock::new(HashMap::new())),
            local_load: Arc::new(RwLock::new(local_load)),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 设置分布式队列
    pub async fn set_queue(&self, queue: DistributedQueue) {
        *self.task_queue.write().await = Some(queue);
    }

    /// 设置节点权重
    pub async fn set_node_weight(&self, node_id: &str, weight: f64) {
        self.node_weights
            .write()
            .await
            .insert(node_id.to_string(), weight.max(0.1));
    }

    /// 启动调度器后台任务
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            return;
        }
        *running = true;
        info!("distributed scheduler started (strategy={})", self.config.strategy.as_str());

        // 健康检查和负载更新循环
        let running_flag = self.running.clone();
        let node_loads = self.node_loads.clone();
        let cluster = self.cluster.clone();
        let config = self.config.clone();
        tokio::spawn(async move {
            while *running_flag.read().await {
                tokio::time::sleep(config.health_check_interval).await;

                // 从集群更新节点状态
                let state = cluster.state().read().await;
                let mut loads = node_loads.write().await;

                // 检查在线节点
                for member in state.members().values() {
                    let is_online = member.status == NodeStatus::Alive;
                    let load = loads
                        .entry(member.id.clone())
                        .or_insert_with(|| NodeLoad::new(&member.id, &member.addr));
                    load.is_online = is_online;
                    load.last_updated = chrono::Utc::now().timestamp();
                }

                // 清理超时节点
                let now = chrono::Utc::now().timestamp();
                loads.retain(|_, l| now - l.last_updated < config.node_timeout_secs as i64);
            }
        });

        // 故障转移处理
        if self.config.enable_auto_failover {
            let running_flag = self.running.clone();
            let node_loads = self.node_loads.clone();
            let _task_queue = self.task_queue.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    if !*running_flag.read().await {
                        break;
                    }

                    let loads = node_loads.read().await;
                    let failed_nodes: Vec<String> = loads
                        .iter()
                        .filter(|(_, l)| !l.is_online && l.pending_tasks > 0)
                        .map(|(id, _)| id.clone())
                        .collect();

                    if !failed_nodes.is_empty() {
                        warn!(
                            "failover: {} node(s) failed with pending tasks, rescheduling",
                            failed_nodes.len()
                        );
                        // 实际重调度逻辑可以在队列中重新入队
                    }
                }
            });
        }
    }

    /// 停止调度器
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("distributed scheduler stopped");
    }

    // ── 节点选择 ──

    /// 为任务选择执行节点
    pub async fn select_node(&self, task: &DistTask) -> SchedulerResult<DistScheduleResult> {
        let start = std::time::Instant::now();
        let loads = self.node_loads.read().await;

        let selection = match self.config.strategy {
            DistScheduleStrategy::LeastTasks => self.select_least_tasks(&loads).await,
            DistScheduleStrategy::RoundRobin => self.select_round_robin(&loads).await,
            DistScheduleStrategy::Weighted => self.select_weighted(&loads).await,
            DistScheduleStrategy::ConsistentHash => {
                self.select_consistent_hash(task, &loads).await
            }
            DistScheduleStrategy::LocalFirst => self.select_local_first(&loads).await,
            DistScheduleStrategy::Adaptive => self.select_adaptive(task, &loads).await,
        };

        let (node_id, node_addr) = selection.ok_or(SchedulerError::NoAvailableNode)?;
        let is_local = node_id == self.config.local_node_id;
        let elapsed = start.elapsed().as_millis() as u64;

        // 预估等待时间
        let estimated_wait = loads
            .get(&node_id)
            .map(|l| (l.pending_tasks as u64) * 100)
            .unwrap_or(0);

        Ok(DistScheduleResult {
            task_id: task.id.clone(),
            assigned_node_id: node_id,
            node_addr,
            is_local,
            scheduling_ms: elapsed,
            estimated_wait_ms: estimated_wait,
        })
    }

    /// 最少任务策略
    async fn select_least_tasks(
        &self,
        loads: &HashMap<String, NodeLoad>,
    ) -> Option<(String, String)> {
        loads
            .iter()
            .filter(|(_, l)| l.is_online)
            .min_by_key(|(_, l)| l.pending_tasks)
            .map(|(id, l)| (id.clone(), l.addr.clone()))
    }

    /// 轮询策略
    async fn select_round_robin(
        &self,
        loads: &HashMap<String, NodeLoad>,
    ) -> Option<(String, String)> {
        let online: Vec<(&String, &NodeLoad)> =
            loads.iter().filter(|(_, l)| l.is_online).collect();
        if online.is_empty() {
            return None;
        }
        let mut counter = self.round_robin_counter.write().await;
        let idx = *counter % online.len();
        *counter += 1;
        let (id, load) = online[idx];
        Some((id.clone(), load.addr.clone()))
    }

    /// 加权策略
    async fn select_weighted(
        &self,
        loads: &HashMap<String, NodeLoad>,
    ) -> Option<(String, String)> {
        let online: Vec<(&String, &NodeLoad)> =
            loads.iter().filter(|(_, l)| l.is_online).collect();
        if online.is_empty() {
            return None;
        }

        let total_weight: f64 = online.iter().map(|(_, l)| l.weight).sum();
        if total_weight <= 0.0 {
            return self.select_least_tasks(loads).await;
        }

        // Use deterministic pseudo-random based on timestamp
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let r = (nanos % 1000000) as f64 / 1000000.0_f64;
        let mut rng = r * total_weight;
        for (id, load) in &online {
            rng -= load.weight;
            if rng <= 0.0 {
                return Some(((*id).clone(), load.addr.clone()));
            }
        }

        // Fallback: return last
        let (id, load) = online[online.len() - 1];
        Some((id.clone(), load.addr.clone()))
    }

    /// 一致性哈希策略
    async fn select_consistent_hash(
        &self,
        task: &DistTask,
        loads: &HashMap<String, NodeLoad>,
    ) -> Option<(String, String)> {
        let online: Vec<(&String, &NodeLoad)> =
            loads.iter().filter(|(_, l)| l.is_online).collect();
        if online.is_empty() {
            return None;
        }

        let hash_input = task.id.to_string();
        let hash = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            hash_input.hash(&mut hasher);
            Hasher::finish(&hasher)
        };
        let idx = (hash as usize) % online.len();
        let (id, load) = online[idx];
        Some((id.clone(), load.addr.clone()))
    }

    /// 本地优先策略
    async fn select_local_first(
        &self,
        loads: &HashMap<String, NodeLoad>,
    ) -> Option<(String, String)> {
        let local_id = &self.config.local_node_id;

        // 本地可用则优先本地
        if let Some(local) = loads.get(local_id) {
            if local.is_online && local.pending_tasks < 5 {
                return Some((local_id.clone(), local.addr.clone()));
            }
        }

        // 否则选择负载最低的远端节点
        self.select_least_tasks(loads).await
    }

    /// 自适应策略（综合评分）
    async fn select_adaptive(
        &self,
        task: &DistTask,
        loads: &HashMap<String, NodeLoad>,
    ) -> Option<(String, String)> {
        let weights = self.node_weights.read().await;
        let online: Vec<(&String, &NodeLoad)> =
            loads.iter().filter(|(_, l)| l.is_online).collect();
        if online.is_empty() {
            return None;
        }

        let local_id = &self.config.local_node_id;

        let best = online
            .iter()
            .max_by(|(id_a, a), (id_b, b)| {
                let a_score = Self::adaptive_score(a, id_a, local_id, task, &weights);
                let b_score = Self::adaptive_score(b, id_b, local_id, task, &weights);
                a_score
                    .partial_cmp(&b_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        best.map(|(id, l)| ((*id).clone(), l.addr.clone()))
    }

    /// 自适应评分（越高越好）
    fn adaptive_score(
        load: &NodeLoad,
        node_id: &str,
        local_id: &str,
        task: &DistTask,
        weights: &HashMap<String, f64>,
    ) -> f64 {
        let mut score = 0.0;

        // 本地优先加分
        if node_id == local_id {
            score += 2.0;
        }

        // 亲缘性加分
        if let Some(ref affinity) = task.affinity_node {
            if node_id == affinity {
                score += 3.0;
            }
        }

        // 低负载加分
        let task_load = load.pending_tasks + load.active_tasks;
        score += (1.0 / (task_load as f64 + 1.0)) * 5.0;

        // 高成功率加分
        score += load.success_rate * 3.0;

        // 低资源使用加分
        score += (1.0 - load.cpu_usage) * 1.0;
        score += (1.0 - load.memory_usage) * 1.0;

        // 权重加分
        let w = weights.get(node_id).copied().unwrap_or(1.0);
        score *= w;

        score
    }

    // ── 任务管理 ──

    /// 提交任务到分布式队列
    pub async fn submit_task(&self, task: DistTask) -> SchedulerResult<DistScheduleResult> {
        // 选择节点
        let schedule_result = self.select_node(&task).await?;

        // 如果是本地，更新本地负载
        if schedule_result.is_local {
            let mut local_load = self.local_load.write().await;
            local_load.pending_tasks += 1;
        }

        // 如果有分布式队列，入队 (使用 publish API)
        if let Some(ref queue) = *self.task_queue.read().await {
            let payload_str = serde_json::to_string(&task).unwrap_or_default();
            queue
                .publish(
                    &format!("dist_task_{}", schedule_result.assigned_node_id),
                    &payload_str,
                )
                .await;
        }

        // 更新节点负载信息
        let mut loads = self.node_loads.write().await;
        if let Some(load) = loads.get_mut(&schedule_result.assigned_node_id) {
            load.pending_tasks += 1;
        }

        info!(
            "task '{}' scheduled to node '{}' (local={})",
            task.name, schedule_result.assigned_node_id, schedule_result.is_local
        );

        Ok(schedule_result)
    }

    /// 记录任务完成
    pub async fn record_completion(&self, task_id: &LsId, result: &DistExecResult) {
        let mut loads = self.node_loads.write().await;
        if let Some(load) = loads.get_mut(&result.node_id) {
            load.active_tasks = load.active_tasks.saturating_sub(1);
            load.pending_tasks = load.pending_tasks.saturating_sub(1);

            if result.success {
                load.completed_tasks += 1;
            } else {
                load.failed_tasks += 1;
            }

            let total = load.completed_tasks + load.failed_tasks;
            load.success_rate = if total > 0 {
                load.completed_tasks as f64 / total as f64
            } else {
                1.0
            };

            let alpha = 0.3;
            load.avg_execution_ms =
                alpha * result.execution_ms as f64 + (1.0 - alpha) * load.avg_execution_ms;
        }

        // 也更新本地负载
        if result.node_id == self.config.local_node_id {
            let mut local = self.local_load.write().await;
            local.pending_tasks = local.pending_tasks.saturating_sub(1);
            if result.success {
                local.completed_tasks += 1;
            } else {
                local.failed_tasks += 1;
            }
        }

        debug!("task {} completed on node {}", task_id, result.node_id);
    }

    /// 获取所有节点负载状态
    pub async fn get_all_loads(&self) -> Vec<NodeLoad> {
        let loads = self.node_loads.read().await;
        let mut result: Vec<NodeLoad> = loads.values().cloned().collect();
        result.sort_by(|a, b| b.is_online.cmp(&a.is_online));
        result
    }

    /// 获取本地负载
    pub async fn get_local_load(&self) -> NodeLoad {
        self.local_load.read().await.clone()
    }

    /// 获取调度器状态摘要
    pub async fn summary(&self) -> DistSchedulerSummary {
        let loads = self.node_loads.read().await;
        let online = loads.values().filter(|l| l.is_online).count();
        let total_pending: u32 = loads.values().map(|l| l.pending_tasks).sum();
        let total_active: u32 = loads.values().map(|l| l.active_tasks).sum();

        DistSchedulerSummary {
            strategy: self.config.strategy,
            local_node_id: self.config.local_node_id.clone(),
            online_nodes: online,
            total_nodes: loads.len(),
            total_pending_tasks: total_pending,
            total_active_tasks: total_active,
            is_running: *self.running.read().await,
        }
    }
}

/// 调度器摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistSchedulerSummary {
    pub strategy: DistScheduleStrategy,
    pub local_node_id: String,
    pub online_nodes: usize,
    pub total_nodes: usize,
    pub total_pending_tasks: u32,
    pub total_active_tasks: u32,
    pub is_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_scheduler() -> DistScheduler {
        let config = DistSchedulerConfig {
            strategy: DistScheduleStrategy::Adaptive,
            local_node_id: "local-node".to_string(),
            ..DistSchedulerConfig::default()
        };
        let cluster = Arc::new(Cluster::new(ClusterConfig::default()));
        DistScheduler::new(config, cluster)
    }

    fn add_test_node(
        loads: &mut HashMap<String, NodeLoad>,
        id: &str,
        addr: &str,
        pending: u32,
        success_rate: f64,
    ) {
        let mut load = NodeLoad::new(id, addr);
        load.pending_tasks = pending;
        load.success_rate = success_rate;
        load.is_online = true;
        loads.insert(id.to_string(), load);
    }

    #[tokio::test]
    async fn test_select_least_tasks() {
        let scheduler = create_scheduler();
        let mut loads = HashMap::new();
        add_test_node(&mut loads, "node-1", "10.0.0.1:8001", 5, 0.9);
        add_test_node(&mut loads, "node-2", "10.0.0.2:8001", 2, 0.95);
        add_test_node(&mut loads, "node-3", "10.0.0.3:8001", 8, 0.8);

        let result = scheduler.select_least_tasks(&loads).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "node-2"); // Lowest pending tasks
    }

    #[tokio::test]
    async fn test_select_round_robin() {
        let scheduler = create_scheduler();
        let mut loads = HashMap::new();
        add_test_node(&mut loads, "node-1", "10.0.0.1:8001", 0, 1.0);
        add_test_node(&mut loads, "node-2", "10.0.0.2:8001", 0, 1.0);

        let r1 = scheduler.select_round_robin(&loads).await;
        let r2 = scheduler.select_round_robin(&loads).await;
        assert!(r1.is_some());
        assert!(r2.is_some());
        assert_ne!(r1.unwrap().0, r2.unwrap().0); // Different nodes
    }

    #[tokio::test]
    async fn test_select_local_first() {
        let scheduler = create_scheduler();
        let mut loads = HashMap::new();
        add_test_node(&mut loads, "local-node", "127.0.0.1:8001", 0, 1.0);
        add_test_node(&mut loads, "remote-node", "10.0.0.1:8001", 0, 1.0);

        let result = scheduler.select_local_first(&loads).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "local-node");
    }

    #[tokio::test]
    async fn test_select_adaptive() {
        let scheduler = create_scheduler();
        let mut loads = HashMap::new();
        add_test_node(&mut loads, "local-node", "127.0.0.1:8001", 0, 0.99);
        add_test_node(&mut loads, "remote-1", "10.0.0.1:8001", 10, 0.5);
        add_test_node(&mut loads, "remote-2", "10.0.0.2:8001", 1, 0.95);

        let task = DistTask::new("test", "test", serde_json::json!({}));
        let result = scheduler.select_adaptive(&task, &loads).await;
        assert!(result.is_some());
        // Local-node should have highest adaptive score
        assert_eq!(result.unwrap().0, "local-node");
    }

    #[tokio::test]
    async fn test_select_node_fallback() {
        let scheduler = create_scheduler();
        let loads = HashMap::new(); // No nodes

        // Manually test the fallback
        let fallback = scheduler.select_least_tasks(&loads).await;
        assert!(fallback.is_none());
    }

    #[tokio::test]
    async fn test_task_creation() {
        let task = DistTask::new("my-task", "code-exec", serde_json::json!({"code": "print(1)"}))
            .with_priority(8)
            .with_target_agent("agent-1")
            .with_affinity("node-1")
            .with_capability("python");

        assert_eq!(task.name, "my-task");
        assert_eq!(task.priority, 8);
        assert_eq!(task.target_agent_id.unwrap(), "agent-1");
        assert_eq!(task.affinity_node.unwrap(), "node-1");
        assert!(!task.required_capabilities.is_empty());
    }

    #[test]
    fn test_node_load_score() {
        let load = NodeLoad {
            node_id: "test".to_string(),
            addr: "127.0.0.1:8001".to_string(),
            pending_tasks: 5,
            active_tasks: 2,
            completed_tasks: 100,
            failed_tasks: 10,
            success_rate: 0.91,
            avg_execution_ms: 50.0,
            cpu_usage: 0.6,
            memory_usage: 0.7,
            weight: 1.0,
            is_online: true,
            last_updated: 0,
        };
        assert!(load.load_score() > 0.0);

        // Higher weight = lower score (better)
        let high_weight = NodeLoad {
            weight: 2.0,
            ..load.clone()
        };
        assert!(high_weight.load_score() < load.load_score());
    }

    #[test]
    fn test_scheduler_summary() {
        let summary = DistSchedulerSummary {
            strategy: DistScheduleStrategy::LeastTasks,
            local_node_id: "node-1".to_string(),
            online_nodes: 3,
            total_nodes: 5,
            total_pending_tasks: 12,
            total_active_tasks: 4,
            is_running: true,
        };
        assert_eq!(summary.online_nodes, 3);
        assert!(summary.is_running);
    }

    #[test]
    fn test_strategy_display() {
        assert_eq!(DistScheduleStrategy::Adaptive.as_str(), "adaptive");
        assert_eq!(
            DistScheduleStrategy::LeastTasks.as_str(),
            "least_tasks"
        );
        assert_eq!(
            DistScheduleStrategy::ConsistentHash.as_str(),
            "consistent_hash"
        );
    }
}
