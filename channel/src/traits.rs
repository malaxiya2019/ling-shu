//! 🧩 MessageChannel trait — 消息通道插件核心接口.

use crate::types::*;
use async_trait::async_trait;

/// 消息通道插件核心接口.
///
/// 每个消息平台 (Telegram, Discord, WeChat 等) 实现此 trait.
/// 对应 OpenClaw 的 `ChannelPlugin` + `ChannelMessageAdapter`.
#[async_trait]
pub trait MessageChannel: Send + Sync {
    /// 唯一标识 (如 "telegram", "wechat", "discord").
    fn id(&self) -> &'static str;

    /// 通道元数据.
    fn meta(&self) -> ChannelMeta;

    /// 能力声明.
    fn capabilities(&self) -> ChannelCapabilities;

    /// ── 发送消息 ──
    /// 发送纯文本消息.
    async fn send_text(&self, ctx: SendTextContext) -> crate::LsResult<SendReceipt>;

    /// 发送媒体消息.
    async fn send_media(&self, ctx: SendMediaContext) -> crate::LsResult<SendReceipt>;

    /// 发送富文本/互动消息 (默认为降级 send_text).
    async fn send_payload(&self, ctx: SendPayloadContext) -> crate::LsResult<SendReceipt> {
        if let Some(text) = &ctx.payload.text {
            self.send_text(SendTextContext {
                to: ctx.to,
                text: text.clone(),
                reply_to_id: ctx.reply_to_id,
                thread_id: ctx.thread_id,
                silent: false,
                account_id: ctx.account_id,
            })
            .await
        } else {
            Err(crate::LsError::Llm(
                "send_payload: no text content in payload".into(),
            ))
        }
    }

    /// ── 入站处理 ──
    /// 处理入站事件 (Webhook/WebSocket 回调解析).
    async fn handle_inbound(&self, event: InboundEvent) -> crate::LsResult<()>;

    /// ── 生命周期 ──
    /// 渠道健康检查.
    async fn health_check(&self) -> crate::LsResult<HealthStatus>;

    /// ── 目标解析 ──
    /// 解析原始输入为目标标识.
    fn parse_target(&self, raw: &str) -> crate::LsResult<MessagingTarget>;
}

/// 消息发送生命周期钩子.
#[async_trait]
pub trait MessageSendLifecycle: Send + Sync {
    /// 发送前调用.
    async fn before_send(&self, ctx: &SendPayloadContext) -> crate::LsResult<()>;

    /// 发送成功后调用.
    async fn after_send_success(
        &self,
        ctx: &SendPayloadContext,
        receipt: &SendReceipt,
    ) -> crate::LsResult<()>;

    /// 发送失败后调用.
    async fn after_send_failure(
        &self,
        ctx: &SendPayloadContext,
        error: &crate::LsError,
    ) -> crate::LsResult<()>;

    /// 持久化提交后调用.
    async fn after_commit(
        &self,
        ctx: &SendPayloadContext,
        receipt: &SendReceipt,
    ) -> crate::LsResult<()>;
}
