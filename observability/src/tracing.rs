//! Tracing 初始化与配置。
//!
//! 根据环境自动选择日志格式：
//! - Dev:   pretty + 颜色 (默认 DEBUG)
//! - Test:  简洁格式 (默认 INFO)
//! - Prod:  JSON 格式 (默认 WARN)
//!
//! 支持 OpenTelemetry (feature `otel`)，自动导出 tracing 数据。
//!
//! ## Profile 选择
//!
//! | Profile | Sink | 用途 |
//! |---|---|---|
//! | `Server` | stdout/stderr | 后台服务模式 (默认) |
//! | `Cil` | `logs/cil.log` | 终端 TUI 模式，不污染终端 |
//! | `Test` | stdout (简洁) | 测试环境 |

use crate::ObservabilityConfig;
use lingshu_core::LsResult;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// 日志输出目标 Profile。
///
/// 决定 tracing sink 的类型和格式。
/// 每个 Profile 对应一种运行模式，确保日志输出不影响主界面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingProfile {
    /// 服务端模式 — stdout/stderr (默认)
    Server,
    /// 终端 TUI 模式 — 文件 sink，不污染终端
    Cil,
    /// 测试模式 — 简洁格式输出
    Test,
}

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

/// 初始化 CIL 模式的日志系统。
///
/// 将日志写入 `logs/cil.log` 文件，避免污染 TUI 终端屏幕。
/// 该函数是幂等的：多次调用只有第一次生效。
///
/// ## 文件日志特点
/// - 输出到 `logs/cil.log` (自动创建目录)
/// - 简洁格式，不含 ANSI 颜色
/// - 默认级别: INFO
/// - 不含 target 前缀 (减少噪音)
pub fn init_cil_logging() -> LsResult<()> {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();

    INIT.get_or_init(|| {
        let log_dir = std::path::PathBuf::from("logs");
        std::fs::create_dir_all(&log_dir).ok();

        let log_path = log_dir.join("cil.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("failed to open CIL log file");

        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(file)
            .with_target(false)
            .with_ansi(false)
            .boxed();

        let env_filter = EnvFilter::new("info");

        let registry = tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer);

        let _ = registry.try_init();

        tracing::info!("cil logging initialized, path: {}", log_path.display());
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

    /// 验证 init_cil_logging 不 panic
    #[test]
    fn test_init_cil_logging() {
        let _ = init_cil_logging();
    }
}
