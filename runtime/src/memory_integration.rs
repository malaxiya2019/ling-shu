//! MemoryIntegration — 记忆系统与 Runtime 的完整集成层。
//!
//! 提供：
//! - `MemoryRuntime` — 持有所有记忆组件
//! - `QueryMemoryTool` — Agent 可调用的记忆查询工具
//! - `ReflectionWorkflow` 自动集成 — 每次查询后自动记录反馈
//! - 完整的初始化流程
//!
//! # 使用示例
//!
//! ```rust,ignore
//! let memory = MemoryRuntime::new();
//! memory.auto_store_episode(&ctx, "启动项目A", "项目正式启动", entities).await;
//!
//! let graph = memory.query("项目A为什么暂停").await.unwrap();
//!
//! // 通过反思工作流分析查询质量
//! let report = memory.query("recent:10").await.unwrap();
//! let stats = memory.query("route_stats").await.unwrap();
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use lingshu_evidence_graph::{EvidenceGraph, EvidenceGraphSerializer, WeightedMergeResult};
use lingshu_memory_episode::{
    EntityRef, Episode, EpisodeRepository, InMemoryEpisodeStore, StateChange,
};
use lingshu_memory_reflection::{
    evaluate_memory_query, FeedbackAnalytics, FeedbackStore, InMemoryFeedbackStore,
    ReflectionEvaluator, ReflectionFeedback, ReflectionWorkflow,
};
use lingshu_memory_semantic::{SemanticIndex, SemanticWorkflow, TfIdfIndex};
use lingshu_memory_workflow::{
    MemoryQuery, MemoryRouter, MemoryWorkflow, MemoryWorkflowRegistry,
    TimelineWorkflowAdapter,
};
use lingshu_traits::tool::{Tool, ToolInfo, ToolMetadata, ToolParam};
use lingshu_workflow_memory::TimelineWorkflow;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

// ─── MemoryRuntime ─────────────────────────────────────

/// MemoryRuntime — 记忆系统的运行时集成入口。
///
/// 持有所有记忆组件，提供统一的初始化、查询、存储接口。
/// 自动集成 ReflectionWorkflow，每次查询后记录反馈用于质量分析。
#[derive(Clone)]
pub struct MemoryRuntime {
    /// Episode 存储
    episode_store: Arc<dyn EpisodeRepository>,
    /// 记忆工作流注册表
    workflow_registry: Arc<RwLock<MemoryWorkflowRegistry>>,
    /// 记忆路由器
    router: Arc<MemoryRouter>,
    /// 语义索引
    semantic_index: Arc<dyn SemanticIndex>,
    /// 反思反馈存储
    feedback_store: Arc<dyn FeedbackStore>,
    /// 反思评估器
    reflection_evaluator: Arc<ReflectionEvaluator>,
}

impl MemoryRuntime {
    /// 创建一个新的记忆运行时（使用内存存储）。
    pub fn new() -> Self {
        let episode_store = Arc::new(InMemoryEpisodeStore::new());
        let semantic_index = Arc::new(TfIdfIndex::new()) as Arc<dyn SemanticIndex>;

        let timeline_workflow = TimelineWorkflow::new(Box::new(episode_store.clone()));
        let timeline_adapter = TimelineWorkflowAdapter::new(timeline_workflow);

        let semantic_workflow = SemanticWorkflow::new(semantic_index.clone());

        // 初始化反思评估器和反馈存储
        let feedback_store = Arc::new(InMemoryFeedbackStore::new()) as Arc<dyn FeedbackStore>;
        let reflection_evaluator = Arc::new(ReflectionEvaluator::new());

        // 创建并注册反思工作流
        let reflection_workflow = ReflectionWorkflow::new()
            .with_feedback_store(feedback_store.clone())
            .with_evaluator((*reflection_evaluator).clone());

        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(timeline_adapter));
        registry.register(Box::new(semantic_workflow));
        registry.register(Box::new(reflection_workflow));

        Self {
            episode_store: episode_store as Arc<dyn EpisodeRepository>,
            workflow_registry: Arc::new(RwLock::new(registry)),
            router: Arc::new(MemoryRouter::new()),
            semantic_index,
            feedback_store,
            reflection_evaluator,
        }
    }

    /// 创建带自定义存储的记忆运行时。
    pub fn with_store(store: Arc<dyn EpisodeRepository>) -> Self {
        let timeline_workflow = TimelineWorkflow::new(Box::new(store.clone()));
        let timeline_adapter = TimelineWorkflowAdapter::new(timeline_workflow);

        let semantic_index = Arc::new(TfIdfIndex::new()) as Arc<dyn SemanticIndex>;
        let semantic_workflow = SemanticWorkflow::new(semantic_index.clone());

        // 初始化反思评估器和反馈存储
        let feedback_store = Arc::new(InMemoryFeedbackStore::new()) as Arc<dyn FeedbackStore>;
        let reflection_evaluator = Arc::new(ReflectionEvaluator::new());

        // 创建并注册反思工作流
        let reflection_workflow = ReflectionWorkflow::new()
            .with_feedback_store(feedback_store.clone())
            .with_evaluator((*reflection_evaluator).clone());

        let mut registry = MemoryWorkflowRegistry::new();
        registry.register(Box::new(timeline_adapter));
        registry.register(Box::new(semantic_workflow));
        registry.register(Box::new(reflection_workflow));

        Self {
            episode_store: store,
            workflow_registry: Arc::new(RwLock::new(registry)),
            router: Arc::new(MemoryRouter::new()),
            semantic_index,
            feedback_store,
            reflection_evaluator,
        }
    }

    /// 创建带 SQLite 持久化存储的记忆运行时。
    #[cfg(feature = "memory-sqlite")]
    pub fn with_sqlite_store(path: impl AsRef<std::path::Path>) -> LsResult<Self> {
        let store = lingshu_memory_episode_sqlite::SQLiteEpisodeStore::new(path)?;
        Ok(Self::with_store(Arc::new(store)))
    }

    /// 存储一个 Episode。
    pub async fn store_episode(&self, episode: Episode) -> LsResult<()> {
        // 同时写入 Episode Store 和 Semantic Index
        let episode_clone = episode.clone();
        self.episode_store.store(episode).await?;
        let _ = self.semantic_index.index_episode(&episode_clone).await;
        Ok(())
    }

    /// 快捷存储一个事件（自动构建 Episode 结构）。
    pub async fn auto_store_episode(
        &self,
        title: &str,
        summary: &str,
        entities: Vec<(&str, &str)>,
        tags: Vec<&str>,
        state_changes: Vec<(&str, &str, Option<&str>, &str)>,
    ) -> LsResult<()> {
        let mut episode = Episode::new(title, summary, chrono::Utc::now());

        for (kind, name) in entities {
            episode = episode.with_entity(EntityRef::new(kind, name));
        }
        for tag in tags {
            episode = episode.with_tag(tag);
        }
        for (change_type, entity_name, from, to) in state_changes {
            episode = episode.with_state_change(StateChange::new(
                EntityRef::new("entity", entity_name),
                change_type,
                from.map(|s| s.to_string()),
                to,
            ));
        }

        let episode_clone = episode.clone();
        self.episode_store.store(episode).await?;
        let _ = self.semantic_index.index_episode(&episode_clone).await;
        Ok(())
    }

    /// 执行记忆查询（旧式单路由模式，保持向后兼容）。
    ///
    /// 使用 MemoryRouter 判断路由，执行单个 workflow。
    /// 每次查询后自动记录反思反馈。
    ///
    /// 特殊查询模式（直接路由到反思工作流）：
    /// - `recent:N` — 最近 N 次查询分析
    /// - `route_stats` / `路由统计` — 路由使用统计
    /// - `conflicts:N` — 最近 N 条冲突记录
    /// - `improve` / `优化建议` — 改进建议
    pub async fn query(&self, question: &str) -> LsResult<EvidenceGraph> {
        let result = self.query_weighted(question).await?;
        Ok(result.graph)
    }

    /// 加权记忆查询 — 使用 ProbabilisticRouter + WeightedGraphMerger。
    ///
    /// 并行执行所有相关 workflow，按路由权重合并结果，检测冲突。
    ///
    /// # 返回
    ///
    /// WeightedMergeResult 包含：
    /// - 合并后的 EvidenceGraph
    /// - 合并统计（新增/去重节点数）
    /// - 检测到的冲突列表
    /// - 节点来源映射
    ///
    /// 特殊查询模式（直接路由到反思工作流）：
    /// - `recent:N` — 最近 N 次查询分析
    /// - `route_stats` / `路由统计` — 路由使用统计
    /// - `conflicts:N` — 最近 N 条冲突记录
    /// - `improve` / `优化建议` — 改进建议
    pub async fn query_weighted(&self, question: &str) -> LsResult<WeightedMergeResult> {
        let start = std::time::Instant::now();

        // 检测是否为反思查询模式
        let q = question.trim().to_lowercase();
        let is_reflection_query = q.starts_with("recent:")
            || q == "route_stats"
            || q == "route stats"
            || q == "路由统计"
            || q.starts_with("conflicts:")
            || q == "improve"
            || q == "优化建议";

        if is_reflection_query {
            debug!(question, "detected reflection query, routing to reflection workflow");
            let registry = self.workflow_registry.read().await;
            match registry.get("reflection") {
                Some(workflow) => {
                    let graph = workflow.execute(MemoryQuery::new(question)).await?;
                    let elapsed = start.elapsed().as_millis() as u64;
                    debug!(question, elapsed_ms = elapsed, nodes = graph.nodes.len(), "reflection query complete");
                    return Ok(WeightedMergeResult {
                        graph,
                        merge_stats: Default::default(),
                        conflicts: Vec::new(),
                        source_map: std::collections::HashMap::new(),
                    });
                }
                None => {
                    return Ok(WeightedMergeResult {
                        graph: EvidenceGraph::empty(question),
                        merge_stats: Default::default(),
                        conflicts: Vec::new(),
                        source_map: std::collections::HashMap::new(),
                    });
                }
            }
        }

        // 使用 ProbabilisticRouter 获取加权路由
        let weights = self.router.probabilistic().route(question);

        // 如果记忆权重很低，尝试走普通路由
        let memory_weight = *weights.get("timeline").unwrap_or(&0.0)
            + *weights.get("semantic").unwrap_or(&0.0);
        let is_conversation = *weights.get("conversation").unwrap_or(&0.0) > memory_weight
            && *weights.get("conversation").unwrap_or(&0.0) > 0.3;

        if is_conversation || memory_weight < 0.05 {
            debug!(question, weights = ?weights, "memory not needed, skipping");
            let empty = WeightedMergeResult {
                graph: EvidenceGraph::empty(question),
                merge_stats: Default::default(),
                conflicts: Vec::new(),
                source_map: std::collections::HashMap::new(),
            };
            // 即使空结果也记录反馈
            let elapsed = start.elapsed().as_millis() as u64;
            self.record_feedback(question, "no_memory", &empty.graph, elapsed).await;
            return Ok(empty);
        }

        // 加权并行执行
        let registry = self.workflow_registry.read().await;
        let result = registry
            .execute_weighted(MemoryQuery::new(question), weights)
            .await?;

        let elapsed = start.elapsed().as_millis() as u64;

        // 记录反思反馈
        self.record_feedback_weighted(question, &result, elapsed).await;

        // 如果有冲突，记录警告
        if !result.conflicts.is_empty() {
            debug!(
                question,
                conflicts = result.conflicts.len(),
                "memory query detected conflicts during merge"
            );
        }

        Ok(result)
    }

    /// 记录单次查询的反思反馈。
    async fn record_feedback(
        &self,
        question: &str,
        route: &str,
        graph: &EvidenceGraph,
        latency_ms: u64,
    ) {
        // 使用 reflection 的评估器生成质量结果
        let reflection_result = evaluate_memory_query(question, route, graph, latency_ms);

        // 转换为反馈记录并存储
        let feedback = ReflectionFeedback::from_result(&reflection_result, "runtime");
        if let Err(e) = self.feedback_store.store(feedback).await {
            tracing::warn!(error = %e, "failed to store reflection feedback");
        }
    }

    /// 记录加权查询的反思反馈（含冲突信息）。
    async fn record_feedback_weighted(
        &self,
        question: &str,
        merge_result: &WeightedMergeResult,
        latency_ms: u64,
    ) {
        let route = "weighted_merge";
        let graph = &merge_result.graph;

        // 使用 reflection 的评估器生成质量结果
        let mut reflection_result = evaluate_memory_query(question, route, graph, latency_ms);

        // 叠加冲突信息
        if !merge_result.conflicts.is_empty() {
            reflection_result.has_conflicts = true;
            for conflict in &merge_result.conflicts {
                let mapped_type = match &conflict.conflict_type {
                    lingshu_evidence_graph::ConflictType::FactConflict => lingshu_memory_reflection::ConflictType::FactConflict,
                    lingshu_evidence_graph::ConflictType::TemporalConflict => lingshu_memory_reflection::ConflictType::TemporalConflict,
                    lingshu_evidence_graph::ConflictType::StateConflict => lingshu_memory_reflection::ConflictType::StateConflict,
                };
                reflection_result.conflicts.push(
                    lingshu_memory_reflection::ConflictInfo {
                        conflict_type: mapped_type,
                        description: conflict.description.clone(),
                        node_ids: conflict.node_ids.clone(),
                        severity: conflict.severity,
                    }
                );
            }
        }

        // 转换为反馈记录并存储
        let feedback = ReflectionFeedback::from_result(&reflection_result, "runtime");
        if let Err(e) = self.feedback_store.store(feedback).await {
            tracing::warn!(error = %e, "failed to store weighted query feedback");
        }
    }

    /// 查询并返回人类可读的文本摘要。
    pub async fn query_text(&self, question: &str) -> LsResult<String> {
        let graph = self.query(question).await?;
        Ok(EvidenceGraphSerializer::to_text_summary(&graph, 20))
    }

    /// 获取 Episode 总数。
    pub async fn episode_count(&self) -> LsResult<usize> {
        self.episode_store.count().await
    }

    /// 获取 Episode 存储引用。
    pub fn episode_store(&self) -> &Arc<dyn EpisodeRepository> {
        &self.episode_store
    }

    /// 获取路由器引用。
    pub fn router(&self) -> &Arc<MemoryRouter> {
        &self.router
    }

    /// 获取反思反馈存储引用。
    pub fn feedback_store(&self) -> &Arc<dyn FeedbackStore> {
        &self.feedback_store
    }

    /// 获取反思评估器引用。
    pub fn reflection_evaluator(&self) -> &Arc<ReflectionEvaluator> {
        &self.reflection_evaluator
    }

    /// 获取反馈统计分析。
    pub async fn feedback_analytics(&self) -> Result<FeedbackAnalyticsSnapshot, lingshu_memory_reflection::ReflectionError> {
        let total = self.feedback_store.count().await?;
        let conflict_rate = FeedbackAnalytics::conflict_rate(self.feedback_store.as_ref()).await?;
        let route_stats = FeedbackAnalytics::route_stats(self.feedback_store.as_ref()).await?;

        Ok(FeedbackAnalyticsSnapshot {
            total_feedbacks: total,
            conflict_rate,
            route_stats,
        })
    }

    /// 注册自定义 MemoryWorkflow。
    pub async fn register_workflow(&self, workflow: Box<dyn MemoryWorkflow>) {
        let mut registry = self.workflow_registry.write().await;
        registry.register(workflow);
    }

    /// 创建一个 `query_memory` 工具，供 Agent 调用。
    pub fn create_query_tool(&self) -> QueryMemoryTool {
        QueryMemoryTool::new(self.clone())
    }
}

impl Default for MemoryRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// ─── FeedbackAnalyticsSnapshot ─────────────────────────

/// 反馈分析快照 — 对外暴露的统计摘要。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeedbackAnalyticsSnapshot {
    /// 总反馈数
    pub total_feedbacks: usize,
    /// 冲突率
    pub conflict_rate: f64,
    /// 各路由统计
    pub route_stats: Vec<lingshu_memory_reflection::RouteStats>,
}

// ─── QueryMemoryTool ────────────────────────────────────

/// QueryMemoryTool — Agent 可调用的记忆查询工具。
///
/// 工具定义：
/// - 名称：query_memory
/// - 描述：查询历史记忆，返回事件时间线
/// - 参数：question（字符串，必填）
pub struct QueryMemoryTool {
    memory: MemoryRuntime,
}

impl QueryMemoryTool {
    pub fn new(memory: MemoryRuntime) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for QueryMemoryTool {
    fn info(&self) -> ToolInfo {
        let mut info = ToolInfo::new(
            "query_memory",
            "查询历史记忆，返回与问题相关的事件时间线。适用于回忆过去发生的事件、项目进展、决策原因等",
            vec![
                ToolParam {
                    name: "question".into(),
                    description: "要查询的问题，如'项目A为什么暂停'".into(),
                    param_type: "string".into(),
                    required: true,
                },
            ],
        );
        info.metadata = ToolMetadata {
            tags: vec!["memory".into(), "history".into(), "timeline".into()],
            ..Default::default()
        };
        info
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input.get("question").and_then(|v| v.as_str()).is_none() {
            return Err(lingshu_core::LsError::InvalidArgument(
                "missing required parameter: question".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        let question = input["question"]
            .as_str()
            .ok_or_else(|| {
                lingshu_core::LsError::InvalidArgument("question must be a string".into())
            })?
            .to_string();

        let start = std::time::Instant::now();
        let graph = self.memory.query(&question).await?;
        let elapsed = start.elapsed().as_millis() as u64;

        let text_summary = EvidenceGraphSerializer::to_text_summary(&graph, 20);

        Ok(serde_json::json!({
            "question": question,
            "found_events": graph.metadata.node_count,
            "time_span": {
                "from": graph.metadata.time_span_start,
                "to": graph.metadata.time_span_end,
            },
            "entities": graph.metadata.entities,
            "source": graph.metadata.source,
            "query_time_ms": elapsed,
            "summary": text_summary,
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(QueryMemoryTool::new(self.memory.clone()))
    }
}

// ─── 工具函数 ───────────────────────────────────────────

/// 将 ToolRegistry 注册到 AgentRuntime 并初始化记忆系统。
///
/// 这是推荐的集成方式。
pub fn setup_memory_for_runtime(
    tool_registry: &Arc<RwLock<lingshu_tool::ToolRegistry>>,
) -> MemoryRuntime {
    let memory = MemoryRuntime::new();
    let query_tool = memory.create_query_tool();

    let registry = tool_registry.clone();
    let tool = Box::new(query_tool) as Box<dyn Tool>;
    tokio::spawn(async move {
        registry.write().await.register(tool).await;
        info!("query_memory tool registered");
    });

    info!("MemoryRuntime initialized for AgentRuntime (with Reflection)");
    memory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_query() {
        let memory = MemoryRuntime::new();

        // 存入测试事件
        memory
            .auto_store_episode(
                "启动项目A",
                "团队决定启动项目A的开发",
                vec![("project", "项目A"), ("person", "张三")],
                vec!["launch"],
                vec![],
            )
            .await
            .unwrap();

        memory
            .auto_store_episode(
                "供应商退出",
                "核心供应商退出合作",
                vec![("project", "项目A"), ("organization", "供应商X")],
                vec!["risk"],
                vec![("status", "项目A", Some("active"), "blocked")],
            )
            .await
            .unwrap();

        memory
            .auto_store_episode(
                "暂停项目A",
                "因供应商问题暂停项目A",
                vec![("project", "项目A")],
                vec!["decision"],
                vec![("status", "项目A", Some("blocked"), "paused")],
            )
            .await
            .unwrap();

        assert_eq!(memory.episode_count().await.unwrap(), 3);

        // 查询
        let graph = memory.query("项目A为什么暂停").await.unwrap();
        assert!(
            graph.metadata.node_count > 0,
            "should find relevant episodes"
        );

        // 文本摘要
        let text = memory.query_text("项目A").await.unwrap();
        assert!(text.contains("启动项目A"), "summary should contain events");
        assert!(text.contains("暂停项目A"), "summary should contain all events");
    }

    #[tokio::test]
    async fn test_query_no_match() {
        let memory = MemoryRuntime::new();
        let graph = memory.query("完全不存在的查询").await.unwrap();
        assert_eq!(graph.nodes.len(), 0);
    }

    #[tokio::test]
    async fn test_router_skips_greeting() {
        let memory = MemoryRuntime::new();
        let graph = memory.query("你好").await.unwrap();
        assert_eq!(graph.nodes.len(), 0, "greeting should skip memory");
    }

    #[tokio::test]
    async fn test_query_tool_info() {
        let memory = MemoryRuntime::new();
        let tool = memory.create_query_tool();
        let info = tool.info();
        assert_eq!(info.name, "query_memory");
        assert!(!info.description.is_empty());
        assert_eq!(info.parameters.len(), 1);
    }

    #[tokio::test]
    async fn test_query_tool_execute() {
        let memory = MemoryRuntime::new();
        memory
            .auto_store_episode(
                "项目A暂停讨论",
                "讨论是否暂停项目A",
                vec![("project", "项目A")],
                vec!["decision"],
                vec![],
            )
            .await
            .unwrap();

        let tool = memory.create_query_tool();
        let result = tool
            .execute(
                LsContext::with_session(lingshu_core::LsId::new()),
                serde_json::json!({"question": "项目A为什么暂停"}),
            )
            .await
            .unwrap();

        // 加权合并模式下，两个 workflow（timeline + semantic）都可能返回结果
        assert!(result["found_events"].as_u64().unwrap_or(0) >= 1, "should find at least 1 event");
        assert!(result["summary"].as_str().unwrap().contains("项目A"));
    }

    #[tokio::test]
    async fn test_query_tool_invalid_input() {
        let memory = MemoryRuntime::new();
        let tool = memory.create_query_tool();
        let result = tool
            .validate(&serde_json::json!({"wrong_param": "test"}));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_auto_store_with_state_changes() {
        let memory = MemoryRuntime::new();
        memory
            .auto_store_episode(
                "状态变更",
                "状态从active变为paused",
                vec![("project", "项目X")],
                vec!["status_change"],
                vec![("status", "项目X", Some("active"), "paused")],
            )
            .await
            .unwrap();

        let graph = memory.query("项目X").await.unwrap();
        assert!(graph.metadata.node_count >= 1);
    }

    #[tokio::test]
    async fn test_episode_count() {
        let memory = MemoryRuntime::new();
        assert_eq!(memory.episode_count().await.unwrap(), 0);

        memory
            .auto_store_episode("事件1", "描述", vec![], vec![], vec![])
            .await
            .unwrap();
        assert_eq!(memory.episode_count().await.unwrap(), 1);

        memory
            .auto_store_episode("事件2", "描述", vec![], vec![], vec![])
            .await
            .unwrap();
        assert_eq!(memory.episode_count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_setup_memory_for_runtime() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let registry = Arc::new(RwLock::new(lingshu_tool::ToolRegistry::new()));
        let _memory = setup_memory_for_runtime(&registry);

        // Wait for the async registration to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let tools = registry.read().await.list_tools().await;
        assert!(!tools.is_empty(), "query_memory tool should be registered");

        // Find our tool — list_tools() returns Vec<String> (names)
        let has_memory_tool = tools.iter().any(|name| name == "query_memory");
        assert!(has_memory_tool, "query_memory tool must be in registry");
    }

    // ── 反思工作流集成测试 ─────────────────────────────

    #[tokio::test]
    async fn test_reflection_workflow_is_registered() {
        let memory = MemoryRuntime::new();
        let registry = memory.workflow_registry.read().await;
        let names = registry.list_names();
        assert!(names.contains(&"reflection".to_string()),
            "reflection workflow should be registered, got: {:?}", names);
    }

    #[tokio::test]
    async fn test_reflection_recent_query() {
        let memory = MemoryRuntime::new();

        // 先做一些查询，让反馈存储有数据
        memory.auto_store_episode("事件1", "测试", vec![], vec![], vec![]).await.unwrap();
        memory.auto_store_episode("事件2", "测试", vec![], vec![], vec![]).await.unwrap();
        let _ = memory.query("事件1").await.unwrap();
        let _ = memory.query("事件2").await.unwrap();

        // 通过反思工作流查询最近分析
        let result = memory.query("recent:10").await.unwrap();
        assert!(!result.nodes.is_empty(), "reflection should return analysis");
        assert_eq!(result.metadata.source, "reflection_workflow");
    }

    #[tokio::test]
    async fn test_feedback_auto_recorded_after_query() {
        let memory = MemoryRuntime::new();

        memory.auto_store_episode("测试事件", "描述", vec![], vec![], vec![]).await.unwrap();
        let _ = memory.query("测试事件").await.unwrap();

        let count = memory.feedback_store.count().await.unwrap();
        assert_eq!(count, 1, "feedback should be auto-recorded after query");
    }

    #[tokio::test]
    async fn test_feedback_analytics_snapshot() {
        let memory = MemoryRuntime::new();

        memory.auto_store_episode("事件A", "测试", vec![], vec![], vec![]).await.unwrap();
        let _ = memory.query("事件A").await.unwrap();
        let _ = memory.query("事件A").await.unwrap();
        let _ = memory.query("事件A").await.unwrap();

        let analytics = memory.feedback_analytics().await.unwrap();
        assert_eq!(analytics.total_feedbacks, 3);
        assert!(analytics.conflict_rate >= 0.0);
    }

    #[tokio::test]
    async fn test_reflection_route_stats_workflow() {
        let memory = MemoryRuntime::new();

        memory.auto_store_episode("事件A", "测试", vec![], vec![], vec![]).await.unwrap();
        let _ = memory.query("事件A").await.unwrap();

        // 路由统计
        let result = memory.query("route_stats").await.unwrap();
        assert!(!result.nodes.is_empty(), "route stats should return data");
    }

    #[tokio::test]
    async fn test_reflection_conflicts_workflow() {
        let memory = MemoryRuntime::new();

        memory.auto_store_episode("事件A", "测试", vec![], vec![], vec![]).await.unwrap();
        let _ = memory.query("事件A").await.unwrap();

        let result = memory.query("conflicts:5").await.unwrap();
        assert!(!result.nodes.is_empty(), "conflicts report should return data");
    }

    #[tokio::test]
    async fn test_reflection_improve_workflow() {
        let memory = MemoryRuntime::new();

        memory.auto_store_episode("事件A", "测试", vec![], vec![], vec![]).await.unwrap();
        let _ = memory.query("事件A").await.unwrap();

        let result = memory.query("improve").await.unwrap();
        assert!(!result.nodes.is_empty(), "improvement suggestions should return data");
    }

    #[tokio::test]
    async fn test_feedback_store_accessor() {
        let memory = MemoryRuntime::new();
        let store = memory.feedback_store();
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_reflection_evaluator_accessor() {
        let memory = MemoryRuntime::new();
        let evaluator = memory.reflection_evaluator();
        assert_eq!(evaluator.min_evidence_count, 1);
    }
}
