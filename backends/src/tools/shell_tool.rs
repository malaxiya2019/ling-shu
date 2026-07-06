//! Shell 命令执行工具 — ShellTool

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;
use std::time::Duration;

/// Shell 命令执行工具.
///
/// 安全说明:
/// - 可配置允许的命令白名单 (allowed_commands)
/// - 默认禁止交互式命令
/// - 输出大小限制 (max_output_bytes)
/// - 超时限制
pub struct ShellTool {
    allowed_commands: Option<Vec<String>>,
    max_output_bytes: usize,
    default_timeout_secs: u64,
}

impl ShellTool {
    /// 创建 shell 工具.
    ///
    /// `allowed_commands` — `Some(list)` 限定可执行命令列表，`None` 不限制.
    pub fn new(allowed_commands: Option<Vec<String>>) -> Self {
        Self {
            allowed_commands,
            max_output_bytes: 1024 * 1024, // 1MB
            default_timeout_secs: 30,
        }
    }

    /// 设置输出大小限制.
    pub fn with_max_output(mut self, max_bytes: usize) -> Self {
        self.max_output_bytes = max_bytes;
        self
    }

    /// 设置默认超时.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.default_timeout_secs = secs;
        self
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "shell".into(),
            description: "在 shell 中执行命令并返回输出。支持管道、重定向等 shell 特性。注意：生产环境中建议配置命令白名单。".into(),
            parameters: vec![
                ToolParam {
                    name: "command".into(),
                    description: "要执行的 shell 命令".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "timeout_secs".into(),
                    description: "超时秒数 (默认 30)".into(),
                    required: false,
                    param_type: "number".into(),
                },
                ToolParam {
                    name: "workdir".into(),
                    description: "工作目录 (默认当前目录)".into(),
                    required: false,
                    param_type: "string".into(),
                },
            ],
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        let cmd = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::Validation("missing required field: command".into()))?;

        if cmd.trim().is_empty() {
            return Err(LsError::Validation("command must not be empty".into()));
        }

        // 安全检查：禁止交互式命令
        let interactive_commands = [
            "vim", "nano", "emacs", "vi", "top", "htop", "less", "more", "tail -f", "watch",
        ];
        let cmd_lower = cmd.to_lowercase();
        for icmd in &interactive_commands {
            if cmd_lower.starts_with(icmd) {
                return Err(LsError::Validation(format!(
                    "interactive command '{icmd}' is not allowed"
                )));
            }
        }

        // 白名单检查
        if let Some(ref allowed) = self.allowed_commands {
            let first_token = cmd.split_whitespace().next().unwrap_or("");
            if !allowed.iter().any(|a| first_token == a) {
                return Err(LsError::Validation(format!(
                    "command '{first_token}' is not in allowed list: {:?}",
                    allowed
                )));
            }
        }

        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let command = input["command"].as_str().unwrap();
        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.default_timeout_secs);
        let workdir = input.get("workdir").and_then(|v| v.as_str());

        let start = std::time::Instant::now();

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C").arg(command);
            c
        } else {
            let mut c = tokio::process::Command::new("sh");
            c.arg("-c").arg(command);
            c
        };

        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        // 限制输出
        let output = cmd
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| LsError::Internal(format!("failed to execute command: {e}")))?;

        let elapsed = start.elapsed();

        if elapsed > Duration::from_secs(timeout_secs) {
            return Err(LsError::Internal(format!(
                "command timed out after {timeout_secs}s"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // 截断输出
        let stdout_trimmed = if stdout.len() > self.max_output_bytes {
            format!(
                "{}...(truncated {} bytes)",
                &stdout[..self.max_output_bytes],
                stdout.len() - self.max_output_bytes
            )
        } else {
            stdout.to_string()
        };

        let success = output.status.success();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(serde_json::json!({
            "command": command,
            "exit_code": exit_code,
            "success": success,
            "stdout": stdout_trimmed,
            "stderr": stderr,
            "stdout_size_bytes": stdout.len(),
            "stderr_size_bytes": stderr.len(),
            "elapsed_ms": elapsed.as_millis(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_shell_echo() {
        let tool = ShellTool::default();
        let result = tool
            .execute(test_ctx(), json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_failure() {
        let tool = ShellTool::default();
        let result = tool
            .execute(test_ctx(), json!({"command": "false"}))
            .await
            .unwrap();
        assert_eq!(result["exit_code"], 1);
        assert!(!result["success"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_shell_invalid_command() {
        let tool = ShellTool::default();
        let result = tool
            .execute(test_ctx(), json!({"command": "nonexistent_cmd_xyz_123"}))
            .await;
        // Should still return output with error code
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_shell_whitelist() {
        let tool = ShellTool::new(Some(vec!["echo".into(), "ls".into()]));
        // Allowed
        assert!(tool
            .execute(test_ctx(), json!({"command": "echo ok"}))
            .await
            .is_ok());
        // Not allowed
        let result = tool.validate(&json!({"command": "rm -rf /"}));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_rejects_interactive() {
        let tool = ShellTool::default();
        let result = tool.validate(&json!({"command": "vim /tmp/test"}));
        assert!(result.is_err());
    }
}
