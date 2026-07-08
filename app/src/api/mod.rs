//! API 路由模块
//!
//! ## 架构
//!
//! 采用 OpenHands FastAPI + MCP router 模式组织路由。
//! 当前为过渡期，所有路由定义在 single-file `full.rs` 中，
//! 后续逐步迁移到独立模块。
//!
//! ### 模块计划
//!
//! - `full.rs` — 当前完整实现 (5113 lines)
//! - `health.rs` — 健康检查 ✅
//! - `metrics.rs` — 实时指标 ✅
//! - `chat.rs` — Chat completion (计划中)
//! - `agents.rs` — Agent 操作 (计划中)
//! - `plugins.rs` — 插件管理 (计划中)
//! - `mcp.rs` — MCP 协议 (计划中)
//! - `auth.rs` — 认证 (计划中)
//! - `federation.rs` — 联邦网络 (计划中)
//! - `eval.rs` — 评测框架 (计划中)
//!
//! 迁移完成后 `full.rs` 将删除。

#[allow(clippy::module_inception)]
pub mod full;

// Re-export everything for backward compatibility
pub use full::*;
