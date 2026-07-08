//! API 密钥管理 — 生成、验证、轮换、撤销.
//!
//! 支持多密钥并行 (key rotation)，密钥前缀识别，自动过期。
//!
//! ## 密钥格式
//! ```text
//! ls_<prefix>_<base64url-32bytes-random>
//! ```
//!
//! ## 环境变量
//! - `LS_API_KEY_RETENTION` — 历史密钥保留数量 (默认: 5)

use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// API 密钥条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    /// 密钥 ID (哈希)
    pub key_id: String,
    /// 密钥前缀 (用于识别)
    pub key_prefix: String,
    /// 关联用户/服务
    pub owner: String,
    /// 角色
    pub roles: Vec<String>,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 过期时间
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 是否已撤销
    pub revoked: bool,
    /// 最后使用时间
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// API 密钥管理器.
pub struct ApiKeyManager {
    /// 活跃密钥 (key_hash → entry)
    active_keys: Arc<RwLock<HashMap<String, ApiKeyEntry>>>,
    /// 历史密钥 (key_hash → entry)
    history_keys: Arc<RwLock<HashMap<String, ApiKeyEntry>>>,
    /// 最大历史密钥保留数
    max_history: usize,
}

impl ApiKeyManager {
    /// 创建密钥管理器.
    pub fn new() -> Self {
        let max_history = std::env::var("LS_API_KEY_RETENTION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        Self {
            active_keys: Arc::new(RwLock::new(HashMap::new())),
            history_keys: Arc::new(RwLock::new(HashMap::new())),
            max_history,
        }
    }

    /// 生成新的 API 密钥.
    pub async fn generate(
        &self,
        prefix: &str,
        owner: &str,
        roles: Vec<String>,
        ttl_seconds: Option<u64>,
    ) -> LsResult<(String, ApiKeyEntry)> {
        use rand::Rng;

        // 生成 32 字节随机值
        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill(&mut key_bytes);

        // 编码为 base64url (无 padding)
        use base64::Engine;
        let key_raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key_bytes);

        // 完整密钥
        let full_key = format!("ls_{prefix}_{key_raw}");

        // 计算哈希
        let key_hash = Self::hash_key(&full_key);
        let key_prefix = format!("ls_{prefix}_");

        let entry = ApiKeyEntry {
            key_id: key_hash.clone(),
            key_prefix,
            owner: owner.to_string(),
            roles,
            created_at: chrono::Utc::now(),
            expires_at: ttl_seconds.map(|s| chrono::Utc::now() + chrono::Duration::seconds(s as i64)),
            revoked: false,
            last_used_at: None,
        };

        self.active_keys.write().await.insert(key_hash, entry.clone());
        info!(key_id = %entry.key_id, owner = %owner, "API key generated");
        Ok((full_key, entry))
    }

    /// 验证 API 密钥.
    pub async fn verify(&self, key: &str) -> LsResult<ApiKeyEntry> {
        let key_hash = Self::hash_key(key);

        let keys = self.active_keys.read().await;
        let entry = keys.get(&key_hash).ok_or_else(|| {
            LsError::AuthenticationFailed("invalid API key".into())
        })?;

        if entry.revoked {
            return Err(LsError::AuthenticationFailed("API key revoked".into()));
        }

        if let Some(expires_at) = entry.expires_at {
            if chrono::Utc::now() > expires_at {
                return Err(LsError::AuthenticationFailed("API key expired".into()));
            }
        }

        Ok(entry.clone())
    }

    /// 撤销 API 密钥.
    pub async fn revoke(&self, key_id: &str) -> LsResult<()> {
        let mut keys = self.active_keys.write().await;
        if let Some(entry) = keys.get_mut(key_id) {
            entry.revoked = true;

            // 移至历史
            let entry_clone = entry.clone();
            let mut history = self.history_keys.write().await;
            history.insert(key_id.to_string(), entry_clone);

            // 限制历史大小
            while history.len() > self.max_history {
                let oldest_key = history.keys().next().unwrap().to_string();
                history.remove(&oldest_key);
            }

            info!(key_id = %key_id, "API key revoked");
            Ok(())
        } else {
            Err(LsError::NotFound(format!("API key '{key_id}' not found")))
        }
    }

    /// 轮换密钥 — 生成新密钥并标记旧密钥为待移除.
    pub async fn rotate(
        &self,
        old_key_id: &str,
        prefix: &str,
        owner: &str,
        roles: Vec<String>,
        ttl_seconds: Option<u64>,
    ) -> LsResult<(String, ApiKeyEntry)> {
        // 撤销旧密钥
        self.revoke(old_key_id).await?;

        // 生成新密钥
        self.generate(prefix, owner, roles, ttl_seconds).await
    }

    /// 标记密钥已使用.
    pub async fn mark_used(&self, key_id: &str) {
        if let Some(entry) = self.active_keys.write().await.get_mut(key_id) {
            entry.last_used_at = Some(chrono::Utc::now());
        }
    }

    /// 获取活跃密钥列表.
    pub async fn list_active(&self) -> Vec<ApiKeyEntry> {
        self.active_keys.read().await.values().cloned().collect()
    }

    /// 获取历史密钥列表.
    pub async fn list_history(&self) -> Vec<ApiKeyEntry> {
        self.history_keys.read().await.values().cloned().collect()
    }

    /// 清理过期密钥.
    pub async fn clean_expired(&self) -> usize {
        let mut keys = self.active_keys.write().await;
        let now = chrono::Utc::now();
        let expired: Vec<String> = keys
            .iter()
            .filter(|(_, e)| {
                if let Some(expires_at) = e.expires_at {
                    now > expires_at
                } else {
                    false
                }
            })
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired.len();
        for key in expired {
            keys.remove(&key);
        }
        count
    }

    /// 计算密钥哈希 (SHA-256).
    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let result = hasher.finalize();
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(result)
    }
}

impl Default for ApiKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_and_verify() {
        let mgr = ApiKeyManager::new();
        let (key, entry) = mgr.generate("test", "user-1", vec!["admin".into()], None).await.unwrap();

        assert!(key.starts_with("ls_test_"));
        assert_eq!(entry.owner, "user-1");

        let verified = mgr.verify(&key).await.unwrap();
        assert_eq!(verified.key_id, entry.key_id);
    }

    #[tokio::test]
    async fn test_verify_invalid_key() {
        let mgr = ApiKeyManager::new();
        let err = mgr.verify("ls_invalid_key").await;
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), LsError::AuthenticationFailed(_)));
    }

    #[tokio::test]
    async fn test_revoke_key() {
        let mgr = ApiKeyManager::new();
        let (key, entry) = mgr.generate("test", "user-1", vec![], None).await.unwrap();

        mgr.revoke(&entry.key_id).await.unwrap();
        let err = mgr.verify(&key).await;
        assert!(err.is_err());

        // Should be in history
        let history = mgr.list_history().await;
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn test_rotate_key() {
        let mgr = ApiKeyManager::new();
        let (old_key, old_entry) = mgr.generate("test", "user-1", vec![], None).await.unwrap();

        let (new_key, new_entry) = mgr
            .rotate(&old_entry.key_id, "test", "user-1", vec!["admin".into()], None)
            .await
            .unwrap();

        // Old key should be revoked
        assert!(mgr.verify(&old_key).await.is_err());
        // New key should work
        assert!(mgr.verify(&new_key).await.is_ok());
        assert_eq!(new_entry.roles, vec!["admin"]);
    }

    #[tokio::test]
    async fn test_expired_key() {
        let mgr = ApiKeyManager::new();
        let (key, _) = mgr.generate("test", "user-1", vec![], Some(0)).await.unwrap();
        // TTL=0 means expired immediately — but may still work if in the same nanosecond
        // Sleep a tiny bit to ensure expiry
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let err = mgr.verify(&key).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_clean_expired() {
        let mgr = ApiKeyManager::new();
        let (_, _) = mgr.generate("test", "u1", vec![], Some(0)).await.unwrap();
        // Sleep to ensure expiry
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let cleaned = mgr.clean_expired().await;
        assert_eq!(cleaned, 1);
    }

    #[tokio::test]
    async fn test_list_active() {
        let mgr = ApiKeyManager::new();
        mgr.generate("test", "u1", vec![], None).await.unwrap();
        mgr.generate("test", "u2", vec![], None).await.unwrap();

        let active = mgr.list_active().await;
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_hash_consistency() {
        let hash1 = ApiKeyManager::hash_key("ls_test_key123");
        let hash2 = ApiKeyManager::hash_key("ls_test_key123");
        assert_eq!(hash1, hash2);
    }
}
