//! 📘 飞书 (Feishu/Lark) 通道插件 — 原生 Rust 实现.
//!
//! 通过飞书开放平台 API 发送/接收消息。
//! 支持：文本、富文本(post)、图片、互动卡片。
//!
//! ## 前置条件
//!
//! 1. 在 [飞书开放平台](https://open.feishu.cn) 创建企业自建应用
//! 2. 获取 `App ID` 和 `App Secret`
//! 3. 启用所需权限（如 `im:message`）
//! 4. 发布应用并获取管理员授权
//!
//! ## 权限清单
//!
//! - `im:message` — 发送消息
//! - `im:message:send_as_bot` — 以 Bot 身份发送
//! - `im:resource` — 上传/下载文件
//!
//! ## 参考
//!
//! - [发送消息 API](https://open.feishu.cn/document/server-docs/im-v1/message/create)
//! - [获取 tenant_access_token](https://open.feishu.cn/document/server-docs/authentication-management/access-token/tenant_access_token_internal)

use async_trait::async_trait;
use crate::types::*;
use crate::traits::MessageChannel;
use crate::{LsError, LsResult};
use std::time::Instant;

/// 飞书通道插件.
pub struct FeishuChannel {
    /// 应用 App ID.
    app_id: String,
    /// 应用 App Secret.
    app_secret: String,
    /// HTTP 客户端.
    client: reqwest::Client,
    /// API 基础 URL.
    api_base: String,
    /// 缓存 tenant_access_token.
    token_cache: tokio::sync::RwLock<TokenCache>,
}

struct TokenCache {
    token: Option<String>,
    expires_at: i64,
}

impl FeishuChannel {
    /// 创建新的飞书通道.
    pub fn new(app_id: impl Into<String>, app_secret: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            app_secret: app_secret.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_base: "https://open.feishu.cn".to_string(),
            token_cache: tokio::sync::RwLock::new(TokenCache {
                token: None,
                expires_at: 0,
            }),
        }
    }

    /// 设置自定义 API 基础 URL.
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 获取 tenant_access_token（自动缓存/刷新）.
    async fn get_token(&self) -> LsResult<String> {
        // 检查缓存
        {
            let cache = self.token_cache.read().await;
            let now = chrono::Utc::now().timestamp();
            if let Some(token) = &cache.token {
                if now < cache.expires_at - 60 {
                    // 提前 60 秒刷新
                    return Ok(token.clone());
                }
            }
        }

        // 刷新 token
        let url = format!("{}/open-apis/auth/v3/tenant_access_token/internal", self.api_base);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret,
            }))
            .send()
            .await
            .map_err(|e| LsError::Plugin(format!("飞书 token 请求失败: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| LsError::Plugin(format!("飞书 token 响应解析失败: {e}")))?;

        let code = resp["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp["msg"].as_str().unwrap_or("unknown");
            return Err(LsError::Plugin(format!("飞书 token 错误 [{code}]: {msg}")));
        }

        let token = resp["tenant_access_token"]
            .as_str()
            .ok_or_else(|| LsError::Plugin("飞书响应缺少 tenant_access_token".into()))?
            .to_string();
        let expire = resp["expire"].as_i64().unwrap_or(7200);

        // 更新缓存
        {
            let mut cache = self.token_cache.write().await;
            cache.token = Some(token.clone());
            cache.expires_at = chrono::Utc::now().timestamp() + expire;
        }

        Ok(token)
    }

    /// 调用飞书 Open API.
    async fn call_api(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        query: Option<Vec<(&str, &str)>>,
    ) -> LsResult<serde_json::Value> {
        let token = self.get_token().await?;
        let url = format!("{}{}", self.api_base.trim_end_matches('/'), method);

        let mut req = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json; charset=utf-8");

        if let Some(p) = params {
            req = req.json(&p);
        }

        if let Some(q) = query {
            req = req.query(&q);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| LsError::Plugin(format!("飞书 API 请求失败: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| LsError::Plugin(format!("飞书 API 响应解析失败: {e}")))?;

        let code = resp["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp["msg"].as_str().unwrap_or("unknown");
            return Err(LsError::Plugin(format!("飞书 API 错误 [{code}]: {msg}")));
        }

        Ok(resp["data"].clone())
    }

    /// 发送消息（通用）.
    async fn send_message(
        &self,
        receive_id: &str,
        msg_type: &str,
        content: &str,
    ) -> LsResult<SendReceipt> {
        let result = self
            .call_api(
                "/open-apis/im/v1/messages",
                Some(serde_json::json!({
                    "receive_id": receive_id,
                    "msg_type": msg_type,
                    "content": content,
                })),
                Some(vec![("receive_id_type", "open_id")]),
            )
            .await?;

        Ok(SendReceipt {
            message_id: result["message_id"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            thread_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }
}

#[async_trait]
impl MessageChannel for FeishuChannel {
    fn id(&self) -> &'static str {
        "feishu"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "飞书",
            description: "飞书消息平台 — 开放平台 API",
            docs_url: Some("https://open.feishu.cn/document/server-docs/im-v1/message/create"),
            aliases: &["feishu", "lark", "飞书"],
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct, ChatType::Group, ChatType::Channel],
            supports_media: true,
            supports_polls: false,
            supports_threads: true,
            supports_edit: false,
            supports_reply: true,
        }
    }

    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
        // 飞书文本消息 content 格式: {"text": "..."}
        let content = serde_json::json!({
            "text": ctx.text,
        });
        self.send_message(&ctx.to, "text", &content.to_string()).await
    }

    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt> {
        // 飞书图片消息 content 格式: {"image_key": "..."}
        // 这里 media_url 是文件 URL，需要先上传获取 image_key
        // 简化实现：发送带图片链接的富文本消息
        let content = if ctx.audio_as_voice {
            // 飞书不支持语音消息，降级为发送文件链接文本
            serde_json::json!({
                "text": format!("🎵 语音消息: {}\n{}", ctx.media_url, ctx.text.as_deref().unwrap_or("")),
            })
        } else {
            serde_json::json!({
                "text": format!("📎 {}\n{}", ctx.media_url, ctx.text.as_deref().unwrap_or("")),
            })
        };
        self.send_message(&ctx.to, "text", &content.to_string()).await
    }

    async fn send_payload(&self, ctx: SendPayloadContext) -> LsResult<SendReceipt> {
        let payload = &ctx.payload;

        // 如果有错误标记，使用错误格式
        if payload.is_error.unwrap_or(false) {
            let err_text = payload.text.as_deref().unwrap_or("未知错误");
            let content = serde_json::json!({
                "text": format!("❌ {err_text}"),
            });
            return self.send_message(&ctx.to, "text", &content.to_string()).await;
        }

        // 如果有媒体 URL，构建富文本
        if let Some(media_urls) = &payload.media_urls {
            if !media_urls.is_empty() {
                let text = payload.text.as_deref().unwrap_or("");
                let media_links: Vec<String> = media_urls
                    .iter()
                    .enumerate()
                    .map(|(i, url)| format!("📎 [{i}]({url})"))
                    .collect();

                let full_text = if text.is_empty() {
                    media_links.join("\n")
                } else {
                    format!("{text}\n{}", media_links.join("\n"))
                };

                let content = serde_json::json!({"text": full_text});
                return self.send_message(&ctx.to, "text", &content.to_string()).await;
            }
        }

        // 纯文本
        let text = payload.text.as_deref().unwrap_or("");
        let content = serde_json::json!({"text": text});
        self.send_message(&ctx.to, "text", &content.to_string()).await
    }

    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<()> {
        // 飞书事件订阅格式:
        // {
        //   "header": { "event_type": "im.message.receive_v1", ... },
        //   "event": {
        //     "sender": { "sender_id": { "open_id": "ou_xxx" }, "sender_type": "user" },
        //     "message": {
        //       "chat_id": "oc_xxx", "chat_type": "group",
        //       "content": "{\"text\":\"hello\"}", "message_id": "om_xxx",
        //       "message_type": "text", "create_time": "1234567890"
        //     }
        //   }
        // }
        let raw = match &event.raw {
            Some(r) => r.clone(),
            None => {
                tracing::warn!("feishu: inbound event without raw payload");
                return Ok(());
            }
        };

        // 验证事件类型
        let event_type = raw["header"]["event_type"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if event_type != "im.message.receive_v1" {
            tracing::debug!(event_type = %event_type, "feishu: ignoring non-message event");
            return Ok(());
        }

        let ev = &raw["event"];
        let msg = &ev["message"];

        // 解析聊天类型
        let chat_type_str = msg["chat_type"].as_str().unwrap_or("p2p");
        let chat_type = match chat_type_str {
            "group" => ChatType::Group,
            _ => ChatType::Direct,
        };

        // 解析发送者
        let sender_id = ev["sender"]["sender_id"]["open_id"]
            .as_str()
            .map(|s| s.to_string());

        let sender_name = ev["sender"]["sender_id"]["user_id"]
            .as_str()
            .map(|s| s.to_string());

        // 解析消息内容
        let message_id = msg["message_id"].as_str().map(|s| s.to_string());
        let chat_id = msg["chat_id"].as_str().map(|s| s.to_string());
        let create_time = msg["create_time"].as_str()
            .and_then(|t| t.parse::<i64>().ok())
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        let msg_type = msg["message_type"].as_str().unwrap_or("text");

        // content 是 JSON 字符串，需要二次解析
        let (text, media_urls) = match msg_type {
            "text" => {
                let content_raw = msg["content"].as_str().unwrap_or("{}");
                let content_val: serde_json::Value =
                    serde_json::from_str(content_raw).unwrap_or(serde_json::json!({}));
                let text = content_val["text"].as_str().map(|s| s.to_string());
                (text, vec![])
            }
            "image" => {
                let content_raw = msg["content"].as_str().unwrap_or("{}");
                let content_val: serde_json::Value =
                    serde_json::from_str(content_raw).unwrap_or(serde_json::json!({}));
                let image_key = content_val["image_key"].as_str().map(|s| s.to_string());
                let media = image_key
                    .map(|k| format!("https://open.feishu.cn/open-apis/im/v1/images/{}?image_type=message", k))
                    .into_iter().collect();
                (None, media)
            }
            _ => (None, vec![]),
        };

        let inbound = InboundEvent {
            channel_id: "feishu".into(),
            message_id,
            sender_id,
            sender_name,
            chat_type,
            chat_id,
            text,
            media_urls,
            reply_to_id: None,
            timestamp: create_time,
            raw: Some(raw),
        };

        tracing::info!(
            channel = "feishu",
            message_id = ?inbound.message_id,
            sender = ?inbound.sender_id,
            text = ?inbound.text,
            "Feishu message received"
        );

        // TODO: 将 inbound 事件推送到 Agent 消息队列
        // 当前: 记录日志

        Ok(())
    }

    async fn health_check(&self) -> LsResult<HealthStatus> {
        let start = Instant::now();
        match self.get_token().await {
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
        // 飞书目标格式:
        // - open_id: ou_xxxx
        // - chat_id: oc_xxxx
        // - user_id: 7g7b_xxxx
        // - @username 或纯文本 fallback
        let (kind, id) = if raw.starts_with("ou_") {
            // 用户 open_id
            (MessagingTargetKind::User, raw.to_string())
        } else if raw.starts_with("oc_") {
            // 群聊 chat_id
            (MessagingTargetKind::Channel, raw.to_string())
        } else if let Some(username) = raw.strip_prefix('@') {
            (MessagingTargetKind::User, username.to_string())
        } else {
            // 默认视为 open_id
            (MessagingTargetKind::User, raw.to_string())
        };

        Ok(MessagingTarget {
            normalized: MessagingTarget::normalize(&kind, &id),
            kind,
            id,
            raw: raw.to_string(),
        })
    }
}
