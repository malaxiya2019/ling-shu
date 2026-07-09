//! 使用报告生成.

use chrono::{DateTime, Duration, Utc};
use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::usage::{UsageRecord, UsageTracker};

/// 报告周期类型.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PeriodType {
    /// 最近 24 小时.
    Daily,
    /// 最近 7 天.
    Weekly,
    /// 最近 30 天.
    Monthly,
    /// 自定义范围.
    Custom {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

/// 使用报告.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    pub user_id: String,
    pub period: PeriodType,
    pub generated_at: DateTime<Utc>,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    /// 按模型细分.
    pub per_model: Vec<ModelUsage>,
}

/// 单模型用量.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// 报告生成器.
pub struct ReportGenerator {
    tracker: Arc<UsageTracker>,
}

impl ReportGenerator {
    pub fn new(tracker: Arc<UsageTracker>) -> Self {
        Self { tracker }
    }

    /// 生成使用报告.
    pub async fn generate(&self, user_id: &str, period: PeriodType) -> LsResult<UsageReport> {
        let now = Utc::now();
        let (start, _end) = match &period {
            PeriodType::Daily => (now - Duration::hours(24), now),
            PeriodType::Weekly => (now - Duration::days(7), now),
            PeriodType::Monthly => (now - Duration::days(30), now),
            PeriodType::Custom { start, end } => (*start, *end),
        };

        let records = self.tracker.get_records(user_id).await?;

        // 过滤时间范围内的记录
        let filtered: Vec<&UsageRecord> = records
            .iter()
            .filter(|r| r.timestamp >= start && r.timestamp <= now)
            .collect();

        let total_requests = filtered.len() as u64;
        let total_input: u64 = filtered.iter().map(|r| r.input_tokens).sum();
        let total_output: u64 = filtered.iter().map(|r| r.output_tokens).sum();
        let total_tokens = total_input + total_output;

        // 按模型分组
        let mut model_map: std::collections::HashMap<String, ModelUsage> =
            std::collections::HashMap::new();
        for r in &filtered {
            let entry = model_map.entry(r.model.clone()).or_insert(ModelUsage {
                model: r.model.clone(),
                requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
            });
            entry.requests += 1;
            entry.input_tokens += r.input_tokens;
            entry.output_tokens += r.output_tokens;
            entry.total_tokens += r.total_tokens;
        }

        let mut per_model: Vec<ModelUsage> = model_map.into_values().collect();
        per_model.sort_by_key(|a| std::cmp::Reverse(a.total_tokens));

        Ok(UsageReport {
            user_id: user_id.to_string(),
            period,
            generated_at: now,
            total_requests,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            total_tokens,
            estimated_cost: 0.0, // 需根据实际方案计算
            per_model,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UsageTracker;

    #[tokio::test]
    async fn test_generate_daily_report() {
        let tracker = Arc::new(UsageTracker::new());
        tracker.record("alice", "gpt-4", 100, 50).await.unwrap();
        tracker.record("alice", "gpt-3.5", 200, 100).await.unwrap();

        let generator = ReportGenerator::new(tracker);
        let report = generator
            .generate("alice", PeriodType::Daily)
            .await
            .unwrap();

        assert_eq!(report.user_id, "alice");
        assert_eq!(report.total_requests, 2);
        assert_eq!(report.total_input_tokens, 300);
        assert_eq!(report.total_output_tokens, 150);
        assert_eq!(report.total_tokens, 450);
        assert_eq!(report.per_model.len(), 2);
    }
}
