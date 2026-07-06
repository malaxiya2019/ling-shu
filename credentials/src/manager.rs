//! CredentialManager — 凭证管理 + 提供商验证

use crate::encrypted_store::CredentialStore;
use crate::types::*;
use lingshu_core::LsResult;
use std::sync::Arc;

/// 凭证管理器.
pub struct CredentialManager {
    store: Arc<CredentialStore>,
}

impl CredentialManager {
    pub fn new(store: Arc<CredentialStore>) -> Self {
        Self { store }
    }

    /// 创建凭证（含验证）.
    pub async fn create(&self, req: CreateCredentialRequest) -> LsResult<CredentialSummary> {
        let entry = self.build_entry(req)?;
        // Validate before saving
        let validation = self.validate_inner(&entry).await;
        if !validation.valid {
            return Err(lingshu_core::LsError::Internal(format!(
                "credential validation failed: {}",
                validation.message
            )));
        }
        self.store.insert(&entry)?;
        self.get_summary(&entry.id)
    }

    /// 创建凭证（跳过 API 验证，仅加密存储）.
    pub fn create_without_validate(
        &self,
        req: CreateCredentialRequest,
    ) -> LsResult<CredentialSummary> {
        let entry = self.build_entry(req)?;
        self.store.insert(&entry)?;
        self.get_summary(&entry.id)
    }

    /// 从请求构建 CredentialEntry.
    fn build_entry(&self, req: CreateCredentialRequest) -> LsResult<CredentialEntry> {
        let provider = GitProvider::from_str(&req.provider).ok_or_else(|| {
            lingshu_core::LsError::Internal(format!("unknown provider: {}", req.provider))
        })?;
        let ct = CredentialType::from_str(&req.credential_type).ok_or_else(|| {
            lingshu_core::LsError::Internal(format!(
                "unknown credential type: {}",
                req.credential_type
            ))
        })?;

        let now = chrono::Utc::now().timestamp();
        let id = uuid::Uuid::new_v4().to_string();

        Ok(CredentialEntry {
            id: id.clone(),
            provider,
            credential_type: ct,
            name: req.name,
            description: req.description.unwrap_or_default(),
            token: req.token,
            username: req.username,
            base_url: req.base_url,
            scopes: req.scopes.unwrap_or_default(),
            permissions_group: req.permissions_group,
            expires_at: req.expires_at,
            created_at: now,
            updated_at: now,
        })
    }

    /// 获取凭证（含 token，用于 API 调用）.
    pub fn get_token(&self, id: &str) -> LsResult<Option<CredentialEntry>> {
        self.store.get(id)
    }

    /// 获取凭证摘要（不含 token）.
    pub fn get_summary(&self, id: &str) -> LsResult<CredentialSummary> {
        let entry = self.store.get(id)?.ok_or_else(|| {
            lingshu_core::LsError::Internal(format!("credential not found: {id}"))
        })?;
        let masked = if entry.token.len() > 8 {
            format!(
                "{}...{}",
                &entry.token[..4],
                &entry.token[entry.token.len() - 4..]
            )
        } else {
            "***".into()
        };
        Ok(CredentialSummary {
            id: entry.id,
            provider: entry.provider.as_str().to_string(),
            credential_type: entry.credential_type.as_str().to_string(),
            name: entry.name,
            description: entry.description,
            masked_token: masked,
            username: entry.username,
            base_url: entry.base_url,
            scopes: entry.scopes,
            permissions_group: entry.permissions_group,
            expires_at: entry.expires_at,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        })
    }

    /// 列表.
    pub fn list(&self) -> LsResult<Vec<CredentialSummary>> {
        self.store.list()
    }

    /// 按提供商列表.
    pub fn list_by_provider(&self, provider: &str) -> LsResult<Vec<CredentialSummary>> {
        self.store.list_by_provider(provider)
    }

    /// 更新.
    pub async fn update(&self, id: &str, req: UpdateCredentialRequest) -> LsResult<bool> {
        self.store.update(id, &req)
    }

    /// 删除.
    pub fn delete(&self, id: &str) -> LsResult<bool> {
        self.store.delete(id)
    }

    /// 验证凭证（对目标提供商 API 做一次实际调用）.
    pub async fn validate(&self, id: &str) -> LsResult<CredentialValidation> {
        let entry = self.store.get(id)?.ok_or_else(|| {
            lingshu_core::LsError::Internal(format!("credential not found: {id}"))
        })?;
        Ok(self.validate_inner(&entry).await)
    }

    /// 内部验证逻辑.
    async fn validate_inner(&self, entry: &CredentialEntry) -> CredentialValidation {
        match entry.provider {
            GitProvider::Gitee => validate_gitee(entry).await,
            GitProvider::Codeup => validate_codeup(entry).await,
            GitProvider::Coding => validate_coding(entry).await,
            GitProvider::GitCode => validate_gitcode(entry).await,
            GitProvider::Cnb => validate_cnb(entry).await,
        }
    }
}

// ── 各提供商验证函数 ──────────────────────────────────────

/// 验证 Gitee 凭证: GET https://gitee.com/api/v5/user
async fn validate_gitee(entry: &CredentialEntry) -> CredentialValidation {
    let url = entry
        .base_url
        .as_deref()
        .unwrap_or("https://gitee.com/api/v5/user");
    match validate_with_get(url, &entry.token).await {
        Ok(scopes) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "gitee".into(),
            valid: true,
            message: "凭证有效".into(),
            scopes_verified: scopes,
        },
        Err(msg) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "gitee".into(),
            valid: false,
            message: msg,
            scopes_verified: vec![],
        },
    }
}

/// 验证 Codeup 凭证: GET https://codeup.aliyun.com/api/v1/user
async fn validate_codeup(entry: &CredentialEntry) -> CredentialValidation {
    let url = entry
        .base_url
        .as_deref()
        .unwrap_or("https://codeup.aliyun.com/api/v1/user");
    match validate_with_get(url, &entry.token).await {
        Ok(scopes) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "codeup".into(),
            valid: true,
            message: "凭证有效".into(),
            scopes_verified: scopes,
        },
        Err(msg) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "codeup".into(),
            valid: false,
            message: msg,
            scopes_verified: vec![],
        },
    }
}

/// 验证 CODING 凭证: GET https://<team>.coding.net/api/current_user
async fn validate_coding(entry: &CredentialEntry) -> CredentialValidation {
    let url = entry
        .base_url
        .as_deref()
        .unwrap_or("https://e.coding.net/api/current_user");
    match validate_with_get(url, &entry.token).await {
        Ok(scopes) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "coding".into(),
            valid: true,
            message: "凭证有效".into(),
            scopes_verified: scopes,
        },
        Err(msg) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "coding".into(),
            valid: false,
            message: msg,
            scopes_verified: vec![],
        },
    }
}

/// 验证 GitCode 凭证: GET https://api.gitcode.com/user
async fn validate_gitcode(entry: &CredentialEntry) -> CredentialValidation {
    let url = entry
        .base_url
        .as_deref()
        .unwrap_or("https://api.gitcode.com/user");
    match validate_with_get(url, &entry.token).await {
        Ok(scopes) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "gitcode".into(),
            valid: true,
            message: "凭证有效".into(),
            scopes_verified: scopes,
        },
        Err(msg) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "gitcode".into(),
            valid: false,
            message: msg,
            scopes_verified: vec![],
        },
    }
}

/// 验证 CNB 凭证: GET https://cnb.cool/api/v1/user
async fn validate_cnb(entry: &CredentialEntry) -> CredentialValidation {
    let url = entry
        .base_url
        .as_deref()
        .unwrap_or("https://cnb.cool/api/v1/user");
    match validate_with_get(url, &entry.token).await {
        Ok(scopes) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "cnb".into(),
            valid: true,
            message: "凭证有效".into(),
            scopes_verified: scopes,
        },
        Err(msg) => CredentialValidation {
            id: entry.id.clone(),
            name: entry.name.clone(),
            provider: "cnb".into(),
            valid: false,
            message: msg,
            scopes_verified: vec![],
        },
    }
}

/// 通用 GET 验证.
async fn validate_with_get(url: &str, token: &str) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("create client: {e}"))?;

    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "Lingshu-Credential-Validator/1.0")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if response.status().is_success() {
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        // Try to extract scopes from response headers or body
        let scopes = body
            .get("scopes")
            .or_else(|| body.get("permissions"))
            .or_else(|| body.get("scope"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(scopes)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("HTTP {status}: {body}"))
    }
}
