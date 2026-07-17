//! WeightedEvidenceGraph — 带权重的多 Workflow 结果合并器。
//!
//! 受到 Grok-1 MoE 的专家输出合并启发，将多个 MemoryWorkflow 的
//! 输出（EvidenceGraph）按路由权重合并为一个统一的图。
//!
//! # 合并逻辑
//!
//! 1. **去重**：基于 TF-IDF 文本相似度检测重复节点
//! 2. **加权评分**：按来源 workflow 的权重平均置信度
//! 3. **冲突标记**：检测重复节点的描述矛盾 + 时间顺序矛盾
//! 4. **边合并**：保留所有非重复边，标记跨 workflow 的关联

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EdgeKind, EvidenceGraph, Node, NodeId, RankingScorer};
use lingshu_memory_metrics::global_collector;

/// 合并过程中跟踪的节点元组：(NodeId, title, timestamp, description, source_workflow)
 type MergedNodeEntry = (NodeId, String, Option<DateTime<Utc>>, String, String);
/// 带权重的 Workflow 输出。
#[derive(Debug, Clone)]
pub struct WeightedWorkflowOutput {
    /// Workflow 名称
    pub workflow_name: String,
    /// 该 workflow 的权重
    pub weight: f64,
    /// 输出的 EvidenceGraph
    pub graph: EvidenceGraph,
}

/// 相似度去重配置。
#[derive(Debug, Clone)]
pub struct DedupConfig {
    /// 标题相似度阈值（0.0 ~ 1.0），高于此值视为重复
    pub title_similarity_threshold: f64,
    /// 是否检查时间戳近似
    pub check_timestamp_proximity: bool,
    /// 时间戳近似容差（秒）
    pub timestamp_tolerance_secs: i64,
    /// 是否合并标签
    pub merge_tags: bool,
    /// 是否合并边
    pub merge_edges: bool,
    /// 是否启用冲突检测
    pub enable_conflict_detection: bool,
    /// 描述差异阈值（0.0~1.0），低于此值视为矛盾
    pub description_difference_threshold: f64,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            title_similarity_threshold: 0.7,
            check_timestamp_proximity: true,
            timestamp_tolerance_secs: 3600,
            merge_tags: true,
            merge_edges: true,
            enable_conflict_detection: true,
            description_difference_threshold: 0.5,
        }
    }
}

/// 冲突类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictType {
    /// 重复节点间描述矛盾（同一事件不同说法）
    FactConflict,
    /// 时间顺序矛盾（A 标记为早于 B，但时间戳相反）
    TemporalConflict,
    /// 状态矛盾（同一实体同时处于互斥状态）
    StateConflict,
}

impl std::fmt::Display for ConflictType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FactConflict => write!(f, "fact_conflict"),
            Self::TemporalConflict => write!(f, "temporal_conflict"),
            Self::StateConflict => write!(f, "state_conflict"),
        }
    }
}

/// 合并过程中检测到的冲突信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConflict {
    /// 冲突类型
    pub conflict_type: ConflictType,
    /// 冲突描述
    pub description: String,
    /// 涉及的节点 ID 列表
    pub node_ids: Vec<String>,
    /// 涉及的节点标题列表
    pub node_titles: Vec<String>,
    /// 来源 workflow 名称
    pub source_workflows: Vec<String>,
    /// 严重程度 (0.0 ~ 1.0)
    pub severity: f64,
}

/// WeightedGraphMerger — 多 Workflow 结果的加权合并器。
#[derive(Debug, Clone)]
pub struct WeightedGraphMerger {
    dedup_config: DedupConfig,
    /// 可选的相关性评分器（合并后对节点排序）
    ranking_scorer: Option<Box<dyn RankingScorer>>,
    /// 评分使用的查询文本
    ranking_query: Option<String>,
}

impl Default for WeightedGraphMerger {
    fn default() -> Self {
        Self::new()
    }
}

impl WeightedGraphMerger {
    /// 创建一个合并器。
    pub fn new() -> Self {
        Self {
            dedup_config: DedupConfig::default(),
            ranking_scorer: None,
            ranking_query: None,
        }
    }

    /// 设置去重配置。
    pub fn with_dedup_config(mut self, config: DedupConfig) -> Self {
        self.dedup_config = config;
        self
    }

    /// 设置相关性评分器。
    ///
    /// 合并后将使用评分器对节点按查询相关性排序。
    /// 评分器会计算每个节点的 relevance_score。
    pub fn with_ranking(mut self, scorer: Box<dyn RankingScorer>, query: impl Into<String>) -> Self {
        self.ranking_scorer = Some(scorer);
        self.ranking_query = Some(query.into());
        self
    }

    /// 清除评分器。
    pub fn without_ranking(mut self) -> Self {
        self.ranking_scorer = None;
        self.ranking_query = None;
        self
    }

    /// 合并多个带权重的 Workflow 输出为单个 EvidenceGraph。
    pub fn merge(&self, outputs: Vec<WeightedWorkflowOutput>) -> WeightedMergeResult {
        let start = std::time::Instant::now();

        if outputs.is_empty() {
            return WeightedMergeResult {
                graph: EvidenceGraph::empty("merged"),
                merge_stats: MergeStats::default(),
                conflicts: Vec::new(),
                source_map: HashMap::new(),
            };
        }

        // 1. 按权重降序排列
        let mut sorted = outputs;
        sorted.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));

        let query = sorted.first().map(|o| o.graph.metadata.query.clone()).unwrap_or_default();
        let mut merged = EvidenceGraph::empty(&query);
        merged.metadata.source = "weighted_merge".to_string();

        let mut stats = MergeStats {
            total_workflows: sorted.len(),
            ..Default::default()
        };

        // (NodeId, title, timestamp, description, source_workflow)
        let mut merged_nodes: Vec<MergedNodeEntry> = Vec::new();
        let mut node_confidence_weight: HashMap<NodeId, (f64, f64)> = HashMap::new();
        let mut node_sources: HashMap<NodeId, Vec<String>> = HashMap::new();
        let mut conflicts: Vec<MergeConflict> = Vec::new();

        // 2. 合并节点（含冲突检测）
        for output in &sorted {
            let wf_name = &output.workflow_name;
            let weight = output.weight;

            for node in &output.graph.nodes {
                let duplicate_result = if self.dedup_config.title_similarity_threshold > 0.0 {
                    merged_nodes.iter().enumerate().find_map(|(idx, (_, title, ts, desc, src_wf))| {
                        if self.is_duplicate(node, title, ts) {
                            Some((idx, desc.clone(), src_wf.clone()))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };

                match duplicate_result {
                    Some((idx, existing_desc, existing_src_wf)) => {
                        let merged_id = merged_nodes[idx].0;
                        let entry = node_confidence_weight.entry(merged_id).or_insert((0.0, 0.0));
                        entry.0 += node.confidence * weight;
                        entry.1 += weight;

                        node_sources.entry(merged_id)
                            .or_default()
                            .push(wf_name.clone());
                        stats.nodes_deduped += 1;

                        // 冲突检测：重复节点但描述差异大
                        if self.dedup_config.enable_conflict_detection {
                            let desc_sim = self.description_similarity(&node.description, &existing_desc);
                            if desc_sim < self.dedup_config.description_difference_threshold {
                                let existing_title = &merged_nodes[idx].1;
                                let mut all_sources = vec![wf_name.clone(), existing_src_wf];
                                all_sources.sort();
                                all_sources.dedup();

                                conflicts.push(MergeConflict {
                                    conflict_type: ConflictType::FactConflict,
                                    description: format!(
                                        "同一事件 '{}' 在不同 workflow 中存在描述矛盾: '{}' vs '{}'",
                                        existing_title, existing_desc, node.description,
                                    ),
                                    node_ids: vec![merged_id.to_string(), node.id.to_string()],
                                    node_titles: vec![existing_title.clone(), node.title.clone()],
                                    source_workflows: all_sources,
                                    severity: 0.6,
                                });
                            }
                        }
                    }
                    None => {
                        let id = node.id;
                        let title = node.title.clone();
                        let ts = node.timestamp;
                        let desc = node.description.clone();
                        merged_nodes.push((id, title, ts, desc, wf_name.clone()));
                        merged.add_node(node.clone());

                        node_confidence_weight.insert(id, (node.confidence * weight, weight));
                        node_sources.entry(id).or_default().push(wf_name.clone());
                        stats.nodes_added += 1;
                    }
                }
            }
        }

        // 3. 更新置信度
        for node in &mut merged.nodes {
            if let Some(&(weighted_sum, total_weight)) = node_confidence_weight.get(&node.id) {
                if total_weight > 0.0 {
                    node.confidence = (weighted_sum / total_weight).clamp(0.0, 1.0);
                }
            }
        }

        // 4. 合并边
        let mut merged_edge_ids: HashSet<_> = merged.edges.iter().map(|e| e.id).collect();
        for output in &sorted {
            for edge in &output.graph.edges {
                if !merged_edge_ids.contains(&edge.id) {
                    let node_ids: HashSet<NodeId> = merged.nodes.iter().map(|n| n.id).collect();
                    if node_ids.contains(&edge.source_id) && node_ids.contains(&edge.target_id) {
                        merged.add_edge(edge.clone());
                        merged_edge_ids.insert(edge.id);
                        stats.edges_added += 1;
                    }
                } else {
                    stats.edges_deduped += 1;
                }
            }
        }

        // 5. 时间顺序冲突检测
        if self.dedup_config.enable_conflict_detection {
            self.detect_temporal_conflicts(&merged, &mut conflicts);
        }

        // 6. 更新元数据 + 写入冲突信息
        merged.metadata.node_count = merged.nodes.len();
        merged.metadata.edge_count = merged.edges.len();
        merged.metadata.build_time_ms = start.elapsed().as_millis() as u64;

        stats.conflicts_detected = conflicts.len();
        if !conflicts.is_empty() {
            if let Ok(conflicts_json) = serde_json::to_value(&conflicts) {
                merged.metadata.attributes.insert(
                    "merge_conflicts".to_string(),
                    conflicts_json.to_string(),
                );
            }
            merged.metadata.attributes.insert(
                "has_conflicts".to_string(),
                "true".to_string(),
            );
        }

        // 合并实体列表
        let all_entities: HashSet<String> = sorted.iter()
            .flat_map(|o| o.graph.metadata.entities.iter().cloned())
            .collect();
        merged.metadata.entities = all_entities.into_iter().collect();

        // 时间跨度
        let all_starts: Vec<DateTime<Utc>> = sorted.iter()
            .filter_map(|o| o.graph.metadata.time_span_start)
            .collect();
        let all_ends: Vec<DateTime<Utc>> = sorted.iter()
            .filter_map(|o| o.graph.metadata.time_span_end)
            .collect();
        if !all_starts.is_empty() {
            merged.metadata.time_span_start = all_starts.into_iter().min();
        }
        if !all_ends.is_empty() {
            merged.metadata.time_span_end = all_ends.into_iter().max();
        }

        stats.total_time_ms = start.elapsed().as_millis() as u64;

        // ── 相关性排序（如有配置评分器）──
        if let Some(ref scorer) = self.ranking_scorer {
            if let Some(ref rank_query) = self.ranking_query {
            for node in &mut merged.nodes {
                node.relevance_score = scorer.score(rank_query, node);
            }
            merged.nodes.sort_by(|a, b| {
                b.relevance_score
                    .partial_cmp(&a.relevance_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            }
        }

        let source_map: HashMap<String, Vec<String>> = node_sources
            .into_iter()
            .map(|(id, sources)| (id.to_string(), sources))
            .collect();


        // ── Memory Metrics 记录 ──
        let collector = global_collector();
        for output in &sorted {
            let node_count = output.graph.nodes.len();
            let latency = output.graph.metadata.build_time_ms as f64;
            let wf_conflicts = conflicts.iter().filter(|c| c.source_workflows.contains(&output.workflow_name)).count();
            collector.record_query(&output.workflow_name, latency, node_count, wf_conflicts);
        }
        // 记录冲突类型
        for conflict in &conflicts {
            collector.record_conflict(&conflict.conflict_type.to_string());
        }

        WeightedMergeResult {
            graph: merged,
            merge_stats: stats,
            conflicts,
            source_map,
        }
    }

    /// 检测合并图中的时间顺序矛盾。
    fn detect_temporal_conflicts(&self, graph: &EvidenceGraph, conflicts: &mut Vec<MergeConflict>) {
        for edge in &graph.edges {
            if edge.kind != EdgeKind::Temporal {
                continue;
            }
            let source = graph.nodes.iter().find(|n| n.id == edge.source_id);
            let target = graph.nodes.iter().find(|n| n.id == edge.target_id);
            if let (Some(src), Some(tgt)) = (source, target) {
                if let (Some(src_ts), Some(tgt_ts)) = (src.timestamp, tgt.timestamp) {
                    if src_ts > tgt_ts {
                        conflicts.push(MergeConflict {
                            conflict_type: ConflictType::TemporalConflict,
                            description: format!(
                                "时间顺序矛盾：'{}' ({}) 标记为早于 '{}' ({}), 但时间戳显示相反",
                                src.title, src_ts.format("%Y-%m-%d %H:%M"),
                                tgt.title, tgt_ts.format("%Y-%m-%d %H:%M"),
                            ),
                            node_ids: vec![src.id.to_string(), tgt.id.to_string()],
                            node_titles: vec![src.title.clone(), tgt.title.clone()],
                            source_workflows: vec!["merged".to_string()],
                            severity: 0.7,
                        });
                    }
                }
            }
        }
    }

    /// 检查两个节点是否重复。
    fn is_duplicate(&self, node: &Node, existing_title: &str, existing_ts: &Option<DateTime<Utc>>) -> bool {
        let title_sim = self.title_similarity(&node.title, existing_title);
        if title_sim < self.dedup_config.title_similarity_threshold {
            return false;
        }
        if self.dedup_config.check_timestamp_proximity {
            if let (Some(ts1), Some(ts2)) = (node.timestamp, existing_ts) {
                let diff = (ts1 - *ts2).num_seconds().abs();
                if diff > self.dedup_config.timestamp_tolerance_secs {
                    return false;
                }
            }
        }
        true
    }

    /// 计算两个标题的字符级相似度（Jaccard + bigram）。
    fn title_similarity(&self, a: &str, b: &str) -> f64 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        if a_lower == b_lower {
            return 1.0;
        }
        let bigrams_a: HashSet<String> = a_lower.chars()
            .collect::<Vec<_>>().windows(2)
            .map(|w| w.iter().collect::<String>()).collect();
        let bigrams_b: HashSet<String> = b_lower.chars()
            .collect::<Vec<_>>().windows(2)
            .map(|w| w.iter().collect::<String>()).collect();
        if bigrams_a.is_empty() && bigrams_b.is_empty() {
            let chars_a: HashSet<char> = a_lower.chars().collect();
            let chars_b: HashSet<char> = b_lower.chars().collect();
            let intersection: usize = chars_a.intersection(&chars_b).count();
            let union: usize = chars_a.union(&chars_b).count();
            return if union == 0 { 0.0 } else { intersection as f64 / union as f64 };
        }
        let intersection: usize = bigrams_a.intersection(&bigrams_b).count();
        let union: usize = bigrams_a.union(&bigrams_b).count();
        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }

    /// 计算描述的文本相似度（用于冲突检测，基于字符 bigram）。
    fn description_similarity(&self, a: &str, b: &str) -> f64 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        if a_lower == b_lower {
            return 1.0;
        }
        // 字符 bigram Jaccard 相似度（与 title_similarity 一致）
        let bigrams_a: HashSet<String> = a_lower.chars()
            .collect::<Vec<_>>().windows(2)
            .map(|w| w.iter().collect::<String>()).collect();
        let bigrams_b: HashSet<String> = b_lower.chars()
            .collect::<Vec<_>>().windows(2)
            .map(|w| w.iter().collect::<String>()).collect();
        if bigrams_a.is_empty() && bigrams_b.is_empty() {
            let chars_a: HashSet<char> = a_lower.chars().collect();
            let chars_b: HashSet<char> = b_lower.chars().collect();
            let intersection: usize = chars_a.intersection(&chars_b).count();
            let union: usize = chars_a.union(&chars_b).count();
            return if union == 0 { 0.0 } else { intersection as f64 / union as f64 };
        }
        let intersection: usize = bigrams_a.intersection(&bigrams_b).count();
        let union: usize = bigrams_a.union(&bigrams_b).count();
        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }
}

// ─── 合并结果 ─────────────────────────────────────────

/// 加权合并结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedMergeResult {
    /// 合并后的 EvidenceGraph
    pub graph: EvidenceGraph,
    /// 合并统计
    pub merge_stats: MergeStats,
    /// 检测到的冲突列表
    pub conflicts: Vec<MergeConflict>,
    /// 节点 ID → 来源 workflow 名列表
    pub source_map: HashMap<String, Vec<String>>,
}

/// 合并统计信息。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MergeStats {
    /// 总 workflow 数
    pub total_workflows: usize,
    /// 新增节点数
    pub nodes_added: usize,
    /// 去重节点数
    pub nodes_deduped: usize,
    /// 新增边数
    pub edges_added: usize,
    /// 去重边数
    pub edges_deduped: usize,
    /// 检测到的冲突数
    pub conflicts_detected: usize,
    /// 总耗时（毫秒）
    pub total_time_ms: u64,
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node};
    use chrono::Duration;

    fn make_graph(query: &str, titles: &[(&str, &str, i64)]) -> EvidenceGraph {
        let mut g = EvidenceGraph::empty(query);
        for (title, desc, hours_ago) in titles {
            let ts = Utc::now() - Duration::hours(*hours_ago);
            g.add_node(Node::event(*title, *desc, ts));
        }
        g.metadata.source = "test".to_string();
        g
    }

    fn tie_nodes(g: &mut EvidenceGraph) {
        let ids: Vec<NodeId> = g.nodes.iter().map(|n| n.id).collect();
        for i in 0..ids.len().saturating_sub(1) {
            g.add_edge(Edge::temporal(ids[i], ids[i + 1]));
        }
    }

    #[test]
    fn test_empty_merge() {
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![]);
        assert!(result.graph.nodes.is_empty());
        assert_eq!(result.merge_stats.total_workflows, 0);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_single_workflow() {
        let mut g = make_graph("测试", &[("事件A", "描述A", 48)]);
        tie_nodes(&mut g);
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 1.0, graph: g }
        ]);
        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.merge_stats.nodes_added, 1);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_merge_two_workflows_no_dups() {
        let g1 = make_graph("项目A", &[("事件A", "描述A", 48)]);
        let g2 = make_graph("项目A", &[("事件B", "描述B", 24)]);
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 0.7, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "semantic".into(), weight: 0.3, graph: g2 },
        ]);
        assert_eq!(result.graph.nodes.len(), 2);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_merge_with_duplicates_similar_desc() {
        let g1 = make_graph("项目A", &[("事件A", "描述A", 48)]);
        let mut g2 = make_graph("项目A", &[("事件A", "描述A相同", 48)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
        }
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 0.7, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "semantic".into(), weight: 0.3, graph: g2 },
        ]);
        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.merge_stats.nodes_deduped, 1);
        // "描述A" vs "描述A相同" — 共享 "描述A"，不应触发冲突
        assert!(result.conflicts.is_empty(), "similar descriptions ok");
    }

    #[test]
    fn test_conflict_detection_fact_conflict() {
        let g1 = make_graph("项目A", &[("事件A", "项目已启动", 48)]);
        let mut g2 = make_graph("项目A", &[("事件A", "项目已取消", 48)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
        }
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 0.7, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "semantic".into(), weight: 0.3, graph: g2 },
        ]);
        assert_eq!(result.graph.nodes.len(), 1);
        // "项目已启动" vs "项目已取消" — 只有"项目已"重叠，触发冲突
        assert!(!result.conflicts.is_empty(), "should detect fact conflict");
        assert_eq!(result.conflicts[0].conflict_type, ConflictType::FactConflict);
    }

    #[test]
    fn test_temporal_conflict_detection() {
        let merger = WeightedGraphMerger::new();
        let mut g1 = EvidenceGraph::empty("测试");
        let n1 = Node::event("事件A", "先发生", Utc::now() - Duration::hours(10));
        let n2 = Node::event("事件B", "后发生", Utc::now() - Duration::hours(2));
        let n1_id = n1.id;
        let n2_id = n2.id;
        g1.add_node(n1);
        g1.add_node(n2);
        g1.add_edge(Edge::temporal(n2_id, n1_id)); // 反序

        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 1.0, graph: g1 },
        ]);
        let temporal_c: Vec<_> = result.conflicts.iter()
            .filter(|c| c.conflict_type == ConflictType::TemporalConflict).collect();
        assert!(!temporal_c.is_empty(), "should detect temporal conflict");
    }

    #[test]
    fn test_weight_ordering() {
        let g1 = make_graph("测试", &[("事件A", "权重高", 48)]);
        let g2 = make_graph("测试", &[("事件B", "权重低", 24)]);
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "primary".into(), weight: 0.9, graph: g1.clone() },
            WeightedWorkflowOutput { workflow_name: "secondary".into(), weight: 0.1, graph: g2.clone() },
        ]);
        assert_eq!(result.graph.nodes.len(), 2);
        let result2 = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "secondary".into(), weight: 0.1, graph: g2 },
            WeightedWorkflowOutput { workflow_name: "primary".into(), weight: 0.9, graph: g1 },
        ]);
        assert_eq!(result2.graph.nodes.len(), 2);
    }

    #[test]
    fn test_title_similarity() {
        let merger = WeightedGraphMerger::new();
        assert!((merger.title_similarity("事件A", "事件A") - 1.0).abs() < 0.01);
        let sim = merger.title_similarity("事件A", "事件B");
        assert!(sim > 0.3, "got {}", sim);
        let sim2 = merger.title_similarity("项目A启动", "今天天气很好");
        assert!(sim2 < 0.3, "got {}", sim2);
    }

    #[test]
    fn test_description_similarity() {
        let merger = WeightedGraphMerger::new();
        assert!((merger.description_similarity("项目已启动", "项目已启动") - 1.0).abs() < 0.01);
        let sim = merger.description_similarity("项目已启动", "项目已取消");
        assert!(sim > 0.3, "shared words should give some sim: {}", sim);
        let sim2 = merger.description_similarity("项目已启动", "今天天气很好");
        assert!(sim2 < 0.1, "got {}", sim2);
    }

    #[test]
    fn test_merge_stats() {
        let g1 = make_graph("测试", &[("事件A", "描述", 48), ("事件B", "描述", 24)]);
        let mut g2 = make_graph("测试", &[("事件A", "描述相同", 48), ("事件C", "描述", 12)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
        }
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "w1".into(), weight: 0.6, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "w2".into(), weight: 0.4, graph: g2 },
        ]);
        assert_eq!(result.merge_stats.total_workflows, 2);
        assert_eq!(result.merge_stats.nodes_deduped, 1);
        assert_eq!(result.graph.nodes.len(), 3);
    }

    #[test]
    fn test_confidence_weighting() {
        let mut g1 = make_graph("测试", &[("事件A", "描述", 48)]);
        if let Some(node) = g1.nodes.first_mut() { node.confidence = 0.5; }
        let mut g2 = make_graph("测试", &[("事件A", "描述相同", 48)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
            node.confidence = 0.9;
        }
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "low_conf".into(), weight: 0.3, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "high_conf".into(), weight: 0.7, graph: g2 },
        ]);
        assert_eq!(result.graph.nodes.len(), 1);
        let merged_node = &result.graph.nodes[0];
        assert!(
            (merged_node.confidence - 0.78).abs() < 0.02,
            "expected ~0.78, got {:.3}", merged_node.confidence
        );
    }

    #[test]
    fn test_source_map() {
        let g1 = make_graph("测试", &[("事件A", "描述", 48)]);
        let g2 = make_graph("测试", &[("事件B", "描述", 24)]);
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 0.7, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "semantic".into(), weight: 0.3, graph: g2 },
        ]);
        for (node_id, sources) in &result.source_map {
            assert!(!sources.is_empty());
            assert!(result.graph.nodes.iter().any(|n| n.id.to_string() == *node_id));
        }
    }

    #[test]
    fn test_disable_conflict_detection() {
        let config = DedupConfig { enable_conflict_detection: false, ..Default::default() };
        let merger = WeightedGraphMerger::new().with_dedup_config(config);
        let g1 = make_graph("测试", &[("事件A", "项目已启动", 48)]);
        let mut g2 = make_graph("测试", &[("事件A", "项目已取消", 48)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
        }
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 0.7, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "semantic".into(), weight: 0.3, graph: g2 },
        ]);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_conflict_stored_in_graph_attributes() {
        let g1 = make_graph("测试", &[("事件A", "项目已启动", 48)]);
        let mut g2 = make_graph("测试", &[("事件A", "项目已取消", 48)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
        }
        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput { workflow_name: "timeline".into(), weight: 0.7, graph: g1 },
            WeightedWorkflowOutput { workflow_name: "semantic".into(), weight: 0.3, graph: g2 },
        ]);
        assert_eq!(result.graph.metadata.attributes.get("has_conflicts"), Some(&"true".to_string()));
        assert!(result.graph.metadata.attributes.contains_key("merge_conflicts"));
    }
}
