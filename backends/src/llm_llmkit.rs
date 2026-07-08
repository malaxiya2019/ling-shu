//! 🚀 llmkit 统一 LLM 后端 — 27+ 提供商支持.
//!
//! 通过单个库支持 Anthropic, OpenAI, Google Gemini, AWS Bedrock,
//! Mistral, Groq, DeepSeek, Ollama, vLLM 等 27+ 提供商。
//!
//! ## 配置
//!
//! ```yaml
//! llm:
//!   provider: llmkit
//!   default_model: claude-sonnet-4-20250514
//!   llmkit_provider: anthropic   # llmkit 内部提供商名称
//! ```
//!
//! 所有 llmkit 支持的提供商名:
//! ai21, anthropic, azure, bedrock, cerebras, cohere, deepseek,
//! doubao, ernie, fireworks, google, grok, groq, jan, llamacpp,
//! lmstudio, minimax, mistral, moonshot, ollama, openai, openrouter,
//! perplexity, qwen, sambanova, together, vertex, vllm, yi, zhipu

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 将 lingshu 消息转换为 llmkit 消息格式.
fn to_llmkit_messages(messages: &[LlmMessage]) -> Vec<llmkit::Message> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                LlmRole::System => "system",
                LlmRole::User => "user",
                LlmRole::Assistant => "assistant",
                LlmRole::Tool => "tool",
            };
            llmkit::Message {
                role: role.to_string(),
                content: m.content.clone(),
            }
        })
        .collect()
}

/// llmkit 后端 — 通过 llmkit 库支持 27+ LLM 提供商.
pub struct LlmkitLlm {
    /// llmkit 客户端 (使用 Box<dyn Any> 因为 llmkit 的客户端类型不统一)
    provider_name: String,
    /// API key
    api_key: String,
    /// 默认模型
    default_model: String,
    /// 用量统计
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl LlmkitLlm {
    /// 创建新的 llmkit 后端实例.
    pub fn new(provider_name: &str, api_key: &str, default_model: &str) -> Self {
        Self {
            provider_name: provider_name.to_lowercase(),
            api_key: api_key.to_string(),
            default_model: default_model.to_string(),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
        }
    }

    /// 根据提供商名称获取 llmkit 客户端构建器.
    fn build_client(&self) -> LsResult<llmkit::builders::ClientBuilder> {
        let provider = &self.provider_name;
        let key = &self.api_key;

        // 使用 llmkit 的通用 new_client 工厂
        let provider_name = match provider.as_str() {
            "ai21" => llmkit::ProviderName::AI21,
            "anthropic" => llmkit::ProviderName::Anthropic,
            "azure" => llmkit::ProviderName::Azure,
            "bedrock" => llmkit::ProviderName::Bedrock,
            "cerebras" => llmkit::ProviderName::Cerebras,
            "cohere" => llmkit::ProviderName::Cohere,
            "deepseek" => llmkit::ProviderName::DeepSeek,
            "doubao" => llmkit::ProviderName::Doubao,
            "ernie" => llmkit::ProviderName::Ernie,
            "fireworks" => llmkit::ProviderName::Fireworks,
            "google" => llmkit::ProviderName::Google,
            "grok" => llmkit::ProviderName::Grok,
            "groq" => llmkit::ProviderName::Groq,
            "jan" => llmkit::ProviderName::Jan,
            "llamacpp" => llmkit::ProviderName::Llamacpp,
            "lmstudio" => llmkit::ProviderName::Lmstudio,
            "minimax" => llmkit::ProviderName::Minimax,
            "mistral" => llmkit::ProviderName::Mistral,
            "moonshot" => llmkit::ProviderName::Moonshot,
            "ollama" => llmkit::ProviderName::Ollama,
            "openai" => llmkit::ProviderName::OpenAI,
            "openrouter" => llmkit::ProviderName::Openrouter,
            "perplexity" => llmkit::ProviderName::Perplexity,
            "qwen" => llmkit::ProviderName::Qwen,
            "sambanova" => llmkit::ProviderName::SambaNova,
            "together" => llmkit::ProviderName::Together,
            "vertex" => llmkit::ProviderName::Vertex,
            "vllm" => llmkit::ProviderName::Vllm,
            "yi" => llmkit::ProviderName::Yi,
            "zhipu" => llmkit::ProviderName::Zhipu,
            other => {
                return Err(LsError::Config(format!(
                    "unsupported llmkit provider '{other}'. Available: anthropic, openai, google, \
                     mistral, groq, deepseek, ollama, bedrock, cohere, +20 more"
                )));
            }
        };

        Ok(llmkit::builders::new_client(provider_name, key))
    }
}

#[async_trait]
impl Llm for LlmkitLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let client = self.build_client()?;
        let messages = to_llmkit_messages(&request.messages);

        // 提取 system prompt (第一个 system 消息)
        let system = request
            .messages
            .iter()
            .find(|m| m.role == LlmRole::System)
            .map(|m| m.content.clone());

        // 提取 user prompt (最后一个 user 消息)
        let prompt = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == LlmRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let mut builder = client
            .text()
            .model(&self.default_model)
            .max_tokens(request.max_tokens.unwrap_or(4096) as u32);

        if let Some(ref sys) = system {
            builder = builder.system(sys);
        }

        // 设置温度
        if let Some(temp) = request.temperature {
            builder = builder.temperature(temp as f32);
        }

        let resp = builder.prompt(&prompt).await.map_err(|e| {
            LsError::Provider(format!("llmkit {} invoke failed: {e}", self.provider_name))
        })?;

        // 更新用量统计
        self.prompt_tokens
            .fetch_add(resp.usage.input, Ordering::SeqCst);
        self.completion_tokens
            .fetch_add(resp.usage.output, Ordering::SeqCst);

        Ok(LlmResponse {
            text: resp.text,
            tool_calls: None,
            usage: LlmUsage {
                prompt_tokens: resp.usage.input,
                completion_tokens: resp.usage.output,
                total_tokens: resp.usage.input + resp.usage.output,
            },
            finish_reason: Some("stop".into()),
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        // TODO: llmkit stream 支持 (callback-based, 需要适配)
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
