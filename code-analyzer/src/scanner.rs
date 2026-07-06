//! 文件扫描器 — 递归遍历目录，对文件分类并检测语言.

use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 文件类别.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileCategory {
    Code,
    Config,
    Docs,
    Infra,
    Data,
    Script,
    Markup,
    Other,
}

/// 扫描到的文件条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// 项目相对路径.
    pub path: String,
    /// 文件名.
    pub name: String,
    /// 文件扩展名.
    pub extension: String,
    /// 检测到的语言.
    pub language: String,
    /// 行数.
    pub size_lines: u32,
    /// 文件类别.
    pub category: FileCategory,
}

/// 文件扫描器.
pub struct FileScanner {
    /// 要忽略的目录/文件名模式.
    ignore_patterns: Vec<String>,
}

impl Default for FileScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl FileScanner {
    pub fn new() -> Self {
        Self {
            ignore_patterns: vec![
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                ".venv".into(),
                "__pycache__".into(),
                ".next".into(),
                "dist".into(),
                "build".into(),
                ".codex".into(),
                ".claude".into(),
                ".cursor".into(),
                ".copilot".into(),
                "vendor".into(),
                ".terraform".into(),
                ".bzr".into(),
                ".hg".into(),
                ".svn".into(),
                "CVS".into(),
            ],
        }
    }

    /// 自定义忽略模式.
    pub fn with_ignore(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns = patterns;
        self
    }

    /// 是否应忽略此路径.
    fn should_ignore(&self, path: &Path) -> bool {
        let _path_str = path.to_string_lossy();
        for component in path.components() {
            let comp_str = component.as_os_str().to_string_lossy();
            if self.ignore_patterns.iter().any(|p| *p == comp_str.as_ref()) {
                return true;
            }
        }
        false
    }

    /// 检测语言（从扩展名）.
    fn detect_language(extension: &str) -> String {
        match extension {
            "rs" => "rust",
            "py" => "python",
            "js" => "javascript",
            "ts" | "tsx" => "typescript",
            "jsx" => "jsx",
            "go" => "go",
            "java" => "java",
            "rb" => "ruby",
            "c" => "c",
            "h" => "c_header",
            "cpp" | "cc" | "cxx" => "cpp",
            "hpp" | "hh" => "cpp_header",
            "cs" => "csharp",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "scala" => "scala",
            "php" => "php",
            "r" => "r",
            "m" => "objective_c",
            "mm" => "objective_cpp",
            "zig" => "zig",
            "hs" => "haskell",
            "ex" | "exs" => "elixir",
            "clj" | "cljs" => "clojure",
            "erl" => "erlang",
            "sh" | "bash" | "zsh" => "shell",
            "pl" => "perl",
            "lua" => "lua",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" | "scss" | "less" => "css",
            "json" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "md" | "markdown" => "markdown",
            "dockerfile" | "Dockerfile" => "dockerfile",
            "makefile" | "Makefile" | "mk" => "makefile",
            "tf" => "terraform",
            "proto" => "protobuf",
            "graphql" | "gql" => "graphql",
            "env" => "env",
            "xml" => "xml",
            "svg" => "svg",
            "vue" => "vue",
            "svelte" => "svelte",
            "dart" => "dart",
            "lisp" | "cl" => "lisp",
            "ml" => "ocaml",
            _ => "unknown",
        }
        .to_string()
    }

    /// 文件分类.
    fn categorize(extension: &str, path: &str) -> FileCategory {
        match extension {
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "rb" | "c" | "h"
            | "cpp" | "hpp" | "cs" | "swift" | "kt" | "scala" | "php" | "r" | "zig" | "hs"
            | "ex" | "clj" | "erl" | "dart" | "lisp" | "ml" | "vue" | "svelte" => {
                FileCategory::Code
            }
            "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "env" | "xml" | "props"
            | "properties" => FileCategory::Config,
            "md" | "markdown" | "rst" | "txt" | "adoc" | "wiki" => FileCategory::Docs,
            "dockerfile" | "makefile" | "Dockerfile" | "Makefile" | "mk" | "tf" | "hcl" => {
                if path.contains("docker")
                    || extension == "dockerfile"
                    || path.ends_with("Dockerfile")
                {
                    FileCategory::Infra
                } else {
                    FileCategory::Infra
                }
            }
            "sh" | "bash" | "zsh" | "pl" | "lua" => FileCategory::Script,
            "html" | "htm" | "css" | "scss" | "less" | "svg" => FileCategory::Markup,
            "sql" | "csv" | "tsv" | "parquet" => FileCategory::Data,
            _ => FileCategory::Other,
        }
    }

    /// 扫描目录.
    pub fn scan(&self, dir: &Path) -> LsResult<Vec<FileEntry>> {
        let mut entries = Vec::new();

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| !self.should_ignore(e.path()))
        {
            let entry =
                entry.map_err(|e| lingshu_core::LsError::Internal(format!("walk error: {e}")))?;

            let path = entry.path();

            // 跳过隐藏文件/目录
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') && name != ".understand-anything" {
                    continue;
                }
            }

            if !entry.file_type().is_file() {
                continue;
            }

            let path_str = path.to_string_lossy();

            // 跳过可执行文件、二进制等
            if self.is_binary(path) {
                continue;
            }

            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // 处理无扩展名的特殊文件 (Dockerfile, Makefile 等)
            let effective_ext = if ext.is_empty() {
                if name == "Dockerfile" {
                    "dockerfile".into()
                } else if name == "Makefile" {
                    "makefile".into()
                } else if name == "Procfile" {
                    "yaml".into()
                } else {
                    ext.clone()
                }
            } else {
                ext.clone()
            };

            let language = Self::detect_language(&effective_ext);
            if language == "unknown" {
                continue; // 跳过无法识别类型的文件
            }

            let line_count = count_lines(path).unwrap_or(0);

            entries.push(FileEntry {
                path: path_str.to_string(),
                name,
                extension: effective_ext.clone(),
                language,
                size_lines: line_count as u32,
                category: Self::categorize(&effective_ext, &path_str),
            });
        }

        Ok(entries)
    }

    fn is_binary(&self, path: &Path) -> bool {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        matches!(
            ext.as_str(),
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "bmp"
                | "ico"
                | "svg"
                | "woff"
                | "woff2"
                | "ttf"
                | "eot"
                | "mp3"
                | "mp4"
                | "avi"
                | "mov"
                | "wav"
                | "zip"
                | "tar"
                | "gz"
                | "bz2"
                | "7z"
                | "rar"
                | "pdf"
                | "doc"
                | "docx"
                | "xls"
                | "xlsx"
                | "o"
                | "so"
                | "dylib"
                | "dll"
                | "exe"
                | "wasm"
                | "class"
                | "jar"
        )
    }
}

fn count_lines(path: &Path) -> std::io::Result<usize> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    Ok(reader.lines().count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("lib.py"), "def hello(): pass").unwrap();
        fs::write(dir.path().join("config.json"), "{}").unwrap();
        fs::write(dir.path().join("README.md"), "# Project").unwrap();
        fs::write(dir.path().join("Dockerfile"), "FROM ubuntu").unwrap();
        fs::write(dir.path().join("build.sh"), "#!/bin/bash\necho hi").unwrap();
        fs::write(dir.path().join("index.html"), "<html></html>").unwrap();
        dir
    }

    #[test]
    fn test_scan_directory() {
        let dir = create_test_dir();
        let scanner = FileScanner::new();
        let entries = scanner.scan(dir.path()).unwrap();

        // Should find all 7 files
        assert_eq!(
            entries.len(),
            7,
            "expected 7 files, got {:?}",
            entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(FileScanner::detect_language("rs"), "rust");
        assert_eq!(FileScanner::detect_language("py"), "python");
        assert_eq!(FileScanner::detect_language("ts"), "typescript");
        assert_eq!(FileScanner::detect_language("unknown"), "unknown");
    }

    #[test]
    fn test_file_categorization() {
        assert_eq!(FileScanner::categorize("rs", "main.rs"), FileCategory::Code);
        assert_eq!(
            FileScanner::categorize("json", "config.json"),
            FileCategory::Config
        );
        assert_eq!(
            FileScanner::categorize("md", "readme.md"),
            FileCategory::Docs
        );
        assert_eq!(
            FileScanner::categorize("sh", "build.sh"),
            FileCategory::Script
        );
        assert_eq!(
            FileScanner::categorize("html", "index.html"),
            FileCategory::Markup
        );
        assert_eq!(
            FileScanner::categorize("tf", "main.tf"),
            FileCategory::Infra
        );
        assert_eq!(
            FileScanner::categorize("sql", "query.sql"),
            FileCategory::Data
        );
    }

    #[test]
    fn test_ignore_dot_git() {
        let dir = create_test_dir();
        let git_dir = dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("config"), "dummy").unwrap();

        let scanner = FileScanner::new();
        let entries = scanner.scan(dir.path()).unwrap();
        assert!(!entries.iter().any(|e| e.path.contains(".git")));
    }
}
