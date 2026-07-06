//! 模板引擎 — 变量注入 & 提示词编译.

use lingshu_core::LsResult;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 模板变量.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    pub default_value: Option<String>,
}

/// 已编译的提示词.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPrompt {
    /// 编译后的文本.
    pub text: String,
    /// 使用的变量.
    pub variables: HashMap<String, String>,
    /// 模板版本.
    pub template_version: String,
}

/// 模板引擎.
#[derive(Clone)]
pub struct TemplateEngine {
    /// 变量插值模式：`{{ variable_name }}`
    pattern: Regex,
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine {
    pub fn new() -> Self {
        Self {
            pattern: Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}").expect("valid regex"),
        }
    }

    /// 使用自定义正则模式.
    pub fn with_pattern(pattern: &str) -> LsResult<Self> {
        let re = Regex::new(pattern)
            .map_err(|e| lingshu_core::LsError::Internal(format!("invalid regex: {e}")))?;
        Ok(Self { pattern: re })
    }

    /// 提取模板中的所有变量名.
    pub fn extract_variables(&self, template: &str) -> Vec<String> {
        let mut vars: Vec<String> = Vec::new();
        for cap in self.pattern.captures_iter(template) {
            if let Some(name) = cap.get(1) {
                let name = name.as_str().to_string();
                if !vars.contains(&name) {
                    vars.push(name);
                }
            }
        }
        vars
    }

    /// 编译模板 — 用变量值替换占位符.
    ///
    /// 未提供的必需变量将保留原占位符.
    pub fn compile(
        &self,
        template: &str,
        variables: &HashMap<String, String>,
        template_version: &str,
    ) -> CompiledPrompt {
        let mut used = HashMap::new();
        let result = self
            .pattern
            .replace_all(template, |caps: &regex::Captures| {
                let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                if let Some(value) = variables.get(name) {
                    used.insert(name.to_string(), value.clone());
                    value.clone()
                } else {
                    // 保留原占位符
                    caps.get(0)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default()
                }
            });

        CompiledPrompt {
            text: result.to_string(),
            variables: used,
            template_version: template_version.to_string(),
        }
    }

    /// 验证模板需要的变量是否都已提供.
    pub fn validate(
        &self,
        template: &str,
        defined_vars: &[TemplateVariable],
    ) -> std::result::Result<(), Vec<String>> {
        let extracted = self.extract_variables(template);
        let mut missing = Vec::new();

        for var in defined_vars {
            if var.required && !extracted.contains(&var.name) {
                // 定义的必需变量在模板中未使用，这不一定是错误
                // 所以继续检查反向：模板中使用的变量是否都有定义
            }
        }

        for var_name in &extracted {
            let defined = defined_vars.iter().find(|v| v.name == *var_name);
            match defined {
                Some(var) => {
                    if var.required && !extracted.contains(&var.name) {
                        // already handled
                    }
                }
                None => {
                    missing.push(var_name.clone());
                }
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_substitution() {
        let engine = TemplateEngine::new();
        let template = "Hello, {{ name }}! You are {{ role }}.";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("role".to_string(), "admin".to_string());

        let result = engine.compile(template, &vars, "1.0");
        assert_eq!(result.text, "Hello, Alice! You are admin.");
    }

    #[test]
    fn test_missing_variable_keeps_placeholder() {
        let engine = TemplateEngine::new();
        let template = "Hello, {{ name }}!";
        let vars = HashMap::new();

        let result = engine.compile(template, &vars, "1.0");
        assert_eq!(result.text, "Hello, {{ name }}!");
    }

    #[test]
    fn test_extract_variables() {
        let engine = TemplateEngine::new();
        let template = "{{a}} + {{b}} = {{result}}";
        let vars = engine.extract_variables(template);
        assert_eq!(vars, vec!["a", "b", "result"]);
    }

    #[test]
    fn test_variable_with_underscore() {
        let engine = TemplateEngine::new();
        let template = "{{ user_name }} has {{ _count }} items";
        let vars = engine.extract_variables(template);
        assert!(vars.contains(&"user_name".to_string()));
    }

    #[test]
    fn test_empty_template() {
        let engine = TemplateEngine::new();
        let template = "";
        let vars = HashMap::new();
        let result = engine.compile(template, &vars, "1.0");
        assert_eq!(result.text, "");
    }

    #[test]
    fn test_compile_tracks_used_variables() {
        let engine = TemplateEngine::new();
        let template = "{{a}} and {{b}}";
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), "x".to_string());
        vars.insert("b".to_string(), "y".to_string());

        let result = engine.compile(template, &vars, "1.0");
        assert_eq!(result.variables.len(), 2);
        assert_eq!(result.variables.get("a"), Some(&"x".to_string()));
    }
}
