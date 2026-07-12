//! 🔍 MCP Server 自动发现 API
//!
//! v4.3 Enterprise

use crate::api::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

#[allow(dead_code)]
pub async fn discovery_list_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    // 简化实现: 返回已发现的 MCP 服务器
    Json(serde_json::json!({
        "servers": [],
        "total": 0,
        "discovery_enabled": true,
    }))
}

#[allow(dead_code)]
pub async fn discovery_health_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "managed_servers": [],
    }))
}

/// Axum route definition for Discovery module
#[allow(dead_code)]
pub fn discovery_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/discovery/servers", axum::routing::get(discovery_list_handler))
        .route("/v1/discovery/health", axum::routing::get(discovery_health_handler))
}
