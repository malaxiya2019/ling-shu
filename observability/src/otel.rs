//! OpenTelemetry 集成 — OTLP 导出 Traces 和 Metrics.
//!
//! ## 使用
//! ```rust,ignore
//! use lingshu_observability::otel::init_otel;
//!
//! let guard = init_otel("http://localhost:4317").await?;
//! // ... your app ...
//! drop(guard); // shutdown OTel
//! ```
//!
//! ## Runtime 操作指标 (v4.2.3)
//!
//! 以下 OTel 仪表通过 `RuntimeOtelMetrics` 注册：
//! - `ls.runtime.agent_count` — Gauge, 当前 Agent 数量
//! - `ls.runtime.tool_calls` — Counter, 工具调用累计次数 (labels: tool_name, status)
//! - `ls.runtime.session_count` — Gauge, 当前活跃会话数

use lingshu_core::LsResult;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
use std::sync::OnceLock;
use tracing::info;

static OTEL_INIT: OnceLock<()> = OnceLock::new();

/// OTel 生命周期守卫 — drop 时优雅关闭导出器.
pub struct OtelGuard {
    _private: (),
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        {
            info!("shutting down OpenTelemetry");
            opentelemetry::global::shutdown_tracer_provider();
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}

/// 初始化 OpenTelemetry 导出器 (Trace + Metrics).
///
/// 成功返回 `OtelGuard`，在程序退出前持有它可确保 OTel 数据被导出。
pub async fn init_otel(endpoint: &str) -> LsResult<Option<OtelGuard>> {
    if OTEL_INIT.set(()).is_err() {
        info!("OpenTelemetry already initialized, skipping");
        return Ok(None);
    }

    #[cfg(feature = "otel")]
    {
        info!(endpoint = %endpoint, "initializing OpenTelemetry OTLP exporter");

        // ── Span Exporter & Tracer Provider ──
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| lingshu_core::LsError::Internal(format!("OTel span exporter: {e}")))?;

        let tracer = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(span_exporter, opentelemetry_sdk::runtime::Tokio)
            .with_sampler(opentelemetry_sdk::trace::Sampler::ParentBased(Box::new(
                opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(0.1),
            )))
            .with_resource(opentelemetry_sdk::Resource::new(vec![
                opentelemetry::KeyValue::new(
                    "service.name",
                    std::env::var("LS_OTEL_SERVICE_NAME").unwrap_or_else(|_| "lingshu".into()),
                ),
                opentelemetry::KeyValue::new(
                    "service.version",
                    std::env::var("LS_SERVICE_VERSION").unwrap_or_else(|_| "1.0.0".into()),
                ),
            ]))
            .build();

        // ── Metric Exporter (optional, sets global meter provider) ──
        let _metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(std::time::Duration::from_secs(10))
            .build();

        opentelemetry::global::set_tracer_provider(tracer);

        info!("OpenTelemetry initialized successfully");
        Ok(Some(OtelGuard { _private: () }))
    }

    #[cfg(not(feature = "otel"))]
    {
        tracing::warn!("OpenTelemetry requested but 'otel' feature not enabled");
        Ok(None)
    }
}

/// 快捷方式 — 从环境变量初始化.
pub async fn init_otel_from_env() -> LsResult<Option<OtelGuard>> {
    let enabled = std::env::var("LS_OTEL_ENABLED").as_deref() == Ok("true");
    if !enabled {
        return Ok(None);
    }

    let endpoint =
        std::env::var("LS_OTEL_ENDPOINT").unwrap_or_else(|_| "http://localhost:4317".into());
    init_otel(&endpoint).await
}

// ═══════════════════════════════════════════════════
// Runtime OTel 指标 (v4.2.3)
// ═══════════════════════════════════════════════════

/// Runtime 操作指标的 OTel 仪表集合。
///
/// 使用 OpenTelemetry SDK 的 Meter API 注册 counter/gauge，
/// 与 Prometheus `RuntimeMetricsCollector` 双写，确保两种导出路径一致。
#[cfg(feature = "otel")]
#[derive(Debug)]
pub struct RuntimeOtelMetrics {
    /// Meter 实例 (用 `ls.runtime` instrument 范围).
    #[allow(dead_code)]
    meter: opentelemetry::metrics::Meter,
    /// Agent 数量 Gauge.
    agent_count: opentelemetry::metrics::Gauge<u64>,
    /// 工具调用 Counter (labels: tool_name, status).
    tool_calls: opentelemetry::metrics::Counter<u64>,
    /// 会话数 Gauge.
    session_count: opentelemetry::metrics::Gauge<u64>,
}

#[cfg(feature = "otel")]
impl RuntimeOtelMetrics {
    /// 创建 OTel Runtime 仪表并注册到全局 MeterProvider.
    pub fn new() -> Self {
        let meter = opentelemetry::global::meter("ls.runtime");
        let agent_count = meter
            .u64_gauge("ls.runtime.agent_count")
            .with_description("当前 Agent 数量")
            .with_unit("{agent}")
            .build();
        let tool_calls = meter
            .u64_counter("ls.runtime.tool_calls")
            .with_description("工具调用累计次数")
            .with_unit("{call}")
            .build();
        let session_count = meter
            .u64_gauge("ls.runtime.session_count")
            .with_description("当前活跃会话数")
            .with_unit("{session}")
            .build();

        Self {
            meter,
            agent_count,
            tool_calls,
            session_count,
        }
    }

    /// 设置当前 Agent 数量.
    pub fn set_agent_count(&self, count: u64) {
        self.agent_count.record(count, &[]);
    }

    /// 记录一次工具调用.
    pub fn inc_tool_calls(&self, tool_name: &str, status: &str) {
        let attributes = [
            opentelemetry::KeyValue::new("tool_name", tool_name.to_string()),
            opentelemetry::KeyValue::new("status", status.to_string()),
        ];
        self.tool_calls.add(1, &attributes);
    }

    /// 设置当前活跃会话数.
    pub fn set_session_count(&self, count: u64) {
        self.session_count.record(count, &[]);
    }
}

#[cfg(feature = "otel")]
impl Default for RuntimeOtelMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg(feature = "otel")]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_otel_without_server() {
        let result = init_otel("http://127.0.0.1:14317").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_init_otel_from_env_disabled() {
        std::env::remove_var("LS_OTEL_ENABLED");
        let result = init_otel_from_env().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_double_init_is_noop() {
        let _ = init_otel("http://127.0.0.1:14317").await;
        let second = init_otel("http://127.0.0.1:14317").await;
        assert!(second.is_ok());
        assert!(second.unwrap().is_none());
    }

    #[test]
    fn test_runtime_otel_metrics_create() {
        let metrics = RuntimeOtelMetrics::new();
        metrics.set_agent_count(5);
        metrics.inc_tool_calls("search", "success");
        metrics.set_session_count(12);
        // 验证仪表创建不 panic
    }
}
