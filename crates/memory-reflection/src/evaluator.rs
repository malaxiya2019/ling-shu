//! ReflectionEvaluator — 记忆查询质量评估器。
//!
//! 对 Memory Query 的检索结果（EvidenceGraph）进行多维度评估：
//! - 证据充分性：找到的相关事件数量
//! - 一致性：检测矛盾事件（相同实体、时间冲突、状态矛盾）
//! - 完整性：时间线覆盖是否完整
//! - 置信度：综合评分
//! - 改进建议：基于评估结果给出可操作建议

use chrono::{DateTime, Utc};
use lingshu_evidence_graph::{Edge, EdgeKind, EvidenceGraph, Node, NodeId, NodeKind};
use std::collections::{HashMap, HashSet};
use lingshu_memory_metrics::global_collector;

// ─── ReflectionResult ───────────────────────────────────

/// 反思评估结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReflectionResult {
    /// 原始查询
    pub query: String,
    /// 使用的路由/工作流
    pub route_used: String,
    /// 证据节点数
    pub evidence_count: usize,
    /// 一致性评分 (0.0 ~ 1.0)
    pub consistency_score: f64,
    /// 完整性评分 (0.0 ~ 1.0)
    pub completeness_score: f64,
    /// 是否存在冲突证据
    pub has_conflicts: bool,
    /// 冲突详情
    pub conflicts: Vec<ConflictInfo>,
    /// 综合置信度 (0.0 ~ 1.0)
    pub confidence: f64,
    /// 发现的信息缺口
    pub gaps: Vec<String>,
    /// 改进建议
    pub suggestions: Vec<String>,
    /// 查询耗时 (ms)
    pub latency_ms: u64,
    /// 评估时间
    pub evaluated_at: DateTime<Utc>,
}

/// 冲突信息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConflictInfo {
    /// 冲突类型
    pub conflict_type: ConflictType,
    /// 冲突描述
    pub description: String,
    /// 涉及的节点 ID
    pub node_ids: Vec<String>,
    /// 严重程度 (0.0 ~ 1.0)
    pub severity: f64,
}

/// 冲突类型。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConflictType {
    /// 时间顺序矛盾（事件 A 在事件 B 之前，但时间戳相反）
    TemporalConflict,
    /// 状态矛盾（同一实体同时处于互斥状态）
    StateConflict,
    /// 事实矛盾（同一实体的事实描述冲突）
    FactConflict,
    /// 时间线断裂（关键时间点缺失）
    TimelineGap,
}

impl std::fmt::Display for ConflictType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemporalConflict => write!(f, "temporal_conflict"),
            Self::StateConflict => write!(f, "state_conflict"),
            Self::FactConflict => write!(f, "fact_conflict"),
            Self::TimelineGap => write!(f, "timeline_gap"),
        }
    }
}

// ─── ReflectionEvaluator ───────────────────────────────

/// 记忆查询质量评估器。
///
/// 对 EvidenceGraph 进行多维度评估，生成质量报告。
#[derive(Debug, Clone)]
pub struct ReflectionEvaluator {
    /// 最小证据数阈值
    pub min_evidence_count: usize,
    /// 一致性评分阈值（低于此值标记为有冲突）
    pub consistency_threshold: f64,
    /// 完整性评分阈值
    pub completeness_threshold: f64,
    /// 最大时间间隔（小时），超过此间隔视为时间线断裂
    pub max_time_gap_hours: i64,
}

impl Default for ReflectionEvaluator {
    fn default() -> Self {
        Self {
            min_evidence_count: 1,
            consistency_threshold: 0.5,
            completeness_threshold: 0.3,
            max_time_gap_hours: 72, // 3 天
        }
    }
}

impl ReflectionEvaluator {
    /// 创建评估器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最小证据数阈值。
    pub fn with_min_evidence(mut self, min: usize) -> Self {
        self.min_evidence_count = min;
        self
    }

    /// 设置一致性阈值。
    pub fn with_consistency_threshold(mut self, threshold: f64) -> Self {
        self.consistency_threshold = threshold;
        self
    }

    /// 设置完整性阈值。
    pub fn with_completeness_threshold(mut self, threshold: f64) -> Self {
        self.completeness_threshold = threshold;
        self
    }

    /// 设置最大时间间隔。
    pub fn with_max_time_gap(mut self, hours: i64) -> Self {
        self.max_time_gap_hours = hours;
        self
    }

    /// 评估一个 Memory Query 的结果。
    pub fn evaluate(
        &self,
        query: &str,
        route: &str,
        graph: &EvidenceGraph,
        latency_ms: u64,
    ) -> ReflectionResult {
        let now = Utc::now();

        // 1. 证据充分性
        let evidence_count = graph.nodes.len();
        let event_nodes: Vec<&Node> = graph.nodes.iter()
            .filter(|n| n.kind == NodeKind::Event)
            .collect();

        // 2. 一致性检测
        let (consistency_score, conflicts) = self.detect_conflicts(graph);

        // 3. 完整性评估
        let completeness_score = self.evaluate_completeness(graph, &event_nodes);

        // 4. 检测信息缺口
        let gaps = self.detect_gaps(graph, &event_nodes);

        // 5. 综合置信度
        let confidence = self.calculate_confidence(
            evidence_count,
            consistency_score,
            completeness_score,
        );

        // 6. 改进建议
        let suggestions = self.generate_suggestions(
            evidence_count,
            consistency_score,
            completeness_score,
            &conflicts,
            &gaps,
        );

        // ── 记录 Reflection 指标 ──
        let has_conflicts = !conflicts.is_empty();
        let has_suggestions = !suggestions.is_empty();
        global_collector().record_reflection(has_conflicts, has_suggestions);

        ReflectionResult {
            query: query.to_string(),
            route_used: route.to_string(),
            evidence_count,
            consistency_score,
            completeness_score,
            has_conflicts,
            conflicts,
            confidence,
            gaps,
            suggestions,
            latency_ms,
            evaluated_at: now,
        }
    }

    /// 检测 EvidenceGraph 中的冲突。
    fn detect_conflicts(&self, graph: &EvidenceGraph) -> (f64, Vec<ConflictInfo>) {
        let mut conflicts = Vec::new();
        let mut total_checks = 0;
        let mut conflict_penalty = 0.0;

        // 1. 时间顺序冲突检测
        // 检查时间边是否与时间戳一致
        let time_edges: Vec<&Edge> = graph.edges.iter()
            .filter(|e| e.kind == EdgeKind::Temporal)
            .collect();

        for edge in &time_edges {
            // 查找 source 和 target 节点
            let source = graph.nodes.iter().find(|n| n.id == edge.source_id);
            let target = graph.nodes.iter().find(|n| n.id == edge.target_id);

            if let (Some(src), Some(tgt)) = (source, target) {
                total_checks += 1;
                // 检查两个节点都有时间戳
                if let (Some(src_ts), Some(tgt_ts)) = (src.timestamp, tgt.timestamp) {
                    // 如果 temporal 边表示 src → tgt（src 早于 tgt），
                    // 但 src 的时间戳晚于 tgt，则存在矛盾
                    if src_ts > tgt_ts {
                        conflict_penalty += 0.3;
                        conflicts.push(ConflictInfo {
                            conflict_type: ConflictType::TemporalConflict,
                            description: format!(
                                "时间顺序矛盾：'{}' ({}) 标记为早于 '{}' ({}), \
                                 但时间戳显示相反",
                                src.title,
                                src_ts.format("%Y-%m-%d"),
                                tgt.title,
                                tgt_ts.format("%Y-%m-%d"),
                            ),
                            node_ids: vec![
                                src.id.to_string(),
                                tgt.id.to_string(),
                            ],
                            severity: 0.7,
                        });
                    }
                }
            }
        }

        // 2. 状态矛盾检测
        let entity_state_map = self.build_entity_state_map(graph);
        for (entity, states) in &entity_state_map {
            if states.len() > 1 {
                let state_pairs = self.find_incompatible_states(states);
                for (s1, s2) in &state_pairs {
                    total_checks += 1;
                    conflict_penalty += 0.4;
                    conflicts.push(ConflictInfo {
                        conflict_type: ConflictType::StateConflict,
                        description: format!(
                            "状态矛盾：实体 '{}' 同时处于 '{}' 和 '{}'",
                            entity, s1, s2,
                        ),
                        node_ids: Vec::new(),
                        severity: 0.8,
                    });
                }
            }
        }

        // 3. 事实矛盾检测
        let fact_contradictions = self.detect_fact_contradictions(graph);
        for contradiction in &fact_contradictions {
            total_checks += 1;
            conflict_penalty += 0.3;
            conflicts.push(ConflictInfo {
                conflict_type: ConflictType::FactConflict,
                description: contradiction.clone(),
                node_ids: Vec::new(),
                severity: 0.6,
            });
        }

        // 计算一致性评分
        let consistency_score = if total_checks == 0 {
            1.0
        } else {
            (1.0 - (conflict_penalty / total_checks as f64)).max(0.0)
        };

        (consistency_score, conflicts)
    }

    /// 构建实体 → 状态映射。
    fn build_entity_state_map(&self, graph: &EvidenceGraph) -> HashMap<String, Vec<String>> {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();

        for node in &graph.nodes {
            // 从标签中提取实体信息
            for tag in &node.tags {
                if tag.starts_with("state_change:") {
                    let state = tag.trim_start_matches("state_change:");
                    for t in &node.tags {
                        if t.starts_with("entity:") {
                            let entity = t.trim_start_matches("entity:");
                            map.entry(entity.to_string())
                                .or_default()
                                .push(state.to_string());
                        }
                    }
                }
            }
        }

        map
    }

    /// 查找不可共存的状态对。
    fn find_incompatible_states(&self, states: &[String]) -> Vec<(String, String)> {
        let mut incompatible = Vec::new();
        let state_set: HashSet<&str> = states.iter().map(|s| s.as_str()).collect();

        let incompat_pairs = [
            ("active", "inactive"),
            ("active", "paused"),
            ("active", "cancelled"),
            ("running", "stopped"),
            ("running", "paused"),
            ("completed", "cancelled"),
            ("enabled", "disabled"),
            ("online", "offline"),
        ];

        for (a, b) in &incompat_pairs {
            if state_set.contains(a) && state_set.contains(b) {
                incompatible.push((a.to_string(), b.to_string()));
            }
        }

        incompatible
    }

    /// 检测事实矛盾。
    fn detect_fact_contradictions(&self, graph: &EvidenceGraph) -> Vec<String> {
        let mut contradictions = Vec::new();

        // 按实体分组，检查描述是否存在矛盾
        let mut entity_descriptions: HashMap<String, Vec<String>> = HashMap::new();

        for node in &graph.nodes {
            for tag in &node.tags {
                if tag.starts_with("entity:") {
                    let entity = tag.trim_start_matches("entity:");
                    entity_descriptions
                        .entry(entity.to_string())
                        .or_default()
                        .push(format!("[{}] {}", node.title, node.description));
                }
            }
        }

        // 对每个实体，检查描述是否存在明显矛盾
        for (entity, descriptions) in &entity_descriptions {
            if descriptions.len() >= 2 {
                let all_text: String = descriptions.join(" ");
                let has_start = all_text.contains("启动") || all_text.contains("开始");
                let has_stop = all_text.contains("暂停") || all_text.contains("停止")
                    || all_text.contains("结束") || all_text.contains("取消");

                if has_start && has_stop && !all_text.contains("恢复") {
                    let desc_lower = all_text.to_lowercase();
                    if desc_lower.contains("同时") || desc_lower.contains("同时进行") {
                        contradictions.push(format!(
                            "实体 '{}' 同时存在 '启动' 和 '暂停/停止' 描述",
                            entity
                        ));
                    }
                }
            }
        }

        contradictions
    }

    /// 评估时间线完整性。
    fn evaluate_completeness(&self, graph: &EvidenceGraph, event_nodes: &[&Node]) -> f64 {
        if event_nodes.len() < 2 {
            return if event_nodes.is_empty() { 0.0 } else { 0.2 };
        }

        // 1. 时间跨度覆盖评分 (0.0 ~ 0.5)
        let timestamps: Vec<DateTime<Utc>> = event_nodes.iter()
            .filter_map(|n| n.timestamp)
            .collect();

        if timestamps.len() < 2 {
            return 0.2;
        }

        let min_ts = timestamps.iter().min().unwrap();
        let max_ts = timestamps.iter().max().unwrap();
        let span_hours = (*max_ts - *min_ts).num_hours().abs();
        let span_score = if span_hours > 0 {
            (span_hours as f64 / (span_hours as f64 + 168.0)).min(0.5)
        } else {
            0.1
        };

        // 2. 时间间隔均匀度评分 (0.0 ~ 0.3)
        let mut sorted_ts: Vec<&DateTime<Utc>> = timestamps.iter().collect();
        sorted_ts.sort();
        let gaps: Vec<i64> = sorted_ts.windows(2)
            .map(|w| (*w[1] - *w[0]).num_hours().abs())
            .collect();

        let gap_score = if gaps.is_empty() {
            0.0
        } else {
            let avg_gap = gaps.iter().sum::<i64>() as f64 / gaps.len() as f64;
            if avg_gap < 1.0 { 0.3 }
            else if avg_gap < 24.0 { 0.25 }
            else if avg_gap < 168.0 { 0.15 }
            else { 0.05 }
        };

        // 3. 实体覆盖评分 (0.0 ~ 0.2)
        let entity_count = graph.metadata.entities.len();
        let entity_score = if entity_count >= 3 { 0.2 }
            else if entity_count >= 1 { 0.1 }
            else { 0.0 };

        span_score + gap_score + entity_score
    }

    /// 检测信息缺口。
    fn detect_gaps(&self, graph: &EvidenceGraph, event_nodes: &[&Node]) -> Vec<String> {
        let mut gaps = Vec::new();

        if event_nodes.is_empty() {
            gaps.push("未找到任何事件节点".to_string());
            return gaps;
        }

        // 检测时间线断裂
        let events_with_ts: Vec<(&Node, DateTime<Utc>)> = event_nodes.iter()
            .filter_map(|n| n.timestamp.map(|ts| (*n, ts)))
            .collect();

        if events_with_ts.len() < 2 {
            // 少于2个有时间戳的事件，不检测断裂
        } else {
            let mut sorted = events_with_ts.clone();
            sorted.sort_by_key(|a| a.1);

            for window in sorted.windows(2) {
                let (first, ts1) = window[0];
                let (second, ts2) = window[1];
                let gap_hours = (ts2 - ts1).num_hours().abs();

                if gap_hours > self.max_time_gap_hours {
                    gaps.push(format!(
                        "时间线断裂：'{}' ({}) 和 '{}' ({}) 之间间隔 {} 天",
                        first.title,
                        ts1.format("%Y-%m-%d"),
                        second.title,
                        ts2.format("%Y-%m-%d"),
                        gap_hours / 24,
                    ));
                }
            }
        }

        // 检测孤立节点（没有边连接的节点）
        let connected_nodes: HashSet<NodeId> = graph.edges.iter()
            .flat_map(|e| vec![e.source_id, e.target_id])
            .collect();

        for node in &graph.nodes {
            if !connected_nodes.contains(&node.id) && node.kind == NodeKind::Event {
                gaps.push(format!(
                    "孤立事件：'{}' 没有与其他事件建立关联",
                    node.title,
                ));
            }
        }

        gaps
    }

    /// 计算综合置信度。
    fn calculate_confidence(
        &self,
        evidence_count: usize,
        consistency_score: f64,
        completeness_score: f64,
    ) -> f64 {
        let evidence_score = if evidence_count >= 5 { 0.4 }
            else if evidence_count >= 3 { 0.3 }
            else if evidence_count >= 1 { 0.15 }
            else { 0.0 };

        let consistency_contrib = consistency_score * 0.35;
        let completeness_contrib = completeness_score * 0.25;

        evidence_score + consistency_contrib + completeness_contrib
    }

    /// 生成改进建议。
    fn generate_suggestions(
        &self,
        evidence_count: usize,
        consistency_score: f64,
        completeness_score: f64,
        conflicts: &[ConflictInfo],
        gaps: &[String],
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        if evidence_count < self.min_evidence_count {
            suggestions.push(format!(
                "证据不足：仅找到 {} 条相关事件，建议扩展搜索范围或使用不同关键词",
                evidence_count,
            ));
        }

        if consistency_score < self.consistency_threshold {
            suggestions.push(format!(
                "一致性较低 ({:.2})：检测到 {} 处冲突，建议核实时间线和状态变更",
                consistency_score,
                conflicts.len(),
            ));
        }

        if completeness_score < self.completeness_threshold {
            suggestions.push("时间线不完整：建议补充更多时间点的事件记录".to_string());
        }

        for gap in gaps {
            if gap.starts_with("时间线断裂") {
                suggestions.push(format!("补充信息：{}，建议查询间隔期间的事件", gap));
            }
        }

        for conflict in conflicts {
            match conflict.conflict_type {
                ConflictType::TemporalConflict => {
                    suggestions.push(format!(
                        "时间冲突：{}，建议修正时间戳或移除错误的时间边",
                        conflict.description,
                    ));
                }
                ConflictType::StateConflict => {
                    suggestions.push(format!(
                        "状态冲突：{}，建议确认实体的最终状态",
                        conflict.description,
                    ));
                }
                ConflictType::FactConflict => {
                    suggestions.push(format!(
                        "事实矛盾：{}，建议核实信息来源",
                        conflict.description,
                    ));
                }
                ConflictType::TimelineGap => {
                    suggestions.push("时间线存在缺口，建议补充中间事件".to_string());
                }
            }
        }

        if suggestions.is_empty() {
            suggestions.push("记忆检索质量良好，无需改进".to_string());
        }

        suggestions
    }
}

// ─── 便捷评估函数 ──────────────────────────────────────

/// 快捷评估函数：从 EvidenceGraph 生成反思结果。
pub fn evaluate_memory_query(
    query: &str,
    route: &str,
    graph: &EvidenceGraph,
    latency_ms: u64,
) -> ReflectionResult {
    let evaluator = ReflectionEvaluator::default();
    evaluator.evaluate(query, route, graph, latency_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_evidence_graph::{Edge, EvidenceGraph, Node};

    fn make_event_node(title: &str, description: &str, hours_ago: i64) -> Node {
        Node::event(
            title,
            description,
            Utc::now() - chrono::Duration::hours(hours_ago),
        )
    }

    #[test]
    fn test_empty_graph() {
        let evaluator = ReflectionEvaluator::new();
        let graph = EvidenceGraph::empty("test query");
        let result = evaluator.evaluate("test query", "episode", &graph, 10);

        assert_eq!(result.evidence_count, 0);
        assert!(!result.has_conflicts);
        assert!((result.confidence - 0.35).abs() < 0.01, "empty graph confidence should be ~0.35, got {:.3}", result.confidence);
        assert!(!result.suggestions.is_empty());
    }

    #[test]
    fn test_single_event() {
        let mut graph = EvidenceGraph::empty("项目A状态");
        graph.add_node(make_event_node("项目A启动", "项目A正式启动", 48));

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("项目A状态", "episode", &graph, 5);

        assert_eq!(result.evidence_count, 1);
        assert!(!result.has_conflicts);
        assert!((result.completeness_score - 0.2).abs() < 0.1);
    }

    #[test]
    fn test_temporal_conflict_correct_order() {
        let mut graph = EvidenceGraph::empty("test");
        let earlier = make_event_node("早期事件", "72小时前", 72);
        let later = make_event_node("晚期事件", "1小时前", 1);

        graph.add_node(earlier.clone());
        graph.add_node(later.clone());

        // 正确的时间边：earlier → later
        graph.add_edge(Edge::temporal(earlier.id, later.id));

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("test", "episode", &graph, 5);

        // 正确顺序不应该有冲突
        assert!(!result.has_conflicts || result.conflicts.iter().all(|c| {
            !matches!(c.conflict_type, ConflictType::TemporalConflict)
        }));
    }

    #[test]
    fn test_temporal_conflict_wrong_order() {
        let mut graph = EvidenceGraph::empty("test");
        let earlier = make_event_node("早期事件", "72小时前", 72);
        let later = make_event_node("晚期事件", "1小时前", 1);

        graph.add_node(earlier.clone());
        graph.add_node(later.clone());

        // 错误的时间边：later → earlier（表示later早于earlier），但时间戳相反
        graph.add_edge(Edge::temporal(later.id, earlier.id));

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("test", "episode", &graph, 5);

        assert!(result.has_conflicts, "should detect temporal conflict");
        let has_temporal = result.conflicts.iter().any(|c|
            matches!(c.conflict_type, ConflictType::TemporalConflict)
        );
        assert!(has_temporal, "should have temporal conflict type");
    }

    #[test]
    fn test_multi_event_consistency() {
        let mut graph = EvidenceGraph::empty("项目A");
        let n1 = make_event_node("项目A启动", "项目A启动", 168);
        let n2 = make_event_node("项目A进行中", "项目A正常进行", 120);
        let n3 = make_event_node("项目A暂停", "供应商问题导致暂停", 72);

        graph.add_node(n1);
        graph.add_node(n2);
        graph.add_node(n3);

        graph.add_edge(Edge::temporal(
            graph.nodes[0].id,
            graph.nodes[1].id,
        ));
        graph.add_edge(Edge::temporal(
            graph.nodes[1].id,
            graph.nodes[2].id,
        ));

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("项目A", "episode", &graph, 10);

        assert!(!result.has_conflicts, "correct timeline should have no conflicts");
        assert_eq!(result.evidence_count, 3);
        assert!(result.confidence > 0.5, "good evidence should have high confidence");
    }

    #[test]
    fn test_completeness_multi_event() {
        let mut graph = EvidenceGraph::empty("完整查询");
        for i in 0..5 {
            let n = make_event_node(
                &format!("事件{}", i),
                &format!("描述{}", i),
                i as i64 * 24,
            );
            graph.add_node(n);
        }

        graph.metadata.entities = vec!["project:A".to_string(), "person:张三".to_string(), "org:公司X".to_string()];

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("完整查询", "episode", &graph, 15);

        assert_eq!(result.evidence_count, 5);
        assert!(result.completeness_score > 0.5, "5 events with entities should have good completeness");
    }

    #[test]
    fn test_detect_gaps() {
        let mut graph = EvidenceGraph::empty("gap test");
        let n1 = make_event_node("事件A", "A", 720); // 30天前
        let n2 = make_event_node("事件B", "B", 1);   // 1小时前
        let n1_id = n1.id;
        let n2_id = n2.id;

        graph.add_node(n1);
        graph.add_node(n2);
        graph.add_edge(Edge::temporal(n1_id, n2_id));

        let evaluator = ReflectionEvaluator::new()
            .with_max_time_gap(48); // 48小时阈值
        let result = evaluator.evaluate("gap test", "episode", &graph, 5);

        let has_gap = result.gaps.iter().any(|g| g.starts_with("时间线断裂"));
        assert!(has_gap, "should detect timeline gap");
    }

    #[test]
    fn test_isolated_node_detection() {
        let mut graph = EvidenceGraph::empty("孤立检测");
        let n1 = make_event_node("连接事件", "有边连接", 48);
        let n2 = make_event_node("孤立事件", "没有连接", 24);

        graph.add_node(n1.clone());
        graph.add_node(n2.clone());
        // 连接 n1 到自身（自环）
        graph.add_edge(Edge::temporal(n1.id, n1.id));

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("孤立检测", "episode", &graph, 5);

        let has_isolated = result.gaps.iter().any(|g| g.starts_with("孤立事件"));
        assert!(has_isolated, "should detect isolated node");
    }

    #[test]
    fn test_confidence_calculation() {
        let evaluator = ReflectionEvaluator::new();

        let high = evaluator.calculate_confidence(5, 1.0, 0.8);
        assert!(high > 0.7, "good evidence should give high confidence, got {:.2}", high);

        let none = evaluator.calculate_confidence(0, 1.0, 0.0);
        assert!(none < 0.5, "no evidence should give low confidence, got {:.2}", none);

        let conflicted = evaluator.calculate_confidence(3, 0.2, 0.5);
        assert!(conflicted < 0.6, "conflicts should reduce confidence, got {:.2}", conflicted);
    }

    #[test]
    fn test_suggestions_generation() {
        let evaluator = ReflectionEvaluator::new();
        let conflicts = vec![ConflictInfo {
            conflict_type: ConflictType::TemporalConflict,
            description: "测试冲突".to_string(),
            node_ids: vec![],
            severity: 0.7,
        }];
        let gaps = vec!["时间线断裂：事件A和事件B之间间隔5天".to_string()];

        let suggestions = evaluator.generate_suggestions(0, 0.3, 0.2, &conflicts, &gaps);
        assert!(!suggestions.is_empty(), "should generate suggestions for poor results");
        assert!(suggestions.iter().any(|s| s.contains("证据不足")));
    }

    #[test]
    fn test_good_result_no_suggestions() {
        let evaluator = ReflectionEvaluator::new();
        let suggestions = evaluator.generate_suggestions(5, 0.9, 0.7, &[], &[]);
        assert!(suggestions.iter().any(|s| s.contains("良好")));
    }

    #[test]
    fn test_incompatible_states() {
        let evaluator = ReflectionEvaluator::new();
        let states = vec!["active".to_string(), "inactive".to_string()];
        let pairs = evaluator.find_incompatible_states(&states);
        assert!(!pairs.is_empty(), "active and inactive are incompatible");
        assert_eq!(pairs[0], ("active".to_string(), "inactive".to_string()));
    }

    #[test]
    fn test_compatible_states() {
        let evaluator = ReflectionEvaluator::new();
        let states = vec!["active".to_string(), "completed".to_string()];
        let pairs = evaluator.find_incompatible_states(&states);
        assert!(pairs.is_empty(), "active and completed are compatible");
    }

    #[test]
    fn test_evaluate_with_tags() {
        let mut graph = EvidenceGraph::empty("标签测试");
        let mut node = make_event_node("带标签事件", "带标签的事件", 24);
        node = node.with_tag("entity:project:项目A");
        node = node.with_tag("state_change:active → paused");
        graph.add_node(node);

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("标签测试", "episode", &graph, 5);

        assert_eq!(result.evidence_count, 1);
        assert!(!result.has_conflicts);
    }

    #[test]
    fn test_evaluate_with_complex_graph() {
        let mut graph = EvidenceGraph::empty("复杂查询");
        // events[0] = oldest (60h ago), events[5] = newest (0h ago)
        // temporal edges 0→1→2→3→4→5 follow chronological order
        let nodes: Vec<Node> = (0..6).map(|i| {
            make_event_node(
                &format!("事件{}", i),
                &format!("描述{}", i),
                (5 - i) as i64 * 12,  // 0→60h, 1→48h, 2→36h, 3→24h, 4→12h, 5→0h
            )
        }).collect();

        for n in &nodes {
            graph.add_node(n.clone());
        }

        for i in 0..nodes.len()-1 {
            graph.add_edge(Edge::temporal(nodes[i].id, nodes[i+1].id));
        }

        graph.metadata.entities = vec![
            "project:项目A".to_string(),
            "person:张三".to_string(),
            "org:公司X".to_string(),
            "tool:Rust".to_string(),
        ];

        let evaluator = ReflectionEvaluator::new();
        let result = evaluator.evaluate("复杂查询", "deep", &graph, 20);

        assert_eq!(result.evidence_count, 6);
        if result.has_conflicts {
            panic!("Unexpected conflicts: {:?}", result.conflicts);
        }
        assert!(result.confidence > 0.6, "complex good graph should have high confidence");

        let has_timeline_gap = result.gaps.iter().any(|g| g.starts_with("时间线断裂"));
        assert!(!has_timeline_gap, "6 events at 12h intervals should not have gaps");
    }

    #[test]
    fn test_evaluate_fast_function() {
        let graph = EvidenceGraph::empty("快捷测试");
        let result = evaluate_memory_query("快捷测试", "conversation", &graph, 3);

        assert_eq!(result.query, "快捷测试");
        assert_eq!(result.route_used, "conversation");
    }

    #[test]
    fn test_configuration() {
        let evaluator = ReflectionEvaluator::new()
            .with_min_evidence(3)
            .with_consistency_threshold(0.8)
            .with_completeness_threshold(0.6)
            .with_max_time_gap(24);

        assert_eq!(evaluator.min_evidence_count, 3);
        assert!((evaluator.consistency_threshold - 0.8).abs() < 0.01);
        assert!((evaluator.completeness_threshold - 0.6).abs() < 0.01);
        assert_eq!(evaluator.max_time_gap_hours, 24);
    }
}
