//! 💬 QQ 通道插件 — 原生 Rust 实现.
//!
//! 通过 [QQ 机器人开放平台](https://bot.q.qq.com/) API 发送/接收消息。
//! 支持：私聊、群聊、文本、图片。
//! 接收方式：WebSocket（推荐）或 HTTP Webhook。
//!
//! ## WebSocket 事件监听
//!
//! ```rust,no_run
//! use lingshu_channel::qq::spawn_qq_websocket;
//! spawn_qq_websocket("APP_ID", "BOT_TOKEN", None).await;
//! ```
//!
//! ## 前置条件
//!
//! 1. 在 [QQ 机器人开放平台](https://bot.q.qq.com/) 创建机器人
//! 2. 获取 `AppID` 和 `BotToken`
//! 3. 机器人需被添加到目标群/用户好友
//!
//! ## 参考
//!
//! - [发送消息 API](https://bot.q.qq.com/wiki/develop/api/openapi/message/post_messages.html)
//! - [机器人鉴权](https://bot.q.qq.com/wiki/develop/api/openapi/auth.html)
//! - [WebSocket 事件](https://bot.q.qq.com/wiki/develop/api/gateway/websocket.html)

use async_trait::async_trait;
use crate::types::*;
use crate::traits::MessageChannel;
use crate::{LsError, LsResult};
use std::sync::Arc;
use std::time::Instant;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::RwLock;

// ===========================================================================
// QQ WebSocket 客户端
// ===========================================================================

/// QQBOT WebSocket 网关地址缓存.
const GATEWAY_URL: &str = "wss://api.sgroup.qq.com/websocket";

/// WebSocket 客户端内部状态.
struct WsState {
    /// 事件通道发送端 (用于将解析的 InboundEvent 发送给处理器).
    tx: tokio::sync::mpsc::UnboundedSender<InboundEvent>,
    /// 当前序列号 (用于心跳).
    seq: i64,
    /// 会话 ID (用于恢复).
    session_id: Option<String>,
}

/// 启动 QQ WebSocket 事件监听器（后台任务）。
///
/// 监听 QQ Bot 的实时事件，解析为 `InboundEvent` 并通过 `tx` 发送。
/// 自动处理重连和心跳。
pub async fn spawn_qq_websocket(
    app_id: String,
    bot_token: String,
    tx: tokio::sync::mpsc::UnboundedSender<InboundEvent>,
) -> LsResult<()> {
    let state = Arc::new(RwLock::new(WsState {
        tx,
        seq: 0,
        session_id: None,
    }));

    let auth = format!("Bot {}.{}", app_id, bot_token);
    let ws_url = format!("{}/?app_id={}&bot_token={}", GATEWAY_URL, app_id, bot_token);

    tokio::spawn(async move {
        loop {
            tracing::info!("QQ WebSocket: connecting to {}", GATEWAY_URL);

            match connect_and_listen(&ws_url, &auth, state.clone()).await {
                Ok(()) => {
                    tracing::info!("QQ WebSocket: connection closed normally, reconnecting in 5s");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "QQ WebSocket: connection error, reconnecting in 5s");
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    Ok(())
}

/// 连接并监听 QQ WebSocket.
async fn connect_and_listen(
    ws_url: &str,
    auth: &str,
    state: Arc<RwLock<WsState>>,
) -> LsResult<()> {
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let (ws_stream, _) = connect_async(ws_url)
        .await
        .map_err(|e| LsError::Plugin(format!("QQ WS connect failed: {e}")))?;

    let (mut write, mut read) = ws_stream.split();

    // 等待 Hello 包 (op=10)
    let hello_timeout = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read.next(),
    )
    .await
    .map_err(|_| LsError::Plugin("QQ WS: timeout waiting for Hello".into()))?
    .ok_or_else(|| LsError::Plugin("QQ WS: stream ended before Hello".into()))?
    .map_err(|e| LsError::Plugin(format!("QQ WS read error: {e}")))?;

    let hello_msg = match hello_timeout {
        Message::Text(t) => serde_json::from_str::<serde_json::Value>(&t)
            .map_err(|e| LsError::Plugin(format!("QQ WS: invalid Hello JSON: {e}")))?,
        Message::Binary(b) => serde_json::from_slice::<serde_json::Value>(&b)
            .map_err(|e| LsError::Plugin(format!("QQ WS: invalid Hello binary: {e}")))?,
        Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => {
            return Err(LsError::Plugin("QQ WS: unexpected non-text Hello".into()));
        }
    };

    let op = hello_msg["op"].as_i64().unwrap_or(-1);
    if op != 10 {
        return Err(LsError::Plugin(format!("QQ WS: expected op=10 Hello, got op={op}")));
    }
    let heartbeat_interval_ms = hello_msg["d"]["heartbeat_interval"].as_i64().unwrap_or(30000);

    // 发送 Identify (op=2)
    let identify = serde_json::json!({
        "op": 2,
        "d": {
            "token": auth,
            "intents": 1 << 30,  // 接收所有事件 (public guild messages)
            "shard": [0, 1],
            "properties": {
                "$os": "linux",
                "$device": "lingshu",
                "$browser": "lingshu-agent"
            }
        }
    });

    let identify_text = serde_json::to_string(&identify)
        .map_err(|e| LsError::Plugin(format!("QQ WS: identify serialize: {e}")))?;

    write
        .send(Message::Text(identify_text))
        .await
        .map_err(|e| LsError::Plugin(format!("QQ WS: identify send: {e}")))?;

    tracing::info!("QQ WebSocket: identify sent");

    // 等待 Ready (op=0, t=READY)
    let ready_timeout = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read.next(),
    )
    .await
    .map_err(|_| LsError::Plugin("QQ WS: timeout waiting for Ready".to_string()))?
    .ok_or_else(|| LsError::Plugin("QQ WS: stream ended before Ready".to_string()))?
    .map_err(|e| LsError::Plugin(format!("QQ WS read error: {e}")))?;

    let ready_msg = match ready_timeout {
        Message::Text(t) => serde_json::from_str::<serde_json::Value>(&t)
            .map_err(|e| LsError::Plugin(format!("QQ WS: invalid Ready JSON: {e}")))?,
        Message::Binary(b) => serde_json::from_slice::<serde_json::Value>(&b)
            .map_err(|e| LsError::Plugin(format!("QQ WS: invalid Ready binary: {e}")))?,
        _ => return Err(LsError::Plugin("QQ WS: unexpected non-text Ready".into())),
    };

    let ready_op = ready_msg["op"].as_i64().unwrap_or(-1);
    if ready_op != 0 {
        return Err(LsError::Plugin(format!("QQ WS: expected op=0 Ready, got op={ready_op}")));
    }

    let session_id = ready_msg["d"]["session_id"].as_str().map(|s| s.to_string());
    {
        let mut s = state.write().await;
        s.session_id = session_id;
        s.seq = ready_msg["s"].as_i64().unwrap_or(0);
    }

    tracing::info!("QQ WebSocket: ready, session_id={:?}", state.read().await.session_id);

    // 主循环: 心跳 + 事件读取
    let heartbeat_interval = std::time::Duration::from_millis(heartbeat_interval_ms as u64);

    loop {
        tokio::select! {
            // 心跳定时器
            _ = tokio::time::sleep_until(
                tokio::time::Instant::now() + heartbeat_interval
            ) => {
                let seq = {
                    let s = state.read().await;
                    s.seq
                };
                let heartbeat = serde_json::json!({
                    "op": 1,
                    "d": seq
                });
                let hb_text = serde_json::to_string(&heartbeat)
                    .unwrap_or_else(|_| r#"{"op":1,"d":null}"#.into());
                if let Err(e) = write.send(Message::Text(hb_text)).await {
                    tracing::warn!(error = %e, "QQ WS: heartbeat send failed");
                    return Err(LsError::Plugin(format!("Heartbeat send: {e}")));
                }
            }

            // 读取事件
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        let payload: serde_json::Value = match serde_json::from_str(&t) {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!(error = %e, raw = %t, "QQ WS: invalid JSON");
                                continue;
                            }
                        };

                        let op = payload["op"].as_i64().unwrap_or(-1);

                        match op {
                            0 => {
                                // Dispatch (事件分发)
                                let seq = payload["s"].as_i64().unwrap_or(0);
                                {
                                    let mut s = state.write().await;
                                    s.seq = seq;
                                }

                                if let Err(e) = handle_qq_ws_event(&payload, &state).await {
                                    tracing::warn!(error = %e, "QQ WS: event handling failed");
                                }
                            }
                            7 => {
                                // Reconnect
                                tracing::warn!("QQ WS: server requested reconnect");
                                return Ok(());
                            }
                            9 => {
                                // Invalid Session
                                tracing::error!("QQ WS: invalid session, reconnecting...");
                                return Ok(());
                            }
                            11 => {
                                // Heartbeat ACK
                                tracing::debug!("QQ WS: heartbeat ack");
                            }
                            _ => {
                                tracing::debug!(op = op, "QQ WS: unknown op code");
                            }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        let text = String::from_utf8_lossy(&b);
                        tracing::warn!("QQ WS: received binary message (unsupported): {}", text);
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // Send pong
                        if let Err(e) = write.send(Message::Pong(vec![])).await {
                            tracing::warn!(error = %e, "QQ WS: pong send failed");
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Ignore
                    }
                    Some(Ok(Message::Close(frame))) => {
                        tracing::info!("QQ WS: connection closed: {:?}", frame);
                        return Ok(());
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(e)) => {
                        tracing::warn!(error = %e, "QQ WS: read error");
                        return Err(LsError::Plugin(format!("WS read: {e}")));
                    }
                    None => {
                        tracing::info!("QQ WS: stream ended");
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// 处理 QQ WebSocket 分发的单个事件.
async fn handle_qq_ws_event(
    payload: &serde_json::Value,
    state: &Arc<RwLock<WsState>>,
) -> LsResult<()> {
    let event_type = payload["t"].as_str().unwrap_or("UNKNOWN");

    match event_type {
        "MESSAGE_CREATE" | "DIRECT_MESSAGE_CREATE" => {
            let d = &payload["d"];
            let is_group = event_type == "MESSAGE_CREATE";
            let author_id = d["author"]["id"].as_str().map(|s| s.to_string());
            let content = d["content"].as_str().map(|s| s.to_string());
            let msg_id = d["id"].as_str().map(|s| s.to_string());
            let channel_id = d["channel_id"].as_str().map(|s| s.to_string());

            // QQ timestamp 格式: "2024-01-01T00:00:00+08:00"
            let ts = d["timestamp"].as_str()
                .and_then(|t| {
                    chrono::DateTime::parse_from_rfc3339(t)
                        .ok()
                        .map(|dt| dt.timestamp())
                })
                .unwrap_or_else(|| chrono::Utc::now().timestamp());

            let event = InboundEvent {
                channel_id: "qq".into(),
                message_id: msg_id,
                sender_id: author_id.clone(),
                sender_name: d["member"]["nick"].as_str().map(|s| s.to_string())
                    .or_else(|| d["author"]["username"].as_str().map(|s| s.to_string())),
                chat_type: if is_group { ChatType::Group } else { ChatType::Direct },
                chat_id: if is_group { channel_id } else { author_id.clone() },
                text: content,
                media_urls: vec![],
                reply_to_id: d["referenced_message"]["id"].as_str().map(|s| s.to_string()),
                timestamp: ts,
                raw: Some(payload.clone()),
            };

            let tx = { state.read().await.tx.clone() };
            if let Err(e) = tx.send(event) {
                tracing::warn!(error = %e, "QQ WS: failed to forward event");
            }
        }
        "GUILD_MESSAGES" | "C2C_MESSAGE_CREATE" => {
            // QQ 频道/私聊消息处理
            let d = &payload["d"];
            let event = InboundEvent {
                channel_id: "qq".into(),
                message_id: d["id"].as_str().map(|s| s.to_string()),
                sender_id: d["author"]["user_openid"].as_str().map(|s| s.to_string())
                    .or_else(|| d["author"]["id"].as_str().map(|s| s.to_string())),
                sender_name: d["author"]["member"]["nick"].as_str().map(|s| s.to_string()),
                chat_type: ChatType::Direct,
                chat_id: d["author"]["user_openid"].as_str().map(|s| s.to_string()),
                text: d["content"].as_str().map(|s| s.to_string()),
                media_urls: vec![],
                reply_to_id: None,
                timestamp: chrono::Utc::now().timestamp(),
                raw: Some(payload.clone()),
            };

            let tx = { state.read().await.tx.clone() };
            if let Err(e) = tx.send(event) {
                tracing::warn!(error = %e, "QQ WS: failed to forward event");
            }
        }
        "AT_MESSAGE_CREATE" => {
            // @机器人消息 (频道)
            let d = &payload["d"];
            let event = InboundEvent {
                channel_id: "qq".into(),
                message_id: d["id"].as_str().map(|s| s.to_string()),
                sender_id: d["author"]["id"].as_str().map(|s| s.to_string()),
                sender_name: d["author"]["username"].as_str().map(|s| s.to_string()),
                chat_type: ChatType::Group,
                chat_id: d["channel_id"].as_str().map(|s| s.to_string()),
                text: d["content"].as_str().map(|s| s.to_string()),
                media_urls: vec![],
                reply_to_id: None,
                timestamp: chrono::Utc::now().timestamp(),
                raw: Some(payload.clone()),
            };

            let tx = { state.read().await.tx.clone() };
            if let Err(e) = tx.send(event) {
                tracing::warn!(error = %e, "QQ WS: failed to forward event");
            }
        }
        _ => {
            tracing::debug!(event_type = %event_type, "QQ WS: unhandled event type");
        }
    }

    Ok(())
}

// ===========================================================================
// QQ 通道插件
// ===========================================================================

/// QQ 通道插件.
///
/// 通过 QQ 官方机器人平台 API 发送消息。
pub struct QqChannel {
    app_id: String,
    bot_token: String,
    client: reqwest::Client,
    api_base: String,
    /// 事件发送端 (WebSocket 监听器注入).
    event_tx: RwLock<Option<tokio::sync::mpsc::UnboundedSender<InboundEvent>>>,
    /// 事件接收端 (用于 handle_inbound 对外暴露).
    event_rx: RwLock<Option<tokio::sync::mpsc::UnboundedReceiver<InboundEvent>>>,
}

impl QqChannel {
    /// 创建新的 QQ 通道.
    pub fn new(app_id: impl Into<String>, bot_token: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            bot_token: bot_token.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_base: "https://api.sgroup.qq.com".to_string(),
            event_tx: RwLock::new(None),
            event_rx: RwLock::new(None),
        }
    }

    /// 设置自定义 API 基础 URL.
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 初始化 WebSocket 事件监听并返回事件接收端.
    pub async fn start_websocket(self: &Arc<Self>) -> LsResult<()> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<InboundEvent>();
        {
            let mut et = self.event_tx.write().await;
            *et = Some(tx.clone());
        }
        {
            let mut er = self.event_rx.write().await;
            *er = Some(rx);
        }

        let app_id = self.app_id.clone();
        let bot_token = self.bot_token.clone();
        spawn_qq_websocket(app_id, bot_token, tx).await
    }

    fn auth_header(&self) -> String {
        format!("Bot {}.{}", self.app_id, self.bot_token)
    }

    async fn call_api(&self, method: &str, params: serde_json::Value) -> LsResult<serde_json::Value> {
        let url = format!("{}{}", self.api_base.trim_end_matches('/'), method);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&params)
            .send()
            .await
            .map_err(|e| LsError::Plugin(format!("QQ API 请求失败: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| LsError::Plugin(format!("QQ API 响应解析失败: {e}")))?;

        if let Some(code) = resp.get("code").and_then(|v| v.as_i64()) {
            if code != 0 {
                let msg = resp.get("message").and_then(|v| v.as_str()).unwrap_or("unknown");
                return Err(LsError::Plugin(format!("QQ API 错误 [{code}]: {msg}")));
            }
        }
        Ok(resp)
    }

    async fn send_to_user(&self, openid: &str, content: &str, msg_type: u32, reply_to: Option<&str>) -> LsResult<SendReceipt> {
        let path = format!("/v2/users/{openid}/messages");
        let mut body = serde_json::json!({"content": content, "msg_type": msg_type});
        if let Some(msg_id) = reply_to {
            body["msg_id"] = serde_json::json!(msg_id);
        }
        let result = self.call_api(&path, body).await?;
        Ok(SendReceipt {
            message_id: result["id"].as_str().unwrap_or("").to_string(),
            thread_id: None,
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    async fn send_to_group(&self, group_openid: &str, content: &str, msg_type: u32, reply_to: Option<&str>) -> LsResult<SendReceipt> {
        let path = format!("/v2/groups/{group_openid}/messages");
        let mut body = serde_json::json!({"content": content, "msg_type": msg_type});
        if let Some(msg_id) = reply_to {
            body["msg_id"] = serde_json::json!(msg_id);
        }
        let result = self.call_api(&path, body).await?;
        Ok(SendReceipt {
            message_id: result["id"].as_str().unwrap_or("").to_string(),
            thread_id: Some(group_openid.to_string()),
            timestamp: chrono::Utc::now().timestamp(),
            raw: Some(result),
        })
    }

    fn is_group_target(id: &str) -> bool {
        id.starts_with("AO_") || id.starts_with("group_") || id.contains("_g")
    }
}

#[async_trait]
impl MessageChannel for QqChannel {
    fn id(&self) -> &'static str { "qq" }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "QQ",
            description: "QQ 消息平台 — 官方机器人 API",
            docs_url: Some("https://bot.q.qq.com/wiki/develop/api/openapi/message/post_messages.html"),
            aliases: &["qq", "qbot", "QQ"],
        }
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct, ChatType::Group],
            supports_media: true,
            supports_polls: false,
            supports_threads: false,
            supports_edit: false,
            supports_reply: true,
        }
    }

    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
        if Self::is_group_target(&ctx.to) {
            self.send_to_group(&ctx.to, &ctx.text, 0, ctx.reply_to_id.as_deref()).await
        } else {
            self.send_to_user(&ctx.to, &ctx.text, 0, ctx.reply_to_id.as_deref()).await
        }
    }

    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt> {
        let text = format!("📎 {} {}", ctx.media_url, ctx.text.as_deref().unwrap_or(""));
        if Self::is_group_target(&ctx.to) {
            self.send_to_group(&ctx.to, &text, 0, ctx.reply_to_id.as_deref()).await
        } else {
            self.send_to_user(&ctx.to, &text, 0, ctx.reply_to_id.as_deref()).await
        }
    }

    async fn send_payload(&self, ctx: SendPayloadContext) -> LsResult<SendReceipt> {
        let payload = &ctx.payload;
        let text = if payload.is_error.unwrap_or(false) {
            format!("❌ {}", payload.text.as_deref().unwrap_or("未知错误"))
        } else if let Some(media_urls) = &payload.media_urls {
            if !media_urls.is_empty() {
                let links = media_urls.iter().enumerate()
                    .map(|(i, u)| format!("📎 [{i}]({u})"))
                    .collect::<Vec<_>>().join("\n");
                match &payload.text {
                    Some(t) if !t.is_empty() => format!("{t}\n{links}"),
                    _ => links,
                }
            } else {
                payload.text.clone().unwrap_or_default()
            }
        } else {
            payload.text.clone().unwrap_or_default()
        };

        if Self::is_group_target(&ctx.to) {
            self.send_to_group(&ctx.to, &text, 0, ctx.reply_to_id.as_deref()).await
        } else {
            self.send_to_user(&ctx.to, &text, 0, ctx.reply_to_id.as_deref()).await
        }
    }

    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<()> {
        // 如果 raw 字段有数据，尝试从 raw 解析
        if let Some(raw) = &event.raw {
            // 支持 webhook 和 WebSocket 两种格式
            if let Some(op) = raw.get("op").and_then(|v| v.as_i64()) {
                // WebSocket 格式 (有 op 字段)
                if op == 0 {
                    let _ = handle_qq_ws_event(raw, &Arc::new(RwLock::new(WsState {
                        tx: self.event_tx.read().await.clone().unwrap_or_else(|| {
                            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
                            tx
                        }),
                        seq: 0,
                        session_id: None,
                    }))).await;
                    return Ok(());
                }
            }
        }

        // 直接传入 InboundEvent 的情况 (已解析)
        tracing::info!(
            channel = "qq",
            message_id = ?event.message_id,
            sender = ?event.sender_id,
            text = ?event.text,
            "QQ message received"
        );

        // TODO: 将 inbound 事件推送到 Agent 消息队列
        Ok(())
    }

    async fn health_check(&self) -> LsResult<HealthStatus> {
        let start = Instant::now();
        match self.client.get(format!("{}/me", self.api_base.trim_end_matches('/')))
            .header("Authorization", self.auth_header())
            .send().await
        {
            Ok(resp) if resp.status().is_success() => Ok(HealthStatus {
                healthy: true,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
                connected_at: Some(chrono::Utc::now().timestamp()),
            }),
            Ok(resp) => Ok(HealthStatus {
                healthy: false,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("HTTP {}", resp.status())),
                connected_at: None,
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
        let (kind, id) = if raw.starts_with("ou_") {
            (MessagingTargetKind::User, raw.to_string())
        } else if raw.starts_with("AO_") || raw.starts_with("group_") {
            (MessagingTargetKind::Channel, raw.to_string())
        } else if let Some(username) = raw.strip_prefix('@') {
            (MessagingTargetKind::User, username.to_string())
        } else {
            (MessagingTargetKind::User, raw.to_string())
        };
        Ok(MessagingTarget {
            normalized: MessagingTarget::normalize(&kind, &id),
            kind, id, raw: raw.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_group_target() {
        assert!(!QqChannel::is_group_target("ou_xxxx123"));
        assert!(QqChannel::is_group_target("AO_xxxx123"));
        assert!(QqChannel::is_group_target("group_xxxx"));
    }

    #[test]
    fn test_parse_target_user() {
        let ch = QqChannel::new("appid", "token");
        let target = ch.parse_target("ou_12345").unwrap();
        assert_eq!(target.kind, MessagingTargetKind::User);
        assert_eq!(target.id, "ou_12345");
    }
}
