use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 文件元信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_id: LsId,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub path: String,
    pub metadata: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 预签名 URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignedUrl {
    pub url: String,
    pub method: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Storage — 文件上传下载、权限控制、预签名生成.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// 上传文件 (bytes).
    async fn upload(&self, ctx: LsContext, filename: &str, content_type: &str, data: Vec<u8>) -> LsResult<FileInfo>;

    /// 下载文件.
    async fn download(&self, ctx: LsContext, file_id: LsId) -> LsResult<(FileInfo, Vec<u8>)>;

    /// 删除文件.
    async fn delete(&self, ctx: LsContext, file_id: LsId) -> LsResult<()>;

    /// 获取文件信息.
    async fn info(&self, ctx: LsContext, file_id: LsId) -> LsResult<FileInfo>;

    /// 生成预签名上传 URL.
    async fn presigned_upload(&self, ctx: LsContext, filename: &str, content_type: &str, ttl_seconds: u64) -> LsResult<PresignedUrl>;

    /// 生成预签名下载 URL.
    async fn presigned_download(&self, ctx: LsContext, file_id: LsId, ttl_seconds: u64) -> LsResult<PresignedUrl>;
}
