use async_trait::async_trait;
use lingshu_core::{LsContext, LsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

/// LLM 消息角色.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 纯文本内容 (backward compatible)
    pub content: String,
    /// 多模态内容部件 (图像/音频等)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<Vec<ContentPart>>,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// 多模态内容部件.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentPart {
    Text {
        #[serde(rename = "type")]
        part_type: ContentPartType,
        text: String,
    },
    ImageUrl {
        #[serde(rename = "type")]
        part_type: ContentPartType,
        image_url: ImageUrl,
    },
}

/// 内容部件类型标记.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentPartType {
    Text,
    #[serde(rename = "image_url")]
    ImageUrl,
}

impl fmt::Display for ContentPartType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentPartType::Text => write!(f, "text"),
            ContentPartType::ImageUrl => write!(f, "image_url"),
        }
    }
}

/// 图像 URL 或 Base64 数据.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    /// URL 或 data:image/...;base64,... 格式
    pub url: String,
    /// 图像细节 (auto/low/high)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ImageUrl {
    /// 从 URL 创建.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            detail: None,
        }
    }

    /// 从 Base64 编码的图像数据创建.
    pub fn from_base64(mime_type: &str, base64_data: &str) -> Self {
        Self {
            url: format!("data:{};base64,{}", mime_type, base64_data),
            detail: None,
        }
    }

    /// 设置图像细节.
    pub fn with_detail(mut self, detail: &str) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
}

impl ContentPart {
    /// 创建文本部件.
    pub fn text(text: impl Into<String>) -> Self {
        ContentPart::Text {
            part_type: ContentPartType::Text,
            text: text.into(),
        }
    }

    /// 创建图像 URL 部件.
    pub fn image_url(image_url: ImageUrl) -> Self {
        ContentPart::ImageUrl {
            part_type: ContentPartType::ImageUrl,
            image_url,
        }
    }
}

/// LLM 请求配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<ToolDefinition>>,
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
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
}

/// OpenAI-compatible tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

/// Function definition within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Tool call from LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

/// Function call details from LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Tool result to send back to LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
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
