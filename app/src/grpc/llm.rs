//! LLM gRPC 服务 — 代理 chat/embed 请求到 Lingshu LLM 后端.

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::info;

use lingshu_traits::llm::{LlmMessage, LlmRequest, LlmRole};
use tokio_stream::wrappers::ReceiverStream;

use crate::LingshuRuntime;

use proto::llm_service_server::LLMService;
use proto::{ChatChunk, ChatRequest, ChatResponse, Choice, EmbedRequest, EmbedResponse,
            Embedding, Message, Usage};

pub struct LLMServiceImpl {
    runtime: Arc<LingshuRuntime>,
}

impl LLMServiceImpl {
    pub fn new(runtime: Arc<LingshuRuntime>) -> Self {
        Self { runtime }
    }
}

#[tonic::async_trait]
impl LLMService for LLMServiceImpl {
    async fn chat(&self, req: Request<ChatRequest>) -> Result<Response<ChatResponse>, Status> {
        let inner = req.into_inner();
        info!(model = %inner.model, "gRPC chat");

        let llm = self.runtime.llm.as_ref()
            .ok_or_else(|| Status::unavailable("no LLM backend available"))?;

        let messages: Vec<LlmMessage> = inner.messages.into_iter()
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
            .collect();

        let llm_req = LlmRequest {
            model: llm.default_model().to_string(),
            messages,
            temperature: inner.temperature,
            max_tokens: if inner.max_tokens > 0 { Some(inner.max_tokens as usize) } else { None },
            stream: false,
            tools: None,
        };

        let response = llm.chat(&llm_req).await
            .map_err(|e| Status::internal(format!("LLM chat failed: {e}")))?;

        Ok(Response::new(ChatResponse {
            id: "chat-grpc-1".into(),
            model: inner.model,
            choices: vec![Choice {
                index: 0,
                message: Some(Message {
                    role: "assistant".into(),
                    content: response.content,
                }),
                finish_reason: "stop".into(),
            }],
            usage: Some(Usage {
                prompt_tokens: response.usage.prompt_tokens as i32,
                completion_tokens: response.usage.completion_tokens as i32,
                total_tokens: response.usage.total_tokens as i32,
            }),
        }))
    }

    async fn chat_stream(&self, _req: Request<ChatRequest>) -> Result<Response<Self::ChatStreamStream>, Status> {
        Err(Status::unimplemented("streaming chat not yet implemented in gRPC"))
    }

    async fn embed(&self, req: Request<EmbedRequest>) -> Result<Response<EmbedResponse>, Status> {
        let inner = req.into_inner();
        info!(model = %inner.model, "gRPC embed");

        let llm = self.runtime.llm.as_ref()
            .ok_or_else(|| Status::unavailable("no LLM backend available"))?;

        let mut embeddings = Vec::new();
        for (i, text) in inner.input.iter().enumerate() {
            let result = llm.embed(text).await
                .map_err(|e| Status::internal(format!("embed failed: {e}")))?;
            embeddings.push(Embedding {
                index: i as i32,
                vector: result.vector.iter().map(|&v| v as f64).collect(),
            });
        }

        Ok(Response::new(EmbedResponse {
            model: inner.model,
            embeddings,
            usage: Some(Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }),
        }))
    }
}
