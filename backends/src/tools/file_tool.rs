//! 文件操作工具集 — FileReadTool, FileWriteTool, ListDirTool

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::tool::{Tool, ToolInfo, ToolParam};
use serde_json::Value;
use std::path::PathBuf;

/// 读取文件内容.
pub struct FileReadTool {
    allowed_base: Option<PathBuf>,
}

impl FileReadTool {
    /// 创建文件读取工具.
    ///
    /// `allowed_base` — 限制可读取的根目录，`None` 表示不限制.
    pub fn new(allowed_base: Option<PathBuf>) -> Self {
        Self { allowed_base }
    }

    fn resolve_path(&self, path: &str) -> LsResult<PathBuf> {
        let p = PathBuf::from(path);
        let canonical = p
            .canonicalize()
            .map_err(|e| LsError::Validation(format!("invalid path '{path}': {e}")))?;

        if let Some(ref base) = self.allowed_base {
            let base_canonical = base
                .canonicalize()
                .map_err(|_| LsError::Validation("allowed_base does not exist".into()))?;
            if !canonical.starts_with(&base_canonical) {
                return Err(LsError::Validation(format!(
                    "path '{path}' is outside allowed base directory"
                )));
            }
        }
        Ok(canonical)
    }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "read_file".into(),
            description: "读取指定文件的内容并返回文本。支持任意文本文件。".into(),
            parameters: vec![ToolParam {
                name: "path".into(),
                description: "要读取的文件路径 (绝对路径或相对于工作目录)".into(),
                required: true,
                param_type: "string".into(),
            }],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::Validation("missing required field: path".into()))?;

        if path.is_empty() {
            return Err(LsError::Validation("path must not be empty".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let path = input["path"].as_str().unwrap();
        let resolved = self.resolve_path(path)?;

        let content = tokio::fs::read_to_string(&resolved).await.map_err(|e| {
            LsError::Internal(format!("failed to read file '{}': {e}", resolved.display()))
        })?;

        Ok(serde_json::json!({
            "path": resolved.to_string_lossy(),
            "content": content,
            "size_bytes": content.len(),
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(FileReadTool { allowed_base: self.allowed_base.clone() })
    }}

/// 写入文件内容.
pub struct FileWriteTool {
    allowed_base: Option<PathBuf>,
}

impl FileWriteTool {
    pub fn new(allowed_base: Option<PathBuf>) -> Self {
        Self { allowed_base }
    }

    fn resolve_path(&self, path: &str) -> LsResult<PathBuf> {
        let p = PathBuf::from(path);
        // 相对路径相对于 allowed_base 解析
        let resolved = if p.is_relative() {
            if let Some(ref base) = self.allowed_base {
                base.join(&p)
            } else {
                p
            }
        } else {
            p
        };

        // 安全检查
        if let Some(ref base) = self.allowed_base {
            let base_canonical = base
                .canonicalize()
                .map_err(|_| LsError::Validation("allowed_base does not exist".into()))?;
            // 对 resolved 做 canonicalize，但如果文件还不存在就取其父目录
            let parent = resolved
                .parent()
                .ok_or_else(|| LsError::Validation("invalid path: no parent".into()))?;
            if !parent.exists() {
                return Err(LsError::Validation(format!(
                    "parent directory does not exist: {}",
                    parent.display()
                )));
            }
            let parent_canonical = parent
                .canonicalize()
                .map_err(|_| LsError::Validation("cannot resolve parent directory".into()))?;
            if !parent_canonical.starts_with(&base_canonical) {
                return Err(LsError::Validation(format!(
                    "path '{path}' is outside allowed base directory"
                )));
            }
        }
        Ok(resolved)
    }
}

impl Default for FileWriteTool {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "write_file".into(),
            description: "将内容写入指定文件。如果文件不存在则创建，存在则覆盖。".into(),
            parameters: vec![
                ToolParam {
                    name: "path".into(),
                    description: "要写入的文件路径".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "content".into(),
                    description: "要写入的文件内容".into(),
                    required: true,
                    param_type: "string".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        if input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            return Err(LsError::Validation("missing required field: path".into()));
        }
        if input.get("content").is_none() {
            return Err(LsError::Validation(
                "missing required field: content".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let path = input["path"].as_str().unwrap();
        let content = input["content"].as_str().unwrap_or("");
        let resolved = self.resolve_path(path)?;

        // 确保父目录存在
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                LsError::Internal(format!(
                    "failed to create parent directory '{}': {e}",
                    parent.display()
                ))
            })?;
        }

        tokio::fs::write(&resolved, content).await.map_err(|e| {
            LsError::Internal(format!(
                "failed to write file '{}': {e}",
                resolved.display()
            ))
        })?;

        Ok(serde_json::json!({
            "path": resolved.to_string_lossy(),
            "size_bytes": content.len(),
            "status": "written",
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(FileWriteTool { allowed_base: self.allowed_base.clone() })
    }}

/// 列出目录内容.
pub struct ListDirTool {
    allowed_base: Option<PathBuf>,
}

impl ListDirTool {
    pub fn new(allowed_base: Option<PathBuf>) -> Self {
        Self { allowed_base }
    }

    fn resolve_path(&self, path: &str) -> LsResult<PathBuf> {
        let p = PathBuf::from(path);
        let canonical = if p.is_relative() {
            if let Some(ref base) = self.allowed_base {
                base.join(&p)
            } else {
                std::env::current_dir()
                    .map_err(|e| LsError::Internal(format!("cannot get cwd: {e}")))?
                    .join(&p)
            }
        } else {
            p
        };

        let canonical = canonical
            .canonicalize()
            .map_err(|e| LsError::Validation(format!("invalid path '{path}': {e}")))?;

        if let Some(ref base) = self.allowed_base {
            let base_canonical = base
                .canonicalize()
                .map_err(|_| LsError::Validation("allowed_base does not exist".into()))?;
            if !canonical.starts_with(&base_canonical) {
                return Err(LsError::Validation(format!(
                    "path '{path}' is outside allowed base directory"
                )));
            }
        }
        Ok(canonical)
    }
}

impl Default for ListDirTool {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl Tool for ListDirTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            tool_id: LsId::new(),
            name: "list_dir".into(),
            description: "列出指定目录下的文件和子目录。递归列出所有内容。".into(),
            parameters: vec![
                ToolParam {
                    name: "path".into(),
                    description: "要列出的目录路径".into(),
                    required: true,
                    param_type: "string".into(),
                },
                ToolParam {
                    name: "recursive".into(),
                    description: "是否递归列出子目录 (默认 false)".into(),
                    required: false,
                    param_type: "boolean".into(),
                },
            ],
        ..Default::default()
        }
    }

    fn validate(&self, input: &Value) -> LsResult<()> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LsError::Validation("missing required field: path".into()))?;
        if path.is_empty() {
            return Err(LsError::Validation("path must not be empty".into()));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: LsContext, input: Value) -> LsResult<Value> {
        self.validate(&input)?;
        let path = input["path"].as_str().unwrap();
        let recursive = input
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolved = self.resolve_path(path)?;

        let mut entries = Vec::new();

        if recursive {
            let _walker = tokio::fs::read_dir(&resolved).await.map_err(|e| {
                LsError::Internal(format!(
                    "failed to read directory '{}': {e}",
                    resolved.display()
                ))
            })?;

            let mut stack: Vec<PathBuf> = vec![resolved.clone()];
            while let Some(dir) = stack.pop() {
                let mut read_dir = tokio::fs::read_dir(&dir).await.map_err(|e| {
                    LsError::Internal(format!("failed to read dir '{}': {e}", dir.display()))
                })?;
                while let Some(entry) = read_dir
                    .next_entry()
                    .await
                    .map_err(|e| LsError::Internal(format!("readdir error: {e}")))?
                {
                    let ft = entry
                        .file_type()
                        .await
                        .map_err(|e| LsError::Internal(format!("file type error: {e}")))?;
                    let is_dir = ft.is_dir();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let full_path = entry.path().to_string_lossy().to_string();
                    entries.push(serde_json::json!({
                        "name": name,
                        "path": full_path,
                        "is_dir": is_dir,
                    }));
                    if is_dir {
                        stack.push(entry.path());
                    }
                }
            }
        } else {
            let mut read_dir = tokio::fs::read_dir(&resolved).await.map_err(|e| {
                LsError::Internal(format!(
                    "failed to read directory '{}': {e}",
                    resolved.display()
                ))
            })?;

            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|e| LsError::Internal(format!("readdir error: {e}")))?
            {
                let ft = entry
                    .file_type()
                    .await
                    .map_err(|e| LsError::Internal(format!("file type error: {e}")))?;
                let name = entry.file_name().to_string_lossy().to_string();
                let full_path = entry.path().to_string_lossy().to_string();
                entries.push(serde_json::json!({
                    "name": name,
                    "path": full_path,
                    "is_dir": ft.is_dir(),
                }));
            }
        }

        Ok(serde_json::json!({
            "path": resolved.to_string_lossy(),
            "entries": entries,
            "total": entries.len(),
            "recursive": recursive,
        }))
    }

    fn duplicate(&self) -> Box<dyn Tool> {
        Box::new(ListDirTool { allowed_base: self.allowed_base.clone() })
    }}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsContext;
    use serde_json::json;
    use tempfile::TempDir;

    fn test_ctx() -> LsContext {
        LsContext::with_session(LsId::new())
    }

    #[tokio::test]
    async fn test_read_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, LingShu!").unwrap();

        let tool = FileReadTool::new(Some(dir.path().to_path_buf()));
        let result = tool
            .execute(test_ctx(), json!({"path": file_path.to_string_lossy()}))
            .await
            .unwrap();
        assert_eq!(result["content"], "Hello, LingShu!");
        assert_eq!(result["size_bytes"], 15);
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tool = FileReadTool::default();
        let result = tool
            .execute(
                test_ctx(),
                json!({"path": "/tmp/nonexistent_file_12345.txt"}),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_file_path_traversal() {
        let dir = TempDir::new().unwrap();
        let tool = FileReadTool::new(Some(dir.path().to_path_buf()));
        let result = tool
            .execute(test_ctx(), json!({"path": "/etc/passwd"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("output.txt");

        let tool = FileWriteTool::new(Some(dir.path().to_path_buf()));
        let result = tool
            .execute(
                test_ctx(),
                json!({"path": file_path.to_string_lossy(), "content": "new content"}),
            )
            .await
            .unwrap();
        assert_eq!(result["status"], "written");

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_write_file_outside_base() {
        let dir = TempDir::new().unwrap();
        let tool = FileWriteTool::new(Some(dir.path().to_path_buf()));
        let result = tool
            .execute(
                test_ctx(),
                json!({"path": "/tmp/evil.txt", "content": "evil"}),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.txt"), "c").unwrap();

        let tool = ListDirTool::new(Some(dir.path().to_path_buf()));
        let result = tool
            .execute(test_ctx(), json!({"path": dir.path().to_string_lossy()}))
            .await
            .unwrap();
        assert_eq!(result["total"], 3);

        let names: Vec<String> = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"b.txt".to_string()));
        assert!(names.contains(&"sub".to_string()));
    }

    #[tokio::test]
    async fn test_list_dir_recursive() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.txt"), "c").unwrap();

        let tool = ListDirTool::new(Some(dir.path().to_path_buf()));
        let result = tool
            .execute(
                test_ctx(),
                json!({"path": dir.path().to_string_lossy(), "recursive": true}),
            )
            .await
            .unwrap();
        // a.txt + sub + sub/c.txt = 3
        assert_eq!(result["total"], 3);
    }
}
