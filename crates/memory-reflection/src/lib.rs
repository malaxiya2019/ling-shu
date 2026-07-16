//! LSMemoryReflection — 记忆反思层。
//!
//! 评估 Memory Query 的质量，检测证据冲突，
//! 记录反馈以持续优化记忆检索和推理。
//!
//! # 架构
//!
//! ```text
//! Memory Query
//!      │
//!      ▼
//!  MemoryWorkflow
//!      │
//!      ▼
//!  EvidenceGraph
//!      │
//!      ▼
//! ReflectionEvaluator ← 评估质量
//!      │
//!  ┌───┴───┐
//!  │       │
//! 得分   反馈记录
//! 详情   (ReflectionFeedback)
//!      │
//!      ▼
//! ReflectionWorkflow (MemoryWorkflow impl)
//!      │
//!      ▼
//!  EvidenceGraph (反思报告)
//! ```
//!
//! # 评估维度
//!
//! - **证据充分性**：找到的相关事件数量
//! - **一致性**：检测矛盾事件（相同实体、时间冲突）
//! - **完整性**：时间线覆盖是否完整
//! - **置信度**：综合评分
//! - **改进建议**：基于评估结果给出可操作建议

mod evaluator;
mod feedback;
mod workflow;

pub use evaluator::*;
pub use feedback::*;
pub use workflow::*;
