//! LSMCP — Lingshu Model Context Protocol 实现
//!
//! 提供 JSON-RPC 2.0 基础的 MCP 协议，用于 AI Agent 与工具之间的标准通信。
//!
//! ## 架构
//!
//! ```text
//!   HTTP (axum) → McpServer::handle_request() → Tool Registry → Tool::execute()
//! ```
//!
//! ## 支持的方法
//!
//! - `tools/list` — 列出已注册的 MCP 工具
//! - `tools/call` — 调用指定工具

pub mod server;
pub mod tool;
pub mod types;

pub use server::McpServer;
pub use types::*;
