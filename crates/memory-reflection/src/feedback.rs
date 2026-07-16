//! ReflectionFeedback — 反思反馈记录和存储。
//!
//! 每次 Memory Query 的评估结果可以记录为 ReflectionFeedback，
//! 用于后续分析和优化。
//!
//! # 用途
//!
//! - 追踪每类查询的检索质量趋势
//! - 识别频繁出现冲突的 Workflow
//! - 为 Memory Planner 提供优化依据
//! - 统计不同类型查询的成功率

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::evaluator::ReflectionResult;

// ─── ReflectionFeedback ─────────────────────────────────

/// 反思反馈记录。
///
/// 记录了单次 Memory Query 的评估结果，
/// 可用于后续分析和 Planner 优化。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReflectionFeedback {
    /// 唯一标识
    pub id: String,
    /// 原始查询语句
    pub query: String,
    /// 使用的路由
    pub route: String,
    /// 使用的工作流名称
    pub workflow: String,
    /// 找到的证据数量
    pub evidence_count: usize,
    /// 一致性评分 (0.0 ~ 1.0)
    pub consistency_score: f64,
    /// 完整性评分 (0.0 ~ 1.0)
    pub completeness_score: f64,
    /// 综合置信度 (0.0 ~ 1.0)
    pub confidence: f64,
    /// 是否存在冲突
    pub has_conflicts: bool,
    /// 冲突数量
    pub conflict_count: usize,
    /// 查询耗时 (ms)
    pub latency_ms: u64,
    /// 用户是否接受了结果
    pub user_accepted: Option<bool>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 额外属性
    pub attributes: HashMap<String, String>,
}

impl ReflectionFeedback {
    /// 从 ReflectionResult 创建反馈记录。
    pub fn from_result(result: &ReflectionResult, workflow: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            query: result.query.clone(),
            route: result.route_used.clone(),
            workflow: workflow.to_string(),
            evidence_count: result.evidence_count,
            consistency_score: result.consistency_score,
            completeness_score: result.completeness_score,
            confidence: result.confidence,
            has_conflicts: result.has_conflicts,
            conflict_count: result.conflicts.len(),
            latency_ms: result.latency_ms,
            user_accepted: None,
            created_at: Utc::now(),
            attributes: HashMap::new(),
        }
    }

    /// 手动创建反馈记录。
    pub fn new(
        query: impl Into<String>,
        route: impl Into<String>,
        workflow: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            query: query.into(),
            route: route.into(),
            workflow: workflow.into(),
            evidence_count: 0,
            consistency_score: 0.0,
            completeness_score: 0.0,
            confidence: 0.0,
            has_conflicts: false,
            conflict_count: 0,
            latency_ms: 0,
            user_accepted: None,
            created_at: Utc::now(),
            attributes: HashMap::new(),
        }
    }

    /// 设置用户是否接受。
    pub fn with_user_accepted(mut self, accepted: bool) -> Self {
        self.user_accepted = Some(accepted);
        self
    }

    /// 添加额外属性。
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// 更新为来自 ReflectionResult。
    pub fn update_from_result(&mut self, result: &ReflectionResult) {
        self.evidence_count = result.evidence_count;
        self.consistency_score = result.consistency_score;
        self.completeness_score = result.completeness_score;
        self.confidence = result.confidence;
        self.has_conflicts = result.has_conflicts;
        self.conflict_count = result.conflicts.len();
        self.latency_ms = result.latency_ms;
    }
}

// ─── FeedbackStore trait ───────────────────────────────

/// 反馈存储抽象。
#[async_trait::async_trait]
pub trait FeedbackStore: Send + Sync {
    /// 存储一条反馈记录。
    async fn store(&self, feedback: ReflectionFeedback) -> Result<(), ReflectionError>;

    /// 查询所有反馈记录。
    async fn list(&self, limit: usize, offset: usize) -> Result<Vec<ReflectionFeedback>, ReflectionError>;

    /// 按路由查询反馈记录。
    async fn list_by_route(&self, route: &str, limit: usize, offset: usize)
        -> Result<Vec<ReflectionFeedback>, ReflectionError>;

    /// 按工作流查询反馈记录。
    async fn list_by_workflow(&self, workflow: &str, limit: usize, offset: usize)
        -> Result<Vec<ReflectionFeedback>, ReflectionError>;

    /// 查询有冲突的记录。
    async fn list_conflicts(&self, limit: usize, offset: usize)
        -> Result<Vec<ReflectionFeedback>, ReflectionError>;

    /// 获取反馈总数。
    async fn count(&self) -> Result<usize, ReflectionError>;

    /// 清除所有反馈。
    async fn clear(&self) -> Result<(), ReflectionError>;
}

// ─── InMemoryFeedbackStore ─────────────────────────────

/// 内存反馈存储（用于测试）。
#[derive(Debug, Clone)]
pub struct InMemoryFeedbackStore {
    feedbacks: Arc<RwLock<Vec<ReflectionFeedback>>>,
}

impl InMemoryFeedbackStore {
    pub fn new() -> Self {
        Self {
            feedbacks: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for InMemoryFeedbackStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl FeedbackStore for InMemoryFeedbackStore {
    async fn store(&self, feedback: ReflectionFeedback) -> Result<(), ReflectionError> {
        let mut store = self.feedbacks.write().await;
        store.push(feedback);
        Ok(())
    }

    async fn list(&self, limit: usize, offset: usize) -> Result<Vec<ReflectionFeedback>, ReflectionError> {
        let store = self.feedbacks.read().await;
        let items: Vec<_> = store.iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(items)
    }

    async fn list_by_route(&self, route: &str, limit: usize, offset: usize)
        -> Result<Vec<ReflectionFeedback>, ReflectionError>
    {
        let store = self.feedbacks.read().await;
        let items: Vec<_> = store.iter()
            .filter(|f| f.route == route)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(items)
    }

    async fn list_by_workflow(&self, workflow: &str, limit: usize, offset: usize)
        -> Result<Vec<ReflectionFeedback>, ReflectionError>
    {
        let store = self.feedbacks.read().await;
        let items: Vec<_> = store.iter()
            .filter(|f| f.workflow == workflow)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(items)
    }

    async fn list_conflicts(&self, limit: usize, offset: usize)
        -> Result<Vec<ReflectionFeedback>, ReflectionError>
    {
        let store = self.feedbacks.read().await;
        let items: Vec<_> = store.iter()
            .filter(|f| f.has_conflicts)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(items)
    }

    async fn count(&self) -> Result<usize, ReflectionError> {
        let store = self.feedbacks.read().await;
        Ok(store.len())
    }

    async fn clear(&self) -> Result<(), ReflectionError> {
        let mut store = self.feedbacks.write().await;
        store.clear();
        Ok(())
    }
}

// ─── FeedbackAnalytics ─────────────────────────────────

/// 反馈分析器 — 对 FeedbackStore 中的数据做统计分析。
pub struct FeedbackAnalytics;

impl FeedbackAnalytics {
    /// 计算指定路由的平均置信度。
    pub async fn avg_confidence_by_route(
        store: &dyn FeedbackStore,
        route: &str,
    ) -> Result<f64, ReflectionError> {
        let all = store.list_by_route(route, 1000, 0).await?;
        if all.is_empty() {
            return Ok(0.0);
        }
        let sum: f64 = all.iter().map(|f| f.confidence).sum();
        Ok(sum / all.len() as f64)
    }

    /// 计算指定工作流的平均一致性评分。
    pub async fn avg_consistency_by_workflow(
        store: &dyn FeedbackStore,
        workflow: &str,
    ) -> Result<f64, ReflectionError> {
        let all = store.list_by_workflow(workflow, 1000, 0).await?;
        if all.is_empty() {
            return Ok(0.0);
        }
        let sum: f64 = all.iter().map(|f| f.consistency_score).sum();
        Ok(sum / all.len() as f64)
    }

    /// 计算冲突率（有冲突的查询占比）。
    pub async fn conflict_rate(
        store: &dyn FeedbackStore,
    ) -> Result<f64, ReflectionError> {
        let total = store.count().await?;
        if total == 0 {
            return Ok(0.0);
        }
        let conflicts = store.list_conflicts(1000, 0).await?;
        Ok(conflicts.len() as f64 / total as f64)
    }

    /// 获取所有使用过的路由列表及其统计。
    pub async fn route_stats(
        store: &dyn FeedbackStore,
    ) -> Result<Vec<RouteStats>, ReflectionError> {
        let all = store.list(1000, 0).await?;
        let mut route_map: HashMap<String, Vec<&ReflectionFeedback>> = HashMap::new();

        for fb in &all {
            route_map.entry(fb.route.clone()).or_default().push(fb);
        }

        let mut stats: Vec<RouteStats> = route_map.into_iter()
            .map(|(route, items)| {
                let avg_conf = items.iter().map(|f| f.confidence).sum::<f64>() / items.len() as f64;
                let conflict_count = items.iter().filter(|f| f.has_conflicts).count();
                RouteStats {
                    route,
                    total_queries: items.len(),
                    avg_confidence: avg_conf,
                    conflict_count,
                    conflict_rate: conflict_count as f64 / items.len() as f64,
                }
            })
            .collect();

        stats.sort_by(|a, b| b.total_queries.cmp(&a.total_queries));
        Ok(stats)
    }
}

/// 路由统计。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouteStats {
    pub route: String,
    pub total_queries: usize,
    pub avg_confidence: f64,
    pub conflict_count: usize,
    pub conflict_rate: f64,
}

// ─── Error ─────────────────────────────────────────────

/// 反思模块错误。
#[derive(Debug, thiserror::Error)]
pub enum ReflectionError {
    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("查询错误: {0}")]
    QueryError(String),

    #[error("分析错误: {0}")]
    AnalysisError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluator::ReflectionResult;

    fn make_sample_result(query: &str, route: &str, evidence: usize, confidence: f64) -> ReflectionResult {
        let _graph = lingshu_evidence_graph::EvidenceGraph::empty(query);
        ReflectionResult {
            query: query.to_string(),
            route_used: route.to_string(),
            evidence_count: evidence,
            consistency_score: 0.9,
            completeness_score: 0.7,
            has_conflicts: false,
            conflicts: vec![],
            confidence,
            gaps: vec![],
            suggestions: vec!["良好".to_string()],
            latency_ms: 10,
            evaluated_at: Utc::now(),
        }
    }

    fn make_conflict_result(query: &str, route: &str) -> ReflectionResult {
        use crate::evaluator::ConflictInfo;
        ReflectionResult {
            query: query.to_string(),
            route_used: route.to_string(),
            evidence_count: 2,
            consistency_score: 0.3,
            completeness_score: 0.4,
            has_conflicts: true,
            conflicts: vec![ConflictInfo {
                conflict_type: crate::evaluator::ConflictType::TemporalConflict,
                description: "测试冲突".to_string(),
                node_ids: vec![],
                severity: 0.7,
            }],
            confidence: 0.25,
            gaps: vec!["时间线断裂".to_string()],
            suggestions: vec!["修复时间线".to_string()],
            latency_ms: 15,
            evaluated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_store_and_list() {
        let store = InMemoryFeedbackStore::new();
        let fb = ReflectionFeedback::new("测试查询", "episode", "timeline");
        store.store(fb).await.unwrap();

        let items = store.list(10, 0).await.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn test_store_from_result() {
        let store = InMemoryFeedbackStore::new();
        let result = make_sample_result("项目A状态", "episode", 3, 0.8);
        let fb = ReflectionFeedback::from_result(&result, "timeline");
        store.store(fb).await.unwrap();

        let items = store.list(10, 0).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].workflow, "timeline");
        assert_eq!(items[0].evidence_count, 3);
    }

    #[tokio::test]
    async fn test_list_by_route() {
        let store = InMemoryFeedbackStore::new();
        store.store(ReflectionFeedback::new("q1", "episode", "w1")).await.unwrap();
        store.store(ReflectionFeedback::new("q2", "semantic", "w2")).await.unwrap();
        store.store(ReflectionFeedback::new("q3", "episode", "w1")).await.unwrap();

        let episode_items = store.list_by_route("episode", 10, 0).await.unwrap();
        assert_eq!(episode_items.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_workflow() {
        let store = InMemoryFeedbackStore::new();
        store.store(ReflectionFeedback::new("q1", "episode", "timeline")).await.unwrap();
        store.store(ReflectionFeedback::new("q2", "semantic", "semantic")).await.unwrap();
        store.store(ReflectionFeedback::new("q3", "episode", "timeline")).await.unwrap();

        let items = store.list_by_workflow("timeline", 10, 0).await.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_list_conflicts() {
        let store = InMemoryFeedbackStore::new();

        let mut clean = ReflectionFeedback::new("q1", "episode", "w1");
        clean.has_conflicts = false;
        store.store(clean).await.unwrap();

        let mut conflict = ReflectionFeedback::new("q2", "episode", "w1");
        conflict.has_conflicts = true;
        store.store(conflict).await.unwrap();

        let conflicts = store.list_conflicts(10, 0).await.unwrap();
        assert_eq!(conflicts.len(), 1);
    }

    #[tokio::test]
    async fn test_count_and_clear() {
        let store = InMemoryFeedbackStore::new();
        assert_eq!(store.count().await.unwrap(), 0);

        store.store(ReflectionFeedback::new("q1", "r1", "w1")).await.unwrap();
        store.store(ReflectionFeedback::new("q2", "r1", "w1")).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);

        store.clear().await.unwrap();
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_pagination() {
        let store = InMemoryFeedbackStore::new();
        for i in 0..10 {
            store.store(
                ReflectionFeedback::new(format!("q{}", i), "episode", "w1")
            ).await.unwrap();
        }

        let page1 = store.list(3, 0).await.unwrap();
        assert_eq!(page1.len(), 3);

        let page2 = store.list(3, 3).await.unwrap();
        assert_eq!(page2.len(), 3);

        let page3 = store.list(3, 9).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_feedback_update_from_result() {
        let mut fb = ReflectionFeedback::new("测试", "episode", "timeline");
        let result = make_sample_result("测试", "episode", 5, 0.9);
        fb.update_from_result(&result);

        assert_eq!(fb.evidence_count, 5);
        assert!((fb.confidence - 0.9).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_user_accepted() {
        let fb = ReflectionFeedback::new("q1", "r1", "w1")
            .with_user_accepted(true);
        assert_eq!(fb.user_accepted, Some(true));
    }

    #[tokio::test]
    async fn test_from_result_with_conflict() {
        let result = make_conflict_result("冲突查询", "episode");
        let fb = ReflectionFeedback::from_result(&result, "timeline");

        assert!(fb.has_conflicts);
        assert_eq!(fb.conflict_count, 1);
    }

    #[tokio::test]
    async fn test_analytics_avg_confidence() {
        let store = InMemoryFeedbackStore::new();
        store.store(
            ReflectionFeedback::from_result(&make_sample_result("q1", "episode", 3, 0.8), "w1")
        ).await.unwrap();
        store.store(
            ReflectionFeedback::from_result(&make_sample_result("q2", "episode", 5, 0.9), "w1")
        ).await.unwrap();
        store.store(
            ReflectionFeedback::from_result(&make_sample_result("q3", "semantic", 2, 0.6), "w2")
        ).await.unwrap();

        let avg = FeedbackAnalytics::avg_confidence_by_route(&store, "episode").await.unwrap();
        assert!((avg - 0.85).abs() < 0.01, "expected 0.85, got {:.2}", avg);
    }

    #[tokio::test]
    async fn test_analytics_avg_consistency() {
        let store = InMemoryFeedbackStore::new();
        store.store(
            ReflectionFeedback::from_result(&make_sample_result("q1", "episode", 3, 0.8), "timeline")
        ).await.unwrap();

        let avg = FeedbackAnalytics::avg_consistency_by_workflow(&store, "timeline").await.unwrap();
        assert!((avg - 0.9).abs() < 0.01, "expected 0.9, got {:.2}", avg);
    }

    #[tokio::test]
    async fn test_analytics_conflict_rate() {
        let store = InMemoryFeedbackStore::new();

        let result1 = make_sample_result("q1", "episode", 3, 0.8);
        store.store(ReflectionFeedback::from_result(&result1, "w1")).await.unwrap();

        let result2 = make_conflict_result("q2", "episode");
        store.store(ReflectionFeedback::from_result(&result2, "w1")).await.unwrap();

        let rate = FeedbackAnalytics::conflict_rate(&store).await.unwrap();
        assert!((rate - 0.5).abs() < 0.01, "expected 0.5, got {:.2}", rate);
    }

    #[tokio::test]
    async fn test_route_stats() {
        let store = InMemoryFeedbackStore::new();
        store.store(
            ReflectionFeedback::from_result(&make_sample_result("q1", "episode", 3, 0.8), "w1")
        ).await.unwrap();
        store.store(
            ReflectionFeedback::from_result(&make_sample_result("q2", "episode", 5, 0.9), "w1")
        ).await.unwrap();
        store.store(
            ReflectionFeedback::from_result(&make_conflict_result("q3", "semantic"), "w2")
        ).await.unwrap();

        let stats = FeedbackAnalytics::route_stats(&store).await.unwrap();
        assert!(!stats.is_empty());
        let episode_stats = stats.iter().find(|s| s.route == "episode").unwrap();
        assert!(episode_stats.conflict_rate < 0.1, "episode should have low conflict rate");
    }
}
