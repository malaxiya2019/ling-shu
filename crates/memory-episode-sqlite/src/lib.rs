//! LSEpisodeSQLite — SQLite 持久化的 Episode 存储。
//!
//! 提供 `SQLiteEpisodeStore`，实现 `EpisodeRepository` trait。
//! 将 Episode 数据持久化到 SQLite 数据库。
//!
//! # 表结构
//!
//! ```sql
//! episodes          — 事件主表
//! episode_entities  — 事件关联实体
//! episode_state_changes — 事件状态变更
//! ```

mod store;
mod migrations;
pub mod evidence_persist;

pub use store::*;
pub use migrations::*;
