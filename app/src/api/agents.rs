//! Agent 操作端点 — 生命周期管理
//!
//! ✅ v4.3 Enterprise: restart / update / delete

use crate::api::AppState;
use std::sync::Arc;

pub use crate::api::full::{
    agent_cancel_handler, agent_delete_handler, agent_list_handler, agent_pause_handler,
    agent_restart_handler, agent_resume_handler, agent_run_handler, agent_status_handler,
    agent_update_handler,
};

/// Axum route definition for Agents module
#[allow(dead_code)]
pub fn agent_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/agent/run", axum::routing::post(agent_run_handler))
        .route("/v1/agents", axum::routing::get(agent_list_handler))
        .route("/v1/agent/:id", axum::routing::get(agent_status_handler))
        .route(
            "/v1/agent/:id/pause",
            axum::routing::post(agent_pause_handler),
        )
        .route(
            "/v1/agent/:id/resume",
            axum::routing::post(agent_resume_handler),
        )
        .route(
            "/v1/agent/:id/cancel",
            axum::routing::post(agent_cancel_handler),
        )
        .route(
            "/v1/agent/:id/restart",
            axum::routing::post(agent_restart_handler),
        )
        .route(
            "/v1/agent/:id/update",
            axum::routing::post(agent_update_handler),
        )
        .route("/v1/agent/:id", axum::routing::delete(agent_delete_handler))
}
