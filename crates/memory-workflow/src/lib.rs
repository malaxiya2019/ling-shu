//! LSMemoryWorkflow — 统一 Memory Workflow trait 和运行时集成。
//!
//! # Architecture
//!
//! ```text
//! Agent Runtime
//!      │
//!      ▼
//! MemoryWorkflow (trait)   ← 统一入口
//!      │
//!   ┌──┴──┐
//!   │     │
//! Timeline  EntitySearch  ... (更多 Workflow)
//!   │     │
//!   └──┬──┘
//!      ▼
//! EvidenceGraph            ← 统一输出
//! ```

mod workflow;
mod router;
mod integration;

pub use workflow::*;
pub use router::*;
pub use integration::*;
