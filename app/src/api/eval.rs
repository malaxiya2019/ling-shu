//! 评测框架端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{Json, extract::State};
use std::sync::Arc;

pub use crate::api::full::{
    eval_run_handler, eval_result_handler, eval_regression_handler,
};

/// Axum route definition for Evaluator module
pub fn eval_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/eval/run", axum::routing::post(eval_run_handler))
        .route("/v1/eval/result", axum::routing::get(eval_result_handler))
        .route("/v1/eval/regression", axum::routing::post(eval_regression_handler))
}
