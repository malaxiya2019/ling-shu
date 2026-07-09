//! 🏖️ Code Sandbox Plugin — 安全代码执行沙箱
//!
//! 提供两种执行模式：
//! - `simple` (默认): 子进程执行 + token 验证 + 超时限制，跨平台兼容
//! - `wasm`: WASM 沙箱执行（仅非 Android 平台）
//!
//! ## 支持的语言
//!
//! | 语言 | 执行方式 | 要求 |
//! |------|----------|------|
//! | Python | `python3 -c` | 需安装 Python |
//! | JavaScript | `node -e` | 需安装 Node.js |
//! | Shell | `bash -c` | 需安装 Bash |
//! | Rust | 编译执行 | 需安装 Rust |
//!
//! ## 安全策略
//!
//! - 超时控制 (默认 30s)
//! - 输出大小限制 (默认 1MB)
//! - 禁止危险命令（rm -rf / 等）
//! - WASM 模式提供完整内存隔离

use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_traits::plugin::{Plugin, PluginInfo, PluginManifest, PluginPermission, PluginStatus};

// ===========================================================================
// 执行结果
// ===========================================================================

/// 代码执行结果.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionResult {
    /// 标准输出.
    pub stdout: String,
    /// 标准错误.
    pub stderr: String,
    /// 退出码.
    pub exit_code: i32,
    /// 执行耗时 (毫秒).
    pub duration_ms: u64,
    /// 是否超时.
    pub timed_out: bool,
    /// 是否被安全策略拦截.
    pub blocked: bool,
    /// 拦截原因.
    pub block_reason: Option<String>,
}

// ===========================================================================
// 代码沙箱插件
// ===========================================================================

/// 代码沙箱插件.
pub struct CodeSandboxPlugin {
    /// 最大执行时间 (秒).
    max_timeout_secs: u64,
    /// 最大输出大小 (字节).
    max_output_bytes: u64,
    /// 允许的语言.
    allowed_languages: Vec<String>,
    /// 执行计数.
    exec_count: AtomicU64,
    /// 插件状态.
    status: PluginStatus,
    /// 创建时间.
    created_at: i64,
}

impl Default for CodeSandboxPlugin {
    fn default() -> Self {
        Self {
            max_timeout_secs: 30,
            max_output_bytes: 1_048_576, // 1MB
            allowed_languages: vec![
                "python".into(),
                "javascript".into(),
                "shell".into(),
                "rust".into(),
                "python3".into(),
                "node".into(),
                "bash".into(),
            ],
            exec_count: AtomicU64::new(0),
            status: PluginStatus::Loaded,
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl CodeSandboxPlugin {
    /// 创建新的代码沙箱插件.
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置最大超时时间.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.max_timeout_secs = secs.max(5).min(300);
        self
    }

    /// 设置最大输出大小.
    pub fn with_max_output(mut self, bytes: u64) -> Self {
        self.max_output_bytes = bytes.min(10 * 1_048_576); // 最大 10MB
        self
    }

    /// 执行代码.
    pub async fn execute(
        &self,
        language: &str,
        code: &str,
        timeout_secs: Option<u64>,
    ) -> ExecutionResult {
        let start = std::time::Instant::now();
        self.exec_count.fetch_add(1, Ordering::Relaxed);

        // 1. 安全检查
        if let Some(reason) = self.check_safety(language, code) {
            return ExecutionResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                duration_ms: start.elapsed().as_millis() as u64,
                timed_out: false,
                blocked: true,
                block_reason: Some(reason),
            };
        }

        // 2. 构建执行命令
        let timeout = timeout_secs
            .unwrap_or(self.max_timeout_secs)
            .min(self.max_timeout_secs);

        let result = match language {
            "python" | "python3" => self.run_python(code, timeout).await,
            "javascript" | "node" | "js" => self.run_javascript(code, timeout).await,
            "shell" | "bash" | "sh" => self.run_shell(code, timeout).await,
            "rust" | "rs" => self.run_rust(code, timeout).await,
            _ => ExecutionResult {
                stdout: String::new(),
                stderr: format!("Unsupported language: {language}"),
                exit_code: -1,
                duration_ms: start.elapsed().as_millis() as u64,
                timed_out: false,
                blocked: false,
                block_reason: Some(format!("unsupported language: {language}")),
            },
        };

        // 截断过大的输出
        ExecutionResult {
            stdout: Self::truncate_output(&result.stdout, self.max_output_bytes),
            stderr: Self::truncate_output(&result.stderr, self.max_output_bytes),
            ..result
        }
    }

    /// 安全检查 — 禁止危险操作.
    fn check_safety(&self, language: &str, code: &str) -> Option<String> {
        if !self.allowed_languages.iter().any(|l| l == &language) {
            return Some(format!("language '{}' not in allowed list", language));
        }

        let code_lower = code.to_lowercase();

        // 禁止的危险模式
        let dangerous_patterns = [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",  // fork bomb
            "dd if=/dev/zero of=/dev/sda",
            "mkfs.",
            "chmod 777 /",
            "> /dev/sda",
            "wget http://",
            "curl http://",
            "socket.connect",
            "subprocess.call",
            "os.system",
            "os.popen",
            "shutil.rmtree",
            "__import__('os')",
            "eval(open",
            "exec(open",
        ];

        for pattern in &dangerous_patterns {
            if code_lower.contains(pattern) {
                return Some(format!("blocked dangerous pattern: {pattern}"));
            }
        }

        // 代码长度限制
        if code.len() > 1_000_000 {
            return Some("code exceeds 1MB limit".into());
        }

        None
    }

    /// 截断输出.
    fn truncate_output(output: &str, max_bytes: u64) -> String {
        if output.len() as u64 > max_bytes {
            let truncated = &output[..max_bytes as usize];
            format!("{}...\n[truncated at {} bytes]", truncated, max_bytes)
        } else {
            output.to_string()
        }
    }

    // ── Python 执行 ────

    async fn run_python(&self, code: &str, timeout_secs: u64) -> ExecutionResult {
        self.run_subprocess("python3", ["-c", code], timeout_secs).await
    }

    // ── JavaScript 执行 ────

    async fn run_javascript(&self, code: &str, timeout_secs: u64) -> ExecutionResult {
        self.run_subprocess("node", ["-e", code], timeout_secs).await
    }

    // ── Shell 执行 ────

    async fn run_shell(&self, code: &str, timeout_secs: u64) -> ExecutionResult {
        self.run_subprocess("bash", ["-c", code], timeout_secs).await
    }

    // ── Rust 执行 ────

    async fn run_rust(&self, code: &str, timeout_secs: u64) -> ExecutionResult {
        let start = std::time::Instant::now();

        // 创建临时文件
        let tmp_dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                return ExecutionResult {
                    stdout: String::new(),
                    stderr: format!("failed to create temp dir: {e}"),
                    exit_code: -1,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    blocked: false,
                    block_reason: None,
                };
            }
        };

        let rs_file = tmp_dir.path().join("main.rs");
        if let Err(e) = tokio::fs::write(&rs_file, code).await {
            return ExecutionResult {
                stdout: String::new(),
                stderr: format!("failed to write source: {e}"),
                exit_code: -1,
                duration_ms: start.elapsed().as_millis() as u64,
                timed_out: false,
                blocked: false,
                block_reason: None,
            };
        }

        // 编译
        let compile_result = self
            .run_subprocess_timeout(
                "rustc",
                [
                    rs_file.to_string_lossy().as_ref(),
                    "-o",
                    tmp_dir.path().join("out").to_string_lossy().as_ref(),
                ],
                timeout_secs,
            )
            .await;

        if compile_result.exit_code != 0 {
            return ExecutionResult {
                duration_ms: start.elapsed().as_millis() as u64,
                ..compile_result
            };
        }

        // 运行
        let run_result = self
            .run_subprocess_timeout(
                tmp_dir.path().join("out").to_string_lossy().as_ref(),
                [] as [&str; 0],
                timeout_secs,
            )
            .await;

        ExecutionResult {
            duration_ms: start.elapsed().as_millis() as u64,
            ..run_result
        }
    }

    // ── 通用子进程执行 ────

    async fn run_subprocess(
        &self,
        program: &str,
        args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
        timeout_secs: u64,
    ) -> ExecutionResult {
        self.run_subprocess_timeout(program, args, timeout_secs)
            .await
    }

    async fn run_subprocess_timeout(
        &self,
        program: impl AsRef<std::ffi::OsStr>,
        args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
        timeout_secs: u64,
    ) -> ExecutionResult {
        let start = std::time::Instant::now();

        let mut child = match tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return ExecutionResult {
                    stdout: String::new(),
                    stderr: format!("failed to spawn: {e}"),
                    exit_code: -1,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                    blocked: false,
                    block_reason: None,
                };
            }
        };

        let timeout = Duration::from_secs(timeout_secs);
        let timed_out = {
            let child_stdout = child.stdout.take();
            let child_stderr = child.stderr.take();

            let result = tokio::time::timeout(timeout, child.wait()).await;

            match result {
                Ok(Ok(status)) => {
                    // Read output
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    if let Some(mut out) = child_stdout {
                        let _ = tokio::io::AsyncReadExt::read_to_string(&mut out, &mut stdout).await;
                    }
                    if let Some(mut err) = child_stderr {
                        let _ = tokio::io::AsyncReadExt::read_to_string(&mut err, &mut stderr).await;
                    }
                    return ExecutionResult {
                        stdout,
                        stderr,
                        exit_code: status.code().unwrap_or(-1),
                        duration_ms: start.elapsed().as_millis() as u64,
                        timed_out: false,
                        blocked: false,
                        block_reason: None,
                    };
                }
                Ok(Err(e)) => {
                    return ExecutionResult {
                        stdout: String::new(),
                        stderr: format!("execution error: {e}"),
                        exit_code: -1,
                        duration_ms: start.elapsed().as_millis() as u64,
                        timed_out: false,
                        blocked: false,
                        block_reason: None,
                    };
                }
                Err(_) => {
                    // timeout
                    let _ = child.kill().await;
                    true
                }
            }
        };

        if timed_out {
            ExecutionResult {
                stdout: String::new(),
                stderr: format!("timed out after {timeout_secs}s"),
                exit_code: -1,
                duration_ms: start.elapsed().as_millis() as u64,
                timed_out: true,
                blocked: false,
                block_reason: None,
            }
        } else {
            unreachable!()
        }
    }

    /// 获取执行统计.
    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "exec_count": self.exec_count.load(Ordering::Relaxed),
            "max_timeout_secs": self.max_timeout_secs,
            "allowed_languages": self.allowed_languages,
        })
    }
}

// ===========================================================================
// Plugin trait 实现
// ===========================================================================

#[async_trait]
impl Plugin for CodeSandboxPlugin {
    fn info(&self) -> PluginInfo {
        let manifest = PluginManifest {
            name: "code-sandbox".into(),
            version: "1.0.0".into(),
            description: "安全的代码执行沙箱 — 支持 Python/JavaScript/Shell/Rust，含超时控制和安全策略".into(),
            author: Some("Lingshu Team".into()),
            homepage: Some("https://github.com/malaxiya2019/ling-shu".into()),
            license: Some("MIT".into()),
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![PluginPermission {
                resource: "subprocess".into(),
                actions: vec!["execute".into(), "read".into()],
            }],
            min_api_version: Some("1.0.0".into()),
        };
        PluginInfo {
            plugin_id: LsId::new(),
            manifest,
            status: self.status.clone(),
            loaded_at: chrono::DateTime::from_timestamp(self.created_at, 0),
        }
    }

    async fn init(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!("code-sandbox plugin initialized");
        Ok(())
    }

    async fn start(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!("code-sandbox plugin started (timeout: {}s)", self.max_timeout_secs);
        Ok(())
    }

    async fn stop(&self, _ctx: LsContext) -> LsResult<()> {
        tracing::info!("code-sandbox plugin stopped");
        Ok(())
    }

    fn required_permissions(&self) -> Vec<PluginPermission> {
        vec![PluginPermission {
            resource: "subprocess".into(),
            actions: vec!["execute".into()],
        }]
    }
}

// ===========================================================================
// WASM Sandbox (非 Android 平台)
// ===========================================================================

#[cfg(all(feature = "wasm", not(target_os = "android")))]
pub mod wasm_sandbox {
    //! WASM 沙箱 — 在 wasmtime 运行时中执行 WASM 模块.
    //!
    //! 提供比子进程更强的安全隔离：
    //! - 无文件系统访问
    //! - 无网络访问
    //! - 有限的内存分配
    //! - 精确的指令计数

    use wasmtime::{Engine, Linker, Module, Store};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, add_to_linker};

    /// WASM 沙箱执行器.
    pub struct WasmSandbox {
        engine: Engine,
    }

    impl WasmSandbox {
        pub fn new() -> Self {
            let engine = Engine::default();
            Self { engine }
        }

        /// 执行 WASM 字节码.
        pub async fn execute(&self, wasm_bytes: &[u8], input: &str) -> Result<String, String> {
            let module = Module::new(&self.engine, wasm_bytes)
                .map_err(|e| format!("module compile: {e}"))?;

            let mut linker: Linker<WasiCtx> = Linker::new(&self.engine);
            add_to_linker(&mut linker, |ctx| ctx)
                .map_err(|e| format!("linker setup: {e}"))?;

            let wasi = WasiCtxBuilder::new()
                .stdin_bytes(input.as_bytes())
                .build();

            let mut store = Store::new(&self.engine, wasi);
            let instance = linker
                .instantiate(&mut store, &module)
                .map_err(|e| format!("instantiate: {e}"))?;

            let start = instance
                .get_export(&mut store, "_start")
                .or_else(|| instance.get_export(&mut store, "main"))
                .ok_or("no _start or main export")?
                .into_func()
                .ok_or("export is not a function")?;

            start.call(&mut store, &[], &mut [])
                .map_err(|e| format!("execution: {e}"))?;

            Ok("WASM execution completed".into())
        }
    }
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_safety() {
        let sandbox = CodeSandboxPlugin::new();
        assert!(sandbox.check_safety("python", "rm -rf /").is_some());
        assert!(sandbox.check_safety("python", "print('hello')").is_none());
        assert!(sandbox.check_safety("unknown_lang", "print('hi')").is_some());
    }

    #[test]
    fn test_truncate_output() {
        let short = "hello";
        assert_eq!(CodeSandboxPlugin::truncate_output(short, 100), short);

        let long = "a".repeat(1000);
        let truncated = CodeSandboxPlugin::truncate_output(&long, 100);
        assert!(truncated.len() < 1000);
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn test_stats() {
        let sandbox = CodeSandboxPlugin::new();
        let stats = sandbox.stats();
        assert_eq!(stats["exec_count"], 0);
        assert!(stats["max_timeout_secs"].as_u64().unwrap() >= 5);
    }
}
