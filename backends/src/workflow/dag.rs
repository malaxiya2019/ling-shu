//! WorkflowDag — DAG 工作流定义与执行引擎.
//!
//! 拓扑排序、环检测、并行执行、上下文传递。
//! 支持超时控制、指数退避重试、条件跳过、事件发射。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// 节点输出数据.
pub type NodeOutput = Value;

/// 节点处理函数.
pub type NodeHandler = Arc<
    dyn Fn(
            LsContext,
            Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = LsResult<NodeOutput>> + Send>>
        + Send
        + Sync,
>;

/// 工作流事件.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkflowEvent {
    /// 工作流开始.
    WorkflowStarted {
        workflow_id: LsId,
        workflow_name: String,
    },
    /// 节点开始执行.
    NodeStarted {
        node_id: LsId,
        node_name: String,
    },
    /// 节点执行完成.
    NodeCompleted {
        node_id: LsId,
        node_name: String,
        duration_ms: u64,
        attempts: u32,
    },
    /// 节点执行失败.
    NodeFailed {
        node_id: LsId,
        node_name: String,
        error: String,
        duration_ms: u64,
        attempts: u32,
    },
    /// 节点被跳过.
    NodeSkipped {
        node_id: LsId,
        node_name: String,
        reason: String,
    },
    /// 节点超时.
    NodeTimedOut {
        node_id: LsId,
        node_name: String,
        timeout_secs: u64,
    },
    /// 工作流结束.
    WorkflowCompleted {
        workflow_id: LsId,
        workflow_name: String,
        success: bool,
        total_duration_ms: u64,
    },
}

/// 工作流事件处理函数.
pub type WorkflowEventHandler = Arc<dyn Fn(WorkflowEvent) + Send + Sync>;

/// 工作流节点.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    /// 节点唯一 ID.
    pub id: LsId,
    /// 节点名称.
    pub name: String,
    /// 节点描述.
    pub description: String,
    /// 依赖的节点 ID 列表.
    pub dependencies: Vec<LsId>,
    /// 节点配置参数 (JSON).
    pub config: Value,
    /// 超时秒数 (0 表示不限制).
    pub timeout_secs: u64,
    /// 重试次数 (0 表示不重试).
    pub retry_count: u32,
    /// 重试基础延迟毫秒数 (指数退避: delay = retry_delay_ms * 2^attempt).
    pub retry_delay_ms: u64,
    /// 条件跳过: 如果指定节点的输出为 `true`, 则跳过当前节点.
    /// 例如 `"step_a"` — 如果 step_a 的输出为 `true`, 跳过此节点.
    pub skip_if: Option<String>,
}

impl WorkflowNode {
    /// 创建新的工作流节点.
    pub fn new(
        id: LsId,
        name: impl Into<String>,
        description: impl Into<String>,
        config: Value,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            config,
            timeout_secs: 0,
            retry_count: 0,
            retry_delay_ms: 1000,
            skip_if: None,
        }
    }
}

/// 节点执行结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: LsId,
    pub node_name: String,
    pub status: NodeStatus,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub attempts: u32,
}

/// 节点执行状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    TimedOut,
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeStatus::Pending => write!(f, "pending"),
            NodeStatus::Running => write!(f, "running"),
            NodeStatus::Completed => write!(f, "completed"),
            NodeStatus::Failed => write!(f, "failed"),
            NodeStatus::Skipped => write!(f, "skipped"),
            NodeStatus::TimedOut => write!(f, "timed_out"),
        }
    }
}

/// 工作流执行结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub workflow_id: LsId,
    pub workflow_name: String,
    pub success: bool,
    pub node_results: Vec<NodeResult>,
    pub total_duration_ms: u64,
    pub started_at: String,
    pub completed_at: String,
}

/// 工作流错误.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowError {
    pub node_name: String,
    pub message: String,
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.node_name, self.message)
    }
}

/// 执行快照 — 用于 Checkpoint/Resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSnapshot {
    /// 已完成节点集合.
    pub completed: HashSet<LsId>,
    /// 节点结果.
    pub results: HashMap<LsId, NodeResult>,
    /// 节点输出值.
    pub node_outputs: HashMap<LsId, Value>,
    /// 是否已失败.
    pub failed: bool,
    /// 快照时间戳.
    pub timestamp: String,
}

/// DAG 工作流引擎.
#[derive(Serialize, Deserialize)]
pub struct WorkflowDag {
    id: LsId,
    name: String,
    nodes: HashMap<LsId, WorkflowNode>,
    #[serde(skip)]
    handlers: HashMap<LsId, NodeHandler>,
    adj_list: HashMap<LsId, Vec<LsId>>, // node -> dependents
    #[serde(skip)]
    #[serde(default)]
    event_handler: Option<WorkflowEventHandler>,
}

impl WorkflowDag {
    /// 创建新的工作流.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: LsId::new(),
            name: name.into(),
            nodes: HashMap::new(),
            handlers: HashMap::new(),
            adj_list: HashMap::new(),
            event_handler: None,
        }
    }

    /// 设置工作流 ID (用于恢复).
    pub fn with_id(mut self, id: LsId) -> Self {
        self.id = id;
        self
    }

    /// 设置事件处理器.
    pub fn set_event_handler(&mut self, handler: WorkflowEventHandler) {
        self.event_handler = Some(handler);
    }

    /// 获取事件处理器引用.
    pub fn event_handler(&self) -> Option<&WorkflowEventHandler> {
        self.event_handler.as_ref()
    }

    /// 获取工作流 ID.
    pub fn id(&self) -> LsId {
        self.id
    }

    /// 获取工作流名称.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 获取节点.
    pub fn get_node(&self, id: &LsId) -> Option<&WorkflowNode> {
        self.nodes.get(id)
    }

    /// 获取所有节点.
    pub fn nodes(&self) -> &HashMap<LsId, WorkflowNode> {
        &self.nodes
    }

    /// 获取邻接表.
    pub fn adj_list(&self) -> &HashMap<LsId, Vec<LsId>> {
        &self.adj_list
    }

    /// 获取处理器.
    pub fn get_handler(&self, id: &LsId) -> Option<&NodeHandler> {
        self.handlers.get(id)
    }

    /// 设置节点超时.
    pub fn set_node_timeout(&mut self, node_id: LsId, timeout_secs: u64) -> LsResult<()> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| LsError::NotFound(format!("node {node_id} not found")))?;
        node.timeout_secs = timeout_secs;
        Ok(())
    }

    /// 设置节点重试.
    pub fn set_node_retry(&mut self, node_id: LsId, retry_count: u32, retry_delay_ms: u64) -> LsResult<()> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| LsError::NotFound(format!("node {node_id} not found")))?;
        node.retry_count = retry_count;
        node.retry_delay_ms = retry_delay_ms;
        Ok(())
    }

    /// 设置节点跳过条件.
    pub fn set_node_skip_if(&mut self, node_id: LsId, skip_if: Option<String>) -> LsResult<()> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or_else(|| LsError::NotFound(format!("node {node_id} not found")))?;
        node.skip_if = skip_if;
        Ok(())
    }

    /// 添加节点 (完整配置).
    #[allow(clippy::too_many_arguments)]
    pub fn add_node_full(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        config: Value,
        handler: NodeHandler,
        timeout_secs: u64,
        retry_count: u32,
        retry_delay_ms: u64,
        skip_if: Option<String>,
    ) -> LsId {
        let id = LsId::new();
        let node = WorkflowNode {
            id,
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            config,
            timeout_secs,
            retry_count,
            retry_delay_ms,
            skip_if,
        };
        self.nodes.insert(id, node);
        self.handlers.insert(id, handler);
        self.adj_list.entry(id).or_default();
        id
    }

    /// 添加节点 (简化版本，保持向后兼容).
    pub fn add_node(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        config: Value,
        handler: NodeHandler,
    ) -> LsId {
        self.add_node_full(name, description, config, handler, 0, 0, 1000, None)
    }

    /// 添加依赖边: `from` -> `to` (to 依赖 from).
    pub fn add_edge(&mut self, from: LsId, to: LsId) -> LsResult<()> {
        if !self.nodes.contains_key(&from) {
            return Err(LsError::NotFound(format!("node {from} not found")));
        }
        if !self.nodes.contains_key(&to) {
            return Err(LsError::NotFound(format!("node {to} not found")));
        }
        if from == to {
            return Err(LsError::Validation("self-dependency is not allowed".into()));
        }

        // Check for cycles before adding
        let mut test_adj = self.adj_list.clone();
        test_adj.entry(from).or_default().push(to);
        if has_cycle(&test_adj) {
            return Err(LsError::Validation(format!(
                "adding edge {from} -> {to} would create a cycle"
            )));
        }

        self.adj_list.entry(from).or_default().push(to);
        if let Some(node) = self.nodes.get_mut(&to) {
            node.dependencies.push(from);
        }

        Ok(())
    }

    /// 批量添加边.
    pub fn add_edges(&mut self, edges: &[(LsId, LsId)]) -> LsResult<()> {
        for &(from, to) in edges {
            self.add_edge(from, to)?;
        }
        Ok(())
    }

    /// 移除节点.
    pub fn remove_node(&mut self, node_id: LsId) -> LsResult<()> {
        if !self.nodes.contains_key(&node_id) {
            return Err(LsError::NotFound(format!("node {node_id} not found")));
        }
        self.nodes.remove(&node_id);
        self.handlers.remove(&node_id);
        self.adj_list.remove(&node_id);
        // Remove edges from other nodes
        for edges in self.adj_list.values_mut() {
            edges.retain(|&id| id != node_id);
        }
        for node in self.nodes.values_mut() {
            node.dependencies.retain(|&id| id != node_id);
        }
        Ok(())
    }

    /// 拓扑排序 (Kahn's algorithm).
    pub fn topological_sort(&self) -> LsResult<Vec<LsId>> {
        let mut in_degree: HashMap<LsId, usize> = HashMap::new();
        for &id in self.nodes.keys() {
            in_degree.entry(id).or_insert(0);
        }
        // Actually compute in-degree from reverse edges
        for to_list in self.adj_list.values() {
            for &to in to_list {
                *in_degree.entry(to).or_default() += 1;
            }
        }

        let mut queue: VecDeque<LsId> = VecDeque::new();
        for (&id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(id);
            }
        }

        let mut result = Vec::new();
        while let Some(id) = queue.pop_front() {
            result.push(id);
            if let Some(dependents) = self.adj_list.get(&id) {
                for &dep_id in dependents {
                    if let Some(deg) = in_degree.get_mut(&dep_id) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep_id);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(LsError::Internal("graph contains a cycle".into()));
        }

        Ok(result)
    }

    /// 获取执行快照 (用于 Checkpoint).
    pub fn snapshot(
        completed: &HashSet<LsId>,
        results: &HashMap<LsId, NodeResult>,
        node_outputs: &HashMap<LsId, Value>,
        failed: bool,
    ) -> ExecutionSnapshot {
        ExecutionSnapshot {
            completed: completed.clone(),
            results: results.clone(),
            node_outputs: node_outputs.clone(),
            failed,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// 核心执行逻辑 (内部方法，接受快照用于恢复).
    async fn execute_inner(
        &self,
        ctx: LsContext,
        input: Value,
        snapshot: Option<ExecutionSnapshot>,
    ) -> LsResult<WorkflowResult> {
        let start = std::time::Instant::now();
        let started_at = chrono::Utc::now().to_rfc3339();
        let order = self.topological_sort()?;

        info!(
            workflow = %self.name,
            nodes = order.len(),
            "workflow execution started"
        );

        // Emit workflow started event
        if let Some(ref handler) = self.event_handler {
            handler(WorkflowEvent::WorkflowStarted {
                workflow_id: self.id,
                workflow_name: self.name.clone(),
            });
        }

        // Restore from snapshot if provided
        let mut results: HashMap<LsId, NodeResult>;
        let mut node_outputs: HashMap<LsId, Value>;
        let mut completed: HashSet<LsId>;
        let mut failed: bool;

        if let Some(snap) = snapshot {
            results = snap.results;
            node_outputs = snap.node_outputs;
            completed = snap.completed;
            failed = snap.failed;
            info!(
                workflow = %self.name,
                completed_nodes = completed.len(),
                "resumed from checkpoint"
            );
        } else {
            results = HashMap::new();
            node_outputs = HashMap::new();
            completed = HashSet::new();
            failed = false;
        }

        // Process nodes in topological order, executing independent nodes in parallel
        while !failed {
            // Scan ALL nodes in topological order for ones whose dependencies are satisfied
            let mut ready = Vec::new();
            for &node_id in &order {
                if completed.contains(&node_id) {
                    continue;
                }
                if let Some(node) = self.nodes.get(&node_id) {
                    let deps_satisfied = node.dependencies.iter().all(|d| completed.contains(d));
                    if deps_satisfied {
                        ready.push(node_id);
                    }
                }
            }

            if ready.is_empty() {
                break;
            }

            // Execute ready nodes in parallel
            let mut handles = Vec::new();
            for &node_id in &ready {
                if let Some(node) = self.nodes.get(&node_id) {
                    // ── Check skip condition ──
                    if let Some(ref skip_node_name) = node.skip_if {
                        let should_skip = node_outputs.iter().any(|(nid, output)| {
                            if let Some(n) = self.nodes.get(nid) {
                                n.name == *skip_node_name
                                    && matches!(output, Value::Bool(true))
                            } else {
                                false
                            }
                        });

                        if should_skip {
                            completed.insert(node_id);
                            let result = NodeResult {
                                node_id,
                                node_name: node.name.clone(),
                                status: NodeStatus::Skipped,
                                output: None,
                                error: None,
                                duration_ms: 0,
                                attempts: 0,
                            };
                            results.insert(node_id, result.clone());

                            if let Some(ref handler) = self.event_handler {
                                handler(WorkflowEvent::NodeSkipped {
                                    node_id,
                                    node_name: node.name.clone(),
                                    reason: format!("skipped by condition from node '{}'", skip_node_name),
                                });
                            }

                            debug!(
                                node = %node.name,
                                reason = "skip_if condition met",
                                "node skipped"
                            );
                            continue;
                        }
                    }

                    let handler = self.handlers.get(&node_id).cloned();
                    let node_ctx = ctx.child();
                    let node_input = input.clone();
                    let timeout_secs = node.timeout_secs;
                    let retry = node.retry_count;
                    let retry_delay = Duration::from_millis(node.retry_delay_ms);
                    let node_name = node.name.clone();
                    let node_id_for_event = node_id;
                    let event_handler = self.event_handler.clone();

                    // Emit NodeStarted
                    if let Some(ref handler) = event_handler {
                        handler(WorkflowEvent::NodeStarted {
                            node_id: node_id_for_event,
                            node_name: node_name.clone(),
                        });
                    }

                    let handle = tokio::spawn(async move {
                        let node_start = std::time::Instant::now();
                        let mut attempts = 0u32;
                        let max_attempts = if retry > 0 { retry + 1 } else { 1 };

                        let result = loop {
                            attempts += 1;
                            let handler_result = match handler {
                                Some(ref h) => {
                                    let fut = h(node_ctx.clone(), node_input.clone());
                                    if timeout_secs > 0 {
                                        match tokio::time::timeout(
                                            Duration::from_secs(timeout_secs),
                                            fut,
                                        )
                                        .await
                                        {
                                            Ok(Ok(output)) => Ok(output),
                                            Ok(Err(e)) => Err(e),
                                            Err(_) => Err(LsError::Timeout(format!(
                                                "node '{node_name}' timed out after {timeout_secs}s"
                                            ))),
                                        }
                                    } else {
                                        fut.await
                                    }
                                }
                                None => Ok(Value::Null),
                            };

                            match handler_result {
                                Ok(output) => break Ok(output),
                                Err(e) => {
                                    if attempts >= max_attempts {
                                        break Err(e);
                                    }
                                    let delay = retry_delay * 2u32.pow(attempts - 1);
                                    warn!(
                                        node = %node_name,
                                        attempt = attempts,
                                        error = %e,
                                        retry_delay_ms = delay.as_millis(),
                                        "retrying node with exponential backoff"
                                    );
                                    tokio::time::sleep(delay).await;
                                }
                            }
                        };

                        let duration = node_start.elapsed().as_millis() as u64;
                        (node_id_for_event, result, duration, attempts)
                    });
                    handles.push(handle);
                }
            }

            // Collect results
            for handle in handles {
                match handle.await {
                    Ok((node_id, Ok(output), duration, attempts)) => {
                        completed.insert(node_id);
                        node_outputs.insert(node_id, output.clone());
                        if let Some(node) = self.nodes.get(&node_id) {
                            results.insert(
                                node_id,
                                NodeResult {
                                    node_id,
                                    node_name: node.name.clone(),
                                    status: NodeStatus::Completed,
                                    output: Some(output.clone()),
                                    error: None,
                                    duration_ms: duration,
                                    attempts,
                                },
                            );

                            if let Some(ref handler) = self.event_handler {
                                handler(WorkflowEvent::NodeCompleted {
                                    node_id,
                                    node_name: node.name.clone(),
                                    duration_ms: duration,
                                    attempts,
                                });
                            }

                            debug!(
                                node = %node.name,
                                duration_ms = duration,
                                "node completed"
                            );
                        }
                    }
                    Ok((node_id, Err(e), duration, attempts)) => {
                        let is_timeout = matches!(&e, LsError::Timeout(_));
                        failed = true;
                        if let Some(node) = self.nodes.get(&node_id) {
                            let status = if is_timeout {
                                NodeStatus::TimedOut
                            } else {
                                NodeStatus::Failed
                            };
                            results.insert(
                                node_id,
                                NodeResult {
                                    node_id,
                                    node_name: node.name.clone(),
                                    status,
                                    output: None,
                                    error: Some(e.to_string()),
                                    duration_ms: duration,
                                    attempts,
                                },
                            );

                            if is_timeout {
                                if let Some(ref handler) = self.event_handler {
                                    handler(WorkflowEvent::NodeTimedOut {
                                        node_id,
                                        node_name: node.name.clone(),
                                        timeout_secs: node.timeout_secs,
                                    });
                                }
                            } else if let Some(ref handler) = self.event_handler {
                                handler(WorkflowEvent::NodeFailed {
                                    node_id,
                                    node_name: node.name.clone(),
                                    error: e.to_string(),
                                    duration_ms: duration,
                                    attempts,
                                });
                            }

                            warn!(
                                node = %node.name,
                                error = %e,
                                "node failed"
                            );
                        }
                    }
                    Err(e) => {
                        failed = true;
                        warn!(error = %e, "task join error");
                    }
                }
            }
        }

        let total_duration = start.elapsed().as_millis() as u64;
        let completed_at = chrono::Utc::now().to_rfc3339();

        // Collect results in topological order
        let mut node_results: Vec<NodeResult> = Vec::new();
        for node_id in &order {
            if let Some(result) = results.remove(node_id) {
                node_results.push(result);
            }
        }

        let success = !failed;

        // Emit workflow completed event
        if let Some(ref handler) = self.event_handler {
            handler(WorkflowEvent::WorkflowCompleted {
                workflow_id: self.id,
                workflow_name: self.name.clone(),
                success,
                total_duration_ms: total_duration,
            });
        }

        info!(
            workflow = %self.name,
            success = success,
            duration_ms = total_duration,
            "workflow execution completed"
        );

        Ok(WorkflowResult {
            workflow_id: self.id,
            workflow_name: self.name.clone(),
            success,
            node_results,
            total_duration_ms: total_duration,
            started_at,
            completed_at,
        })
    }

    /// 执行工作流.
    pub async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<WorkflowResult> {
        self.execute_inner(ctx, input, None).await
    }

    /// 从快照恢复执行.
    pub async fn resume(
        &self,
        ctx: LsContext,
        input: Value,
        snapshot: ExecutionSnapshot,
    ) -> LsResult<WorkflowResult> {
        self.execute_inner(ctx, input, Some(snapshot)).await
    }

    /// 获取工作流信息.
    pub fn info(&self) -> WorkflowInfo {
        WorkflowInfo {
            id: self.id,
            name: self.name.clone(),
            node_count: self.nodes.len(),
            edge_count: self.adj_list.values().map(|v| v.len()).sum(),
        }
    }

    /// 获取工作流节点数.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 获取工作流边数.
    pub fn edge_count(&self) -> usize {
        self.adj_list.values().map(|v| v.len()).sum()
    }

    /// 获取没有依赖的根节点.
    pub fn root_nodes(&self) -> Vec<LsId> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.dependencies.is_empty())
            .map(|(id, _)| *id)
            .collect()
    }

    /// 获取叶子节点 (没有出边).
    pub fn leaf_nodes(&self) -> Vec<LsId> {
        self.nodes
            .keys()
            .filter(|id| {
                self.adj_list
                    .get(id)
                    .is_none_or(|edges| edges.is_empty())
            })
            .copied()
            .collect()
    }
}

impl Clone for WorkflowDag {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            nodes: self.nodes.clone(),
            handlers: HashMap::new(), // handlers are not cloneable — reset on clone
            adj_list: self.adj_list.clone(),
            event_handler: None, // event handler resets on clone
        }
    }
}

/// 工作流摘要信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInfo {
    pub id: LsId,
    pub name: String,
    pub node_count: usize,
    pub edge_count: usize,
}

/// 检测有向图中是否存在环 (DFS).
fn has_cycle(adj: &HashMap<LsId, Vec<LsId>>) -> bool {
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    fn dfs(
        node: LsId,
        adj: &HashMap<LsId, Vec<LsId>>,
        visited: &mut HashSet<LsId>,
        in_stack: &mut HashSet<LsId>,
    ) -> bool {
        visited.insert(node);
        in_stack.insert(node);

        if let Some(neighbors) = adj.get(&node) {
            for &next in neighbors {
                if !visited.contains(&next) {
                    if dfs(next, adj, visited, in_stack) {
                        return true;
                    }
                } else if in_stack.contains(&next) {
                    return true;
                }
            }
        }

        in_stack.remove(&node);
        false
    }

    for &node in adj.keys() {
        if !visited.contains(&node) && dfs(node, adj, &mut visited, &mut in_stack) {
            return true;
        }
    }
    false
}

/// 简化工作流类型 (使用闭包/函数).
pub struct Workflow {
    inner: WorkflowDag,
}

/// 工作流节点配置构造器.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub description: String,
    pub config: Value,
    pub timeout_secs: u64,
    pub retry_count: u32,
    pub retry_delay_ms: u64,
    pub skip_if: Option<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            description: String::new(),
            config: Value::Null,
            timeout_secs: 0,
            retry_count: 0,
            retry_delay_ms: 1000,
            skip_if: None,
        }
    }
}

impl Workflow {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            inner: WorkflowDag::new(name),
        }
    }

    /// 获取内部 DAG 引用.
    pub fn dag(&self) -> &WorkflowDag {
        &self.inner
    }

    /// 获取内部 DAG 可变引用.
    pub fn dag_mut(&mut self) -> &mut WorkflowDag {
        &mut self.inner
    }

    /// 设置事件处理器.
    pub fn on_event(&mut self, handler: WorkflowEventHandler) {
        self.inner.set_event_handler(handler);
    }

    /// 添加一个异步节点处理函数 (基础版).
    pub fn add_node<F, Fut>(&mut self, name: &str, handler: F) -> LsId
    where
        F: Fn(LsContext, Value) -> Fut + Send + Sync + 'static + Clone,
        Fut: std::future::Future<Output = LsResult<Value>> + Send + 'static,
    {
        self.inner.add_node(
            name,
            "",
            Value::Null,
            Arc::new(move |ctx, input| {
                let h = handler.clone();
                Box::pin(async move { h(ctx, input).await })
            }),
        )
    }

    /// 添加一个异步节点处理函数 (完整配置版).
    pub fn add_node_with<F, Fut>(
        &mut self,
        name: &str,
        config: NodeConfig,
        handler: F,
    ) -> LsId
    where
        F: Fn(LsContext, Value) -> Fut + Send + Sync + 'static + Clone,
        Fut: std::future::Future<Output = LsResult<Value>> + Send + 'static,
    {
        self.inner.add_node_full(
            name,
            config.description,
            config.config,
            Arc::new(move |ctx, input| {
                let h = handler.clone();
                Box::pin(async move { h(ctx, input).await })
            }),
            config.timeout_secs,
            config.retry_count,
            config.retry_delay_ms,
            config.skip_if,
        )
    }

    /// 添加依赖边.
    pub fn add_edge(&mut self, from: LsId, to: LsId) -> LsResult<()> {
        self.inner.add_edge(from, to)
    }

    /// 执行工作流.
    pub async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<WorkflowResult> {
        self.inner.execute(ctx, input).await
    }

    /// 从快照恢复执行.
    pub async fn resume(
        &self,
        ctx: LsContext,
        input: Value,
        snapshot: ExecutionSnapshot,
    ) -> LsResult<WorkflowResult> {
        self.inner.resume(ctx, input, snapshot).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_empty_workflow() {
        let wf = WorkflowDag::new("empty");
        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 0);
    }

    #[tokio::test]
    async fn test_single_node() {
        let mut wf = WorkflowDag::new("single");
        wf.add_node(
            "hello",
            "Says hello",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"message": "hello"})) })),
        );
        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 1);
        assert_eq!(result.node_results[0].node_name, "hello");
        assert_eq!(
            result.node_results[0].output.as_ref().unwrap()["message"],
            "hello"
        );
    }

    #[tokio::test]
    async fn test_linear_dag() {
        let mut wf = WorkflowDag::new("linear");
        let a = wf.add_node(
            "step_a",
            "Step A",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"step": "A"})) })),
        );
        let b = wf.add_node(
            "step_b",
            "Step B",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"step": "B"})) })),
        );
        let c = wf.add_node(
            "step_c",
            "Step C",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"step": "C"})) })),
        );

        wf.add_edge(a, b).unwrap();
        wf.add_edge(b, c).unwrap();

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 3);

        // Verify order: A, B, C
        assert_eq!(result.node_results[0].node_name, "step_a");
        assert_eq!(result.node_results[1].node_name, "step_b");
        assert_eq!(result.node_results[2].node_name, "step_c");
    }

    #[tokio::test]
    async fn test_fork_join_dag() {
        let mut wf = WorkflowDag::new("fork-join");
        let start = wf.add_node(
            "start",
            "Start",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"phase": "start"})) })),
        );
        let fork_a = wf.add_node(
            "fork_a",
            "Fork A",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"branch": "A"})) })),
        );
        let fork_b = wf.add_node(
            "fork_b",
            "Fork B",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"branch": "B"})) })),
        );
        let join = wf.add_node(
            "join",
            "Join",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"phase": "join"})) })),
        );

        wf.add_edge(start, fork_a).unwrap();
        wf.add_edge(start, fork_b).unwrap();
        wf.add_edge(fork_a, join).unwrap();
        wf.add_edge(fork_b, join).unwrap();

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 4);
    }

    #[tokio::test]
    async fn test_cycle_detection() {
        let mut wf = WorkflowDag::new("cycle-test");
        let a = wf.add_node(
            "a",
            "A",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let b = wf.add_node(
            "b",
            "B",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );

        wf.add_edge(a, b).unwrap();
        // Creating a cycle: b -> a
        let result = wf.add_edge(b, a);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_node_failure() {
        let mut wf = WorkflowDag::new("failure-test");
        wf.add_node(
            "fail",
            "Fail node",
            Value::Null,
            Arc::new(|_ctx, _input| {
                Box::pin(async { Err(LsError::Internal("intentional failure".into())) })
            }),
        );

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.node_results[0].status, NodeStatus::Failed);
    }

    #[tokio::test]
    async fn test_workflow_info() {
        let mut wf = WorkflowDag::new("info-test");
        let a = wf.add_node(
            "a",
            "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let b = wf.add_node(
            "b",
            "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        wf.add_edge(a, b).unwrap();

        let info = wf.info();
        assert_eq!(info.name, "info-test");
        assert_eq!(info.node_count, 2);
        assert_eq!(info.edge_count, 1);
    }

    // ── 新增测试 ──

    #[tokio::test]
    async fn test_node_timeout() {
        let mut wf = WorkflowDag::new("timeout-test");
        let id = wf.add_node_full(
            "slow",
            "Slow node",
            Value::Null,
            Arc::new(|_ctx, _input| {
                Box::pin(async {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Ok(json!({"done": true}))
                })
            }),
            1,   // timeout_secs
            0,   // retry_count
            1000, // retry_delay_ms
            None, // skip_if
        );
        let _ = id;
        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.node_results[0].status, NodeStatus::TimedOut);
    }

    #[tokio::test]
    async fn test_exponential_backoff_retry() {
        let attempt_count = Arc::new(AtomicUsize::new(0));
        let attempt_count_clone = attempt_count.clone();
        let mut wf = WorkflowDag::new("retry-test");
        wf.add_node_full(
            "flaky",
            "Flaky node",
            Value::Null,
            Arc::new(move |_ctx, _input| {
                let count = attempt_count_clone.clone();
                Box::pin(async move {
                    let attempt = count.fetch_add(1, Ordering::SeqCst);
                    if attempt < 2 {
                        Err(LsError::Internal("transient error".into()))
                    } else {
                        Ok(json!({"success": true}))
                    }
                })
            }),
            0,     // timeout_secs
            3,     // retry_count
            50,    // retry_delay_ms (fast for test)
            None,  // skip_if
        );

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3); // 2 failures + 1 success
        assert_eq!(result.node_results[0].attempts, 3);
    }

    #[tokio::test]
    async fn test_skip_if_condition() {
        let mut wf = WorkflowDag::new("skip-test");
        let decision = wf.add_node(
            "decider",
            "Decider",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!(true)) })),
        );
        let worker = wf.add_node_full(
            "worker",
            "Worker (should be skipped)",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"worked": true})) })),
            0,
            0,
            1000,
            Some("decider".to_string()), // skip_if: decider's output is true
        );

        wf.add_edge(decision, worker).unwrap();

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 2);
        assert_eq!(result.node_results[0].status, NodeStatus::Completed);
        assert_eq!(result.node_results[1].status, NodeStatus::Skipped);
    }

    #[tokio::test]
    async fn test_skip_if_false_does_not_skip() {
        let mut wf = WorkflowDag::new("no-skip-test");
        let decision = wf.add_node(
            "decider",
            "Decider",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!(false)) })),
        );
        let worker = wf.add_node_full(
            "worker",
            "Worker (should run)",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"worked": true})) })),
            0,
            0,
            1000,
            Some("decider".to_string()),
        );

        wf.add_edge(decision, worker).unwrap();

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(result.success);
        assert_eq!(result.node_results.len(), 2);
        assert_eq!(result.node_results[0].status, NodeStatus::Completed);
        assert_eq!(result.node_results[1].status, NodeStatus::Completed);
    }

    #[tokio::test]
    async fn test_event_emission() {
        use std::sync::Mutex;

        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let mut wf = WorkflowDag::new("event-test");
        wf.set_event_handler(Arc::new(move |event| {
            let mut ev = events_clone.lock().unwrap();
            ev.push(event);
        }));

        let a = wf.add_node(
            "node_a",
            "Node A",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"done": true})) })),
        );

        wf.add_node(
            "node_b",
            "Node B",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"done": true})) })),
        );

        // Make node_b depend on node_a
        let b = wf.nodes().iter().find(|(_, n)| n.name == "node_b").map(|(id, _)| *id).unwrap();
        wf.add_edge(a, b).unwrap();

        let _ = wf.execute(test_ctx(), Value::Null).await.unwrap();

        let captured = events.lock().unwrap();
        assert!(captured.len() >= 4); // WorkflowStarted + 2x NodeCompleted or NodeStarted + WorkflowCompleted
        // Check we at least have start and end events
        let has_start = captured.iter().any(|e| matches!(e, WorkflowEvent::WorkflowStarted { .. }));
        let has_complete = captured.iter().any(|e| matches!(e, WorkflowEvent::WorkflowCompleted { .. }));
        assert!(has_start);
        assert!(has_complete);
    }

    #[tokio::test]
    async fn test_serialization_roundtrip() {
        let mut wf = WorkflowDag::new("serialize-test");
        let a = wf.add_node(
            "step_a",
            "Step A",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"step": "A"})) })),
        );
        let b = wf.add_node(
            "step_b",
            "Step B",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"step": "B"})) })),
        );
        wf.add_edge(a, b).unwrap();

        // Serialize DAG (handlers are skipped)
        let json_str = serde_json::to_string_pretty(&wf).unwrap();
        let deserialized: WorkflowDag = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.name(), "serialize-test");
        assert_eq!(deserialized.node_count(), 2);
        assert_eq!(deserialized.edge_count(), 1);

        // Serialize node
        let node_json = serde_json::to_string_pretty(deserialized.get_node(&a).unwrap()).unwrap();
        let node_back: WorkflowNode = serde_json::from_str(&node_json).unwrap();
        assert_eq!(node_back.name, "step_a");
    }

    #[tokio::test]
    async fn test_add_edges_batch() {
        let mut wf = WorkflowDag::new("batch-edge-test");
        let a = wf.add_node(
            "a", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let b = wf.add_node(
            "b", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let c = wf.add_node(
            "c", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );

        wf.add_edges(&[(a, b), (b, c)]).unwrap();
        let info = wf.info();
        assert_eq!(info.edge_count, 2);
    }

    #[tokio::test]
    async fn test_remove_node() {
        let mut wf = WorkflowDag::new("remove-test");
        let a = wf.add_node(
            "a", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let b = wf.add_node(
            "b", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        wf.add_edge(a, b).unwrap();
        assert_eq!(wf.node_count(), 2);

        wf.remove_node(a).unwrap();
        assert_eq!(wf.node_count(), 1);
        // b's dependencies should be updated
        let node_b = wf.get_node(&b).unwrap();
        assert!(node_b.dependencies.is_empty());
    }

    #[tokio::test]
    async fn test_root_and_leaf_nodes() {
        let mut wf = WorkflowDag::new("root-leaf-test");
        let a = wf.add_node(
            "a", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let b = wf.add_node(
            "b", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        let c = wf.add_node(
            "c", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(Value::Null) })),
        );
        wf.add_edge(a, b).unwrap();
        wf.add_edge(a, c).unwrap();

        let roots = wf.root_nodes();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], a);

        let leaves = wf.leaf_nodes();
        assert_eq!(leaves.len(), 2);
        assert!(leaves.contains(&b));
        assert!(leaves.contains(&c));
    }

    #[tokio::test]
    async fn test_workflow_clone_resets_handlers() {
        let mut wf = WorkflowDag::new("clone-test");
        wf.add_node(
            "a", "",
            Value::Null,
            Arc::new(|_ctx, _input| Box::pin(async { Ok(json!({"done": true})) })),
        );
        let wf2 = wf.clone();
        assert_eq!(wf2.node_count(), wf.node_count());
        // Handlers should be empty in clone
        assert!(wf2.handlers.is_empty());
    }
}
