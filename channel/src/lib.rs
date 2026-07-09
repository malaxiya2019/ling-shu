//! # lingshu-channel — 多平台消息通道抽象
//!
//! 提供渠道无关的消息发送/接收抽象，支持:
//! - 原生通道实现 (Telegram)
//! - MCP 协议桥接 (通过 MCP Client 连接外部通道网关)
//! - 通道注册表 (已加载 + 内置懒加载)
//! - 消息发送生命周期钩子
//!
//! ## 架构
//!
//! ```text
//! lingshu Agent
//!     │
//!     ├── MessageChannel trait ←── TelegramChannel (原生)
//!     │
//!     └── McpChannelAdapter ──MCP协议──► 通道网关 (TypeScript)
//!                                            ├── WhatsApp
//!                                            ├── WeChat
//!                                            └── Telegram
//! ```
//!
//! ## 快速开始
//!
//! ```rust,no_run
//! use lingshu_channel::registry::ChannelRegistry;
//! use lingshu_channel::traits::MessageChannel;
//!
//! # async fn example() {
//! let registry = ChannelRegistry::new();
//! // 注册通道后可通过 registry.get("telegram") 获取
//! # }
//! ```

// ── 模块声明 ───────────────────────────────────────

pub mod types;
pub mod traits;
pub mod registry;
pub mod mcp_adapter;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "feishu")]
pub mod feishu;

#[cfg(feature = "qq")]
pub mod qq;

#[cfg(feature = "wechat")]
pub mod wechat;

// ── 类型重导出 ─────────────────────────────────────

pub use lingshu_core::{LsError, LsResult};
pub use types::*;
pub use traits::*;
pub use registry::ChannelRegistry;
pub use mcp_adapter::McpChannelAdapter;

#[cfg(feature = "telegram")]
pub use telegram::TelegramChannel;

#[cfg(feature = "feishu")]
pub use feishu::FeishuChannel;

#[cfg(feature = "qq")]
pub use qq::QqChannel;

#[cfg(feature = "wechat")]
pub use wechat::WeChatChannel;
pub mod session_store;
pub mod router;
