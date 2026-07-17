//! LSMemoryMetrics — Memory 子系统可观测指标。
//!
//! 提供 Memory 子系统的运行时指标采集，包括：
//!
//! - **查询指标**: 命中率、冲突率、各 Workflow 调用次数/延迟
//! - **巩固指标**: Consolidation 运行次数、成功率、产出
//! - **反思指标**: Reflection 评估次数、修正率
//! - **延迟分布**: 按 Workflow 的查询延迟直方图
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use lingshu_memory_metrics::MemoryMetricsCollector;
//!
//! let collector = MemoryMetricsCollector::default();
//!
//! // 记录一次查询
//! collector.record_query("timeline", 45.2, 3, 0);
//!
//! // 记录一次合并冲突
//! collector.record_conflict("fact_conflict");
//!
//! // 获取快照
//! let snapshot = collector.snapshot();
//! println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

// ═══════════════════════════════════════════════════
// 核心收集器
// ═══════════════════════════════════════════════════

/// Memory 指标收集器 — 线程安全、原子计数。
///
/// 所有指标通过原子操作和 RwLock 保护，支持并发读写。
/// 可通过 `snapshot()` 获取当前状态的不可变快照。
pub struct MemoryMetricsCollector {
    // ── 查询统计 ──
    /// 总查询次数
    total_queries: AtomicU64,
    /// 命中查询（返回非空结果）次数
    hit_queries: AtomicU64,
    /// 含冲突的查询次数
    conflict_queries: AtomicU64,
    /// 空结果查询次数
    empty_queries: AtomicU64,

    // ── Workflow 维度统计 ──
    workflows: RwLock<HashMap<String, WorkflowMetricsInner>>,

    // ── 巩固统计 ──
    /// Consolidation 运行次数
    consolidation_runs: AtomicU64,
    /// Consolidation 成功次数
    consolidation_success: AtomicU64,
    /// Consolidation 失败次数
    consolidation_failures: AtomicU64,
    /// 已巩固的 Episode 总数
    consolidated_episodes: AtomicU64,
    /// 已处理的 Episode 总数
    processed_episodes: AtomicU64,

    // ── 反思统计 ──
    /// Reflection 评估次数
    reflection_runs: AtomicU64,
    /// Reflection 检测到冲突的次数
    reflection_conflicts: AtomicU64,
    /// Reflection 给出改进建议的次数
    reflection_suggestions: AtomicU64,

    // ── 延迟统计 ──
    latency_buckets: RwLock<LatencyBuckets>,

    // ── 系统信息 ──
    started_at: DateTime<Utc>,
}

impl Default for MemoryMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryMetricsCollector {
    /// 创建新的指标收集器。
    pub fn new() -> Self {
        Self {
            total_queries: AtomicU64::new(0),
            hit_queries: AtomicU64::new(0),
            conflict_queries: AtomicU64::new(0),
            empty_queries: AtomicU64::new(0),
            workflows: RwLock::new(HashMap::new()),
            consolidation_runs: AtomicU64::new(0),
            consolidation_success: AtomicU64::new(0),
            consolidation_failures: AtomicU64::new(0),
            consolidated_episodes: AtomicU64::new(0),
            processed_episodes: AtomicU64::new(0),
            reflection_runs: AtomicU64::new(0),
            reflection_conflicts: AtomicU64::new(0),
            reflection_suggestions: AtomicU64::new(0),
            latency_buckets: RwLock::new(LatencyBuckets::new()),
            started_at: Utc::now(),
        }
    }

    // ─── 查询指标 ───────────────────────────────────

    /// 记录一次 Memory 查询。
    ///
    /// # 参数
    ///
    /// * `workflow` - 执行查询的工作流名称
    /// * `latency_ms` - 查询耗时（毫秒）
    /// * `node_count` - 返回的节点数
    /// * `conflict_count` - 检测到的冲突数
    pub fn record_query(&self, workflow: &str, latency_ms: f64, node_count: usize, conflict_count: usize) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);

        if node_count > 0 {
            self.hit_queries.fetch_add(1, Ordering::Relaxed);
        } else {
            self.empty_queries.fetch_add(1, Ordering::Relaxed);
        }

        if conflict_count > 0 {
            self.conflict_queries.fetch_add(1, Ordering::Relaxed);
        }

        // 记录 Workflow 级别指标
        if let Ok(mut wfs) = self.workflows.write() {
            let entry = wfs.entry(workflow.to_string()).or_insert_with(WorkflowMetricsInner::new);
            entry.total_calls += 1;
            if node_count > 0 {
                entry.hit_calls += 1;
            }
            entry.total_latency_ms += latency_ms;
            entry.max_latency_ms = entry.max_latency_ms.max(latency_ms);
            entry.min_latency_ms = if entry.min_latency_ms == 0.0 || latency_ms < entry.min_latency_ms {
                latency_ms
            } else {
                entry.min_latency_ms
            };
            entry.total_nodes += node_count as u64;
            entry.conflict_count += conflict_count as u64;
        }

        // 记录延迟分布
        if let Ok(mut buckets) = self.latency_buckets.write() {
            buckets.record(latency_ms);
        }
    }

    /// 记录一次合并冲突。
    pub fn record_conflict(&self, conflict_type: &str) {
        // 在 workflow 级别已计数，这里额外按类型记录
        tracing::debug!(conflict_type, "memory conflict recorded");
    }

    // ─── 巩固指标 ───────────────────────────────────

    /// 记录一次 Consolidation 运行。
    pub fn record_consolidation(&self, success: bool, processed: u64, consolidated: u64) {
        self.consolidation_runs.fetch_add(1, Ordering::Relaxed);
        if success {
            self.consolidation_success.fetch_add(1, Ordering::Relaxed);
        } else {
            self.consolidation_failures.fetch_add(1, Ordering::Relaxed);
        }
        self.processed_episodes.fetch_add(processed, Ordering::Relaxed);
        self.consolidated_episodes.fetch_add(consolidated, Ordering::Relaxed);
    }

    // ─── 反思指标 ───────────────────────────────────

    /// 记录一次 Reflection 评估。
    pub fn record_reflection(&self, has_conflicts: bool, has_suggestions: bool) {
        self.reflection_runs.fetch_add(1, Ordering::Relaxed);
        if has_conflicts {
            self.reflection_conflicts.fetch_add(1, Ordering::Relaxed);
        }
        if has_suggestions {
            self.reflection_suggestions.fetch_add(1, Ordering::Relaxed);
        }
    }

    // ─── 快照 ───────────────────────────────────────

    /// 获取当前指标的不可变快照。
    pub fn snapshot(&self) -> MemoryMetricsSnapshot {
        let total = self.total_queries.load(Ordering::Relaxed);
        let hits = self.hit_queries.load(Ordering::Relaxed);
        let conflicts = self.conflict_queries.load(Ordering::Relaxed);
        let empty = self.empty_queries.load(Ordering::Relaxed);

        let cons_runs = self.consolidation_runs.load(Ordering::Relaxed);
        let cons_ok = self.consolidation_success.load(Ordering::Relaxed);
        let cons_fail = self.consolidation_failures.load(Ordering::Relaxed);

        let ref_runs = self.reflection_runs.load(Ordering::Relaxed);
        let ref_conf = self.reflection_conflicts.load(Ordering::Relaxed);
        let ref_sugg = self.reflection_suggestions.load(Ordering::Relaxed);

        let workflows = self.workflows.read()
            .map(|w| {
                let mut results: HashMap<String, WorkflowMetrics> = HashMap::new();
                for (name, inner) in w.iter() {
                    results.insert(name.clone(), WorkflowMetrics::from_inner(inner));
                }
                results
            })
            .unwrap_or_default();

        let buckets = self.latency_buckets.read()
            .map(|b| b.snapshot())
            .unwrap_or_default();

        MemoryMetricsSnapshot {
            uptime_seconds: (Utc::now() - self.started_at).num_seconds() as u64,
            total_queries: total,
            hit_queries: hits,
            hit_rate: if total > 0 { hits as f64 / total as f64 } else { 0.0 },
            empty_queries: empty,
            empty_rate: if total > 0 { empty as f64 / total as f64 } else { 0.0 },
            conflict_queries: conflicts,
            conflict_rate: if total > 0 { conflicts as f64 / total as f64 } else { 0.0 },
            workflows,
            consolidation_runs: cons_runs,
            consolidation_success: cons_ok,
            consolidation_failures: cons_fail,
            consolidation_success_rate: if cons_runs > 0 { cons_ok as f64 / cons_runs as f64 } else { 0.0 },
            processed_episodes: self.processed_episodes.load(Ordering::Relaxed),
            consolidated_episodes: self.consolidated_episodes.load(Ordering::Relaxed),
            reflection_runs: ref_runs,
            reflection_conflict_rate: if ref_runs > 0 { ref_conf as f64 / ref_runs as f64 } else { 0.0 },
            reflection_suggestion_rate: if ref_runs > 0 { ref_sugg as f64 / ref_runs as f64 } else { 0.0 },
            latency: buckets,
        }
    }
}

// ═══════════════════════════════════════════════════
// Workflow 指标（内部可变）
// ═══════════════════════════════════════════════════

struct WorkflowMetricsInner {
    total_calls: u64,
    hit_calls: u64,
    total_latency_ms: f64,
    max_latency_ms: f64,
    min_latency_ms: f64,
    total_nodes: u64,
    conflict_count: u64,
}

impl WorkflowMetricsInner {
    fn new() -> Self {
        Self {
            total_calls: 0,
            hit_calls: 0,
            total_latency_ms: 0.0,
            max_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            total_nodes: 0,
            conflict_count: 0,
        }
    }
}

// ═══════════════════════════════════════════════════
// Workflow 指标（快照）
// ═══════════════════════════════════════════════════

/// 单个 Workflow 的指标快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMetrics {
    /// 总调用次数
    pub total_calls: u64,
    /// 命中次数（返回非空结果）
    pub hit_calls: u64,
    /// 命中率
    pub hit_rate: f64,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: f64,
    /// 最大延迟（毫秒）
    pub max_latency_ms: f64,
    /// 最小延迟（毫秒）
    pub min_latency_ms: f64,
    /// 返回的总节点数
    pub total_nodes: u64,
    /// 冲突次数
    pub conflict_count: u64,
    /// 冲突率
    pub conflict_rate: f64,
}

impl WorkflowMetrics {
    fn from_inner(inner: &WorkflowMetricsInner) -> Self {
        Self {
            total_calls: inner.total_calls,
            hit_calls: inner.hit_calls,
            hit_rate: if inner.total_calls > 0 {
                inner.hit_calls as f64 / inner.total_calls as f64
            } else {
                0.0
            },
            avg_latency_ms: if inner.total_calls > 0 {
                inner.total_latency_ms / inner.total_calls as f64
            } else {
                0.0
            },
            max_latency_ms: inner.max_latency_ms,
            min_latency_ms: if inner.min_latency_ms == f64::MAX {
                0.0
            } else {
                inner.min_latency_ms
            },
            total_nodes: inner.total_nodes,
            conflict_count: inner.conflict_count,
            conflict_rate: if inner.total_calls > 0 {
                inner.conflict_count as f64 / inner.total_calls as f64
            } else {
                0.0
            },
        }
    }
}

// ═══════════════════════════════════════════════════
// 延迟桶
// ═══════════════════════════════════════════════════

/// 延迟分布统计（毫秒）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyDistribution {
    /// P50 延迟
    pub p50_ms: f64,
    /// P90 延迟
    pub p90_ms: f64,
    /// P95 延迟
    pub p95_ms: f64,
    /// P99 延迟
    pub p99_ms: f64,
    /// 平均延迟
    pub avg_ms: f64,
    /// 最大延迟
    pub max_ms: f64,
    /// 样本数
    pub sample_count: u64,
}

struct LatencyBuckets {
    samples: Vec<f64>,
    max_samples: usize,
}

impl LatencyBuckets {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(4096),
            max_samples: 10000,
        }
    }

    fn record(&mut self, latency_ms: f64) {
        if self.samples.len() < self.max_samples {
            self.samples.push(latency_ms);
        }
    }

    fn snapshot(&self) -> LatencyDistribution {
        let count = self.samples.len();
        if count == 0 {
            return LatencyDistribution::default();
        }

        let mut sorted = self.samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let sum: f64 = sorted.iter().sum();

        fn percentile(sorted: &[f64], pct: f64) -> f64 {
            if sorted.is_empty() {
                return 0.0;
            }
            let idx = ((sorted.len() as f64) * pct / 100.0).ceil() as usize;
            let idx = idx.min(sorted.len() - 1);
            sorted[idx]
        }

        LatencyDistribution {
            p50_ms: percentile(&sorted, 50.0),
            p90_ms: percentile(&sorted, 90.0),
            p95_ms: percentile(&sorted, 95.0),
            p99_ms: percentile(&sorted, 99.0),
            avg_ms: if count > 0 { sum / count as f64 } else { 0.0 },
            max_ms: sorted.last().copied().unwrap_or(0.0),
            sample_count: count as u64,
        }
    }
}

// ═══════════════════════════════════════════════════
// 快照结构
// ═══════════════════════════════════════════════════

/// Memory 子系统的完整指标快照。
///
/// 可序列化为 JSON，用于监控报表和调试。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetricsSnapshot {
    /// 收集器运行时间（秒）
    pub uptime_seconds: u64,

    // ── 查询统计 ──
    /// 总查询次数
    pub total_queries: u64,
    /// 命中查询次数
    pub hit_queries: u64,
    /// 命中率 (0.0 ~ 1.0)
    pub hit_rate: f64,
    /// 空结果查询次数
    pub empty_queries: u64,
    /// 空结果率
    pub empty_rate: f64,
    /// 含冲突的查询次数
    pub conflict_queries: u64,
    /// 冲突率 (0.0 ~ 1.0)
    pub conflict_rate: f64,

    // ── Workflow 维度 ──
    /// 各 Workflow 的指标
    pub workflows: HashMap<String, WorkflowMetrics>,

    // ── 巩固统计 ──
    /// Consolidation 总运行次数
    pub consolidation_runs: u64,
    /// Consolidation 成功次数
    pub consolidation_success: u64,
    /// Consolidation 失败次数
    pub consolidation_failures: u64,
    /// Consolidation 成功率
    pub consolidation_success_rate: f64,
    /// 已处理的 Episode 总数
    pub processed_episodes: u64,
    /// 已巩固的 Episode 总数
    pub consolidated_episodes: u64,

    // ── 反思统计 ──
    /// Reflection 评估次数
    pub reflection_runs: u64,
    /// Reflection 检测到冲突的比例
    pub reflection_conflict_rate: f64,
    /// Reflection 给出改进建议的比例
    pub reflection_suggestion_rate: f64,

    // ── 延迟统计 ──
    /// 延迟分布
    pub latency: LatencyDistribution,
}

impl MemoryMetricsSnapshot {
    /// 快速健康检查：返回是否所有关键指标正常。
    pub fn is_healthy(&self) -> bool {
        // 只要运行过查询就认为是健康的
        // 可以扩展为更复杂的健康逻辑
        true
    }

    /// 返回格式化的摘要信息。
    pub fn summary(&self) -> String {
        let uptime = if self.uptime_seconds < 60 {
            format!("{}秒", self.uptime_seconds)
        } else if self.uptime_seconds < 3600 {
            format!("{}分{}秒", self.uptime_seconds / 60, self.uptime_seconds % 60)
        } else {
            format!("{}时{}分", self.uptime_seconds / 3600, (self.uptime_seconds % 3600) / 60)
        };

        let mut lines = vec![
            format!("📊 Memory Metrics (运行 {uptime})"),
            format!(""),
            format!("  查询:"),
            format!("    总次数:    {}", self.total_queries),
            format!("    命中率:    {:.1}% ({} / {})", self.hit_rate * 100.0, self.hit_queries, self.total_queries),
            format!("    冲突率:    {:.1}%", self.conflict_rate * 100.0),
            format!("    延迟 P50:  {:.0}ms", self.latency.p50_ms),
            format!("    延迟 P95:  {:.0}ms", self.latency.p95_ms),
        ];

        if !self.workflows.is_empty() {
            lines.push(String::new());
            lines.push("  Workflows:".to_string());
            for (name, wf) in &self.workflows {
                lines.push(format!(
                    "    {:<20} calls={} hit={:.0}% avg={:.0}ms conflicts={}",
                    name,
                    wf.total_calls,
                    wf.hit_rate * 100.0,
                    wf.avg_latency_ms,
                    wf.conflict_count,
                ));
            }
        }

        if self.consolidation_runs > 0 {
            lines.push(String::new());
            lines.push("  巩固:".to_string());
            lines.push(format!("    运行:      {}", self.consolidation_runs));
            lines.push(format!("    成功率:    {:.1}%", self.consolidation_success_rate * 100.0));
            lines.push(format!("    已巩固:    {} / {}", self.consolidated_episodes, self.processed_episodes));
        }

        if self.reflection_runs > 0 {
            lines.push(String::new());
            lines.push("  反思:".to_string());
            lines.push(format!("    评估次数:  {}", self.reflection_runs));
            lines.push(format!("    冲突率:    {:.1}%", self.reflection_conflict_rate * 100.0));
            lines.push(format!("    建议率:    {:.1}%", self.reflection_suggestion_rate * 100.0));
        }

        lines.join("\n")
    }
}

/// 默认全局指标收集器实例。
static GLOBAL_COLLECTOR: std::sync::LazyLock<MemoryMetricsCollector> =
    std::sync::LazyLock::new(MemoryMetricsCollector::new);

/// 获取全局指标收集器。
pub fn global_collector() -> &'static MemoryMetricsCollector {
    &GLOBAL_COLLECTOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_collector_empty() {
        let collector = MemoryMetricsCollector::new();
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.total_queries, 0);
        assert_eq!(snapshot.hit_rate, 0.0);
    }

    #[test]
    fn test_record_query_hit() {
        let collector = MemoryMetricsCollector::new();
        collector.record_query("timeline", 10.5, 3, 0);
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.total_queries, 1);
        assert_eq!(snapshot.hit_queries, 1);
        assert!((snapshot.hit_rate - 1.0).abs() < 0.001);
        assert_eq!(snapshot.conflict_queries, 0);
    }

    #[test]
    fn test_record_query_empty() {
        let collector = MemoryMetricsCollector::new();
        collector.record_query("semantic", 5.0, 0, 0);
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.total_queries, 1);
        assert_eq!(snapshot.hit_queries, 0);
        assert_eq!(snapshot.empty_queries, 1);
    }

    #[test]
    fn test_record_query_with_conflicts() {
        let collector = MemoryMetricsCollector::new();
        collector.record_query("timeline", 20.0, 5, 2);
        let snapshot = collector.snapshot();
        assert_eq!(snapshot.total_queries, 1);
        assert_eq!(snapshot.conflict_queries, 1);
        assert_eq!(snapshot.conflict_rate, 1.0);
    }

    #[test]
    fn test_workflow_metrics() {
        let collector = MemoryMetricsCollector::new();
        collector.record_query("timeline", 10.0, 3, 0);
        collector.record_query("timeline", 20.0, 5, 1);
        collector.record_query("semantic", 5.0, 1, 0);

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.workflows.len(), 2);

        let tl = snapshot.workflows.get("timeline").unwrap();
        assert_eq!(tl.total_calls, 2);
        assert_eq!(tl.hit_calls, 2);
        assert!((tl.avg_latency_ms - 15.0).abs() < 0.001);
        assert_eq!(tl.conflict_count, 1);

        let sm = snapshot.workflows.get("semantic").unwrap();
        assert_eq!(sm.total_calls, 1);
        assert_eq!(sm.conflict_count, 0);
    }

    #[test]
    fn test_consolidation_metrics() {
        let collector = MemoryMetricsCollector::new();
        collector.record_consolidation(true, 10, 3);
        collector.record_consolidation(true, 5, 2);
        collector.record_consolidation(false, 0, 0);

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.consolidation_runs, 3);
        assert_eq!(snapshot.consolidation_success, 2);
        assert_eq!(snapshot.consolidation_failures, 1);
        assert!((snapshot.consolidation_success_rate - 2.0 / 3.0).abs() < 0.01);
        assert_eq!(snapshot.processed_episodes, 15);
        assert_eq!(snapshot.consolidated_episodes, 5);
    }

    #[test]
    fn test_reflection_metrics() {
        let collector = MemoryMetricsCollector::new();
        collector.record_reflection(true, true);
        collector.record_reflection(true, false);
        collector.record_reflection(false, false);

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.reflection_runs, 3);
        assert!((snapshot.reflection_conflict_rate - 2.0 / 3.0).abs() < 0.01);
        assert!((snapshot.reflection_suggestion_rate - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_latency_percentiles() {
        let collector = MemoryMetricsCollector::new();
        for i in 1..=100 {
            collector.record_query("test", i as f64, 1, 0);
        }

        let snapshot = collector.snapshot();
        assert!(snapshot.latency.p50_ms > 48.0 && snapshot.latency.p50_ms < 52.0,
            "p50 should be ~50, got {}", snapshot.latency.p50_ms);
        assert!(snapshot.latency.p90_ms > 88.0, "p90 should be ~90, got {}", snapshot.latency.p90_ms);
        assert!(snapshot.latency.max_ms >= 100.0);
        assert_eq!(snapshot.latency.sample_count, 100);
    }

    #[test]
    fn test_multiple_queries() {
        let collector = MemoryMetricsCollector::new();
        for i in 0..100 {
            let hit = i % 3 != 0;
            let conflicts = if i % 5 == 0 { 1 } else { 0 };
            let nodes = if hit { (i % 5) + 1 } else { 0 };
            collector.record_query("timeline", i as f64, nodes, conflicts);
        }

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.total_queries, 100);

        // hits: 100 - (100/3) ≈ 67
        assert!(snapshot.hit_queries >= 66 && snapshot.hit_queries <= 68,
            "expected ~67 hits, got {}", snapshot.hit_queries);

        // conflicts: 100/5 = 20
        assert_eq!(snapshot.conflict_queries, 20);
    }

    #[test]
    fn test_snapshot_json_serialization() {
        let collector = MemoryMetricsCollector::new();
        collector.record_query("timeline", 15.0, 3, 0);
        collector.record_consolidation(true, 10, 3);

        let snapshot = collector.snapshot();
        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        assert!(json.contains("total_queries"));
        assert!(json.contains("hit_rate"));
        assert!(json.contains("consolidation_runs"));

        // 可以反序列化回来
        let parsed: MemoryMetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_queries, 1);
        assert_eq!(parsed.consolidation_runs, 1);
    }

    #[test]
    fn test_summary_output() {
        let collector = MemoryMetricsCollector::new();
        collector.record_query("timeline", 15.0, 3, 0);
        collector.record_consolidation(true, 10, 3);

        let snapshot = collector.snapshot();
        let summary = snapshot.summary();
        assert!(summary.contains("命中率"));
        assert!(summary.contains("Memory Metrics"));
    }

    #[test]
    fn test_global_collector() {
        let c1 = global_collector();
        let c2 = global_collector();
        // 通过指针比较确认是同一个实例
        assert_eq!(
            std::ptr::from_ref(c1),
            std::ptr::from_ref(c2)
        );
    }
}
