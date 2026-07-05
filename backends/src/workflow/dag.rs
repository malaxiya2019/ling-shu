//! WorkflowDag — DAG 工作流定义与执行引擎.
//!
//! 拓扑排序、环检测、并行执行、上下文传递。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::fmt;
use tracing::{debug, info, warn};

/// 节点输出数据.
pub type NodeOutput = Value;

/// 节点处理函数.
pub type NodeHandler = Arc<dyn Fn(LsContext, Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = LsResult<NodeOutput>> + Send>> + Send + Sync>;

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
    /// 重试次数.
    pub retry_count: u32,
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

/// DAG 工作流引擎.
pub struct WorkflowDag {
    id: LsId,
    name: String,
    nodes: HashMap<LsId, WorkflowNode>,
    handlers: HashMap<LsId, NodeHandler>,
    adj_list: HashMap<LsId, Vec<LsId>>,  // node -> dependents
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
        }
    }

    /// 添加节点.
    pub fn add_node(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        config: Value,
        handler: NodeHandler,
    ) -> LsId {
        let id = LsId::new();
        let node = WorkflowNode {
            id,
            name: name.into(),
            description: description.into(),
            dependencies: Vec::new(),
            config,
            timeout_secs: 0,
            retry_count: 0,
        };
        self.nodes.insert(id, node);
        self.handlers.insert(id, handler);
        self.adj_list.entry(id).or_default();
        id
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

    /// 拓扑排序 (Kahn's algorithm).
    pub fn topological_sort(&self) -> LsResult<Vec<LsId>> {
        let mut in_degree: HashMap<LsId, usize> = HashMap::new();
        for &id in self.nodes.keys() {
            in_degree.entry(id).or_insert(0);
        }
        for deps in self.adj_list.values() {
            for &dep in deps {
                *in_degree.entry(dep).or_insert(0) += 0;
            }
        }
        // Actually compute in-degree from reverse edges
        for (_from, to_list) in &self.adj_list {
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

    /// 执行工作流.
    pub async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<WorkflowResult> {
        let start = std::time::Instant::now();
        let started_at = chrono::Utc::now().to_rfc3339();
        let order = self.topological_sort()?;

        info!(
            workflow = %self.name,
            nodes = order.len(),
            "workflow execution started"
        );

        let mut results: HashMap<LsId, NodeResult> = HashMap::new();
        let mut node_outputs: HashMap<LsId, Value> = HashMap::new();

        // Track completed nodes for dependency resolution
        let mut completed = HashSet::new();
        let mut failed = false;

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
                    let handler = self.handlers.get(&node_id).cloned();
                    let node_ctx = ctx.child();
                    let node_input = input.clone();
                    let _timeout = node.timeout_secs;
                    let retry = node.retry_count;

                    let node_name = node.name.clone();
                    let handle = tokio::spawn(async move {
                        let node_start = std::time::Instant::now();
                        let mut attempts = 0u32;
                        let max_attempts = if retry > 0 { retry + 1 } else { 1 };

                        let result = loop {
                            attempts += 1;
                            match handler {
                                Some(ref h) => {
                                    match h(node_ctx.clone(), node_input.clone()).await {
                                        Ok(output) => break Ok(output),
                                        Err(e) => {
                                            if attempts >= max_attempts {
                                                break Err(e);
                                            }
                                            warn!(
                                                node = %node_name,
                                                attempt = attempts,
                                                error = %e,
                                                "retrying node"
                                            );
                                        }
                                    }
                                }
                                None => {
                                    break Ok(Value::Null);
                                }
                            }
                        };

                        let duration = node_start.elapsed().as_millis() as u64;
                        (node_id, result, duration, attempts)
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
                                    output: Some(output),
                                    error: None,
                                    duration_ms: duration,
                                    attempts,
                                },
                            );
                            debug!(
                                node = %node.name,
                                duration_ms = duration,
                                "node completed"
                            );
                        }
                    }
                    Ok((node_id, Err(e), duration, attempts)) => {
                        failed = true;
                        if let Some(node) = self.nodes.get(&node_id) {
                            results.insert(
                                node_id,
                                NodeResult {
                                    node_id,
                                    node_name: node.name.clone(),
                                    status: NodeStatus::Failed,
                                    output: None,
                                    error: Some(e.to_string()),
                                    duration_ms: duration,
                                    attempts,
                                },
                            );
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

    /// 获取工作流信息.
    pub fn info(&self) -> WorkflowInfo {
        WorkflowInfo {
            id: self.id,
            name: self.name.clone(),
            node_count: self.nodes.len(),
            edge_count: self.adj_list.values().map(|v| v.len()).sum(),
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
        if !visited.contains(&node) {
            if dfs(node, adj, &mut visited, &mut in_stack) {
                return true;
            }
        }
    }
    false
}

/// 简化工作流类型 (使用闭包/函数).
pub struct Workflow {
    inner: WorkflowDag,
}

impl Workflow {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            inner: WorkflowDag::new(name),
        }
    }

    /// 添加一个异步节点处理函数.
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

    /// 添加依赖边.
    pub fn add_edge(&mut self, from: LsId, to: LsId) -> LsResult<()> {
        self.inner.add_edge(from, to)
    }

    /// 执行工作流.
    pub async fn execute(&self, ctx: LsContext, input: Value) -> LsResult<WorkflowResult> {
        self.inner.execute(ctx, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;

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
            Arc::new(|_ctx, _input| {
                Box::pin(async { Ok(json!({"message": "hello"})) })
            }),
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
        let a = wf.add_node("step_a", "Step A", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"step": "A"})) })
        }));
        let b = wf.add_node("step_b", "Step B", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"step": "B"})) })
        }));
        let c = wf.add_node("step_c", "Step C", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"step": "C"})) })
        }));

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
        let start = wf.add_node("start", "Start", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"phase": "start"})) })
        }));
        let fork_a = wf.add_node("fork_a", "Fork A", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"branch": "A"})) })
        }));
        let fork_b = wf.add_node("fork_b", "Fork B", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"branch": "B"})) })
        }));
        let join = wf.add_node("join", "Join", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(json!({"phase": "join"})) })
        }));

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
        let a = wf.add_node("a", "A", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(Value::Null) })
        }));
        let b = wf.add_node("b", "B", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(Value::Null) })
        }));

        wf.add_edge(a, b).unwrap();
        // Creating a cycle: b -> a
        let result = wf.add_edge(b, a);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_node_failure() {
        let mut wf = WorkflowDag::new("failure-test");
        wf.add_node("fail", "Fail node", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async {
                Err(LsError::Internal("intentional failure".into()))
            })
        }));

        let result = wf.execute(test_ctx(), Value::Null).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.node_results[0].status, NodeStatus::Failed);
    }

    #[tokio::test]
    async fn test_workflow_info() {
        let mut wf = WorkflowDag::new("info-test");
        let a = wf.add_node("a", "", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(Value::Null) })
        }));
        let b = wf.add_node("b", "", Value::Null, Arc::new(|_ctx, _input| {
            Box::pin(async { Ok(Value::Null) })
        }));
        wf.add_edge(a, b).unwrap();

        let info = wf.info();
        assert_eq!(info.name, "info-test");
        assert_eq!(info.node_count, 2);
        assert_eq!(info.edge_count, 1);
    }
}
