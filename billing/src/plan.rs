//! 计费方案 — 定义不同 tier 的费率与配额.

use serde::{Deserialize, Serialize};

/// 计费层级.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BillingTier {
    /// 免费层.
    Free,
    /// 基础层.
    Basic,
    /// 专业层.
    Pro,
    /// 企业层.
    Enterprise,
    /// 自定义层.
    Custom(String),
}

impl BillingTier {
    pub fn as_str(&self) -> &str {
        match self {
            BillingTier::Free => "free",
            BillingTier::Basic => "basic",
            BillingTier::Pro => "pro",
            BillingTier::Enterprise => "enterprise",
            BillingTier::Custom(s) => s.as_str(),
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "free" => BillingTier::Free,
            "basic" => BillingTier::Basic,
            "pro" => BillingTier::Pro,
            "enterprise" => BillingTier::Enterprise,
            custom => BillingTier::Custom(custom.to_string()),
        }
    }
}

/// 按模型区分的费率.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateCard {
    /// 模型名称（支持 glob 模式）.
    pub model_pattern: String,
    /// 每 1K 输入 Token 的价格（USD）.
    pub price_per_1k_input: f64,
    /// 每 1K 输出 Token 的价格（USD）.
    pub price_per_1k_output: f64,
}

/// 计费方案.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPlan {
    pub tier: BillingTier,
    pub name: String,
    /// 每月免费 Token 配额.
    pub monthly_token_quota: u64,
    /// 每月最大请求数.
    pub monthly_request_limit: u64,
    /// 支持的最大并发.
    pub max_concurrency: u32,
    /// 费率表.
    pub rate_cards: Vec<RateCard>,
    /// 超出配额后是否允许按量计费.
    pub allow_overage: bool,
}

impl BillingPlan {
    /// 创建默认免费方案.
    pub fn free() -> Self {
        Self {
            tier: BillingTier::Free,
            name: "Free".into(),
            monthly_token_quota: 1_000_000,
            monthly_request_limit: 10_000,
            max_concurrency: 1,
            rate_cards: vec![RateCard {
                model_pattern: "*".into(),
                price_per_1k_input: 0.0,
                price_per_1k_output: 0.0,
            }],
            allow_overage: false,
        }
    }

    /// 创建默认基础方案.
    pub fn basic() -> Self {
        Self {
            tier: BillingTier::Basic,
            name: "Basic".into(),
            monthly_token_quota: 10_000_000,
            monthly_request_limit: 100_000,
            max_concurrency: 5,
            rate_cards: vec![
                RateCard {
                    model_pattern: "gpt-4*".into(),
                    price_per_1k_input: 0.03,
                    price_per_1k_output: 0.06,
                },
                RateCard {
                    model_pattern: "gpt-3.5*".into(),
                    price_per_1k_input: 0.0015,
                    price_per_1k_output: 0.002,
                },
                RateCard {
                    model_pattern: "*".into(),
                    price_per_1k_input: 0.01,
                    price_per_1k_output: 0.02,
                },
            ],
            allow_overage: true,
        }
    }

    /// 创建默认专业方案.
    pub fn pro() -> Self {
        Self {
            tier: BillingTier::Pro,
            name: "Pro".into(),
            monthly_token_quota: 100_000_000,
            monthly_request_limit: 1_000_000,
            max_concurrency: 20,
            rate_cards: vec![
                RateCard {
                    model_pattern: "gpt-4*".into(),
                    price_per_1k_input: 0.02,
                    price_per_1k_output: 0.04,
                },
                RateCard {
                    model_pattern: "*".into(),
                    price_per_1k_input: 0.005,
                    price_per_1k_output: 0.01,
                },
            ],
            allow_overage: true,
        }
    }

    /// 估算本次调用的费用.
    pub fn estimate_cost(&self, model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        let card = self.rate_cards.iter().find(|c| {
            if c.model_pattern == "*" {
                return true;
            }
            // 简单 glob 匹配
            if c.model_pattern.ends_with('*') {
                let prefix = &c.model_pattern[..c.model_pattern.len() - 1];
                model.starts_with(prefix)
            } else {
                model == c.model_pattern
            }
        });

        match card {
            Some(c) => {
                (input_tokens as f64 / 1000.0) * c.price_per_1k_input
                    + (output_tokens as f64 / 1000.0) * c.price_per_1k_output
            }
            None => {
                // 无匹配费率，按默认零费率
                0.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_plan_estimate_cost() {
        let plan = BillingPlan::basic();
        let cost = plan.estimate_cost("gpt-4", 1000, 500);
        // (1000/1000)*0.03 + (500/1000)*0.06 = 0.03 + 0.03 = 0.06
        assert!((cost - 0.06).abs() < 1e-10);
    }

    #[test]
    fn test_basic_plan_estimate_cost_unknown_model() {
        let plan = BillingPlan::basic();
        let cost = plan.estimate_cost("claude-3", 1000, 500);
        // falls back to "*" rate: 0.01 + 0.01 = 0.02
        assert!((cost - 0.02).abs() < 1e-10);
    }

    #[test]
    fn test_free_plan_zero_cost() {
        let plan = BillingPlan::free();
        let cost = plan.estimate_cost("gpt-4", 10000, 5000);
        assert!((cost - 0.0).abs() < 1e-10);
    }
}
