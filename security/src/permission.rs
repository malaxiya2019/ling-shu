use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 权限格式: `ls.{domain}.{resource}.{action}`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Permission(String);

impl Permission {
    /// 构造权限字符串.
    pub fn new(domain: &str, resource: &str, action: &str) -> Self {
        Self(format!("ls.{}.{}.{}", domain, resource, action))
    }

    /// 从字符串解析.
    pub fn parse(s: &str) -> LsResult<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 4 && parts[0] == "ls" {
            Ok(Self(s.to_string()))
        } else {
            Err(LsError::InvalidArgument(format!(
                "invalid permission format: {s}"
            )))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 隔离级别.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IsolationLevel {
    Global,
    Tenant,
    User,
    #[default]
    Session,
}

/// 权限校验器.
#[derive(Debug)]
pub struct PermissionChecker {
    admin_permissions: HashSet<Permission>,
}

impl PermissionChecker {
    pub fn new() -> Self {
        Self {
            admin_permissions: HashSet::new(),
        }
    }
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionChecker {
    /// 校验主体是否有权执行某操作.
    pub fn check(&self, granted: &[Permission], required: &Permission) -> LsResult<()> {
        if self.admin_permissions.contains(required) {
            return Ok(());
        }
        if granted.contains(required) {
            return Ok(());
        }
        Err(LsError::PermissionDenied(format!(
            "missing permission: {}",
            required.as_str()
        )))
    }

    /// 注册管理员权限.
    pub fn grant_admin(&mut self, permission: Permission) {
        self.admin_permissions.insert(permission);
    }
}
