//! ToolSandbox — 工具安全执行沙箱.
//!
//! 提供超时控制、资源限制、输出截断等安全保障机制。
//! 所有工具执行必须经过沙箱检查。

use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::tool::Tool;
use serde_json::Value;
use std::time::Instant;
use tokio::time::{timeout, Duration};
use tracing::{debug, warn};

/// 沙箱管理器.
#[derive(Debug, Clone)]
pub struct ToolSandbox {
    /// 系统级默认超时（毫秒）.
    default_timeout_ms: u64,
    /// 系统级最大输出字节.
    max_output_bytes: u64,
    /// 是否启用严格模式（拒绝未配置沙箱的工具）.
    strict_mode: bool,
}

impl ToolSandbox {
    /// 创建沙箱管理器.
    pub fn new() -> Self {
        Self {
            default_timeout_ms: 30_000,
            max_output_bytes: 10_000_000,
            strict_mode: false,
        }
    }

    /// 设置默认超时.
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.default_timeout_ms = ms;
        self
    }

    /// 设置最大输出字节.
    pub fn with_max_output(mut self, bytes: u64) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    /// 设置严格模式.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    /// 在沙箱中安全执行工具.
    ///
    /// 处理：
    /// - 超时控制
    /// - 输出大小限制
    /// - 调用计时
    /// - 错误捕获
    pub async fn execute(
        &self,
        tool: &dyn Tool,
        ctx: LsContext,
        input: Value,
    ) -> LsResult<(Value, u64)> {
        let tool_info = tool.info();
        let tool_name = &tool_info.name;

        // 严格模式检查
        if self.strict_mode && tool_info.metadata.sandbox_config.is_none() {
            warn!(
                tool = %tool_name,
                "严格模式: 工具缺少沙箱配置"
            );
            return Err(LsError::Plugin(format!(
                "工具 '{tool_name}' 缺少沙箱配置，严格模式下拒绝执行"
            )));
        }

        // 确定超时时间
        let timeout_ms = tool_info
            .metadata
            .timeout_ms
            .or_else(|| {
                tool_info
                    .metadata
                    .sandbox_config
                    .as_ref()
                    .map(|c| c.max_execution_ms)
            })
            .unwrap_or(self.default_timeout_ms);

        let timeout_dur = Duration::from_millis(timeout_ms);

        debug!(
            tool = %tool_name,
            timeout_ms = timeout_ms,
            "沙箱执行工具"
        );

        // 执行带超时
        let start = Instant::now();
        let result = timeout(timeout_dur, async {
            // 校验参数
            tool.validate(&input)?;
            // 执行工具
            tool.execute(ctx, input).await
        })
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(output)) => {
                // 检查输出大小
                let output_size = serde_json::to_vec(&output)
                    .map(|v| v.len() as u64)
                    .unwrap_or(0);

                if output_size > self.max_output_bytes {
                    warn!(
                        tool = %tool_name,
                        size = output_size,
                        max = self.max_output_bytes,
                        "工具输出超出大小限制"
                    );
                    return Err(LsError::Plugin(format!(
                        "工具 '{tool_name}' 输出 {output_size} 字节，超过限制 {} 字节",
                        self.max_output_bytes
                    )));
                }

                Ok((output, duration_ms))
            }
            Ok(Err(e)) => {
                warn!(tool = %tool_name, error = %e, "工具执行失败");
                Err(e)
            }
            Err(_elapsed) => {
                warn!(
                    tool = %tool_name,
                    timeout_ms = timeout_ms,
                    "工具执行超时"
                );
                Err(LsError::Plugin(format!(
                    "工具 '{tool_name}' 执行超时 ({}ms)",
                    timeout_ms
                )))
            }
        }
    }

    /// 获取沙箱的默认超时设置.
    pub fn default_timeout_ms(&self) -> u64 {
        self.default_timeout_ms
    }

    /// 获取最大输出字节限制.
    pub fn max_output_bytes(&self) -> u64 {
        self.max_output_bytes
    }
}

impl Default for ToolSandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lingshu_core::{LsId, LsResult};
    use lingshu_traits::tool::ToolInfo;

    struct FastTool;
    #[async_trait]
    impl Tool for FastTool {
        fn info(&self) -> ToolInfo {
            ToolInfo::new("fast", "A fast tool", vec![]).with_timeout(5_000)
        }
        fn validate(&self, _input: &Value) -> LsResult<()> {
            Ok(())
        }
        async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(input)
        }

        fn duplicate(&self) -> Box<dyn lingshu_traits::tool::Tool> {
            Box::new(FastTool)
        }
    }

    struct SlowTool;
    #[async_trait]
    impl Tool for SlowTool {
        fn info(&self) -> ToolInfo {
            ToolInfo::new("slow", "A slow tool", vec![]).with_timeout(50)
        }
        fn validate(&self, _input: &Value) -> LsResult<()> {
            Ok(())
        }
        async fn execute(&self, _ctx: LsContext, _input: Value) -> LsResult<Value> {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(serde_json::json!({"done": true}))
        }

        fn duplicate(&self) -> Box<dyn lingshu_traits::tool::Tool> {
            Box::new(SlowTool)
        }
    }

    #[tokio::test]
    async fn test_fast_tool_executes() {
        let sandbox = ToolSandbox::new();
        let tool = FastTool;
        let ctx = LsContext::with_session(LsId::new());
        let result = sandbox
            .execute(&tool, ctx, serde_json::json!({"msg": "hi"}))
            .await;
        assert!(result.is_ok());
        let (output, dur) = result.unwrap();
        assert_eq!(output["msg"], "hi");
        assert!(dur >= 10);
    }

    #[tokio::test]
    async fn test_slow_tool_times_out() {
        let sandbox = ToolSandbox::new();
        let tool = SlowTool;
        let ctx = LsContext::with_session(LsId::new());
        let result = sandbox.execute(&tool, ctx, serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("超时"),
            "Expected timeout error, got: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_strict_mode_rejects_no_sandbox_config() {
        // A tool with no sandbox_config and no timeout
        struct UnsafeTool;
        #[async_trait]
        impl Tool for UnsafeTool {
            fn info(&self) -> ToolInfo {
                ToolInfo::new("unsafe", "No sandbox config", vec![])
            }
            fn validate(&self, _input: &Value) -> LsResult<()> {
                Ok(())
            }
            async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
                Ok(input)
            }

            fn duplicate(&self) -> Box<dyn lingshu_traits::tool::Tool> {
                Box::new(UnsafeTool)
            }
        }

        let sandbox = ToolSandbox::new().with_strict(true);
        let tool = UnsafeTool;
        let ctx = LsContext::with_session(LsId::new());
        let result = sandbox.execute(&tool, ctx, serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
