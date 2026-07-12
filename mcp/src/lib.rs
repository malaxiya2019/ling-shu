//! LSMCP — Lingshu Model Context Protocol 实现
//!
//! 提供 JSON-RPC 2.0 基础的 MCP 协议，用于 AI Agent 与工具之间的标准通信。
//!
//! ## 架构
//!
//! ```text
//!   HTTP (axum) → McpServer::handle_request() → Tool Registry → Tool::execute()
//!                                        ↓
//!                               Progress Notification
//!                                        ↓
//!                             SSE / WebSocket Bridge
//! ```
//!
//! ## 支持的方法
//!
//! - `tools/list`              — 列出已注册的 MCP 工具
//! - `tools/call`              — 调用指定工具（支持 `progress_token`）
//! - `notifications/progress`  — 工具执行进度通知（由服务端推送）
//! - `mcp_status`              — 查询服务器状态与工具执行信息
//! - `resources/list`          — 列出可用资源 (预留)
//! - `prompts/list`            — 列出可用提示 (预留)
//!
//! ## 流式进度使用示例
//!
//! ```json
//! // 客户端请求工具调用时附带 progress_token
//! {
//!   "jsonrpc": "2.0",
//!   "method": "tools/call",
//!   "params": {
//!     "name": "code_analysis",
//!     "arguments": { "project": "my-app" },
//!     "progress_token": "analysis-001"
//!   },
//!   "id": 1
//! }
//!
//! // 服务端在执行过程中推送进度通知
//! {
//!   "jsonrpc": "2.0",
//!   "method": "notifications/progress",
//!   "params": {
//!     "progress_token": "analysis-001",
//!     "progress": 45,
//!     "total": 100,
//!     "message": "Analyzing file 5/12..."
//!   }
//! }
//! ```

pub mod credential_tools;
pub mod discovery;
#[cfg(feature = "agent-runtime")]
pub mod agent_tools;
pub mod server;
pub mod tool;
pub mod types;
#[cfg(feature = "rmcp")]
pub mod server_launcher;

#[cfg(feature = "rmcp")]
pub mod rmcp_server;
#[cfg(feature = "rmcp")]
pub mod rmcp_client;

pub use server::McpServer;
pub use types::*;
