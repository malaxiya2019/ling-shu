//! LSRuntime — Lingshu 运行时实现。
//!
//! 全系统唯一入口与资源管控中心，负责：
//! - 生命周期管理 (LifecycleManager)
//! - 会话管理 (SessionManager)
//! - 内部调度 (InternalScheduler)
//! - 故障恢复 (RecoveryManager)
//! - 智能体生命周期管理 (AgentManager)

pub mod agent_manager;
pub mod lifecycle;
pub mod recovery;
pub mod scheduler;
pub mod session;
pub mod tool_registry;

pub use agent_manager::{AgentManager, AgentSummary};
pub use lifecycle::{LifecycleManager, LifecycleState};
pub use recovery::{FaultEvent, FaultLevel, RecoveryManager, RecoveryResult, RecoveryStrategy};
pub use scheduler::InternalScheduler;
pub use session::{SessionInfo, SessionManager, SessionState};
pub use tool_registry::ToolRegistry;
