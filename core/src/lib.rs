//! LSCore — Lingshu 核心类型库.
//!
//! 提供全系统统一使用的原始类型：
//! - `LsId` — 全局唯一标识符
//! - `LsError` — 统一错误类型
//! - `LsResult<T>` — 统一返回类型
//! - `LsContext` — 异步请求上下文

pub mod context;
pub mod error;
pub mod id;

pub use context::{LsContext, SharedContext};
pub use error::{LsError, LsResult};
pub use id::LsId;
