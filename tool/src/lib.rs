//! LSTool — Tool Runtime.
//!
//! 提供统一的工具注册、权限控制、沙箱执行框架。
//!
//! # 模块
//!
//! - `registry` — 增强版 ToolRegistry（分类/标签/权限/沙箱）
//! - `permission` — 基于角色和作用域的权限控制
//! - `sandbox` — 超时/资源限制/安全执行沙箱
//! - `types` — 扩展类型定义

pub mod permission;
pub mod registry;
pub mod sandbox;
pub mod types;

pub use permission::{CallerInfo, CallerRole, ToolPermission};
pub use registry::ToolRegistry;
pub use sandbox::ToolSandbox;
pub use types::*;
