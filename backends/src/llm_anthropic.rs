//! Anthropic LLM 后端 — 对接 Messages API.
//!
//! 支持:
//! - 非流式调用 (invoke)
//! - 流式调用 (invoke_stream) — SSE 事件解析
//! - 用量统计

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

// ── Anthropic API 类型 ──────────────────────────────

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct MessagesResponse {
    id: String,
    #[serde(rename = "type")]
    resp_type: String,
    role: String,
    content: Vec<ContentBlock>,
    model: String,
    #[serde(rename = "stop_reason")]
    stop_reason: String,
    usage: UsageData,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct UsageData {
    #[serde(rename = "input_tokens")]
    input_tokens: u64,
    #[serde(rename = "output_tokens")]
    output_tokens: u64,
}

// ── Anthropic SSE 流式事件类型 ───────────────────────

#[derive(Deserialize)]
struct ContentBlockDeltaEvent {
    #[allow(dead_code)]
    index: u32,
    delta: ContentBlockDelta,
}

#[derive(Deserialize)]
struct ContentBlockDelta {
    #[serde(rename = "type")]
    _block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct MessageDeltaEvent {
    #[serde(rename = "stop_reason")]
    _stop_reason: Option<String>,
    #[serde(rename = "stop_sequence")]
    _stop_sequence: Option<String>,
    #[allow(dead_code)]
    usage: Option<UsageData>,
}

// ── Anthropic LLM 实现 ──────────────────────────────

/// Anthropic LLM 后端.
pub struct AnthropicLlm {
    client: Client,
    api_key: String,
    base_url: String,
    anthropic_version: String,
    default_model: String,
    input_tokens: AtomicU64,
    output_tokens: AtomicU64,
}

impl AnthropicLlm {
    /// 创建 Anthropic LLM 实例.
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com/v1".into()),
            anthropic_version: "2023-06-01".into(),
            default_model: default_model.into(),
            input_tokens: AtomicU64::new(0),
            output_tokens: AtomicU64::new(0),
        }
    }

    #[allow(dead_code)]
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    fn messages_url(&self) -> String {
        format!("{}/messages", self.base_url)
    }

    fn convert_messages(msgs: &[LlmMessage]) -> Vec<AnthropicMessage> {
        msgs.iter()
            .map(|m| AnthropicMessage {
                role: match m.role {
                    LlmRole::System => "user".to_string(),
                    LlmRole::User => "user".to_string(),
                    LlmRole::Assistant => "assistant".to_string(),
                    LlmRole::Tool => "user".to_string(),
                },
                content: m.content.clone(),
            })
            .collect()
    }

    fn extract_system(msgs: &[LlmMessage]) -> Option<String> {
        msgs.iter()
            .find(|m| matches!(m.role, LlmRole::System))
            .map(|m| m.content.clone())
    }
}

#[async_trait]
impl Llm for AnthropicLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        let system = Self::extract_system(&request.messages);
        let messages = Self::convert_messages(&request.messages);

        let req_body = MessagesRequest {
            model: request.model,
            messages,
            system,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: Some(false),
        };

        let resp = self
            .client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.anthropic_version)
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| LsError::Llm(format!("anthropic request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(LsError::Llm(format!(
                "Anthropic API error {status}: {body}"
            )));
        }

        let msg_resp: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| LsError::Llm(format!("anthropic parse failed: {e}")))?;

        let text = msg_resp
            .content
            .into_iter()
            .find(|b| b.block_type == "text")
            .and_then(|b| b.text)
            .unwrap_or_default();

        self.input_tokens
            .fetch_add(msg_resp.usage.input_tokens, Ordering::AcqRel);
        self.output_tokens
            .fetch_add(msg_resp.usage.output_tokens, Ordering::AcqRel);

        Ok(LlmResponse {
            message: LlmMessage {
                role: LlmRole::Assistant,
                content: text,
                content_parts: None,
        name: None,
                tool_calls: None,
            },
            finish_reason: msg_resp.stop_reason,
            usage: LlmUsage {
                prompt_tokens: msg_resp.usage.input_tokens,
                completion_tokens: msg_resp.usage.output_tokens,
                total_tokens: msg_resp.usage.input_tokens + msg_resp.usage.output_tokens,
            },
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<mpsc::Receiver<LsResult<LlmChunk>>> {
        let system = Self::extract_system(&request.messages);
        let messages = Self::convert_messages(&request.messages);

        let req_body = MessagesRequest {
            model: request.model,
            messages,
            system,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: Some(true),
        };

        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let url = self.messages_url();
        let version = self.anthropic_version.clone();
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let response = match client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", &version)
                .header("Content-Type", "application/json")
                .json(&req_body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(Err(LsError::Llm(format!(
                            "anthropic stream request failed: {e}"
                        ))))
                        .await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx
                    .send(Err(LsError::Llm(format!(
                        "Anthropic API error {status}: {body}"
                    ))))
                    .await;
                return;
            }

            // Anthropic SSE 流: 每行格式 "event: <name>" / "data: <json>"
            use futures_util::StreamExt;
            let mut stream = response.bytes_stream();
            let mut current_event = String::new();

            while let Some(chunk_result) = stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(Err(LsError::Llm(format!("stream read error: {e}"))))
                            .await;
                        break;
                    }
                };

                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        current_event.clear();
                        continue;
                    }
                    if let Some(event_name) = line.strip_prefix("event: ") {
                        current_event = event_name.to_string();
                        continue;
                    }
                    if let Some(data) = line.strip_prefix("data: ") {
                        match current_event.as_str() {
                            "content_block_delta" => {
                                if let Ok(delta) =
                                    serde_json::from_str::<ContentBlockDeltaEvent>(data)
                                {
                                    if let Some(text) = delta.delta.text {
                                        let _ = tx
                                            .send(Ok(LlmChunk {
                                                content: Some(text),
                                                tool_calls: None,
                                                finish_reason: None,
                                            }))
                                            .await;
                                    }
                                }
                            }
                            "message_delta" => {
                                if let Ok(_delta) = serde_json::from_str::<MessageDeltaEvent>(data)
                                {
                                    let _ = tx
                                        .send(Ok(LlmChunk {
                                            content: None,
                                            tool_calls: None,
                                            finish_reason: Some("stop".into()),
                                        }))
                                        .await;
                                }
                            }
                            _ => {
                                // message_start, content_block_start/stop, message_stop, ping — 忽略
                            }
                        }
                        continue;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut map = HashMap::new();
        map.insert(
            "input_tokens".into(),
            self.input_tokens.load(Ordering::Acquire),
        );
        map.insert(
            "output_tokens".into(),
            self.output_tokens.load(Ordering::Acquire),
        );
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anthropic_usage_stats() {
        let llm = AnthropicLlm::new("sk-ant-test", "claude-3-5-sonnet-20241022", None);
        let ctx = LsContext::with_session(lingshu_core::LsId::new());
        let stats = llm.usage_stats(ctx).await.unwrap();
        assert_eq!(stats.get("input_tokens"), Some(&0));
        assert_eq!(stats.get("output_tokens"), Some(&0));
    }

    #[test]
    fn test_message_conversion() {
        let msgs = vec![
            LlmMessage {
                role: LlmRole::User,
                content: "hello".into(),
                content_parts: None,
        name: None,
                tool_calls: None,
            },
            LlmMessage {
                role: LlmRole::Assistant,
                content: "hi there".into(),
                content_parts: None,
        name: None,
                tool_calls: None,
            },
        ];
        let converted = AnthropicLlm::convert_messages(&msgs);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }

    #[test]
    fn test_extract_system() {
        let msgs = vec![
            LlmMessage {
                role: LlmRole::System,
                content: "be helpful".into(),
                content_parts: None,
        name: None,
                tool_calls: None,
            },
            LlmMessage {
                role: LlmRole::User,
                content: "hi".into(),
                content_parts: None,
        name: None,
                tool_calls: None,
            },
        ];
        assert_eq!(
            AnthropicLlm::extract_system(&msgs),
            Some("be helpful".into())
        );

        let no_system = vec![LlmMessage {
            role: LlmRole::User,
            content: "hi".into(),
            content_parts: None,
        name: None,
            tool_calls: None,
        }];
        assert_eq!(AnthropicLlm::extract_system(&no_system), None);
    }

    #[test]
    fn test_default_model() {
        let llm = AnthropicLlm::new("sk-test", "claude-3-haiku-20240307", None);
        assert_eq!(llm.default_model(), "claude-3-haiku-20240307");
    }
}
