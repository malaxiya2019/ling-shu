//! LSCredentials — 多 Git 提供商凭证保险库.
//!
//! 支持: Gitee, Codeup (阿里云效), CODING (腾讯云), GitCode, CNB (腾讯云).
//!
//! ## 架构
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           CredentialManager              │
//! │  ┌─────────────────┐ ┌───────────────┐ │
//! │  │  CredentialStore │ │  Validators    │ │
//! │  │ (AES-256 + SQLite)│ │ (各提供商API) │ │
//! │  └─────────────────┘ └───────────────┘ │
//! └─────────────────────────────────────────┘
//! ```

pub mod types;
pub mod encrypted_store;
pub mod manager;

pub use types::*;
pub use encrypted_store::CredentialStore;
pub use manager::CredentialManager;
