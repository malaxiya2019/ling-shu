//! 插件管理端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use std::sync::Arc;

// Plugin types re-exported from full.rs for the transition period
pub use crate::api::full::{
    plugin_list_handler, plugin_install_handler, plugin_get_handler,
    plugin_start_handler, plugin_stop_handler, plugin_uninstall_handler,
    plugin_events_handler,
    market_refresh_handler, market_install_handler, market_search_handler,
};

/// Axum route definition for Plugins module
#[allow(dead_code)]
pub fn plugin_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/plugins", axum::routing::get(plugin_list_handler))
        .route("/v1/plugins/install", axum::routing::post(plugin_install_handler))
        .route("/v1/plugins/:id", axum::routing::get(plugin_get_handler))
        .route("/v1/plugins/:id/start", axum::routing::post(plugin_start_handler))
        .route("/v1/plugins/:id/stop", axum::routing::post(plugin_stop_handler))
        .route("/v1/plugins/:id/uninstall", axum::routing::post(plugin_uninstall_handler))
        .route("/v1/plugins/events", axum::routing::get(plugin_events_handler))
        .route("/v1/plugins/market/refresh", axum::routing::post(market_refresh_handler))
        .route("/v1/plugins/market/install", axum::routing::post(market_install_handler))
        .route("/v1/plugins/market/search", axum::routing::get(market_search_handler))
}
