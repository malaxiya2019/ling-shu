//! LSWorkflow — 基于 DAG 的工作流引擎.
//!
//! ## 核心概念
//! - **Node**: 工作流中的一个任务节点
//! - **Edge**: 节点间的依赖关系 (DAG 有向无环图)
//! - **Workflow**: 完整的 DAG 定义与执行引擎
//!
//! ## 使用方法
//! ```rust,ignore
//! let mut wf = Workflow::new("my-workflow");
//! let node_a = wf.add_node("step_a", |ctx| async { Ok(json!({"result": "A"})) });
//! let node_b = wf.add_node("step_b", |ctx| async { Ok(json!({"result": "B"})) });
//! wf.add_edge(node_a, node_b).unwrap(); // B 依赖 A
//! let result = wf.execute(ctx).await.unwrap();
//! ```

pub mod dag;

pub use dag::{Workflow, WorkflowDag, WorkflowError, WorkflowNode, WorkflowResult};
