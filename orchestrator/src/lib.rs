//! LSOrchestrator — 多智能体编排引擎.
//!
//! 提供智能体注册、发现、调度、通信和任务委派能力。
//! 基于 `Agent` trait 构建，支持多种调度策略和通信模式。
//!
//! ## 核心组件
//! - **AgentRegistry** — 基于能力的智能体注册与发现
//! - **AgentScheduler** — 任务调度与负载均衡
//! - **InterAgentComm** — 智能体间消息传递
//! - **Orchestrator** — 编排器主引擎 (团队管理、任务委派)
//! - [可选] **AutoAgentsBridge** — 多 Agent 编排框架集成
//! - [可选] **LoongAdapter** — 轻量 Agent 基础设施集成

pub mod comm;
pub mod orchestrator;
pub mod pipeline;
pub mod registry;
pub mod scheduler;

// autoagents_bridge 和 loong_adapter 始终注册模块（内部通过 cfg 隔离实现和桩）
pub mod autoagents_bridge;
pub mod loong_adapter;

pub use comm::{AgentMessage, DeliveryStatus, InterAgentComm, MessageEnvelope};
pub use orchestrator::{DelegationResult, Orchestrator, OrchestratorConfig, TeamConfig};
pub use pipeline::{
    default_code_graph, CodeUnderstandingPipeline, IncrementalChange, PipelineConfig,
    PipelineReport,
};
pub use registry::{AgentCapability, AgentInfo, AgentProbe, AgentRegistry, ProbeResult};
pub use scheduler::{AgentScheduler, ScheduleStrategy, TaskAssignment};

pub use autoagents_bridge::{AutoAgentsOrchestrator, CrewConfig, ReActConfig};
pub use loong_adapter::{LoongAdapter, LoongAgentConfig};
