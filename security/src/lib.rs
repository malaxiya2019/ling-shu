//! LSSecurity — 安全与权限体系。
//!
//! 模型: RBAC + ABAC 混合权限
//! 原则: 默认拒绝、最小权限、全程可追溯、隔离优先

pub mod audit;
pub mod auth;
pub mod permission;
pub mod service_auth;

#[cfg(feature = "oauth2")]
pub mod oauth2;
pub mod api_key;

pub use auth::*;
pub use permission::*;
pub use service_auth::*;
pub use api_key::*;

#[cfg(feature = "oauth2")]
pub use oauth2::*;
