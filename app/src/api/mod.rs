//! API 路由模块 — OpenHands FastAPI + MCP Router 模式
//!
//! ## 架构
//!
//! 每个功能域一个独立模块，每个模块导出 `*_routes()` 函数。
//! `build_app_router()` 聚合所有模块的路由为单一 Router。
//! `build_router(state)` 应用状态并合并 full.rs 遗留路由。
//!
//! ### 模块
//!
//! | 模块 | 状态 | 说明 |
//! |------|------|------|
//! | `full.rs` | 🏗 过渡期 | 旧 single-file 实现 (5678 lines) |
//! | `health.rs` | ✅ | 健康检查端点 |
//! | `metrics.rs` | ✅ | 实时指标端点 |
//! | `auth.rs` | ✅ | 认证端点 |
//! | `chat.rs` | ✅ | Chat completion 端点 |
//! | `agents.rs` | ✅ | Agent 操作端点 |
//! | `plugins.rs` | ✅ | 插件管理端点 |
//! | `mcp.rs` | ✅ | MCP 协议端点 |
//! | `federation.rs` | ✅ | 联邦网络端点 |
//! | `eval.rs` | ✅ | 评测框架端点 |

pub mod agents;
pub mod auth;
pub mod chat;
pub mod eval;
pub mod federation;
pub mod full;
pub mod health;
pub mod mcp;
pub mod metrics;
pub mod plugins;

// ── Shared State ────────────────────────────────────

pub use full::AppState;

// ── Composite Router ────────────────────────────────

use axum::Router;
use std::sync::Arc;

/// 构建无状态的聚合路由 (State 未绑定).
///
/// 遵循 OpenHands FastAPI + MCP router 模式:
/// 每个模块独立定义路由，由本函数聚合.
pub fn build_app_router() -> Router<Arc<AppState>> {
    Router::new()
        .merge(health::health_routes())
        .merge(metrics::metrics_routes())
        .merge(auth::auth_routes())
        .merge(chat::chat_routes())
        .merge(agents::agent_routes())
        .merge(federation::federation_routes())
        .merge(eval::eval_routes())
        .merge(plugins::plugin_routes())
        .merge(mcp::mcp_routes())
}

/// 构建完整路由 — 应用共享状态 + 合并 full.rs 遗留路由.
///
/// 替代 `full::build_router()`.
pub fn build_router(state: Arc<AppState>) -> Router {
    build_app_router()
        .merge(full::build_router(state.clone()))
        .with_state(state)
}
