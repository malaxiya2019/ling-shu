//! 提示词注册表 — 版本化管理模板.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::template::{TemplateEngine, TemplateVariable};

/// 提示词版本.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub version_id: Uuid,
    pub version_number: u64,
    pub template: String,
    pub variables: Vec<TemplateVariable>,
    pub created_at: DateTime<Utc>,
    pub changelog: Option<String>,
}

/// 提示词信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInfo {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub current_version: u64,
    pub versions: Vec<PromptVersion>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

/// 提示词注册表.
#[derive(Clone)]
pub struct PromptRegistry {
    prompts: DashMap<String, PromptInfo>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self {
            prompts: DashMap::new(),
        }
    }

    /// 注册新提示词.
    pub fn register(
        &self,
        name: &str,
        description: &str,
        template: &str,
        variables: Vec<TemplateVariable>,
    ) -> LsResult<PromptInfo> {
        if self.prompts.contains_key(name) {
            return Err(LsError::Internal(format!("prompt '{name}' already exists")));
        }

        let now = Utc::now();
        let version = PromptVersion {
            version_id: Uuid::new_v4(),
            version_number: 1,
            template: template.to_string(),
            variables,
            created_at: now,
            changelog: Some("initial version".into()),
        };

        let info = PromptInfo {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            current_version: 1,
            versions: vec![version],
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
        };

        self.prompts.insert(name.to_string(), info.clone());
        Ok(info)
    }

    /// 获取提示词最新版本.
    pub fn get(&self, name: &str) -> LsResult<PromptInfo> {
        self.prompts
            .get(name)
            .map(|e| e.value().clone())
            .ok_or_else(|| LsError::NotFound(format!("prompt '{name}' not found")))
    }

    /// 获取特定版本的提示词.
    pub fn get_version(&self, name: &str, version: u64) -> LsResult<PromptVersion> {
        let info = self.get(name)?;
        info.versions
            .into_iter()
            .find(|v| v.version_number == version)
            .ok_or_else(|| LsError::NotFound(format!("prompt '{name}' version {version}")))
    }

    /// 创建新版本.
    pub fn create_version(
        &self,
        name: &str,
        template: &str,
        variables: Vec<TemplateVariable>,
        changelog: &str,
    ) -> LsResult<PromptVersion> {
        let mut info = self.get(name)?;
        let new_version_number = info.current_version + 1;

        let version = PromptVersion {
            version_id: Uuid::new_v4(),
            version_number: new_version_number,
            template: template.to_string(),
            variables,
            created_at: Utc::now(),
            changelog: Some(changelog.to_string()),
        };

        info.versions.push(version.clone());
        info.current_version = new_version_number;
        info.updated_at = Utc::now();

        self.prompts.insert(name.to_string(), info);
        Ok(version)
    }

    /// 编译提示词.
    pub fn compile(
        &self,
        name: &str,
        variables: &std::collections::HashMap<String, String>,
        engine: &TemplateEngine,
    ) -> LsResult<crate::CompiledPrompt> {
        let info = self.get(name)?;
        let current_version = info
            .versions
            .iter()
            .find(|v| v.version_number == info.current_version)
            .ok_or_else(|| LsError::Internal(format!("no version found for prompt '{name}'")))?;

        Ok(engine.compile(
            &current_version.template,
            variables,
            &format!("{}.v{}", name, info.current_version),
        ))
    }

    /// 添加标签.
    pub fn add_tag(&self, name: &str, tag: &str) -> LsResult<()> {
        if let Some(mut info) = self.prompts.get_mut(name) {
            if !info.tags.contains(&tag.to_string()) {
                info.tags.push(tag.to_string());
            }
            Ok(())
        } else {
            Err(LsError::NotFound(format!("prompt '{name}' not found")))
        }
    }

    /// 列出所有提示词.
    pub fn list(&self) -> Vec<PromptInfo> {
        self.prompts.iter().map(|e| e.value().clone()).collect()
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_register_and_get() {
        let registry = PromptRegistry::new();
        let info = registry
            .register(
                "chat.system",
                "System prompt for chat",
                "You are {{ role }}, an AI assistant.",
                vec![TemplateVariable {
                    name: "role".into(),
                    description: Some("Assistant role".into()),
                    required: true,
                    default_value: None,
                }],
            )
            .unwrap();

        assert_eq!(info.name, "chat.system");
        assert_eq!(info.current_version, 1);

        let fetched = registry.get("chat.system").unwrap();
        assert_eq!(fetched.id, info.id);
    }

    #[test]
    fn test_get_not_found() {
        let registry = PromptRegistry::new();
        let result = registry.get("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_version() {
        let registry = PromptRegistry::new();
        registry
            .register("test", "test prompt", "version 1", vec![])
            .unwrap();

        registry
            .create_version("test", "version 2", vec![], "updated template")
            .unwrap();

        let info = registry.get("test").unwrap();
        assert_eq!(info.current_version, 2);
        assert_eq!(info.versions.len(), 2);
    }

    #[test]
    fn test_compile_prompt() {
        let registry = PromptRegistry::new();
        let engine = TemplateEngine::new();

        registry
            .register(
                "greeting",
                "Greeting prompt",
                "Hello, {{ name }}!",
                vec![TemplateVariable {
                    name: "name".into(),
                    description: None,
                    required: true,
                    default_value: None,
                }],
            )
            .unwrap();

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let compiled = registry.compile("greeting", &vars, &engine).unwrap();
        assert_eq!(compiled.text, "Hello, World!");
    }

    #[test]
    fn test_duplicate_registration() {
        let registry = PromptRegistry::new();
        registry
            .register("dup", "desc", "template", vec![])
            .unwrap();
        let result = registry.register("dup", "desc", "template", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_prompts() {
        let registry = PromptRegistry::new();
        registry.register("a", "desc a", "tpl a", vec![]).unwrap();
        registry.register("b", "desc b", "tpl b", vec![]).unwrap();

        let list = registry.list();
        assert_eq!(list.len(), 2);
    }
}
