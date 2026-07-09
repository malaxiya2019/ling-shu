//! 凭证类型定义 — 5 大 Git 提供商 + 加密存储

use serde::{Deserialize, Serialize};

/// Git 提供商.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GitProvider {
    Gitee,
    Codeup,
    Coding,
    GitCode,
    Cnb,
}

impl GitProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            GitProvider::Gitee => "gitee",
            GitProvider::Codeup => "codeup",
            GitProvider::Coding => "coding",
            GitProvider::GitCode => "gitcode",
            GitProvider::Cnb => "cnb",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "gitee" => Some(GitProvider::Gitee),
            "codeup" => Some(GitProvider::Codeup),
            "coding" => Some(GitProvider::Coding),
            "gitcode" => Some(GitProvider::GitCode),
            "cnb" => Some(GitProvider::Cnb),
            _ => None,
        }
    }
}

/// 凭证类型.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CredentialType {
    PersonalAccessToken,
    EnterpriseToken,
    DeploymentToken,
    AccessToken,
}

impl CredentialType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CredentialType::PersonalAccessToken => "personal_access_token",
            CredentialType::EnterpriseToken => "enterprise_token",
            CredentialType::DeploymentToken => "deployment_token",
            CredentialType::AccessToken => "access_token",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "personal_access_token" => Some(CredentialType::PersonalAccessToken),
            "enterprise_token" => Some(CredentialType::EnterpriseToken),
            "deployment_token" => Some(CredentialType::DeploymentToken),
            "access_token" => Some(CredentialType::AccessToken),
            _ => None,
        }
    }
}

/// 凭证条目（加密前结构）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialEntry {
    pub id: String,
    pub provider: GitProvider,
    pub credential_type: CredentialType,
    pub name: String,
    pub description: String,
    /// 明文 token（存储时会加密）.
    pub token: String,
    pub username: Option<String>,
    pub base_url: Option<String>,
    pub scopes: Vec<String>,
    pub permissions_group: Option<String>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 凭证条目（加密后持久化结构）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEntry {
    pub id: String,
    pub provider: String,
    pub credential_type: String,
    pub name: String,
    pub description: String,
    /// AES-256-GCM 加密的 token（base64）.
    pub encrypted_token: String,
    pub nonce: String,
    pub username: Option<String>,
    pub base_url: Option<String>,
    pub scopes: String,
    pub permissions_group: Option<String>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 创建凭证请求.
#[derive(Debug, Deserialize)]
pub struct CreateCredentialRequest {
    pub provider: String,
    pub credential_type: String,
    pub name: String,
    pub description: Option<String>,
    pub token: String,
    pub username: Option<String>,
    pub base_url: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub permissions_group: Option<String>,
    pub expires_at: Option<i64>,
}

/// 更新凭证请求.
#[derive(Debug, Deserialize)]
pub struct UpdateCredentialRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub token: Option<String>,
    pub username: Option<String>,
    pub base_url: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub permissions_group: Option<String>,
    pub expires_at: Option<i64>,
}

/// 凭证验证结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialValidation {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub valid: bool,
    pub message: String,
    pub scopes_verified: Vec<String>,
}

/// 凭证概要（列表用，不暴露 token）.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSummary {
    pub id: String,
    pub provider: String,
    pub credential_type: String,
    pub name: String,
    pub description: String,
    pub masked_token: String,
    pub username: Option<String>,
    pub base_url: Option<String>,
    pub scopes: Vec<String>,
    pub permissions_group: Option<String>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}
