//! 路由策略实现.
//!
//! 每种策略决定如何从启用的后端列表中选择下一个调用目标。

use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::metrics::MetricsCollector;

/// 路由策略 trait.
#[async_trait]
pub trait RouterStrategy {
    /// 从可用后端列表中选择一个后端.
    async fn select(
        &self,
        backends: &[String],
        metrics: &MetricsCollector,
    ) -> Option<String>;
}

// ── Priority Strategy ──────────────────────────────

/// 优先级策略 — 按条目 `priority` 排序选择.
///
/// 注意：此策略的优先级在 `BackendEntry` 上定义，但在此处只做简单的
/// 按选择第一个可用后端。实际优先级排序在注册时由条目顺序管理。
pub struct PriorityStrategy;

impl PriorityStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouterStrategy for PriorityStrategy {
    async fn select(&self, backends: &[String], _metrics: &MetricsCollector) -> Option<String> {
        backends.first().cloned()
    }
}

// ── Fallback Strategy ─────────────────────────────

/// 降级策略 — 如同 Priority，但记录降级事件.
pub struct FallbackStrategy;

impl FallbackStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouterStrategy for FallbackStrategy {
    async fn select(&self, backends: &[String], metrics: &MetricsCollector) -> Option<String> {
        // 选择成功率最高的后端
        let mut candidates: Vec<(String, f64)> = backends
            .iter()
            .map(|name| {
                let rate = metrics
                    .get(name)
                    .map(|m| m.success_rate())
                    .unwrap_or(1.0);
                (name.clone(), rate)
            })
            .collect();

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.first().map(|(name, _)| name.clone())
    }
}

// ── Latency Strategy ──────────────────────────────

/// 延迟策略 — 选择平均延迟最低的后端.
pub struct LatencyStrategy;

impl LatencyStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouterStrategy for LatencyStrategy {
    async fn select(&self, backends: &[String], metrics: &MetricsCollector) -> Option<String> {
        let mut candidates: Vec<(String, f64)> = backends
            .iter()
            .map(|name| {
                let avg_latency = metrics
                    .get(name)
                    .map(|m| m.avg_latency_ms)
                    .unwrap_or(f64::MAX);
                (name.clone(), avg_latency)
            })
            .collect();

        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.first().map(|(name, _)| name.clone())
    }
}

// ── Cost Strategy ─────────────────────────────────

/// 成本策略 — 选择每次请求成本最低的后端.
pub struct CostStrategy;

impl CostStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouterStrategy for CostStrategy {
    async fn select(&self, backends: &[String], metrics: &MetricsCollector) -> Option<String> {
        let mut candidates: Vec<(String, f64)> = backends
            .iter()
            .map(|name| {
                let cost = metrics
                    .get(name)
                    .map(|m| m.total_cost)
                    .unwrap_or(0.0);
                (name.clone(), cost)
            })
            .collect();

        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.first().map(|(name, _)| name.clone())
    }
}

// ── Round-Robin Strategy ──────────────────────────

/// 轮询策略 — 循环选择后端.
pub struct RoundRobinStrategy {
    counter: AtomicUsize,
}

impl RoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl RouterStrategy for RoundRobinStrategy {
    async fn select(&self, backends: &[String], _metrics: &MetricsCollector) -> Option<String> {
        if backends.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % backends.len();
        backends.get(idx).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_priority_selects_first() {
        let strategy = PriorityStrategy::new();
        let backends = vec!["a".into(), "b".into(), "c".into()];
        let metrics = MetricsCollector::new();
        let selected = strategy.select(&backends, &metrics).await;
        assert_eq!(selected, Some("a".into()));
    }

    #[tokio::test]
    async fn test_priority_empty() {
        let strategy = PriorityStrategy::new();
        let metrics = MetricsCollector::new();
        let selected = strategy.select(&[], &metrics).await;
        assert_eq!(selected, None);
    }

    #[tokio::test]
    async fn test_fallback_selects_highest_success_rate() {
        let strategy = FallbackStrategy::new();
        let backends = vec!["healthy".into(), "unhealthy".into()];
        let mut metrics = MetricsCollector::new();
        metrics.record("healthy", Duration::from_millis(100), 0.0, true);
        metrics.record("healthy", Duration::from_millis(100), 0.0, true);
        metrics.record("unhealthy", Duration::from_millis(100), 0.0, false);
        metrics.record("unhealthy", Duration::from_millis(100), 0.0, false);

        let selected = strategy.select(&backends, &metrics).await;
        assert_eq!(selected, Some("healthy".into()));
    }

    #[tokio::test]
    async fn test_latency_selects_lowest() {
        let strategy = LatencyStrategy::new();
        let backends = vec!["slow".into(), "fast".into()];
        let mut metrics = MetricsCollector::new();
        metrics.record("slow", Duration::from_millis(500), 0.0, true);
        metrics.record("fast", Duration::from_millis(10), 0.0, true);

        let selected = strategy.select(&backends, &metrics).await;
        assert_eq!(selected, Some("fast".into()));
    }

    #[tokio::test]
    async fn test_round_robin_cycles() {
        let strategy = RoundRobinStrategy::new();
        let backends = vec!["a".into(), "b".into(), "c".into()];
        let metrics = MetricsCollector::new();

        let mut selections = Vec::new();
        for _ in 0..6 {
            let s = strategy.select(&backends, &metrics).await;
            selections.push(s);
        }

        assert_eq!(selections[0], Some("a".into()));
        assert_eq!(selections[1], Some("b".into()));
        assert_eq!(selections[2], Some("c".into()));
        assert_eq!(selections[3], Some("a".into()));
        assert_eq!(selections[4], Some("b".into()));
        assert_eq!(selections[5], Some("c".into()));
    }

    #[tokio::test]
    async fn test_cost_strategy() {
        let strategy = CostStrategy::new();
        let backends = vec!["expensive".into(), "cheap".into()];
        let mut metrics = MetricsCollector::new();
        metrics.record("expensive", Duration::from_millis(100), 0.50, true);
        metrics.record("cheap", Duration::from_millis(100), 0.01, true);

        let selected = strategy.select(&backends, &metrics).await;
        assert_eq!(selected, Some("cheap".into()));
    }
}
