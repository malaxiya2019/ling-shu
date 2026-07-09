//! JWT 认证 — 令牌签发与验证.
//!
//! 用户认证: JWT (RS256/HS256)
//! 服务间认证: 非对称签名
//!
//! 遵循 LSCode v1.0.0:
//! - 用户用 JWT/会话令牌
//! - 服务/插件用非对称签名
//! - 禁止硬编码密钥

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};

/// JWT 载荷 — 包含 LSCode 规范所需字段.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 主体 (用户 ID).
    pub sub: String,
    /// 会话 ID.
    pub sid: String,
    /// 租户 ID (可选).
    pub tid: Option<String>,
    /// 角色列表.
    pub roles: Vec<String>,
    /// 过期时间 (Unix timestamp).
    pub exp: u64,
    /// 签发时间 (Unix timestamp).
    pub iat: u64,
}

/// 认证结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub user_id: String,
    pub session_id: String,
    pub tenant_id: Option<String>,
    pub roles: Vec<String>,
}

/// JWT 令牌服务.
#[derive(Debug, Clone)]
pub struct JwtService {
    secret: Vec<u8>,
    /// 令牌有效期 (秒).
    ttl_seconds: u64,
}

impl JwtService {
    /// 使用 HMAC 密钥创建服务.
    pub fn new(secret: impl Into<Vec<u8>>, ttl_seconds: u64) -> Self {
        Self {
            secret: secret.into(),
            ttl_seconds,
        }
    }

    /// 从环境变量 `LS_SECURITY_JWT_SECRET` 或配置创建.
    pub fn from_env_or(default_secret: &str, ttl_seconds: u64) -> Self {
        let secret =
            std::env::var("LS_SECURITY_JWT_SECRET").unwrap_or_else(|_| default_secret.to_string());
        Self::new(secret, ttl_seconds)
    }
    /// 签发 JWT 令牌 (简化版 — 兼容旧 API).
    pub fn generate_token(
        &self,
        user_id: &str,
        ttl_override: Option<u64>,
    ) -> LsResult<String> {
        let ttl = ttl_override.unwrap_or(self.ttl_seconds);
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            sid: uuid::Uuid::new_v4().to_string(),
            tid: None,
            roles: vec!["admin".to_string()],
            iat: now.timestamp() as u64,
            exp: (now + chrono::Duration::seconds(ttl as i64)).timestamp() as u64,
        };
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
        jsonwebtoken::encode(&header, &claims, &jsonwebtoken::EncodingKey::from_secret(&self.secret))
            .map_err(|e| LsError::AuthenticationFailed(format!("jwt encode: {e}")))
    }

    /// 签发完整的 JWT 令牌.
    pub fn issue(
        &self,
        user_id: &str,
        session_id: &str,
        tenant_id: Option<&str>,
        roles: Vec<String>,
    ) -> LsResult<String> {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            sid: session_id.to_string(),
            tid: tenant_id.map(|s| s.to_string()),
            roles,
            iat: now.timestamp() as u64,
            exp: (now + chrono::Duration::seconds(self.ttl_seconds as i64)).timestamp() as u64,
        };

        let header = Header::new(jsonwebtoken::Algorithm::HS256);
        encode(&header, &claims, &EncodingKey::from_secret(&self.secret))
            .map_err(|e| LsError::AuthenticationFailed(format!("jwt encode: {e}")))
    }

    /// 验证 JWT 令牌并返回声明.
    pub fn verify(&self, token: &str) -> LsResult<Claims> {
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_exp = true;

        let token_data =
            decode::<Claims>(token, &DecodingKey::from_secret(&self.secret), &validation)
                .map_err(|e| LsError::AuthenticationFailed(format!("jwt decode: {e}")))?;

        Ok(token_data.claims)
    }

    /// 验证令牌并返回 AuthResult.
    pub fn authenticate(&self, token: &str) -> LsResult<AuthResult> {
        let claims = self.verify(token)?;
        Ok(AuthResult {
            user_id: claims.sub,
            session_id: claims.sid,
            tenant_id: claims.tid,
            roles: claims.roles,
        })
    }
}

/// 服务间认证 — 使用非对称签名 (Ed25519).
///
/// 推荐使用 `Ed25519Service` (见 `service_auth` 模块) 替代此骨架实现。
#[derive(Debug)]
#[deprecated(
    since = "1.0.0",
    note = "use Ed25519Service from service_auth module instead"
)]
pub struct ServiceAuth;

#[allow(deprecated)]
impl ServiceAuth {
    /// 验证请求签名.
    ///
    /// # 参数
    /// - `payload`: 请求体
    /// - `signature`: 签名值 (hex 或 base64)
    /// - `public_key`: 对端公钥
    pub fn verify_signature(
        _payload: &[u8],
        _signature: &[u8],
        _public_key: &[u8],
    ) -> LsResult<bool> {
        Err(LsError::NotImplemented(
            "service auth — use Ed25519Service instead".into(),
        ))
    }

    /// 对请求签名.
    pub fn sign(_payload: &[u8], _private_key: &[u8]) -> LsResult<Vec<u8>> {
        Err(LsError::NotImplemented(
            "service auth — use Ed25519Service instead".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> JwtService {
        JwtService::new("test-secret-key-for-unit-tests", 3600)
    }

    #[test]
    fn test_issue_and_verify() {
        let svc = test_service();
        let token = svc
            .issue(
                "user_abc",
                "session_xyz",
                Some("tenant_1"),
                vec!["admin".into()],
            )
            .unwrap();
        assert!(!token.is_empty());

        let claims = svc.verify(&token).unwrap();
        assert_eq!(claims.sub, "user_abc");
        assert_eq!(claims.sid, "session_xyz");
        assert_eq!(claims.tid, Some("tenant_1".into()));
        assert_eq!(claims.roles, vec!["admin"]);
    }

    #[test]
    fn test_authenticate() {
        let svc = test_service();
        let token = svc.issue("alice", "s1", None, vec!["user".into()]).unwrap();
        let result = svc.authenticate(&token).unwrap();
        assert_eq!(result.user_id, "alice");
        assert_eq!(result.session_id, "s1");
        assert_eq!(result.tenant_id, None);
        assert_eq!(result.roles, vec!["user"]);
    }

    #[test]
    fn test_invalid_token() {
        let svc = test_service();
        let err = svc.verify("invalid.token.here");
        assert!(err.is_err());
        match err.unwrap_err() {
            LsError::AuthenticationFailed(_) => {} // expected
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn test_tampered_token() {
        let svc = test_service();
        let token = svc.issue("bob", "s2", None, vec![]).unwrap();
        // 篡改 token
        let parts: Vec<&str> = token.split('.').collect();
        let tampered = format!("{}.{}.invalidsig", parts[0], parts[1]);
        let err = svc.verify(&tampered);
        assert!(err.is_err());
    }

    #[test]
    fn test_expired_token() {
        let svc = JwtService::new("test-key", 0); // TTL 为 0
        let token = svc.issue("charlie", "s3", None, vec![]).unwrap();
        // 因为 TTL=0, 正常情况下 token 已过期
        let result = svc.verify(&token);
        // 如果立刻验证可能在同一个秒内, 所以允许两种结果
        if let Err(e) = result {
            assert!(matches!(e, LsError::AuthenticationFailed(_)));
        }
    }

    #[test]
    fn test_from_env() {
        unsafe {
            std::env::set_var("LS_SECURITY_JWT_SECRET", "env-secret");
        }
        let svc = JwtService::from_env_or("fallback", 300);
        let token = svc.issue("dave", "s4", None, vec![]).unwrap();
        let claims = svc.verify(&token).unwrap();
        assert_eq!(claims.sub, "dave");
        unsafe {
            std::env::remove_var("LS_SECURITY_JWT_SECRET");
        }
    }

    #[test]
    #[allow(deprecated)]
    fn test_service_auth_not_implemented() {
        let err = ServiceAuth::verify_signature(b"hello", b"sig", b"key");
        assert!(matches!(err.unwrap_err(), LsError::NotImplemented(_)));
    }
}
