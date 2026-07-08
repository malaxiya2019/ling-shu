//! LSRuntime — Lingshu 运行时实现。
//!
//! 全系统唯一入口与资源管控中心，负责：
//! - 生命周期管理 (LifecycleManager)
//! - 会话管理 (SessionManager)
//! - 内部调度 (InternalScheduler)
//! - 故障恢复 (RecoveryManager)
//! - 智能体生命周期管理 (AgentManager)
//! - [可选] chidori 集成 — 持久化可回放 Agent 恢复

pub mod agent_manager;
pub mod lifecycle;
pub mod recovery;
pub mod scheduler;
pub mod session;
pub mod tool_registry;

// chidori_recovery 始终注册模块（内部通过 cfg 隔离实现和桩）
pub mod chidori_recovery;

pub use agent_manager::{AgentManager, AgentSummary};
pub use lifecycle::{LifecycleManager, LifecycleState};
pub use recovery::{FaultEvent, FaultLevel, RecoveryManager, RecoveryResult, RecoveryStrategy};
pub use scheduler::InternalScheduler;
pub use session::{SessionInfo, SessionManager, SessionState};
pub use tool_registry::ToolRegistry;

pub use chidori_recovery::{
    ChidoriRecoveryManager, ChidoriSavePoint, CheckpointConfig, CheckpointRecovery,
};
