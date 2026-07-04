//! Mock LLM — 零依赖模拟引擎，用于开发和测试.
//!
//! 当 API key 未配置时自动启用，确保示例始终可运行.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use lingshu_core::{LsContext, LsResult};
use lingshu_traits::llm::*;
use tokio::sync::mpsc;

/// Mock LLM — 返回回显式回复，不依赖外部 API.
pub struct MockLlm {
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl MockLlm {
    pub fn new() -> Self {
        Self {
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl Llm for MockLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        tracing::info!(model = %request.model, messages = %request.messages.len(), "mock llm invoke");
        let user_msg = request.messages.iter()
            .rev()
            .find(|m| matches!(m.role, LlmRole::User))
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let reply_text = format!(
            "[Mock] 你好！你刚才说的是: 「{0}」\n我是 Lingshu 的 Mock LLM，正在用于开发测试。",
            user_msg
        );
        let reply_len = reply_text.len() as u64;

        self.prompt_tokens.fetch_add(10, Ordering::AcqRel);
        self.completion_tokens.fetch_add(reply_len, Ordering::AcqRel);

        Ok(LlmResponse {
            message: LlmMessage {
                role: LlmRole::Assistant,
                content: reply_text,
                name: None,
                tool_calls: None,
            },
            finish_reason: "stop".into(),
            usage: LlmUsage {
                prompt_tokens: 10,
                completion_tokens: reply_len,
                total_tokens: 10 + reply_len,
            },
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<mpsc::Receiver<LsResult<LlmChunk>>> {
        tracing::info!(model = %request.model, "mock llm stream");
        let user_msg = request.messages.iter()
            .rev()
            .find(|m| matches!(m.role, LlmRole::User))
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let reply_text = format!(
            "[Mock] 你好！你刚才说的是: 「{0}」\n我是 Lingshu 的 Mock LLM，流式响应测试。",
            user_msg
        );

        let (tx, rx) = mpsc::channel(64);
        let reply = reply_text.clone();
        let reply_len = reply_text.len() as u64;

        tokio::spawn(async move {
            for ch in reply.chars() {
                let chunk = LlmChunk {
                    content: Some(ch.to_string()),
                    tool_calls: None,
                    finish_reason: None,
                };
                if tx.send(Ok(chunk)).await.is_err() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let _ = tx.send(Ok(LlmChunk {
                content: None,
                tool_calls: None,
                finish_reason: Some("stop".into()),
            })).await;
        });

        self.prompt_tokens.fetch_add(10, Ordering::AcqRel);
        self.completion_tokens.fetch_add(reply_len, Ordering::AcqRel);

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
    use lingshu_core::LsId;

    #[tokio::test]
    async fn test_mock_usage_stats() {
        let llm = MockLlm::new();
        let ctx = LsContext::with_session(LsId::new());
        let stats = llm.usage_stats(ctx).await.unwrap();
        assert_eq!(stats.get("prompt_tokens"), Some(&0));
        assert_eq!(stats.get("completion_tokens"), Some(&0));
    }

    #[tokio::test]
    async fn test_mock_invoke() {
        let llm = MockLlm::new();
        let ctx = LsContext::with_session(LsId::new());
        let request = LlmRequest {
            model: "mock".into(),
            messages: vec![
                LlmMessage { role: LlmRole::System, content: "test".into(), name: None, tool_calls: None },
                LlmMessage { role: LlmRole::User, content: "hello world".into(), name: None, tool_calls: None },
            ],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: false,
        };
        let resp = llm.invoke(ctx, request).await.unwrap();
        assert!(resp.message.content.contains("hello world"));
        assert_eq!(resp.finish_reason, "stop");
    }

    #[tokio::test]
    async fn test_mock_stream() {
        let llm = MockLlm::new();
        let ctx = LsContext::with_session(LsId::new());
        let request = LlmRequest {
            model: "mock".into(),
            messages: vec![
                LlmMessage { role: LlmRole::User, content: "hello".into(), name: None, tool_calls: None },
            ],
            temperature: None,
            max_tokens: None,
            tools: None,
            stream: true,
        };
        let mut rx = llm.invoke_stream(ctx, request).await.unwrap();
        let mut content = String::new();
        while let Some(result) = rx.recv().await {
            let chunk = match result {
                Ok(c) => c,
                Err(_) => break,
            };
            if let Some(text) = &chunk.content {
                content.push_str(text);
            }
            if chunk.finish_reason.is_some() {
                break;
            }
        }
        assert!(content.contains("hello"));
        assert!(content.contains("[Mock]"));
    }
}
