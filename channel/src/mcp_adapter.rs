//! 🌉 MCP 通道适配器 — 通过 MCP 协议桥接外部消息通道.
//!
//! 核心设计: lingshu Agent 通过 MCP Client 连接一个「通道网关」MCP 服务器，
//! 该服务器实际对接 WhatsApp/WeChat/Telegram 等平台。
//! 对应 OpenClaw 的 Gateway 架构。

use async_trait::async_trait;
use std::sync::Arc;
use crate::types::*;
use crate::traits::MessageChannel;
use crate::{LsError, LsResult};

/// MCP 通道适配器 — 将 MCP 服务器的 tools 暴露为 MessageChannel.
///
/// 连接到一个运行中的「通道网关」MCP 服务器，
/// 该服务器应暴露以下工具:
///
/// - `channel_send_text` — 发送文本
/// - `channel_send_media` — 发送媒体
/// - `channel_send_payload` — 发送富文本
/// - `channel_health` — 健康检查
pub struct McpChannelAdapter {
    /// 通道标识.
    id: &'static str,
    /// 通道元数据.
    meta: ChannelMeta,
    /// 能力声明.
    capabilities: ChannelCapabilities,
    /// MCP 客户端池 (共享).
    mcp_pool: Arc<lingshu_mcp::rmcp_client::McpClientPool>,
    /// MCP 服务器名称.
    server_name: String,
}

impl McpChannelAdapter {
    /// 创建新的 MCP 通道适配器.
    ///
    /// `server_name` 是 MCP 客户端池中注册的服务器名称.
    pub fn new(
        id: &'static str,
        meta: ChannelMeta,
        capabilities: ChannelCapabilities,
        mcp_pool: Arc<lingshu_mcp::rmcp_client::McpClientPool>,
        server_name: impl Into<String>,
    ) -> Self {
        Self {
            id,
            meta,
            capabilities,
            mcp_pool,
            server_name: server_name.into(),
        }
    }

    /// 调用 MCP 工具的辅助方法.
    async fn call_tool(&self, tool: &str, args: serde_json::Value) -> LsResult<serde_json::Value> {
        let result = self
            .mcp_pool
            .call_tool(&self.server_name, tool, args)
            .await?;
        // 从 MCP 结果中提取 text 内容
        for content in &result.content {
            if let lingshu_mcp::rmcp_client::McpContent::Text { text } = content {
                return serde_json::from_str(text)
                    .map_err(|e| LsError::Plugin(format!("MCP result parse failed: {e}")));
            }
        }
        Err(LsError::Plugin("MCP tool returned no text content".into()))
    }
}

#[async_trait]
impl MessageChannel for McpChannelAdapter {
    fn id(&self) -> &'static str {
        self.id
    }

    fn meta(&self) -> ChannelMeta {
        self.meta.clone()
    }

    fn capabilities(&self) -> ChannelCapabilities {
        self.capabilities.clone()
    }

    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt> {
        let args = serde_json::json!({
            "to": ctx.to,
            "text": ctx.text,
            "reply_to_id": ctx.reply_to_id,
            "thread_id": ctx.thread_id,
            "silent": ctx.silent,
        });
        let result = self.call_tool("channel_send_text", args).await?;
        serde_json::from_value(result)
            .map_err(|e| LsError::Plugin(format!("Send receipt parse failed: {e}")))
    }

    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt> {
        let args = serde_json::json!({
            "to": ctx.to,
            "text": ctx.text,
            "media_url": ctx.media_url,
            "audio_as_voice": ctx.audio_as_voice,
            "reply_to_id": ctx.reply_to_id,
            "thread_id": ctx.thread_id,
        });
        let result = self.call_tool("channel_send_media", args).await?;
        serde_json::from_value(result)
            .map_err(|e| LsError::Plugin(format!("Send receipt parse failed: {e}")))
    }

    async fn send_payload(&self, ctx: SendPayloadContext) -> LsResult<SendReceipt> {
        let args = serde_json::json!({
            "to": ctx.to,
            "payload": ctx.payload,
            "reply_to_id": ctx.reply_to_id,
            "thread_id": ctx.thread_id,
        });
        let result = self.call_tool("channel_send_payload", args).await?;
        serde_json::from_value(result)
            .map_err(|e| LsError::Plugin(format!("Send receipt parse failed: {e}")))
    }

    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<()> {
        let args = serde_json::to_value(&event)
            .map_err(|e| LsError::Plugin(format!("InboundEvent serialize failed: {e}")))?;
        self.call_tool("channel_handle_inbound", args).await?;
        Ok(())
    }

    async fn health_check(&self) -> LsResult<HealthStatus> {
        let result = self.call_tool("channel_health", serde_json::json!({})).await?;
        serde_json::from_value(result)
            .map_err(|e| LsError::Plugin(format!("Health status parse failed: {e}")))
    }

    fn parse_target(&self, raw: &str) -> LsResult<MessagingTarget> {
        // 默认解析逻辑: 用 MCP 服务器的解析能力
        // 如果 raw 以 @ 开头 → User 类型
        // 否则 → Channel 类型
        let (kind, id) = if let Some(id) = raw.strip_prefix('@') {
            (MessagingTargetKind::User, id.to_string())
        } else if let Some(id) = raw.strip_prefix("channel:") {
            (MessagingTargetKind::Channel, id.to_string())
        } else {
            // 默认为 Channel/Group 类型
            (MessagingTargetKind::Channel, raw.to_string())
        };
        Ok(MessagingTarget {
            normalized: MessagingTarget::normalize(&kind, &id),
            kind,
            id,
            raw: raw.to_string(),
        })
    }
}
