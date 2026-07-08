//! MCP (Model Context Protocol) 端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

pub use crate::api::full::mcp_handler;
use axum::response::Html;

/// GET /v1/mcp/tools — 列出 MCP 工具
pub async fn mcp_tools_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tools = state.runtime.mcp_server.list_tools();
    Json(serde_json::json!({
        "tools": tools,
        "count": tools.len(),
    }))
}

/// GET /v1/mcp/ui — MCP 管理界面
pub async fn mcp_ui_handler() -> Result<Html<String>, (axum::http::StatusCode, String)> {
    let html = include_str!("../../mcp_ui.html").to_string();
    Ok(Html(html))
}

/// Axum route definition for MCP module
pub fn mcp_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/mcp", axum::routing::post(mcp_handler))
        .route("/v1/mcp/tools", axum::routing::get(mcp_tools_handler))
        .route("/v1/mcp/ui", axum::routing::get(mcp_ui_handler))
}
