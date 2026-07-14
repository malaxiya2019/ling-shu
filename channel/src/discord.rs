//! 🎮 Discord 通道插件 — 原生 Rust 实现.
//!
//! 通过 Discord Bot API (REST) 发送/接收消息.
//!
//! # Feature
//! `discord` (需显式启用)
//!
//! # 环境变量
//! - `DISCORD_BOT_TOKEN` — Discord Bot Token
//!
//! # 参考
//! - [Discord Developer Portal — Create Message](https://discord.com/developers/docs/resources/channel#create-message)
//! - [Discord API Reference](https://discord.com/developers/docs/intro)

use crate::traits::MessageChannel;
use crate::types::*;
use crate::{LsError, LsResult};
use async_trait::async_trait;
use std::time::Instant;

/// Discord API 基础 URL.
const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

/// Discord 通道插件.
pub struct DiscordChannel {
    /// Bot Token.
    bot_token: String,
    /// HTTP 客户端.
    client: reqwest::Client,
    /// API 基础 URL (可自定义, 用于自托管/代理).
    api_base: String,
}

impl DiscordChannel {
    /// 创建新的 Discord 通道.
    ///
    /// # 环境变量
    /// 从 `DISCORD_BOT_TOKEN` 读取 Bot Token.
    pub fn new() -> LsResult<Self> {
        let bot_token = std::env::var("DISCORD_BOT_TOKEN")
            .map_err(|_| LsError::Config("DISCORD_BOT_TOKEN 未设置".into()))?;
        Ok(Self {
            bot_token,
            client: reqwest::Client::new(),
            api_base: DISCORD_API_BASE.to_string(),
        })
    }

    /// 使用指定 Token 创建 Discord 通道.
    pub fn with_token(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            client: reqwest::Client::new(),
            api_base: DISCORD_API_BASE.to_string(),
        }
    }

    /// 设置自定义 API 基础 URL (用于代理/自托管).
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 发送 Discord REST API 请求 (JSON body).
    async fn call_api(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> LsResult<serde_json::Value> {
        let url = format!("{}{}", self.api_base.trim_end_matches('/'), path);

        let mut req = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .header("Content-Type", "application/json");

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| LsError::Plugin(format!("Discord API 请求失败: {e}")))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LsError::Plugin(format!("Discord API 响应解析失败: {e}")))?;

        if status.is_success() {
            Ok(body)
        } else {
            let msg = body["message"].as_str().unwrap_or("unknown error");
            let code = body["code"].as_i64().unwrap_or(0);
            Err(LsError::Plugin(format!(
                "Discord API 错误 [{}] (code {}): {}",
                status.as_u16(),
                code,
                msg
            )))
        }
    }

    /// 发送消息到 Discord 频道.
    async fn send_message(
        &self,
        channel_id: &str,
        content: &str,
        tts: bool,
        reply_to_id: Option<&str>,
    ) -> LsResult<SendReceipt> {
        let mut payload = serde_json::json!({
            "content": content,
            "tts": tts,
        });

        if let Some(reply_id) = reply_to_id {
            payload["message_reference"] = serde_json::json!({
                "message_id": reply_id,
            });
        }

        let path = format!("/channels/{}/messages", channel_id);
        let result = self
            .call_api(reqwest::Method::POST, &path, Some(payload))
            .await?;

        Ok(SendReceipt {
            message_id: result["id"].as_str().unwrap_or("").to_string(),
            thread_id: result
                .get("thread_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }
}

#[async_trait]
impl MessageChannel for DiscordChannel {
    fn id(&self) -> &'static str {
        "discord"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Discord",
            description: "Discord 消息平台 — Bot API",
            docs_url: Some("https://discord.com/developers/docs/intro"),
            aliases: &["discord", "dc", "ds"],
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct, ChatType::Group, ChatType::Channel],
            supports_media: true,
            supports_polls: false,
            supports_threads: true,
            supports_edit: true,
            supports_reply: true,
        }
    }

    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
        self.send_message(&ctx.to, &ctx.text, false, ctx.reply_to_id.as_deref())
            .await
    }

    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt> {
        // Discord 不支持直接通过 REST API 发送远程 URL 为文件附件.
        // 发送带媒体链接的文本消息作为降级方案, 同时尝试用 embed 形式展示.
        let content = if let Some(text) = &ctx.text {
            format!("{}\n{}", text, ctx.media_url)
        } else {
            ctx.media_url.clone()
        };

        let path = format!("/channels/{}/messages", ctx.to);
        let mut payload = serde_json::json!({
            "content": content,
            "tts": false,
        });

        // 添加 embed 以获得更好的预览
        payload["embeds"] = serde_json::json!([{
            "url": ctx.media_url,
        }]);

        if let Some(reply_id) = &ctx.reply_to_id {
            payload["message_reference"] = serde_json::json!({
                "message_id": reply_id,
            });
        }

        let result = self
            .call_api(reqwest::Method::POST, &path, Some(payload))
            .await?;

        Ok(SendReceipt {
            message_id: result["id"].as_str().unwrap_or("").to_string(),
            thread_id: result
                .get("thread_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<()> {
        // Discord 通过 Interaction (Slash Command / Modal) 或 Webhook 接收事件.
        // 实际的 Webhook 路由在 server 层处理.
        // 此处解析标准化后的 InboundEvent 并记录日志.
        let raw = match &event.raw {
            Some(r) => r,
            None => {
                tracing::warn!("discord: inbound event without raw payload");
                return Ok(());
            }
        };

        // 尝试解析 Interaction 类型
        let interaction_type = raw["type"].as_i64().unwrap_or(0);
        let interaction_name = raw["data"]["name"].as_str().unwrap_or("");

        tracing::info!(
            channel = "discord",
            interaction_type = %interaction_type,
            interaction_name = %interaction_name,
            message_id = ?event.message_id,
            sender = ?event.sender_id,
            text = ?event.text,
            "Discord interaction received"
        );

        // TODO: 将 inbound 事件推送到 Agent 消息队列
        // 当前: 记录日志

        Ok(())
    }

    async fn health_check(&self) -> LsResult<HealthStatus> {
        let start = Instant::now();
        match self
            .call_api(reqwest::Method::GET, "/users/@me", None)
            .await
        {
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
        // Discord 目标格式:
        // - discord://channel/{channel_id}
        // - discord://user/{user_id}
        // - {channel_id} (纯数字)
        // - <#{channel_id}> (Discord 频道提及格式)

        let (kind, id) = if let Some(rest) = raw.strip_prefix("discord://") {
            // URI 格式: discord://channel/xxx 或 discord://user/xxx
            if let Some(channel_id) = rest.strip_prefix("channel/") {
                (MessagingTargetKind::Channel, channel_id.to_string())
            } else if let Some(user_id) = rest.strip_prefix("user/") {
                (MessagingTargetKind::User, user_id.to_string())
            } else {
                // 默认视为频道
                (MessagingTargetKind::Channel, rest.to_string())
            }
        } else if let Some(inner) = raw.strip_prefix("<#").and_then(|s| s.strip_suffix('>')) {
            // <#123456789> 频道提及格式
            (MessagingTargetKind::Channel, inner.to_string())
        } else if raw.chars().all(|c| c.is_ascii_digit()) {
            // 纯数字 → Snowflake ID (可能是用户或频道)
            (MessagingTargetKind::Channel, raw.to_string())
        } else {
            return Err(LsError::Plugin(format!("Invalid Discord target: {raw}")));
        };

        Ok(MessagingTarget {
            normalized: MessagingTarget::normalize(&kind, &id),
            kind,
            id,
            raw: raw.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_target_discord_uri() {
        let channel = DiscordChannel::with_token("test_token");
        let target = channel.parse_target("discord://channel/123456789").unwrap();
        assert_eq!(target.kind, MessagingTargetKind::Channel);
        assert_eq!(target.id, "123456789");
        assert_eq!(target.normalized, "channel:123456789");
    }

    #[test]
    fn test_parse_target_discord_user_uri() {
        let channel = DiscordChannel::with_token("test_token");
        let target = channel.parse_target("discord://user/987654321").unwrap();
        assert_eq!(target.kind, MessagingTargetKind::User);
        assert_eq!(target.id, "987654321");
    }

    #[test]
    fn test_parse_target_mention() {
        let channel = DiscordChannel::with_token("test_token");
        let target = channel.parse_target("<#123456789>").unwrap();
        assert_eq!(target.kind, MessagingTargetKind::Channel);
        assert_eq!(target.id, "123456789");
    }

    #[test]
    fn test_parse_target_numeric() {
        let channel = DiscordChannel::with_token("test_token");
        let target = channel.parse_target("123456789").unwrap();
        assert_eq!(target.kind, MessagingTargetKind::Channel);
        assert_eq!(target.id, "123456789");
    }

    #[test]
    fn test_parse_target_invalid() {
        let channel = DiscordChannel::with_token("test_token");
        let result = channel.parse_target("invalid-format");
        assert!(result.is_err());
    }
}
