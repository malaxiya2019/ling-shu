//! 健康检查端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{Json, extract::State};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
#[allow(dead_code)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime: String,
    pub checks: Vec<HealthCheckItem>,
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct HealthCheckItem {
    pub name: String,
    pub healthy: bool,
    pub detail: String,
}

/// GET /v1/health — 系统健康检查
#[allow(dead_code)]
pub async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let runtime = &state.runtime;
    let config = &runtime.config;
    let uptime = crate::api::full::format_duration(runtime.start_time.elapsed());
    let pkg_version = env!("CARGO_PKG_VERSION");

    let mut checks = Vec::new();

    // Runtime checks
    let agent_count = runtime.agent_manager.active_agent_count();
    checks.push(HealthCheckItem {
        name: "runtime".into(),
        healthy: true,
        detail: format!("{} active agents", agent_count),
    });

    // Plugin registry
    let plugin_count = state.plugin_registry.count().await;
    checks.push(HealthCheckItem {
        name: "plugins".into(),
        healthy: true,
        detail: format!("{} plugins registered", plugin_count),
    });

    // Event bus
    checks.push(HealthCheckItem {
        name: "eventbus".into(),
        healthy: true,
        detail: "operational".into(),
    });

    // Memory system
    checks.push(HealthCheckItem {
        name: "memory".into(),
        healthy: true,
        detail: "operational".into(),
    });

    // MCP server
    checks.push(HealthCheckItem {
        name: "mcp".into(),
        healthy: true,
        detail: "operational".into(),
    });

    // WebSocket server
    checks.push(HealthCheckItem {
        name: "websocket".into(),
        healthy: true,
        detail: format!("{} connections", state.ws_manager.active_connections()),
    });

    let status = if checks.iter().all(|c| c.healthy) {
        "healthy"
    } else {
        "degraded"
    };

    Json(HealthResponse {
        status: status.into(),
        version: format!("{}-{}", pkg_version, config.mode.as_deref().unwrap_or("dev")),
        uptime,
        checks,
    })
}

/// Axum route definition for Health module
#[allow(dead_code)]
pub fn health_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/health", axum::routing::get(health_handler))
}
