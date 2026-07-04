//! RetryLlm — 统一超时 + 自动重试封装层.
//!
//! 包装任意 `Box<dyn Llm>`，为其添加:
//! - `tokio::time::timeout` 超时控制
//! - 指数退避自动重试 (最多 3 次)
//!
//! # 使用
//! ```ignore
//! use lingshu_backends::RetryLlm;
//! let llm = RetryLlm::new(inner_llm, 3, 30);
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::{Llm, LlmChunk, LlmRequest, LlmResponse};
use tokio::sync::mpsc;

/// 带有超时和自动重试的 LLM 包装器.
pub struct RetryLlm {
    inner: Box<dyn Llm>,
    max_retries: u32,
    timeout: Duration,
    #[allow(dead_code)]
    base_delay: Duration,
}

impl RetryLlm {
    /// 创建 RetryLlm 包装器.
    ///
    /// # 参数
    /// - `inner`: 被包装的 LLM 实例
    /// - `max_retries`: 最大重试次数 (默认 3)
    /// - `timeout_secs`: 单次调用的超时秒数 (默认 120)
    pub fn new(inner: Box<dyn Llm>, max_retries: u32, timeout_secs: u64) -> Self {
        Self {
            inner,
            max_retries,
            timeout: Duration::from_secs(timeout_secs),
            base_delay: Duration::from_millis(500),
        }
    }

    /// 使用默认值的便捷构造函数: 3 次重试, 120 秒超时.
    pub fn with_defaults(inner: Box<dyn Llm>) -> Self {
        Self::new(inner, 3, 120)
    }

    /// 判断错误是否可重试.
    fn is_retryable(err: &LsError) -> bool {
        matches!(err, LsError::Llm(_) | LsError::Timeout(_))
    }

    /// 计算退避延迟: base * 2^attempt, 最大 30 秒.
    fn backoff(attempt: u32) -> Duration {
        let ms = 500u64 * 2u64.pow(attempt);
        Duration::from_millis(ms.min(30_000))
    }
}

#[async_trait]
impl Llm for RetryLlm {
    async fn invoke(&self, ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let mut last_err = LsError::NotImplemented("unreachable".into());

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = Self::backoff(attempt - 1);
                tracing::warn!(
                    attempt, delay_ms = delay.as_millis(),
                    "retrying LLM invoke after error"
                );
                tokio::time::sleep(delay).await;
            }

            let fut = self.inner.invoke(ctx.child(), request.clone());
            match tokio::time::timeout(self.timeout, fut).await {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => {
                    if !Self::is_retryable(&e) || attempt == self.max_retries {
                        return Err(e);
                    }
                    last_err = e;
                }
                Err(_elapsed) => {
                    if attempt == self.max_retries {
                        return Err(LsError::Timeout("retry: all attempts exhausted".into()));
                    }
                    last_err = LsError::Timeout("retry: attempt timed out".into());
                    tracing::warn!(attempt, timeout_s = %self.timeout.as_secs(), "LLM invoke timed out");
                }
            }
        }

        Err(last_err)
    }

    async fn invoke_stream(
        &self,
        ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<mpsc::Receiver<LsResult<LlmChunk>>> {
        // 流式调用不做自动重试（流中间断开无法恢复），只加超时
        let fut = self.inner.invoke_stream(ctx.child(), request);
        match tokio::time::timeout(self.timeout, fut).await {
            Ok(Ok(mut rx)) => {
                // 包装 receiver 以支持流级别超时
                let timeout = self.timeout;
                let (tx, rx_out) = mpsc::channel(64);
                tokio::spawn(async move {
                    loop {
                        match tokio::time::timeout(timeout, rx.recv()).await {
                            Ok(Some(chunk)) => {
                                if tx.send(chunk).await.is_err() {
                                    break;
                                }
                            }
                            Ok(None) => break,
                            Err(_) => {
                                let _ = tx.send(Err(LsError::Timeout("stream: idle timeout".into()))).await;
                                break;
                            }
                        }
                    }
                });
                Ok(rx_out)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(LsError::Timeout("stream: initial connection timed out".into())),
        }
    }

    async fn usage_stats(&self, ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        self.inner.usage_stats(ctx.child()).await
    }
}

/// 将 LLM 实例包装为带超时重试的版本.
pub fn with_retry(
    inner: Box<dyn Llm>,
    max_retries: u32,
    timeout_secs: u64,
) -> Box<dyn Llm> {
    Box::new(RetryLlm::new(inner, max_retries, timeout_secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockLlm;
    use lingshu_core::LsId;
    use lingshu_traits::llm::{LlmMessage, LlmRole};

    #[tokio::test]
    async fn test_retry_passthrough() {
        let inner = Box::new(MockLlm::new());
        let retry = RetryLlm::with_defaults(inner);
        let ctx = LsContext::with_session(LsId::new());
        let request = LlmRequest {
            model: "mock".into(),
            messages: vec![
                LlmMessage { role: LlmRole::User, content: "hello".into(), name: None, tool_calls: None },
            ],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        };
        let resp = retry.invoke(ctx, request).await.unwrap();
        assert!(resp.message.content.contains("hello"));
    }

    #[tokio::test]
    async fn test_retry_usage_stats() {
        let inner = Box::new(MockLlm::new());
        let retry = RetryLlm::with_defaults(inner);
        let ctx = LsContext::with_session(LsId::new());
        let stats = retry.usage_stats(ctx).await.unwrap();
        assert!(stats.contains_key("prompt_tokens"));
    }

    #[tokio::test]
    async fn test_with_retry_fn() {
        let inner = Box::new(MockLlm::new());
        let llm = with_retry(inner, 3, 30);
        let ctx = LsContext::with_session(LsId::new());
        let stats = llm.usage_stats(ctx).await.unwrap();
        assert!(stats.contains_key("prompt_tokens"));
    }
}
