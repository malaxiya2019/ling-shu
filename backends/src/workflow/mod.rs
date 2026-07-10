//! LSWorkflow — 基于 DAG 的工作流引擎.
//!
//! ## 核心概念
//! - **Node**: 工作流中的一个任务节点
//! - **Edge**: 节点间的依赖关系 (DAG 有向无环图)
//! - **Workflow**: 完整的 DAG 定义与执行引擎
//!
//! ## 模块
//! - `dag` — 核心 DAG 引擎（拓扑排序、环检测、并行执行、超时、重试、条件跳过）
//! - `checkpoint` — 工作流快照持久化与恢复
//! - `planner` — 工作流规划器（从步骤列表自动构建 DAG）
//! - `registry` — 工作流注册表（管理多个工作流的注册与执行）
//!
//! ## 使用方法
//! ```rust,ignore
//! let mut wf = Workflow::new("my-workflow");
//! let node_a = wf.add_node("step_a", |ctx| async { Ok(json!({"result": "A"})) });
//! let node_b = wf.add_node("step_b", |ctx| async { Ok(json!({"result": "B"})) });
//! wf.add_edge(node_a, node_b).unwrap(); // B 依赖 A
//! let result = wf.execute(ctx).await.unwrap();
//! ```

pub mod checkpoint;
pub mod dag;
pub mod planner;
pub mod registry;

pub use checkpoint::{
    CheckpointManager, CheckpointStore, InMemoryCheckpointStore, WorkflowCheckpoint,
    WorkflowCheckpointSummary, resume_from_checkpoint,
};
pub use dag::{
    ExecutionSnapshot, NodeConfig, NodeHandler, NodeOutput, NodeResult, NodeStatus, Workflow,
    WorkflowDag, WorkflowError, WorkflowEvent, WorkflowEventHandler, WorkflowInfo, WorkflowNode,
    WorkflowResult,
};
pub use planner::{PlannedWorkflow, Planner, SimplePlanner, WorkflowStep};
pub use registry::{WorkflowRegistry, WorkflowRegistryEntry};
