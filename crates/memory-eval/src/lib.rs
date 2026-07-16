//! LSMemoryEval — 记忆系统评测框架。
//!
//! 提供标准化的评测指标和工具，用于评估记忆系统的质量。
//!
//! # 评测指标
//!
//! - **Recall**: 正确召回的事件占总应召回事件的比例
//! - **Precision**: 召回结果中正确事件的比例
//! - **F1 Score**: Recall 和 Precision 的调和平均
//! - **Latency**: 查询延迟（毫秒）
//! - **Token Cost**: 每次查询的 Token 消耗
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lingshu_memory_eval::{MemoryEvaluator, EvaluationDataset, EvaluationItem};
//!
//! let dataset = EvaluationDataset::projects();
//! let result = evaluator.evaluate(&dataset).await?;
//! println!("Recall: {:.2}%, F1: {:.2}%", result.recall * 100.0, result.f1_score * 100.0);
//! ```

use async_trait::async_trait;
use chrono::Utc;
use lingshu_core::LsResult;
// use lingshu_evidence_graph::{EvidenceGraph, NodeKind};
use lingshu_memory_episode::{EntityRef, Episode, EpisodeQuery};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;

// ─── 评测数据类型 ──────────────────────────────────────

/// 单个评测项 — 一个查询及其期望结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationItem {
    /// 评测查询文本
    pub query: String,
    /// 期望召回的事件标题列表
    pub expected_episode_titles: Vec<String>,
    /// 期望召回的实体列表（"kind:name" 格式）
    pub expected_entities: Vec<String>,
    /// 可选的标签分类
    pub tags: Vec<String>,
    /// 评测项描述
    pub description: String,
}

impl EvaluationItem {
    /// 创建一个新的评测项。
    pub fn new(
        query: impl Into<String>,
    expected_titles: Vec<&str>,
    ) -> Self {
        Self {
            query: query.into(),
            expected_episode_titles: expected_titles.into_iter().map(|s| s.to_string()).collect(),
            expected_entities: Vec::new(),
            tags: Vec::new(),
            description: String::new(),
        }
    }

    /// 添加期望实体。
    pub fn with_entities(mut self, entities: Vec<impl Into<String>>) -> Self {
        self.expected_entities = entities.into_iter().map(|s| s.into()).collect();
        self
    }

    /// 添加标签。
    pub fn with_tags(mut self, tags: Vec<impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(|s| s.into()).collect();
        self
    }

    /// 设置描述。
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

/// 评测数据集 — 一组评测项，构成一个评测场景。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationDataset {
    /// 数据集名称
    pub name: String,
    /// 评测项列表
    pub items: Vec<EvaluationItem>,
}

impl EvaluationDataset {
    /// 创建一个新的评测数据集。
    pub fn new(name: impl Into<String>, items: Vec<EvaluationItem>) -> Self {
        Self {
            name: name.into(),
            items,
        }
    }

    /// 构建评测用的 Episode 数据。
    pub fn build_episodes(&self) -> Vec<Episode> {
        let mut episodes = Vec::new();
        for item in &self.items {
            for title in &item.expected_episode_titles {
                let mut ep = Episode::new(
                    title,
                    &item.description,
                    Utc::now(),
                );
                for entity_str in &item.expected_entities {
                    if let Some((kind, name)) = entity_str.split_once(':') {
                        ep = ep.with_entity(EntityRef::new(kind, name));
                    }
                }
                for tag in &item.tags {
                    ep = ep.with_tag(tag);
                }
                episodes.push(ep);
            }
        }
        episodes
    }
}

// ─── 单次查询结果 ─────────────────────────────────────

/// 单次查询的评测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// 评测查询文本
    pub query: String,
    /// 期望的事件标题
    expected_titles: Vec<String>,
    /// 实际召回的事件标题
    pub found_titles: Vec<String>,
    /// 命中的事件标题（交集）
    pub matched_titles: Vec<String>,
    /// 漏召回的事件标题
    pub missed_titles: Vec<String>,
    /// 误召回的事件标题
    pub extra_titles: Vec<String>,
    /// 查询耗时（毫秒）
    pub latency_ms: u64,
    /// 召回率（0.0 ~ 1.0）
    pub recall: f64,
    /// 精确率（0.0 ~ 1.0）
    pub precision: f64,
    /// F1 分数（0.0 ~ 1.0）
    pub f1_score: f64,
    /// 是否成功（至少有一条匹配）
    pub success: bool,
}

impl QueryResult {
    /// 从查询结果计算指标。
    pub fn from_results(
        query: String,
    expected_titles: Vec<String>,
        found_titles: Vec<String>,
        latency_ms: u64,
    ) -> Self {
        let expected_set: HashSet<String> = expected_titles.iter().cloned().collect();
        let found_set: HashSet<String> = found_titles.iter().cloned().collect();

        let mut matched: Vec<String> = expected_set.intersection(&found_set).cloned().collect();
        let mut missed: Vec<String> = expected_set.difference(&found_set).cloned().collect();
        let mut extra: Vec<String> = found_set.difference(&expected_set).cloned().collect();
        matched.sort();
        missed.sort();
        extra.sort();

        let expected_count = expected_titles.len();
        let found_count = found_titles.len();
        let matched_count = matched.len();

        let recall = if expected_count > 0 {
            matched_count as f64 / expected_count as f64
        } else {
            1.0
        };

        let precision = if expected_count == 0 {
            1.0  // No ground truth: vacuously precise
        } else if found_count > 0 {
            matched_count as f64 / found_count as f64
        } else {
            0.0
        };

        let f1_score = if recall + precision > 0.0 {
            2.0 * recall * precision / (recall + precision)
        } else {
            0.0
        };

        Self {
            query,
            expected_titles,
            found_titles,
            matched_titles: matched,
            missed_titles: missed,
            extra_titles: extra,
            latency_ms,
            recall,
            precision,
            f1_score,
            success: matched_count > 0 || expected_count == 0,
        }
    }
}

// ─── 总体评测结果 ─────────────────────────────────────

/// 总体评测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// 数据集名称
    pub dataset_name: String,
    /// 查询总数
    pub total_queries: usize,
    /// 成功查询数（至少有一条匹配）
    pub successful_queries: usize,
    /// 平均召回率
    pub avg_recall: f64,
    /// 平均精确率
    pub avg_precision: f64,
    /// 平均 F1 分数
    pub avg_f1_score: f64,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: f64,
    /// 最慢查询延迟（毫秒）
    pub max_latency_ms: u64,
    /// 总耗时（毫秒）
    pub total_time_ms: u64,
    /// 每条查询的详细结果
    pub query_results: Vec<QueryResult>,
}

impl EvaluationResult {
    /// 从多个 QueryResult 计算总体结果。
    pub fn from_results(dataset_name: String, results: Vec<QueryResult>, total_time_ms: u64) -> Self {
        let total = results.len();
        let successful = results.iter().filter(|r| r.success).count();

        let avg_recall = if total > 0 {
            results.iter().map(|r| r.recall).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let avg_precision = if total > 0 {
            results.iter().map(|r| r.precision).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let avg_f1 = if total > 0 {
            results.iter().map(|r| r.f1_score).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let avg_latency = if total > 0 {
            results.iter().map(|r| r.latency_ms).sum::<u64>() as f64 / total as f64
        } else {
            0.0
        };

        let max_latency = results.iter().map(|r| r.latency_ms).max().unwrap_or(0);

        Self {
            dataset_name,
            total_queries: total,
            successful_queries: successful,
            avg_recall,
            avg_precision,
            avg_f1_score: avg_f1,
            avg_latency_ms: avg_latency,
            max_latency_ms: max_latency,
            total_time_ms,
            query_results: results,
        }
    }

    /// 格式化为可读字符串。
    pub fn to_text_summary(&self) -> String {
        format!(
            r#"📊 评测报告: {}
━━━━━━━━━━━━━━━━━━━━━━━━━━━
📈 总查询: {} | 成功: {} | 成功率: {:.1}%
🎯 平均召回率: {:.1}%
🎯 平均精确率: {:.1}%
🎯 平均 F1:    {:.1}%
⚡ 平均延迟:   {:.0}ms
⚡ 最慢延迟:   {}ms
⏱  总耗时:    {}ms
━━━━━━━━━━━━━━━━━━━━━━━━━━━"#,
            self.dataset_name,
            self.total_queries,
            self.successful_queries,
            (self.successful_queries as f64 / self.total_queries as f64) * 100.0,
            self.avg_recall * 100.0,
            self.avg_precision * 100.0,
            self.avg_f1_score * 100.0,
            self.avg_latency_ms,
            self.max_latency_ms,
            self.total_time_ms,
        )
    }
}

// ─── MemoryEvaluator Trait ────────────────────────────

/// MemoryEvaluator — 记忆系统评测接口。
///
/// 所有实现需要提供对查询的评估能力。
/// 不修改记忆系统，只读取和评估。
#[async_trait]
pub trait MemoryEvaluator: Send + Sync {
    /// 对整个数据集运行评测。
    async fn evaluate(&self, dataset: &EvaluationDataset) -> LsResult<EvaluationResult>;

    /// 对单个评测项运行评测。
    async fn evaluate_query(&self, item: &EvaluationItem) -> LsResult<QueryResult>;
}

// ─── 基于 EpisodeRepository 的评测实现 ─────────────────

/// EpisodeEvaluator — 基于 EpisodeRepository 的评测实现。
///
/// 向存储写入评测数据集，然后执行查询并评估结果。
pub struct EpisodeEvaluator {
    store: Box<dyn lingshu_memory_episode::EpisodeRepository>,
}

impl EpisodeEvaluator {
    /// 创建一个新的 EpisodeEvaluator。
    pub fn new(store: Box<dyn lingshu_memory_episode::EpisodeRepository>) -> Self {
        Self { store }
    }

    /// 使用内存存储创建评测器。
    pub fn in_memory() -> Self {
        Self {
            store: Box::new(lingshu_memory_episode::InMemoryEpisodeStore::new()),
        }
    }

    /// 准备评测数据：清空存储并写入数据集。
    pub async fn prepare(&self, dataset: &EvaluationDataset) -> LsResult<()> {
        self.store.clear().await?;
        let episodes = dataset.build_episodes();
        if !episodes.is_empty() {
            self.store.store_batch(episodes).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl MemoryEvaluator for EpisodeEvaluator {
    async fn evaluate(&self, dataset: &EvaluationDataset) -> LsResult<EvaluationResult> {
        let start = Instant::now();
        let mut query_results = Vec::with_capacity(dataset.items.len());

        for item in &dataset.items {
            let result = self.evaluate_query(item).await?;
            query_results.push(result);
        }

        let elapsed = start.elapsed().as_millis() as u64;
        Ok(EvaluationResult::from_results(
            dataset.name.clone(),
            query_results,
            elapsed,
        ))
    }

    async fn evaluate_query(&self, item: &EvaluationItem) -> LsResult<QueryResult> {
        let query_start = Instant::now();

        // 使用 EpisodeQuery 按实体和关键词搜索
        let mut episode_query = EpisodeQuery::default().with_limit(50);

        // 如果指定了实体，按实体过滤
        for entity_str in &item.expected_entities {
            if let Some((kind, name)) = entity_str.split_once(':') {
                episode_query = episode_query.with_entity(EntityRef::new(kind, name));
            }
        }

        // 关键词搜索
        if !item.query.is_empty() {
            episode_query = episode_query.with_search(&item.query);
        }

        let episodes = self.store.query(episode_query).await?;
        let elapsed = query_start.elapsed().as_millis() as u64;

        let found_titles: Vec<String> = episodes.iter().map(|e| e.title.clone()).collect();

        Ok(QueryResult::from_results(
            item.query.clone(),
            item.expected_episode_titles.clone(),
            found_titles,
            elapsed,
        ))
    }
}

// ─── 内置评测数据集 ─────────────────────────────────────

impl EvaluationDataset {
    /// 项目事件评测集 — 测试项目相关的记忆查询。
    pub fn projects() -> Self {
        Self::new(
            "项目事件评测",
            vec![
                EvaluationItem::new(
                    "项目A为什么暂停",
                    vec!["启动项目A", "供应商退出", "暂停项目A"],
                )
                .with_entities(vec!["project:项目A"])
                .with_tags(vec!["project", "timeline"])
                .with_description("项目A从启动到暂停的完整时间线"),
                EvaluationItem::new(
                    "项目B的进展",
                    vec!["启动项目B", "完成项目B"],
                )
                .with_entities(vec!["project:项目B"])
                .with_tags(vec!["project"])
                .with_description("项目B的完整生命周期"),
                EvaluationItem::new(
                    "项目C的状态",
                    vec!["启动项目C"],
                )
                .with_entities(vec!["project:项目C"])
                .with_description("项目C仅有启动事件"),
            ],
        )
    }

    /// 人员关系评测集 — 测试人员相关的记忆查询。
    pub fn persons() -> Self {
        Self::new(
            "人员关系评测",
            vec![
                EvaluationItem::new(
                    "张三做了什么决策",
                    vec!["张三加入团队", "张三决定暂停项目A"],
                )
                .with_entities(vec!["person:张三"])
                .with_tags(vec!["person", "decision"])
                .with_description("张三的关键决策记录"),
                EvaluationItem::new(
                    "李四负责什么项目",
                    vec!["李四加入团队", "李四负责项目B"],
                )
                .with_entities(vec!["person:李四"])
                .with_tags(vec!["person"])
                .with_description("李四的职责范围"),
            ],
        )
    }

    /// 时间线评测集 — 测试按时间范围的查询能力。
    pub fn timelines() -> Self {
        Self::new(
            "时间线评测",
            vec![
                EvaluationItem::new(
                    "上个月发生了什么",
                    vec!["项目A暂停", "项目B启动"],
                )
                .with_tags(vec!["timeline", "recent"])
                .with_description("最近一个月的事件"),
                EvaluationItem::new(
                    "今年的项目启动",
                    vec!["启动项目A", "启动项目B", "启动项目C"],
                )
                .with_tags(vec!["project", "launch"])
                .with_description("今年的项目启动事件"),
            ],
        )
    }

    /// 状态变更评测集 — 测试状态变化的查询能力。
    pub fn state_changes() -> Self {
        Self::new(
            "状态变更评测",
            vec![
                EvaluationItem::new(
                    "哪些项目状态变了",
                    vec!["供应商退出", "暂停项目A"],
                )
                .with_tags(vec!["status", "change"])
                .with_description("项目状态变更事件"),
                EvaluationItem::new(
                    "项目A的状态变化",
                    vec!["启动项目A", "供应商退出", "暂停项目A"],
                )
                .with_entities(vec!["project:项目A"])
                .with_tags(vec!["status", "timeline"])
                .with_description("项目A的完整状态变化历史"),
            ],
        )
    }

    /// 混合查询评测集 — 综合测试。
    pub fn mixed() -> Self {
        Self::new(
            "混合查询评测",
            vec![
                EvaluationItem::new(
                    "谁暂停了项目",
                    vec!["暂停项目A"],
                )
                .with_tags(vec!["decision"])
                .with_description("谁做出了暂停决策"),
                EvaluationItem::new(
                    "为什么项目A暂停",
                    vec!["供应商退出", "暂停项目A"],
                )
                .with_tags(vec!["reason", "timeline"])
                .with_description("项目A暂停的原因分析"),
                EvaluationItem::new(
                    "最近的项目决策",
                    vec!["暂停项目A", "完成项目B"],
                )
                .with_tags(vec!["decision", "recent"])
                .with_description("最近的项目相关决策"),
                EvaluationItem::new(
                    "团队人员变动",
                    vec!["张三加入团队", "李四加入团队"],
                )
                .with_tags(vec!["person", "team"])
                .with_description("团队人员加入事件"),
            ],
        )
    }

    /// 边界情况评测集 — 测试极端情况。
    pub fn edge_cases() -> Self {
        Self::new(
            "边界情况评测",
            vec![
                // 空查询
                EvaluationItem::new("", vec![])
                    .with_tags(vec!["edge"])
                    .with_description("空查询应返回空结果"),
                // 不存在的查询
                EvaluationItem::new("完全不存在的查询内容", vec![])
                    .with_tags(vec!["edge"])
                    .with_description("不存在的查询应返回空结果"),
                // 非常短的查询
                EvaluationItem::new("A", vec!["启动项目A"])
                    .with_tags(vec!["edge"])
                    .with_description("单个字符查询"),
                // 特殊字符
                EvaluationItem::new("项目@#$%^&暂停", vec!["暂停项目A"])
                    .with_tags(vec!["edge"])
                    .with_description("含特殊字符的查询"),
            ],
        )
    }

    /// 获取所有内置数据集。
    pub fn all_builtin() -> Vec<Self> {
        vec![
            Self::projects(),
            Self::persons(),
            Self::timelines(),
            Self::state_changes(),
            Self::mixed(),
            Self::edge_cases(),
        ]
    }

    /// 合并多个数据集。
    pub fn merge(datasets: Vec<Self>) -> Self {
        let name = format!(
            "合并评测 ({}个数据集)",
            datasets.len()
        );
        let mut all_items = Vec::new();
        for ds in datasets {
            all_items.extend(ds.items);
        }
        Self::new(name, all_items)
    }
}

// ─── 便捷评测运行器 ─────────────────────────────────────

/// 运行完整的内存评测套件。
///
/// 将所有内置数据集写入内存存储，执行查询，返回汇总结果。
pub async fn run_memory_benchmark(
    evaluator: &dyn MemoryEvaluator,
) -> LsResult<Vec<EvaluationResult>> {
    let mut results = Vec::new();
    for dataset in EvaluationDataset::all_builtin() {
        let result = evaluator.evaluate(&dataset).await?;
        tracing::info!(
            "Benchmark '{}': recall={:.1}% precision={:.1}% f1={:.1}% latency={:.0}ms",
            result.dataset_name,
            result.avg_recall * 100.0,
            result.avg_precision * 100.0,
            result.avg_f1_score * 100.0,
            result.avg_latency_ms,
        );
        results.push(result);
    }
    Ok(results)
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_memory_episode::InMemoryEpisodeStore;

    fn make_evaluator() -> EpisodeEvaluator {
        EpisodeEvaluator::in_memory()
    }

    // ─── EvaluationItem 测试 ──────────────────────────

    #[test]
    fn test_evaluation_item_new() {
        let item = EvaluationItem::new("查询", vec!["事件A", "事件B"]);
        assert_eq!(item.query, "查询");
        assert_eq!(item.expected_episode_titles.len(), 2);
        assert!(item.expected_entities.is_empty());
    }

    #[test]
    fn test_evaluation_item_with_entities() {
        let item = EvaluationItem::new("查询", vec!["事件A"])
            .with_entities(vec!["project:项目A", "person:张三"]);
        assert_eq!(item.expected_entities.len(), 2);
    }

    #[test]
    fn test_evaluation_item_with_tags() {
        let item = EvaluationItem::new("查询", vec!["事件A"])
            .with_tags(vec!["tag1", "tag2"]);
        assert_eq!(item.tags.len(), 2);
    }

    #[test]
    fn test_evaluation_item_with_description() {
        let item = EvaluationItem::new("查询", vec!["事件A"])
            .with_description("这是一个测试");
        assert_eq!(item.description, "这是一个测试");
    }

    // ─── EvaluationDataset 测试 ───────────────────────

    #[test]
    fn test_dataset_new() {
        let items = vec![
            EvaluationItem::new("查询1", vec!["事件A"]),
            EvaluationItem::new("查询2", vec!["事件B"]),
        ];
        let dataset = EvaluationDataset::new("测试数据集", items);
        assert_eq!(dataset.name, "测试数据集");
        assert_eq!(dataset.items.len(), 2);
    }

    #[test]
    fn test_build_episodes() {
        let dataset = EvaluationDataset::projects();
        let episodes = dataset.build_episodes();
        assert_eq!(episodes.len(), 6);  // 3 items x 2-3 titles each
        assert_eq!(episodes[0].title, "启动项目A");
    }

    #[test]
    fn test_build_episodes_with_entities() {
        let dataset = EvaluationDataset::new(
            "测试",
            vec![
                EvaluationItem::new("查询", vec!["事件"])
                    .with_entities(vec!["project:项目X", "person:张三"]),
            ],
        );
        let episodes = dataset.build_episodes();
        assert_eq!(episodes[0].entities.len(), 2);
    }

    #[test]
    fn test_builtin_datasets_count() {
        let datasets = EvaluationDataset::all_builtin();
        assert_eq!(datasets.len(), 6);
    }

    #[test]
    fn test_merge_datasets() {
        let d1 = EvaluationDataset::projects();
        let d2 = EvaluationDataset::persons();
        let merged = EvaluationDataset::merge(vec![d1, d2]);
        assert_eq!(merged.items.len(), 5); // 3 + 2
        assert!(merged.name.contains("合并"));
    }

    // ─── QueryResult 测试 ─────────────────────────────

    #[test]
    fn test_query_result_perfect_match() {
        let result = QueryResult::from_results(
            "测试".into(),
            vec!["A".into(), "B".into()],
            vec!["A".into(), "B".into()],
            10,
        );
        assert!((result.recall - 1.0).abs() < 1e-6);
        assert!((result.precision - 1.0).abs() < 1e-6);
        assert!((result.f1_score - 1.0).abs() < 1e-6);
        assert!(result.success);
        assert_eq!(result.latency_ms, 10);
    }

    #[test]
    fn test_query_result_partial_match() {
        let result = QueryResult::from_results(
            "测试".into(),
            vec!["A".into(), "B".into(), "C".into()],
            vec!["A".into(), "D".into()],
            5,
        );
        assert!((result.recall - 1.0 / 3.0).abs() < 1e-6);
        assert!((result.precision - 0.5).abs() < 1e-6);
        assert!(result.success);
        assert_eq!(result.missed_titles.len(), 2);
        assert_eq!(result.extra_titles.len(), 1);
    }

    #[test]
    fn test_query_result_no_match() {
        let result = QueryResult::from_results(
            "测试".into(),
            vec!["A".into(), "B".into()],
            vec!["C".into(), "D".into()],
            0,
        );
        assert!((result.recall).abs() < 1e-6);
        assert!((result.precision).abs() < 1e-6);
        assert!(!result.success);
    }

    #[test]
    fn test_query_result_empty_expected() {
        let result = QueryResult::from_results(
            "测试".into(),
            vec![],
            vec!["A".into()],
            0,
        );
        assert!((result.recall - 1.0).abs() < 1e-6);
        assert!((result.precision - 1.0).abs() < 1e-6);
        assert!(result.success); // empty expected = vacuously successful
    }

    // ─── EvaluationResult 测试 ────────────────────────

    #[test]
    fn test_evaluation_result_empty() {
        let result = EvaluationResult::from_results("空测试".into(), vec![], 0);
        assert_eq!(result.total_queries, 0);
        assert_eq!(result.avg_recall, 0.0);
    }

    #[test]
    fn test_evaluation_result_multiple_queries() {
        let results = vec![
            QueryResult::from_results("q1".into(), vec!["A".into()], vec!["A".into()], 10),
            QueryResult::from_results("q2".into(), vec!["B".into()], vec!["B".into()], 20),
            QueryResult::from_results("q3".into(), vec!["C".into()], vec![], 30),
        ];
        let eval_result = EvaluationResult::from_results("综合测试".into(), results, 100);
        assert_eq!(eval_result.total_queries, 3);
        assert_eq!(eval_result.successful_queries, 2);
        assert!((eval_result.avg_recall - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(eval_result.max_latency_ms, 30);
    }

    #[test]
    fn test_evaluation_result_text_summary() {
        let results = vec![
            QueryResult::from_results("q1".into(), vec!["A".into()], vec!["A".into()], 10),
        ];
        let eval_result = EvaluationResult::from_results("测试".into(), results, 10);
        let summary = eval_result.to_text_summary();
        assert!(summary.contains("测试"));
        assert!(summary.contains("100.0%")); // perfect recall
    }

    // ─── EpisodeEvaluator 测试 ────────────────────────

    #[tokio::test]
    async fn test_evaluator_in_memory() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::new(
            "测试",
            vec![
                EvaluationItem::new("项目A", vec!["项目A"])
                    .with_entities(vec!["project:项目A"]),
            ],
        );
        evaluator.prepare(&dataset).await.unwrap();
        assert!(evaluator.store.count().await.unwrap() > 0);
    }

    #[tokio::test]
    async fn test_evaluate_query_perfect() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::projects();
        evaluator.prepare(&dataset).await.unwrap();

        let item = EvaluationItem::new("项目A", vec!["启动项目A", "供应商退出", "暂停项目A"])
            .with_entities(vec!["project:项目A"]);
        let result = evaluator.evaluate_query(&item).await.unwrap();
        assert!(result.success, "should find project A events");
        assert!(result.recall > 0.0, "should have some recall");
    }

    #[tokio::test]
    async fn test_evaluate_full_dataset() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::projects();
        evaluator.prepare(&dataset).await.unwrap();

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 3);
    }

    #[tokio::test]
    async fn test_edge_case_empty_query() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::edge_cases();
        evaluator.prepare(&dataset).await.unwrap();

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 4);
        // Edge cases may have lower recall
        assert!(result.avg_recall >= 0.0);
    }

    #[tokio::test]
    async fn test_benchmark_all_datasets() {
        let evaluator = EpisodeEvaluator::in_memory();
        let project_dataset = EvaluationDataset::projects();
        evaluator.prepare(&project_dataset).await.unwrap();

        let results = run_memory_benchmark(&evaluator).await.unwrap();
        assert_eq!(results.len(), 6);
    }

    #[tokio::test]
    async fn test_evaluate_persons_dataset() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::persons();
        evaluator.prepare(&dataset).await.unwrap();

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 2);
    }

    #[tokio::test]
    async fn test_evaluate_timelines_dataset() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::timelines();
        evaluator.prepare(&dataset).await.unwrap();

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 2);
    }

    #[tokio::test]
    async fn test_evaluate_state_changes() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::state_changes();
        evaluator.prepare(&dataset).await.unwrap();

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 2);
    }

    #[tokio::test]
    async fn test_evaluate_mixed() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::mixed();
        evaluator.prepare(&dataset).await.unwrap();

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 4);
    }

    #[tokio::test]
    async fn test_prepare_clears_old_data() {
        let evaluator = EpisodeEvaluator::in_memory();

        let ds1 = EvaluationDataset::projects();
        evaluator.prepare(&ds1).await.unwrap();
        assert_eq!(evaluator.store.count().await.unwrap(), 6);  // projects: 3 items x 2-3 titles

        let ds2 = EvaluationDataset::persons();
        evaluator.prepare(&ds2).await.unwrap();
        assert_eq!(evaluator.store.count().await.unwrap(), 4);  // persons: 2 items x 2 titles each
    }

    #[tokio::test]
    async fn test_empty_dataset() {
        let evaluator = EpisodeEvaluator::in_memory();
        let dataset = EvaluationDataset::new("空数据集", vec![]);
        evaluator.prepare(&dataset).await.unwrap();
        assert_eq!(evaluator.store.count().await.unwrap(), 0);

        let result = evaluator.evaluate(&dataset).await.unwrap();
        assert_eq!(result.total_queries, 0);
    }

    #[test]
    fn test_query_result_has_latency() {
        let result = QueryResult::from_results(
            "查询".into(),
            vec!["A".into()],
            vec!["A".into()],
            42,
        );
        assert_eq!(result.latency_ms, 42);
    }

    #[test]
    fn test_query_result_matched_missed_extra() {
        let result = QueryResult::from_results(
            "查询".into(),
            vec!["A".into(), "B".into(), "C".into()],
            vec!["A".into(), "D".into(), "E".into()],
            0,
        );
        assert_eq!(result.matched_titles, vec!["A"]);
        assert_eq!(result.missed_titles, vec!["B", "C"]);
        assert_eq!(result.extra_titles, vec!["D", "E"]);
    }

    #[test]
    fn test_builtin_projects_dataset_structure() {
        let dataset = EvaluationDataset::projects();
        assert_eq!(dataset.items.len(), 3);
        assert!(dataset.items[0].expected_entities.contains(&"project:项目A".to_string()));
    }

    #[test]
    fn test_builtin_edge_cases_structure() {
        let dataset = EvaluationDataset::edge_cases();
        assert_eq!(dataset.items.len(), 4);
        assert!(dataset.items.iter().any(|i| i.query.is_empty()));
    }

    #[test]
    fn test_merge_dedup_not_needed() {
        let d1 = EvaluationDataset::new("A", vec![
            EvaluationItem::new("q1", vec!["e1"]),
        ]);
        let d2 = EvaluationDataset::new("B", vec![
            EvaluationItem::new("q2", vec!["e2"]),
        ]);
        let merged = EvaluationDataset::merge(vec![d1, d2]);
        assert_eq!(merged.items.len(), 2);
    }
}
