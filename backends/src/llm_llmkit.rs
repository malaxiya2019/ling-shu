//! 🚀 llmkit 统一 LLM 后端 — 100+ 提供商支持.
//!
//! 基于 llmkit 0.1.3 库.
//!
//! ## 配置
//!
//! ```yaml
//! llm:
//!   provider: llmkit
//!   default_model: deepseek-chat
//!   llmkit_provider: deepseek
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use llmkit::{CompletionRequest, LLMKitClient, Message, ContentBlock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 根据提供商名称构建 llmkit 客户端.
async fn build_client(provider: &str, api_key: &str) -> LsResult<LLMKitClient> {
    let mut builder = LLMKitClient::builder();

    builder = match provider {
        "deepseek" => {
            builder.with_deepseek(api_key).map_err(|e| {
                LsError::Llm(format!("llmkit deepseek init failed: {e}"))
            })?
        }
        "qwen" | "alibaba" => {
            builder.with_alibaba(api_key).map_err(|e| {
                LsError::Llm(format!("llmkit qwen init failed: {e}"))
            })?
        }
        "anthropic" => {
            builder.with_anthropic(api_key).map_err(|e| {
                LsError::Llm(format!("llmkit anthropic init failed: {e}"))
            })?
        }
        "openai" => {
            builder.with_openai(api_key).map_err(|e| {
                LsError::Llm(format!("llmkit openai init failed: {e}"))
            })?
        }
        // OpenAI 兼容的通用 provider
        _ => {
            let base_url = std::env::var("LLMKIT_BASE_URL")
                .unwrap_or_else(|_| format!("https://api.{}.com/v1", provider));
            builder.with_openai_compatible(provider, &base_url, Some(api_key.into()))
                .map_err(|e| {
                    LsError::Llm(format!("llmkit {} init failed: {e}", provider))
                })?
        }
    };

    builder.build().await.map_err(|e| {
        LsError::Llm(format!("llmkit client build failed: {e}"))
    })
}

/// llmkit 后端.
pub struct LlmkitLlm {
    client: LLMKitClient,
    provider_name: String,
    default_model: String,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl LlmkitLlm {
    pub async fn new(provider_name: &str, api_key: &str, default_model: &str) -> LsResult<Self> {
        let client = build_client(provider_name, api_key).await?;
        Ok(Self {
            client,
            provider_name: provider_name.to_string(),
            default_model: default_model.to_string(),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
        })
    }
}

#[async_trait]
impl Llm for LlmkitLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let messages: Vec<Message> = request
            .messages
            .iter()
            .map(|m| match m.role {
                LlmRole::System => Message::system(&m.content),
                LlmRole::User => Message::user(&m.content),
                LlmRole::Assistant => Message::assistant(&m.content),
                LlmRole::Tool => {
                    let tool_use_id = m.name.as_deref().unwrap_or("unknown_tool");
                    Message::tool_results(vec![ContentBlock::ToolResult {
                        tool_use_id: tool_use_id.into(),
                        content: m.content.clone(),
                        is_error: false,
                    }])
                },
            })
            .collect();

        let system = request
            .messages
            .iter()
            .find(|m| m.role == LlmRole::System)
            .map(|m| m.content.clone());

        let mut req = CompletionRequest::new(&self.default_model, messages);
        if let Some(ref sys) = system {
            req = req.with_system(sys);
        }
        if let Some(max_tokens) = request.max_tokens {
            req = req.with_max_tokens(max_tokens as u32);
        }
        if let Some(temp) = request.temperature {
            req = req.with_temperature(temp as f32);
        }

        let resp = self.client.complete(req).await.map_err(|e| {
            LsError::Llm(format!("llmkit {} invoke failed: {e}", self.provider_name))
        })?;

        self.prompt_tokens
            .fetch_add(resp.usage.input_tokens as u64, Ordering::SeqCst);
        self.completion_tokens
            .fetch_add(resp.usage.output_tokens as u64, Ordering::SeqCst);

        Ok(LlmResponse {
            message: LlmMessage {
                role: LlmRole::Assistant,
                content: resp.text_content(),
                content_parts: None,
                name: None,
                tool_calls: None,
            },
            finish_reason: "stop".into(),
            usage: LlmUsage {
                prompt_tokens: resp.usage.input_tokens as u64,
                completion_tokens: resp.usage.output_tokens as u64,
                total_tokens: resp.usage.total_tokens() as u64,
            },
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        Err(LsError::NotImplemented("llmkit streaming not yet implemented".into()))
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut stats = HashMap::new();
        stats.insert("prompt_tokens".into(), self.prompt_tokens.load(Ordering::SeqCst));
        stats.insert("completion_tokens".into(), self.completion_tokens.load(Ordering::SeqCst));
        Ok(stats)
    }
}
