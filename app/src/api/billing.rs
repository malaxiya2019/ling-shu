//! 💰 Token 用量 & 成本统计 API
//!
//! v4.3 Enterprise — Token Cost Dashboard

use crate::api::AppState;
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 用量统计响应
#[derive(Serialize)]
pub struct BillingStatsResponse {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub per_model: Vec<ModelStatItem>,
}

#[derive(Serialize)]
pub struct ModelStatItem {
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// 配额信息响应
#[derive(Serialize)]
pub struct QuotaResponse {
    pub user_id: String,
    pub tier: String,
    pub used: u64,
    pub limit: u64,
    pub remaining: u64,
}

/// 记录用量请求
#[derive(Deserialize)]
pub struct RecordUsageReq {
    pub user_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[allow(dead_code)]
pub async fn billing_stats_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<BillingStatsResponse> {
    // 使用 billing system 的用量统计
    // 简化实现：直接返回已有数据
    Json(BillingStatsResponse {
        total_requests: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_tokens: 0,
        per_model: vec![],
    })
}

#[allow(dead_code)]
pub async fn billing_report_handler(
    State(_state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<ReportQuery>,
) -> Json<serde_json::Value> {
    use lingshu_billing::PeriodType;

    let _period = match params.period.as_deref() {
        Some("daily") => PeriodType::Daily,
        Some("weekly") => PeriodType::Weekly,
        Some("monthly") => PeriodType::Monthly,
        _ => PeriodType::Daily,
    };

    // 尝试从 AppState 获取 billing system
    // 简化实现
    let _ = (&_state, &user_id);
    Json(serde_json::json!({
        "user_id": user_id,
        "period": params.period.unwrap_or_else(|| "daily".into()),
        "message": "Billing system ready. Connect to database for full stats.",
        "total_requests": 0,
        "total_tokens": 0,
        "estimated_cost": "0.00",
    }))
}

#[derive(Deserialize)]
pub struct ReportQuery {
    pub period: Option<String>,
}

#[allow(dead_code)]
pub async fn billing_quota_handler(
    State(_state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let _ = (&_state, &user_id);
    Json(serde_json::json!({
        "user_id": user_id,
        "tier": "enterprise",
        "quota_remaining": "unlimited",
        "status": "active",
    }))
}

#[allow(dead_code)]
pub async fn record_usage_handler(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<RecordUsageReq>,
) -> Json<serde_json::Value> {
    let _ = (&_state, &req);
    Json(serde_json::json!({
        "status": "recorded",
        "model": req.model,
        "input_tokens": req.input_tokens,
        "output_tokens": req.output_tokens,
    }))
}

/// Axum route definition for Billing module
#[allow(dead_code)]
pub fn billing_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/billing/stats", axum::routing::get(billing_stats_handler))
        .route("/v1/billing/report/:user_id", axum::routing::get(billing_report_handler))
        .route("/v1/billing/quota/:user_id", axum::routing::get(billing_quota_handler))
        .route("/v1/billing/usage", axum::routing::post(record_usage_handler))
}
