//! Chat completion 端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Chat completion 请求
#[derive(Deserialize)]
pub struct ChatCompletionRequest {
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub stream: Option<bool>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Chat completion 响应
#[derive(Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
}

#[derive(Serialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

/// POST /v1/chat/completions — Chat completion (OpenAI 兼容)
pub async fn chat_completions_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Json<serde_json::Value> {
    let model_name = req.model.clone().unwrap_or_else(|| "claude-3-haiku".into());
    let messages: Vec<String> = req.messages.iter().map(|m| format!("{}: {}", m.role, m.content)).collect();
    let prompt = messages.join("\n");

    // 使用 LLM backend 处理
    match state.runtime.llm.invoke(&prompt, &state.runtime.config.default_llm_options()).await {
        Ok(response) => {
            Json(serde_json::json!({
                "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                "object": "chat.completion",
                "created": chrono::Utc::now().timestamp(),
                "model": model_name,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": response,
                    },
                    "finish_reason": "stop"
                }]
            }))
        }
        Err(e) => Json(serde_json::json!({
            "error": {
                "message": e.to_string(),
                "type": "internal_error"
            }
        })),
    }
}

/// Axum route definition for Chat module
pub fn chat_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/chat/completions", axum::routing::post(chat_completions_handler))
        .route("/v1/chat", axum::routing::post(crate::api::full::chat_handler))
}
