//! LSTraits — Lingshu 公共接口契约。
//!
//! 定义 14 个核心 Trait，全系统跨模块交互的唯一标准。
//! 所有 Trait 均已标注 `#[async_trait] + Send + Sync + 'static`，
//! 所有方法统一返回 `LsResult<T>`。

pub mod agent;
pub mod database;
pub mod embedding;
pub mod event_bus;
pub mod knowledge;
pub mod llm;
pub mod memory;
pub mod plugin;
pub mod repository;
pub mod runtime;
pub mod scheduler;
pub mod storage;
pub mod tool;
pub mod vector_store;

pub use agent::*;
pub use database::*;
pub use embedding::*;
pub use event_bus::*;
pub use knowledge::*;
pub use llm::*;
pub use memory::*;
pub use plugin::*;
pub use repository::*;
pub use runtime::*;
pub use scheduler::*;
pub use storage::*;
pub use tool::*;
pub use vector_store::*;
