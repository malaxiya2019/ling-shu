//! 📦 Plugin Manifest — 插件清单扩展与依赖解析.
//!
//! 在 [`lingshu_traits::plugin::PluginManifest`] 基础上增加了市场元数据、
//! 依赖声明、分类标签等字段，并提供了依赖约束解析和版本兼容性检查。

use lingshu_core::{LsError, LsResult};
use lingshu_traits::plugin::PluginManifest;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件依赖声明.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// 依赖的插件名称.
    pub name: String,
    /// 版本约束 (e.g. ">=1.0.0", "~2.3").
    pub version_req: String,
    /// 可选的依赖类型.
    #[serde(rename = "type", default)]
    pub dep_type: PluginDepType,
}

/// 依赖类型.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum PluginDepType {
    /// 运行时必需依赖 (默认).
    #[default]
    Required,
    /// 可选依赖.
    Optional,
    /// 构建/开发依赖.
    Dev,
}

/// 市场元数据 — 附加于插件清单用于市场分发.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketMeta {
    /// 所属分类.
    #[serde(default)]
    pub categories: Vec<String>,
    /// 标签.
    #[serde(default)]
    pub tags: Vec<String>,
    /// 仓库地址.
    pub repository: Option<String>,
    /// README 内容 (Markdown).
    pub readme: Option<String>,
    /// 图标 URL 或 base64 data URI.
    pub icon: Option<String>,
    /// 插件截图 URL 列表.
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// 最低引擎版本要求.
    pub min_engine_version: Option<String>,
}

/// 扩展插件清单 — 融合基础清单 + 市场元数据 + 依赖.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedManifest {
    /// 基础插件清单.
    #[serde(flatten)]
    pub base: PluginManifest,
    /// 市场元数据.
    #[serde(flatten)]
    pub market: MarketMeta,
    /// 依赖列表.
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
}

/// 依赖解析器 — 检查依赖是否满足约束.
pub struct DependencyResolver;

impl DependencyResolver {
    /// 检查一组已安装的插件是否满足所有依赖.
    ///
    /// `installed`: 已安装插件的名称 → 版本号.
    /// `deps`: 待检查的依赖列表.
    ///
    /// 返回缺失/不满足的依赖列表.
    pub fn check_dependencies(
        installed: &HashMap<String, String>,
        deps: &[PluginDependency],
    ) -> Vec<PluginDependency> {
        let mut unsatisfied = Vec::new();

        for dep in deps {
            let version_str = match installed.get(&dep.name) {
                Some(v) => v,
                None => {
                    unsatisfied.push(dep.clone());
                    continue;
                }
            };

            let version = match Version::parse(version_str) {
                Ok(v) => v,
                Err(_) => {
                    unsatisfied.push(dep.clone());
                    continue;
                }
            };

            let req = match VersionReq::parse(&dep.version_req) {
                Ok(r) => r,
                Err(_) => {
                    unsatisfied.push(dep.clone());
                    continue;
                }
            };

            if !req.matches(&version) {
                unsatisfied.push(dep.clone());
            }
        }

        unsatisfied
    }

    /// 验证版本约束字符串是否合法.
    pub fn validate_version_req(req: &str) -> bool {
        VersionReq::parse(req).is_ok()
    }

    /// 比较两个版本字符串.
    pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
        let va = Version::parse(a).ok()?;
        let vb = Version::parse(b).ok()?;
        Some(va.cmp(&vb))
    }
}

/// 解析 JSON 格式的插件清单文件内容.
pub fn parse_manifest(json: &str) -> LsResult<ExtendedManifest> {
    serde_json::from_str(json)
        .map_err(|e| LsError::Plugin(format!("invalid extended manifest JSON: {e}")))
}

/// 加载并解析插件包中的 manifest.json 文件.
pub fn load_manifest_from_path(path: &std::path::Path) -> LsResult<ExtendedManifest> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| LsError::Plugin(format!("cannot read manifest '{}': {e}", path.display())))?;
    parse_manifest(&content)
}

/// 从基础 PluginManifest + 可选扩展字段构建 ExtendedManifest.
impl From<PluginManifest> for ExtendedManifest {
    fn from(base: PluginManifest) -> Self {
        Self {
            base,
            market: MarketMeta::default(),
            dependencies: Vec::new(),
        }
    }
}

/// 版本兼容性检查工具.
pub struct VersionCompat;

impl VersionCompat {
    /// 检查插件版本是否满足引擎要求.
    pub fn check_engine_compat(manifest: &ExtendedManifest, engine_version: &str) -> LsResult<()> {
        if let Some(ref req_str) = manifest.market.min_engine_version {
            let req = VersionReq::parse(req_str).map_err(|e| {
                LsError::Plugin(format!("invalid engine version req '{}': {e}", req_str))
            })?;
            let engine = Version::parse(engine_version).map_err(|e| {
                LsError::Plugin(format!("invalid engine version '{}': {e}", engine_version))
            })?;
            if !req.matches(&engine) {
                return Err(LsError::Plugin(format!(
                    "plugin '{}' requires engine version '{}', current is '{}'",
                    manifest.base.name, req_str, engine_version
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_extended_manifest() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "plugin_type": "dynamic",
            "permissions": [],
            "categories": ["llm", "tools"],
            "tags": ["ai", "chat"],
            "repository": "https://github.com/test/plugin",
            "dependencies": [
                {"name": "base-plugin", "version_req": ">=0.5.0"}
            ]
        }"#;

        let manifest = parse_manifest(json).unwrap();
        assert_eq!(manifest.base.name, "test-plugin");
        assert_eq!(manifest.base.version, "1.0.0");
        assert_eq!(manifest.market.categories, vec!["llm", "tools"]);
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies[0].name, "base-plugin");
        assert_eq!(manifest.dependencies[0].version_req, ">=0.5.0");
        assert_eq!(manifest.dependencies[0].dep_type, PluginDepType::Required);
    }

    #[test]
    fn test_dependency_resolver_satisfied() {
        let mut installed = HashMap::new();
        installed.insert("base".into(), "1.5.0".into());
        installed.insert("util".into(), "2.0.0".into());

        let deps = vec![
            PluginDependency {
                name: "base".into(),
                version_req: ">=1.0.0".into(),
                dep_type: PluginDepType::Required,
            },
            PluginDependency {
                name: "util".into(),
                version_req: ">=2.0.0, <3.0.0".into(),
                dep_type: PluginDepType::Required,
            },
        ];

        let unsatisfied = DependencyResolver::check_dependencies(&installed, &deps);
        assert!(unsatisfied.is_empty());
    }

    #[test]
    fn test_dependency_resolver_missing() {
        let installed = HashMap::new();
        let deps = vec![PluginDependency {
            name: "missing-dep".into(),
            version_req: ">=1.0.0".into(),
            dep_type: PluginDepType::Required,
        }];

        let unsatisfied = DependencyResolver::check_dependencies(&installed, &deps);
        assert_eq!(unsatisfied.len(), 1);
        assert_eq!(unsatisfied[0].name, "missing-dep");
    }

    #[test]
    fn test_dependency_resolver_version_mismatch() {
        let mut installed = HashMap::new();
        installed.insert("base".into(), "0.9.0".into());

        let deps = vec![PluginDependency {
            name: "base".into(),
            version_req: ">=1.0.0".into(),
            dep_type: PluginDepType::Required,
        }];

        let unsatisfied = DependencyResolver::check_dependencies(&installed, &deps);
        assert_eq!(unsatisfied.len(), 1);
    }

    #[test]
    fn test_version_validation() {
        assert!(DependencyResolver::validate_version_req(">=1.0.0"));
        assert!(DependencyResolver::validate_version_req("~2.3.4"));
        assert!(DependencyResolver::validate_version_req("^0.5"));
        assert!(!DependencyResolver::validate_version_req(
            "not-a-version-req"
        ));
    }

    #[test]
    fn test_version_comparison() {
        assert_eq!(
            DependencyResolver::compare_versions("1.0.0", "2.0.0"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(
            DependencyResolver::compare_versions("2.0.0", "1.0.0"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            DependencyResolver::compare_versions("1.0.0", "1.0.0"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(DependencyResolver::compare_versions("abc", "1.0.0"), None);
    }

    #[test]
    fn test_engine_compat() {
        let json = r#"{
            "name": "test", "version": "1.0.0", "description": "",
            "plugin_type": "dynamic", "permissions": [],
            "min_engine_version": ">=1.0.0"
        }"#;
        let manifest = parse_manifest(json).unwrap();
        assert!(VersionCompat::check_engine_compat(&manifest, "1.5.0").is_ok());
        assert!(VersionCompat::check_engine_compat(&manifest, "0.9.0").is_err());
    }
}
