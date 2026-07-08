//! LLM 工厂模块 — 根据配置动态选择后端实现.
//!
//! 设计原则:
//! - 配置驱动: 通过 `LlmConfig.provider` 选择提供商
//! - 自动降级: 未设置 API key 或 feature 未启用时自动回退到 Mock/NullLlm
//! - 自动重试: 构建时自动包装 `RetryLlm`（超时 + 指数退避重试）
//! - 开闭原则: 新增协议只需新增 impl + factory 分支

use async_trait::async_trait;
use lingshu_config::settings::{LlmConfig, LlmProvider};
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::{Llm, LlmChunk, LlmRequest, LlmResponse};
use std::collections::HashMap;

/// 根据配置构建带超时重试的 LLM 实例.
///
/// 自动包装 `RetryLlm`: 使用配置中的 `timeout_seconds` 和 `max_retries`.
pub fn build_llm(config: &LlmConfig) -> Box<dyn Llm> {
    let inner = build_raw(config);
    let max_retries = 3;
    let timeout = config.timeout_seconds.max(5);
    crate::with_retry(inner, max_retries, timeout)
}

/// 根据配置构建裸 LLM 实例（无重试包装）.
pub fn build_raw(config: &LlmConfig) -> Box<dyn Llm> {
    match config.provider {
        LlmProvider::Openai => build_openai(config),
        LlmProvider::Anthropic => build_anthropic(config),
        LlmProvider::Groq => build_groq(config),
        LlmProvider::Mock => build_mock(config),
        LlmProvider::Llmkit => build_llmkit(config),
        LlmProvider::Llamacpp => build_llamacpp(config),
    }
}

// ── NullLlm — 当没有任何 provider 可用时返回 ─────────

#[allow(dead_code)]
struct NullLlm;

#[async_trait]
impl Llm for NullLlm {
    async fn invoke(&self, _ctx: LsContext, _request: LlmRequest) -> LsResult<LlmResponse> {
        Err(LsError::NotImplemented(
            "no LLM provider enabled: enable at least one feature (openai, anthropic, groq, mock)"
                .into(),
        ))
    }
    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        Err(LsError::NotImplemented("no LLM provider enabled".into()))
    }
    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        Ok(HashMap::new())
    }
}

// ── OpenAI ──────────────────────────────────────────

#[cfg(feature = "openai")]
fn build_openai(config: &LlmConfig) -> Box<dyn Llm> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok());
    match api_key {
        Some(key) => {
            tracing::info!(
                "llm: using OpenAI provider (model: {})",
                config.default_model
            );
            let base_url = std::env::var("OPENAI_BASE_URL").ok();
            Box::new(crate::OpenAiLlm::new(key, &config.default_model, base_url))
        }
        None => {
            tracing::warn!("OPENAI_API_KEY not set for 'openai' provider, falling back to Mock");
            build_mock(config)
        }
    }
}

#[cfg(not(feature = "openai"))]
fn build_openai(_config: &LlmConfig) -> Box<dyn Llm> {
    tracing::warn!("'openai' feature not enabled, falling back");
    build_fallback()
}

// ── Anthropic ───────────────────────────────────────

#[cfg(feature = "anthropic")]
fn build_anthropic(config: &LlmConfig) -> Box<dyn Llm> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());
    match api_key {
        Some(key) => {
            tracing::info!(
                "llm: using Anthropic provider (model: {})",
                config.default_model
            );
            Box::new(crate::AnthropicLlm::new(key, &config.default_model, None))
        }
        None => {
            tracing::warn!("ANTHROPIC_API_KEY not set for 'anthropic' provider, falling back");
            build_mock(config)
        }
    }
}

#[cfg(not(feature = "anthropic"))]
fn build_anthropic(_config: &LlmConfig) -> Box<dyn Llm> {
    tracing::warn!("'anthropic' feature not enabled, falling back");
    build_fallback()
}

// ── Groq (OpenAI 兼容格式) ───────────────────────────

#[cfg(any(feature = "groq", feature = "openai"))]
fn build_groq(config: &LlmConfig) -> Box<dyn Llm> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("GROQ_API_KEY").ok());
    match api_key {
        Some(key) => {
            tracing::info!("llm: using Groq provider (model: {})", config.default_model);
            Box::new(crate::OpenAiLlm::new(
                key,
                &config.default_model,
                Some("https://api.groq.com/openai/v1".into()),
            ))
        }
        None => {
            tracing::warn!("GROQ_API_KEY not set for 'groq' provider, falling back");
            build_mock(config)
        }
    }
}

#[cfg(not(any(feature = "groq", feature = "openai")))]
fn build_groq(_config: &LlmConfig) -> Box<dyn Llm> {
    tracing::warn!("'groq' feature not enabled, falling back");
    build_fallback()
}

// ── Mock / Fallback ─────────────────────────────────

#[cfg(feature = "mock")]
fn build_mock(_config: &LlmConfig) -> Box<dyn Llm> {
    tracing::info!("llm: using Mock provider (no external API calls)");
    Box::new(crate::MockLlm::new())
}

#[cfg(not(feature = "mock"))]
fn build_mock(_config: &LlmConfig) -> Box<dyn Llm> {
    build_fallback()
}

#[allow(dead_code)]
fn build_fallback() -> Box<dyn Llm> {
    tracing::warn!("no LLM backend available, using NullLlm (all calls return NotImplemented)");
    Box::new(NullLlm)
}

/// 便捷函数: 直接加载配置并构建带重试的 LLM 实例.
pub fn build_llm_from_env() -> Box<dyn Llm> {
    let config = lingshu_config::ConfigLoader::with_cwd()
        .load(None)
        .unwrap_or_default();
    let provider = LlmProvider::from_env();
    let mut llm_config = config.llm.clone();
    llm_config.provider = provider;
    build_llm(&llm_config)
}

// ── llmkit ──────────────────────────────────────────

#[cfg(feature = "llmkit")]
fn build_llmkit(config: &LlmConfig) -> Box<dyn Llm> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| {
            // Try common env vars based on the llmkit provider
            let provider = config.llmkit_provider.to_lowercase();
            match provider.as_str() {
                "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
                "openai" => std::env::var("OPENAI_API_KEY").ok(),
                "google" => std::env::var("GOOGLE_API_KEY").ok(),
                "mistral" => std::env::var("MISTRAL_API_KEY").ok(),
                "groq" => std::env::var("GROQ_API_KEY").ok(),
                "deepseek" => std::env::var("DEEPSEEK_API_KEY").ok(),
                "ollama" => Some("ollama".into()), // Ollama doesn't need a real key
                "bedrock" | "vertex" => std::env::var("AWS_ACCESS_KEY_ID").ok(),
                _ => std::env::var("LLMKIT_API_KEY")
                    .or_else(|_| std::env::var("API_KEY").ok()),
            }
        });
    match api_key {
        Some(key) => {
            tracing::info!(
                "llm: using llmkit provider '{}' (model: {})",
                config.llmkit_provider,
                config.default_model
            );
            Box::new(crate::LlmkitLlm::new(
                &config.llmkit_provider,
                &key,
                &config.default_model,
            ))
        }
        None => {
            tracing::warn!(
                "API key not set for llmkit provider '{}', falling back to Mock",
                config.llmkit_provider
            );
            build_mock(config)
        }
    }
}

#[cfg(not(feature = "llmkit"))]
fn build_llmkit(_config: &LlmConfig) -> Box<dyn Llm> {
    tracing::warn!("'llmkit' feature not enabled, falling back");
    build_fallback()
}

// ── llama.cpp ──────────────────────────────────────

#[cfg(feature = "llamacpp")]
fn build_llamacpp(config: &LlmConfig) -> Box<dyn Llm> {
    let base_url = std::env::var("LLAMACPP_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    tracing::info!(
        "llm: using llama.cpp backend (url: {}, model: {})",
        base_url,
        config.default_model
    );
    Box::new(crate::LlamaCppLlm::new(&base_url, &config.default_model))
}

#[cfg(not(feature = "llamacpp"))]
fn build_llamacpp(_config: &LlmConfig) -> Box<dyn Llm> {
    tracing::warn!("'llamacpp' feature not enabled, falling back");
    build_fallback()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;

    #[test]
    fn test_build_mock() {
        let config = LlmConfig {
            provider: LlmProvider::Mock,
            ..LlmConfig::default()
        };
        let llm = build_llm(&config);
        let ctx = LsContext::with_session(LsId::new());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let stats = rt.block_on(llm.usage_stats(ctx)).unwrap();
        assert_eq!(stats.get("prompt_tokens"), Some(&0));
    }

    #[test]
    fn test_build_openai_no_key_falls_back() {
        let config = LlmConfig {
            provider: LlmProvider::Openai,
            api_key: None,
            ..LlmConfig::default()
        };
        let llm = build_llm(&config);
        let ctx = LsContext::with_session(LsId::new());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let stats = rt.block_on(llm.usage_stats(ctx)).unwrap();
        assert!(stats.contains_key("prompt_tokens") || stats.is_empty());
    }

    #[test]
    fn test_build_anthropic_no_key_falls_back() {
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            api_key: None,
            ..LlmConfig::default()
        };
        let llm = build_llm(&config);
        let ctx = LsContext::with_session(LsId::new());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let stats = rt.block_on(llm.usage_stats(ctx)).unwrap();
        assert!(stats.contains_key("prompt_tokens") || stats.is_empty());
    }

    #[test]
    fn test_build_groq_falls_back() {
        let config = LlmConfig {
            provider: LlmProvider::Groq,
            ..LlmConfig::default()
        };
        let llm = build_llm(&config);
        let ctx = LsContext::with_session(LsId::new());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let stats = rt.block_on(llm.usage_stats(ctx)).unwrap();
        assert!(stats.contains_key("prompt_tokens") || stats.is_empty());
    }

    #[test]
    fn test_provider_from_env() {
        unsafe {
            std::env::set_var("LLM_PROVIDER", "anthropic");
        }
        assert_eq!(LlmProvider::from_env(), LlmProvider::Anthropic);
        unsafe {
            std::env::remove_var("LLM_PROVIDER");
        }
    }
}
