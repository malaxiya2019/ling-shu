//! OpenAI LLM 后端实现 — 对接 Chat Completions API.
//!
//! 支持:
//! - 非流式调用 (invoke)
//! - 流式调用 (invoke_stream)
//! - 用量统计

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};

// ── OpenAI API 类型 ─────────────────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<UsageData>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct UsageData {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

// ── OpenAI LLM 实现 ─────────────────────────────────

pub struct OpenAiLlm {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl OpenAiLlm {
    /// 创建 OpenAI LLM 实例.
    ///
    /// # 参数
    /// - `api_key`: OpenAI API 密钥
    /// - `default_model`: 默认模型名 (如 "gpt-4o")
    /// - `base_url`: API 基础 URL (默认 "https://api.openai.com/v1")
    pub fn new(api_key: impl Into<String>, default_model: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
            default_model: default_model.into(),
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
        }
    }

    /// 返回默认模型名称.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn convert_messages(msgs: &[LlmMessage]) -> Vec<ChatMessage> {
        msgs.iter().map(|m| ChatMessage {
            role: match m.role {
                LlmRole::System => "system".into(),
                LlmRole::User => "user".into(),
                LlmRole::Assistant => "assistant".into(),
                LlmRole::Tool => "tool".into(),
            },
            content: m.content.clone(),
        }).collect()
    }
}

#[async_trait]
impl Llm for OpenAiLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let req_body = ChatRequest {
            model: request.model,
            messages: Self::convert_messages(&request.messages),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(false),
        };

        let resp = self.client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req_body)
            .send()
            .await
            .map_err(|e| LsError::Llm(format!("request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(LsError::Llm(format!("API error {status}: {body}")));
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .map_err(|e| LsError::Llm(format!("parse failed: {e}")))?;

        let choice = chat_resp.choices.into_iter().next()
            .ok_or_else(|| LsError::Llm("no choices returned".into()))?;

        let usage = chat_resp.usage.unwrap_or(UsageData {
            prompt_tokens: 0, completion_tokens: 0, total_tokens: 0,
        });

        self.prompt_tokens.fetch_add(usage.prompt_tokens, Ordering::AcqRel);
        self.completion_tokens.fetch_add(usage.completion_tokens, Ordering::AcqRel);

        Ok(LlmResponse {
            message: LlmMessage {
                role: LlmRole::Assistant,
                content: choice.message.content.unwrap_or_default(),
                name: None,
                tool_calls: None,
            },
            finish_reason: choice.finish_reason,
            usage: LlmUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            },
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        let req_body = ChatRequest {
            model: request.model,
            messages: Self::convert_messages(&request.messages),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(true),
        };

        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let url = self.chat_url();
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        tokio::spawn(async move {
            let response = match client
                .post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&req_body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(LsError::Llm(format!("stream request failed: {e}")))).await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx.send(Err(LsError::Llm(format!("API error {status}: {body}")))).await;
                return;
            }

            let mut stream = response.bytes_stream();
            use futures_util::StreamExt;
            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(LsError::Llm(format!("stream read error: {e}")))).await;
                        break;
                    }
                };

                // SSE 解析: 行以 "data: " 开头
                let text = String::from_utf8_lossy(&chunk);
                for line in text.lines() {
                    let line = line.trim();
                    if !line.starts_with("data: ") { continue; }
                    let data = &line[6..];
                    if data == "[DONE]" { break; }

                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(sc) => {
                            for choice in sc.choices {
                                let _ = tx.send(Ok(LlmChunk {
                                    content: choice.delta.content,
                                    tool_calls: None,
                                    finish_reason: choice.finish_reason,
                                })).await;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(LsError::Llm(format!("stream parse: {e}")))).await;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut map = HashMap::new();
        map.insert("prompt_tokens".into(), self.prompt_tokens.load(Ordering::Acquire));
        map.insert("completion_tokens".into(), self.completion_tokens.load(Ordering::Acquire));
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_openai_usage_stats() {
        let llm = OpenAiLlm::new("sk-test", "gpt-4o", None);
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let stats = llm.usage_stats(ctx).await.unwrap();
        assert_eq!(stats.get("prompt_tokens"), Some(&0));
        assert_eq!(stats.get("completion_tokens"), Some(&0));
    }

    #[test]
    fn test_message_conversion() {
        let msgs = vec![
            LlmMessage { role: LlmRole::System, content: "be helpful".into(), name: None, tool_calls: None },
            LlmMessage { role: LlmRole::User, content: "hi".into(), name: None, tool_calls: None },
        ];
        let converted = OpenAiLlm::convert_messages(&msgs);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].content, "hi");
    }
}
