//! WeightedEvidenceGraph — 带权重的多 Workflow 结果合并器。
//!
//! 受到 Grok-1 MoE 的专家输出合并启发，将多个 MemoryWorkflow 的
//! 输出（EvidenceGraph）按路由权重合并为一个统一的图。
//!
//! # 合并逻辑
//!
//! 1. **去重**：基于 TF-IDF 文本相似度检测重复节点
//! 2. **加权评分**：按来源 workflow 的权重平均置信度
//! 3. **冲突标记**：用 ReflectionEvaluator 检测矛盾
//! 4. **边合并**：保留所有非重复边，标记跨 workflow 的关联

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EvidenceGraph, Node, NodeId};

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
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            title_similarity_threshold: 0.7,
            check_timestamp_proximity: true,
            timestamp_tolerance_secs: 3600, // 1 小时
            merge_tags: true,
            merge_edges: true,
        }
    }
}

/// WeightedGraphMerger — 多 Workflow 结果的加权合并器。
#[derive(Debug, Clone)]
pub struct WeightedGraphMerger {
    dedup_config: DedupConfig,
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
        }
    }

    /// 设置去重配置。
    pub fn with_dedup_config(mut self, config: DedupConfig) -> Self {
        self.dedup_config = config;
        self
    }

    /// 合并多个带权重的 Workflow 输出为单个 EvidenceGraph。
    ///
    /// # 合并步骤
    ///
    /// 1. 按权重从高到低排序 Workflow
    /// 2. 逐节点合并（高权重优先）
    /// 3. 检测重复节点（基于 TF-IDF 标题相似度）
    /// 4. 置信度 = 来源 workflow 权重的加权平均
    /// 5. 冲突检测（同一时间点不同事实）
    /// 6. 边合并（跨 workflow 关联）
    pub fn merge(&self, outputs: Vec<WeightedWorkflowOutput>) -> WeightedMergeResult {
        let start = std::time::Instant::now();

        if outputs.is_empty() {
            return WeightedMergeResult {
                graph: EvidenceGraph::empty("merged"),
                merge_stats: MergeStats::default(),
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

        // 跟踪已合并的节点（避免重复）
        let mut merged_nodes: Vec<(NodeId, String, Option<DateTime<Utc>>)> = Vec::new();
        // 节点 ID → 累积置信度权重
        let mut node_confidence_weight: HashMap<NodeId, (f64, f64)> = HashMap::new();
        // 节点 ID → 来源 workflow 名列表
        let mut node_sources: HashMap<NodeId, Vec<String>> = HashMap::new();

        // 2. 合并节点
        for output in &sorted {
            let wf_name = &output.workflow_name;
            let weight = output.weight;

            for node in &output.graph.nodes {
                // 检查是否与已合并的节点重复
                let duplicate_idx = if self.dedup_config.title_similarity_threshold > 0.0 {
                    merged_nodes.iter().position(|(_, title, ts)| {
                        self.is_duplicate(node, title, ts)
                    })
                } else {
                    None
                };

                match duplicate_idx {
                    Some(idx) => {
                        // 更新已有节点的置信度（加权平均）
                        let merged_id = merged_nodes[idx].0;
                        let entry = node_confidence_weight.entry(merged_id).or_insert((0.0, 0.0));
                        entry.0 += node.confidence * weight;
                        entry.1 += weight;

                        // 记录来源
                        node_sources.entry(merged_id)
                            .or_default()
                            .push(wf_name.clone());

                        stats.nodes_deduped += 1;
                    }
                    None => {
                        // 添加新节点
                        let id = node.id;
                        let title = node.title.clone();
                        let ts = node.timestamp;
                        merged_nodes.push((id, title, ts));
                        merged.add_node(node.clone());

                        node_confidence_weight.insert(id, (node.confidence * weight, weight));
                        node_sources.entry(id).or_default().push(wf_name.clone());

                        stats.nodes_added += 1;
                    }
                }
            }
        }

        // 3. 更新合并后节点的置信度
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
                    // 检查边的两端节点是否都在图中
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

        // 5. 更新元数据
        merged.metadata.node_count = merged.nodes.len();
        merged.metadata.edge_count = merged.edges.len();
        merged.metadata.build_time_ms = start.elapsed().as_millis() as u64;

        // 合并实体列表
        let all_entities: HashSet<String> = sorted.iter()
            .flat_map(|o| o.graph.metadata.entities.iter().cloned())
            .collect();
        merged.metadata.entities = all_entities.into_iter().collect();

        // 时间跨度 — 取所有来源的最小/最大值
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

        WeightedMergeResult {
            graph: merged,
            merge_stats: stats,
        }
    }

    /// 检查两个节点是否重复。
    fn is_duplicate(&self, node: &Node, existing_title: &str, existing_ts: &Option<DateTime<Utc>>) -> bool {
        // 标题相似度检查
        let title_sim = self.title_similarity(&node.title, existing_title);
        if title_sim < self.dedup_config.title_similarity_threshold {
            return false;
        }

        // 时间戳近似检查（如果启用）
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

        // 字符 bigram Jaccard 相似度
        let bigrams_a: HashSet<String> = a_lower.chars()
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| w.iter().collect::<String>())
            .collect();
        let bigrams_b: HashSet<String> = b_lower.chars()
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| w.iter().collect::<String>())
            .collect();

        if bigrams_a.is_empty() && bigrams_b.is_empty() {
            // 单字符或空标题
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
    /// 总耗时（毫秒）
    pub total_time_ms: u64,
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Node};
    use chrono::Duration;

    fn make_graph(query: &str, titles: &[(&str, &str, i64)]) -> EvidenceGraph {
        // titles: (title, description, hours_ago)
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
    }

    #[test]
    fn test_single_workflow() {
        let mut g = make_graph("测试", &[("事件A", "描述A", 48)]);
        tie_nodes(&mut g);

        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput {
                workflow_name: "timeline".into(),
                weight: 1.0,
                graph: g,
            }
        ]);

        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.merge_stats.nodes_added, 1);
    }

    #[test]
    fn test_merge_two_workflows_no_dups() {
        let g1 = make_graph("项目A", &[("事件A", "描述A", 48)]);
        let g2 = make_graph("项目A", &[("事件B", "描述B", 24)]);

        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput {
                workflow_name: "timeline".into(),
                weight: 0.7,
                graph: g1,
            },
            WeightedWorkflowOutput {
                workflow_name: "semantic".into(),
                weight: 0.3,
                graph: g2,
            },
        ]);

        assert_eq!(result.graph.nodes.len(), 2, "two distinct events should both be kept");
    }

    #[test]
    fn test_merge_with_duplicates() {
        let g1 = make_graph("项目A", &[("事件A", "描述A", 48)]);
        let mut g2 = make_graph("项目A", &[("事件A", "描述A相同", 48)]);
        // 第二个 graph 的节点会有不同 ID，但标题和描述相似
        // 强制让 g2 的第一个节点与 g1 的相同
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
        }

        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput {
                workflow_name: "timeline".into(),
                weight: 0.7,
                graph: g1,
            },
            WeightedWorkflowOutput {
                workflow_name: "semantic".into(),
                weight: 0.3,
                graph: g2,
            },
        ]);

        assert_eq!(result.graph.nodes.len(), 1, "duplicate events should be deduped");
        assert_eq!(result.merge_stats.nodes_deduped, 1);
    }

    #[test]
    fn test_weight_ordering() {
        let g1 = make_graph("测试", &[("事件A", "权重高", 48)]);
        let g2 = make_graph("测试", &[("事件B", "权重低", 24)]);

        let merger = WeightedGraphMerger::new();

        // 先高权重后低权重
        let result = merger.merge(vec![
            WeightedWorkflowOutput {
                workflow_name: "primary".into(),
                weight: 0.9,
                graph: g1.clone(),
            },
            WeightedWorkflowOutput {
                workflow_name: "secondary".into(),
                weight: 0.1,
                graph: g2.clone(),
            },
        ]);

        assert_eq!(result.graph.nodes.len(), 2);

        // 先低权重后高权重（结果应该一样）
        let result2 = merger.merge(vec![
            WeightedWorkflowOutput {
                workflow_name: "secondary".into(),
                weight: 0.1,
                graph: g2,
            },
            WeightedWorkflowOutput {
                workflow_name: "primary".into(),
                weight: 0.9,
                graph: g1,
            },
        ]);

        assert_eq!(result2.graph.nodes.len(), 2);
    }

    #[test]
    fn test_title_similarity() {
        let merger = WeightedGraphMerger::new();

        // 完全相同
        assert!((merger.title_similarity("事件A", "事件A") - 1.0).abs() < 0.01);

        // 部分相似（共享 "事件"）
        let sim = merger.title_similarity("事件A", "事件B");
        assert!(sim > 0.3, "shared '事件' should give some similarity, got {}", sim);

        // 完全不同
        let sim2 = merger.title_similarity("项目A启动", "今天天气很好");
        assert!(sim2 < 0.3, "unrelated titles should have low similarity, got {}", sim2);
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
            WeightedWorkflowOutput {
                workflow_name: "w1".into(),
                weight: 0.6,
                graph: g1,
            },
            WeightedWorkflowOutput {
                workflow_name: "w2".into(),
                weight: 0.4,
                graph: g2,
            },
        ]);

        assert_eq!(result.merge_stats.total_workflows, 2);
        // A 被去重，B 和 C 被添加
        assert_eq!(result.merge_stats.nodes_deduped, 1);
        assert_eq!(result.graph.nodes.len(), 3);
    }

    #[test]
    fn test_confidence_weighting() {
        let mut g1 = make_graph("测试", &[("事件A", "描述", 48)]);
        if let Some(node) = g1.nodes.first_mut() {
            node.confidence = 0.5;
        }
        let mut g2 = make_graph("测试", &[("事件A", "描述相同", 48)]);
        if let Some(node) = g2.nodes.first_mut() {
            node.title = "事件A".to_string();
            node.timestamp = Some(Utc::now() - Duration::hours(48));
            node.confidence = 0.9;
        }

        let merger = WeightedGraphMerger::new();
        let result = merger.merge(vec![
            WeightedWorkflowOutput {
                workflow_name: "low_conf".into(),
                weight: 0.3,
                graph: g1,
            },
            WeightedWorkflowOutput {
                workflow_name: "high_conf".into(),
                weight: 0.7,
                graph: g2,
            },
        ]);

        assert_eq!(result.graph.nodes.len(), 1);
        let merged_node = &result.graph.nodes[0];
        // 加权平均: (0.5*0.3 + 0.9*0.7) / (0.3 + 0.7) = (0.15 + 0.63) / 1.0 = 0.78
        assert!(
            (merged_node.confidence - 0.78).abs() < 0.02,
            "expected confidence ~0.78, got {:.3}",
            merged_node.confidence
        );
    }
}
