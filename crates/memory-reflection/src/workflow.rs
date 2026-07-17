//! ReflectionWorkflow — 记忆反思工作流。
//!
//! 将记忆反思能力暴露为 MemoryWorkflow，使 Agent 可以通过
//! 统一接口调用反思功能。输入是历史 Memory Query 记录，
//! 输出是包含反思报告的 EvidenceGraph。

use async_trait::async_trait;
use chrono::Utc;
use lingshu_core::LsResult;
use lingshu_evidence_graph::{EvidenceGraph, Node, Edge, NodeId};
use lingshu_memory_workflow::{MemoryQuery, MemoryWorkflow};
use std::sync::Arc;

use crate::evaluator::ReflectionEvaluator;
use crate::feedback::{FeedbackAnalytics, FeedbackStore, InMemoryFeedbackStore, ReflectionFeedback};

/// 反思工作流模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReflectionMode {
    Recent(usize),
    RouteStats,
    Conflicts(usize),
    Improve,
}

impl std::fmt::Display for ReflectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Recent(n) => write!(f, "recent({})", n),
            Self::RouteStats => write!(f, "route_stats"),
            Self::Conflicts(n) => write!(f, "conflicts({})", n),
            Self::Improve => write!(f, "improve"),
        }
    }
}

/// ReflectionWorkflow — 记忆反思工作流。
pub struct ReflectionWorkflow {
    evaluator: ReflectionEvaluator,
    feedback_store: Option<Arc<dyn FeedbackStore>>,
    default_store: Arc<InMemoryFeedbackStore>,
    max_results: usize,
}

impl ReflectionWorkflow {
    pub fn new() -> Self {
        Self {
            evaluator: ReflectionEvaluator::new(),
            feedback_store: None,
            default_store: Arc::new(InMemoryFeedbackStore::new()),
            max_results: 20,
        }
    }

    pub fn with_feedback_store(mut self, store: Arc<dyn FeedbackStore>) -> Self {
        self.feedback_store = Some(store);
        self
    }

    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    pub fn with_evaluator(mut self, evaluator: ReflectionEvaluator) -> Self {
        self.evaluator = evaluator;
        self
    }

    fn store(&self) -> &dyn FeedbackStore {
        self.feedback_store
            .as_ref()
            .map(|s| s.as_ref())
            .unwrap_or_else(|| self.default_store.as_ref())
    }

    fn parse_mode(&self, question: &str) -> ReflectionMode {
        let q = question.trim().to_lowercase();

        if q.starts_with("recent:") {
            let n: usize = q.trim_start_matches("recent:").trim().parse().unwrap_or(10);
            return ReflectionMode::Recent(n.min(self.max_results));
        }
        if q == "route_stats" || q == "route stats" || q == "路由统计" {
            return ReflectionMode::RouteStats;
        }
        if q.starts_with("conflicts:") {
            let n: usize = q.trim_start_matches("conflicts:").trim().parse().unwrap_or(5);
            return ReflectionMode::Conflicts(n.min(self.max_results));
        }
        if q == "improve" || q == "优化建议" {
            return ReflectionMode::Improve;
        }

        ReflectionMode::Recent(10)
    }

    async fn execute_internal(&self, query: &MemoryQuery) -> LsResult<EvidenceGraph> {
        let start = std::time::Instant::now();
        let mode = self.parse_mode(&query.question);

        let mut graph = EvidenceGraph::empty(&query.question);
        graph.metadata.source = "reflection_workflow".into();
        graph.metadata.build_time_ms = start.elapsed().as_millis() as u64;

        match mode {
            ReflectionMode::Recent(n) => {
                self.build_recent_analysis(&mut graph, n).await?;
            }
            ReflectionMode::RouteStats => {
                self.build_route_stats(&mut graph).await?;
            }
            ReflectionMode::Conflicts(n) => {
                self.build_conflict_report(&mut graph, n).await?;
            }
            ReflectionMode::Improve => {
                self.build_improvement_report(&mut graph).await?;
            }
        }

        graph.metadata.build_time_ms = start.elapsed().as_millis() as u64;
        graph.metadata.node_count = graph.nodes.len();
        graph.metadata.edge_count = graph.edges.len();

        Ok(graph)
    }

    async fn build_recent_analysis(&self, graph: &mut EvidenceGraph, n: usize) -> LsResult<()> {
        let items = self.store().list(n, 0).await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        if items.is_empty() {
            let mut node = Node::event(
                "反思报告：最近查询分析",
                "暂无反馈记录。请至少执行一次 Memory Query 后再运行反思。",
                Utc::now(),
            );
            node = node.with_tag("reflection:empty");
            graph.add_node(node);
            return Ok(());
        }

        let avg_confidence: f64 = items.iter().map(|f| f.confidence).sum::<f64>() / items.len() as f64;
        let conflict_count = items.iter().filter(|f| f.has_conflicts).count();
        let overview = format!(
            "最近 {} 次查询分析：平均置信度 {:.2}，冲突率 {:.0}% ({}/{} 次有冲突)",
            items.len(),
            avg_confidence,
            (conflict_count as f64 / items.len() as f64) * 100.0,
            conflict_count,
            items.len(),
        );

        let mut overview_node = Node::event(
            format!("反思报告：最近 {} 次查询分析", items.len()),
            &overview,
            Utc::now(),
        );
        overview_node = overview_node.with_tag("reflection:overview");
        overview_node = overview_node.with_tag(format!("total_queries:{}", items.len()));
        overview_node = overview_node.with_tag(format!("avg_confidence:{:.3}", avg_confidence));
        overview_node = overview_node.with_tag(format!("conflict_count:{}", conflict_count));
        graph.add_node(overview_node);

        let mut prev_id: Option<NodeId> = None;
        for item in &items {
            let detail = format!(
                "路由: {}, 工作流: {}, 证据: {}, 置信度: {:.2}, 冲突: {}, 耗时: {}ms",
                item.route, item.workflow, item.evidence_count,
                item.confidence, item.has_conflicts, item.latency_ms,
            );

            let mut node = Node::event(&item.query, &detail, item.created_at);
            node = node.with_tag(format!("route:{}", item.route));
            node = node.with_tag(format!("workflow:{}", item.workflow));
            node = node.with_tag(format!("confidence:{:.2}", item.confidence));
            if item.has_conflicts {
                node = node.with_tag("reflection:conflict");
            }

            let node_id = node.id;
            graph.add_node(node);

            if let Some(prev) = prev_id {
                graph.add_edge(Edge::temporal(prev, node_id));
            }
            prev_id = Some(node_id);
        }

        Ok(())
    }

    async fn build_route_stats(&self, graph: &mut EvidenceGraph) -> LsResult<()> {
        let stats = FeedbackAnalytics::route_stats(self.store()).await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        if stats.is_empty() {
            let mut node = Node::event("反思报告：路由统计", "暂无反馈记录。", Utc::now());
            node = node.with_tag("reflection:empty");
            graph.add_node(node);
            return Ok(());
        }

        for stat in &stats {
            let summary = format!(
                "总查询: {}, 平均置信度: {:.3}, 冲突: {} ({:.1}%)",
                stat.total_queries,
                stat.avg_confidence,
                stat.conflict_count,
                stat.conflict_rate * 100.0,
            );

            let mut node = Node::event(format!("路由: {}", stat.route), &summary, Utc::now());
            node = node.with_tag("reflection:route_stats");
            node = node.with_tag(format!("route:{}", stat.route));
            node = node.with_tag(format!("total_queries:{}", stat.total_queries));
            node = node.with_tag(format!("avg_confidence:{:.3}", stat.avg_confidence));
            node = node.with_tag(format!("conflict_rate:{:.3}", stat.conflict_rate));
            graph.add_node(node);
        }

        Ok(())
    }

    async fn build_conflict_report(&self, graph: &mut EvidenceGraph, n: usize) -> LsResult<()> {
        let conflicts = self.store().list_conflicts(n, 0).await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        if conflicts.is_empty() {
            let mut node = Node::event(
                "反思报告：冲突检测",
                "未检测到任何冲突。所有查询的一致性良好。",
                Utc::now(),
            );
            node = node.with_tag("reflection:no_conflicts");
            graph.add_node(node);
            return Ok(());
        }

        let header = format!("检测到 {} 条冲突记录", conflicts.len());
        let mut header_node = Node::event("反思报告：冲突检测", &header, Utc::now());
        header_node = header_node.with_tag("reflection:conflict_header");
        header_node = header_node.with_tag(format!("conflict_count:{}", conflicts.len()));
        graph.add_node(header_node);

        for conflict in &conflicts {
            let detail = format!(
                "查询: '{}', 路由: {}, 工作流: {}, 一致性: {:.2}, 证据数: {}",
                conflict.query, conflict.route, conflict.workflow,
                conflict.consistency_score, conflict.evidence_count,
            );

            let mut node = Node::event(&conflict.query, &detail, conflict.created_at);
            node = node.with_tag("reflection:conflict");
            node = node.with_tag(format!("route:{}", conflict.route));
            node = node.with_tag(format!("workflow:{}", conflict.workflow));
            node = node.with_tag(format!("consistency:{:.3}", conflict.consistency_score));
            graph.add_node(node);
        }

        Ok(())
    }

    async fn build_improvement_report(&self, graph: &mut EvidenceGraph) -> LsResult<()> {
        let items = self.store().list(self.max_results, 0).await
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;

        if items.is_empty() {
            let mut node = Node::event(
                "反思报告：优化建议",
                "暂无足够数据生成建议。请先运行一些 Memory Query。",
                Utc::now(),
            );
            node = node.with_tag("reflection:empty");
            graph.add_node(node);
            return Ok(());
        }

        let suggestions = self.generate_improvement_suggestions(&items);
        let header = format!(
            "基于 {} 次查询记录，生成 {} 条优化建议",
            items.len(),
            suggestions.len(),
        );
        let mut header_node = Node::event("反思报告：优化建议", &header, Utc::now());
        header_node = header_node.with_tag("reflection:suggestions_header");
        graph.add_node(header_node);

        for (i, suggestion) in suggestions.iter().enumerate() {
            let mut node = Node::event(format!("建议 #{}", i + 1), suggestion, Utc::now());
            node = node.with_tag("reflection:suggestion");
            graph.add_node(node);
        }

        Ok(())
    }

    fn generate_improvement_suggestions(&self, items: &[ReflectionFeedback]) -> Vec<String> {
        let mut suggestions = Vec::new();

        if items.is_empty() {
            suggestions.push("暂无查询记录，无需优化".to_string());
            return suggestions;
        }

        let high_latency: Vec<_> = items.iter()
            .filter(|f| f.latency_ms > 1000)
            .collect();
        if !high_latency.is_empty() {
            suggestions.push(format!(
                "检测到 {} 次查询耗时超过 1 秒（最高 {}ms），\
                 建议检查 Workflow 执行效率或索引性能",
                high_latency.len(),
                high_latency.iter().map(|f| f.latency_ms).max().unwrap_or(0),
            ));
        }

        let route_conflicts: Vec<_> = items.iter()
            .filter(|f| f.has_conflicts)
            .collect();
        if !route_conflicts.is_empty() {
            let mut route_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for fb in &route_conflicts {
                *route_counts.entry(fb.route.as_str()).or_insert(0) += 1;
            }

            for (route, count) in &route_counts {
                let total: usize = items.iter().filter(|f| f.route == *route).count();
                suggestions.push(format!(
                    "路由 '{}' 的冲突率 {}/{} ({:.0}%)，\
                     建议检查该路由的时间线一致性或状态变更逻辑",
                    route, count, total, (*count as f64 / total as f64) * 100.0,
                ));
            }
        }

        let workflow_confidence: std::collections::HashMap<&str, Vec<f64>> = items.iter()
            .fold(std::collections::HashMap::new(), |mut acc, f| {
                acc.entry(f.workflow.as_str()).or_default().push(f.confidence);
                acc
            });

        for (workflow, confidences) in &workflow_confidence {
            let avg: f64 = confidences.iter().sum::<f64>() / confidences.len() as f64;
            if avg < 0.4 {
                suggestions.push(format!(
                    "工作流 '{}' 平均置信度仅 {:.2}，建议优化检索策略",
                    workflow, avg,
                ));
            }
        }

        let empty_results: Vec<_> = items.iter()
            .filter(|f| f.evidence_count == 0)
            .collect();
        if !empty_results.is_empty() {
            suggestions.push(format!(
                "{} 次查询未找到任何相关记忆，建议扩展数据源或调整路由规则",
                empty_results.len(),
            ));
        }

        if suggestions.is_empty() {
            suggestions.push("所有指标正常，无需优化".to_string());
        }

        suggestions
    }
}

impl Default for ReflectionWorkflow {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryWorkflow for ReflectionWorkflow {
    fn name(&self) -> &str {
        "reflection"
    }

    async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
        self.execute_internal(&query).await
    }
}

pub fn create_reflection_workflow() -> ReflectionWorkflow {
    ReflectionWorkflow::new()
}

pub fn create_reflection_workflow_with_store(
    store: Arc<dyn FeedbackStore>,
) -> ReflectionWorkflow {
    ReflectionWorkflow::new().with_feedback_store(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn add_sample_feedback(store: &InMemoryFeedbackStore) {
        let fb1 = ReflectionFeedback {
            id: "1".to_string(),
            query: "项目A状态".to_string(),
            route: "episode".to_string(),
            workflow: "timeline".to_string(),
            evidence_count: 5,
            consistency_score: 0.9,
            completeness_score: 0.8,
            confidence: 0.85,
            has_conflicts: false,
            conflict_count: 0,
            latency_ms: 10,
            user_accepted: Some(true),
            created_at: Utc::now() - chrono::Duration::hours(1),
            attributes: Default::default(),
        };
        let fb2 = ReflectionFeedback {
            id: "2".to_string(),
            query: "RAG是什么".to_string(),
            route: "semantic".to_string(),
            workflow: "semantic".to_string(),
            evidence_count: 3,
            consistency_score: 1.0,
            completeness_score: 0.6,
            confidence: 0.75,
            has_conflicts: false,
            conflict_count: 0,
            latency_ms: 5,
            user_accepted: Some(true),
            created_at: Utc::now() - chrono::Duration::hours(2),
            attributes: Default::default(),
        };
        let fb3 = ReflectionFeedback {
            id: "3".to_string(),
            query: "项目A为什么暂停".to_string(),
            route: "episode".to_string(),
            workflow: "timeline".to_string(),
            evidence_count: 2,
            consistency_score: 0.3,
            completeness_score: 0.4,
            confidence: 0.25,
            has_conflicts: true,
            conflict_count: 2,
            latency_ms: 2000,
            user_accepted: None,
            created_at: Utc::now() - chrono::Duration::hours(3),
            attributes: Default::default(),
        };

        store.store(fb1).await.unwrap();
        store.store(fb2).await.unwrap();
        store.store(fb3).await.unwrap();
    }

    #[test]
    fn test_workflow_name() {
        let workflow = ReflectionWorkflow::new();
        assert_eq!(workflow.name(), "reflection");
    }

    #[tokio::test]
    async fn test_workflow_empty() {
        let workflow = ReflectionWorkflow::new();
        let query = MemoryQuery::new("recent:5");
        let result = workflow.execute(query).await.unwrap();

        assert!(!result.nodes.is_empty(), "should return empty report node");
        assert_eq!(result.metadata.source, "reflection_workflow");
    }

    #[tokio::test]
    async fn test_workflow_recent() {
        let store = Arc::new(InMemoryFeedbackStore::new());
        add_sample_feedback(&store).await;
        let workflow = ReflectionWorkflow::new()
            .with_feedback_store(store);

        let result = workflow.execute(MemoryQuery::new("recent:10")).await.unwrap();
        assert!(result.nodes.len() >= 3, "should have overview + 3 items, got {}", result.nodes.len());
    }

    #[tokio::test]
    async fn test_workflow_route_stats() {
        let store = Arc::new(InMemoryFeedbackStore::new());
        add_sample_feedback(&store).await;
        let workflow = ReflectionWorkflow::new()
            .with_feedback_store(store);

        let result = workflow.execute(MemoryQuery::new("route_stats")).await.unwrap();
        assert!(!result.nodes.is_empty(), "should have route stats");
    }

    #[tokio::test]
    async fn test_workflow_conflicts() {
        let store = Arc::new(InMemoryFeedbackStore::new());
        add_sample_feedback(&store).await;
        let workflow = ReflectionWorkflow::new()
            .with_feedback_store(store);

        let result = workflow.execute(MemoryQuery::new("conflicts:5")).await.unwrap();
        assert!(!result.nodes.is_empty(), "should have conflict report");
    }

    #[tokio::test]
    async fn test_workflow_improve() {
        let store = Arc::new(InMemoryFeedbackStore::new());
        add_sample_feedback(&store).await;
        let workflow = ReflectionWorkflow::new()
            .with_feedback_store(store);

        let result = workflow.execute(MemoryQuery::new("improve")).await.unwrap();
        assert!(!result.nodes.is_empty(), "should have improvement suggestions");
    }

    #[tokio::test]
    async fn test_workflow_chinese_mode() {
        let workflow = ReflectionWorkflow::new();
        let result = workflow.execute(MemoryQuery::new("路由统计")).await.unwrap();
        assert_eq!(result.metadata.source, "reflection_workflow");

        let result = workflow.execute(MemoryQuery::new("优化建议")).await.unwrap();
        assert_eq!(result.metadata.source, "reflection_workflow");
    }

    #[tokio::test]
    async fn test_parse_modes() {
        let workflow = ReflectionWorkflow::new();

        assert_eq!(workflow.parse_mode("recent:5"), ReflectionMode::Recent(5));
        assert_eq!(workflow.parse_mode("route_stats"), ReflectionMode::RouteStats);
        assert_eq!(workflow.parse_mode("conflicts:3"), ReflectionMode::Conflicts(3));
        assert_eq!(workflow.parse_mode("improve"), ReflectionMode::Improve);
        assert_eq!(workflow.parse_mode("未知内容"), ReflectionMode::Recent(10));
    }

    #[tokio::test]
    async fn test_workflow_with_store() {
        let store = Arc::new(InMemoryFeedbackStore::new()) as Arc<dyn FeedbackStore>;
        let workflow = create_reflection_workflow_with_store(store);
        assert_eq!(workflow.name(), "reflection");
    }

    #[tokio::test]
    async fn test_workflow_no_conflicts() {
        let store = Arc::new(InMemoryFeedbackStore::new());
        for i in 0..3 {
            let fb = ReflectionFeedback::new(format!("查询{}", i), "episode", "timeline");
            store.store(fb).await.unwrap();
        }

        let store_arc = store as Arc<dyn FeedbackStore>;
        let workflow = ReflectionWorkflow::new()
            .with_feedback_store(store_arc);

        let result = workflow.execute(MemoryQuery::new("conflicts:5")).await.unwrap();
        assert!(!result.nodes.is_empty(), "should have 'no conflicts' report");
        let has_no_conflicts = result.nodes.iter().any(|n|
            n.tags.contains(&"reflection:no_conflicts".to_string())
        );
        assert!(has_no_conflicts, "should indicate no conflicts");
    }

    #[tokio::test]
    async fn test_improvement_suggestions_generation() {
        let workflow = ReflectionWorkflow::new();

        let suggestions = workflow.generate_improvement_suggestions(&[]);
        assert!(!suggestions.is_empty());

        let items = vec![ReflectionFeedback {
            id: "1".to_string(),
            query: "慢查询".to_string(),
            route: "deep".to_string(),
            workflow: "complex".to_string(),
            evidence_count: 5,
            consistency_score: 0.9,
            completeness_score: 0.8,
            confidence: 0.85,
            has_conflicts: false,
            conflict_count: 0,
            latency_ms: 5000,
            user_accepted: None,
            created_at: Utc::now(),
            attributes: Default::default(),
        }];
        let suggestions = workflow.generate_improvement_suggestions(&items);
        let has_latency = suggestions.iter().any(|s| s.contains("耗时"));
        assert!(has_latency, "should have latency suggestion");
    }

    #[test]
    fn test_display_mode() {
        assert_eq!(format!("{}", ReflectionMode::Recent(5)), "recent(5)");
        assert_eq!(format!("{}", ReflectionMode::RouteStats), "route_stats");
        assert_eq!(format!("{}", ReflectionMode::Conflicts(3)), "conflicts(3)");
        assert_eq!(format!("{}", ReflectionMode::Improve), "improve");
    }
}
