//! LSAutonomy — Experience Store
//!
//! 记录 Agent 的执行经验，供自我反思和自我进化引擎使用。

use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// 经验类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperienceType {
    /// 任务执行
    TaskExecution,
    /// 决策过程
    Decision,
    /// 对话交互
    Conversation,
    /// 错误/异常
    Error,
    /// 性能评估
    Performance,
    /// 外部反馈
    Feedback,
    /// 群体协作
    Collaboration,
}

impl ExperienceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExperienceType::TaskExecution => "task_execution",
            ExperienceType::Decision => "decision",
            ExperienceType::Conversation => "conversation",
            ExperienceType::Error => "error",
            ExperienceType::Performance => "performance",
            ExperienceType::Feedback => "feedback",
            ExperienceType::Collaboration => "collaboration",
        }
    }
}

/// 经验严重等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperienceSeverity {
    /// 信息
    Info,
    /// 注意
    Notice,
    /// 警告
    Warning,
    /// 错误
    Error,
    /// 严重
    Critical,
}

impl ExperienceSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExperienceSeverity::Info => "info",
            ExperienceSeverity::Notice => "notice",
            ExperienceSeverity::Warning => "warning",
            ExperienceSeverity::Error => "error",
            ExperienceSeverity::Critical => "critical",
        }
    }

    pub fn score(&self) -> u8 {
        match self {
            ExperienceSeverity::Info => 1,
            ExperienceSeverity::Notice => 2,
            ExperienceSeverity::Warning => 3,
            ExperienceSeverity::Error => 4,
            ExperienceSeverity::Critical => 5,
        }
    }
}

/// 单条经验记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceEntry {
    /// 经验 ID
    pub id: LsId,
    /// Agent ID
    pub agent_id: String,
    /// 经验类型
    pub exp_type: ExperienceType,
    /// 严重等级
    pub severity: ExperienceSeverity,
    /// 时间戳
    pub timestamp: i64,
    /// 标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 上下文数据（JSON）
    pub context: serde_json::Value,
    /// 执行结果
    pub outcome: ExperienceOutcome,
    /// 相关标签
    pub tags: Vec<String>,
    /// 关联的任务 ID
    pub related_task_id: Option<LsId>,
    /// 关联的其他 Agent ID
    pub related_agent_ids: Vec<String>,
    /// 执行耗时 ms
    pub duration_ms: u64,
    /// 是否已分析
    pub analyzed: bool,
}

impl ExperienceEntry {
    pub fn new(
        agent_id: impl Into<String>,
        exp_type: ExperienceType,
        title: impl Into<String>,
        description: impl Into<String>,
        outcome: ExperienceOutcome,
    ) -> Self {
        Self {
            id: LsId::new(),
            agent_id: agent_id.into(),
            exp_type,
            severity: ExperienceSeverity::Info,
            timestamp: chrono::Utc::now().timestamp(),
            title: title.into(),
            description: description.into(),
            context: serde_json::json!({}),
            outcome,
            tags: Vec::new(),
            related_task_id: None,
            related_agent_ids: Vec::new(),
            duration_ms: 0,
            analyzed: false,
        }
    }

    pub fn with_severity(mut self, severity: ExperienceSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn with_related_task(mut self, task_id: LsId) -> Self {
        self.related_task_id = Some(task_id);
        self
    }
}

/// 经验结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExperienceOutcome {
    /// 成功
    Success,
    /// 失败
    Failure(String),
    /// 部分成功（附说明）
    PartialSuccess(String),
    /// 未知
    Unknown,
}

impl ExperienceOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, ExperienceOutcome::Success)
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, ExperienceOutcome::Failure(_))
    }

    pub fn score(&self) -> f64 {
        match self {
            ExperienceOutcome::Success => 1.0,
            ExperienceOutcome::PartialSuccess(_) => 0.5,
            ExperienceOutcome::Failure(_) => 0.0,
            ExperienceOutcome::Unknown => 0.25,
        }
    }
}

/// 经验摘要统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceSummary {
    /// Agent ID
    pub agent_id: String,
    /// 总经验数
    pub total_count: u64,
    /// 成功数
    pub success_count: u64,
    /// 失败数
    pub failure_count: u64,
    /// 各类经验数量
    pub type_counts: HashMap<String, u64>,
    /// 各等级数量
    pub severity_counts: HashMap<String, u64>,
    /// 平均耗时 ms
    pub avg_duration_ms: f64,
    /// 平均成功率
    pub success_rate: f64,
    /// 最近经验时间
    pub last_experience_at: i64,
    /// 常见失败标签
    pub common_failure_tags: Vec<(String, u64)>,
}

impl ExperienceSummary {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            total_count: 0,
            success_count: 0,
            failure_count: 0,
            type_counts: HashMap::new(),
            severity_counts: HashMap::new(),
            avg_duration_ms: 0.0,
            success_rate: 1.0,
            last_experience_at: 0,
            common_failure_tags: Vec::new(),
        }
    }
}

/// 经验存储
pub struct ExperienceStore {
    /// 所有经验记录（按 agent_id 分组）
    entries: Arc<RwLock<HashMap<String, Vec<ExperienceEntry>>>>,
    /// 最大存储条数
    max_entries_per_agent: usize,
}

impl ExperienceStore {
    pub fn new(max_entries_per_agent: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries_per_agent,
        }
    }

    /// 存储一条经验
    pub async fn store(&self, entry: ExperienceEntry) {
        let mut all = self.entries.write().await;
        let agent_entries = all.entry(entry.agent_id.clone()).or_default();
        let agent_id = entry.agent_id.clone();
        agent_entries.push(entry);
        // 裁剪超过上限的条目
        while agent_entries.len() > self.max_entries_per_agent {
            agent_entries.remove(0);
        }
        debug!("stored experience for agent '{}'", agent_id);
    }

    /// 批量存储经验
    pub async fn store_batch(&self, entries: Vec<ExperienceEntry>) {
        for entry in entries {
            self.store(entry).await;
        }
    }

    /// 获取 Agent 的所有经验
    pub async fn get_agent_experiences(
        &self,
        agent_id: &str,
    ) -> Vec<ExperienceEntry> {
        let all = self.entries.read().await;
        all.get(agent_id).cloned().unwrap_or_default()
    }

    /// 获取 Agent 的经验（按类型过滤）
    pub async fn get_experiences_by_type(
        &self,
        agent_id: &str,
        exp_type: ExperienceType,
    ) -> Vec<ExperienceEntry> {
        let all = self.entries.read().await;
        all.get(agent_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.exp_type == exp_type)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取 Agent 的失败经验
    pub async fn get_failures(&self, agent_id: &str) -> Vec<ExperienceEntry> {
        let all = self.entries.read().await;
        all.get(agent_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| matches!(e.outcome, ExperienceOutcome::Failure(_)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取所有 Agent ID
    pub async fn get_agent_ids(&self) -> Vec<String> {
        let all = self.entries.read().await;
        all.keys().cloned().collect()
    }

    /// 计算经验摘要
    pub async fn summarize(&self, agent_id: &str) -> ExperienceSummary {
        let all = self.entries.read().await;
        let entries = match all.get(agent_id) {
            Some(e) => e,
            None => return ExperienceSummary::new(agent_id),
        };

        let total_count = entries.len() as u64;
        let success_count = entries
            .iter()
            .filter(|e| matches!(e.outcome, ExperienceOutcome::Success))
            .count() as u64;
        let failure_count = entries
            .iter()
            .filter(|e| matches!(e.outcome, ExperienceOutcome::Failure(_)))
            .count() as u64;

        let mut type_counts: HashMap<String, u64> = HashMap::new();
        let mut severity_counts: HashMap<String, u64> = HashMap::new();
        let mut total_duration: u64 = 0;

        let mut failure_tags: Vec<String> = Vec::new();

        for entry in entries {
            *type_counts.entry(entry.exp_type.as_str().to_string()).or_insert(0) += 1;
            *severity_counts
                .entry(entry.severity.as_str().to_string())
                .or_insert(0) += 1;
            total_duration += entry.duration_ms;

            if matches!(entry.outcome, ExperienceOutcome::Failure(_)) {
                for tag in &entry.tags {
                    failure_tags.push(tag.clone());
                }
            }
        }

        // 常见失败标签
        let mut tag_count: HashMap<String, u64> = HashMap::new();
        for tag in failure_tags {
            *tag_count.entry(tag).or_insert(0) += 1;
        }
        let mut common_failure_tags: Vec<(String, u64)> = tag_count.into_iter().collect();
        common_failure_tags.sort_by_key(|b| std::cmp::Reverse(b.1));
        common_failure_tags.truncate(10);

        let last_exp = entries.last().map(|e| e.timestamp).unwrap_or(0);

        ExperienceSummary {
            agent_id: agent_id.to_string(),
            total_count,
            success_count,
            failure_count,
            type_counts,
            severity_counts,
            avg_duration_ms: if total_count > 0 {
                total_duration as f64 / total_count as f64
            } else {
                0.0
            },
            success_rate: if total_count > 0 {
                success_count as f64 / total_count as f64
            } else {
                1.0
            },
            last_experience_at: last_exp,
            common_failure_tags,
        }
    }

    /// 清除 Agent 的所有经验
    pub async fn clear_agent(&self, agent_id: &str) {
        let mut all = self.entries.write().await;
        all.remove(agent_id);
        info!("cleared all experiences for agent '{}'", agent_id);
    }

    /// 获取未分析的经验
    pub async fn get_unanalyzed(&self, agent_id: &str) -> Vec<ExperienceEntry> {
        let all = self.entries.read().await;
        all.get(agent_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| !e.analyzed)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 标记经验为已分析
    pub async fn mark_analyzed(&self, agent_id: &str, exp_id: &LsId) {
        let mut all = self.entries.write().await;
        if let Some(entries) = all.get_mut(agent_id) {
            if let Some(entry) = entries.iter_mut().find(|e| e.id == *exp_id) {
                entry.analyzed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let store = ExperienceStore::new(100);
        let entry = ExperienceEntry::new(
            "agent-1",
            ExperienceType::TaskExecution,
            "Task completed",
            "Successfully executed task",
            ExperienceOutcome::Success,
        );
        store.store(entry).await;

        let entries = store.get_agent_experiences("agent-1").await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Task completed");
    }

    #[tokio::test]
    async fn test_summarize() {
        let store = ExperienceStore::new(100);
        let agent_id = "agent-1";

        // Add 3 successes and 2 failures
        for i in 0..3 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("Success {}", i),
                "ok",
                ExperienceOutcome::Success,
            );
            store.store(entry).await;
        }
        for i in 0..2 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::Decision,
                format!("Fail {}", i),
                "error",
                ExperienceOutcome::Failure(format!("err {}", i)),
            )
            .with_severity(ExperienceSeverity::Error);
            store.store(entry).await;
        }

        let summary = store.summarize(agent_id).await;
        assert_eq!(summary.total_count, 5);
        assert_eq!(summary.success_count, 3);
        assert_eq!(summary.failure_count, 2);
        assert!((summary.success_rate - 0.6).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_get_failures() {
        let store = ExperienceStore::new(100);
        store
            .store(ExperienceEntry::new(
                "agent-1",
                ExperienceType::TaskExecution,
                "ok",
                "ok",
                ExperienceOutcome::Success,
            ))
            .await;
        store
            .store(ExperienceEntry::new(
                "agent-1",
                ExperienceType::TaskExecution,
                "fail",
                "err",
                ExperienceOutcome::Failure("timeout".into()),
            ))
            .await;

        let failures = store.get_failures("agent-1").await;
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].title, "fail");
    }

    #[tokio::test]
    async fn test_max_entries() {
        let store = ExperienceStore::new(3);
        for i in 0..5 {
            store
                .store(ExperienceEntry::new(
                    "agent-1",
                    ExperienceType::TaskExecution,
                    format!("exp {}", i),
                    "test",
                    ExperienceOutcome::Success,
                ))
                .await;
        }
        let entries = store.get_agent_experiences("agent-1").await;
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].title, "exp 2"); // Oldest removed
    }

    #[test]
    fn test_experience_outcome_scores() {
        assert!((ExperienceOutcome::Success.score() - 1.0).abs() < 0.01);
        assert!((ExperienceOutcome::Failure("x".into()).score() - 0.0).abs() < 0.01);
        assert!((ExperienceOutcome::PartialSuccess("x".into()).score() - 0.5).abs() < 0.01);
    }
}
