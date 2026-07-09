# OpenClaw 消息通道架构参考 — lingshu 适配方案

> 日期: 2026-07-09
> 来源: [openclaw/openclaw](https://github.com/openclaw/openclaw) v3.x (TypeScript)
> 目标: 提取核心设计模式，映射到 lingshu Rust 架构

---

## 一、核心抽象层级对比

### 1.1 OpenClaw 三层抽象

```
┌─────────────────────────────────────────────┐
│  ChannelPlugin (插件契约)                     │
│  ├─ id / meta / capabilities                │
│  ├─ ConfigAdapter     → 配置解析              │
│  ├─ SetupAdapter      → 设置向导              │
│  ├─ SecurityAdapter   → DM 安全策略           │
│  ├─ GatewayAdapter    → Gateway 连接          │
│  ├─ StatusAdapter     → 健康/状态              │
│  └─ MessageAdapter    → 消息收发 (核心)        │
├─────────────────────────────────────────────┤
│  ChannelMessageAdapter (消息适配器)            │
│  ├─ send.text()       → 纯文本发送             │
│  ├─ send.media()      → 媒体文件发送           │
│  ├─ send.payload()    → 富文本/互动消息        │
│  ├─ send.poll()       → 投票                  │
│  └─ receive           → 入站确认策略           │
├─────────────────────────────────────────────┤
│  MessagingTarget / ChatType (标准化类型)       │
│  ├─ ChatType = direct | group | channel      │
│  └─ MessagingTarget = { kind, id, normalized }│
└─────────────────────────────────────────────┘
```

### 1.2 lingshu 现有对等层

```
┌─────────────────────────────────────────────┐
│  lingshu-plugin (WASM 热加载插件系统)          │
│  ├─ PluginRegistry                           │
│  ├─ HotReloadWatcher                         │
│  └─ PluginMarket                             │
├─────────────────────────────────────────────┤
│  lingshu-mcp (MCP 协议层)                     │
│  ├─ MCP Server (stdio/HTTP)                  │
│  ├─ MCP Client Pool                          │
│  └─ rmcp bridge                              │
├─────────────────────────────────────────────┤
│  lingshu-backends (LLM 后端)                  │
│  ├─ llmkit (27+ 提供商)                       │
│  ├─ llama.cpp (本地推理)                      │
│  └─ Llm trait                                 │
└─────────────────────────────────────────────┘
```

---

## 二、ReplyPayload 设计提取

### 2.1 OpenClaw 定义

`src/auto-reply/reply-payload.ts`:

```typescript
type ReplyPayload = {
  text?: string;                    // 文本内容
  mediaUrl?: string;                // 单媒体 URL
  mediaUrls?: string[];             // 多媒体 URL
  presentation?: MessagePresentation; // 富文本展示
  delivery?: ReplyPayloadDelivery;  // 投递偏好 (如 pin 消息)
  replyToId?: string;               // 回复目标消息 ID
  replyToTag?: boolean;             // 标记为回复
  audioAsVoice?: boolean;           // 音频作为语音消息
  isError?: boolean;                // 错误标记
  isReasoning?: boolean;            // 推理/思考块
  isStatusNotice?: boolean;         // 状态通知
  channelData?: Record<string, unknown>; // 渠道专有数据
};
```

### 2.2 Rust 映射

```rust
/// Agent 回复载荷 — 渠道无关.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyPayload {
    pub text: Option<String>,
    pub media_urls: Option<Vec<String>>,
    pub reply_to_id: Option<String>,
    pub audio_as_voice: Option<bool>,
    pub is_error: Option<bool>,
    pub is_reasoning: Option<bool>,
    pub is_status_notice: Option<bool>,
    /// 渠道专有数据 (JSON value 透传)
    pub channel_data: Option<serde_json::Value>,
}

/// 渲染后的消息批次 — 发送前的最终形态.
#[derive(Debug, Clone)]
pub struct RenderedMessageBatch {
    pub payloads: Vec<ReplyPayload>,
    pub plan: MessageBatchPlan,
}

/// 消息批次渲染计划 — 用于投递路由和恢复.
#[derive(Debug, Clone)]
pub struct MessageBatchPlan {
    pub text_count: u32,
    pub media_count: u32,
    pub payload_count: u32,
}
```

---

## 三、ChannelOutboundSessionRoute 设计提取

### 3.1 OpenClaw 定义

`src/channels/plugins/types.core.ts`:

```typescript
type ChannelOutboundSessionRoute = {
  sessionKey: string;           // 会话标识
  baseSessionKey: string;       // 基础会话标识
  recipientSessionExact?: bool; // 精确收件人会话
  peer: {                       // 对端信息
    kind: ChatType;             // direct | group | channel
    id: string;
  };
  chatType: "direct" | "group" | "channel";
  from: string;                 // 发送方 ID
  to: string;                   // 目标 ID
  threadId?: string | number;   // 线程 ID
};
```

### 3.2 Rust 映射

```rust
/// 渠道会话路由 — 出站消息的目标路由.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct ChannelPeer {
    pub kind: ChatType,
    pub id: String,
}

/// 归一化聊天类型.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatType {
    Direct,   // 私聊
    Group,    // 群组
    Channel,  // 频道/公告
}
```

---

## 四、消息收发适配器接口 (Rust trait)

### 4.1 完整通道插件 trait

```rust
/// 消息通道插件核心 trait.
#[async_trait]
pub trait MessageChannel: Send + Sync {
    /// 唯一标识 (如 "telegram", "wechat", "whatsapp").
    fn id(&self) -> &'static str;

    /// 通道元数据.
    fn meta(&self) -> ChannelMeta;

    /// 能力声明.
    fn capabilities(&self) -> ChannelCapabilities;

    /// 发送文本消息.
    async fn send_text(&self, ctx: SendTextContext) -> LsResult<SendReceipt>;

    /// 发送媒体消息.
    async fn send_media(&self, ctx: SendMediaContext) -> LsResult<SendReceipt>;

    /// 发送富文本/互动消息 (可选).
    async fn send_payload(&self, ctx: SendPayloadContext) -> LsResult<SendReceipt> {
        // 默认: 降级为 send_text
        self.send_text(ctx.into()).await
    }

    /// 处理入站事件.
    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<InboundContext>;

    /// 渠道健康检查.
    async fn health_check(&self) -> LsResult<HealthStatus>;

    /// 解析目标标识.
    fn parse_target(&self, raw: &str) -> LsResult<MessagingTarget>;
}

/// 通道元数据.
#[derive(Debug, Clone)]
pub struct ChannelMeta {
    pub label: &'static str,
    pub description: &'static str,
    pub docs_url: Option<&'static str>,
    pub aliases: &'static [&'static str],
}

/// 通道能力声明.
#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    pub chat_types: Vec<ChatType>,
    pub supports_media: bool,
    pub supports_polls: bool,
    pub supports_threads: bool,
    pub supports_edit: bool,
    pub supports_reply: bool,
}
```

### 4.2 发送上下文

```rust
/// 文本发送上下文.
#[derive(Debug, Clone)]
pub struct SendTextContext {
    pub to: String,                // 目标
    pub text: String,              // 文本
    pub reply_to_id: Option<String>, // 回复目标
    pub thread_id: Option<String>, // 线程
    pub silent: bool,              // 静默发送
    pub account_id: Option<String>, // 账户标识
}

/// 媒体发送上下文.
#[derive(Debug, Clone)]
pub struct SendMediaContext {
    pub to: String,
    pub text: Option<String>,      // 媒体附文
    pub media_url: String,         // 媒体 URL
    pub audio_as_voice: bool,      // 音频作为语音消息
    pub reply_to_id: Option<String>,
    pub thread_id: Option<String>,
    pub account_id: Option<String>,
}

/// 富文本发送上下文.
#[derive(Debug, Clone)]
pub struct SendPayloadContext {
    pub to: String,
    pub payload: ReplyPayload,
    pub reply_to_id: Option<String>,
    pub thread_id: Option<String>,
    pub account_id: Option<String>,
}

/// 发送回执.
#[derive(Debug, Clone)]
pub struct SendReceipt {
    pub message_id: String,
    pub thread_id: Option<String>,
    pub timestamp: i64,
    pub raw: Option<serde_json::Value>,
}
```

---

## 五、通道插件注册与发现

### 5.1 OpenClaw 注册机制

```
listChannelPlugins()         → 列出已加载插件
getChannelPlugin(id)         → 已加载 → 捆绑回退
getBundledChannelPlugin(id)  → 内置插件懒加载
normalizeChannelId(raw)      → 别名 → 规范 ID
```

### 5.2 Rust 注册表设计

```rust
/// 通道插件注册表.
pub struct ChannelRegistry {
    /// 已加载插件.
    loaded: HashMap<&'static str, Arc<dyn MessageChannel>>,
    /// 内置插件工厂 (懒加载).
    builtins: HashMap<&'static str, Box<dyn Fn() -> Arc<dyn MessageChannel> + Send + Sync>>,
    /// 通道 ID → 别名映射.
    aliases: HashMap<String, &'static str>,
}

impl ChannelRegistry {
    pub fn new() -> Self { /* ... */ }

    /// 注册一个已加载的插件.
    pub fn register(&mut self, channel: Arc<dyn MessageChannel>) { /* ... */ }

    /// 注册一个内置插件工厂.
    pub fn register_builtin(
        &mut self,
        factory: Box<dyn Fn() -> Arc<dyn MessageChannel> + Send + Sync>,
    ) { /* ... */ }

    /// 获取通道插件 (已加载 → 内置懒加载回退).
    pub fn get(&self, id: &str) -> Option<Arc<dyn MessageChannel>> { /* ... */ }

    /// 通过别名规范化通道 ID.
    pub fn normalize_id(&self, raw: &str) -> Option<&'static str> { /* ... */ }

    /// 列出所有可用通道.
    pub fn list(&self) -> Vec<&'static str> { /* ... */ }
}
```

---

## 六、消息生命周期 (参考 OpenClaw LiveMessagePhase)

### 6.1 OpenClaw 定义

```typescript
type LiveMessagePhase = "idle" | "previewing" | "finalizing" | "finalized" | "cancelled";

type LiveMessageState<TPayload> = {
  phase: LiveMessagePhase;
  canFinalizeInPlace: boolean;
  receipt?: MessageReceipt;
  lastRendered?: RenderedMessageBatch<TPayload>;
};
```

### 6.2 Rust 状态机

```rust
/// 实时消息生命周期阶段.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveMessagePhase {
    Idle,
    Previewing,    // 流式预览中
    Finalizing,    // 正在最终确认
    Finalized,     // 已发送完成
    Cancelled,     // 用户取消
}

/// 实时消息状态.
#[derive(Debug, Clone)]
pub struct LiveMessageState {
    pub phase: LiveMessagePhase,
    pub receipt: Option<SendReceipt>,
    pub last_rendered: Option<RenderedMessageBatch>,
}

/// 生命周期钩子 — Agent 回复发送的完整流程.
#[async_trait]
pub trait MessageSendLifecycle: Send + Sync {
    /// 渲染前调用.
    async fn before_render(&self, ctx: &SendPayloadContext) -> LsResult<()>;

    /// 发送成功后调用.
    async fn after_send_success(&self, ctx: &SendPayloadContext, receipt: &SendReceipt) -> LsResult<()>;

    /// 发送失败后调用.
    async fn after_send_failure(&self, ctx: &SendPayloadContext, error: &LsError) -> LsResult<()>;

    /// 持久化提交后调用.
    async fn after_commit(&self, ctx: &SendPayloadContext, receipt: &SendReceipt) -> LsResult<()>;
}
```

---

## 七、投递保证策略

### 7.1 OpenClaw 定义

```typescript
type MessageDurabilityPolicy = "required" | "best_effort" | "disabled";
```

### 7.2 Rust 映射

```rust
/// 消息投递持久化策略.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryDurability {
    /// 必须持久化投递 (重试直到确认).
    Required,
    /// 尽力投递 (不重试).
    BestEffort,
    /// 不持久化 (仅实时通道).
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
```

---

## 八、通道插件实现示例 (Telegram)

```rust
use async_trait::async_trait;
use crate::channel::*;

pub struct TelegramChannel {
    bot_token: String,
    api_client: reqwest::Client,
}

#[async_trait]
impl MessageChannel for TelegramChannel {
    fn id(&self) -> &'static str { "telegram" }

    fn meta(&self) -> ChannelMeta {
        ChannelMeta {
            label: "Telegram",
            description: "Telegram 消息平台",
            docs_url: Some("https://docs.openclaw.ai/channels/telegram"),
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
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        let payload = serde_json::json!({
            "chat_id": ctx.to,
            "text": ctx.text,
            "reply_to_message_id": ctx.reply_to_id,
            "disable_notification": ctx.silent,
        });
        let resp = self.api_client.post(&url).json(&payload).send().await?;
        // ... 解析响应，返回 SendReceipt
        todo!()
    }

    async fn handle_inbound(&self, event: InboundEvent) -> LsResult<InboundContext> {
        // 将 Telegram Update 转换为 InboundContext
        todo!()
    }

    fn parse_target(&self, raw: &str) -> LsResult<MessagingTarget> {
        // 支持 @username, chat_id, channel:xxx 等格式
        todo!()
    }
}
```

---

## 九、与 lingshu 现有架构的集成路径

### 9.1 分层集成

```
┌──────────────────────────────────────────────────┐
│  lingshu Agent Engine (现有)                       │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │ LLM      │ │ Tool     │ │ Session/Recovery │ │
│  │ Backends │ │ System   │ │ Manager          │ │
│  └──────────┘ └──────────┘ └──────────────────┘ │
├──────────────────────────────────────────────────┤
│  Channel Adapter Layer (新增)                     │
│  ┌──────────────────────────────────────────┐   │
│  │  ChannelRegistry (通道注册表)              │   │
│  └──────────┬───────────────────────────────┘   │
│  ┌──────────┼──────────┐ ┌──────────────────┐  │
│  │ Telegram │ WeChat   │ │ Discord  ...     │  │
│  │ Channel  │ Channel  │ │ Channel          │  │
│  └──────────┴──────────┘ └──────────────────┘  │
├──────────────────────────────────────────────────┤
│  Transport Layer (传输层)                          │
│  ├─ Webhook Receiver (Axum 路由)                 │
│  ├─ WebSocket Client (长连接)                    │
│  └─ Polling (短轮询)                             │
└──────────────────────────────────────────────────┘
```

### 9.2 复用 MCP 协议通道

最有效的集成方式是**不直接接入每个 IM 平台**，而是通过 MCP 协议将 OpenClaw 的通道能力暴露给 lingshu：

```
lingshu Agent
    │
    ├── MCP Client  ←→  OpenClaw MCP Server  ←→  WhatsApp/WeChat/Telegram...
    │                     (通过 MCP 协议调用
    │                       OpenClaw 的 send_message)
    │
    └── 直接通道插件 (需要长期运行时)
         (Rust 原生, 适用于需要低延迟的场景)
```

### 9.3 推荐实现优先级

| 阶段 | 内容 | 工作量 |
|------|------|--------|
| 🥇 P0 | 定义 `MessageChannel` trait + 核心类型 | ~200 行 |
| 🥇 P0 | 实现 `ChannelRegistry` | ~100 行 |
| 🥈 P1 | Telegram 通道插件 (Webhook + API) | ~500 行 |
| 🥈 P1 | Discord 通道插件 (Gateway + REST) | ~500 行 |
| 🥉 P2 | MCP 桥接通道 (复用 OpenClaw 渠道) | ~300 行 |
| 🥉 P2 | WeChat 通道插件 | ~600 行 |
| 🥉 P3 | 其他通道 | 逐个适配 |

---

## 十、关键设计原则总结

1. **渠道无关载荷**: `ReplyPayload` 在所有通道间共享，通道负责渲染为平台原生格式
2. **能力声明**: 每个通道明确声明支持的 `ChatType`/媒体/投票/线程等能力，核心根据能力降级
3. **先解析后路由**: `MessagingTarget` 统一解析 → `ChannelSessionRoute` 精确路由
4. **生命周期钩子**: 发送前/成功后/失败后/提交后四个钩子，支持日志/审计/重试
5. **持久化投递**: `DeliveryDurability` 策略控制是否可靠投递
6. **捆绑 + 热加载**: 内置通道捆绑注册，外部通道 WASM 热加载

---

*参考: [OpenClaw GitHub](https://github.com/openclaw/openclaw) | 文档日期: 2026-07-09*
