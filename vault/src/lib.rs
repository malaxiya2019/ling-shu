//! LSVault — HashiCorp Vault 集成
//!
//! ## 功能
//! - KV v2 Secrets Engine — 读写密钥
//! - 动态 Secret — 数据库/云服务凭证
//! - Transit Engine — 加解密
//! - Lease 管理 — 续约/撤销
//! - 健康检查
//!
//! ## 使用
//! ```ignore
//! use lingshu_vault::{VaultClient, VaultClientTrait, VaultConfig};
//!
//! let config = VaultConfig {
//!     address: "https://vault.example.com:8200".into(),
//!     token: "hvs.xxxx".into(),
//!     ..Default::default()
//! };
//! let client = VaultClient::new(config)?;
//! let health = client.health().await?;
//! ```

pub mod client;
pub mod models;

pub use client::{MockVaultClient, VaultClient, VaultClientTrait};
pub use models::*;
