//! LSRuntime — Lingshu 运行时实现 (v4.0 Agent Runtime).
//!
//! 全系统唯一入口与资源管控中心，负责：
//! - 生命周期管理 (LifecycleManager)
//! - 会话管理 (SessionManager)
//! - 内部调度 (InternalScheduler)
//! - 故障恢复 (RecoveryManager)
//! - 智能体生命周期管理 (AgentManager)
//! - Agent 执行流水线 (AgentPipeline)
//! - Agent 持久化 (AgentPersistence)
//! - Agent 池复用 (AgentPool)
//! - Agent 工厂 (AgentFactory / LsAgentFactory)
//! - 顶层运行时 (AgentRuntime)
//! - [可选] chidori 集成 — 持久化可回放 Agent 恢复

pub mod agent_factory;
pub mod agent_manager;
pub mod agent_persistence;
pub mod agent_pipeline;
pub mod agent_pool;
pub mod agent_runtime;
pub mod lifecycle;
pub mod api;
pub mod events;
pub mod memory_pipeline;
pub mod recovery;
pub mod scheduler;
pub mod session;
pub mod tool_registry;

// chidori_recovery 始终注册模块（内部通过 cfg 隔离实现和桩）
pub mod chidori_recovery;

pub use agent_factory::{AgentRegistration, LsAgentFactory};
pub use agent_manager::{AgentManager, AgentSummary};
pub use agent_persistence::{AgentPersistenceManager, AgentRecord, AgentStore, InMemoryAgentStore};
pub use agent_pipeline::{
    ActStage, AgentPipeline, MemoryStage, PipelineAgent, PipelineContext, PipelineStage,
    PostProcessStage, PreProcessStage, StageAction, ThinkStage,
};
pub use agent_pool::{AgentFactory, AgentHandle, AgentPool, AgentPoolConfig, AgentPoolStats};
pub use agent_runtime::{AgentRuntime, AgentRuntimeConfig, WorkflowAccess};
pub use lifecycle::{LifecycleManager, LifecycleState};
pub use recovery::{FaultEvent, FaultLevel, RecoveryManager, RecoveryResult, RecoveryStrategy};
pub use scheduler::{InternalScheduler, ScheduledTask, TaskState};
pub use session::{SessionInfo, SessionManager, SessionState};
pub use tool_registry::ToolRegistry;

pub use chidori_recovery::{
    ChidoriRecoveryManager, ChidoriSavePoint, CheckpointConfig, CheckpointRecovery,
};
#[cfg(feature = "api-server")]
pub mod api_server;
