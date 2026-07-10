//! ToolPermission — 基于角色和作用域的工具权限控制.
//!
//! # 权限模型
//!
//! 每个工具有一个 `PermissionLevel`（Public / User / Admin / SuperAdmin）。
//! 调用者有一个 `CallerRole`，权限检查确保 `caller.role >= tool.metadata.permission_level`。
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use lingshu_tool::permission::{ToolPermission, CallerInfo, CallerRole};
//!
//! let permission = ToolPermission::new();
//! let caller = CallerInfo { role: CallerRole::User, user_id: Some("u_123".into()) };
//! assert!(permission.check("read_file", &caller).is_ok());
//! ```

use lingshu_core::{LsError, LsResult};
use lingshu_traits::tool::PermissionLevel;
use serde::{Deserialize, Serialize};

/// 调用者角色.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallerRole {
    /// 匿名用户（最低权限）
    Anonymous,
    /// 普通用户
    User,
    /// 管理员
    Admin,
    /// 超级管理员
    SuperAdmin,
}

impl Default for CallerRole {
    fn default() -> Self {
        Self::Anonymous
    }
}

impl PartialOrd for CallerRole {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CallerRole {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(r: &CallerRole) -> u8 {
            match r {
                CallerRole::Anonymous => 0,
                CallerRole::User => 1,
                CallerRole::Admin => 2,
                CallerRole::SuperAdmin => 3,
            }
        }
        rank(self).cmp(&rank(other))
    }
}

impl std::fmt::Display for CallerRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anonymous => write!(f, "anonymous"),
            Self::User => write!(f, "user"),
            Self::Admin => write!(f, "admin"),
            Self::SuperAdmin => write!(f, "super_admin"),
        }
    }
}

/// 调用者信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerInfo {
    /// 调用者角色.
    pub role: CallerRole,
    /// 可选的用户 ID.
    pub user_id: Option<String>,
}

impl CallerInfo {
    pub fn anonymous() -> Self {
        Self {
            role: CallerRole::Anonymous,
            user_id: None,
        }
    }

    pub fn user(id: impl Into<String>) -> Self {
        Self {
            role: CallerRole::User,
            user_id: Some(id.into()),
        }
    }

    pub fn admin(id: impl Into<String>) -> Self {
        Self {
            role: CallerRole::Admin,
            user_id: Some(id.into()),
        }
    }
}

/// 工具权限检查器.
#[derive(Debug, Clone)]
pub struct ToolPermission {
    /// 允许覆盖单个工具的权限级别.
    overrides: std::collections::HashMap<String, PermissionLevel>,
}

impl ToolPermission {
    pub fn new() -> Self {
        Self {
            overrides: std::collections::HashMap::new(),
        }
    }

    /// 为特定工具设置权限覆盖.
    pub fn set_override(
        &mut self,
        tool_name: impl Into<String>,
        level: PermissionLevel,
    ) {
        self.overrides.insert(tool_name.into(), level);
    }

    /// 批量设置权限覆盖.
    pub fn set_overrides(
        &mut self,
        overrides: impl IntoIterator<Item = (impl Into<String>, PermissionLevel)>,
    ) {
        for (name, level) in overrides {
            self.overrides.insert(name.into(), level);
        }
    }

    /// 检查调用者是否有权执行指定工具.
    ///
    /// `tool_level` 是工具的权限级别，`caller` 是调用者信息。
    /// 如果 `overrides` 中有对此工具的覆盖，则使用覆盖值。
    pub fn check(
        &self,
        tool_name: &str,
        tool_level: &PermissionLevel,
        caller: &CallerInfo,
    ) -> LsResult<()> {
        // 使用覆盖值（如果有）
        let effective_level = self
            .overrides
            .get(tool_name)
            .unwrap_or(tool_level);

        let caller_rank = match &caller.role {
            CallerRole::SuperAdmin => 3u8,
            CallerRole::Admin => 2,
            CallerRole::User => 1,
            CallerRole::Anonymous => 0,
        };

        let required_rank = match effective_level {
            PermissionLevel::SuperAdmin => 3u8,
            PermissionLevel::Admin => 2,
            PermissionLevel::User => 1,
            PermissionLevel::Public => 0,
        };

        if caller_rank >= required_rank {
            Ok(())
        } else {
            Err(LsError::PermissionDenied(format!(
                "工具 '{tool_name}' 需要 {effective_level} 权限，当前调用者角色为 {}",
                caller.role
            )))
        }
    }

    /// 获取工具的有效权限级别（考虑覆盖）.
    pub fn effective_level(
        &self,
        tool_name: &str,
        tool_level: &PermissionLevel,
    ) -> PermissionLevel {
        self.overrides
            .get(tool_name)
            .cloned()
            .unwrap_or(*tool_level)
    }

    /// 列出所有被覆盖的工具权限.
    pub fn list_overrides(&self) -> &std::collections::HashMap<String, PermissionLevel> {
        &self.overrides
    }
}

impl Default for ToolPermission {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_traits::tool::PermissionLevel;

    #[test]
    fn test_public_tool_anyone_can_call() {
        let perm = ToolPermission::new();
        assert!(perm.check("echo", &PermissionLevel::Public, &CallerInfo::anonymous()).is_ok());
        assert!(perm.check("echo", &PermissionLevel::Public, &CallerInfo::user("u1")).is_ok());
    }

    #[test]
    fn test_admin_tool_requires_admin() {
        let perm = ToolPermission::new();
        assert!(perm.check("admin_panel", &PermissionLevel::Admin, &CallerInfo::anonymous()).is_err());
        assert!(perm.check("admin_panel", &PermissionLevel::Admin, &CallerInfo::user("u1")).is_err());
        assert!(perm.check("admin_panel", &PermissionLevel::Admin, &CallerInfo::admin("a1")).is_ok());
    }

    #[test]
    fn test_override_permission() {
        let mut perm = ToolPermission::new();
        perm.set_override("dangerous_tool", PermissionLevel::SuperAdmin);
        assert!(perm.check("dangerous_tool", &PermissionLevel::Public, &CallerInfo::admin("a1")).is_err());
        assert!(perm.check("dangerous_tool", &PermissionLevel::Public, &CallerInfo::anonymous()).is_err());
    }

    #[test]
    fn test_role_ordering() {
        assert!(CallerRole::SuperAdmin > CallerRole::Admin);
        assert!(CallerRole::Admin > CallerRole::User);
        assert!(CallerRole::User > CallerRole::Anonymous);
    }
}
