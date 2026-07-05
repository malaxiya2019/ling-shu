//! Tracing 初始化与配置。
//!
//! 根据环境自动选择日志格式：
//! - Dev:   pretty + 颜色 (默认 DEBUG)
//! - Test:  简洁格式 (默认 INFO)
//! - Prod:  JSON 格式 (默认 WARN)
//!
//! 支持 OpenTelemetry (feature `otel`)，自动导出 tracing 数据。

use crate::ObservabilityConfig;
use lingshu_core::LsResult;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// 初始化 tracing subscriber.
///
/// 根据 `config` 自动选择日志格式和级别。
/// 可以多次调用，但只有第一次生效（后续调用为 no-op）。
pub fn init_tracing(config: &ObservabilityConfig) -> LsResult<()> {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();

    INIT.get_or_init(|| {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

        let registry = tracing_subscriber::registry().with(env_filter);

        if config.json_output {
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_current_span(true)
                .with_span_list(true)
                .with_line_number(true)
                .with_file(true)
                .with_level(true)
                .boxed();
            let _ = registry.with(json_layer).try_init();
        } else {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .pretty()
                .with_target(true)
                .with_line_number(true)
                .with_file(true)
                .with_level(true)
                .boxed();
            let _ = registry.with(fmt_layer).try_init();
        }

        tracing::info!(
            service.name = %config.service_name,
            service.version = %config.service_version,
            environment = %config.environment,
            log_format = if config.json_output { "json" } else { "pretty" },
            "tracing initialized"
        );
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 init_tracing 不 panic（使用 try_init 避免冲突）
    #[test]
    fn test_init_tracing_dev() {
        let config = ObservabilityConfig {
            json_output: false,
            log_level: "debug".into(),
            ..ObservabilityConfig::default()
        };
        let _ = init_tracing(&config);
    }

    #[test]
    fn test_init_tracing_json() {
        let config = ObservabilityConfig {
            json_output: true,
            log_level: "warn".into(),
            ..ObservabilityConfig::default()
        };
        let _ = init_tracing(&config);
    }
}
