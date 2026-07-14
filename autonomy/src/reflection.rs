//! LSAutonomy — Self-Reflection Engine
//!
//! Agent 自我反思引擎，分析历史经验并生成洞察和改进建议。

use crate::experience::*;
use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// 洞察类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsightType {
    /// 重复失败模式
    FailurePattern,
    /// 性能退化
    PerformanceDegradation,
    /// 效率提升机会
    EfficiencyOpportunity,
    /// 协作改进
    CollaborationImprovement,
    /// 知识缺口
    KnowledgeGap,
    /// 策略优化
    StrategyOptimization,
    /// 资源配置
    ResourceAllocation,
    /// 行为调整
    BehaviorAdjustment,
}

impl InsightType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InsightType::FailurePattern => "failure_pattern",
            InsightType::PerformanceDegradation => "performance_degradation",
            InsightType::EfficiencyOpportunity => "efficiency_opportunity",
            InsightType::CollaborationImprovement => "collaboration_improvement",
            InsightType::KnowledgeGap => "knowledge_gap",
            InsightType::StrategyOptimization => "strategy_optimization",
            InsightType::ResourceAllocation => "resource_allocation",
            InsightType::BehaviorAdjustment => "behavior_adjustment",
        }
    }
}

/// 反思洞察
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionInsight {
    /// 洞察 ID
    pub id: LsId,
    /// 洞察类型
    pub insight_type: InsightType,
    /// 标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 优先级（1-10）
    pub priority: u8,
    /// 置信度（0.0-1.0）
    pub confidence: f64,
    /// 相关经验 ID 列表
    pub related_experience_ids: Vec<LsId>,
    /// 改进建议
    pub suggestions: Vec<String>,
    /// 受影响的能力
    pub affected_capabilities: Vec<String>,
    /// 创建时间
    pub created_at: i64,
    /// 是否已应用
    pub applied: bool,
}

impl ReflectionInsight {
    pub fn new(
        insight_type: InsightType,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: LsId::new(),
            insight_type,
            title: title.into(),
            description: description.into(),
            priority: 5,
            confidence: 0.5,
            related_experience_ids: Vec::new(),
            suggestions: Vec::new(),
            affected_capabilities: Vec::new(),
            created_at: chrono::Utc::now().timestamp(),
            applied: false,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn add_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    pub fn add_experience(mut self, exp_id: LsId) -> Self {
        self.related_experience_ids.push(exp_id);
        self
    }
}

/// 反思报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionReport {
    /// Agent ID
    pub agent_id: String,
    /// 报告 ID
    pub id: LsId,
    /// 生成时间
    pub generated_at: i64,
    /// 分析的经验数量
    pub analyzed_count: u64,
    /// 发现的洞察
    pub insights: Vec<ReflectionInsight>,
    /// Agent 当前表现摘要
    pub performance_summary: ExperienceSummary,
    /// 总体健康评分（0.0-1.0）
    pub health_score: f64,
    /// 需要立即关注的问题
    pub critical_issues: Vec<String>,
}

/// 反思引擎配置
#[derive(Debug, Clone)]
pub struct ReflectionConfig {
    /// 洞察置信度阈值
    pub confidence_threshold: f64,
    /// 失败模式检测窗口（最近 N 条经验）
    pub failure_pattern_window: usize,
    /// 性能退化检测窗口
    pub degradation_window: usize,
    /// 最低经验数量才触发反思
    pub min_experiences_for_reflection: usize,
    /// 自动标记经验为已分析
    pub auto_mark_analyzed: bool,
}

impl Default for ReflectionConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.3,
            failure_pattern_window: 10,
            degradation_window: 20,
            min_experiences_for_reflection: 5,
            auto_mark_analyzed: true,
        }
    }
}

/// 自我反思引擎
pub struct ReflectionEngine {
    config: ReflectionConfig,
    experience_store: Arc<ExperienceStore>,
    /// 缓存最近的洞察
    recent_insights: Arc<RwLock<Vec<ReflectionInsight>>>,
}

impl ReflectionEngine {
    pub fn new(config: ReflectionConfig, experience_store: Arc<ExperienceStore>) -> Self {
        Self {
            config,
            experience_store,
            recent_insights: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 对指定 Agent 执行反思
    pub async fn reflect(&self, agent_id: &str) -> ReflectionReport {
        let summary = self.experience_store.summarize(agent_id).await;
        let experiences = self.experience_store.get_agent_experiences(agent_id).await;

        let analyzed_count = experiences.len() as u64;
        let mut insights = Vec::new();

        // 检测失败模式
        if let Some(pattern_insight) = self.detect_failure_patterns(agent_id, &experiences).await {
            insights.push(pattern_insight);
        }

        // 检测性能退化
        if let Some(degradation_insight) = self
            .detect_performance_degradation(agent_id, &experiences)
            .await
        {
            insights.push(degradation_insight);
        }

        // 检测效率机会
        if let Some(efficiency_insight) = self
            .detect_efficiency_opportunities(agent_id, &experiences)
            .await
        {
            insights.push(efficiency_insight);
        }

        // 检测协作改进
        if let Some(collab_insight) = self
            .detect_collaboration_improvements(agent_id, &experiences)
            .await
        {
            insights.push(collab_insight);
        }

        // 过滤低置信度洞察
        insights.retain(|i| i.confidence >= self.config.confidence_threshold);

        // 计算健康评分
        let health_score = self.calculate_health_score(&summary, &insights);

        // 找出关键问题
        let critical_issues: Vec<String> = insights
            .iter()
            .filter(|i| i.priority >= 8)
            .map(|i| i.title.clone())
            .collect();

        // 标记已分析
        if self.config.auto_mark_analyzed {
            for exp in &experiences {
                if !exp.analyzed {
                    self.experience_store.mark_analyzed(agent_id, &exp.id).await;
                }
            }
        }

        // 缓存洞察
        {
            let mut cache = self.recent_insights.write().await;
            for insight in &insights {
                cache.push(insight.clone());
            }
            // 保留最近的 100 条
            while cache.len() > 100 {
                cache.remove(0);
            }
        }

        let report = ReflectionReport {
            agent_id: agent_id.to_string(),
            id: LsId::new(),
            generated_at: chrono::Utc::now().timestamp(),
            analyzed_count,
            insights,
            performance_summary: summary,
            health_score,
            critical_issues,
        };

        info!(
            "reflection complete for '{}': {} insights (health: {:.2})",
            agent_id,
            report.insights.len(),
            report.health_score
        );

        report
    }

    /// 检测重复失败模式
    async fn detect_failure_patterns(
        &self,
        agent_id: &str,
        experiences: &[ExperienceEntry],
    ) -> Option<ReflectionInsight> {
        let failures: Vec<&ExperienceEntry> = experiences
            .iter()
            .filter(|e| matches!(e.outcome, ExperienceOutcome::Failure(_)))
            .collect();

        if failures.len() < 2 {
            return None;
        }

        // 统计失败标签
        let mut tag_freq: HashMap<String, u64> = HashMap::new();
        for f in &failures {
            for tag in &f.tags {
                *tag_freq.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        // 找出高频失败标签
        let high_freq: Vec<(String, u64)> = tag_freq
            .into_iter()
            .filter(|(_, count)| *count as f64 >= failures.len() as f64 * 0.3)
            .collect();

        if high_freq.is_empty() {
            return None;
        }

        let total = experiences.len();
        let failure_rate = failures.len() as f64 / total as f64;

        let title = format!(
            "重复失败模式: {} 次失败 (失败率 {:.1}%)",
            failures.len(),
            failure_rate * 100.0
        );

        let tags_desc: Vec<String> = high_freq
            .iter()
            .map(|(t, c)| format!("'{}' ({}次)", t, c))
            .collect();

        let mut insight = ReflectionInsight::new(
            InsightType::FailurePattern,
            title,
            format!(
                "Agent '{}' 在过去 {} 次经验中遇到 {} 次失败，常见失败标签: {}",
                agent_id,
                total,
                failures.len(),
                tags_desc.join(", ")
            ),
        )
        .with_confidence((failure_rate * 0.7 + 0.3).min(1.0))
        .with_priority((failure_rate * 10.0).min(10.0).round() as u8);

        for tag in &high_freq {
            let suggestion = format!("优化与 '{}' 相关的处理逻辑，考虑增加验证和重试机制", tag.0);
            insight = insight.clone().add_suggestion(suggestion);
        }

        for f in failures.iter().take(5) {
            insight = insight.clone().add_experience(f.id);
        }

        Some(insight)
    }

    /// 检测性能退化
    async fn detect_performance_degradation(
        &self,
        agent_id: &str,
        experiences: &[ExperienceEntry],
    ) -> Option<ReflectionInsight> {
        if experiences.len() < self.config.degradation_window {
            return None;
        }

        // 比较前半段和后半段的平均耗时
        let mid = experiences.len() / 2;
        let (first_half, second_half) = experiences.split_at(mid);

        let first_avg: f64 =
            first_half.iter().map(|e| e.duration_ms as f64).sum::<f64>() / first_half.len() as f64;

        let second_avg: f64 = second_half
            .iter()
            .map(|e| e.duration_ms as f64)
            .sum::<f64>()
            / second_half.len() as f64;

        if second_avg > first_avg * 1.5 && first_avg > 0.0 {
            let degradation = (second_avg / first_avg - 1.0) * 100.0;
            let insight = ReflectionInsight::new(
                InsightType::PerformanceDegradation,
                format!("性能退化: 执行耗时增加 {:.0}%", degradation),
                format!(
                    "Agent '{}' 最近 {} 条经验的平均耗时 {:.0}ms，较前期 {:.0}ms 显著增加",
                    agent_id,
                    second_half.len(),
                    second_avg,
                    first_avg
                ),
            )
            .with_confidence(0.8)
            .with_priority(((degradation / 20.0).round() as u8).min(10))
            .add_suggestion("检查近期调用模式变化，是否有新引入的耗时操作")
            .add_suggestion("考虑增加资源配额或优化任务队列")
            .add_suggestion("分析耗时最高的任务类型并针对性优化");

            return Some(insight);
        }

        None
    }

    /// 检测效率提升机会
    async fn detect_efficiency_opportunities(
        &self,
        agent_id: &str,
        experiences: &[ExperienceEntry],
    ) -> Option<ReflectionInsight> {
        if experiences.is_empty() {
            return None;
        }

        // 按类型分组计算成功率和耗时
        let mut type_stats: HashMap<String, (u64, u64, u64)> = HashMap::new();

        for exp in experiences {
            let stats = type_stats
                .entry(exp.exp_type.as_str().to_string())
                .or_insert((0, 0, 0));
            stats.0 += 1;
            stats.1 += exp.duration_ms;
            if exp.outcome.is_success() {
                stats.2 += 1;
            }
        }

        // 找出成功率低或耗时高的类型
        for (exp_type, (count, total_dur, success)) in &type_stats {
            if *count >= 3 {
                let success_rate = *success as f64 / *count as f64;
                let avg_dur = *total_dur as f64 / *count as f64;

                if success_rate < 0.6 {
                    let insight = ReflectionInsight::new(
                        InsightType::EfficiencyOpportunity,
                        format!(
                            "{} 类型效率偏低: 成功率 {:.0}%, 平均耗时 {:.0}ms",
                            exp_type,
                            success_rate * 100.0,
                            avg_dur
                        ),
                        format!(
                            "Agent '{}' 的 '{}' 类型任务成功率仅 {:.0}%，需要优化处理流程",
                            agent_id,
                            exp_type,
                            success_rate * 100.0
                        ),
                    )
                    .with_confidence(0.7)
                    .with_priority(((1.0 - success_rate) * 10.0).round() as u8)
                    .add_suggestion(format!("优化 '{}' 类型任务的执行逻辑", exp_type))
                    .add_suggestion("考虑增加前置验证减少失败");

                    return Some(insight);
                }
            }
        }

        None
    }

    /// 检测协作改进
    async fn detect_collaboration_improvements(
        &self,
        agent_id: &str,
        experiences: &[ExperienceEntry],
    ) -> Option<ReflectionInsight> {
        let collab_exps: Vec<&ExperienceEntry> = experiences
            .iter()
            .filter(|e| e.exp_type == ExperienceType::Collaboration)
            .collect();

        if collab_exps.len() < 3 {
            return None;
        }

        let failures = collab_exps
            .iter()
            .filter(|e| e.outcome.is_failure())
            .count();
        let failure_rate = failures as f64 / collab_exps.len() as f64;

        if failure_rate > 0.3 {
            let insight = ReflectionInsight::new(
                InsightType::CollaborationImprovement,
                format!(
                    "协作效率下降: {}% 的协作经验失败",
                    (failure_rate * 100.0).round()
                ),
                format!(
                    "Agent '{}' 的协作任务失败率 {:.0}%，需要调整协作策略",
                    agent_id,
                    failure_rate * 100.0
                ),
            )
            .with_confidence(0.75)
            .with_priority(((failure_rate * 8.0).round() as u8).min(10))
            .add_suggestion("检查与协作 Agent 的通信协议")
            .add_suggestion("考虑增加协作超时和重试机制")
            .add_suggestion("评估当前协作策略是否适合任务类型");

            return Some(insight);
        }

        None
    }

    /// 计算健康评分
    fn calculate_health_score(
        &self,
        summary: &ExperienceSummary,
        insights: &[ReflectionInsight],
    ) -> f64 {
        let mut score = summary.success_rate * 0.5;

        // 基于洞察数量和优先级扣分
        let penalty: f64 = insights
            .iter()
            .map(|i| i.priority as f64 * 0.05)
            .sum::<f64>()
            .min(0.4);

        score -= penalty;

        // 基于失败数量额外扣分
        if summary.failure_count > 10 {
            score -= 0.1;
        }

        score.clamp(0.0, 1.0)
    }

    /// 获取最近的洞察
    pub async fn get_recent_insights(&self, limit: usize) -> Vec<ReflectionInsight> {
        let cache = self.recent_insights.read().await;
        cache.iter().rev().take(limit).cloned().collect()
    }

    /// 获取 Agent 的洞察历史
    pub async fn get_agent_insights(&self, _agent_id: &str) -> Vec<ReflectionInsight> {
        let cache = self.recent_insights.read().await;
        // Filter by agent_id via related_experience_ids (simplified)
        cache.clone()
    }

    /// 标记洞察为已应用
    pub async fn mark_applied(&self, insight_id: &LsId) {
        let mut cache = self.recent_insights.write().await;
        if let Some(insight) = cache.iter_mut().find(|i| i.id == *insight_id) {
            insight.applied = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_store_with_failures() -> (Arc<ExperienceStore>, String) {
        let store = Arc::new(ExperienceStore::new(100));
        let agent_id = "agent-1";

        // Create 3 successes
        for i in 0..3 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("success-{}", i),
                "ok",
                ExperienceOutcome::Success,
            );
            let _ = store.store_blocking(entry);
        }

        // Create 4 failures with common tags
        for i in 0..4 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("fail-{}", i),
                "error occurred",
                ExperienceOutcome::Failure(format!("timeout {}", i)),
            )
            .with_tag("network-error")
            .with_tag("timeout");
            let _ = store.store_blocking(entry);
        }

        (store, agent_id.to_string())
    }

    // Helper to store synchronously for tests
    trait StoreBlocking {
        fn store_blocking(&self, entry: ExperienceEntry);
    }
    impl StoreBlocking for ExperienceStore {
        fn store_blocking(&self, entry: ExperienceEntry) {
            futures::executor::block_on(self.store(entry));
        }
    }

    #[tokio::test]
    async fn test_detect_failure_patterns() {
        let (store, agent_id) = create_store_with_failures();
        let engine = ReflectionEngine::new(ReflectionConfig::default(), store);
        let experiences = engine
            .experience_store
            .get_agent_experiences(&agent_id)
            .await;

        let insight = engine
            .detect_failure_patterns(&agent_id, &experiences)
            .await;
        assert!(insight.is_some());
        let insight = insight.unwrap();
        assert_eq!(insight.insight_type, InsightType::FailurePattern);
        assert!(insight.confidence >= 0.3);
        assert!(!insight.suggestions.is_empty());
    }

    #[tokio::test]
    async fn test_full_reflection() {
        let (store, agent_id) = create_store_with_failures();
        let engine = ReflectionEngine::new(ReflectionConfig::default(), store);
        let report = engine.reflect(&agent_id).await;

        assert_eq!(report.agent_id, agent_id);
        assert!(!report.insights.is_empty());
        assert!(report.health_score >= 0.0);
        assert!(report.health_score <= 1.0);
    }

    #[tokio::test]
    async fn test_detect_performance_degradation() {
        let store = Arc::new(ExperienceStore::new(100));
        let agent_id = "agent-1";

        // First 10 entries: fast (100ms)
        for i in 0..10 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("fast-{}", i),
                "quick",
                ExperienceOutcome::Success,
            )
            .with_duration(100);
            store.store(entry).await;
        }

        // Last 10 entries: slow (300ms)
        for i in 0..10 {
            let entry = ExperienceEntry::new(
                agent_id,
                ExperienceType::TaskExecution,
                format!("slow-{}", i),
                "slow",
                ExperienceOutcome::Success,
            )
            .with_duration(300);
            store.store(entry).await;
        }

        let engine = ReflectionEngine::new(
            ReflectionConfig {
                degradation_window: 5,
                ..ReflectionConfig::default()
            },
            store,
        );

        let experiences = engine
            .experience_store
            .get_agent_experiences(agent_id)
            .await;
        let insight = engine
            .detect_performance_degradation(agent_id, &experiences)
            .await;
        assert!(insight.is_some());
        assert_eq!(
            insight.unwrap().insight_type,
            InsightType::PerformanceDegradation
        );
    }

    #[test]
    fn test_insight_creation() {
        let insight = ReflectionInsight::new(
            InsightType::StrategyOptimization,
            "Test insight",
            "Description",
        )
        .with_priority(8)
        .with_confidence(0.9)
        .add_suggestion("do something");

        assert_eq!(insight.priority, 8);
        assert!((insight.confidence - 0.9).abs() < 0.01);
        assert_eq!(insight.suggestions.len(), 1);
    }
}
