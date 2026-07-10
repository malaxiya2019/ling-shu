//! Plugin Sandbox — 权限检查与沙箱隔离.

use lingshu_core::{LsError, LsResult};
use lingshu_traits::plugin::{PluginManifest, PluginPermission};

/// 检查插件是否拥有执行某操作所需的权限.
pub fn check_permission(manifest: &PluginManifest, resource: &str, action: &str) -> LsResult<()> {
    for perm in &manifest.permissions {
        if perm.resource == resource && perm.actions.contains(&action.to_string()) {
            return Ok(());
        }
    }
    Err(LsError::PermissionDenied(format!(
        "plugin '{}' does not have permission '{}' on resource '{}'",
        manifest.name, action, resource
    )))
}

/// 检查插件是否拥有所有指定权限.
pub fn require_permissions(
    manifest: &PluginManifest,
    required: &[PluginPermission],
) -> LsResult<()> {
    for req in required {
        for action in &req.actions {
            check_permission(manifest, &req.resource, action)?;
        }
    }
    Ok(())
}

/// 检查插件版本是否满足最低 API 版本要求.
pub fn check_api_version(manifest: &PluginManifest, min_api_version: &str) -> LsResult<()> {
    if let Some(ref ver) = manifest.min_api_version {
        if ver.as_str() > min_api_version {
            return Err(LsError::Plugin(format!(
                "plugin '{}' requires API version '{}', but runtime only provides '{}'",
                manifest.name, ver, min_api_version
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_traits::plugin::PluginManifest;

    fn test_manifest() -> PluginManifest {
        PluginManifest {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            author: None,
            homepage: None,
            license: None,
            plugin_type: "static".into(),
            entry_point: None,
            permissions: vec![PluginPermission {
                resource: "llm".into(),
                actions: vec!["invoke".into()],
            }],
            min_api_version: Some("1.0.0".into()),
        ..Default::default()
        }
    }

    #[test]
    fn test_check_permission_allowed() {
        let m = test_manifest();
        assert!(check_permission(&m, "llm", "invoke").is_ok());
    }

    #[test]
    fn test_check_permission_denied() {
        let m = test_manifest();
        assert!(check_permission(&m, "storage", "write").is_err());
    }

    #[test]
    fn test_check_api_version_ok() {
        let m = test_manifest();
        assert!(check_api_version(&m, "1.0.0").is_ok());
        assert!(check_api_version(&m, "1.1.0").is_ok());
    }
}
