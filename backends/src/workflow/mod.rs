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
//! - `workflow_tools` — Agent 可调用的工作流工具（execute_workflow, list_workflows）

pub mod checkpoint;
pub mod dag;
pub mod planner;
pub mod registry;
pub mod workflow_tools;
pub mod workflow_access;

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
pub use workflow_tools::{WorkflowExecuteTool, ListWorkflowsTool, register_workflow_tools};
