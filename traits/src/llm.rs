use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// LLM 消息角色.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmRole {
    System,
    User,
    Assistant,
    Tool,
}

/// LLM 消息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<Value>>,
}

/// LLM 请求配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<Value>>,
    pub stream: bool,
}

/// LLM 响应.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub message: LlmMessage,
    pub finish_reason: String,
    pub usage: LlmUsage,
}

/// LLM 用量统计.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// LLM 流式块.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmChunk {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<Value>>,
    pub finish_reason: Option<String>,
}

/// Llm — 模型调用、流式输出、结构化返回、用量统计.
#[async_trait]
pub trait Llm: Send + Sync + 'static {
    /// 发起非流式调用.
    async fn invoke(&self, ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse>;

    /// 发起流式调用.
    async fn invoke_stream(
        &self,
        ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>>;

    /// 查询用量统计.
    async fn usage_stats(&self, ctx: LsContext) -> LsResult<HashMap<String, u64>>;
}

// ── Blanket impl: Box<dyn Llm> 也实现 Llm ──────────

#[async_trait]
impl<T: Llm + ?Sized> Llm for Box<T> {
    async fn invoke(&self, ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        (**self).invoke(ctx, request).await
    }

    async fn invoke_stream(
        &self,
        ctx: LsContext,
        request: LlmRequest,
    ) -> LsResult<tokio::sync::mpsc::Receiver<LsResult<LlmChunk>>> {
        (**self).invoke_stream(ctx, request).await
    }

    async fn usage_stats(&self, ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        (**self).usage_stats(ctx).await
    }
}
