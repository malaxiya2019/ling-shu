//! 联邦网络端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use std::sync::Arc;

pub use crate::api::full::{
    federation_status_handler, federation_nodes_handler, federation_execute_handler,
};

/// Axum route definition for Federation module
#[allow(dead_code)]
pub fn federation_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/federation/status", axum::routing::get(federation_status_handler))
        .route("/v1/federation/nodes", axum::routing::get(federation_nodes_handler))
        .route("/v1/federation/execute", axum::routing::post(federation_execute_handler))
}
