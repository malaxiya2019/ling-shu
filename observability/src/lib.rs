//! LSObservability — Lingshu 可观测性体系。
//!
//! 提供全链路追踪（tracing + OpenTelemetry）、指标采集（Prometheus）
//! 与健康检查的统一入口。

pub mod health;
pub mod metrics;
pub mod span;
pub mod tracing;
#[cfg(feature = "otel")]
pub mod otel;
#[cfg(feature = "loki")]
pub mod loki;

pub use health::*;
pub use metrics::*;
pub use span::*;
pub use tracing::*;

use lingshu_config::env::Environment;

/// 可观测性全局配置.
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// 服务名称，用于 tracing/metrics 标记.
    pub service_name: String,
    /// 服务版本号.
    pub service_version: String,
    /// 运行环境.
    pub environment: Environment,
    /// 是否启用 JSON 日志输出.
    pub json_output: bool,
    /// 日志级别 (tracing 的过滤级别).
    pub log_level: String,
    /// 是否启用 OpenTelemetry (需要 feature `otel`).
    pub enable_otel: bool,
    /// OpenTelemetry 端点 (如 `http://localhost:4317`).
    pub otel_endpoint: Option<String>,
    /// 是否启用 Prometheus 指标 (需要 feature `metrics`).
    pub enable_metrics: bool,
    /// Prometheus 指标 HTTP 监听地址.
    pub metrics_addr: String,
    /// 是否启用健康检查 HTTP 端点.
    pub enable_health: bool,
    /// 健康检查 HTTP 监听地址.
    pub health_addr: String,
}

impl ObservabilityConfig {
    /// 从环境变量加载可观测性配置.
    pub fn from_env() -> Self {
        let env = lingshu_config::env::current_environment();
        Self {
            service_name: std::env::var("LS_SERVICE_NAME")
                .unwrap_or_else(|_| "lingshu".to_string()),
            service_version: std::env::var("LS_SERVICE_VERSION")
                .unwrap_or_else(|_| "1.0.0".to_string()),
            environment: env,
            json_output: std::env::var("LS_LOG_FORMAT").as_deref() == Ok("json"),
            log_level: std::env::var("LS_LOG_LEVEL")
                .unwrap_or_else(|_| env.log_level().to_string()),
            enable_otel: std::env::var("LS_OTEL_ENABLED").as_deref() == Ok("true"),
            otel_endpoint: std::env::var("LS_OTEL_ENDPOINT").ok(),
            enable_metrics: std::env::var("LS_METRICS_ENABLED").as_deref() != Ok("false"),
            metrics_addr: std::env::var("LS_METRICS_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:9090".to_string()),
            enable_health: std::env::var("LS_HEALTH_ENABLED").as_deref() != Ok("false"),
            health_addr: std::env::var("LS_HEALTH_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        }
    }

    /// 使用指定值构建配置.
    pub fn new(service_name: &str, service_version: &str, environment: Environment) -> Self {
        Self {
            service_name: service_name.to_string(),
            service_version: service_version.to_string(),
            environment,
            json_output: false,
            log_level: environment.log_level().to_string(),
            enable_otel: false,
            otel_endpoint: None,
            enable_metrics: true,
            metrics_addr: "0.0.0.0:9090".to_string(),
            enable_health: true,
            health_addr: "0.0.0.0:8080".to_string(),
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self::from_env()
    }
}
