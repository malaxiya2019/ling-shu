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
//! ## 环境变量
//! - `LS_OTEL_ENDPOINT` — OTLP gRPC 端点 (默认: `http://localhost:4317`)
//! - `LS_OTEL_SERVICE_NAME` — 服务名 (默认: `lingshu`)
//! - `LS_OTEL_ENABLED` — 设为 `true` 启用

use lingshu_core::LsResult;
use std::sync::OnceLock;
use tracing::{info, warn};

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
            opentelemetry_sdk::runtime::TokioCurrentThread::shutdown_all();
            // Force a short sleep to let pending exports finish
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

        // ── Tracer Provider ──
        let tracer_provider = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint)
                    .with_timeout(std::time::Duration::from_secs(10)),
            )
            .with_trace_config(
                opentelemetry_sdk::trace::Config::default()
                    .with_sampler(opentelemetry_sdk::trace::Sampler::ParentBased(
                        Box::new(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(0.1)),
                    ))
                    .with_resource(opentelemetry_sdk::Resource::new(vec![
                        opentelemetry::KeyValue::new("service.name", 
                            std::env::var("LS_OTEL_SERVICE_NAME").unwrap_or_else(|_| "lingshu".into())),
                        opentelemetry::KeyValue::new("service.version",
                            std::env::var("LS_SERVICE_VERSION").unwrap_or_else(|_| "1.0.0".into())),
                    ])),
            )
            .install_batch(opentelemetry_sdk::runtime::TokioCurrentThread)
            .map_err(|e| lingshu_core::LsError::Internal(format!("OTel tracer init: {e}")))?;

        // ── Meter Provider ──
        let meter_provider = opentelemetry_otlp::new_pipeline()
            .metrics(opentelemetry_sdk::runtime::TokioCurrentThread)
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint)
                    .with_timeout(std::time::Duration::from_secs(10)),
            )
            .with_resource(opentelemetry_sdk::Resource::new(vec![
                opentelemetry::KeyValue::new("service.name",
                    std::env::var("LS_OTEL_SERVICE_NAME").unwrap_or_else(|_| "lingshu".into())),
            ]))
            .build()
            .map_err(|e| lingshu_core::LsError::Internal(format!("OTel meter init: {e}")))?;

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
        // Should return an error or handle gracefully when no OTLP server is available
        let result = init_otel("http://127.0.0.1:14317").await;
        // May succeed with config but fail later on export — that's fine
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
