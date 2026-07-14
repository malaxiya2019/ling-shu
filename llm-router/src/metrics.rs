//! 后端性能指标收集器.
//!
//! 追踪每个后端的延迟、错误率、调用次数等，供路由策略决策使用。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// 后端运行时指标.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendMetrics {
    /// 后端名称
    pub name: String,
    /// 总调用次数
    pub total_calls: u64,
    /// 成功调用次数
    pub success_calls: u64,
    /// 失败调用次数
    pub error_calls: u64,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: f64,
    /// P50 延迟（毫秒）
    pub p50_latency_ms: f64,
    /// P95 延迟（毫秒）
    pub p95_latency_ms: f64,
    /// P99 延迟（毫秒）
    pub p99_latency_ms: f64,
    /// 总成本（美元）
    pub total_cost: f64,
    /// 最近错误信息
    pub last_error: Option<String>,
    /// 最后调用时间戳
    pub last_call_at: Option<i64>,
}

#[allow(dead_code)]
impl BackendMetrics {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            total_calls: 0,
            success_calls: 0,
            error_calls: 0,
            avg_latency_ms: 0.0,
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            total_cost: 0.0,
            last_error: None,
            last_call_at: None,
        }
    }

    /// 错误率.
    pub fn error_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.error_calls as f64 / self.total_calls as f64
        }
    }

    /// 成功率.
    pub fn success_rate(&self) -> f64 {
        if self.total_calls == 0 {
            1.0
        } else {
            self.success_calls as f64 / self.total_calls as f64
        }
    }
}

/// 指标收集器.
#[derive(Debug)]
pub struct MetricsCollector {
    backends: HashMap<String, Vec<Duration>>,
    errors: HashMap<String, u64>,
    successes: HashMap<String, u64>,
    costs: HashMap<String, f64>,
    last_errors: HashMap<String, Option<String>>,
    last_calls: HashMap<String, Option<i64>>,
}

#[allow(dead_code)]
impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            errors: HashMap::new(),
            successes: HashMap::new(),
            costs: HashMap::new(),
            last_errors: HashMap::new(),
            last_calls: HashMap::new(),
        }
    }

    /// 记录一次调用.
    pub fn record(&mut self, backend: &str, latency: Duration, cost: f64, success: bool) {
        self.backends
            .entry(backend.to_string())
            .or_default()
            .push(latency);

        if success {
            *self.successes.entry(backend.to_string()).or_insert(0) += 1;
        } else {
            *self.errors.entry(backend.to_string()).or_insert(0) += 1;
        }

        *self.costs.entry(backend.to_string()).or_insert(0.0) += cost;
        self.last_calls
            .entry(backend.to_string())
            .and_modify(|v| {
                let now = chrono::Utc::now().timestamp();
                *v = Some(now);
            })
            .or_insert(Some(chrono::Utc::now().timestamp()));
    }

    /// 记录错误.
    pub fn record_error(&mut self, backend: &str, error: String) {
        *self.errors.entry(backend.to_string()).or_insert(0) += 1;
        self.last_errors
            .entry(backend.to_string())
            .and_modify(|v| *v = Some(error.clone()))
            .or_insert(Some(error));
    }

    /// 获取后端指标快照.
    pub fn get(&self, backend: &str) -> Option<BackendMetrics> {
        let latencies = self.backends.get(backend)?;
        let total = latencies.len() as u64;
        let success = *self.successes.get(backend).unwrap_or(&0);
        let error = *self.errors.get(backend).unwrap_or(&0);

        let mut sorted = latencies.clone();
        sorted.sort();

        let avg = if total > 0 {
            sorted.iter().map(|d| d.as_secs_f64()).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let p50 = percentile(&sorted, 0.50);
        let p95 = percentile(&sorted, 0.95);
        let p99 = percentile(&sorted, 0.99);

        Some(BackendMetrics {
            name: backend.to_string(),
            total_calls: total,
            success_calls: success,
            error_calls: error,
            avg_latency_ms: avg * 1000.0,
            p50_latency_ms: p50 * 1000.0,
            p95_latency_ms: p95 * 1000.0,
            p99_latency_ms: p99 * 1000.0,
            total_cost: *self.costs.get(backend).unwrap_or(&0.0),
            last_error: self.last_errors.get(backend).and_then(|e| e.clone()),
            last_call_at: *self.last_calls.get(backend).unwrap_or(&None),
        })
    }

    /// 获取所有后端指标.
    pub fn all(&self) -> HashMap<String, BackendMetrics> {
        self.backends
            .keys()
            .filter_map(|name| self.get(name).map(|m| (name.clone(), m)))
            .collect()
    }

    /// 清除所有数据.
    pub fn clear(&mut self) {
        self.backends.clear();
        self.errors.clear();
        self.successes.clear();
        self.costs.clear();
        self.last_errors.clear();
        self.last_calls.clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn percentile(data: &[Duration], p: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let idx = ((data.len() as f64) * p).ceil() as usize;
    let idx = idx.max(1).min(data.len()) - 1;
    data[idx].as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_record() {
        let mut collector = MetricsCollector::new();
        collector.record("openai", Duration::from_millis(100), 0.01, true);
        collector.record("openai", Duration::from_millis(200), 0.01, true);
        collector.record("openai", Duration::from_millis(300), 0.01, false);

        let metrics = collector.get("openai").unwrap();
        assert_eq!(metrics.total_calls, 3);
        assert_eq!(metrics.success_calls, 2);
        assert_eq!(metrics.error_calls, 1);
        assert!(metrics.avg_latency_ms > 100.0);
        assert!(metrics.error_rate() > 0.3);
    }

    #[test]
    fn test_empty_metrics() {
        let collector = MetricsCollector::new();
        assert!(collector.get("nonexistent").is_none());
    }

    #[test]
    fn test_clear() {
        let mut collector = MetricsCollector::new();
        collector.record("test", Duration::from_millis(50), 0.0, true);
        assert!(collector.get("test").is_some());
        collector.clear();
        assert!(collector.get("test").is_none());
    }

    #[test]
    fn test_error_rate_zero_division() {
        let _collector = MetricsCollector::new();
        // BackendMetrics::new creates a fresh struct
        let metrics = BackendMetrics::new("test");
        assert_eq!(metrics.error_rate(), 0.0);
        assert_eq!(metrics.success_rate(), 1.0);
    }

    #[test]
    fn test_percentile() {
        let data = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
            Duration::from_millis(50),
        ];
        assert!((percentile(&data, 0.50) - 0.030).abs() < 0.001);
        assert!((percentile(&data, 0.95) - 0.050).abs() < 0.001);
        assert!((percentile(&data, 1.0) - 0.050).abs() < 0.001);
    }

    #[test]
    fn test_empty_percentile() {
        assert_eq!(percentile(&[], 0.5), 0.0);
    }
}
