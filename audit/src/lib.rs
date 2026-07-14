//! LSAudit — Lingshu 不可变审计日志与事件溯源.
//!
//! 提供追加写入的审计事件存储、事件溯源查询和审计追踪功能。
//!
//! ## 架构
//!
//! ```text
//! ┌───────────────────────────────────────────┐
//! │              AuditSystem                   │
//! │  ┌──────────────┐ ┌──────────────────┐   │
//! │  │ AuditLog     │ │ EventSourcer     │   │
//! │  │ (内存/文件)   │ │ (事件溯源恢复)   │   │
//! │  └──────────────┘ └──────────────────┘   │
//! │  ┌──────────────────────────────────┐    │
//! │  │        AuditQuery                │    │
//! │  └──────────────────────────────────┘    │
//! │  ┌──────────────────────────────────┐    │
//! │  │      SqliteAuditLog (SQLite)     │    │
//! │  └──────────────────────────────────┘    │
//! └───────────────────────────────────────────┘
//! ```

pub mod event;
pub mod log;
pub mod query;
#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use event::{EventSourcer, EventStore, InMemoryEventStore, StoredEvent};
pub use log::{AuditEntry, AuditEventType, AuditLog, AuditLogStore};
pub use query::{AuditQuery, AuditQueryBuilder};
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteAuditLog;
