//! Vault 数据模型

use serde::{Deserialize, Serialize};

/// Vault 配置.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Vault 服务地址，例如 `https://vault.example.com:8200`
    pub address: String,
    /// 用于认证的令牌 (Token Auth)
    pub token: String,
    /// 是否跳过 TLS 验证 (仅开发环境)
    pub tls_skip_verify: bool,
    /// 默认 KV Secrets Engine 路径
    pub kv_engine_path: String,
    /// 默认 Transit Engine 路径
    pub transit_engine_path: Option<String>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            address: "http://127.0.0.1:8200".into(),
            token: String::new(),
            tls_skip_verify: false,
            kv_engine_path: "secret".into(),
            transit_engine_path: Some("transit".into()),
        }
    }
}

/// KV 密钥 (写入).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KvSecretData {
    pub data: serde_json::Map<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}

/// KV 密钥 (读取响应).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KvSecretResponse {
    pub data: KvSecretDataInner,
    #[serde(default)]
    pub metadata: KvMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KvSecretDataInner {
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// KV 元数据.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct KvMetadata {
    #[serde(default)]
    pub created_time: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub destroyed: bool,
}

/// 动态 Secret (例如数据库凭证).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynamicSecret {
    pub lease_id: String,
    pub lease_duration: u64,
    pub renew: bool,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// Vault 健康状态.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultHealth {
    pub initialized: bool,
    pub sealed: bool,
    pub standby: bool,
    pub cluster_name: Option<String>,
    pub cluster_id: Option<String>,
}

/// Vault 操作结果.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultResult<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
