//! LSEpisode — Episode Memory Store.
//!
//! 事件级记忆存储，记录带有时间戳、实体、状态变更的非结构化事件。
//! 是 L3 Episode Memory 的核心存储层。
//!
//! # 设计原则
//!
//! - **不要** GraphDB — 第一版只做 KV + 时间索引
//! - **不要** 因果推理 — 第一版只做事实记录
//! - **不要** 向量嵌入 — 第一版只做标签/实体匹配
//!
//! # Architecture
//!
//! ```text
//! EpisodeRepository (trait)
//!       │
//!   ┌───┴───┐
//!   │        │
//! InMemory  SQLite (future)
//! ```

mod episode;
mod repository;
mod query;
mod store;

pub use episode::*;
pub use repository::*;
pub use query::*;
pub use store::*;
