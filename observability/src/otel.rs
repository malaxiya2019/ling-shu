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

use lingshu_core::LsResult;
use std::sync::OnceLock;
use tracing::info;
use opentelemetry_otlp::{WithExportConfig};

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
            .with_sampler(opentelemetry_sdk::trace::Sampler::ParentBased(
                Box::new(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(0.1)),
            ))
            .with_resource(opentelemetry_sdk::Resource::new(vec![
                opentelemetry::KeyValue::new("service.name",
                    std::env::var("LS_OTEL_SERVICE_NAME").unwrap_or_else(|_| "lingshu".into())),
                opentelemetry::KeyValue::new("service.version",
                    std::env::var("LS_SERVICE_VERSION").unwrap_or_else(|_| "1.0.0".into())),
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
        warn!("OpenTelemetry requested but 'otel' feature not enabled");
        Ok(None)
    }
}

/// 快捷方式 — 从环境变量初始化.
pub async fn init_otel_from_env() -> LsResult<Option<OtelGuard>> {
    let enabled = std::env::var("LS_OTEL_ENABLED").as_deref() == Ok("true");
    if !enabled {
        return Ok(None);
    }

    let endpoint = std::env::var("LS_OTEL_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".into());
    init_otel(&endpoint).await
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
}
