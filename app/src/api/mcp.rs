//! MCP (Model Context Protocol) 端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{extract::State, response::Html, Json};
use lingshu_core::{LsContext, LsId};
use std::sync::Arc;

/// POST /v1/mcp — MCP JSON-RPC 端点
#[allow(dead_code)]
pub async fn mcp_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let ctx = LsContext::with_session(LsId::new());
    let response = state.runtime.mcp_server.handle_request(&ctx, body).await;
    Json(serde_json::to_value(&response).unwrap_or_default())
}

/// GET /v1/mcp/tools — 列出 MCP 工具
#[allow(dead_code)]
pub async fn mcp_tools_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tools = state.runtime.mcp_server.list_tools();
    Json(serde_json::json!({
        "tools": tools,
        "count": tools.len(),
    }))
}

/// GET /v1/mcp/ui — MCP 管理界面
#[allow(dead_code)]
pub async fn mcp_ui_handler() -> Result<Html<String>, (axum::http::StatusCode, String)> {
    let html = include_str!("../../mcp_ui.html").to_string();
    Ok(Html(html))
}

/// Axum route definition for MCP module
#[allow(dead_code)]
pub fn mcp_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/mcp", axum::routing::post(mcp_handler))
        .route("/v1/mcp/tools", axum::routing::get(mcp_tools_handler))
        .route("/v1/mcp/ui", axum::routing::get(mcp_ui_handler))
}
