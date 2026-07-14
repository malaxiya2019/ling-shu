//! Chat completion 端点
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Chat completion 请求
#[derive(Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

/// POST /v1/chat/completions — Chat completion (OpenAI 兼容)
#[allow(dead_code)]
pub async fn chat_completions_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Json<serde_json::Value> {
    let model_name = req.model.clone().unwrap_or_else(|| "claude-3-haiku".into());
    let messages: Vec<String> = req
        .messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect();
    let prompt = messages.join("\n");

    // 使用 LLM backend 处理
    if let Some(llm) = state.runtime.llm.as_ref() {
        use lingshu_traits::llm::{LlmMessage, LlmRequest, LlmRole};
        let llm_req = LlmRequest {
            model: model_name.clone(),
            messages: vec![LlmMessage {
                role: LlmRole::User,
                content: prompt,
                content_parts: None,
                name: None,
                tool_calls: None,
            }],
            temperature: req.temperature.map(|t| t as f64),
            max_tokens: req.max_tokens,
            tools: None,
            stream: req.stream.unwrap_or(false),
        };
        match llm.invoke(state.runtime.root_ctx.clone(), llm_req).await {
            Ok(response) => {
                return Json(serde_json::json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                    "object": "chat.completion",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model_name,
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": response.message.content,
                        },
                        "finish_reason": "stop"
                    }]
                }));
            }
            Err(e) => {
                return Json(serde_json::json!({
                    "error": {
                        "message": e.to_string(),
                        "type": "internal_error"
                    }
                }));
            }
        }
    }

    // 无 LLM 后端时的降级响应
    Json(serde_json::json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model_name,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "No LLM backend configured",
            },
            "finish_reason": "stop"
        }]
    }))
}

/// Axum route definition for Chat module
#[allow(dead_code)]
pub fn chat_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/v1/chat/completions",
            axum::routing::post(chat_completions_handler),
        )
        .route(
            "/v1/chat",
            axum::routing::post(crate::api::full::chat_handler),
        )
}
