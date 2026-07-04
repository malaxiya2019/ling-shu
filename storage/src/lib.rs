//! LSStorage — 文件存储实现。
//!
//! 基于本地文件系统的完整实现，支持上传、下载、删除、元信息查询。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::storage::{FileInfo, PresignedUrl, Storage};
use std::collections::HashMap;
use std::path::PathBuf;

/// 基于本地文件系统的存储实现.
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    /// 创建 LocalStorage 实例.
    ///
    /// # 参数
    /// - `base_path`: 文件存储根目录
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// 返回存储根目录路径.
    pub fn base_path(&self) -> &std::path::PathBuf {
        &self.base_path
    }

    /// 获取文件在磁盘上的完整路径.
    fn resolve_path(&self, relative: &str) -> PathBuf {
        self.base_path.join(relative)
    }

    /// 从磁盘路径解析文件信息 (不读内容).
    fn file_info_from_path(
        file_id: LsId,
        filename: &str,
        content_type: &str,
        path: &str,
        metadata: HashMap<String, String>,
    ) -> LsResult<FileInfo> {
        let full_path = PathBuf::from(path);
        let meta = std::fs::metadata(&full_path).map_err(|e| {
            LsError::Storage(format!("metadata read failed: {e}"))
        })?;

        Ok(FileInfo {
            file_id,
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size: meta.len(),
            path: path.to_string(),
            metadata,
            created_at: meta.created()
                .map(|t| chrono::DateTime::from(t))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn upload(
        &self,
        ctx: LsContext,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> LsResult<FileInfo> {
        let file_id = LsId::new();
        let relative = format!("{}/{}", ctx.session_id, file_id);
        let full_path = self.resolve_path(&relative);

        // 确保目录存在
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| LsError::Storage(format!("create dir failed: {e}")))?;
        }

        // 写入文件
        tokio::fs::write(&full_path, &data).await
            .map_err(|e| LsError::Storage(format!("write failed: {e}")))?;

        // 存储元信息文件
        let meta_path = full_path.with_extension("meta");
        let meta = serde_json::json!({
            "file_id": file_id.to_string(),
            "filename": filename,
            "content_type": content_type,
            "path": relative,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        tokio::fs::write(&meta_path, serde_json::to_string(&meta).unwrap_or_default()).await
            .map_err(|e| LsError::Storage(format!("write meta failed: {e}")))?;

        Ok(FileInfo {
            file_id,
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size: data.len() as u64,
            path: relative,
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        })
    }

    async fn download(&self, _ctx: LsContext, file_id: LsId) -> LsResult<(FileInfo, Vec<u8>)> {
        // 查找匹配的文件
        let _pattern = format!("*/{}", file_id);
        let mut found = None;

        let mut read_dir = tokio::fs::read_dir(&self.base_path).await
            .map_err(|e| LsError::Storage(format!("read dir failed: {e}")))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| LsError::Storage(format!("read entry failed: {e}")))? 
        {
            let path = entry.path();
            if path.is_dir() {
                let mut sub_dir = tokio::fs::read_dir(&path).await
                    .map_err(|e| LsError::Storage(format!("read subdir failed: {e}")))?;
                while let Some(sub_entry) = sub_dir.next_entry().await
                    .map_err(|e| LsError::Storage(format!("read entry failed: {e}")))? 
                {
                    let sub_path = sub_entry.path();
                    if sub_path.is_file()
                        && sub_path.extension().and_then(|e| e.to_str()) != Some("meta")
                        && sub_path.file_stem().and_then(|s| s.to_str()) == Some(&file_id.to_string())
                    {
                        found = Some(sub_path);
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }

        let file_path = found.ok_or_else(|| {
            LsError::NotFound(format!("file not found: {file_id}"))
        })?;

        let data = tokio::fs::read(&file_path).await
            .map_err(|e| LsError::Storage(format!("read failed: {e}")))?;

        let file_name = file_path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let info = Self::file_info_from_path(
            file_id,
            &file_name,
            "application/octet-stream",
            file_path.to_str().unwrap_or(""),
            HashMap::new(),
        )?;

        Ok((info, data))
    }

    async fn delete(&self, _ctx: LsContext, file_id: LsId) -> LsResult<()> {
        let _pattern = format!("*/{}", file_id);
        let mut deleted = false;

        if let Ok(mut read_dir) = tokio::fs::read_dir(&self.base_path).await {
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(mut sub_dir) = tokio::fs::read_dir(&path).await {
                        while let Ok(Some(sub_entry)) = sub_dir.next_entry().await {
                            let sub_path = sub_entry.path();
                            if sub_path.is_file()
                                && sub_path.file_stem().and_then(|s| s.to_str()) == Some(&file_id.to_string())
                            {
                                let _ = tokio::fs::remove_file(&sub_path).await;
                                let meta_path = sub_path.with_extension("meta");
                                let _ = tokio::fs::remove_file(&meta_path).await;
                                deleted = true;
                            }
                        }
                    }
                }
            }
        }

        if !deleted {
            return Err(LsError::NotFound(format!("file not found: {file_id}")));
        }
        Ok(())
    }

    async fn info(&self, _ctx: LsContext, file_id: LsId) -> LsResult<FileInfo> {
        // 尝试从 .meta 文件读取
        if let Ok(mut read_dir) = tokio::fs::read_dir(&self.base_path).await {
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(mut sub_dir) = tokio::fs::read_dir(&path).await {
                        while let Ok(Some(sub_entry)) = sub_dir.next_entry().await {
                            let sub_path = sub_entry.path();
                            if sub_path.extension().and_then(|e| e.to_str()) == Some("meta")
                                && sub_path.file_stem().and_then(|s| s.to_str()) == Some(&file_id.to_string())
                            {
                                let content = tokio::fs::read_to_string(&sub_path).await
                                    .map_err(|e| LsError::Storage(format!("read meta failed: {e}")))?;
                                let meta: serde_json::Value = serde_json::from_str(&content)
                                    .map_err(|e| LsError::Storage(format!("parse meta failed: {e}")))?;

                                return Ok(FileInfo {
                                    file_id,
                                    filename: meta["filename"].as_str().unwrap_or("unknown").to_string(),
                                    content_type: meta["content_type"].as_str().unwrap_or("application/octet-stream").to_string(),
                                    size: tokio::fs::metadata(sub_path.with_extension(""))
                                        .await
                                        .map(|m| m.len())
                                        .unwrap_or(0),
                                    path: meta["path"].as_str().unwrap_or("").to_string(),
                                    metadata: HashMap::new(),
                                    created_at: meta["created_at"].as_str()
                                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                        .map(|dt| dt.with_timezone(&chrono::Utc))
                                        .unwrap_or_else(chrono::Utc::now),
                                });
                            }
                        }
                    }
                }
            }
        }

        Err(LsError::NotFound(format!("file info not found: {file_id}")))
    }

    async fn presigned_upload(
        &self,
        _ctx: LsContext,
        _filename: &str,
        _content_type: &str,
        _ttl_seconds: u64,
    ) -> LsResult<PresignedUrl> {
        // 本地文件系统不支持预签名 URL
        Err(LsError::NotImplemented(
            "presigned_upload requires S3/MinIO backend".into(),
        ))
    }

    async fn presigned_download(
        &self,
        _ctx: LsContext,
        _file_id: LsId,
        _ttl_seconds: u64,
    ) -> LsResult<PresignedUrl> {
        Err(LsError::NotImplemented(
            "presigned_download requires S3/MinIO backend".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use tempfile::TempDir;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_upload_and_download() {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(dir.path());

        let ctx = test_ctx();
        let data = b"hello lingshu storage".to_vec();

        let info = storage
            .upload(ctx.child(), "test.txt", "text/plain", data.clone())
            .await
            .unwrap();

        assert_eq!(info.filename, "test.txt");
        assert_eq!(info.content_type, "text/plain");
        assert_eq!(info.size, data.len() as u64);

        let (dl_info, dl_data) = storage.download(ctx.child(), info.file_id).await.unwrap();
        assert_eq!(dl_data, data);
        assert_eq!(dl_info.file_id, info.file_id);
    }

    #[tokio::test]
    async fn test_info() {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(dir.path());

        let ctx = test_ctx();
        let info = storage
            .upload(ctx.child(), "info_test.txt", "application/json", b"{}".to_vec())
            .await
            .unwrap();

        let meta = storage.info(ctx.child(), info.file_id).await.unwrap();
        assert_eq!(meta.file_id, info.file_id);
        assert_eq!(meta.filename, info.filename);
        assert!(meta.size > 0);
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(dir.path());

        let ctx = test_ctx();
        let info = storage
            .upload(ctx.child(), "delete_me.txt", "text/plain", b"data".to_vec())
            .await
            .unwrap();

        storage.delete(ctx.child(), info.file_id).await.unwrap();

        let result = storage.download(ctx.child(), info.file_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_not_found() {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(dir.path());
        let ctx = test_ctx();
        let result = storage.download(ctx, LsId::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_presigned_not_implemented() {
        let dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(dir.path());
        let ctx = test_ctx();

        let result = storage
            .presigned_upload(ctx.child(), "test.txt", "text/plain", 3600)
            .await;
        assert!(matches!(result.unwrap_err(), LsError::NotImplemented(_)));
    }
}
