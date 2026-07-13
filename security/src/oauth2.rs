//! OAuth2 / OIDC 认证 — 第三方身份提供商集成.
//!
//! 支持 OpenID Connect 标准流程，兼容 Google、GitHub、Microsoft 等 OIDC 提供商。
//!
//! ## 环境变量
//! - `LS_OAUTH2_ENABLED` — 启用 OAuth2
//! - `LS_OAUTH2_PROVIDER` — 提供商 (google/github/microsoft/custom)
//! - `LS_OAUTH2_CLIENT_ID` — 客户端 ID
//! - `LS_OAUTH2_CLIENT_SECRET` — 客户端密钥
//! - `LS_OAUTH2_REDIRECT_URL` — 回调 URL
//! - `LS_OAUTH2_ISSUER_URL` — (自定义) 签发者 URL
//! - `LS_OAUTH2_SCOPES` — 请求的 scopes (逗号分隔)

use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OAuth2 提供商.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OAuth2Provider {
    Google,
    GitHub,
    Microsoft,
    Custom(String),
}

impl OAuth2Provider {
    pub fn as_str(&self) -> &str {
        match self {
            OAuth2Provider::Google => "google",
            OAuth2Provider::GitHub => "github",
            OAuth2Provider::Microsoft => "microsoft",
            OAuth2Provider::Custom(s) => s,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "google" => OAuth2Provider::Google,
            "github" => OAuth2Provider::GitHub,
            "microsoft" => OAuth2Provider::Microsoft,
            other => OAuth2Provider::Custom(other.to_string()),
        }
    }
}

/// OAuth2 配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    /// 是否启用
    pub enabled: bool,
    /// 提供商
    pub provider: OAuth2Provider,
    /// 客户端 ID
    pub client_id: String,
    /// 客户端密钥
    pub client_secret: String,
    /// 回调 URL
    pub redirect_url: String,
    /// 签发者 URL (自定义提供商)
    pub issuer_url: Option<String>,
    /// 请求的 scopes
    pub scopes: Vec<String>,
}

impl Default for OAuth2Config {
    fn default() -> Self {
        Self::from_env()
    }
}

impl OAuth2Config {
    /// 从环境变量加载.
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("LS_OAUTH2_ENABLED").as_deref() == Ok("true"),
            provider: OAuth2Provider::from_str(
                &std::env::var("LS_OAUTH2_PROVIDER").unwrap_or_else(|_| "google".into()),
            ),
            client_id: std::env::var("LS_OAUTH2_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("LS_OAUTH2_CLIENT_SECRET").unwrap_or_default(),
            redirect_url: std::env::var("LS_OAUTH2_REDIRECT_URL")
                .unwrap_or_else(|_| "http://localhost:8080/api/auth/callback".into()),
            issuer_url: std::env::var("LS_OAUTH2_ISSUER_URL").ok(),
            scopes: std::env::var("LS_OAUTH2_SCOPES")
                .unwrap_or_else(|_| "openid,profile,email".into())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        }
    }
}

/// OIDC 用户信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcUserInfo {
    /// 用户 ID (sub claim)
    pub sub: String,
    /// 邮箱
    pub email: Option<String>,
    /// 邮箱已验证
    pub email_verified: Option<bool>,
    /// 用户名
    pub name: Option<String>,
    /// 头像 URL
    pub picture: Option<String>,
    /// 区域
    pub locale: Option<String>,
    /// 原始声明
    pub raw_claims: HashMap<String, serde_json::Value>,
}

/// OAuth2 认证管理器.
pub struct OAuth2Manager {
    config: OAuth2Config,
    /// 提供商元数据 (从 .well-known/openid-configuration 获取)
    #[allow(dead_code)]
    provider_metadata: Option<HashMap<String, String>>,
}

impl OAuth2Manager {
    /// 创建 OAuth2 管理器.
    pub fn new(config: OAuth2Config) -> Self {
        Self {
            config,
            #[allow(dead_code)]
    provider_metadata: None,
        }
    }

    /// 从环境变量创建.
    pub fn from_env() -> Self {
        Self::new(OAuth2Config::from_env())
    }

    /// 获取授权 URL (用户跳转).
    pub fn authorization_url(&self, state: &str) -> LsResult<String> {
        if !self.config.enabled {
            return Err(LsError::NotImplemented("OAuth2 is not enabled".into()));
        }

        let base_url = match self.config.provider {
            OAuth2Provider::Google => "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            OAuth2Provider::GitHub => "https://github.com/login/oauth/authorize".to_string(),
            OAuth2Provider::Microsoft => {
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string()
            }
            OAuth2Provider::Custom(ref issuer) => {
                format!("{issuer}/authorize")
            }
        };

        let scopes = self.config.scopes.join(" ");
        let url = format!(
            "{base_url}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            urlencoding(&self.config.client_id),
            urlencoding(&self.config.redirect_url),
            urlencoding(&scopes),
            urlencoding(state),
        );

        Ok(url)
    }

    /// 交换授权码为令牌并获取用户信息.
    pub async fn exchange_code(&self, code: &str) -> LsResult<OidcUserInfo> {
        if !self.config.enabled {
            return Err(LsError::NotImplemented("OAuth2 is not enabled".into()));
        }

        let token_url = match self.config.provider {
            OAuth2Provider::Google => {
                "https://oauth2.googleapis.com/token"
            }
            OAuth2Provider::GitHub => {
                "https://github.com/login/oauth/access_token"
            }
            OAuth2Provider::Microsoft => {
                "https://login.microsoftonline.com/common/oauth2/v2.0/token"
            }
            OAuth2Provider::Custom(ref issuer) => {
                // For custom providers, we'd fetch from .well-known
                return Err(LsError::NotImplemented(format!(
                    "Custom OIDC provider '{issuer}' requires issuer metadata fetch"
                )));
            }
        };

        // Build token exchange request
        let client = reqwest::Client::new();
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", self.config.redirect_url.as_str()),
            ("grant_type", "authorization_code"),
        ];

        let resp = client
            .post(token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| LsError::Internal(format!("OAuth2 token exchange: {e}")))?;

        let token_data: HashMap<String, serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| LsError::Internal(format!("OAuth2 token response: {e}")))?;

        // Extract ID token
        let id_token = token_data
            .get("id_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::AuthenticationFailed("no id_token in response".into()))?;

        // Decode JWT (without verification for now — use JWKS in production)
        self.decode_id_token(id_token)
    }

    /// 解码 ID Token (JWT) 提取用户信息.
    fn decode_id_token(&self, token: &str) -> LsResult<OidcUserInfo> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() < 2 {
            return Err(LsError::AuthenticationFailed("invalid id_token format".into()));
        }

        // Decode payload (base64)
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| LsError::AuthenticationFailed(format!("base64 decode: {e}")))?;

        let claims: HashMap<String, serde_json::Value> = serde_json::from_slice(&payload)
            .map_err(|e| LsError::AuthenticationFailed(format!("json decode: {e}")))?;

        let sub = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::AuthenticationFailed("missing sub claim".into()))?
            .to_string();

        let email = claims.get("email").and_then(|v| v.as_str()).map(String::from);
        let email_verified = claims.get("email_verified").and_then(|v| v.as_bool());
        let name = claims.get("name").and_then(|v| v.as_str()).map(String::from);
        let picture = claims.get("picture").and_then(|v| v.as_str()).map(String::from);
        let locale = claims.get("locale").and_then(|v| v.as_str()).map(String::from);

        Ok(OidcUserInfo {
            sub,
            email,
            email_verified,
            name,
            picture,
            locale,
            raw_claims: claims,
        })
    }

    /// 生成 OAuth2 state 参数 (防 CSRF).
    pub fn generate_state() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..32).map(|_| rng.gen::<char>()).collect()
    }
}

/// URL 编码辅助.
fn urlencoding(input: &str) -> String {
    use urlencoding::encode;
    encode(input).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_str() {
        assert_eq!(OAuth2Provider::from_str("google"), OAuth2Provider::Google);
        assert_eq!(OAuth2Provider::from_str("GITHUB"), OAuth2Provider::GitHub);
        assert_eq!(OAuth2Provider::from_str("Microsoft"), OAuth2Provider::Microsoft);
        match OAuth2Provider::from_str("custom-oidc") {
            OAuth2Provider::Custom(s) => assert_eq!(s, "custom-oidc"),
            _ => panic!("expected custom"),
        }
    }

    #[test]
    fn test_config_disabled_by_default() {
        let config = OAuth2Config {
            enabled: false,
            ..Default::default()
        };
        let manager = OAuth2Manager::new(config);
        let err = manager.authorization_url("test-state");
        assert!(err.is_err());
    }

    #[test]
    fn test_authorization_url_google() {
        let config = OAuth2Config {
            enabled: true,
            provider: OAuth2Provider::Google,
            client_id: "test-client-id".into(),
            client_secret: "test-secret".into(),
            redirect_url: "http://localhost:8080/callback".into(),
            issuer_url: None,
            scopes: vec!["openid".into(), "profile".into(), "email".into()],
        };
        let manager = OAuth2Manager::new(config);
        let url = manager.authorization_url("my-state").unwrap();
        assert!(url.contains("accounts.google.com"));
        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("state=my-state"));
        assert!(url.contains("openid"));
    }

    #[test]
    fn test_generate_state() {
        let state1 = OAuth2Manager::generate_state();
        let state2 = OAuth2Manager::generate_state();
        assert_ne!(state1, state2);
        assert_eq!(state1.len(), 32);
    }

    #[test]
    fn test_decode_id_token_mock() {
        // Create a mock JWT with valid JSON payload
        use base64::Engine;
        let payload = serde_json::json!({
            "sub": "12345",
            "email": "user@example.com",
            "email_verified": true,
            "name": "Test User",
            "picture": "https://example.com/avatar.png",
            "locale": "en"
        });
        let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_string(&payload).unwrap().as_bytes());

        let manager = OAuth2Manager::new(OAuth2Config {
            enabled: true,
            ..Default::default()
        });
        let token = format!("header.{payload_b64}.signature");
        let info = manager.decode_id_token(&token).unwrap();
        assert_eq!(info.sub, "12345");
        assert_eq!(info.email.unwrap(), "user@example.com");
        assert_eq!(info.name.unwrap(), "Test User");
    }

    #[test]
    fn test_decode_id_token_invalid() {
        let manager = OAuth2Manager::new(OAuth2Config::default());
        let err = manager.decode_id_token("invalid");
        assert!(err.is_err());
    }
}
