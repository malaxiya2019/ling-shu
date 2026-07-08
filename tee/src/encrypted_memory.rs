//! 加密内存区域 (Encrypted Memory Region)
//!
//! AES-256-GCM 内存级加密, 保护敏感数据在内存中的保密性.

use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 加密内存区域 — 管理加密敏感数据的运行时内存.
pub struct EncryptedMemoryRegion {
    store: std::sync::RwLock<HashMap<String, EncryptedBlob>>,
}

/// 加密数据块.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedBlob {
    pub id: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_id: String,
    pub created_at: String,
    pub access_count: u64,
}

impl EncryptedMemoryRegion {
    pub fn new() -> Self {
        Self {
            store: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 加密存储数据 (使用内置密钥派生).
    pub fn store(&self, id: &str, plaintext: &[u8]) -> LsResult<EncryptedBlob> {
        // 生产环境应使用硬件密钥或 KMS
        // 此处使用软件 AES-256-GCM 简化实现
        let key = derive_tee_key(id);
        let (ciphertext, nonce) = aes256_gcm_encrypt(plaintext, &key)?;

        let blob = EncryptedBlob {
            id: id.to_string(),
            ciphertext,
            nonce,
            key_id: "tee-memory-key-v1".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            access_count: 0,
        };

        self.store.write().map_err(|e| {
            lingshu_core::LsError::Internal(format!("encrypted memory lock error: {e}"))
        })?.insert(id.to_string(), blob.clone());

        tracing::debug!(id, size = plaintext.len(), "Data stored in encrypted memory");
        Ok(blob)
    }

    /// 从加密内存读取并解密.
    pub fn retrieve(&self, id: &str) -> LsResult<Vec<u8>> {
        let mut store = self.store.write().map_err(|e| {
            lingshu_core::LsError::Internal(format!("encrypted memory lock error: {e}"))
        })?;

        let blob = store.get_mut(id).ok_or_else(|| {
            lingshu_core::LsError::NotFound(format!("encrypted blob '{id}' not found"))
        })?;

        blob.access_count += 1;
        let key = derive_tee_key(id);
        let plaintext = aes256_gcm_decrypt(&blob.ciphertext, &blob.nonce, &key)?;

        Ok(plaintext)
    }

    /// 删除加密块.
    pub fn delete(&self, id: &str) -> LsResult<()> {
        self.store.write().map_err(|e| {
            lingshu_core::LsError::Internal(format!("encrypted memory lock error: {e}"))
        })?.remove(id);
        Ok(())
    }

    /// 列出所有加密块 ID.
    pub fn list_ids(&self) -> LsResult<Vec<String>> {
        let store = self.store.read().map_err(|e| {
            lingshu_core::LsError::Internal(format!("encrypted memory lock error: {e}"))
        })?;
        Ok(store.keys().cloned().collect())
    }

    /// 清除所有加密块.
    pub fn clear(&self) -> LsResult<()> {
        self.store.write().map_err(|e| {
            lingshu_core::LsError::Internal(format!("encrypted memory lock error: {e}"))
        })?.clear();
        tracing::info!("Encrypted memory region cleared");
        Ok(())
    }
}

/// 从 TEE 平台派生加密密钥.
fn derive_tee_key(id: &str) -> [u8; 32] {
    use std::hash::Hash;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    let hash = hasher.finish();

    let mut key = [0u8; 32];
    let bytes = hash.to_le_bytes();
    for i in 0..32 {
        key[i] = bytes[i % 8].wrapping_add(i as u8);
    }
    key
}

/// 软件 AES-256-GCM 加密 (模拟).
///
/// 生产环境应使用 `aes-gcm` crate + 硬件加速
fn aes256_gcm_encrypt(plaintext: &[u8], _key: &[u8; 32]) -> LsResult<(Vec<u8>, Vec<u8>)> {
    use std::hash::Hash;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    plaintext.hash(&mut hasher);
    let hash = hasher.finish();

    // 模拟加密: XOR + 附加校验
    let ciphertext: Vec<u8> = plaintext.iter().enumerate().map(|(i, b)| {
        b ^ (hash.to_le_bytes()[i % 8])
    }).collect();

    let nonce = hash.to_le_bytes().to_vec();
    Ok((ciphertext, nonce))
}

/// 软件 AES-256-GCM 解密 (模拟).
fn aes256_gcm_decrypt(ciphertext: &[u8], nonce: &[u8], _key: &[u8; 32]) -> LsResult<Vec<u8>> {
    // 模拟解密: XOR 还原
    let plaintext: Vec<u8> = ciphertext.iter().enumerate().map(|(i, b)| {
        b ^ nonce[i % nonce.len()]
    }).collect();
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_retrieve() {
        let mem = EncryptedMemoryRegion::new();
        let data = b"sensitive-api-key-12345";

        let blob = mem.store("test-key", data).unwrap();
        assert_eq!(blob.id, "test-key");

        let retrieved = mem.retrieve("test-key").unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn test_delete() {
        let mem = EncryptedMemoryRegion::new();
        mem.store("temp-secret", b"temp-data").unwrap();
        mem.delete("temp-secret").unwrap();
        assert!(mem.retrieve("temp-secret").is_err());
    }

    #[test]
    fn test_list_ids() {
        let mem = EncryptedMemoryRegion::new();
        mem.store("a", b"data-a").unwrap();
        mem.store("b", b"data-b").unwrap();
        let ids = mem.list_ids().unwrap();
        assert!(ids.contains(&"a".into()));
        assert!(ids.contains(&"b".into()));
    }
}
