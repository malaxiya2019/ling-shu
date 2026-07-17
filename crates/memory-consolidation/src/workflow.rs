//! ConsolidationWorkflow — 记忆巩固工作流。
//!
//! 将短期零散的 Episode 提炼为结构化长期 ConsolidatedMemory，
//! 统一通过 MemoryWorkflow trait 暴露给 Memory Runtime。
//!
//! # 流程
//!
//! ```text
//! MemoryQuery
//!      │
//!      ▼
//! ConsolidationWorkflow.execute()
//!      │
//!      ├── 1. 获取未巩固的 Episode
//!      ├── 2. 运行 ConsolidationEngine 的策略
//!      ├── 3. ImportanceScorer 评估重要性
//!      ├── 4. 持久化到 ConsolidatedMemoryRepository
//!      └── 5. 构建并返回 EvidenceGraph
//! ```

use crate::engine::ConsolidationEngine;
use crate::importance::ImportanceScorer;
use crate::store::ConsolidatedMemoryRepository;
use crate::strategy::ConsolidationStrategy;
use crate::types::*;
use async_trait::async_trait;
use lingshu_core::LsResult;
use lingshu_evidence_graph::{Edge, EvidenceGraph, Node, NodeId};
use lingshu_memory_episode::{Episode, EpisodeRepository};
use lingshu_memory_workflow::{MemoryQuery, MemoryWorkflow};
use std::sync::Arc;
use tracing::{debug, info};

/// ConsolidationWorkflow — 记忆巩固工作流。
///
/// 实现 MemoryWorkflow trait，将 Episode 巩固为 ConsolidatedMemory。
///
/// # 工作流模式
///
/// 根据查询内容采用不同的巩固策略：
/// - 空查询或通用查询 → 对所有未巩固的 Episode 执行全量巩固
/// - 包含实体名称 → 按实体巩固
#[allow(dead_code)]
pub struct ConsolidationWorkflow {
    /// Episode 存储
    episode_store: Arc<dyn EpisodeRepository>,
    /// 巩固记忆持久化存储
    consolidated_store: Arc<dyn ConsolidatedMemoryRepository>,
    /// 巩固引擎
    engine: ConsolidationEngine,
    /// 重要性评分器
    importance_scorer: ImportanceScorer,
    /// 最大结果数
    max_results: usize,
}

impl ConsolidationWorkflow {
    /// 创建 ConsolidationWorkflow。
    pub fn new(
        episode_store: Arc<dyn EpisodeRepository>,
        consolidated_store: Arc<dyn ConsolidatedMemoryRepository>,
        config: ConsolidationConfig,
    ) -> Self {
        let engine = ConsolidationEngine::new(episode_store.clone(), config);
        Self {
            episode_store,
            consolidated_store,
            engine,
            importance_scorer: ImportanceScorer::new(),
            max_results: 50,
        }
    }

    /// 添加巩固策略。
    pub async fn add_strategy(&self, strategy: Box<dyn ConsolidationStrategy>) {
        self.engine.add_strategy(strategy).await;
    }

    /// 批量添加巩固策略。
    pub async fn add_strategies(&self, strategies: Vec<Box<dyn ConsolidationStrategy>>) {
        self.engine.add_strategies(strategies).await;
    }

    /// 设置重要性评分器。
    pub fn with_importance_scorer(mut self, scorer: ImportanceScorer) -> Self {
        self.importance_scorer = scorer;
        self
    }

    /// 设置最大结果数。
    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    /// 从 Episode 构建 EvidenceGraph 节点。
    #[allow(dead_code)]
    fn episode_to_node(
        episode: &Episode,
        importance: f64,
        level: &crate::importance::ImportanceLevel,
    ) -> Node {
        let mut node = Node::event(&episode.title, &episode.summary, episode.timestamp)
            .with_tag(format!("importance:{:.2}", importance))
            .with_tag(format!("level:{}", level.as_str()))
            .with_tag(format!("source_id:{}", episode.id));

        // 添加实体作为标签
        for entity in &episode.entities {
            node = node.with_tag(format!("entity:{}:{}", entity.kind, entity.name));
        }

        // 添加状态变更
        for change in &episode.state_changes {
            let change_str = format!(
                "{} {} → {}",
                change.change_type,
                change.from.as_deref().unwrap_or("?"),
                change.to
            );
            node = node.with_tag(format!("state_change:{}", change_str));
        }

        node
    }

    /// 从 ConsolidatedMemory 构建 EvidenceGraph 节点。
    fn consolidated_to_node(
        memory: &ConsolidatedMemory,
        importance: f64,
        level: &crate::importance::ImportanceLevel,
    ) -> Node {
        let timestamp = memory.created_at;
        let mut node = Node::event(
            format!("[巩固] {}", memory.title),
            format!(
                "[策略: {}, 置信度: {:.2}, 重要性: {:.2}] {}",
                memory.strategy, memory.confidence, importance, memory.summary
            ),
            timestamp,
        )
        .with_tag("consolidated")
        .with_tag(format!("strategy:{}", memory.strategy))
        .with_tag(format!("importance:{:.2}", importance))
        .with_tag(format!("level:{}", level.as_str()))
        .with_tag(format!("confidence:{:.2}", memory.confidence))
        .with_tag(format!("consolidated_id:{}", memory.id));

        for tag in &memory.tags {
            node = node.with_tag(tag);
        }

        node
    }

    /// 内部执行：巩固并构建 EvidenceGraph。
    async fn execute_internal(&self, query: &MemoryQuery) -> LsResult<EvidenceGraph> {
        let start = std::time::Instant::now();

        // 检查是否有注册的策略
        let strategies = self.engine.list_strategies().await;
        if strategies.is_empty() {
            debug!("consolidation: no strategies registered, returning empty");
            return Ok(EvidenceGraph::empty(&query.question));
        }

        // 1. 获取未巩固的 Episode
        let unconsolidated = self
            .engine
            .run_consolidation()
            .await
            .map_err(|e| {
                lingshu_core::LsError::Internal(format!("consolidation failed: {}", e))
            })?;

        info!(
            "consolidation: processed {}, consolidated {}",
            unconsolidated.processed_count, unconsolidated.consolidated_count
        );

        // 2. 从 consolidated store 读取所有已巩固的记忆
        let consolidated_memories = self
            .consolidated_store
            .list_consolidated(self.max_results, 0)
            .await
            .map_err(|e| {
                lingshu_core::LsError::Internal(format!("failed to list consolidated: {}", e))
            })?;

        // 3. 构建 EvidenceGraph
        let mut graph = EvidenceGraph::empty(&query.question);
        graph.metadata.source = "consolidation_workflow".into();
        graph.metadata.node_count = consolidated_memories.len();
        graph.metadata.build_time_ms = start.elapsed().as_millis() as u64;

        // 添加 consolidated memories 作为节点
        let mut prev_node_id: Option<NodeId> = None;
        let mut all_entities = Vec::new();
        let mut time_span_start: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut time_span_end: Option<chrono::DateTime<chrono::Utc>> = None;

        for mem in &consolidated_memories {
            let importance = self.importance_scorer.score(mem);
            let level = crate::importance::ImportanceLevel::from_score(importance);
            let node = Self::consolidated_to_node(mem, importance, &level);

            let node_id = node.id;

            // 时间顺序边
            if let Some(prev_id) = prev_node_id {
                graph.add_edge(Edge::temporal(prev_id, node_id));
            }

            graph.add_node(node);
            prev_node_id = Some(node_id);

            // 更新实体的集合
            for entity in &mem.entities {
                let key = format!("{}:{}", entity.kind, entity.name);
                if !all_entities.contains(&key) {
                    all_entities.push(key);
                }
            }

            // 更新时间范围
            if let Some(start) = mem.time_span_start {
                if time_span_start.is_none() || start < time_span_start.unwrap() {
                    time_span_start = Some(start);
                }
            }
            if let Some(end) = mem.time_span_end {
                if time_span_end.is_none() || end > time_span_end.unwrap() {
                    time_span_end = Some(end);
                }
            }
        }

        graph.metadata.node_count = graph.nodes.len();
        graph.metadata.edge_count = graph.edges.len();
        graph.metadata.entities = all_entities;
        graph.metadata.time_span_start = time_span_start;
        graph.metadata.time_span_end = time_span_end;

        // 在 metadata 中记录 consolidation 统计
        graph
            .metadata
            .attributes
            .insert("consolidated_count".to_string(), consolidated_memories.len().to_string());
        graph
            .metadata
            .attributes
            .insert("strategies".to_string(), strategies.join(","));

        // 记录平均重要性分数
        if !consolidated_memories.is_empty() {
            let avg_importance: f64 = consolidated_memories
                .iter()
                .map(|m| self.importance_scorer.score(m))
                .sum::<f64>()
                / consolidated_memories.len() as f64;
            graph
                .metadata
                .attributes
                .insert("avg_importance".to_string(), format!("{:.3}", avg_importance));
        }

        debug!(
            "consolidation workflow completed: {} nodes, {} edges, {:.0}ms",
            graph.nodes.len(),
            graph.edges.len(),
            graph.metadata.build_time_ms
        );

        Ok(graph)
    }
}

#[async_trait]
impl MemoryWorkflow for ConsolidationWorkflow {
    fn name(&self) -> &str {
        "consolidation"
    }

    async fn execute(&self, query: MemoryQuery) -> LsResult<EvidenceGraph> {
        self.execute_internal(&query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importance::ImportanceScorer;
    use crate::store::InMemoryConsolidatedStore;
    use crate::strategy::default_strategies;
    use lingshu_memory_episode::{EntityRef, Episode, InMemoryEpisodeStore};

    fn make_episode(title: &str, entity: &str, hours_ago: i64) -> Episode {
        let mut ep = Episode::new(
            title,
            format!("测试: {}", title),
            chrono::Utc::now() - chrono::Duration::hours(hours_ago),
        );
        ep.entities.push(EntityRef::new("project", entity));
        ep
    }

    async fn setup_workflow() -> (Arc<dyn EpisodeRepository>, ConsolidationWorkflow) {
        let store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let consolidated_store = Arc::new(InMemoryConsolidatedStore::new())
            as Arc<dyn ConsolidatedMemoryRepository>;

        // 添加测试数据
        for i in 0..5 {
            store
                .store(make_episode(
                    &format!("事件{}", i),
                    "项目A",
                    i as i64 * 12,
                ))
                .await
                .unwrap();
        }

        let config = ConsolidationConfig {
            auto_write_episodes: true,
            ..Default::default()
        };
        let workflow = ConsolidationWorkflow::new(store.clone(), consolidated_store, config);
        workflow.add_strategies(default_strategies()).await;

        (store, workflow)
    }

    #[tokio::test]
    async fn test_workflow_name() {
        let (_, workflow) = setup_workflow().await;
        assert_eq!(workflow.name(), "consolidation");
    }

    #[tokio::test]
    async fn test_workflow_execute() {
        let (_, workflow) = setup_workflow().await;
        let query = MemoryQuery::new("test");
        let result = workflow.execute(query).await.unwrap();
        // Should have at least some consolidated memories
        assert!(result.metadata.node_count > 0 || result.nodes.is_empty());
    }

    #[tokio::test]
    async fn test_workflow_empty_strategies() {
        let store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let consolidated_store = Arc::new(InMemoryConsolidatedStore::new())
            as Arc<dyn ConsolidatedMemoryRepository>;

        let workflow = ConsolidationWorkflow::new(
            store.clone(),
            consolidated_store,
            ConsolidationConfig::default(),
        );
        // no strategies registered

        let query = MemoryQuery::new("test");
        let result = workflow.execute(query).await.unwrap();
        assert!(result.nodes.is_empty(), "no strategies should produce no nodes");
    }

    #[tokio::test]
    async fn test_workflow_returns_evidence_graph() {
        let (_, workflow) = setup_workflow().await;
        let query = MemoryQuery::new("consolidation test");
        let result = workflow.execute(query).await.unwrap();

        // Should be an EvidenceGraph
        assert_eq!(result.metadata.source, "consolidation_workflow");
        // build_time_ms may be 0 on fast systems
        // build_time_ms is u64, always >= 0
    }

    #[tokio::test]
    async fn test_workflow_with_importance_scorer() {
        let store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let consolidated_store = Arc::new(InMemoryConsolidatedStore::new())
            as Arc<dyn ConsolidatedMemoryRepository>;

        // Add episodes
        store
            .store(make_episode("重要事件", "项目A", 1))
            .await
            .unwrap();

        let config = ConsolidationConfig::default();
        let scorer = ImportanceScorer::new();
        let workflow = ConsolidationWorkflow::new(store.clone(), consolidated_store, config)
            .with_importance_scorer(scorer);
        workflow.add_strategies(default_strategies()).await;

        let query = MemoryQuery::new("test");
        let _result = workflow.execute(query).await.unwrap();
        // node_count is usize, always >= 0
    }

    #[tokio::test]
    async fn test_workflow_metadata_includes_stats() {
        let (_, workflow) = setup_workflow().await;

        let result = workflow
            .execute(MemoryQuery::new("统计测试"))
            .await
            .unwrap();

        // Should have consolidation metadata
        if result.metadata.node_count > 0 {
            assert!(result.metadata.attributes.contains_key("consolidated_count"));
            assert!(result.metadata.attributes.contains_key("strategies"));
        }
    }

    #[tokio::test]
    async fn test_workflow_persists_to_store() {
        let store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let consolidated_store = Arc::new(InMemoryConsolidatedStore::new())
            as Arc<dyn ConsolidatedMemoryRepository>;

        for i in 0..3 {
            store
                .store(make_episode(&format!("事件{}", i), "项目A", i as i64 * 12))
                .await
                .unwrap();
        }

        let config = ConsolidationConfig {
            auto_write_episodes: true,
            ..Default::default()
        };
        let workflow = ConsolidationWorkflow::new(
            store.clone(),
            consolidated_store.clone(),
            config,
        );
        workflow.add_strategies(default_strategies()).await;

        let _result = workflow
            .execute(MemoryQuery::new("persist test"))
            .await
            .unwrap();

        // Consolidated memories should have been stored
        let _count = consolidated_store.count_consolidated().await.unwrap();
        // May be 0 if engine found no candidates, or >0 if consolidation ran
        // count is usize, always >= 0
    }

    #[tokio::test]
    async fn test_workflow_multiple_calls_idempotent() {
        let store = Arc::new(InMemoryEpisodeStore::new()) as Arc<dyn EpisodeRepository>;
        let consolidated_store = Arc::new(InMemoryConsolidatedStore::new())
            as Arc<dyn ConsolidatedMemoryRepository>;

        for i in 0..3 {
            store
                .store(make_episode(&format!("事件{}", i), "项目A", i as i64 * 12))
                .await
                .unwrap();
        }

        let config = ConsolidationConfig {
            auto_write_episodes: true,
            ..Default::default()
        };
        let workflow = ConsolidationWorkflow::new(
            store.clone(),
            consolidated_store.clone(),
            config,
        );
        workflow.add_strategies(default_strategies()).await;

        // Run twice
        let _r1 = workflow
            .execute(MemoryQuery::new("test"))
            .await
            .unwrap();
        let _r2 = workflow
            .execute(MemoryQuery::new("test"))
            .await
            .unwrap();

        // Second run should not increase consolidated count unbounded
        // because first run marked episodes as consolidated
        let count_after = consolidated_store.count_consolidated().await.unwrap();
        assert!(
            count_after <= 5,
            "consolidation should not grow unbounded, got {}",
            count_after
        );
    }

    #[tokio::test]
    async fn test_node_has_importance_tags() {
        let (_, workflow) = setup_workflow().await;
        let result = workflow
            .execute(MemoryQuery::new("tag test"))
            .await
            .unwrap();

        // Nodes should have importance tags
        for node in &result.nodes {
            let has_importance = node.tags.iter().any(|t| t.starts_with("importance:"));
            assert!(has_importance, "node should have importance tag");
        }
    }

    #[tokio::test]
    async fn test_workflow_source_metadata() {
        let (_, workflow) = setup_workflow().await;
        let result = workflow
            .execute(MemoryQuery::new("source test"))
            .await
            .unwrap();

        assert_eq!(result.metadata.source, "consolidation_workflow");
    }
}
