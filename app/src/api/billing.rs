//! 💰 Token 用量 & 成本统计 API
//!
//! v4.3 Enterprise — Token Cost Dashboard
//!
//! 使用全局内存存储跟踪用量数据（进程内持久化）。
//! 生产环境应替换为数据库后端。

use crate::api::AppState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::sync::RwLock;

// ── 全局内存存储 ────────────────────────────────────

static BILLING_STORE: LazyLock<RwLock<BillingStore>> =
    LazyLock::new(|| RwLock::new(BillingStore::new()));

struct BillingStore {
    /// 按模型统计
    per_model: HashMap<String, ModelCounter>,
    /// 按用户统计
    per_user: HashMap<String, UserCounter>,
}

#[derive(Default, Clone, Serialize)]
struct ModelCounter {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Default, Clone, Serialize)]
struct UserCounter {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
}

impl BillingStore {
    fn new() -> Self {
        Self {
            per_model: HashMap::new(),
            per_user: HashMap::new(),
        }
    }

    fn record(&mut self, model: &str, user_id: &str, input_tokens: u64, output_tokens: u64) {
        let mc = self.per_model.entry(model.to_string()).or_default();
        mc.requests += 1;
        mc.input_tokens += input_tokens;
        mc.output_tokens += output_tokens;

        let uc = self.per_user.entry(user_id.to_string()).or_default();
        uc.requests += 1;
        uc.input_tokens += input_tokens;
        uc.output_tokens += output_tokens;
    }
}

// ── 响应类型 ────────────────────────────────────────

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

/// 记录用量请求
#[derive(Deserialize)]
pub struct RecordUsageReq {
    pub user_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Deserialize)]
pub struct ReportQuery {
    pub period: Option<String>,
}

// ── Handler ─────────────────────────────────────────

/// GET /v1/billing/stats — 全局用量统计
pub async fn billing_stats_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<BillingStatsResponse> {
    let store = BILLING_STORE.read().await;

    let total_requests: u64 = store.per_model.values().map(|m| m.requests).sum();
    let total_input: u64 = store.per_model.values().map(|m| m.input_tokens).sum();
    let total_output: u64 = store.per_model.values().map(|m| m.output_tokens).sum();

    let per_model: Vec<ModelStatItem> = store
        .per_model
        .iter()
        .map(|(model, mc)| ModelStatItem {
            model: model.clone(),
            requests: mc.requests,
            input_tokens: mc.input_tokens,
            output_tokens: mc.output_tokens,
            total_tokens: mc.input_tokens + mc.output_tokens,
        })
        .collect();

    Json(BillingStatsResponse {
        total_requests,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_tokens: total_input + total_output,
        per_model,
    })
}

/// GET /v1/billing/report/:user_id — 用户成本报告
pub async fn billing_report_handler(
    State(_state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<ReportQuery>,
) -> Json<serde_json::Value> {
    let _period = params.period.unwrap_or_else(|| "daily".into());

    let store = BILLING_STORE.read().await;
    let user_stats = store.per_user.get(&user_id).cloned().unwrap_or_default();

    // 简化成本计算：input $0.002/1K tokens, output $0.008/1K tokens (参考 GPT-3.5)
    let input_cost = user_stats.input_tokens as f64 * 0.002 / 1000.0;
    let output_cost = user_stats.output_tokens as f64 * 0.008 / 1000.0;
    let total_cost = input_cost + output_cost;

    Json(serde_json::json!({
        "user_id": user_id,
        "period": _period,
        "total_requests": user_stats.requests,
        "total_tokens": user_stats.input_tokens + user_stats.output_tokens,
        "input_tokens": user_stats.input_tokens,
        "output_tokens": user_stats.output_tokens,
        "estimated_cost": format!("{:.4}", total_cost),
        "input_cost": format!("{:.4}", input_cost),
        "output_cost": format!("{:.4}", output_cost),
        "currency": "USD",
        "tier": "enterprise",
    }))
}

/// GET /v1/billing/quota/:user_id — 用户配额查询
pub async fn billing_quota_handler(
    State(_state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let store = BILLING_STORE.read().await;
    let used = store
        .per_user
        .get(&user_id)
        .map(|u| u.input_tokens + u.output_tokens)
        .unwrap_or(0);

    Json(serde_json::json!({
        "user_id": user_id,
        "tier": "enterprise",
        "used": used,
        "limit": "unlimited",
        "remaining": "unlimited",
        "status": "active",
    }))
}

/// POST /v1/billing/usage — 记录用量
pub async fn record_usage_handler(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<RecordUsageReq>,
) -> Json<serde_json::Value> {
    let mut store = BILLING_STORE.write().await;
    store.record(
        &req.model,
        &req.user_id,
        req.input_tokens,
        req.output_tokens,
    );

    let total_requests: u64 = store.per_model.values().map(|m| m.requests).sum();

    Json(serde_json::json!({
        "status": "recorded",
        "model": req.model,
        "input_tokens": req.input_tokens,
        "output_tokens": req.output_tokens,
        "total_requests": total_requests,
    }))
}

/// Axum route definition for Billing module
pub fn billing_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/v1/billing/stats",
            axum::routing::get(billing_stats_handler),
        )
        .route(
            "/v1/billing/report/:user_id",
            axum::routing::get(billing_report_handler),
        )
        .route(
            "/v1/billing/quota/:user_id",
            axum::routing::get(billing_quota_handler),
        )
        .route(
            "/v1/billing/usage",
            axum::routing::post(record_usage_handler),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_stats() {
        // 确保初始状态为空
        {
            let mut store = BILLING_STORE.write().await;
            store.per_model.clear();
            store.per_user.clear();
        }

        // 记录用量
        {
            let mut store = BILLING_STORE.write().await;
            store.record("gpt-4", "user-1", 100, 50);
            store.record("gpt-4", "user-1", 200, 80);
            store.record("gpt-3.5", "user-2", 50, 20);
        }

        // 验证全局统计
        {
            let store = BILLING_STORE.read().await;
            assert_eq!(store.per_model.len(), 2);

            let gpt4 = store.per_model.get("gpt-4").unwrap();
            assert_eq!(gpt4.requests, 2);
            assert_eq!(gpt4.input_tokens, 300);
            assert_eq!(gpt4.output_tokens, 130);

            let gpt35 = store.per_model.get("gpt-3.5").unwrap();
            assert_eq!(gpt35.requests, 1);
            assert_eq!(gpt35.input_tokens, 50);
            assert_eq!(gpt35.output_tokens, 20);
        }

        // 验证用户统计
        {
            let store = BILLING_STORE.read().await;
            let user1 = store.per_user.get("user-1").unwrap();
            assert_eq!(user1.requests, 2);
            assert_eq!(user1.input_tokens, 300);
            assert_eq!(user1.output_tokens, 130);

            let user2 = store.per_user.get("user-2").unwrap();
            assert_eq!(user2.requests, 1);
        }

        // 清理
        {
            let mut store = BILLING_STORE.write().await;
            store.per_model.clear();
            store.per_user.clear();
        }
    }

    #[tokio::test]
    async fn test_empty_store() {
        let store = BILLING_STORE.read().await;
        assert!(store.per_model.is_empty());
        assert!(store.per_user.is_empty());
    }
}
