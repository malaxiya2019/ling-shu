//! Metrics — Prometheus 指标系统。
//!
//! 提供统一的计数器、直方图和仪表盘注册表，
//! 支持通过 HTTP 端点 `/metrics` 暴露。
//!
//! ## Feature
//! - `metrics` (默认启用) — 启用 Prometheus 指标
//!
//! ## 默认指标
//! - `ls_llm_invocations_total` — LLM 调用次数 (labels: provider, model)
//! - `ls_llm_tokens_total` — Token 用量 (labels: type: prompt/completion)
//! - `ls_memory_ops_total` — 记忆操作次数 (labels: operation: read/write/delete)
//! - `ls_events_published_total` — 事件发布数 (labels: topic)
//! - `ls_sessions_active` — 当前活跃会话数 (gauge)
//! - `ls_tasks_submitted_total` — 任务提交数
//! - `ls_tasks_completed_total` — 任务完成数
//! - `ls_tasks_failed_total` — 任务失败数
//! - `ls_lifecycle_transitions_total` — 生命周期状态转换数
//! - `ls_http_requests_duration_seconds` — HTTP 请求耗时 (histogram)

use once_cell::sync::Lazy;
use std::sync::Arc;

/// 全局 Prometheus 注册表.
#[cfg(feature = "metrics")]
pub static REGISTRY: Lazy<prometheus::Registry> = Lazy::new(prometheus::Registry::new);

/// 指标注册表封装.
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    #[cfg(feature = "metrics")]
    inner: Arc<prometheus::Registry>,
}

impl MetricsRegistry {
    /// 创建新的指标注册表.
    pub fn new() -> Self {
        #[cfg(feature = "metrics")]
        {
            Self {
                inner: Arc::new(prometheus::Registry::new()),
            }
        }
        #[cfg(not(feature = "metrics"))]
        {
            Self {}
        }
    }

    /// 创建全局默认注册表.
    pub fn global() -> Self {
        #[cfg(feature = "metrics")]
        {
            Self {
                inner: Arc::new(REGISTRY.clone()),
            }
        }
        #[cfg(not(feature = "metrics"))]
        {
            Self {}
        }
    }

    /// 注册并获取计数器.
    #[cfg(feature = "metrics")]
    pub fn counter(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> prometheus::Result<prometheus::IntCounterVec> {
        let opts = prometheus::Opts::new(name, help);
        let counter = prometheus::IntCounterVec::new(opts, labels)?;
        self.inner.register(Box::new(counter.clone()))?;
        Ok(counter)
    }

    /// 注册并获取仪表盘.
    #[cfg(feature = "metrics")]
    pub fn gauge(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> prometheus::Result<prometheus::IntGaugeVec> {
        let opts = prometheus::Opts::new(name, help);
        let gauge = prometheus::IntGaugeVec::new(opts, labels)?;
        self.inner.register(Box::new(gauge.clone()))?;
        Ok(gauge)
    }

    /// 注册并获取直方图.
    #[cfg(feature = "metrics")]
    pub fn histogram(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
        buckets: Option<Vec<f64>>,
    ) -> prometheus::Result<prometheus::HistogramVec> {
        let mut opts = prometheus::HistogramOpts::new(name, help);
        if let Some(b) = buckets {
            opts = opts.buckets(b);
        }
        let histogram = prometheus::HistogramVec::new(opts, labels)?;
        self.inner.register(Box::new(histogram.clone()))?;
        Ok(histogram)
    }

    /// 采集所有指标为 Prometheus 文本格式.
    #[cfg(feature = "metrics")]
    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.inner.gather()
    }

    /// 采集所有指标为 Prometheus 文本格式 (no-op 版本).
    #[cfg(not(feature = "metrics"))]
    pub fn gather(&self) -> Vec<u8> {
        Vec::new()
    }

    /// 采集指标为 Prometheus 文本字符串.
    #[cfg(feature = "metrics")]
    pub fn gather_text(&self) -> String {
        use prometheus::TextEncoder;
        let encoder = TextEncoder::new();
        let metric_families = self.gather();
        let mut buffer = String::new();
        encoder.encode_utf8(&metric_families, &mut buffer).unwrap();
        buffer
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════
// 默认指标定义
// ═══════════════════════════════════════════════════

/// 默认指标名称常量.
pub mod names {
    /// LLM 调用次数.
    pub const LLM_INVOCATIONS: &str = "ls_llm_invocations_total";
    /// Token 用量.
    pub const LLM_TOKENS: &str = "ls_llm_tokens_total";
    /// 记忆操作次数.
    pub const MEMORY_OPS: &str = "ls_memory_ops_total";
    /// 事件发布数.
    pub const EVENTS_PUBLISHED: &str = "ls_events_published_total";
    /// 活跃会话数.
    pub const SESSIONS_ACTIVE: &str = "ls_sessions_active";
    /// 任务提交数.
    pub const TASKS_SUBMITTED: &str = "ls_tasks_submitted_total";
    /// 任务完成数.
    pub const TASKS_COMPLETED: &str = "ls_tasks_completed_total";
    /// 任务失败数.
    pub const TASKS_FAILED: &str = "ls_tasks_failed_total";
    /// 生命周期转换数.
    pub const LIFECYCLE_TRANSITIONS: &str = "ls_lifecycle_transitions_total";
    /// HTTP 请求耗时.
    pub const HTTP_DURATION: &str = "ls_http_requests_duration_seconds";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "metrics")]
    fn test_counter_registration() {
        let registry = MetricsRegistry::new();
        let counter = registry.counter("test_counter", "test help", &["label1"]);
        assert!(counter.is_ok());
        let counter = counter.unwrap();
        counter.with_label_values(&["val1"]).inc();
        assert_eq!(counter.with_label_values(&["val1"]).get(), 1);
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_gather_text() {
        let registry = MetricsRegistry::new();
        let counter = registry
            .counter("test_gather_count", "test help desc", &["label"])
            .unwrap();
        counter.with_label_values(&["val1"]).inc();
        // 验证 counter 工作正常
        assert_eq!(counter.with_label_values(&["val1"]).get(), 1);
        // 验证 gather 不 panic
        let gathered = registry.gather();
        assert!(!gathered.is_empty());
    }

    #[test]
    #[cfg(feature = "metrics")]
    fn test_global_registry() {
        let r1 = MetricsRegistry::global();
        let r2 = MetricsRegistry::global();
        let c = r1.counter("global_test", "help", &["x"]).unwrap();
        c.with_label_values(&["y"]).inc_by(5);
        // r2 共享同一个注册表
        let gathered = r2.gather();
        assert!(gathered.iter().any(|mf| mf.get_name() == "global_test"));
    }
}
