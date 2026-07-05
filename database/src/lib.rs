//! LSDatabase — 结构化数据持久化.
//!
//! ## Feature Flags
//! - `sqlite` — SQLite 后端 (默认启用, 开发环境)
//! - `postgres` — PostgreSQL 后端 (生产环境)
//!
//! ## 核心实现
//! - `SqliteDatabase` — 基于 rusqlite 的异步 SQLite 实现
//! - `PostgresDatabase` — 基于 sqlx 的 PostgreSQL 实现
//!
//! ## 自动迁移
//! 首次连接时自动执行 `migrations/001_init.sql`, 创建以下表:
//! - documents, users, sessions, memories, vectors, events, audit_logs, plugins

pub mod repository;
#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "sqlite")]
pub use repository::DatabaseRepository;
pub use sqlite::SqliteDatabase;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "postgres")]
pub use postgres::PostgresDatabase;
