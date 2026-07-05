//! ServiceAuth — 基于 Ed25519 的服务间认证。
//!
//! 提供:
//! - Ed25519 密钥对生成与管理 (PKCS#8 / PEM)
//! - 服务身份令牌签发与验证 (EdDSA)
//! - 服务公钥注册表 (ServiceRegistry)
//! - 密钥轮换与优雅过渡 (KeyRotationManager)
//!
//! 遵循 LSCode v1.0.0:
//! - 服务间认证用非对称签名 (Ed25519)
//! - 禁止硬编码密钥
//! - 支持密钥轮换

use base64::Engine;
use ed25519_dalek::{
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey},
    Signature, Signer, SigningKey, Verifier, VerifyingKey,
};
use lingshu_core::{LsError, LsResult};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ═══════════════════════════════════════════════════
// 1. 密钥对封装
// ═══════════════════════════════════════════════════

/// Ed25519 密钥对 + 元数据.
#[derive(Debug, Clone)]
pub struct ServiceKeyBundle {
    /// 所属服务 ID.
    pub service_id: String,
    /// 密钥标识 (kid)，用于 Key Rotation.
    pub key_id: String,
    /// 签名私钥.
    signing_key: SigningKey,
    /// 验证公钥.
    pub verifying_key: VerifyingKey,
    /// 创建时间.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 过期时间 (None = 永不过期).
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ServiceKeyBundle {
    /// 生成新的 Ed25519 密钥对.
    pub fn generate(service_id: impl Into<String>) -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        let key_id = uuid::Uuid::new_v4().to_string();

        Self {
            service_id: service_id.into(),
            key_id,
            signing_key,
            verifying_key,
            created_at: chrono::Utc::now(),
            expires_at: None,
        }
    }

    /// 从 PKCS#8 PEM 私钥内容加载密钥对.
    pub fn from_pem(service_id: impl Into<String>, pem: &str) -> LsResult<Self> {
        // Strip PEM headers and decode base64
        let b64 = pem
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect::<Vec<_>>()
            .join("");
        use base64::Engine as _;
        let der = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .map_err(|e| LsError::Internal(format!("pem base64 decode: {e}")))?;
        let signing_key = SigningKey::from_pkcs8_der(&der)
            .map_err(|e| LsError::Internal(format!("ed25519 pem parse failed: {e}")))?;
        let verifying_key = signing_key.verifying_key();
        let key_id = uuid::Uuid::new_v4().to_string();

        Ok(Self {
            service_id: service_id.into(),
            key_id,
            signing_key,
            verifying_key,
            created_at: chrono::Utc::now(),
            expires_at: None,
        })
    }

    /// 序列化私钥为 PKCS#8 PEM 字符串.
    pub fn to_pem(&self) -> LsResult<String> {
        use base64::Engine as _;
        let der = self
            .signing_key
            .to_pkcs8_der()
            .map_err(|e| LsError::Serialization(format!("ed25519 der encode: {e}")))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(der.as_bytes());
        Ok(format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
            b64
        ))
    }

    /// 导出公钥为 SPKI DER 字节.
    pub fn public_key_der(&self) -> Vec<u8> {
        self.verifying_key
            .to_public_key_der()
            .map(|der| der.to_vec())
            .unwrap_or_else(|_| self.verifying_key.as_bytes().to_vec())
    }

    /// 设置过期时间.
    pub fn with_expiry(mut self, ttl_seconds: u64) -> Self {
        self.expires_at = Some(chrono::Utc::now() + chrono::Duration::seconds(ttl_seconds as i64));
        self
    }

    /// 检查密钥是否过期.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| chrono::Utc::now() >= exp)
            .unwrap_or(false)
    }

    /// 编码公钥为 base64 字符串.
    pub fn public_key_b64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.verifying_key.as_bytes())
    }
}

// ═══════════════════════════════════════════════════
// 2. 服务身份令牌
// ═══════════════════════════════════════════════════

/// 服务间认证令牌载荷.
///
/// 格式: header.payload.signature (Ed25519 签名)
/// header: {"alg":"EdDSA","kid":"...","typ":"LS-SERVICE-AUTH"}
/// payload: 见 ServiceClaims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceClaims {
    /// 签发者服务 ID.
    pub iss: String,
    /// 目标服务 ID.
    pub aud: String,
    /// 会话/请求 ID.
    pub sid: String,
    /// 权限列表.
    pub perms: Vec<String>,
    /// 签发时间 (Unix timestamp).
    pub iat: u64,
    /// 过期时间 (Unix timestamp).
    pub exp: u64,
}

/// 服务认证结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAuthResult {
    pub issuer_service: String,
    pub audience_service: String,
    pub session_id: String,
    pub permissions: Vec<String>,
    pub key_id: String,
}

/// 序列化工具.
fn b64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn b64_decode(data: &str) -> LsResult<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .map_err(|e| LsError::AuthenticationFailed(format!("base64 decode: {e}")))
}

/// Ed25519 服务令牌.
pub struct ServiceToken;

impl ServiceToken {
    /// 签发 Ed25519 签名的服务令牌.
    pub fn issue(key: &ServiceKeyBundle, claims: &ServiceClaims) -> LsResult<String> {
        let payload_json =
            serde_json::to_string(claims).map_err(|e| LsError::Serialization(e.to_string()))?;

        let header = serde_json::json!({
            "alg": "EdDSA",
            "kid": key.key_id,
            "typ": "LS-SERVICE-AUTH"
        });
        let header_json =
            serde_json::to_string(&header).map_err(|e| LsError::Serialization(e.to_string()))?;

        let header_b64 = b64_encode(header_json.as_bytes());
        let payload_b64 = b64_encode(payload_json.as_bytes());

        // 签名 header.payload
        let signing_input = format!("{header_b64}.{payload_b64}");
        let signature: Signature = key.signing_key.sign(signing_input.as_bytes());
        let sig_b64 = b64_encode(&signature.to_bytes());

        Ok(format!("{signing_input}.{sig_b64}"))
    }

    /// 验证服务令牌并返回声明.
    pub fn verify(
        token: &str,
        verifying_keys: &HashMap<String, VerifyingKey>,
    ) -> LsResult<ServiceAuthResult> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(LsError::AuthenticationFailed("invalid token format".into()));
        }

        let header_b64 = parts[0];
        let payload_b64 = parts[1];
        let sig_b64 = parts[2];

        // 解码 header 获取 kid
        let header_json = b64_decode(header_b64)?;
        let header: serde_json::Value = serde_json::from_slice(&header_json)
            .map_err(|e| LsError::AuthenticationFailed(format!("header parse: {e}")))?;

        let kid = header["kid"]
            .as_str()
            .ok_or_else(|| LsError::AuthenticationFailed("missing kid".into()))?;

        // 查找对应公钥
        let verifying_key = verifying_keys
            .get(kid)
            .ok_or_else(|| LsError::AuthenticationFailed(format!("unknown kid: {kid}")))?;

        // 验证签名
        let signing_input = format!("{header_b64}.{payload_b64}");
        let sig_bytes = b64_decode(sig_b64)?;

        let sig_bytes_array: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| LsError::AuthenticationFailed("invalid signature length".into()))?;
        let signature = Signature::from_bytes(&sig_bytes_array);

        verifying_key
            .verify(signing_input.as_bytes(), &signature)
            .map_err(|e| LsError::AuthenticationFailed(format!("signature verify: {e}")))?;

        // 解码 payload
        let payload_json = b64_decode(payload_b64)?;
        let claims: ServiceClaims = serde_json::from_slice(&payload_json)
            .map_err(|e| LsError::AuthenticationFailed(format!("claims parse: {e}")))?;

        // 检查过期
        let now = chrono::Utc::now().timestamp() as u64;
        if now > claims.exp {
            return Err(LsError::AuthenticationFailed("token expired".into()));
        }

        Ok(ServiceAuthResult {
            issuer_service: claims.iss,
            audience_service: claims.aud,
            session_id: claims.sid,
            permissions: claims.perms,
            key_id: kid.to_string(),
        })
    }
}

// ═══════════════════════════════════════════════════
// 3. 服务公钥注册表
// ═══════════════════════════════════════════════════

/// 服务公钥注册表 — 管理所有服务的公钥.
#[derive(Debug, Clone)]
pub struct ServiceRegistry {
    /// service_id -> { kid: VerifyingKey }
    keys: Arc<RwLock<HashMap<String, HashMap<String, VerifyingKey>>>>,
    /// service_id -> { kid: metadata }
    metadata: Arc<RwLock<HashMap<String, HashMap<String, KeyMetadata>>>>,
}

/// 密钥元数据.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub key_id: String,
    pub service_id: String,
    pub algorithm: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_revoked: bool,
}

impl ServiceRegistry {
    /// 创建空的服务注册表.
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册服务公钥.
    pub async fn register(&self, bundle: &ServiceKeyBundle) -> LsResult<()> {
        let mut keys = self.keys.write().await;
        let mut meta = self.metadata.write().await;

        let service_keys = keys.entry(bundle.service_id.clone()).or_default();
        service_keys.insert(bundle.key_id.clone(), bundle.verifying_key);

        let service_meta = meta.entry(bundle.service_id.clone()).or_default();
        service_meta.insert(
            bundle.key_id.clone(),
            KeyMetadata {
                key_id: bundle.key_id.clone(),
                service_id: bundle.service_id.clone(),
                algorithm: "EdDSA".into(),
                created_at: bundle.created_at,
                expires_at: bundle.expires_at,
                is_revoked: false,
            },
        );

        Ok(())
    }

    /// 吊销服务密钥.
    pub async fn revoke(&self, service_id: &str, key_id: &str) -> LsResult<()> {
        let mut keys = self.keys.write().await;
        let mut meta = self.metadata.write().await;

        if let Some(service_keys) = keys.get_mut(service_id) {
            service_keys.remove(key_id);
        }

        if let Some(service_meta) = meta.get_mut(service_id) {
            if let Some(entry) = service_meta.get_mut(key_id) {
                entry.is_revoked = true;
            }
        }

        Ok(())
    }

    /// 获取服务的所有活跃公钥.
    pub async fn get_keys(&self, service_id: &str) -> HashMap<String, VerifyingKey> {
        let keys = self.keys.read().await;
        keys.get(service_id).cloned().unwrap_or_default()
    }

    /// 获取服务的密钥元数据.
    pub async fn get_metadata(&self, service_id: &str) -> Vec<KeyMetadata> {
        let meta = self.metadata.read().await;
        meta.get(service_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// 检查服务是否有活跃密钥.
    pub async fn has_active_keys(&self, service_id: &str) -> bool {
        let keys = self.keys.read().await;
        keys.get(service_id).map(|k| !k.is_empty()).unwrap_or(false)
    }

    /// 列出所有注册的服务 ID.
    pub async fn list_services(&self) -> Vec<String> {
        let keys = self.keys.read().await;
        keys.keys().cloned().collect()
    }

    /// 清理过期密钥 (异步回收).
    pub async fn clean_expired(&self) -> LsResult<u64> {
        let now = chrono::Utc::now();
        let mut cleanup: Vec<(String, String)> = Vec::new();

        {
            let meta = self.metadata.read().await;
            for (service_id, keys) in meta.iter() {
                for (key_id, entry) in keys.iter() {
                    if entry.is_revoked {
                        cleanup.push((service_id.clone(), key_id.clone()));
                    } else if let Some(exp) = entry.expires_at {
                        if now >= exp {
                            cleanup.push((service_id.clone(), key_id.clone()));
                        }
                    }
                }
            }
        }

        let count = cleanup.len() as u64;
        let mut keys = self.keys.write().await;
        for (service_id, key_id) in cleanup {
            if let Some(service_keys) = keys.get_mut(&service_id) {
                service_keys.remove(&key_id);
            }
        }

        Ok(count)
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════
// 4. 密钥轮换管理器
// ═══════════════════════════════════════════════════

/// 密钥轮换策略.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationStrategy {
    /// 固定间隔轮换 (秒).
    FixedInterval(u64),
    /// 每次签发新令牌时自动轮换.
    OnEachIssue,
    /// 手动触发.
    Manual,
}

/// 密钥轮换管理器.
#[derive(Debug, Clone)]
pub struct KeyRotationManager {
    /// 当前活跃的密钥对.
    current: Arc<RwLock<ServiceKeyBundle>>,
    /// 上一个密钥对 (过渡期内仍可验证).
    previous: Arc<RwLock<Option<ServiceKeyBundle>>>,
    /// 轮换策略.
    strategy: RotationStrategy,
    /// 过渡期 (秒): 旧密钥在此时间内仍可验证.
    overlap_seconds: u64,
}

impl KeyRotationManager {
    /// 创建密钥轮换管理器.
    pub fn new(
        initial_key: ServiceKeyBundle,
        strategy: RotationStrategy,
        overlap_seconds: u64,
    ) -> Self {
        Self {
            current: Arc::new(RwLock::new(initial_key)),
            previous: Arc::new(RwLock::new(None)),
            strategy,
            overlap_seconds,
        }
    }

    /// 获取当前签发密钥.
    pub async fn signing_key(&self) -> ServiceKeyBundle {
        self.current.read().await.clone()
    }

    /// 验证时查找公钥 (优先当前，回退上一个).
    pub async fn get_verifying_key(&self, kid: &str) -> Option<VerifyingKey> {
        let current = self.current.read().await;
        if current.key_id == kid {
            return Some(current.verifying_key);
        }
        let previous = self.previous.read().await;
        if let Some(ref prev) = *previous {
            if prev.key_id == kid && !prev.is_expired() {
                return Some(prev.verifying_key);
            }
        }
        None
    }

    /// 获取所有活跃公钥 (当前 + 过渡期内上一个).
    pub async fn active_keys(&self) -> HashMap<String, VerifyingKey> {
        let mut map = HashMap::new();
        let current = self.current.read().await;
        map.insert(current.key_id.clone(), current.verifying_key);

        let previous = self.previous.read().await;
        if let Some(ref prev) = *previous {
            if !prev.is_expired() {
                map.insert(prev.key_id.clone(), prev.verifying_key);
            }
        }

        map
    }

    /// 轮换密钥 — 生成新密钥对，旧密钥进入过渡期.
    pub async fn rotate(&self, service_id: &str) -> LsResult<()> {
        let mut current = self.current.write().await;

        // 将当前密钥移入 previous (带过渡期 TTL)
        let old_key = current.clone();
        let overlap_key = old_key.with_expiry(self.overlap_seconds);

        // 生成新密钥
        let new_key = ServiceKeyBundle::generate(service_id);

        let mut previous = self.previous.write().await;
        *previous = Some(overlap_key);
        *current = new_key;

        Ok(())
    }

    /// 自动轮换检查 — 如果策略为 FixedInterval 且到期则轮换.
    pub async fn maybe_rotate(&self, service_id: &str) -> LsResult<bool> {
        if let RotationStrategy::FixedInterval(interval_secs) = self.strategy {
            let current = self.current.read().await;
            let elapsed = (chrono::Utc::now() - current.created_at).num_seconds() as u64;
            if elapsed >= interval_secs {
                drop(current);
                self.rotate(service_id).await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 返回当前密钥的 kid.
    pub async fn current_kid(&self) -> String {
        self.current.read().await.key_id.clone()
    }
}

// ═══════════════════════════════════════════════════
// 5. 高级接口 — Ed25519Service
// ═══════════════════════════════════════════════════

/// Ed25519 服务认证 — 一站式接口.
#[derive(Debug, Clone)]
pub struct Ed25519Service {
    /// 本地密钥对 (含轮换).
    key_manager: KeyRotationManager,
    /// 远程服务公钥注册表.
    registry: ServiceRegistry,
    /// 本服务 ID.
    service_id: String,
    /// 默认令牌 TTL (秒).
    default_ttl: u64,
}

impl Ed25519Service {
    /// 创建 Ed25519 认证服务.
    pub fn new(
        service_id: impl Into<String>,
        strategy: RotationStrategy,
        overlap_seconds: u64,
        default_ttl: u64,
    ) -> Self {
        let sid: String = service_id.into();
        let initial_key = ServiceKeyBundle::generate(&sid);
        Self {
            key_manager: KeyRotationManager::new(initial_key, strategy, overlap_seconds),
            registry: ServiceRegistry::new(),
            service_id: sid,
            default_ttl,
        }
    }

    /// 从 PEM 私钥加载 (用于恢复).
    pub fn from_pem(
        service_id: impl Into<String>,
        pem: &str,
        strategy: RotationStrategy,
        overlap_seconds: u64,
        default_ttl: u64,
    ) -> LsResult<Self> {
        let sid: String = service_id.into();
        let key = ServiceKeyBundle::from_pem(&sid, pem)?;
        Ok(Self {
            key_manager: KeyRotationManager::new(key, strategy, overlap_seconds),
            registry: ServiceRegistry::new(),
            service_id: sid,
            default_ttl,
        })
    }

    /// 注册本服务到注册表 (自注册).
    pub async fn register_self(&self) -> LsResult<()> {
        let key = self.key_manager.signing_key().await;
        self.registry.register(&key).await
    }

    /// 签发服务令牌.
    pub async fn issue_token(
        &self,
        audience: &str,
        session_id: &str,
        permissions: Vec<String>,
    ) -> LsResult<String> {
        // 检查是否需要自动轮换
        self.key_manager.maybe_rotate(&self.service_id).await?;

        let now = chrono::Utc::now();
        let claims = ServiceClaims {
            iss: self.service_id.clone(),
            aud: audience.to_string(),
            sid: session_id.to_string(),
            perms: permissions,
            iat: now.timestamp() as u64,
            exp: (now + chrono::Duration::seconds(self.default_ttl as i64)).timestamp() as u64,
        };

        let key = self.key_manager.signing_key().await;
        ServiceToken::issue(&key, &claims)
    }

    /// 验证服务令牌 (优先本地密钥，回退注册表).
    pub async fn verify_token(&self, token: &str) -> LsResult<ServiceAuthResult> {
        // 先尝试本地活跃密钥 (含过渡期)
        let local_keys = self.key_manager.active_keys().await;
        if let Ok(result) = ServiceToken::verify(token, &local_keys) {
            return Ok(result);
        }
        // 回退: 从注册表查询所有服务的公钥
        let services = self.registry.list_services().await;
        for svc in services {
            let remote_keys = self.registry.get_keys(&svc).await;
            if let Ok(result) = ServiceToken::verify(token, &remote_keys) {
                return Ok(result);
            }
        }
        Err(LsError::AuthenticationFailed(
            "token verification failed: no matching key found in local or registry".into(),
        ))
    }

    /// 注册远程服务公钥.
    pub async fn register_remote(&self, bundle: &ServiceKeyBundle) -> LsResult<()> {
        self.registry.register(bundle).await
    }

    /// 吊销远程服务密钥.
    pub async fn revoke_remote(&self, service_id: &str, key_id: &str) -> LsResult<()> {
        self.registry.revoke(service_id, key_id).await
    }

    /// 获取密钥管理器引用.
    pub fn key_manager(&self) -> &KeyRotationManager {
        &self.key_manager
    }

    /// 获取注册表引用.
    pub fn registry(&self) -> &ServiceRegistry {
        &self.registry
    }

    /// 获取本服务 ID.
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    /// 轮换密钥.
    pub async fn rotate_key(&self) -> LsResult<()> {
        self.key_manager.rotate(&self.service_id).await
    }
}

// ═══════════════════════════════════════════════════
// 6. 测试
// ═══════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let bundle = ServiceKeyBundle::generate("test-service");
        assert_eq!(bundle.service_id, "test-service");
        assert!(!bundle.key_id.is_empty());
        assert!(!bundle.is_expired());
    }

    #[test]
    fn test_pem_roundtrip() {
        let bundle = ServiceKeyBundle::generate("svc-01");
        let pem = bundle.to_pem().unwrap();
        let restored = ServiceKeyBundle::from_pem("svc-01", &pem).unwrap();
        assert_eq!(
            bundle.verifying_key.as_bytes(),
            restored.verifying_key.as_bytes()
        );
    }

    #[test]
    fn test_service_token_issue_and_verify() {
        let key = ServiceKeyBundle::generate("issuer-svc");

        let claims = ServiceClaims {
            iss: "issuer-svc".into(),
            aud: "target-svc".into(),
            sid: uuid::Uuid::new_v4().to_string(),
            perms: vec!["read".into(), "write".into()],
            iat: chrono::Utc::now().timestamp() as u64,
            exp: (chrono::Utc::now() + chrono::Duration::seconds(3600)).timestamp() as u64,
        };

        let token = ServiceToken::issue(&key, &claims).unwrap();
        assert_eq!(token.split('.').count(), 3);

        // 验证
        let mut keys = HashMap::new();
        keys.insert(key.key_id.clone(), key.verifying_key);
        let result = ServiceToken::verify(&token, &keys).unwrap();

        assert_eq!(result.issuer_service, "issuer-svc");
        assert_eq!(result.audience_service, "target-svc");
        assert_eq!(result.permissions, vec!["read", "write"]);
    }

    #[test]
    fn test_service_token_expired() {
        let key = ServiceKeyBundle::generate("svc");

        let claims = ServiceClaims {
            iss: "svc".into(),
            aud: "other".into(),
            sid: "s1".into(),
            perms: vec![],
            iat: 1000000,
            exp: 1, // 已过期
        };

        let token = ServiceToken::issue(&key, &claims).unwrap();

        let mut keys = HashMap::new();
        keys.insert(key.key_id.clone(), key.verifying_key);
        let result = ServiceToken::verify(&token, &keys);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            LsError::AuthenticationFailed(_)
        ));
    }

    #[test]
    fn test_invalid_signature() {
        let key1 = ServiceKeyBundle::generate("svc-a");
        let key2 = ServiceKeyBundle::generate("svc-b");

        let claims = ServiceClaims {
            iss: "svc-a".into(),
            aud: "other".into(),
            sid: "s1".into(),
            perms: vec![],
            iat: chrono::Utc::now().timestamp() as u64,
            exp: (chrono::Utc::now() + chrono::Duration::seconds(3600)).timestamp() as u64,
        };

        // 用 key1 签发
        let token = ServiceToken::issue(&key1, &claims).unwrap();

        // 用 key2 的公钥验证 (应该失败)
        let mut keys = HashMap::new();
        keys.insert("wrong-key".into(), key2.verifying_key);
        let result = ServiceToken::verify(&token, &keys);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_service_registry() {
        let registry = ServiceRegistry::new();
        let bundle = ServiceKeyBundle::generate("svc-a");

        registry.register(&bundle).await.unwrap();
        assert!(registry.has_active_keys("svc-a").await);

        let keys = registry.get_keys("svc-a").await;
        assert_eq!(keys.len(), 1);
        assert!(keys.contains_key(&bundle.key_id));

        let services = registry.list_services().await;
        assert_eq!(services, vec!["svc-a"]);

        registry.revoke("svc-a", &bundle.key_id).await.unwrap();
        assert!(!registry.has_active_keys("svc-a").await);
    }

    #[tokio::test]
    async fn test_service_registry_multiple_keys() {
        let registry = ServiceRegistry::new();
        let svc = "multi-key-svc";

        let key1 = ServiceKeyBundle::generate(svc);
        let key2 = ServiceKeyBundle::generate(svc);

        registry.register(&key1).await.unwrap();
        registry.register(&key2).await.unwrap();

        let keys = registry.get_keys(svc).await;
        assert_eq!(keys.len(), 2);

        let meta = registry.get_metadata(svc).await;
        assert_eq!(meta.len(), 2);
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let initial_key = ServiceKeyBundle::generate("rotating-svc");
        let mgr = KeyRotationManager::new(initial_key, RotationStrategy::Manual, 60);

        let kid_before = mgr.current_kid().await;
        mgr.rotate("rotating-svc").await.unwrap();
        let kid_after = mgr.current_kid().await;

        assert_ne!(kid_before, kid_after);

        // 过渡期内，两个 key 都应活跃
        let active = mgr.active_keys().await;
        assert_eq!(active.len(), 2);
        assert!(active.contains_key(&kid_before));
        assert!(active.contains_key(&kid_after));
    }

    #[tokio::test]
    async fn test_ed25519_service_full_flow() {
        let svc_a = Ed25519Service::new("service-a", RotationStrategy::Manual, 60, 3600);
        let svc_b = Ed25519Service::new("service-b", RotationStrategy::Manual, 60, 3600);

        // 互相注册公钥
        let key_a = svc_a.key_manager.signing_key().await;
        let key_b = svc_b.key_manager.signing_key().await;
        svc_a.register_remote(&key_b).await.unwrap();
        svc_b.register_remote(&key_a).await.unwrap();

        // service-a 向 service-b 签发令牌
        let token = svc_a
            .issue_token("service-b", "session-001", vec!["read".into()])
            .await
            .unwrap();

        // service-b 验证令牌 (使用本地密钥验证)
        let result = svc_b.verify_token(&token).await.unwrap();
        assert_eq!(result.issuer_service, "service-a");
        assert_eq!(result.audience_service, "service-b");
        assert_eq!(result.session_id, "session-001");
        assert_eq!(result.permissions, vec!["read"]);
    }

    #[tokio::test]
    async fn test_rotation_with_service() {
        let svc = Ed25519Service::new("rotating-svc", RotationStrategy::Manual, 60, 3600);

        // 注册自己的公钥
        let key = svc.key_manager.signing_key().await;
        svc.register_remote(&key).await.unwrap();

        // 签发令牌 (用旧密钥)
        let token1 = svc.issue_token("other-svc", "s1", vec![]).await.unwrap();

        // 轮换密钥
        svc.rotate_key().await.unwrap();

        // 注册新公钥
        let new_key = svc.key_manager.signing_key().await;
        svc.register_remote(&new_key).await.unwrap();

        // 旧令牌仍可验证 (过渡期内)
        let result = svc.verify_token(&token1).await;
        assert!(result.is_ok());

        // 新令牌也可验证
        let token2 = svc.issue_token("other-svc", "s2", vec![]).await.unwrap();
        let result = svc.verify_token(&token2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_clean_expired_keys() {
        let registry = ServiceRegistry::new();
        let bundle = ServiceKeyBundle::generate("temp-svc");
        registry.register(&bundle).await.unwrap();
        assert!(registry.has_active_keys("temp-svc").await);

        let count = registry.clean_expired().await.unwrap();
        assert_eq!(count, 0); // 没有过期密钥

        // 吊销密钥
        registry.revoke("temp-svc", &bundle.key_id).await.unwrap();
        let count = registry.clean_expired().await.unwrap();
        assert_eq!(count, 1);
        assert!(!registry.has_active_keys("temp-svc").await);
    }

    #[test]
    fn test_b64_roundtrip() {
        let data = b"hello lingshu ed25519";
        let encoded = b64_encode(data);
        let decoded = b64_decode(&encoded).unwrap();
        assert_eq!(data.to_vec(), decoded);
    }

    #[test]
    fn test_key_id_is_unique() {
        let k1 = ServiceKeyBundle::generate("svc");
        let k2 = ServiceKeyBundle::generate("svc");
        assert_ne!(k1.key_id, k2.key_id);
    }

    #[test]
    fn test_public_key_b64() {
        let bundle = ServiceKeyBundle::generate("svc");
        let b64 = bundle.public_key_b64();
        assert!(!b64.is_empty());
        // 32 bytes → ~44 chars in base64
        assert_eq!(b64.len(), 44);
    }

    #[test]
    fn test_key_expiry() {
        let bundle = ServiceKeyBundle::generate("svc").with_expiry(0);
        // TTL=0, 应该已过期
        assert!(bundle.is_expired());
    }
}
