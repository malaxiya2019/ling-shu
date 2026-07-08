//! 🚀 llmkit 统一 LLM 后端 — 100+ 提供商支持.
//!
//! 通过 llmkit 0.1 库支持 100+ 提供商、11000+ 模型.
//!
//! ## 配置
//!
//! ```yaml
//! llm:
//!   provider: llmkit
//!   default_model: claude-sonnet-4-20250514
//!   llmkit_provider: anthropic   # llmkit 内部提供商名称
//! ```

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use llmkit::{CompletionRequest, LLMKitClient, Message, ContentBlock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 提供商名称到 llmkit ClientBuilder 方法的映射.
fn build_client(provider: &str, api_key: &str) -> LsResult<LLMKitClient> {
    let mut builder = LLMKitClient::builder();

    builder = match provider {
        "ai21" => builder.with_provider_config(
            "ai21",
            llmkit::ProviderConfig::new(api_key),
        ),
        "anthropic" => builder.with_anthropic(api_key).map_err(|e| {
            LsError::Internal(format!("llmkit anthropic init failed: {e}"))
        })?,
        "azure" => builder.with_provider_config(
            "azure",
            llmkit::ProviderConfig::new(api_key),
        ),
        "bedrock" => builder.with_provider_config(
            "bedrock",
            llmkit::ProviderConfig::new(api_key),
        ),
        "cerebras" => builder.with_provider_config(
            "cerebras",
            llmkit::ProviderConfig::new(api_key),
        ),
        "cohere" => builder.with_provider_config(
            "cohere",
            llmkit::ProviderConfig::new(api_key),
        ),
        "deepseek" => builder.with_provider_config(
            "deepseek",
            llmkit::ProviderConfig::new(api_key),
        ),
        "doubao" => builder.with_provider_config(
            "doubao",
            llmkit::ProviderConfig::new(api_key),
        ),
        "fireworks" => builder.with_provider_config(
            "fireworks",
            llmkit::ProviderConfig::new(api_key),
        ),
        "google" => builder.with_provider_config(
            "google",
            llmkit::ProviderConfig::new(api_key),
        ),
        "grok" => builder.with_provider_config(
            "grok",
            llmkit::ProviderConfig::new(api_key),
        ),
        "groq" => builder.with_provider_config(
            "groq",
            llmkit::ProviderConfig::new(api_key),
        ),
        "jan" => builder.with_provider_config(
            "jan",
            llmkit::ProviderConfig::new(api_key),
        ),
        "llamacpp" => builder.with_provider_config(
            "llamacpp",
            llmkit::ProviderConfig::new(api_key),
        ),
        "lmstudio" => builder.with_provider_config(
            "lmstudio",
            llmkit::ProviderConfig::new(api_key),
        ),
        "mistral" => builder.with_provider_config(
            "mistral",
            llmkit::ProviderConfig::new(api_key),
        ),
        "moonshot" => builder.with_provider_config(
            "moonshot",
            llmkit::ProviderConfig::new(api_key),
        ),
        "ollama" => builder.with_provider_config(
            "ollama",
            llmkit::ProviderConfig::new(api_key),
        ),
        "openai" => builder.with_openai(api_key).map_err(|e| {
            LsError::Internal(format!("llmkit openai init failed: {e}"))
        })?,
        "openrouter" => builder.with_provider_config(
            "openrouter",
            llmkit::ProviderConfig::new(api_key),
        ),
        "perplexity" => builder.with_provider_config(
            "perplexity",
            llmkit::ProviderConfig::new(api_key),
        ),
        "qwen" => builder.with_provider_config(
            "qwen",
            llmkit::ProviderConfig::new(api_key),
        ),
        "sambanova" => builder.with_provider_config(
            "sambanova",
            llmkit::ProviderConfig::new(api_key),
        ),
        "together" => builder.with_provider_config(
            "together",
            llmkit::ProviderConfig::new(api_key),
        ),
        "vertex" => builder.with_provider_config(
            "vertex",
            llmkit::ProviderConfig::new(api_key),
        ),
        "vllm" => builder.with_provider_config(
            "vllm",
            llmkit::ProviderConfig::new(api_key),
        ),
        other => {
            return Err(LsError::Config(format!(
                "unsupported llmkit provider '{other}'. Available: anthropic, openai, google, \
                 mistral, groq, deepseek, ollama, bedrock, cohere, +20 more"
            )));
        }
    };

    builder.build().map_err(|e| {
        LsError::Internal(format!("llmkit client build failed: {e}"))
    })
}

/// llmkit 后端 — 通过 llmkit 库支持 100+ LLM 提供商.
pub struct LlmkitLlm {
    client: LLMKitClient,
    provider_name: String,
    default_model: String,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl LlmkitLlm {
    /// 创建新的 llmkit 后端实例.
    pub fn new(provider_name: &str, api_key: &str, default_model: &str) -> LsResult<Self> {
        let client = build_client(provider_name, api_key)?;
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
        // 转换消息
        let messages: Vec<Message> = request
            .messages
            .iter()
            .map(|m| match m.role {
                LlmRole::System => Message::system(&m.content),
                LlmRole::User => Message::user(&m.content),
                LlmRole::Assistant => Message::assistant(&m.content),
                LlmRole::Tool => {
                    let tool_use_id = m.name.as_deref().unwrap_or("unknown_tool");
                    Message::tool_results(vec![ContentBlock::tool_result(
                        tool_use_id,
                        &m.content,
                        false,
                    )])
                },
            })
            .collect();

        // 提取 system prompt（llmkit 支持独立的 system 字段）
        let system = request
            .messages
            .iter()
            .find(|m| m.role == LlmRole::System)
            .map(|m| m.content.clone());

        // 构建 CompletionRequest
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

        // 调用 llmkit
        let resp = self.client.complete(req).await.map_err(|e| {
            LsError::Provider(format!("llmkit {} invoke failed: {e}", self.provider_name))
        })?;

        // 更新用量统计
        self.prompt_tokens
            .fetch_add(resp.usage.input_tokens as u64, Ordering::SeqCst);
        self.completion_tokens
            .fetch_add(resp.usage.output_tokens as u64, Ordering::SeqCst);

        Ok(LlmResponse {
            text: resp.text_content(),
            tool_calls: None,
            usage: LlmUsage {
                prompt_tokens: resp.usage.input_tokens as u64,
                completion_tokens: resp.usage.output_tokens as u64,
                total_tokens: resp.usage.total_tokens() as u64,
            },
            finish_reason: Some("stop".into()),
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        Err(LsError::NotImplemented(
            "llmkit streaming not yet implemented".into(),
        ))
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut stats = HashMap::new();
        stats.insert(
            "prompt_tokens".into(),
            self.prompt_tokens.load(Ordering::SeqCst),
        );
        stats.insert(
            "completion_tokens".into(),
            self.completion_tokens.load(Ordering::SeqCst),
        );
        Ok(stats)
    }
}
