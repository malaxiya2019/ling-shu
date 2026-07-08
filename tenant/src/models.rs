//! 多租户数据模型 — 组织/项目/用户三级结构.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 组织 — 顶层隔离单位.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub owner_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: OrgStatus,
    pub settings: OrgSettings,
}

/// 组织状态.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrgStatus {
    Active,
    Suspended,
    Disabled,
}

impl Default for OrgStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 组织设置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgSettings {
    pub max_projects: u32,
    pub max_users: u32,
    pub max_agents: u32,
    pub allowed_llm_providers: Vec<String>,
    pub enable_audit_log: bool,
    pub retention_days: u32,
}

impl Default for OrgSettings {
    fn default() -> Self {
        Self {
            max_projects: 50,
            max_users: 500,
            max_agents: 100,
            allowed_llm_providers: vec!["openai".into(), "anthropic".into()],
            enable_audit_log: true,
            retention_days: 90,
        }
    }
}

/// 项目 — 组织内的二级单位.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: ProjectStatus,
    pub settings: ProjectSettings,
}

/// 项目状态.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectStatus {
    Active,
    Archived,
    Frozen,
}

impl Default for ProjectStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 项目设置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub max_agents: u32,
    pub max_sessions: u32,
    pub token_quota_per_day: u64,
    pub enable_federation: bool,
    pub enable_plugins: bool,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            max_agents: 20,
            max_sessions: 100,
            token_quota_per_day: 1_000_000,
            enable_federation: true,
            enable_plugins: true,
        }
    }
}

/// 租户用户 — 绑定到组织/项目的用户.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantUser {
    pub id: String,
    pub org_id: String,
    pub project_ids: Vec<String>,
    pub email: String,
    pub display_name: String,
    pub role: TenantRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: UserStatus,
}

/// 租户角色.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TenantRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl TenantRole {
    pub fn permissions(&self) -> Vec<&str> {
        match self {
            TenantRole::Owner => vec!["*"],
            TenantRole::Admin => vec![
                "ls.tenant.org.read",
                "ls.tenant.org.write",
                "ls.tenant.project.read",
                "ls.tenant.project.write",
                "ls.tenant.user.read",
                "ls.tenant.user.write",
                "ls.agent.run",
                "ls.agent.read",
            ],
            TenantRole::Member => vec![
                "ls.tenant.project.read",
                "ls.agent.run",
                "ls.agent.read",
            ],
            TenantRole::Viewer => vec!["ls.agent.read"],
        }
    }
}

impl Default for TenantRole {
    fn default() -> Self {
        Self::Member
    }
}

/// 用户状态.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserStatus {
    Active,
    Invited,
    Disabled,
}

impl Default for UserStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 创建组织请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
}

/// 创建项目请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
}

/// 邀请用户请求.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteUserRequest {
    pub email: String,
    pub role: TenantRole,
}

impl Organization {
    pub fn new(name: &str, slug: &str, owner_id: &str, description: &str) -> Self {
        let now = Utc::now();
        Self {
            id: format!("org-{}", Uuid::new_v4()),
            name: name.to_string(),
            slug: slug.to_string(),
            description: description.to_string(),
            owner_id: owner_id.to_string(),
            created_at: now,
            updated_at: now,
            status: OrgStatus::Active,
            settings: OrgSettings::default(),
        }
    }
}

impl Project {
    pub fn new(org_id: &str, name: &str, description: &str) -> Self {
        let now = Utc::now();
        Self {
            id: format!("proj-{}", Uuid::new_v4()),
            org_id: org_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            created_at: now,
            updated_at: now,
            status: ProjectStatus::Active,
            settings: ProjectSettings::default(),
        }
    }
}

impl TenantUser {
    pub fn new(org_id: &str, email: &str, display_name: &str, role: TenantRole) -> Self {
        let now = Utc::now();
        Self {
            id: format!("user-{}", Uuid::new_v4()),
            org_id: org_id.to_string(),
            project_ids: Vec::new(),
            email: email.to_string(),
            display_name: display_name.to_string(),
            role,
            created_at: now,
            updated_at: now,
            status: UserStatus::Active,
        }
    }
}
