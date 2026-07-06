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
//! │  │ (追加写入日志) │ │ (事件溯源恢复)   │   │
//! │  └──────────────┘ └──────────────────┘   │
//! │  ┌──────────────────────────────────┐    │
//! │  │        AuditQuery                │    │
//! │  └──────────────────────────────────┘    │
//! └───────────────────────────────────────────┘
//! ```

pub mod event;
pub mod log;
pub mod query;

pub use event::{EventSourcer, StoredEvent};
pub use log::{AuditEntry, AuditEventType, AuditLog, AuditLogStore};
pub use query::{AuditQuery, AuditQueryBuilder};
