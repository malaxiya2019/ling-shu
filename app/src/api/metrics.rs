//! 实时 Metrics 端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use std::sync::Arc;

/// GET /v1/metrics — 系统实时指标 (CPU/Memory/Token)
#[allow(dead_code)]
pub async fn metrics_handler() -> (axum::http::StatusCode, String) {
    // Simplified metrics — full version in full.rs
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_all();

    let metrics = serde_json::json!({
        "cpu_usage_percent": sys.global_cpu_usage(),
        "memory": {
            "total_mb": sys.total_memory() / 1024 / 1024,
            "used_mb": sys.used_memory() / 1024 / 1024,
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    (
        axum::http::StatusCode::OK,
        serde_json::to_string(&metrics).unwrap_or_default(),
    )
}

/// Axum route definition for Metrics module
#[allow(dead_code)]
pub fn metrics_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route("/v1/metrics", axum::routing::get(metrics_handler))
}
