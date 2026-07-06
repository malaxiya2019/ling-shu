//! LSKnowledgeGraph — Lingshu 知识图谱引擎.
//!
//! 参照 Understand Anything 的图类型系统设计，提供：
//! - 21 种节点 + 35 种边的类型系统
//! - GraphBuilder 去重图构建
//! - 图查询 / 搜索
//! - 逻辑层检测
//! - 导览生成
//! - Agent 交互记忆存储
//!
//! ## 架构
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │           KnowledgeGraph                  │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────┐ │
//! │  │  Types   │ │ Builder  │ │  Query   │ │
//! │  │ (N/E)    │ │ (去重)   │ │ (搜索)   │ │
//! │  └──────────┘ └──────────┘ └──────────┘ │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────┐ │
//! │  │  Layer   │ │  Tour    │ │  Memory  │ │
//! │  │ (分层)   │ │ (导览)   │ │ (记忆)   │ │
//! │  └──────────┘ └──────────┘ └──────────┘ │
//! └──────────────────────────────────────────┘
//! ```

pub mod builder;
pub mod memory;
pub mod store;
pub mod types;

pub use builder::GraphBuilder;
pub use memory::{GraphMemory, GraphMemoryStore};
pub use store::GraphStore;
pub use types::*;
