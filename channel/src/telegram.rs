//! ✈️ Telegram 通道插件 — 原生 Rust 实现.
//!
//! 通过 Telegram Bot API 发送/接收消息.
//! 作为参考实现，展示 MessageChannel trait 的具体用法.

use async_trait::async_trait;
use crate::types::*;
use crate::traits::MessageChannel;
use crate::{LsError, LsResult};

/// Telegram 通道插件.
pub struct TelegramChannel {
    /// Bot Token.
    bot_token: String,
    /// HTTP 客户端.
    client: reqwest::Client,
    /// API 基础 URL.
    api_base: String,
}

impl TelegramChannel {
    /// 创建新的 Telegram 通道.
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            client: reqwest::Client::new(),
            api_base: "https://api.telegram.org".to_string(),
        }
    }

    /// 设置自定义 API 基础 URL (用于代理/自托管).
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 调用 Telegram Bot API.
    async fn call_api<T: serde::Serialize>(
        &self,
        method: &str,
        params: T,
    ) -> LsResult<serde_json::Value> {
        let url = format!(
            "{}/bot{}/{}",
            self.api_base.trim_end_matches('/'),
            self.bot_token,
            method
        );
        let resp = self
            .client
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|e| LsError::Plugin(format!("Telegram API error: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| LsError::Plugin(format!("Telegram response parse: {e}")))?;

        if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null))
        } else {
            let desc = resp
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            Err(LsError::Plugin(format!("Telegram API: {desc}")))
        }
    }
}

#[async_trait]
impl MessageChannel for TelegramChannel {
    fn id(&self) -> &'static str {
        "telegram"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Telegram",
            description: "Telegram 消息平台 — Bot API",
            docs_url: Some("https://core.telegram.org/bots/api"),
            aliases: &["tg", "telegram"],
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct, ChatType::Group, ChatType::Channel],
            supports_media: true,
            supports_polls: true,
            supports_threads: true,
            supports_edit: true,
            supports_reply: true,
        }
    }

    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
        let mut params = serde_json::json!({
            "chat_id": ctx.to,
            "text": ctx.text,
            "disable_notification": ctx.silent,
            "parse_mode": "MarkdownV2",
        });
        if let Some(reply_to) = &ctx.reply_to_id {
            params["reply_to_message_id"] = serde_json::json!(reply_to);
        }
        if let Some(thread) = &ctx.thread_id {
            params["message_thread_id"] = serde_json::json!(thread);
        }

        let result = self.call_api("sendMessage", params).await?;
        Ok(SendReceipt {
            message_id: result["message_id"].as_i64().unwrap_or(0).to_string(),
            thread_id: ctx.thread_id,
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt> {
        let method = if ctx.audio_as_voice { "sendVoice" } else { "sendDocument" };
        // 使用 serde_json::Map 手动构建以支持动态 key
        let mut map = serde_json::Map::new();
        map.insert("chat_id".into(), serde_json::json!(ctx.to));
        let media_key = if ctx.audio_as_voice { "voice" } else { "document" };
        map.insert(media_key.into(), serde_json::json!(ctx.media_url));
        map.insert("disable_notification".into(), serde_json::json!(false));
        if let Some(text) = &ctx.text {
            map.insert("caption".into(), serde_json::json!(text));
        }
        if let Some(reply_to) = &ctx.reply_to_id {
            map.insert("reply_to_message_id".into(), serde_json::json!(reply_to));
        }
        let params = serde_json::Value::Object(map);

        let result = self.call_api(method, params).await?;
        Ok(SendReceipt {
            message_id: result["message_id"].as_i64().unwrap_or(0).to_string(),
            thread_id: ctx.thread_id,
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    async fn handle_inbound(&self, _event: InboundEvent) -> LsResult<()> {
        // Telegram 通过 Webhook / getUpdates 接收消息
        // 此方法将 Telegram Update 转换为 InboundEvent
        // 实际的 Webhook 路由在 server 层处理
        Ok(())
    }

    async fn health_check(&self) -> LsResult<HealthStatus> {
        let start = std::time::Instant::now();
        match self.call_api("getMe", serde_json::json!({})).await {
            Ok(_) => Ok(HealthStatus {
                healthy: true,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
                connected_at: Some(chrono::Utc::now().timestamp()),
            }),
            Err(e) => Ok(HealthStatus {
                healthy: false,
                latency_ms: None,
                error: Some(e.to_string()),
                connected_at: None,
            }),
        }
    }

    fn parse_target(&self, raw: &str) -> LsResult<MessagingTarget> {
        // Telegram: @username, chat_id (数字), channel:@channel
        let (kind, id) = if let Some(username) = raw.strip_prefix('@') {
            // @username 或 @channelusername
            (MessagingTargetKind::User, username.to_string())
        } else if raw.chars().all(|c| c.is_ascii_digit() || c == '-') {
            // 纯数字 → chat_id
            (MessagingTargetKind::Channel, raw.to_string())
        } else {
            return Err(LsError::Plugin(format!("Invalid Telegram target: {raw}")));
        };
        Ok(MessagingTarget {
            normalized: MessagingTarget::normalize(&kind, &id),
            kind,
            id,
            raw: raw.to_string(),
        })
    }
}
