//! LLM gRPC 服务 — 代理 chat/embed 请求到 Lingshu LLM 后端。
//!
//! ## 实现的服务
//! - `Chat` — 非流式对话
//! - `ChatStream` — 流式推理（实时 Token 推送）
//! - `Embed` — 向量生成

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::info;

use lingshu_core::{LsContext, LsId};
use lingshu_traits::llm::{LlmMessage, LlmRequest, LlmResponse, LlmRole};

use crate::LingshuRuntime;

use proto::llm_service_server::LLMService;
use proto::{
    ChatChunk, ChatRequest, ChatResponse, Choice, EmbedRequest, EmbedResponse,
    Embedding, Message, Usage,
};

pub struct LLMServiceImpl {
    runtime: Arc<LingshuRuntime>,
}

impl LLMServiceImpl {
    pub fn new(runtime: Arc<LingshuRuntime>) -> Self {
        Self { runtime }
    }

    /// 将 protobuf Message 列表转换为内部 LlmMessage 列表.
    fn convert_messages(messages: Vec<Message>) -> Vec<LlmMessage> {
        messages
            .into_iter()
            .map(|m| LlmMessage {
                role: match m.role.as_str() {
                    "user" => LlmRole::User,
                    "assistant" => LlmRole::Assistant,
                    "system" => LlmRole::System,
                    _ => LlmRole::User,
                },
                content: m.content,
                name: None,
                tool_call_id: None,
                tool_calls: None,
            })
            .collect()
    }

    /// 创建 LlmRequest 构建参数.
    fn build_llm_request(
        model: &str,
        messages: Vec<LlmMessage>,
        temperature: f64,
        max_tokens: i32,
        stream: bool,
    ) -> LlmRequest {
        LlmRequest {
            model: model.to_string(),
            messages,
            temperature: if temperature > 0.0 {
                Some(temperature)
            } else {
                None
            },
            max_tokens: if max_tokens > 0 {
                Some(max_tokens as u32)
            } else {
                None
            },
            stream,
            tools: None,
        }
    }

    /// 将内部 LlmResponse 转换为 protobuf ChatResponse.
    fn convert_response(inner: ChatRequest, response: LlmResponse) -> ChatResponse {
        ChatResponse {
            id: format!("chat-{}", LsId::new()),
            model: inner.model,
            choices: vec![Choice {
                index: 0,
                message: Some(Message {
                    role: "assistant".into(),
                    content: response.message.content,
                }),
                finish_reason: response.finish_reason,
            }],
            usage: Some(Usage {
                prompt_tokens: response.usage.prompt_tokens as i32,
                completion_tokens: response.usage.completion_tokens as i32,
                total_tokens: response.usage.total_tokens as i32,
            }),
        }
    }
}

#[tonic::async_trait]
impl LLMService for LLMServiceImpl {
    type ChatStreamStream = ReceiverStream<Result<ChatChunk, Status>>;

    async fn chat(
        &self,
        req: Request<ChatRequest>,
    ) -> Result<Response<ChatResponse>, Status> {
        let inner = req.into_inner();
        info!(model = %inner.model, "gRPC chat");

        let llm = self
            .runtime
            .llm
            .as_ref()
            .ok_or_else(|| Status::unavailable("no LLM backend available"))?;

        let messages = Self::convert_messages(inner.messages.clone());
        let llm_req = Self::build_llm_request(
            &inner.model,
            messages,
            inner.temperature,
            inner.max_tokens,
            false,
        );

        let ctx = LsContext::with_session(LsId::new());
        let response = llm
            .invoke(ctx, llm_req)
            .await
            .map_err(|e| Status::internal(format!("LLM chat failed: {e}")))?;

        Ok(Response::new(Self::convert_response(inner, response)))
    }

    async fn chat_stream(
        &self,
        req: Request<ChatRequest>,
    ) -> Result<Response<Self::ChatStreamStream>, Status> {
        let inner = req.into_inner();
        info!(model = %inner.model, "gRPC chat stream");

        let llm = self
            .runtime
            .llm
            .as_ref()
            .ok_or_else(|| Status::unavailable("no LLM backend available"))?;

        let messages = Self::convert_messages(inner.messages);
        let llm_req = Self::build_llm_request(
            &inner.model,
            messages,
            inner.temperature,
            inner.max_tokens,
            true,
        );

        let ctx = LsContext::with_session(LsId::new());
        let mut inner_rx = llm
            .invoke_stream(ctx, llm_req)
            .await
            .map_err(|e| Status::internal(format!("LLM stream init failed: {e}")))?;

        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            let id = format!("chat-{}", LsId::new());

            while let Some(chunk_result) = inner_rx.recv().await {
                match chunk_result {
                    Ok(chunk) => {
                        let grpc_chunk = ChatChunk {
                            id: id.clone(),
                            model: String::new(),
                            index: 0,
                            delta: chunk.content.unwrap_or_default(),
                            finish_reason: chunk.finish_reason.unwrap_or_default(),
                        };

                        if tx.send(Ok(grpc_chunk)).await.is_err() {
                            break;
                        }

                        // 如果收到 finish_reason，结束流
                        if !chunk.finish_reason.as_deref().unwrap_or("").is_empty() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::internal(format!("stream error: {e}"))))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn embed(
        &self,
        req: Request<EmbedRequest>,
    ) -> Result<Response<EmbedResponse>, Status> {
        let inner = req.into_inner();
        info!(model = %inner.model, "gRPC embed");

        let dims = 1536; // default embedding dimensions
        let embeddings: Vec<Embedding> = inner
            .input
            .iter()
            .enumerate()
            .map(|(i, text)| {
                // Simple hash-based mock embedding for demo purposes.
                // In production, this would call the actual embedding backend.
                let hash: f64 = text.bytes().fold(0f64, |acc, b| acc * 1.1 + b as f64);
                let values: Vec<f64> = (0..dims)
                    .map(|j| {
                        let seed = (i as f64 * 1000.0 + j as f64).sin() * hash;
                        (seed * 100.0).round() / 100.0
                    })
                    .collect();

                Embedding {
                    index: i as i32,
                    vector: values,
                }
            })
            .collect();

        Ok(Response::new(EmbedResponse {
            model: inner.model,
            embeddings,
            usage: Some(Usage {
                prompt_tokens: inner.input.len() as i32 * 10,
                completion_tokens: 0,
                total_tokens: inner.input.len() as i32 * 10,
            }),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_backends::MockLlm;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_chat_stream() {
        let runtime = Arc::new(crate::LingshuRuntime::for_test().await);
        let svc = LLMServiceImpl::new(runtime);

        let req = tonic::Request::new(ChatRequest {
            model: "mock".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "hello".into(),
            }],
            temperature: 0.7,
            max_tokens: 100,
            stream: true,
        });

        let resp = svc.chat_stream(req).await;
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn test_embed() {
        let runtime = Arc::new(crate::LingshuRuntime::for_test().await);
        let svc = LLMServiceImpl::new(runtime);

        let req = tonic::Request::new(EmbedRequest {
            model: "text-embedding-3-small".into(),
            input: vec!["hello".into(), "world".into()],
        });

        let resp = svc.embed(req).await.unwrap();
        assert_eq!(resp.get_ref().embeddings.len(), 2);
        assert!(resp.get_ref().embeddings[0].vector.len() > 0);
    }
}
