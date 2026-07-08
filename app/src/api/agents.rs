//! Agent 操作端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

pub use crate::api::full::{
    agent_run_handler, agent_list_handler, agent_status_handler,
    agent_pause_handler, agent_resume_handler, agent_cancel_handler,
};

/// Axum route definition for Agents module
pub fn agent_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/agent/run", axum::routing::post(agent_run_handler))
        .route("/v1/agents", axum::routing::get(agent_list_handler))
        .route("/v1/agent/{id}", axum::routing::get(agent_status_handler))
        .route("/v1/agent/{id}/pause", axum::routing::post(agent_pause_handler))
        .route("/v1/agent/{id}/resume", axum::routing::post(agent_resume_handler))
        .route("/v1/agent/{id}/cancel", axum::routing::post(agent_cancel_handler))
}
