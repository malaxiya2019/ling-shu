//! 配额管理 — 基于用户 tier 的用量上限检查.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};

use crate::plan::BillingPlan;

/// 用户当前配额状态.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quota {
    pub user_id: String,
    pub tier: String,
    pub tokens_used: u64,
    pub token_quota: u64,
    pub requests_used: u64,
    pub request_limit: u64,
    pub reset_at: DateTime<Utc>,
}

impl Quota {
    /// 配额是否充足.
    pub fn has_quota(&self, estimated_tokens: u64) -> bool {
        self.tokens_used + estimated_tokens <= self.token_quota
            && self.requests_used < self.request_limit
    }

    /// 剩余 Token 配额.
    pub fn remaining_tokens(&self) -> u64 {
        self.token_quota.saturating_sub(self.tokens_used)
    }

    /// 剩余请求数.
    pub fn remaining_requests(&self) -> u64 {
        self.request_limit.saturating_sub(self.requests_used)
    }
}

/// 配额管理器.
#[derive(Clone)]
pub struct QuotaManager {
    plans: Vec<BillingPlan>,
    /// 用户当前用量 (user_id -> (tokens, requests, period_start))
    usage: DashMap<String, (u64, u64, DateTime<Utc>)>,
}

impl QuotaManager {
    pub fn new(plans: Vec<BillingPlan>) -> Self {
        Self {
            plans,
            usage: DashMap::new(),
        }
    }

    /// 获取默认方案（按 tier 匹配）.
    pub fn get_plan(&self, tier: &str) -> LsResult<BillingPlan> {
        self.plans
            .iter()
            .find(|p| p.tier.as_str() == tier)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("billing plan {tier} not found")))
    }

    /// 检查用户配额.
    pub async fn check_quota(&self, user_id: &str, tier: &str) -> LsResult<Quota> {
        let plan = self.get_plan(tier)?;
        let now = Utc::now();

        let (tokens_used, requests_used, period_start) = self
            .usage
            .get(user_id)
            .map(|e| {
                let (t, r, ts) = e.value().clone();
                // 如果已进入新周期，重置用量
                if now.signed_duration_since(ts) > chrono::Duration::days(30) {
                    (0, 0, now)
                } else {
                    (t, r, ts)
                }
            })
            .unwrap_or((0, 0, now));

        Ok(Quota {
            user_id: user_id.to_string(),
            tier: tier.to_string(),
            tokens_used,
            token_quota: plan.monthly_token_quota,
            requests_used,
            request_limit: plan.monthly_request_limit,
            reset_at: period_start + chrono::Duration::days(30),
        })
    }

    /// 消费配额.
    pub async fn consume(&self, user_id: &str, tokens: u64) {
        self.usage
            .entry(user_id.to_string())
            .and_modify(|(t, r, _ts)| {
                *t += tokens;
                *r += 1;
            })
            .or_insert_with(|| (tokens, 1, Utc::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plans() -> Vec<BillingPlan> {
        vec![BillingPlan::free(), BillingPlan::basic()]
    }

    #[tokio::test]
    async fn test_check_free_quota() {
        let manager = QuotaManager::new(test_plans());
        let quota = manager.check_quota("alice", "free").await.unwrap();
        assert_eq!(quota.tier, "free");
        assert_eq!(quota.token_quota, 1_000_000);
        assert_eq!(quota.tokens_used, 0);
    }

    #[tokio::test]
    async fn test_check_basic_quota() {
        let manager = QuotaManager::new(test_plans());
        let quota = manager.check_quota("bob", "basic").await.unwrap();
        assert_eq!(quota.token_quota, 10_000_000);
    }

    #[tokio::test]
    async fn test_plan_not_found() {
        let manager = QuotaManager::new(test_plans());
        let result = manager.check_quota("alice", "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_consume_updates_quota() {
        let manager = QuotaManager::new(test_plans());
        manager.consume("alice", 500).await;
        let quota = manager.check_quota("alice", "free").await.unwrap();
        assert_eq!(quota.tokens_used, 500);
        assert_eq!(quota.requests_used, 1);
    }
}
