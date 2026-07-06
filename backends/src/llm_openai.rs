//! OpenAI LLM 后端实现 — 对接 Chat Completions API.
//!
//! 支持:
//! - 非流式调用 (invoke)
//! - 流式调用 (invoke_stream)
//! - 用量统计
//! - 多模态 (image_url)

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsResult};
use lingshu_traits::llm::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<lingshu_traits::llm::ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    content: serde_json::Value, // String | Vec<ContentPart>
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
#[allow(dead_code)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiToolCallFunction,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
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
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

/// OpenAI 流式 tool_call delta.
#[derive(Deserialize, Clone)]
#[allow(dead_code)]
struct StreamToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<StreamFunctionDelta>,
}

#[derive(Deserialize, Clone)]
struct StreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// ── OpenAI API 内容格式 ─────────────────────────────

/// OpenAI API 内容部件 (用于多模态).
#[derive(Serialize)]
#[serde(untagged)]
enum OpenAiContentPart {
    Text {
        #[serde(rename = "type")]
        part_type: String,
        text: String,
    },
    ImageUrl {
        #[serde(rename = "type")]
        part_type: String,
        image_url: OpenAiImageUrl,
    },
}

#[derive(Serialize)]
struct OpenAiImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
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
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
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

    /// 将 LlmMessage 转换为 OpenAI ChatMessage (支持多模态).
    fn convert_message(msg: &LlmMessage) -> ChatMessage {
        // 如果有 content_parts，构造为数组
        if let Some(parts) = &msg.content_parts {
            let openai_parts: Vec<OpenAiContentPart> = parts
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text, .. } => OpenAiContentPart::Text {
                        part_type: "text".into(),
                        text: text.clone(),
                    },
                    ContentPart::ImageUrl { image_url, .. } => OpenAiContentPart::ImageUrl {
                        part_type: "image_url".into(),
                        image_url: OpenAiImageUrl {
                            url: image_url.url.clone(),
                            detail: image_url.detail.clone(),
                        },
                    },
                })
                .collect();

            ChatMessage {
                role: match msg.role {
                    LlmRole::System => "system".into(),
                    LlmRole::User => "user".into(),
                    LlmRole::Assistant => "assistant".into(),
                    LlmRole::Tool => "tool".into(),
                },
                name: msg.name.clone(),
                tool_call_id: None,
                content: serde_json::to_value(openai_parts).unwrap_or_default(),
            }
        } else {
            // 传统纯文本模式
            ChatMessage {
                role: match msg.role {
                    LlmRole::System => "system".into(),
                    LlmRole::User => "user".into(),
                    LlmRole::Assistant => "assistant".into(),
                    LlmRole::Tool => "tool".into(),
                },
                name: msg.name.clone(),
                tool_call_id: None,
                content: serde_json::Value::String(msg.content.clone()),
            }
        }
    }

    fn convert_messages(msgs: &[LlmMessage]) -> Vec<ChatMessage> {
        msgs.iter().map(Self::convert_message).collect()
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
            tools: request.tools.clone(),
            tool_choice: None,
        };

        let resp = self
            .client
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
            .map_err(|e| LsError::Llm(format!("parse response failed: {e}")))?;

        let choice = chat_resp.choices.into_iter().next().ok_or_else(|| {
            LsError::Llm("no choices in response".into())
        })?;

        let usage = chat_resp.usage.unwrap_or(UsageData {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        // 转换 tool_calls
        let tool_calls = choice.message.tool_calls.map(|calls| {
            calls
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    call_type: tc.call_type,
                    function: ToolCallFunction {
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                    },
                })
                .collect()
        });

        self.prompt_tokens
            .fetch_add(usage.prompt_tokens, Ordering::Release);
        self.completion_tokens
            .fetch_add(usage.completion_tokens, Ordering::Release);

        Ok(LlmResponse {
            message: LlmMessage {
                role: LlmRole::Assistant,
                content: choice.message.content.unwrap_or_default(),
                content_parts: None,
                name: None,
                tool_calls,
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
            tools: request.tools.clone(),
            tool_choice: None,
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
                    let _ = tx
                        .send(Err(LsError::Llm(format!("stream request failed: {e}"))))
                        .await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx
                    .send(Err(LsError::Llm(format!("API error {status}: {body}"))))
                    .await;
                return;
            }

            let mut stream = response.bytes_stream();
            use futures_util::StreamExt;
            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(Err(LsError::Llm(format!("stream read error: {e}"))))
                            .await;
                        break;
                    }
                };

                // SSE 解析: 行以 "data: " 开头
                let text = String::from_utf8_lossy(&chunk);
                for line in text.lines() {
                    let line = line.trim();
                    if !line.starts_with("data: ") {
                        continue;
                    }
                    let data = &line[6..];
                    if data == "[DONE]" {
                        break;
                    }

                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(sc) => {
                            for choice in sc.choices {
                                // Parse streaming tool calls into LlmChunk.tool_calls
                                let tool_calls = choice.delta.tool_calls.map(|deltas| {
                                    deltas.into_iter().map(|tc| {
                                        lingshu_traits::llm::ToolCall {
                                            id: tc.id.unwrap_or_default(),
                                            call_type: tc.call_type.unwrap_or_else(|| "function".into()),
                                            function: lingshu_traits::llm::ToolCallFunction {
                                                name: tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default(),
                                                arguments: tc.function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default(),
                                            },
                                        }
                                    }).collect()
                                });

                                let _ = tx
                                    .send(Ok(LlmChunk {
                                        content: choice.delta.content,
                                        tool_calls,
                                        finish_reason: choice.finish_reason,
                                    }))
                                    .await;
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(Err(LsError::Llm(format!("stream parse: {e}"))))
                                .await;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut map = HashMap::new();
        map.insert(
            "prompt_tokens".into(),
            self.prompt_tokens.load(Ordering::Acquire),
        );
        map.insert(
            "completion_tokens".into(),
            self.completion_tokens.load(Ordering::Acquire),
        );
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
    fn test_message_conversion_text_only() {
        let msg = LlmMessage {
            role: LlmRole::User,
            content: "hi".into(),
            content_parts: None,
            name: None,
            tool_calls: None,
        };
        let converted = OpenAiLlm::convert_message(&msg);
        assert_eq!(converted.role, "user");
        assert_eq!(converted.content, serde_json::Value::String("hi".into()));
    }

    #[test]
    fn test_message_conversion_multimodal() {
        let msg = LlmMessage {
            role: LlmRole::User,
            content: "".into(),
            content_parts: Some(vec![
                ContentPart::text("What's in this image?"),
                ContentPart::image_url(ImageUrl::new("https://example.com/img.png")),
            ]),
            name: None,
            tool_calls: None,
        };
        let converted = OpenAiLlm::convert_message(&msg);
        assert_eq!(converted.role, "user");
        assert!(converted.content.is_array());
        let arr = converted.content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "What's in this image?");
        assert_eq!(arr[1]["type"], "image_url");
        assert_eq!(arr[1]["image_url"]["url"], "https://example.com/img.png");
    }

    #[test]
    fn test_message_conversion_system() {
        let msg = LlmMessage {
            role: LlmRole::System,
            content: "be helpful".into(),
            content_parts: None,
            name: None,
            tool_calls: None,
        };
        let converted = OpenAiLlm::convert_message(&msg);
        assert_eq!(converted.role, "system");
        assert_eq!(converted.content, serde_json::Value::String("be helpful".into()));
    }
}
