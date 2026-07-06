//! 用量跟踪 — 记录每次 LLM 调用的 Token 消耗.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 单次用量记录.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Uuid,
    pub user_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub timestamp: DateTime<Utc>,
}

impl UsageRecord {
    pub fn new(user_id: &str, model: &str, input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.to_string(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            timestamp: Utc::now(),
        }
    }
}

/// 用量汇总（按用户/模型聚合）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub user_id: String,
    pub model: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub request_count: u64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// 用量跟踪器.
pub struct UsageTracker {
    /// 所有记录（内存存储）.
    records: Arc<RwLock<Vec<UsageRecord>>>,
    /// 按用户+模型聚合的索引.
    summary: DashMap<String, UsageAggregate>,
}

#[derive(Debug, Clone, Default)]
struct UsageAggregate {
    total_input: u64,
    total_output: u64,
    count: u64,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            summary: DashMap::new(),
        }
    }

    /// 记录一次用量.
    pub async fn record(
        &self,
        user_id: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> LsResult<UsageRecord> {
        let record = UsageRecord::new(user_id, model, input_tokens, output_tokens);

        // 持久化记录
        self.records.write().await.push(record.clone());

        // 更新聚合索引
        let key = format!("{}:{}", user_id, model);
        self.summary
            .entry(key)
            .and_modify(|agg| {
                agg.total_input += input_tokens;
                agg.total_output += output_tokens;
                agg.count += 1;
            })
            .or_insert_with(|| UsageAggregate {
                total_input: input_tokens,
                total_output: output_tokens,
                count: 1,
            });

        Ok(record)
    }

    /// 获取用户在某模型上的汇总.
    pub async fn get_summary(&self, user_id: &str, model: &str) -> LsResult<UsageSummary> {
        let key = format!("{}:{}", user_id, model);
        let agg = self.summary.get(&key).ok_or_else(|| {
            LsError::NotFound(format!("no usage for user {user_id} model {model}"))
        })?;

        let now = Utc::now();
        Ok(UsageSummary {
            user_id: user_id.to_string(),
            model: model.to_string(),
            total_input_tokens: agg.total_input,
            total_output_tokens: agg.total_output,
            total_tokens: agg.total_input + agg.total_output,
            request_count: agg.count,
            period_start: now - chrono::Duration::days(30),
            period_end: now,
        })
    }

    /// 获取用户所有记录.
    pub async fn get_records(&self, user_id: &str) -> LsResult<Vec<UsageRecord>> {
        let records = self.records.read().await;
        let user_records: Vec<_> = records
            .iter()
            .filter(|r| r.user_id == user_id)
            .cloned()
            .collect();
        if user_records.is_empty() {
            return Err(LsError::NotFound(format!("no records for user {user_id}")));
        }
        Ok(user_records)
    }

    /// 获取所有记录（管理员用）.
    pub async fn get_all_records(&self) -> LsResult<Vec<UsageRecord>> {
        Ok(self.records.read().await.clone())
    }
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_usage() {
        let tracker = UsageTracker::new();
        let record = tracker.record("alice", "gpt-4", 100, 50).await.unwrap();
        assert_eq!(record.user_id, "alice");
        assert_eq!(record.total_tokens, 150);
    }

    #[tokio::test]
    async fn test_get_summary() {
        let tracker = UsageTracker::new();
        tracker.record("alice", "gpt-4", 100, 50).await.unwrap();
        tracker.record("alice", "gpt-4", 200, 100).await.unwrap();

        let summary = tracker.get_summary("alice", "gpt-4").await.unwrap();
        assert_eq!(summary.total_input_tokens, 300);
        assert_eq!(summary.total_output_tokens, 150);
        assert_eq!(summary.total_tokens, 450);
        assert_eq!(summary.request_count, 2);
    }

    #[tokio::test]
    async fn test_get_summary_not_found() {
        let tracker = UsageTracker::new();
        let result = tracker.get_summary("bob", "gpt-4").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_records() {
        let tracker = UsageTracker::new();
        tracker.record("alice", "gpt-4", 100, 50).await.unwrap();
        let records = tracker.get_records("alice").await.unwrap();
        assert_eq!(records.len(), 1);
    }
}
