//! Workflow Planner — 工作流规划器.
//!
//! 将用户目标或步骤列表自动转化为 DAG 工作流。
//! 支持 Planner trait 扩展和 SimplePlanner 默认实现。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use super::dag::{NodeHandler, WorkflowDag};

/// 工作流步骤定义.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// 步骤名称.
    pub name: String,
    /// 步骤描述.
    pub description: String,
    /// 步骤配置.
    pub config: Value,
    /// 依赖的步骤名称列表.
    pub depends_on: Vec<String>,
    /// 超时秒数.
    pub timeout_secs: u64,
    /// 重试次数.
    pub retry_count: u32,
    /// 重试延迟毫秒.
    pub retry_delay_ms: u64,
    /// 条件跳过.
    pub skip_if: Option<String>,
}

impl WorkflowStep {
    /// 创建新的步骤.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            config: Value::Null,
            depends_on: Vec::new(),
            timeout_secs: 0,
            retry_count: 0,
            retry_delay_ms: 1000,
            skip_if: None,
        }
    }

    /// 设置描述.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// 设置配置.
    pub fn with_config(mut self, config: Value) -> Self {
        self.config = config;
        self
    }

    /// 添加依赖.
    pub fn depends_on(mut self, dep: impl Into<String>) -> Self {
        self.depends_on.push(dep.into());
        self
    }

    /// 设置超时.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 设置重试.
    pub fn with_retry(mut self, count: u32, delay_ms: u64) -> Self {
        self.retry_count = count;
        self.retry_delay_ms = delay_ms;
        self
    }
}

/// 规划器 trait — 将目标转化为 DAG 工作流.
#[async_trait::async_trait]
pub trait Planner: Send + Sync {
    /// 根据目标和上下文规划工作流.
    async fn plan(&self, goal: &str, context: &Value) -> LsResult<PlannedWorkflow>;
}

/// 已规划的工作流 (不包含处理器).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedWorkflow {
    /// 工作流名称.
    pub name: String,
    /// 步骤列表.
    pub steps: Vec<WorkflowStep>,
    /// 额外上下文.
    pub context: Value,
}

impl PlannedWorkflow {
    /// 创建新的已规划工作流.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
            context: Value::Null,
        }
    }

    /// 添加步骤.
    pub fn add_step(&mut self, step: WorkflowStep) {
        self.steps.push(step);
    }
}

/// SimplePlanner — 从步骤列表构建 DAG 工作流.
///
/// 将 `PlannedWorkflow` 中的步骤列表转换为实际的 `WorkflowDag`，
/// 根据 `depends_on` 自动建立边。
pub struct SimplePlanner;

impl SimplePlanner {
    /// 从已规划的工作流构建 DAG.
    ///
    /// `handler_factory` — 根据步骤名称创建节点处理函数。
    pub fn build_dag<F>(planned: &PlannedWorkflow, handler_factory: F) -> LsResult<WorkflowDag>
    where
        F: Fn(&str) -> Option<NodeHandler>,
    {
        let mut dag = WorkflowDag::new(&planned.name);
        let mut node_ids: Vec<(String, LsId)> = Vec::new();

        // 第一遍: 创建所有节点
        for step in &planned.steps {
            let handler = handler_factory(&step.name).unwrap_or_else(|| {
                Arc::new(|_ctx: LsContext, _input: Value| {
                    Box::pin(async { Ok::<Value, LsError>(Value::Null) })
                        as std::pin::Pin<
                            Box<dyn std::future::Future<Output = LsResult<Value>> + Send>,
                        >
                }) as NodeHandler
            });

            let id = dag.add_node_full(
                &step.name,
                &step.description,
                step.config.clone(),
                handler,
                step.timeout_secs,
                step.retry_count,
                step.retry_delay_ms,
                step.skip_if.clone(),
            );
            node_ids.push((step.name.clone(), id));
        }

        // 第二遍: 通过名称建立边
        let name_to_id: std::collections::HashMap<&str, LsId> = node_ids
            .iter()
            .map(|(name, id)| (name.as_str(), *id))
            .collect();

        for step in &planned.steps {
            let to_id = name_to_id
                .get(step.name.as_str())
                .ok_or_else(|| LsError::NotFound(format!("step '{}' not found", step.name)))?;

            for dep_name in &step.depends_on {
                let from_id = name_to_id.get(dep_name.as_str()).ok_or_else(|| {
                    LsError::NotFound(format!("dependency '{}' not found", dep_name))
                })?;
                dag.add_edge(*from_id, *to_id)?;
            }
        }

        debug!(
            workflow = %planned.name,
            nodes = dag.node_count(),
            edges = dag.edge_count(),
            "built DAG from planned workflow"
        );

        Ok(dag)
    }

    /// 从步骤列表直接构建 DAG (使用默认空处理器).
    pub fn build_dag_from_steps(steps: Vec<WorkflowStep>) -> LsResult<WorkflowDag> {
        let planned = PlannedWorkflow {
            name: "auto".to_string(),
            steps,
            context: Value::Null,
        };
        Self::build_dag(&planned, |_| None)
    }

    /// 从步骤列表创建 WorkflowDag 并返回名称到 ID 的映射.
    pub fn build_dag_with_mapping(
        planned: &PlannedWorkflow,
        handler_factory: impl Fn(&str) -> Option<NodeHandler>,
    ) -> LsResult<(WorkflowDag, std::collections::HashMap<String, LsId>)> {
        let mut dag = WorkflowDag::new(&planned.name);
        let mut name_to_id: std::collections::HashMap<String, LsId> =
            std::collections::HashMap::new();

        // 第一遍: 创建所有节点
        for step in &planned.steps {
            let handler = handler_factory(&step.name).unwrap_or_else(|| {
                Arc::new(|_ctx: LsContext, _input: Value| {
                    Box::pin(async { Ok::<Value, LsError>(Value::Null) })
                        as std::pin::Pin<
                            Box<dyn std::future::Future<Output = LsResult<Value>> + Send>,
                        >
                }) as NodeHandler
            });

            let id = dag.add_node_full(
                &step.name,
                &step.description,
                step.config.clone(),
                handler,
                step.timeout_secs,
                step.retry_count,
                step.retry_delay_ms,
                step.skip_if.clone(),
            );
            name_to_id.insert(step.name.clone(), id);
        }

        // 第二遍: 通过名称建立边
        for step in &planned.steps {
            if let Some(to_id) = name_to_id.get(&step.name) {
                for dep_name in &step.depends_on {
                    if let Some(from_id) = name_to_id.get(dep_name) {
                        dag.add_edge(*from_id, *to_id)?;
                    }
                }
            }
        }

        Ok((dag, name_to_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn test_workflow_step_builder() {
        let step = WorkflowStep::new("test")
            .with_description("A test step")
            .with_config(json!({"key": "value"}))
            .depends_on("prev_step")
            .with_timeout(30)
            .with_retry(3, 1000);

        assert_eq!(step.name, "test");
        assert_eq!(step.description, "A test step");
        assert_eq!(step.config["key"], "value");
        assert_eq!(step.depends_on, vec!["prev_step"]);
        assert_eq!(step.timeout_secs, 30);
        assert_eq!(step.retry_count, 3);
        assert_eq!(step.retry_delay_ms, 1000);
    }

    #[test]
    fn test_simple_planner_build_empty() {
        let steps = vec![];
        let dag = SimplePlanner::build_dag_from_steps(steps).unwrap();
        assert_eq!(dag.node_count(), 0);
    }

    #[test]
    fn test_simple_planner_build_single() {
        let steps = vec![WorkflowStep::new("single_step")];
        let dag = SimplePlanner::build_dag_from_steps(steps).unwrap();
        assert_eq!(dag.node_count(), 1);
        assert_eq!(dag.root_nodes().len(), 1);
    }

    #[test]
    fn test_simple_planner_build_linear() {
        let steps = vec![
            WorkflowStep::new("step_a"),
            WorkflowStep::new("step_b").depends_on("step_a"),
            WorkflowStep::new("step_c").depends_on("step_b"),
        ];
        let dag = SimplePlanner::build_dag_from_steps(steps).unwrap();
        assert_eq!(dag.node_count(), 3);
        assert_eq!(dag.edge_count(), 2);

        // Verify topological order
        let order = dag.topological_sort().unwrap();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn test_simple_planner_build_fork_join() {
        let steps = vec![
            WorkflowStep::new("start"),
            WorkflowStep::new("fork_a").depends_on("start"),
            WorkflowStep::new("fork_b").depends_on("start"),
            WorkflowStep::new("join")
                .depends_on("fork_a")
                .depends_on("fork_b"),
        ];
        let dag = SimplePlanner::build_dag_from_steps(steps).unwrap();
        assert_eq!(dag.node_count(), 4);
        assert_eq!(dag.edge_count(), 4);
    }

    #[test]
    fn test_planned_workflow_add_step() {
        let mut planned = PlannedWorkflow::new("test");
        planned.add_step(WorkflowStep::new("step_a"));
        planned.add_step(WorkflowStep::new("step_b").depends_on("step_a"));
        assert_eq!(planned.steps.len(), 2);
    }

    #[test]
    fn test_build_dag_with_mapping() {
        let planned = PlannedWorkflow {
            name: "mapping-test".to_string(),
            steps: vec![
                WorkflowStep::new("step_a"),
                WorkflowStep::new("step_b").depends_on("step_a"),
            ],
            context: Value::Null,
        };

        let (dag, mapping) = SimplePlanner::build_dag_with_mapping(&planned, |name| {
            if name == "step_a" {
                Some(Arc::new(|_ctx, _input| {
                    Box::pin(async { Ok::<Value, LsError>(json!({"done": true})) })
                        as std::pin::Pin<
                            Box<dyn std::future::Future<Output = LsResult<Value>> + Send>,
                        >
                }) as NodeHandler)
            } else {
                None
            }
        })
        .unwrap();

        assert_eq!(dag.node_count(), 2);
        assert!(mapping.contains_key("step_a"));
        assert!(mapping.contains_key("step_b"));
    }
}
