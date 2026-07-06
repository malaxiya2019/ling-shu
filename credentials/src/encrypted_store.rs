//! 加密存储层 — AES-256-GCM 加密 + SQLite 持久化

use crate::types::*;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use lingshu_core::LsResult;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

/// 加密凭证存储.
pub struct CredentialStore {
    conn: Mutex<rusqlite::Connection>,
    cipher: Aes256Gcm,
}

impl CredentialStore {
    /// 打开或创建凭证数据库.
    pub fn open(db_path: &Path, master_key: &str) -> LsResult<Self> {
        let conn = rusqlite::Connection::open(db_path)
            .map_err(|e| lingshu_core::LsError::Internal(format!("open credentials db: {e}")))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS credentials (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                credential_type TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                encrypted_token TEXT NOT NULL,
                nonce TEXT NOT NULL,
                username TEXT,
                base_url TEXT,
                scopes TEXT NOT NULL DEFAULT '[]',
                permissions_group TEXT,
                expires_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| lingshu_core::LsError::Internal(format!("init credentials table: {e}")))?;

        // Derive AES-256 key from master key
        let key = Sha256::digest(master_key.as_bytes());
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| lingshu_core::LsError::Internal(format!("init cipher: {e}")))?;

        Ok(Self {
            conn: Mutex::new(conn),
            cipher,
        })
    }

    /// 加密 token.
    fn encrypt(&self, plaintext: &str) -> LsResult<(String, String)> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| lingshu_core::LsError::Internal(format!("encrypt: {e}")))?;
        Ok((
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &ciphertext),
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce_bytes),
        ))
    }

    /// 解密 token.
    fn decrypt(&self, encrypted_token: &str, nonce_b64: &str) -> LsResult<String> {
        let ciphertext =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted_token)
                .map_err(|e| lingshu_core::LsError::Internal(format!("decode ciphertext: {e}")))?;
        let nonce_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, nonce_b64)
                .map_err(|e| lingshu_core::LsError::Internal(format!("decode nonce: {e}")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| lingshu_core::LsError::Internal(format!("decrypt: {e}")))?;
        String::from_utf8(plaintext)
            .map_err(|e| lingshu_core::LsError::Internal(format!("utf8: {e}")))
    }

    /// 插入凭证.
    pub fn insert(&self, entry: &CredentialEntry) -> LsResult<()> {
        let (enc_token, nonce) = self.encrypt(&entry.token)?;
        let scopes = serde_json::to_string(&entry.scopes).unwrap_or_else(|_| "[]".into());

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO credentials (id, provider, credential_type, name, description, encrypted_token, nonce, username, base_url, scopes, permissions_group, expires_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                entry.id,
                entry.provider.as_str(),
                entry.credential_type.as_str(),
                entry.name,
                entry.description,
                enc_token,
                nonce,
                entry.username,
                entry.base_url,
                scopes,
                entry.permissions_group,
                entry.expires_at,
                entry.created_at,
                entry.updated_at,
            ],
        ).map_err(|e| lingshu_core::LsError::Internal(format!("insert credential: {e}")))?;
        Ok(())
    }

    /// 获取凭证（已解密）.
    pub fn get(&self, id: &str) -> LsResult<Option<CredentialEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider, credential_type, name, description, encrypted_token, nonce, username, base_url, scopes, permissions_group, expires_at, created_at, updated_at
             FROM credentials WHERE id = ?1"
        ).map_err(|e| lingshu_core::LsError::Internal(format!("prepare get: {e}")))?;

        let mut rows = stmt
            .query(rusqlite::params![id])
            .map_err(|e| lingshu_core::LsError::Internal(format!("query get: {e}")))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?
        {
            let enc_token: String = row
                .get(5)
                .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
            let nonce: String = row
                .get(6)
                .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
            let token = self.decrypt(&enc_token, &nonce)?;
            let provider_str: String = row
                .get(1)
                .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
            let ct_str: String = row
                .get(2)
                .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
            let scopes_str: String = row
                .get(9)
                .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?;
            let scopes: Vec<String> = serde_json::from_str(&scopes_str).unwrap_or_default();

            Ok(Some(CredentialEntry {
                id: row
                    .get(0)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                provider: GitProvider::from_str(&provider_str).unwrap_or(GitProvider::Gitee),
                credential_type: CredentialType::from_str(&ct_str)
                    .unwrap_or(CredentialType::PersonalAccessToken),
                name: row
                    .get(3)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                description: row
                    .get(4)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                token,
                username: row
                    .get(7)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                base_url: row
                    .get(8)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                scopes,
                permissions_group: row
                    .get(10)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                expires_at: row
                    .get(11)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                created_at: row
                    .get(12)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
                updated_at: row
                    .get(13)
                    .map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?,
            }))
        } else {
            Ok(None)
        }
    }

    /// 列出所有凭证（摘要，不暴露 token）.
    pub fn list(&self) -> LsResult<Vec<CredentialSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider, credential_type, name, description, encrypted_token, username, base_url, scopes, permissions_group, expires_at, created_at, updated_at
             FROM credentials ORDER BY created_at DESC"
        ).map_err(|e| lingshu_core::LsError::Internal(format!("prepare list: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let enc_token: String = row.get(5)?;
                let masked = if enc_token.len() > 8 {
                    format!(
                        "{}...{}",
                        &enc_token[..4],
                        &enc_token[enc_token.len() - 4..]
                    )
                } else {
                    "***".into()
                };
                let scopes_str: String = row.get(8)?;
                let scopes: Vec<String> = serde_json::from_str(&scopes_str).unwrap_or_default();
                Ok(CredentialSummary {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    credential_type: row.get(2)?,
                    name: row.get(3)?,
                    description: row.get(4)?,
                    masked_token: masked,
                    username: row.get(6)?,
                    base_url: row.get(7)?,
                    scopes,
                    permissions_group: row.get(9)?,
                    expires_at: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })
            .map_err(|e| lingshu_core::LsError::Internal(format!("query list: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?);
        }
        Ok(results)
    }

    /// 更新凭证.
    pub fn update(&self, id: &str, req: &UpdateCredentialRequest) -> LsResult<bool> {
        let entry = self.get(id)?;
        let entry = match entry {
            Some(e) => e,
            None => return Ok(false),
        };

        let new_token = req.token.as_deref().unwrap_or(&entry.token);
        let (enc_token, nonce) = self.encrypt(new_token)?;
        let scopes = req
            .scopes
            .as_ref()
            .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "[]".into()))
            .unwrap_or_else(|| {
                serde_json::to_string(&entry.scopes).unwrap_or_else(|_| "[]".into())
            });
        let now = chrono::Utc::now().timestamp();

        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE credentials SET name=?1, description=?2, encrypted_token=?3, nonce=?4, username=?5, base_url=?6, scopes=?7, permissions_group=?8, expires_at=?9, updated_at=?10 WHERE id=?11",
            rusqlite::params![
                req.name.as_deref().unwrap_or(&entry.name),
                req.description.as_deref().unwrap_or(&entry.description),
                enc_token,
                nonce,
                req.username.as_deref().or(entry.username.as_deref()),
                req.base_url.as_deref().or(entry.base_url.as_deref()),
                scopes,
                req.permissions_group.as_deref().or(entry.permissions_group.as_deref()),
                req.expires_at.or(entry.expires_at),
                now,
                id,
            ],
        ).map_err(|e| lingshu_core::LsError::Internal(format!("update credential: {e}")))?;
        Ok(affected > 0)
    }

    /// 删除凭证.
    pub fn delete(&self, id: &str) -> LsResult<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM credentials WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| lingshu_core::LsError::Internal(format!("delete credential: {e}")))?;
        Ok(affected > 0)
    }

    /// 按提供商列出.
    pub fn list_by_provider(&self, provider: &str) -> LsResult<Vec<CredentialSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, provider, credential_type, name, description, encrypted_token, username, base_url, scopes, permissions_group, expires_at, created_at, updated_at
             FROM credentials WHERE provider = ?1 ORDER BY created_at DESC"
        ).map_err(|e| lingshu_core::LsError::Internal(format!("prepare list_by_provider: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![provider], |row| {
                let enc_token: String = row.get(5)?;
                let masked = if enc_token.len() > 8 {
                    format!(
                        "{}...{}",
                        &enc_token[..4],
                        &enc_token[enc_token.len() - 4..]
                    )
                } else {
                    "***".into()
                };
                let scopes_str: String = row.get(8)?;
                let scopes: Vec<String> = serde_json::from_str(&scopes_str).unwrap_or_default();
                Ok(CredentialSummary {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    credential_type: row.get(2)?,
                    name: row.get(3)?,
                    description: row.get(4)?,
                    masked_token: masked,
                    username: row.get(6)?,
                    base_url: row.get(7)?,
                    scopes,
                    permissions_group: row.get(9)?,
                    expires_at: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })
            .map_err(|e| lingshu_core::LsError::Internal(format!("query list_by_provider: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| lingshu_core::LsError::Internal(e.to_string()))?);
        }
        Ok(results)
    }
}
