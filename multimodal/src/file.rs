//! 文件分析模块 — 文件类型检测、内容提取.

use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};

/// 文件类型分类.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FileType {
    Image,
    Audio,
    Video,
    Text,
    Document,
    Code,
    Archive,
    Data,
    Unknown,
}

/// 文件分析结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// 文件名
    pub name: String,
    /// MIME 类型
    pub mime_type: String,
    /// 文件大小 (字节)
    pub size_bytes: u64,
    /// 文件类型分类
    pub file_type: FileType,
    /// 检测到的编码
    pub encoding: Option<String>,
    /// 文本内容预览 (仅文本/文档类)
    pub preview: Option<String>,
}

/// 文件分析器.
pub struct FileAnalyzer;

impl FileAnalyzer {
    /// 分析文件.
    pub fn analyze(name: &str, data: &[u8]) -> LsResult<FileInfo> {
        let size_bytes = data.len() as u64;
        let mime = guess_mime(name, data);
        let file_type = classify_file_type(&mime);
        let preview = if file_type == FileType::Text
            || file_type == FileType::Code
            || file_type == FileType::Document
        {
            extract_text_preview(data)
        } else {
            None
        };

        Ok(FileInfo {
            name: name.to_string(),
            mime_type: mime,
            size_bytes,
            file_type,
            encoding: None,
            preview,
        })
    }

    /// 猜测 MIME 类型.
    pub fn guess_mime(name: &str, data: &[u8]) -> String {
        guess_mime(name, data)
    }

    /// 分类文件类型.
    pub fn classify(mime: &str) -> FileType {
        classify_file_type(mime)
    }

    /// 提取文件扩展名.
    pub fn extension(name: &str) -> &str {
        std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
    }

    /// 是否为可安全预览的文本文件.
    pub fn is_text_previewable(data: &[u8]) -> bool {
        // 检查前 512 字节是否大部分为 UTF-8 文本
        let check_len = data.len().min(512);
        let slice = &data[..check_len];
        // 如果包含 null 字节，通常是二进制
        if slice.contains(&0x00) {
            return false;
        }
        // 检查是否为有效 UTF-8
        std::str::from_utf8(slice).is_ok()
    }
}

/// 猜测 MIME 类型 (先查扩展名，再查 magic bytes).
fn guess_mime(name: &str, data: &[u8]) -> String {
    // 优先用扩展名匹配
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if !ext.is_empty() {
        let mime = mime_guess::from_ext(ext)
            .first_or_octet_stream()
            .to_string();
        if mime != "application/octet-stream" {
            return mime;
        }
    }

    // 再用 magic bytes
    if data.len() >= 4 {
        // 图像
        if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            return "image/jpeg".into();
        }
        if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
            return "image/png".into();
        }
        if data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x38 {
            return "image/gif".into();
        }
        // PDF
        if data[0] == 0x25 && data[1] == 0x50 && data[2] == 0x44 && data[3] == 0x46 {
            return "application/pdf".into();
        }
        // ZIP
        if data[0] == 0x50 && data[1] == 0x4B && data[2] == 0x03 && data[3] == 0x04 {
            return "application/zip".into();
        }
    }

    // 文本检测
    if FileAnalyzer::is_text_previewable(data) {
        return "text/plain".into();
    }

    "application/octet-stream".into()
}

/// 根据 MIME 类型分类.
fn classify_file_type(mime: &str) -> FileType {
    match mime {
        m if m.starts_with("image/") => FileType::Image,
        m if m.starts_with("audio/") => FileType::Audio,
        m if m.starts_with("video/") => FileType::Video,
        m if m.starts_with("text/") => FileType::Text,
        m if m.starts_with("application/pdf") => FileType::Document,
        m if m.starts_with("application/msword")
            || m.starts_with("application/vnd.openxmlformats-officedocument") =>
        {
            FileType::Document
        }
        m if m.contains("javascript")
            || m.contains("json")
            || m.contains("xml")
            || m.contains("yaml")
            || m.contains("toml") =>
        {
            FileType::Code
        }
        m if m.starts_with("application/zip")
            || m.starts_with("application/x-tar")
            || m.starts_with("application/gzip") =>
        {
            FileType::Archive
        }
        _ => FileType::Unknown,
    }
}

/// 提取文本内容预览 (前 1000 字节).
fn extract_text_preview(data: &[u8]) -> Option<String> {
    let max_preview = 1000;
    let preview_len = data.len().min(max_preview);
    let preview = &data[..preview_len];

    std::str::from_utf8(preview).ok().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_image() {
        assert_eq!(classify_file_type("image/jpeg"), FileType::Image);
        assert_eq!(classify_file_type("image/png"), FileType::Image);
        assert_eq!(classify_file_type("image/gif"), FileType::Image);
    }

    #[test]
    fn test_classify_audio() {
        assert_eq!(classify_file_type("audio/mpeg"), FileType::Audio);
        assert_eq!(classify_file_type("audio/wav"), FileType::Audio);
    }

    #[test]
    fn test_classify_text() {
        assert_eq!(classify_file_type("text/plain"), FileType::Text);
        assert_eq!(classify_file_type("text/html"), FileType::Text);
    }

    #[test]
    fn test_classify_code() {
        assert_eq!(classify_file_type("application/json"), FileType::Code);
        assert_eq!(classify_file_type("application/xml"), FileType::Code);
    }

    #[test]
    fn test_guess_mime_by_ext() {
        let mime = guess_mime("test.png", &[]);
        assert_eq!(mime, "image/png");

        let mime = guess_mime("test.jpg", &[]);
        assert_eq!(mime, "image/jpeg");
    }

    #[test]
    fn test_guess_mime_by_magic() {
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let mime = guess_mime("unknown", &jpeg_data);
        assert_eq!(mime, "image/jpeg");

        let png_data = vec![0x89, 0x50, 0x4E, 0x47];
        let mime = guess_mime("unknown", &png_data);
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn test_extension() {
        assert_eq!(FileAnalyzer::extension("test.jpg"), "jpg");
        assert_eq!(FileAnalyzer::extension("archive.tar.gz"), "gz");
        assert_eq!(FileAnalyzer::extension("noext"), "");
    }

    #[test]
    fn test_text_previewable() {
        let text = b"hello world this is text";
        assert!(FileAnalyzer::is_text_previewable(text));

        let binary = vec![0x00, 0x01, 0x02, 0xFF];
        assert!(!FileAnalyzer::is_text_previewable(&binary));
    }

    #[test]
    fn test_analyze_text_file() {
        let data = b"Hello, this is a text file!";
        let info = FileAnalyzer::analyze("test.txt", data).unwrap();
        assert_eq!(info.name, "test.txt");
        assert_eq!(info.size_bytes, data.len() as u64);
        assert_eq!(info.file_type, FileType::Text);
        assert!(info.preview.is_some());
    }

    #[test]
    fn test_analyze_image_file() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let info = FileAnalyzer::analyze("photo.jpg", &data).unwrap();
        assert_eq!(info.file_type, FileType::Image);
        assert_eq!(info.mime_type, "image/jpeg");
    }
}
