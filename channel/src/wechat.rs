//! 💬 微信 (WeChat Official Account) 通道插件 — 原生 Rust 实现.
//!
//! 通过微信公众号平台 API 发送/接收消息。
//! 支持：文本消息、图片消息、关注/取消关注事件。
//! 接收方式：HTTP Webhook（公众号后台配置服务器 URL）。
//!
//! ## 前置条件
//!
//! 1. 在 [微信公众平台](https://mp.weixin.qq.com) 注册服务号
//! 2. 获取 `AppID` 和 `AppSecret`
//! 3. 在"开发 → 基本配置"中设置服务器 URL
//! 4. 在"开发 → 基本配置"中设置 Token（用于验证签名）
//!
//! ## 流程
//!
//! ```text
//! 微信服务器 ──GET(POST)──► Lingshu Webhook ──parse──► Agent
//!     ▲                                                    │
//!     └───────────────── XML/JSON ◄────────────────────────┘
//! ```
//!
//! ## 参考
//!
//! - [接入指南](https://developers.weixin.qq.com/doc/offiaccount/Basic_Information/Access_Overview.html)
//! - [消息管理](https://developers.weixin.qq.com/doc/offiaccount/Message_Management/)
//! - [发送客服消息](https://developers.weixin.qq.com/doc/offiaccount/Message_Management/Service_Center_messages.html)

use crate::traits::MessageChannel;
use crate::types::*;
use crate::{LsError, LsResult};
use async_trait::async_trait;
use sha1::{Digest, Sha1};
use std::time::Instant;

// ── 微信通道 ───────────────────────────────────────

/// 微信公众平台通道插件.
pub struct WeChatChannel {
    /// 公众号 AppID.
    app_id: String,
    /// 公众号 AppSecret.
    app_secret: String,
    /// 服务器配置 Token（用于验证签名）.
    token: String,
    /// HTTP 客户端.
    client: reqwest::Client,
    /// API 基础 URL.
    api_base: String,
    /// 缓存 access_token.
    token_cache: tokio::sync::RwLock<TokenCache>,
    /// 通道启动时间.
    started_at: Instant,
}

struct TokenCache {
    token: Option<String>,
    expires_at: i64,
}

impl WeChatChannel {
    /// 创建新的微信通道.
    pub fn new(
        app_id: impl Into<String>,
        app_secret: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_secret: app_secret.into(),
            token: token.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_base: "https://api.weixin.qq.com".to_string(),
            token_cache: tokio::sync::RwLock::new(TokenCache {
                token: None,
                expires_at: 0,
            }),
            started_at: Instant::now(),
        }
    }

    /// 验证服务器签名（用于微信服务器 URL 配置验证）.
    ///
    /// 微信服务器会发送 GET 请求，包含 `signature`、`timestamp`、`nonce`、`echostr`。
    /// 需要将 token、timestamp、nonce 排序后 SHA1 加密，与 signature 比对。
    pub fn verify_signature(&self, signature: &str, timestamp: &str, nonce: &str) -> bool {
        let mut parts = [self.token.as_str(), timestamp, nonce];
        parts.sort_unstable();
        let joined = parts.concat();
        let hash = {
            let mut hasher = Sha1::new();
            hasher.update(joined.as_bytes());
            hasher.finalize()
        };
        let hex_hash = hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        hex_hash == signature
    }

    /// 获取 access_token（自动缓存/刷新）.
    async fn get_access_token(&self) -> LsResult<String> {
        {
            let cache = self.token_cache.read().await;
            let now = chrono::Utc::now().timestamp();
            if let Some(token) = &cache.token {
                if now < cache.expires_at - 60 {
                    return Ok(token.clone());
                }
            }
        }

        // 刷新 token
        let url = format!(
            "{}/cgi-bin/token?grant_type=client_credential&appid={}&secret={}",
            self.api_base, self.app_id, self.app_secret
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("wechat token request failed: {e}")))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LsError::Internal(format!("wechat token parse failed: {e}")))?;

        let access_token = body["access_token"]
            .as_str()
            .ok_or_else(|| LsError::Internal(format!("wechat token error: {:?}", body)))?
            .to_string();

        let expires_in = body["expires_in"].as_i64().unwrap_or(7200);

        let mut cache = self.token_cache.write().await;
        cache.token = Some(access_token.clone());
        cache.expires_at = chrono::Utc::now().timestamp() + expires_in;

        Ok(access_token)
    }

    /// 解析微信传入的 XML 消息。
    #[allow(dead_code)]
    fn strip_cdata(s: &str) -> String {
        if s.starts_with("<![CDATA[") && s.ends_with("]]>") {
            s[9..s.len() - 3].to_string()
        } else {
            s.to_string()
        }
    }

    fn parse_wechat_xml(xml: &str) -> LsResult<InboundEvent> {
        // 简易 XML 解析 — 提取关键字段
        let extract = |tag: &str| -> Option<String> {
            let open = format!("<{}>", tag);
            let close = format!("</{}>", tag);
            xml.find(&open).and_then(|start| {
                let content_start = start + open.len();
                xml[content_start..].find(&close).map(|end| {
                    let raw = &xml[content_start..content_start + end];
                    if raw.starts_with("<![CDATA[") && raw.ends_with("]]>") {
                        raw[9..raw.len() - 3].to_string()
                    } else {
                        raw.to_string()
                    }
                })
            })
        };

        let msg_type = extract("MsgType").unwrap_or_default();
        let content = extract("Content");
        let from_user = extract("FromUserName");
        let msg_id = extract("MsgId");
        let create_time_str = extract("CreateTime").unwrap_or_default();
        let create_time: i64 = create_time_str
            .parse()
            .unwrap_or_else(|_| chrono::Utc::now().timestamp());

        Ok(InboundEvent {
            channel_id: "wechat".into(),
            message_id: msg_id,
            sender_id: from_user.clone(),
            sender_name: from_user,
            chat_type: ChatType::Direct,
            chat_id: extract("ToUserName"),
            text: content,
            media_urls: vec![],
            reply_to_id: None,
            timestamp: create_time,
            raw: Some(serde_json::json!({
                "raw_xml": xml,
                "msg_type": msg_type,
            })),
        })
    }

    /// 构建 XML 文本回复.
    fn build_text_reply(to: &str, from: &str, content: &str) -> String {
        let timestamp = chrono::Utc::now().timestamp();
        format!(
            r#"<xml>
<ToUserName><![CDATA[{}]]></ToUserName>
<FromUserName><![CDATA[{}]]></FromUserName>
<CreateTime>{}</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[{}]]></Content>
</xml>"#,
            to, from, timestamp, content
        )
    }
}

#[async_trait]
impl MessageChannel for WeChatChannel {
    fn id(&self) -> &'static str {
        "wechat"
    }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "微信",
            description: "微信公众号消息通道",
            docs_url: Some("https://developers.weixin.qq.com/doc/offiaccount/"),
            aliases: &["wechat", "weixin", "wx"],
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct],
            supports_media: true,
            supports_polls: false,
            supports_threads: false,
            supports_edit: false,
            supports_reply: true,
        }
    }

    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
        let token = self.get_access_token().await?;
        let url = format!(
            "{}/cgi-bin/message/custom/send?access_token={}",
            self.api_base, token
        );

        let body = serde_json::json!({
            "touser": ctx.to,
            "msgtype": "text",
            "text": {
                "content": ctx.text,
            }
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("wechat send failed: {e}")))?;

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LsError::Internal(format!("wechat send parse failed: {e}")))?;

        if result.get("errcode").and_then(|c| c.as_i64()) != Some(0) {
            return Err(LsError::Internal(format!("wechat API error: {:?}", result)));
        }

        Ok(SendReceipt {
            message_id: result
                .get("msgid")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string(),
            thread_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt> {
        let token = self.get_access_token().await?;
        let url = format!(
            "{}/cgi-bin/message/custom/send?access_token={}",
            self.api_base, token
        );

        let body = serde_json::json!({
            "touser": ctx.to,
            "msgtype": "image",
            "image": {
                "media_id": ctx.media_url,
            }
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("wechat send media failed: {e}")))?;

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LsError::Internal(format!("wechat send media parse failed: {e}")))?;

        if result.get("errcode").and_then(|c| c.as_i64()) != Some(0) {
            return Err(LsError::Internal(format!(
                "wechat media API error: {:?}",
                result
            )));
        }

        Ok(SendReceipt {
            message_id: result
                .get("msgid")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string(),
            thread_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<()> {
        // 微信入站消息处理 — 由 Webhook handler 解析后调用
        tracing::info!(
            "wechat inbound: from={:?}, text={:?}",
            event.sender_id,
            event.text.as_deref().unwrap_or("(non-text)")
        );
        Ok(())
    }

    async fn health_check(&self) -> LsResult<HealthStatus> {
        match self.get_access_token().await {
            Ok(_) => Ok(HealthStatus {
                healthy: true,
                latency_ms: Some(self.started_at.elapsed().as_millis() as u64),
                error: None,
                connected_at: Some(self.started_at.elapsed().as_secs() as i64),
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
        // 微信用户 OpenID
        Ok(MessagingTarget {
            kind: MessagingTargetKind::User,
            id: raw.to_string(),
            raw: raw.to_string(),
            normalized: MessagingTarget::normalize(&MessagingTargetKind::User, raw),
        })
    }
}

// ── 微信 Webhook 处理 ──────────────────────────────

/// 微信服务器验证处理.
///
/// 用于 GET 请求，验证服务器地址有效性。
/// 微信服务器向配置的 URL 发送 GET 请求，携带 `signature`、`timestamp`、`nonce`、`echostr`。
/// 验证成功则返回 `echostr`。
pub fn handle_verification(
    channel: &WeChatChannel,
    signature: &str,
    timestamp: &str,
    nonce: &str,
    echostr: &str,
) -> Result<String, String> {
    if channel.verify_signature(signature, timestamp, nonce) {
        Ok(echostr.to_string())
    } else {
        Err("signature verification failed".into())
    }
}

/// 处理微信消息（POST 请求）.
///
/// 微信服务器发送 POST 请求，body 为 XML 格式。
/// 返回 XML 格式的回复。
pub async fn handle_message(_channel: &WeChatChannel, xml_body: &str) -> Result<String, String> {
    let event =
        WeChatChannel::parse_wechat_xml(xml_body).map_err(|e| format!("parse failed: {e}"))?;

    let from = event.sender_id.clone().unwrap_or_default();
    let to = event.chat_id.clone().unwrap_or_default();
    let text = event.text.clone().unwrap_or_default();

    // 简单自动回复 — 实际使用中应转发给 Agent
    let reply = if text.is_empty() {
        "收到消息！".to_string()
    } else {
        format!("已收到：{}", text)
    };

    Ok(WeChatChannel::build_text_reply(&from, &to, &reply))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_verification() {
        let channel = WeChatChannel::new("appid", "secret", "test_token");
        // 模拟微信签名验证
        let mut parts = ["test_token", "1234567890", "nonce123"];
        parts.sort_unstable();
        let joined = parts.concat();
        let mut hasher = Sha1::new();
        hasher.update(joined.as_bytes());
        let hash = hasher.finalize();
        let signature = hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        assert!(channel.verify_signature(&signature, "1234567890", "nonce123"));
        assert!(!channel.verify_signature(&signature, "wrong", "nonce123"));
    }

    #[test]
    fn test_parse_xml() {
        let xml = r#"<xml>
<ToUserName><![CDATA[toUser]]></ToUserName>
<FromUserName><![CDATA[fromUser]]></FromUserName>
<CreateTime>1348831860</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[你好]]></Content>
<MsgId>1234567890123456</MsgId>
</xml>"#;

        let event = WeChatChannel::parse_wechat_xml(xml).unwrap();
        assert_eq!(event.sender_id.as_deref(), Some("fromUser"));
        assert_eq!(event.text.as_deref(), Some("你好"));
        assert_eq!(event.message_id.as_deref(), Some("1234567890123456"));
    }

    #[test]
    fn test_build_text_reply() {
        let reply = WeChatChannel::build_text_reply("fromUser", "toUser", "hello");
        assert!(reply.contains("fromUser"));
        assert!(reply.contains("toUser"));
        assert!(reply.contains("hello"));
        assert!(reply.contains("<xml>"));
        assert!(reply.contains("</xml>"));
    }
}
