//! LSBilling — Lingshu 用量跟踪与计费系统.
//!
//! 提供 Token 级用量记录、计费策略、配额管理和使用报告功能。
//!
//! ## 架构
//!
//! ```text
//! ┌───────────────────────────────────────────┐
//! │              BillingSystem                 │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────┐  │
//! │  │ UsageTracker││BillingPlan││QuotaManager│  │
//! │  └──────────┘ └──────────┘ └──────────┘  │
//! │  ┌──────────────────────────────────┐    │
//! │  │        UsageReport               │    │
//! │  └──────────────────────────────────┘    │
//! └───────────────────────────────────────────┘
//! ```

pub mod plan;
pub mod quota;
pub mod report;
pub mod usage;

pub use plan::{BillingPlan, BillingTier, RateCard};
pub use quota::{Quota, QuotaManager};
pub use report::{PeriodType, ReportGenerator, UsageReport};
pub use usage::{UsageRecord, UsageTracker};

use lingshu_core::LsResult;
use std::sync::Arc;

/// 计费系统统一入口.
#[derive(Clone)]
pub struct BillingSystem {
    pub tracker: Arc<UsageTracker>,
    pub quota_manager: Arc<QuotaManager>,
    pub report_generator: Arc<ReportGenerator>,
}

impl BillingSystem {
    pub fn new(plans: Vec<BillingPlan>) -> LsResult<Self> {
        let tracker = Arc::new(UsageTracker::new());
        let quota_manager = Arc::new(QuotaManager::new(plans));
        let report_generator = Arc::new(ReportGenerator::new(tracker.clone()));
        Ok(Self {
            tracker,
            quota_manager,
            report_generator,
        })
    }

    /// 记录一次用量（同时更新配额消耗）.
    pub async fn record_usage(
        &self,
        user_id: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> LsResult<UsageRecord> {
        let total = input_tokens + output_tokens;
        let record = self
            .tracker
            .record(user_id, model, input_tokens, output_tokens)
            .await?;
        self.quota_manager.consume(user_id, total).await;
        Ok(record)
    }

    /// 检查用户配额.
    pub async fn check_quota(&self, user_id: &str, tier: &str) -> LsResult<Quota> {
        self.quota_manager.check_quota(user_id, tier).await
    }

    /// 生成使用报告.
    pub async fn generate_report(
        &self,
        user_id: &str,
        period: PeriodType,
    ) -> LsResult<UsageReport> {
        self.report_generator.generate(user_id, period).await
    }
}
