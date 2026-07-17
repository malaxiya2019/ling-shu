//! RankingScorer — Memory Retrieval Ranking 系统。
//!
//! 对 EvidenceGraph 中的节点按查询相关性排序，使合并器
//! 在加权合并时能基于更有信号的质量评分做决策。
//!
//! # 架构
//!
//! ```text
//! 查询文本 + EvidenceGraph
//!         │
//!    ┌────┴────┐
//!    │         │
//! TfIdfScorer  RecencyScorer
//!    │         │
//!    └────┬────┘
//!         ▼
//!  CompositeScorer (加权组合)
//!         │
//!         ▼
//!  各 Node.relevance_score 更新
//!         │
//!         ▼
//!  WeightedGraphMerger 使用评分优化合并决策
//! ```

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::{EvidenceGraph, Node, NodeId};

// ═══════════════════════════════════════════════════
// RankingScorer trait
// ═══════════════════════════════════════════════════

/// 相关性评分器 — 计算节点与查询之间的相关性分数 (0.0 ~ 1.0)。
pub trait RankingScorer: Send + Sync + std::fmt::Debug {
    /// 评分器名称。
    fn name(&self) -> &str;

    /// 对单个节点评分。
    fn score(&self, query: &str, node: &Node) -> f64;

    /// 对图所有节点批量评分，返回 (node_id → score) 映射。
    fn score_graph(&self, query: &str, graph: &EvidenceGraph) -> HashMap<NodeId, f64> {
        graph.nodes.iter().map(|n| (n.id, self.score(query, n))).collect()
    }

    /// Clone this scorer into a boxed trait object.
    fn clone_box(&self) -> Box<dyn RankingScorer>;
}

// ═══════════════════════════════════════════════════
// TfIdfScorer — 文本相关性
// ═══════════════════════════════════════════════════

/// 基于 TF-IDF 的文本相关性评分器。
/// 计算查询词与节点标题/描述的词频重叠度。
/// 使用简单的词袋模型 + IDF 加权。
#[derive(Debug)]
pub struct TfIdfScorer {
    /// IDF 语料库（词 → IDF 值），为空时使用均匀权重
    idf_corpus: HashMap<String, f64>,
    /// 标题权重 (0.0 ~ 1.0)
    title_weight: f64,
    /// 描述权重 (0.0 ~ 1.0)
    description_weight: f64,
}

impl TfIdfScorer {
    /// 创建默认 TF-IDF 评分器。
    pub fn new() -> Self {
        Self {
            idf_corpus: HashMap::new(),
            title_weight: 0.6,
            description_weight: 0.4,
        }
    }

    /// 从现有图语料库构建 IDF。
    pub fn with_corpus(mut self, graphs: &[&EvidenceGraph]) -> Self {
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        let mut total_docs = 0usize;

        for graph in graphs {
            for node in &graph.nodes {
                total_docs += 1;
                let mut words: std::collections::HashSet<String> = std::collections::HashSet::new();
                for w in tokenize(&node.title) { words.insert(w); }
                for w in tokenize(&node.description) { words.insert(w); }
                for w in words {
                    *doc_freq.entry(w).or_insert(0) += 1;
                }
            }
        }

        if total_docs > 0 {
            for (word, df) in &doc_freq {
                let idf = ((total_docs as f64) / (*df as f64)).ln() + 1.0;
                self.idf_corpus.insert(word.clone(), idf);
            }
        }

        self
    }

    /// 设置标题与描述的权重比例。
    pub fn with_weights(mut self, title_weight: f64, description_weight: f64) -> Self {
        let total = title_weight + description_weight;
        if total > 0.0 {
            self.title_weight = title_weight / total;
            self.description_weight = description_weight / total;
        }
        self
    }
}

impl Clone for TfIdfScorer {
    fn clone(&self) -> Self {
        Self {
            idf_corpus: self.idf_corpus.clone(),
            title_weight: self.title_weight,
            description_weight: self.description_weight,
        }
    }
}

impl Default for TfIdfScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl RankingScorer for TfIdfScorer {
    fn name(&self) -> &str {
        "tfidf"
    }

    fn score(&self, query: &str, node: &Node) -> f64 {
        if query.is_empty() {
            return 0.0;
        }

        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return 0.0;
        }

        let title_tokens = tokenize(&node.title);
        let desc_tokens = tokenize(&node.description);

        // 计算 TF-IDF 加权分数
        let title_score = self.tfidf_score(&query_tokens, &title_tokens);
        let desc_score = self.tfidf_score(&query_tokens, &desc_tokens);

        let combined = title_score * self.title_weight + desc_score * self.description_weight;

        // 无匹配词时直接返回 0
        if combined <= 0.0 {
            return 0.0;
        }

        // Sigmoid 压缩到 0.0~1.0
        let sigmoid = |x: f64| 1.0 / (1.0 + (-x * 3.0).exp());
        sigmoid(combined)
    }

    fn clone_box(&self) -> Box<dyn RankingScorer> {
        Box::new(self.clone())
    }
}

impl TfIdfScorer {
    fn tfidf_score(&self, query_tokens: &[String], doc_tokens: &[String]) -> f64 {
        if doc_tokens.is_empty() {
            return 0.0;
        }

        // 词频统计
        let mut tf: HashMap<&str, usize> = HashMap::new();
        for t in doc_tokens {
            *tf.entry(t).or_insert(0) += 1;
        }

        let max_tf = tf.values().copied().max().unwrap_or(1) as f64;

        let mut score = 0.0;
        for qt in query_tokens {
            let term_tf = tf.get(qt.as_str()).copied().unwrap_or(0) as f64;
            if term_tf > 0.0 {
                // 归一化词频
                let norm_tf = term_tf / max_tf;
                // IDF 加权
                let idf = self.idf_corpus.get(qt).copied().unwrap_or(1.0);
                score += norm_tf * idf;
            }
        }

        score / (query_tokens.len() as f64)
    }
}

// ═══════════════════════════════════════════════════
// RecencyScorer — 时效性评分
// ═══════════════════════════════════════════════════

/// 基于时间衰减的时效性评分器。
///
/// 越近期的事件评分越高，使用指数衰减函数。
/// 无时间戳的节点使用默认低分。
#[derive(Debug, Clone)]
pub struct RecencyScorer {
    /// 半衰期（小时），超过此时间的节点评分减半
    half_life_hours: f64,
    /// 默认分数（无时间戳的节点）
    default_score: f64,
    /// 当前参考时间（None = 使用 Utc::now）
    reference_time: Option<DateTime<Utc>>,
}

impl RecencyScorer {
    /// 创建时效性评分器，默认半衰期 24 小时。
    pub fn new() -> Self {
        Self {
            half_life_hours: 24.0,
            default_score: 0.1,
            reference_time: None,
        }
    }

    /// 设置半衰期（小时）。
    pub fn with_half_life(mut self, hours: f64) -> Self {
        self.half_life_hours = hours.max(0.1);
        self
    }

    /// 设置参考时间（用于测试）。
    pub fn with_reference(mut self, time: DateTime<Utc>) -> Self {
        self.reference_time = Some(time);
        self
    }
}

impl Default for RecencyScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl RankingScorer for RecencyScorer {
    fn name(&self) -> &str {
        "recency"
    }

    fn score(&self, _query: &str, node: &Node) -> f64 {
        match node.timestamp {
            Some(ts) => {
                let now = self.reference_time.unwrap_or_else(Utc::now);
                let age_hours = (now - ts).num_minutes() as f64 / 60.0;
                if age_hours < 0.0 {
                    return 1.0; // 未来时间戳给满分
                }
                // 指数衰减: score = 2^(-age / half_life)
                2.0f64.powf(-age_hours / self.half_life_hours)
            }
            None => self.default_score,
        }
    }

    fn clone_box(&self) -> Box<dyn RankingScorer> {
        Box::new(self.clone())
    }
}

// ═══════════════════════════════════════════════════
// ConfidenceScorer — 置信度评分
// ═══════════════════════════════════════════════════

/// 基于节点自带置信度的评分器。
///
/// 直接使用 `node.confidence` 作为相关性评分。
#[derive(Debug, Clone)]
pub struct ConfidenceScorer;

impl RankingScorer for ConfidenceScorer {
    fn name(&self) -> &str {
        "confidence"
    }

    fn score(&self, _query: &str, node: &Node) -> f64 {
        node.confidence
    }

    fn clone_box(&self) -> Box<dyn RankingScorer> {
        Box::new(self.clone())
    }
}

// ═══════════════════════════════════════════════════
// CompositeScorer — 加权组合评分
// ═══════════════════════════════════════════════════

/// 加权组合多个评分器的结果。
///
/// 允许按业务需求自由组合不同维度的评分。
///
/// # 示例
///
/// ```rust,ignore
/// let scorer = CompositeScorer::new()
///     .add(Box::new(TfIdfScorer::new()), 0.5)
///     .add(Box::new(RecencyScorer::new()), 0.3)
///     .add(Box::new(ConfidenceScorer), 0.2);
/// let score = scorer.score("查询文本", &node);
/// ```
#[derive(Debug)]
pub struct CompositeScorer {
    scorers: Vec<(Box<dyn RankingScorer>, f64)>,
}

impl CompositeScorer {
    pub fn new() -> Self {
        Self { scorers: Vec::new() }
    }

    /// 添加一个评分器及其权重。
    pub fn add(mut self, scorer: Box<dyn RankingScorer>, weight: f64) -> Self {
        self.scorers.push((scorer, weight));
        self
    }

    /// 使用默认权重添加常用评分器。
    pub fn default_with_query() -> Self {
        Self {
            scorers: vec![
                (Box::new(TfIdfScorer::new()), 0.5),
                (Box::new(RecencyScorer::new()), 0.3),
                (Box::new(ConfidenceScorer), 0.2),
            ],
        }
    }
}

impl Clone for CompositeScorer {
    fn clone(&self) -> Self {
        Self {
            scorers: self.scorers.iter().map(|(s, w)| (s.clone_box(), *w)).collect(),
        }
    }
}

impl Default for CompositeScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl RankingScorer for CompositeScorer {
    fn name(&self) -> &str {
        "composite"
    }

    fn score(&self, query: &str, node: &Node) -> f64 {
        if self.scorers.is_empty() {
            return 0.0;
        }

        let total_weight: f64 = self.scorers.iter().map(|(_, w)| w).sum();
        if total_weight == 0.0 {
            return 0.0;
        }

        let weighted_sum: f64 = self.scorers
            .iter()
            .map(|(scorer, weight)| scorer.score(query, node) * weight)
            .sum();

        (weighted_sum / total_weight).clamp(0.0, 1.0)
    }

    fn clone_box(&self) -> Box<dyn RankingScorer> {
        Box::new(self.clone())
    }
}

// ═══════════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════════

/// 分词：将文本拆分为小写的 token 列表。
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .filter(|s| !s.is_empty() && s.len() >= 2)
        .map(|s| s.to_string())
        .collect()
}

// ═══════════════════════════════════════════════════
// 便捷函数
// ═══════════════════════════════════════════════════

/// 对 EvidenceGraph 中的节点执行相关性排序（原地修改 relevance_score）。
pub fn rank_graph_nodes(
    graph: &mut EvidenceGraph,
    query: &str,
    scorer: &dyn RankingScorer,
) {
    for node in &mut graph.nodes {
        node.relevance_score = scorer.score(query, node);
    }

    // 按相关性降序排列节点
    graph.nodes.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// 对 WeightedMergeResult 的图执行排序。
pub fn rank_merged_graph(
    result: &mut crate::merger::WeightedMergeResult,
    query: &str,
    scorer: &dyn RankingScorer,
) {
    rank_graph_nodes(&mut result.graph, query, scorer);
}
// ═══════════════════════════════════════════════════
// Clone 支持
// ═══════════════════════════════════════════════════

impl Clone for Box<dyn RankingScorer> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvidenceGraph, Node};
    use chrono::Duration;

    fn make_test_graph() -> EvidenceGraph {
        let mut graph = EvidenceGraph::empty("test query");
        graph.add_node(
            Node::event("Project kickoff", "Project A officially launched, all team members participated", Utc::now() - Duration::hours(2))
                .with_confidence(0.9)
        );
        graph.add_node(
            Node::event("Project paused", "Project A paused due to supplier issues", Utc::now() - Duration::hours(72))
                .with_confidence(0.8)
        );
        graph.add_node(
            Node::fact("Team composition", "Project A team has 5 engineers, 1 product manager")
                .with_confidence(0.7)
        );
        graph
    }

    // ── TfIdfScorer tests ──

    #[test]
    fn test_tfidf_empty_query() {
        let scorer = TfIdfScorer::new();
        let node = Node::fact("测试标题", "测试描述");
        assert_eq!(scorer.score("", &node), 0.0);
    }

    #[test]
    fn test_tfidf_exact_match() {
        let scorer = TfIdfScorer::new();
        let node = Node::event("Project kickoff", "Project A officially launched", Utc::now());
        let score = scorer.score("Project kickoff", &node);
        assert!(score > 0.5, "exact match should score high, got {}", score);
    }

    #[test]
    fn test_tfidf_no_match() {
        let scorer = TfIdfScorer::new();
        let node = Node::event("Weather report", "Today is sunny", Utc::now());
        let score = scorer.score("Project A launch", &node);
        assert!(score < 0.1, "no match should score near 0, got {}", score);
    }

    #[test]
    fn test_tfidf_partial_match() {
        let scorer = TfIdfScorer::new();
        let node = Node::event("Project paused", "Project A paused due to supplier issues", Utc::now());
        let score = scorer.score("Why was project paused", &node);
        assert!(score > 0.3, "partial match should score medium, got {}", score);
    }

    // ── RecencyScorer tests ──

    #[test]
    fn test_recency_recent() {
        let scorer = RecencyScorer::new().with_half_life(24.0);
        let node = Node::event("近期事件", "刚刚发生", Utc::now() - Duration::hours(1));
        let score = scorer.score("", &node);
        assert!(score > 0.9, "recent event should score high, got {}", score);
    }

    #[test]
    fn test_recency_old() {
        let scorer = RecencyScorer::new().with_half_life(24.0);
        let node = Node::event("旧事件", "很久以前", Utc::now() - Duration::days(30));
        let score = scorer.score("", &node);
        assert!(score < 0.5, "old event should score lower, got {}", score);
    }

    #[test]
    fn test_recency_no_timestamp() {
        let scorer = RecencyScorer::new();
        let node = Node::fact("事实", "没有时间戳");
        let score = scorer.score("", &node);
        assert!((score - 0.1).abs() < 0.01, "no timestamp should get default, got {}", score);
    }

    #[test]
    fn test_recency_future() {
        let scorer = RecencyScorer::new();
        let node = Node::event("未来事件", "在将来", Utc::now() + Duration::days(1));
        let score = scorer.score("", &node);
        assert!((score - 1.0).abs() < 0.01, "future event should score 1.0, got {}", score);
    }

    // ── ConfidenceScorer tests ──

    #[test]
    fn test_confidence_scorer() {
        let scorer = ConfidenceScorer;
        let node = Node::fact("测试", "描述").with_confidence(0.75);
        let score = scorer.score("任何查询", &node);
        assert!((score - 0.75).abs() < 0.01);
    }

    // ── CompositeScorer tests ──

    #[test]
    fn test_composite_empty() {
        let scorer = CompositeScorer::new();
        let node = Node::fact("测试", "描述");
        assert_eq!(scorer.score("查询", &node), 0.0);
    }

    #[test]
    fn test_composite_default() {
        let scorer = CompositeScorer::default_with_query();
        let node = Node::event("Project kickoff", "Project A officially launched", Utc::now() - Duration::hours(1))
            .with_confidence(0.9);
        let score = scorer.score("Project kickoff", &node);
        assert!(score > 0.4, "composite should score reasonably, got {}", score);
    }

    // ── rank_graph_nodes tests ──

    #[test]
    fn test_rank_graph_nodes_sorts_by_score() {
        let mut graph = make_test_graph();
        let scorer = TfIdfScorer::new();

        rank_graph_nodes(&mut graph, "project paused", &scorer);

        // 按分数降序排列
        for i in 1..graph.nodes.len() {
            assert!(
                graph.nodes[i - 1].relevance_score >= graph.nodes[i].relevance_score,
                "nodes should be sorted by relevance descending"
            );
        }
    }

    #[test]
    fn test_rank_graph_nodes_sets_scores() {
        let mut graph = make_test_graph();
        let scorer = TfIdfScorer::new();

        rank_graph_nodes(&mut graph, "project paused", &scorer);

        for node in &graph.nodes {
            let score = node.relevance_score;
            assert!((0.0..=1.0).contains(&score), "score should be in [0,1], got {}", score);
        }

        // 包含"paused"的节点应排在前面
        let top = &graph.nodes[0];
        assert!(top.title.contains("paused") || top.description.contains("paused"),
            "top result should contain query term, got: {}", top.title);
    }

    // ── tokenize tests ──

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("项目A正式启动");
        assert!(!tokens.is_empty());
        // 当前 tokenize 不支持 CJK 分词，整段保留
        assert!(tokens.iter().any(|t| t.contains("项目")));
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_short_words_filtered() {
        let tokens = tokenize("a b c 项目");
        assert_eq!(tokens, vec!["项目"]);
    }

    // ── with_relevance test ──

    #[test]
    fn test_with_relevance() {
        let node = Node::fact("测试", "描述").with_relevance(0.85);
        assert!((node.relevance_score - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_with_relevance_clamping() {
        let node = Node::fact("测试", "描述").with_relevance(1.5);
        assert!((node.relevance_score - 1.0).abs() < 0.01);
    }
}
