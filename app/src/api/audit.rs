//! 审计日志 API 端点
//!
//! 提供审计日志的查询、统计、详情、导出和归档功能。
//!
//! | 端点 | 方法 | 说明 |
//! |------|------|------|
//! | `/v1/audit/logs` | GET | 查询审计日志 |
//! | `/v1/audit/stats` | GET | 审计统计 |
//! | `/v1/audit/entry/:id` | GET | 单条审计详情 |
//! | `/v1/audit/export` | GET | 审计日志导出 (CSV/JSON) |
//! | `/v1/audit/archive` | POST | 归档旧审计记录 |

use crate::api::AppState;
use std::sync::Arc;

// 从 full.rs 中重导出 handler
pub use crate::api::full::{
    audit_archive_handler, audit_entry_handler, audit_export_handler, audit_query_handler,
    audit_stats_handler,
};

/// Axum route definition for Audit module
pub fn audit_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/audit/logs", axum::routing::get(audit_query_handler))
        .route("/v1/audit/stats", axum::routing::get(audit_stats_handler))
        .route(
            "/v1/audit/entry/:id",
            axum::routing::get(audit_entry_handler),
        )
        .route(
            "/v1/audit/export",
            axum::routing::get(audit_export_handler),
        )
        .route(
            "/v1/audit/archive",
            axum::routing::post(audit_archive_handler),
        )
}
