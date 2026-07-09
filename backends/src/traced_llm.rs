//! TracedLlm — OTel 装饰器, 为任意 `dyn Llm` 自动注入 GenAI tracing span。
//!
//! 遵循 [Monocle GenAI Metamodel](https://github.com/monocle2ai) span 属性约定。
//!
//! ## 用法
//! ```rust,ignore
//! use lingshu_backends::traced_llm::TracedLlm;
//!
//! let inner: Arc<dyn Llm> = Arc::new(OpenAiLlm::new(...));
//! let traced: Arc<dyn Llm> = Arc::new(TracedLlm::new(inner, "gpt-4o"));
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use lingshu_traits::llm::*;
use lingshu_observability::genai::{GenAiOperation, record_usage};
use std::collections::HashMap;
use std::sync::Arc;

/// 装饰器: 包裹 `dyn Llm` 并在每次调用时创建 OTel GenAI span。
pub struct TracedLlm {
    inner: Arc<dyn Llm>,
    model_name: String,
}

impl TracedLlm {
    /// 创建跟踪装饰器。
    ///
    /// - `inner` — 实际的 LLM 后端
    /// - `model_name` — 模型标识（如 `gpt-4o`, `claude-sonnet-4`）
    pub fn new(inner: Arc<dyn Llm>, model_name: impl Into<String>) -> Self {
        Self {
            inner,
            model_name: model_name.into(),
        }
    }
}

#[async_trait]
impl Llm for TracedLlm {
    async fn invoke(&self, ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let span = lingshu_observability::genai_span!(
            GenAiOperation::Chat,
            &self.model_name,
            ctx
        );
        span.record("gen_ai.request.max_tokens", tracing::field::debug(&request.max_tokens));
        span.record("gen_ai.request.temperature", tracing::field::debug(&request.temperature));
        let _guard = span.enter();
        let start = std::time::Instant::now();

        let result = self.inner.invoke(ctx, request).await;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        match &result {
            Ok(resp) => {
                record_usage(&span, resp.usage.prompt_tokens, resp.usage.completion_tokens);
                tracing::debug!(
                    duration_ms,
                    input_tokens = resp.usage.prompt_tokens,
                    output_tokens = resp.usage.completion_tokens,
                    "LLM invoke completed",
                );
            }
            Err(e) => {
                tracing::warn!(duration_ms, error = %e, "LLM invoke failed");
            }
        }

        result
    }

    async fn invoke_stream(
        &self,
        ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        let span = lingshu_observability::genai_span!(
            GenAiOperation::ChatStream,
            &self.model_name,
            ctx
        );
        span.record("gen_ai.request.max_tokens", tracing::field::debug(&request.max_tokens));
        span.record("gen_ai.request.temperature", tracing::field::debug(&request.temperature));
        let _guard = span.enter();

        let result = self.inner.invoke_stream(ctx, request).await;

        match &result {
            Ok(_rx) => {
                tracing::debug!("LLM stream started");
            }
            Err(e) => {
                tracing::warn!(error = %e, "LLM stream initiation failed");
            }
        }

        result
    }

    async fn usage_stats(&self, ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let span = lingshu_observability::genai_span!(
            GenAiOperation::Chat,
            &self.model_name,
            ctx
        );
        let _guard = span.enter();
        self.inner.usage_stats(ctx).await
    }
}
