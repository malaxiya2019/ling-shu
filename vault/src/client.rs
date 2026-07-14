//! HashiCorp Vault HTTP 客户端
//!
//! 支持 KV v2 读写、动态 Secret、健康检查、Token 认证。

use crate::models::*;
use async_trait::async_trait;
use lingshu_core::LsResult;
use tracing::{debug, info, warn};

/// Vault 客户端 Trait.
#[async_trait]
pub trait VaultClientTrait: Send + Sync {
    /// 健康检查.
    async fn health(&self) -> LsResult<VaultHealth>;

    /// 读取 KV Secret (KV v2).
    async fn read_secret(&self, path: &str) -> LsResult<KvSecretResponse>;

    /// 写入 KV Secret (KV v2).
    async fn write_secret(
        &self,
        path: &str,
        data: serde_json::Map<String, serde_json::Value>,
    ) -> LsResult<KvSecretResponse>;

    /// 删除 KV Secret.
    async fn delete_secret(&self, path: &str) -> LsResult<()>;

    /// 列出路径下的密钥.
    async fn list_secrets(&self, path: &str) -> LsResult<Vec<String>>;

    /// 请求动态 Secret (例如数据库/云凭证).
    async fn request_dynamic_secret(&self, path: &str) -> LsResult<DynamicSecret>;

    /// 续约 Lease.
    async fn renew_lease(&self, lease_id: &str, increment: u64) -> LsResult<()>;

    /// 撤销 Lease.
    async fn revoke_lease(&self, lease_id: &str) -> LsResult<()>;

    /// Transit — 加密数据.
    async fn encrypt(&self, key_name: &str, plaintext_base64: &str) -> LsResult<String>;

    /// Transit — 解密数据.
    async fn decrypt(&self, key_name: &str, ciphertext: &str) -> LsResult<String>;
}

/// HashiCorp Vault HTTP 客户端.
pub struct VaultClient {
    config: VaultConfig,
    client: reqwest::Client,
}

impl VaultClient {
    /// 创建新的 Vault 客户端.
    pub fn new(config: VaultConfig) -> LsResult<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("lingshu-vault/3.3");

        if config.tls_skip_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build().map_err(|e| {
            lingshu_core::LsError::Internal(format!("failed to build HTTP client: {e}"))
        })?;

        Ok(Self { config, client })
    }

    /// 获取带认证头的请求构造器.
    #[allow(dead_code)]
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.config.token)
    }

    /// 构建 KV v2 读取路径: /v1/{engine}/data/{path}
    fn kv_read_path(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/data/{}",
            self.config.address, self.config.kv_engine_path, path
        )
    }

    /// 构建 KV v2 写入路径.
    fn kv_write_path(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/data/{}",
            self.config.address, self.config.kv_engine_path, path
        )
    }

    /// 构建 KV v2 删除路径.
    fn kv_delete_path(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/data/{}",
            self.config.address, self.config.kv_engine_path, path
        )
    }

    /// 构建 list 路径.
    fn kv_list_path(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/metadata/{}",
            self.config.address, self.config.kv_engine_path, path
        )
    }

    /// 构建 health 路径.
    fn health_path(&self) -> String {
        format!("{}/v1/sys/health", self.config.address)
    }

    /// 构建动态 Secret 路径.
    fn dynamic_secret_path(&self, path: &str) -> String {
        format!("{}/v1/{}", self.config.address, path)
    }

    /// 构建 lease renew 路径.
    fn renew_lease_path(&self) -> String {
        format!("{}/v1/sys/leases/renew", self.config.address)
    }

    /// 构建 lease revoke 路径.
    fn revoke_lease_path(&self) -> String {
        format!("{}/v1/sys/leases/revoke", self.config.address)
    }

    /// 构建 transit encrypt 路径.
    fn transit_encrypt_path(&self, key_name: &str) -> String {
        let engine = self
            .config
            .transit_engine_path
            .as_deref()
            .unwrap_or("transit");
        format!("{}/v1/{}/encrypt/{}", self.config.address, engine, key_name)
    }

    /// 构建 transit decrypt 路径.
    fn transit_decrypt_path(&self, key_name: &str) -> String {
        let engine = self
            .config
            .transit_engine_path
            .as_deref()
            .unwrap_or("transit");
        format!("{}/v1/{}/decrypt/{}", self.config.address, engine, key_name)
    }
}

#[async_trait]
impl VaultClientTrait for VaultClient {
    async fn health(&self) -> LsResult<VaultHealth> {
        let resp = self
            .client
            .get(self.health_path())
            .header("X-Vault-Token", &self.config.token)
            .send()
            .await
            .map_err(|e| {
                lingshu_core::LsError::Internal(format!("vault health check failed: {e}"))
            })?;

        let health: VaultHealth = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault health parse failed: {e}"))
        })?;

        info!(
            initialized = health.initialized,
            sealed = health.sealed,
            "Vault health check"
        );
        Ok(health)
    }

    async fn read_secret(&self, path: &str) -> LsResult<KvSecretResponse> {
        let resp = self
            .client
            .get(self.kv_read_path(path))
            .header("X-Vault-Token", &self.config.token)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("vault read failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(lingshu_core::LsError::Internal(format!(
                "vault read secret '{path}' failed: {status} — {body}"
            )));
        }

        let secret: KvSecretResponse = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault read parse failed: {e}"))
        })?;

        debug!(path, version = secret.metadata.version, "Vault secret read");
        Ok(secret)
    }

    async fn write_secret(
        &self,
        path: &str,
        data: serde_json::Map<String, serde_json::Value>,
    ) -> LsResult<KvSecretResponse> {
        let body = serde_json::json!({ "data": data, "options": { "cas": 0 } });

        let resp = self
            .client
            .post(self.kv_write_path(path))
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("vault write failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(lingshu_core::LsError::Internal(format!(
                "vault write secret '{path}' failed: {status} — {body_text}"
            )));
        }

        let secret: KvSecretResponse = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault write parse failed: {e}"))
        })?;

        info!(
            path,
            version = secret.metadata.version,
            "Vault secret written"
        );
        Ok(secret)
    }

    async fn delete_secret(&self, path: &str) -> LsResult<()> {
        let resp = self
            .client
            .delete(self.kv_delete_path(path))
            .header("X-Vault-Token", &self.config.token)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("vault delete failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(lingshu_core::LsError::Internal(format!(
                "vault delete secret '{path}' failed: {status}"
            )));
        }

        info!(path, "Vault secret deleted");
        Ok(())
    }

    async fn list_secrets(&self, path: &str) -> LsResult<Vec<String>> {
        let resp = self
            .client
            .request(
                reqwest::Method::from_bytes(b"LIST").unwrap(),
                self.kv_list_path(path),
            )
            .header("X-Vault-Token", &self.config.token)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("vault list failed: {e}")))?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        // Vault list returns { "data": { "keys": [...] } }
        #[derive(serde::Deserialize)]
        struct ListResponse {
            data: ListData,
        }
        #[derive(serde::Deserialize)]
        struct ListData {
            keys: Vec<String>,
        }

        let list: ListResponse = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault list parse failed: {e}"))
        })?;

        Ok(list.data.keys)
    }

    async fn request_dynamic_secret(&self, path: &str) -> LsResult<DynamicSecret> {
        let resp = self
            .client
            .get(self.dynamic_secret_path(path))
            .header("X-Vault-Token", &self.config.token)
            .send()
            .await
            .map_err(|e| {
                lingshu_core::LsError::Internal(format!("vault dynamic secret request failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(lingshu_core::LsError::Internal(format!(
                "vault dynamic secret '{path}' failed: {status}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct DynResp {
            lease_id: String,
            lease_duration: u64,
            data: serde_json::Map<String, serde_json::Value>,
        }

        let dyn_resp: DynResp = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault dynamic secret parse failed: {e}"))
        })?;

        Ok(DynamicSecret {
            lease_id: dyn_resp.lease_id,
            lease_duration: dyn_resp.lease_duration,
            renew: true,
            data: dyn_resp.data,
        })
    }

    async fn renew_lease(&self, lease_id: &str, increment: u64) -> LsResult<()> {
        let body = serde_json::json!({
            "lease_id": lease_id,
            "increment": increment,
        });

        let resp = self
            .client
            .put(self.renew_lease_path())
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                lingshu_core::LsError::Internal(format!("vault renew lease failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            warn!(lease_id, status = %status, "Vault lease renew failed");
            return Err(lingshu_core::LsError::Internal(format!(
                "vault renew lease failed: {status}"
            )));
        }

        info!(lease_id, "Vault lease renewed");
        Ok(())
    }

    async fn revoke_lease(&self, lease_id: &str) -> LsResult<()> {
        let body = serde_json::json!({ "lease_id": lease_id });

        let resp = self
            .client
            .put(self.revoke_lease_path())
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                lingshu_core::LsError::Internal(format!("vault revoke lease failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            warn!(lease_id, status = %status, "Vault lease revoke failed");
            return Err(lingshu_core::LsError::Internal(format!(
                "vault revoke lease failed: {status}"
            )));
        }

        info!(lease_id, "Vault lease revoked");
        Ok(())
    }

    async fn encrypt(&self, key_name: &str, plaintext_base64: &str) -> LsResult<String> {
        let body = serde_json::json!({
            "plaintext": plaintext_base64,
        });

        let resp = self
            .client
            .post(self.transit_encrypt_path(key_name))
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("vault encrypt failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(lingshu_core::LsError::Internal(format!(
                "vault encrypt with key '{key_name}' failed: {status}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct EncryptResp {
            data: EncryptData,
        }
        #[derive(serde::Deserialize)]
        struct EncryptData {
            ciphertext: String,
        }

        let enc: EncryptResp = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault encrypt parse failed: {e}"))
        })?;

        Ok(enc.data.ciphertext)
    }

    async fn decrypt(&self, key_name: &str, ciphertext: &str) -> LsResult<String> {
        let body = serde_json::json!({
            "ciphertext": ciphertext,
        });

        let resp = self
            .client
            .post(self.transit_decrypt_path(key_name))
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("vault decrypt failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(lingshu_core::LsError::Internal(format!(
                "vault decrypt with key '{key_name}' failed: {status}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct DecryptResp {
            data: DecryptData,
        }
        #[derive(serde::Deserialize)]
        struct DecryptData {
            plaintext: String,
        }

        let dec: DecryptResp = resp.json().await.map_err(|e| {
            lingshu_core::LsError::Internal(format!("vault decrypt parse failed: {e}"))
        })?;

        Ok(dec.data.plaintext)
    }
}

/// 内存 Mock Vault 客户端 (用于测试).
pub struct MockVaultClient {
    secrets: std::sync::RwLock<
        std::collections::HashMap<String, serde_json::Map<String, serde_json::Value>>,
    >,
}

impl Default for MockVaultClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVaultClient {
    pub fn new() -> Self {
        Self {
            secrets: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl VaultClientTrait for MockVaultClient {
    async fn health(&self) -> LsResult<VaultHealth> {
        Ok(VaultHealth {
            initialized: true,
            sealed: false,
            standby: false,
            cluster_name: Some("mock-cluster".into()),
            cluster_id: Some("mock-id".into()),
        })
    }

    async fn read_secret(&self, path: &str) -> LsResult<KvSecretResponse> {
        let secrets = self
            .secrets
            .read()
            .map_err(|e| lingshu_core::LsError::Internal(format!("mock vault lock error: {e}")))?;
        match secrets.get(path) {
            Some(data) => {
                let mut md = serde_json::Map::new();
                md.insert("data".into(), serde_json::Value::Object(data.clone()));
                Ok(KvSecretResponse {
                    data: KvSecretDataInner { data: data.clone() },
                    metadata: KvMetadata {
                        created_time: chrono::Utc::now().to_rfc3339(),
                        version: 1,
                        destroyed: false,
                    },
                })
            }
            None => Err(lingshu_core::LsError::NotFound(format!(
                "secret '{path}' not found"
            ))),
        }
    }

    async fn write_secret(
        &self,
        path: &str,
        data: serde_json::Map<String, serde_json::Value>,
    ) -> LsResult<KvSecretResponse> {
        let mut secrets = self
            .secrets
            .write()
            .map_err(|e| lingshu_core::LsError::Internal(format!("mock vault lock error: {e}")))?;
        secrets.insert(path.to_string(), data.clone());
        Ok(KvSecretResponse {
            data: KvSecretDataInner { data },
            metadata: KvMetadata {
                created_time: chrono::Utc::now().to_rfc3339(),
                version: 1,
                destroyed: false,
            },
        })
    }

    async fn delete_secret(&self, path: &str) -> LsResult<()> {
        let mut secrets = self
            .secrets
            .write()
            .map_err(|e| lingshu_core::LsError::Internal(format!("mock vault lock error: {e}")))?;
        secrets.remove(path);
        Ok(())
    }

    async fn list_secrets(&self, path: &str) -> LsResult<Vec<String>> {
        let secrets = self
            .secrets
            .read()
            .map_err(|e| lingshu_core::LsError::Internal(format!("mock vault lock error: {e}")))?;
        let keys: Vec<String> = secrets
            .keys()
            .filter(|k| k.starts_with(path))
            .map(|k| {
                k.trim_start_matches(path)
                    .trim_start_matches('/')
                    .to_string()
            })
            .collect();
        Ok(keys)
    }

    async fn request_dynamic_secret(&self, _path: &str) -> LsResult<DynamicSecret> {
        let mut data = serde_json::Map::new();
        data.insert(
            "username".into(),
            serde_json::Value::String("mock_user".into()),
        );
        data.insert(
            "password".into(),
            serde_json::Value::String("mock_pass".into()),
        );
        Ok(DynamicSecret {
            lease_id: uuid::Uuid::new_v4().to_string(),
            lease_duration: 3600,
            renew: true,
            data,
        })
    }

    async fn renew_lease(&self, lease_id: &str, _increment: u64) -> LsResult<()> {
        info!(lease_id, "Mock vault lease renewed");
        Ok(())
    }

    async fn revoke_lease(&self, lease_id: &str) -> LsResult<()> {
        info!(lease_id, "Mock vault lease revoked");
        Ok(())
    }

    async fn encrypt(&self, key_name: &str, plaintext_base64: &str) -> LsResult<String> {
        // Mock: base64 encode as "ciphertext"
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(plaintext_base64);
        Ok(format!("vault:v{}:{}", key_name, encoded))
    }

    async fn decrypt(&self, key_name: &str, ciphertext: &str) -> LsResult<String> {
        // Mock: strip prefix and base64 decode
        let prefix = format!("vault:v{}:", key_name);
        if let Some(b64) = ciphertext.strip_prefix(&prefix) {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| {
                    lingshu_core::LsError::Internal(format!("mock vault decrypt failed: {e}"))
                })?;
            Ok(String::from_utf8_lossy(&decoded).to_string())
        } else {
            Err(lingshu_core::LsError::Internal(
                "mock vault: invalid ciphertext format".into(),
            ))
        }
    }
}
