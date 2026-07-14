//! Workflow — 工作流引擎
//!
//! 基于 DAG 的工作流定义与执行引擎，支持条件分支、并行执行、人工审批、子工作流等。

pub mod approval;
pub mod checkpoint;
pub mod dag;
pub mod planner;
pub mod registry;
pub mod sub_workflow;
pub mod workflow_access;
pub mod workflow_tools;

pub use approval::{ApprovalManager, ApprovalRequest, ApprovalStatus};
pub use checkpoint::*;
pub use dag::*;
pub use planner::{PlannedWorkflow, Planner, SimplePlanner, WorkflowStep};
pub use registry::{WorkflowRegistry, WorkflowRegistryEntry};
pub use sub_workflow::{
    SubWorkflowConfig, SubWorkflowExecutor, SubWorkflowResult, SubWorkflowStrategy,
};
pub use workflow_tools::{register_workflow_tools, ListWorkflowsTool, WorkflowExecuteTool};
