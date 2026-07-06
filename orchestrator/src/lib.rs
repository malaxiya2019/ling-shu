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

pub mod comm;
pub mod orchestrator;
pub mod registry;
pub mod scheduler;
pub mod pipeline;

pub use comm::{AgentMessage, DeliveryStatus, InterAgentComm, MessageEnvelope};
pub use orchestrator::{DelegationResult, Orchestrator, OrchestratorConfig, TeamConfig};
pub use registry::{AgentCapability, AgentInfo, AgentRegistry, AgentProbe, ProbeResult};
pub use scheduler::{AgentScheduler, ScheduleStrategy, TaskAssignment};
pub use pipeline::{CodeUnderstandingPipeline, IncrementalChange, PipelineConfig, PipelineReport, default_code_graph};
