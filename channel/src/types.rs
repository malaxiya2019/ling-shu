//! 🎯 通道核心类型 — 渠道无关的消息载荷、目标、回执与能力声明.

use serde::{Deserialize, Serialize};

// ── 聊天类型 ───────────────────────────────────────

/// 归一化聊天类型 — 所有消息平台统一映射为此三种.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatType {
    /// 私聊 (一对一).
    Direct,
    /// 群组 (多人).
    Group,
    /// 频道/公告 (广播).
    Channel,
}

impl ChatType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatType::Direct => "direct",
            ChatType::Group => "group",
            ChatType::Channel => "channel",
        }
    }
}

impl std::fmt::Display for ChatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── 消息载荷 ───────────────────────────────────────

/// Agent 回复载荷 — 渠道无关.
///
/// 对应 OpenClaw 的 `ReplyPayload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyPayload {
    /// 文本内容.
    pub text: Option<String>,
    /// 媒体文件列表.
    pub media_urls: Option<Vec<String>>,
    /// 回复目标消息 ID.
    pub reply_to_id: Option<String>,
    /// 音频作为语音消息而非文件发送.
    pub audio_as_voice: Option<bool>,
    /// 标记为错误信息.
    pub is_error: Option<bool>,
    /// 标记为推理/思考块 (通道可选择展示方式).
    pub is_reasoning: Option<bool>,
    /// 标记为状态通知 (非回复内容).
    pub is_status_notice: Option<bool>,
    /// 渠道专有数据 (JSON 透传).
    pub channel_data: Option<serde_json::Value>,
}

impl ReplyPayload {
    /// 创建纯文本载荷.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            media_urls: None,
            reply_to_id: None,
            audio_as_voice: None,
            is_error: None,
            is_reasoning: None,
            is_status_notice: None,
            channel_data: None,
        }
    }

    /// 创建错误载荷.
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            is_error: Some(true),
            ..Default::default()
        }
    }
}

impl Default for ReplyPayload {
    fn default() -> Self {
        Self {
            text: None,
            media_urls: None,
            reply_to_id: None,
            audio_as_voice: None,
            is_error: None,
            is_reasoning: None,
            is_status_notice: None,
            channel_data: None,
        }
    }
}

// ── 消息批次 ───────────────────────────────────────

/// 渲染后的消息批次 — 发送前的最终形态.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedMessageBatch {
    pub payloads: Vec<ReplyPayload>,
    pub plan: MessageBatchPlan,
}

/// 消息批次渲染计划 — 用于投递路由和恢复.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatchPlan {
    pub text_count: u32,
    pub media_count: u32,
    pub payload_count: u32,
}

// ── 发送上下文 ─────────────────────────────────────

/// 文本发送上下文.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTextContext {
    /// 目标标识.
    pub to: String,
    /// 文本内容.
    pub text: String,
    /// 回复目标消息 ID.
    pub reply_to_id: Option<String>,
    /// 线程/主题 ID.
    pub thread_id: Option<String>,
    /// 静默发送.
    pub silent: bool,
    /// 账户标识.
    pub account_id: Option<String>,
}

/// 媒体发送上下文.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMediaContext {
    pub to: String,
    pub text: Option<String>,
    pub media_url: String,
    pub audio_as_voice: bool,
    pub reply_to_id: Option<String>,
    pub thread_id: Option<String>,
    pub account_id: Option<String>,
}

/// 富文本发送上下文.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPayloadContext {
    pub to: String,
    pub payload: ReplyPayload,
    pub reply_to_id: Option<String>,
    pub thread_id: Option<String>,
    pub account_id: Option<String>,
}

// ── 发送结果 ───────────────────────────────────────

/// 发送回执.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReceipt {
    /// 平台消息 ID.
    pub message_id: String,
    /// 线程 ID.
    pub thread_id: Option<String>,
    /// 发送时间戳.
    pub timestamp: i64,
    /// 原始平台响应.
    pub raw: Option<serde_json::Value>,
}

// ── 消息目标 ───────────────────────────────────────

/// 解析后的消息目标.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingTarget {
    /// 目标类型.
    pub kind: MessagingTargetKind,
    /// 目标 ID.
    pub id: String,
    /// 用户原始输入.
    pub raw: String,
    /// 规范化键值.
    pub normalized: String,
}

/// 目标类型.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessagingTargetKind {
    /// 用户 (私聊).
    User,
    /// 频道/群组.
    Channel,
}

impl MessagingTarget {
    /// 构建规范化键值.
    pub fn normalize(kind: &MessagingTargetKind, id: &str) -> String {
        format!("{}:{}", 
            match kind { MessagingTargetKind::User => "user", MessagingTargetKind::Channel => "channel" },
            id.to_lowercase())
    }
}

// ── 会话路由 ───────────────────────────────────────

/// 渠道会话路由 — 出站消息的目标路由.
///
/// 对应 OpenClaw 的 `ChannelOutboundSessionRoute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSessionRoute {
    /// 会话键 (全局唯一).
    pub session_key: String,
    /// 对端信息.
    pub peer: ChannelPeer,
    /// 聊天类型.
    pub chat_type: ChatType,
    /// 发送方标识.
    pub from: String,
    /// 目标标识.
    pub to: String,
    /// 线程/主题 ID (用于 Telegram Topic, Feishu 等).
    pub thread_id: Option<String>,
}

/// 对端标识.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPeer {
    pub kind: ChatType,
    pub id: String,
}

// ── 通道能力 ───────────────────────────────────────

/// 通道能力声明.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    /// 支持的聊天类型.
    pub chat_types: Vec<ChatType>,
    /// 支持媒体文件.
    pub supports_media: bool,
    /// 支持投票.
    pub supports_polls: bool,
    /// 支持线程.
    pub supports_threads: bool,
    /// 支持编辑.
    pub supports_edit: bool,
    /// 支持回复.
    pub supports_reply: bool,
}

/// 通道元数据.
#[derive(Debug, Clone, )]
pub struct ChannelMeta {
    /// 显示标签.
    pub label: &'static str,
    /// 简要描述.
    pub description: &'static str,
    /// 文档链接.
    pub docs_url: Option<&'static str>,
    /// 别名列表.
    pub aliases: &'static [&'static str],
}

// ── 投递持久化 ─────────────────────────────────────

/// 消息投递持久化策略.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryDurability {
    /// 必须持久化投递 (重试直到确认).
    Required,
    /// 尽力投递 (不重试).
    BestEffort,
    /// 不持久化.
    Disabled,
}

/// 持久化投递能力声明.
#[derive(Debug, Clone, Default)]
pub struct DurableDeliveryCapabilities {
    pub text: bool,
    pub media: bool,
    pub reply_to: bool,
    pub thread: bool,
    pub batch: bool,
}

// ── 消息生命周期 ───────────────────────────────────

/// 实时消息生命周期阶段.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiveMessagePhase {
    Idle,
    Previewing,
    Finalizing,
    Finalized,
    Cancelled,
}

/// 实时消息状态.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveMessageState {
    pub phase: LiveMessagePhase,
    pub receipt: Option<SendReceipt>,
    pub last_rendered: Option<RenderedMessageBatch>,
}

// ── 入站事件 ───────────────────────────────────────

/// 标准化入站事件 — 从各平台 Webhook/WebSocket 解析而来.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundEvent {
    /// 来源通道 ID.
    pub channel_id: String,
    /// 消息 ID.
    pub message_id: Option<String>,
    /// 发送者 ID.
    pub sender_id: Option<String>,
    /// 发送者名称.
    pub sender_name: Option<String>,
    /// 聊天类型.
    pub chat_type: ChatType,
    /// 聊天 ID.
    pub chat_id: Option<String>,
    /// 文本内容.
    pub text: Option<String>,
    /// 媒体 URL 列表.
    pub media_urls: Vec<String>,
    /// 回复目标消息 ID.
    pub reply_to_id: Option<String>,
    /// 时间戳.
    pub timestamp: i64,
    /// 原始事件 JSON.
    pub raw: Option<serde_json::Value>,
}

// ── 健康状态 ───────────────────────────────────────

/// 渠道健康状态.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub connected_at: Option<i64>,
}
