//! LSCore — Lingshu 核心类型库 (v5.0).
//!
//! 全系统统一使用的原始类型，**所有 crate 的唯一公共依赖**。
//!
//! # 核心类型
//!
//! | 类型 | 用途 | 特性 |
//! |------|------|------|
//! | [`LsId`] | 全局唯一标识符 | `Copy + Clone + Ord + Serialize/Deserialize` |
//! | [`LsError`] | 统一错误类型 | 支持链式错误、HTTP 状态码映射 |
//! | [`LsResult<T>`] | 统一返回类型 | `type LsResult<T> = Result<T, LsError>` |
//! | [`LsContext`] | 异步请求上下文 | 携带 session_id / user_id / metadata / trace_id |
//! | [`SharedContext`] | 线程安全上下文 | `Arc<LsContext>` 别名 |
//!
//! # 设计原则
//!
//! - **零外部依赖链** — 不依赖其他 LingShu crate
//! - **全系统统一** — 所有 crate 共用同一套基础类型
//! - **Serde 全覆盖** — 所有类型均实现 `Serialize/Deserialize`

pub mod context;
pub mod error;
pub mod id;

/// 全局唯一标识符 — 基于 UUIDv7（时间排序），支持 JSON 序列化、Copy、排序。
///
/// # 示例
/// ```ignore
/// use lingshu_core::LsId;
/// let id = LsId::new();
/// println!("{}", id);
/// ```
pub use id::LsId;

/// 统一错误类型 — 全系统错误的唯一入口。
///
/// 变体包括：`NotFound`, `Validation`, `Internal`, `Timeout`, `Unauthorized` 等。
/// 自动实现 `Into<LsResult<T>>` 和 `From<LsError> for HttpStatusCode`。
pub use error::{LsError, LsResult};

/// 异步请求上下文 — 携带调用链所需的 session、用户、元数据、追踪信息。
///
/// 通过 `LsContext::with_session(id)` 创建，支持 `.with_user()`, `.with_metadata()` 链式调用。
pub use context::{LsContext, SharedContext};
