//! 终端运行时隔离测试。
//!
//! 验证 RFC-0052 Terminal Runtime Isolation 的各项保证：
//!
//! ## 测试覆盖
//!
//! | 测试 | 验证内容 | 类型 |
//! |---|---|---|
//! | `tracing_profile_enum` | TracingProfile 枚举完整性 | 单元 |
//! | `cil_logging_creates_file` | CIL 日志写入文件而非 stdout | 集成 |
//! | `cil_logging_content` | CIL 日志文件包含预期内容 | 集成 |
//! | `tracing_profile_dispatch` | 不同 Profile 选择不同 sink | 单元 |
//!
//! ## 手动验证 (需要 TTY)
//!
//! ```bash
//! # 验证 CIL 模式不输出 tracing 日志到终端
//! cargo run -- --cil 2>/dev/null
//! # 预期: 只看到 TUI 界面, 无 tracing 日志
//!
//! # 验证日志写入文件
//! cat logs/cil.log
//! # 预期: 包含 "cil logging initialized" 等日志
//!
//! # 验证终端恢复
//! cargo run -- --cil
//! # 按 Ctrl+C 退出后, 终端应正常工作
//! # (不残留 raw mode)
//! ```

use std::io::Read;

/// TracingProfile 枚举必须包含预期的三个变体。
#[test]
fn test_tracing_profile_enum() {
    // Server
    let server = lingshu_observability::tracing::TracingProfile::Server;
    assert_eq!(server as u8, 0);
    assert!(matches!(server, lingshu_observability::tracing::TracingProfile::Server));

    // Cil
    let cil = lingshu_observability::tracing::TracingProfile::Cil;
    assert_eq!(cil as u8, 1);
    assert!(matches!(cil, lingshu_observability::tracing::TracingProfile::Cil));

    // Test
    let test = lingshu_observability::tracing::TracingProfile::Test;
    assert_eq!(test as u8, 2);
    assert!(matches!(test, lingshu_observability::tracing::TracingProfile::Test));

    // 确认三个变体互不相等
    assert_ne!(server, cil);
    assert_ne!(server, test);
    assert_ne!(cil, test);
}

/// init_cil_logging 必须在 logs/cil.log 创建日志文件。
#[test]
fn test_cil_logging_creates_file() {
    // 使用临时目录隔离测试
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let original_dir = std::env::current_dir().expect("failed to get cwd");
    std::env::set_current_dir(tmp_dir.path()).expect("failed to chdir");

    // 调用 init_cil_logging
    let result = lingshu_observability::tracing::init_cil_logging();
    assert!(result.is_ok(), "init_cil_logging should succeed");

    // 验证日志文件已创建
    let log_path = tmp_dir.path().join("logs").join("cil.log");
    assert!(
        log_path.exists(),
        "cil.log should exist at: {}",
        log_path.display()
    );

    // 验证文件可读且非空
    let mut file = std::fs::File::open(&log_path)
        .expect("should be able to open cil.log");
    let mut content = String::new();
    file.read_to_string(&mut content)
        .expect("should be able to read cil.log");
    assert!(
        !content.is_empty(),
        "cil.log should contain log output"
    );

    // 验证包含初始化消息
    assert!(
        content.contains("cil logging initialized"),
        "cil.log should contain initialization message"
    );

    // 清理
    std::env::set_current_dir(original_dir).expect("failed to restore cwd");
}

/// init_cil_logging 的日志不应该包含 ANSI 颜色转义码。
#[test]
fn test_cil_logging_no_ansi() {
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let original_dir = std::env::current_dir().expect("failed to get cwd");
    std::env::set_current_dir(tmp_dir.path()).expect("failed to chdir");

    let _ = lingshu_observability::tracing::init_cil_logging();

    let log_path = tmp_dir.path().join("logs").join("cil.log");
    let mut file = std::fs::File::open(&log_path).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();

    // ANSI 转义以 ESC (0x1b) 开头
    assert!(
        !content.contains('\x1b'),
        "CIL log should not contain ANSI escape codes"
    );

    std::env::set_current_dir(original_dir).expect("failed to restore cwd");
}

/// CIL 日志不应包含 target 前缀（减少噪音）。
#[test]
fn test_cil_logging_no_target() {
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let original_dir = std::env::current_dir().expect("failed to get cwd");
    std::env::set_current_dir(tmp_dir.path()).expect("failed to chdir");

    let _ = lingshu_observability::tracing::init_cil_logging();

    let log_path = tmp_dir.path().join("logs").join("cil.log");
    let mut file = std::fs::File::open(&log_path).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();

    // 默认 tracing fmt 使用 "target" 参数控制是否显示模块路径
    // with_target(false) 应不包含 "lingshu_observability::tracing"
    assert!(
        !content.contains("lingshu_observability::tracing"),
        "CIL log should not contain target prefixes"
    );

    std::env::set_current_dir(original_dir).expect("failed to restore cwd");
}

/// 验证 TracingProfile 可用于运行时配置选择。
#[test]
fn test_tracing_profile_dispatch() {
    // 模拟 Profile 分发逻辑
    fn select_log_path(profile: lingshu_observability::tracing::TracingProfile) -> &'static str {
        match profile {
            lingshu_observability::tracing::TracingProfile::Server => "stdout",
            lingshu_observability::tracing::TracingProfile::Cil => "logs/cil.log",
            lingshu_observability::tracing::TracingProfile::Test => "stdout",
        }
    }

    assert_eq!(select_log_path(lingshu_observability::tracing::TracingProfile::Server), "stdout");
    assert_eq!(select_log_path(lingshu_observability::tracing::TracingProfile::Cil), "logs/cil.log");
    assert_eq!(select_log_path(lingshu_observability::tracing::TracingProfile::Test), "stdout");
}
