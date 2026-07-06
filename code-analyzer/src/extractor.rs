//! 结构提取器 — 基于正则快速提取函数/类/导入声明.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// 提取出的函数.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFunction {
    pub name: String,
    pub line_start: u32,
    pub line_end: u32,
    pub params: Vec<String>,
}

/// 提取出的类.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedClass {
    pub name: String,
    pub line_start: u32,
    pub line_end: u32,
    pub methods: Vec<String>,
}

/// 导入声明.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedImport {
    pub source: String,
    pub specifiers: Vec<String>,
    pub line_number: u32,
}

/// 提取结果.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractionResult {
    pub functions: Vec<ExtractedFunction>,
    pub classes: Vec<ExtractedClass>,
    pub imports: Vec<ExtractedImport>,
}

/// 结构提取器.
#[allow(dead_code)]
pub struct StructureExtractor {
    // Rust
    rust_fn: Regex,
    rust_struct: Regex,
    rust_impl: Regex,
    rust_use: Regex,
    rust_mod: Regex,
    // Python
    py_fn: Regex,
    py_class: Regex,
    py_import: Regex,
    py_from: Regex,
    // TypeScript/JavaScript
    ts_fn: Regex,
    ts_class: Regex,
    ts_import: Regex,
    ts_export_fn: Regex,
    ts_arrow: Regex,
    // Go
    go_fn: Regex,
    go_struct: Regex,
    go_import: Regex,
    go_interface: Regex,
}

impl Default for StructureExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureExtractor {
    pub fn new() -> Self {
        Self {
            // Rust
            rust_fn: Regex::new(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([^)]*)").unwrap(),
            rust_struct: Regex::new(r"(?m)^\s*(?:pub\s+)?struct\s+(\w+)").unwrap(),
            rust_impl: Regex::new(r"(?m)^\s*(?:pub\s+)?impl\s+(\w+)").unwrap(),
            rust_use: Regex::new(r"(?m)^\s*use\s+([^;]+);").unwrap(),
            rust_mod: Regex::new(r"(?m)^\s*(?:pub\s+)?mod\s+(\w+)").unwrap(),
            // Python
            py_fn: Regex::new(r"(?m)^\s*(?:async\s+)?def\s+(\w+)\s*\(([^)]*)").unwrap(),
            py_class: Regex::new(r"(?m)^\s*class\s+(\w+)").unwrap(),
            py_import: Regex::new(r"(?m)^\s*import\s+([\w\s,]+)").unwrap(),
            py_from: Regex::new(r"(?m)^\s*from\s+([\w.]+)\s+import\s+([\w\s,]+)").unwrap(),
            // TS/JS
            ts_fn: Regex::new(r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(([^)]*)").unwrap(),
            ts_class: Regex::new(r"(?m)^\s*(?:export\s+)?(?:abstract\s+)?class\s+(\w+)").unwrap(),
            ts_import: Regex::new(r#"(?m)^\s*import\s+\{?([^}]+)\}?\s*from\s+['"]([^'"]+)['"]"#).unwrap(),
            ts_export_fn: Regex::new(r"(?m)^\s*export\s+(?:async\s+)?function\s+(\w+)\s*\(([^)]*)").unwrap(),
            ts_arrow: Regex::new(r"(?m)^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?\(([^)]*)\)\s*=>").unwrap(),
            // Go
            go_fn: Regex::new(r"(?m)^\s*(?:func\s+)(?:\([^)]*\)\s+)?(\w+)\s*\(([^)]*)").unwrap(),
            go_struct: Regex::new(r"(?m)^\s*type\s+(\w+)\s+struct").unwrap(),
            go_import: Regex::new(r#"(?m)^\s*import\s+[("]\s*$"#).unwrap(),
            go_interface: Regex::new(r"(?m)^\s*type\s+(\w+)\s+interface").unwrap(),
        }
    }

    /// 根据语言提取结构.
    pub fn extract(&self, content: &str, language: &str) -> ExtractionResult {
        match language {
            "rust" => self.extract_rust(content),
            "python" => self.extract_python(content),
            "javascript" | "typescript" | "jsx" | "tsx" => self.extract_typescript(content),
            "go" => self.extract_go(content),
            _ => ExtractionResult::default(),
        }
    }

    fn extract_rust(&self, content: &str) -> ExtractionResult {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u32;

        let mut functions = Vec::new();
        for cap in self.rust_fn.captures_iter(content) {
            let name = cap[1].to_string();
            let params: Vec<String> = cap[2].split(',')
                .map(|p| p.trim().split_whitespace().last().unwrap_or(p.trim()).to_string())
                .filter(|p| !p.is_empty())
                .collect();
            // 估算行范围
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            functions.push(ExtractedFunction {
                name, params, line_start,
                line_end: find_block_end(lines.as_slice(), line_start as usize, total_lines),
            });
        }

        let mut classes = Vec::new();
        for cap in self.rust_struct.captures_iter(content) {
            let name = cap[1].to_string();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            classes.push(ExtractedClass {
                name, methods: vec![], line_start,
                line_end: find_block_end(lines.as_slice(), line_start as usize, total_lines),
            });
        }

        let mut imports = Vec::new();
        for cap in self.rust_use.captures_iter(content) {
            let source = cap[1].trim().to_string().replace(" ", "");
            let line_number = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            imports.push(ExtractedImport {
                source, specifiers: vec![], line_number,
            });
        }

        ExtractionResult { functions, classes, imports }
    }

    fn extract_python(&self, content: &str) -> ExtractionResult {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u32;

        let mut functions = Vec::new();
        for cap in self.py_fn.captures_iter(content) {
            let name = cap[1].to_string();
            let params: Vec<String> = cap[2].split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty() && p != "self" && p != "cls")
                .collect();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            functions.push(ExtractedFunction { name, params, line_start, line_end: total_lines });
        }

        let mut classes = Vec::new();
        for cap in self.py_class.captures_iter(content) {
            let name = cap[1].to_string();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            classes.push(ExtractedClass { name, methods: vec![], line_start, line_end: total_lines });
        }

        let mut imports = Vec::new();
        for cap in self.py_import.captures_iter(content) {
            let line_number = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            for spec in cap[1].split(',') {
                let spec = spec.trim().to_string();
                if !spec.is_empty() {
                    imports.push(ExtractedImport { source: spec.clone(), specifiers: vec![], line_number });
                }
            }
        }
        for cap in self.py_from.captures_iter(content) {
            let source = cap[1].to_string();
            let line_number = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            let specifiers: Vec<String> = cap[2].split(',').map(|s| s.trim().to_string()).collect();
            imports.push(ExtractedImport { source, specifiers, line_number });
        }

        ExtractionResult { functions, classes, imports }
    }

    fn extract_typescript(&self, content: &str) -> ExtractionResult {
        let total_lines = content.lines().count() as u32;

        let mut functions = Vec::new();
        for cap in self.ts_fn.captures_iter(content) {
            let name = cap[1].to_string();
            let params: Vec<String> = cap[2].split(',')
                .map(|p| p.trim().split(':').next().unwrap_or(p.trim()).to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            functions.push(ExtractedFunction { name, params, line_start, line_end: total_lines });
        }
        for cap in self.ts_arrow.captures_iter(content) {
            let name = cap[1].to_string();
            let params: Vec<String> = cap[2].split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            functions.push(ExtractedFunction { name, params, line_start, line_end: total_lines });
        }

        let mut classes = Vec::new();
        for cap in self.ts_class.captures_iter(content) {
            let name = cap[1].to_string();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            classes.push(ExtractedClass { name, methods: vec![], line_start, line_end: total_lines });
        }

        let mut imports = Vec::new();
        for cap in self.ts_import.captures_iter(content) {
            let source = cap[2].to_string();
            let line_number = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            let specifiers: Vec<String> = cap[1].split(',').map(|s| s.trim().to_string()).collect();
            imports.push(ExtractedImport { source, specifiers, line_number });
        }

        ExtractionResult { functions, classes, imports }
    }

    fn extract_go(&self, content: &str) -> ExtractionResult {
        let total_lines = content.lines().count() as u32;

        let mut functions = Vec::new();
        for cap in self.go_fn.captures_iter(content) {
            let name = cap[1].to_string();
            let params: Vec<String> = cap[2].split(',')
                .map(|p| p.trim().split_whitespace().last().unwrap_or(p.trim()).to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            functions.push(ExtractedFunction { name, params, line_start, line_end: total_lines });
        }

        let mut classes = Vec::new();
        for cap in self.go_struct.captures_iter(content) {
            let name = cap[1].to_string();
            let line_start = content[..cap.get(0).unwrap().start()].lines().count() as u32 + 1;
            classes.push(ExtractedClass { name, methods: vec![], line_start, line_end: total_lines });
        }

        ExtractionResult { functions, classes, imports: vec![] }
    }
}

/// 简单块结束估算.
fn find_block_end(lines: &[&str], start: usize, _total: u32) -> u32 {
    let mut depth = 0;
    let mut found = false;
    for (i, line) in lines.iter().enumerate().skip(start.saturating_sub(1)) {
        let trimmed = line.trim();
        if trimmed.ends_with('{') || trimmed.ends_with("->") {
            depth += 1;
            found = true;
        }
        if found && trimmed == "}" {
            depth -= 1;
            if depth <= 0 {
                return (i + 1) as u32;
            }
        }
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn") || trimmed.starts_with("struct ") {
            if found && i > start {
                return i as u32;
            }
        }
    }
    lines.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_functions() {
        let content = r#"
fn hello() {
    println!("hi");
}

pub async fn greet(name: String) {
    println!("{name}");
}
"#;
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "rust");
        assert_eq!(result.functions.len(), 2);
        assert_eq!(result.functions[0].name, "hello");
        assert_eq!(result.functions[1].name, "greet");
    }

    #[test]
    fn test_extract_rust_struct() {
        let content = r#"
struct Config {
    name: String,
}

pub struct App {
    config: Config,
}
"#;
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "rust");
        assert_eq!(result.classes.len(), 2);
        assert_eq!(result.classes[0].name, "Config");
    }

    #[test]
    fn test_extract_rust_imports() {
        let content = r#"
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
"#;
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "rust");
        assert_eq!(result.imports.len(), 2);
    }

    #[test]
    fn test_extract_python() {
        let content = r#"
import os, sys
from typing import Optional, List

class MyClass:
    def method1(self):
        pass

async def hello(name: str):
    print(f"hi {name}")
"#;
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "python");
        assert_eq!(result.functions.len(), 2);
        assert_eq!(result.functions[0].name, "method1");
        assert_eq!(result.functions[1].name, "hello");
        assert_eq!(result.classes.len(), 1);
        assert_eq!(result.classes[0].name, "MyClass");
        assert_eq!(result.imports.len(), 4); // os, sys, typing, Optional, List
    }

    #[test]
    fn test_extract_typescript() {
        let content = r#"
import { Component } from 'react';
import { useState } from 'react';

function hello(name: string) {
    return `Hello ${name}`;
}

class MyComponent {
    render() { return null; }
}

const greet = (name: string) => `Hi ${name}`;
"#;
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "typescript");
        assert_eq!(result.functions.len(), 2); // hello + greet arrow
        assert_eq!(result.classes.len(), 1);
        assert!(result.imports.len() >= 1);
    }

    #[test]
    fn test_extract_go() {
        let content = r#"
func main() {
    fmt.Println("hello")
}

func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("hi"))
}

type Config struct {
    Name string
}
"#;
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "go");
        assert_eq!(result.functions.len(), 2);
        assert_eq!(result.classes.len(), 1);
    }

    #[test]
    fn test_no_match_for_unknown_language() {
        let content = "some random text";
        let extractor = StructureExtractor::new();
        let result = extractor.extract(content, "ruby");
        assert_eq!(result.functions.len(), 0);
    }
}
