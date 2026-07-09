//! TenantManager — 租户 CRUD + 隔离校验.

use lingshu_core::{LsError, LsResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::*;

/// 租户管理器 — 内存实现 (生产环境应替换为数据库持久化).
#[derive(Debug)]
pub struct TenantManager {
    orgs: Arc<RwLock<HashMap<String, Organization>>>,
    projects: Arc<RwLock<HashMap<String, Project>>>,
    users: Arc<RwLock<HashMap<String, TenantUser>>>,
}

impl TenantManager {
    pub fn new() -> Self {
        Self {
            orgs: Arc::new(RwLock::new(HashMap::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Organization CRUD ─────────────────────────────

    pub async fn create_organization(
        &self,
        name: &str,
        slug: &str,
        owner_id: &str,
    ) -> LsResult<Organization> {
        let mut orgs = self.orgs.write().await;
        // 检查 slug 唯一性
        if orgs.values().any(|o| o.slug == slug) {
            return Err(LsError::AlreadyExists(format!("org slug '{slug}' already exists")));
        }
        let org = Organization::new(name, slug, owner_id, "");
        let id = org.id.clone();
        orgs.insert(id, org.clone());
        Ok(org)
    }

    pub async fn get_organization(&self, org_id: &str) -> LsResult<Organization> {
        let orgs = self.orgs.read().await;
        orgs.get(org_id).cloned().ok_or_else(|| {
            LsError::NotFound(format!("organization {org_id}"))
        })
    }

    pub async fn list_organizations(&self) -> LsResult<Vec<Organization>> {
        let orgs = self.orgs.read().await;
        let mut list: Vec<_> = orgs.values().cloned().collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    pub async fn update_organization(&self, org_id: &str, name: &str, description: &str) -> LsResult<Organization> {
        let mut orgs = self.orgs.write().await;
        let org = orgs.get_mut(org_id).ok_or_else(|| {
            LsError::NotFound(format!("organization {org_id}"))
        })?;
        org.name = name.to_string();
        org.description = description.to_string();
        org.updated_at = chrono::Utc::now();
        Ok(org.clone())
    }

    pub async fn delete_organization(&self, org_id: &str) -> LsResult<bool> {
        let mut orgs = self.orgs.write().await;
        let existed = orgs.remove(org_id).is_some();
        if !existed {
            return Err(LsError::NotFound(format!("organization {org_id}")));
        }
        // 同时删除该组织下的所有项目和用户
        let mut projs = self.projects.write().await;
        projs.retain(|_, p| p.org_id != org_id);
        let mut users = self.users.write().await;
        users.retain(|_, u| u.org_id != org_id);
        Ok(true)
    }

    pub async fn suspend_organization(&self, org_id: &str) -> LsResult<Organization> {
        let mut orgs = self.orgs.write().await;
        let org = orgs.get_mut(org_id).ok_or_else(|| {
            LsError::NotFound(format!("organization {org_id}"))
        })?;
        org.status = OrgStatus::Suspended;
        org.updated_at = chrono::Utc::now();
        Ok(org.clone())
    }

    // ── Project CRUD ──────────────────────────────────

    pub async fn create_project(&self, org_id: &str, name: &str, description: &str) -> LsResult<Project> {
        // 验证组织存在
        self.get_organization(org_id).await?;
        let mut projs = self.projects.write().await;
        let project = Project::new(org_id, name, description);
        let id = project.id.clone();
        projs.insert(id, project.clone());
        Ok(project)
    }

    pub async fn get_project(&self, project_id: &str) -> LsResult<Project> {
        let projs = self.projects.read().await;
        projs.get(project_id).cloned().ok_or_else(|| {
            LsError::NotFound(format!("project {project_id}"))
        })
    }

    pub async fn list_projects(&self, org_id: &str) -> LsResult<Vec<Project>> {
        self.get_organization(org_id).await?;
        let projs = self.projects.read().await;
        let mut list: Vec<_> = projs.values().filter(|p| p.org_id == org_id).cloned().collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    pub async fn update_project(&self, project_id: &str, name: &str, description: &str) -> LsResult<Project> {
        let mut projs = self.projects.write().await;
        let proj = projs.get_mut(project_id).ok_or_else(|| {
            LsError::NotFound(format!("project {project_id}"))
        })?;
        proj.name = name.to_string();
        proj.description = description.to_string();
        proj.updated_at = chrono::Utc::now();
        Ok(proj.clone())
    }

    pub async fn archive_project(&self, project_id: &str) -> LsResult<Project> {
        let mut projs = self.projects.write().await;
        let proj = projs.get_mut(project_id).ok_or_else(|| {
            LsError::NotFound(format!("project {project_id}"))
        })?;
        proj.status = ProjectStatus::Archived;
        proj.updated_at = chrono::Utc::now();
        Ok(proj.clone())
    }

    pub async fn delete_project(&self, project_id: &str) -> LsResult<bool> {
        let mut projs = self.projects.write().await;
        let existed = projs.remove(project_id).is_some();
        if !existed {
            return Err(LsError::NotFound(format!("project {project_id}")));
        }
        Ok(true)
    }

    // ── User CRUD ────────────────────────────────────

    pub async fn invite_user(&self, org_id: &str, email: &str, role: TenantRole) -> LsResult<TenantUser> {
        self.get_organization(org_id).await?;
        let mut users = self.users.write().await;
        if users.values().any(|u| u.email == email && u.org_id == org_id) {
            return Err(LsError::AlreadyExists(format!("user {email} already in org {org_id}")));
        }
        let user = TenantUser::new(org_id, email, email, role);
        let id = user.id.clone();
        users.insert(id, user.clone());
        Ok(user)
    }

    pub async fn get_user(&self, user_id: &str) -> LsResult<TenantUser> {
        let users = self.users.read().await;
        users.get(user_id).cloned().ok_or_else(|| {
            LsError::NotFound(format!("user {user_id}"))
        })
    }

    pub async fn list_users(&self, org_id: &str) -> LsResult<Vec<TenantUser>> {
        self.get_organization(org_id).await?;
        let users = self.users.read().await;
        let mut list: Vec<_> = users.values().filter(|u| u.org_id == org_id).cloned().collect();
        list.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(list)
    }

    pub async fn update_user_role(&self, user_id: &str, role: TenantRole) -> LsResult<TenantUser> {
        let mut users = self.users.write().await;
        let user = users.get_mut(user_id).ok_or_else(|| {
            LsError::NotFound(format!("user {user_id}"))
        })?;
        user.role = role;
        user.updated_at = chrono::Utc::now();
        Ok(user.clone())
    }

    pub async fn add_user_to_project(&self, user_id: &str, project_id: &str) -> LsResult<()> {
        let mut users = self.users.write().await;
        let user = users.get_mut(user_id).ok_or_else(|| {
            LsError::NotFound(format!("user {user_id}"))
        })?;
        if !user.project_ids.contains(&project_id.to_string()) {
            user.project_ids.push(project_id.to_string());
        }
        Ok(())
    }

    pub async fn remove_user(&self, user_id: &str) -> LsResult<bool> {
        let mut users = self.users.write().await;
        let existed = users.remove(user_id).is_some();
        if !existed {
            return Err(LsError::NotFound(format!("user {user_id}")));
        }
        Ok(true)
    }

    pub async fn disable_user(&self, user_id: &str) -> LsResult<TenantUser> {
        let mut users = self.users.write().await;
        let user = users.get_mut(user_id).ok_or_else(|| {
            LsError::NotFound(format!("user {user_id}"))
        })?;
        user.status = UserStatus::Disabled;
        user.updated_at = chrono::Utc::now();
        Ok(user.clone())
    }

    // ── 隔离校验 ─────────────────────────────────────

    /// 验证用户是否有权访问指定资源.
    pub async fn check_access(&self, user_id: &str, org_id: &str, required_permission: &str) -> LsResult<bool> {
        let users = self.users.read().await;
        let user = users.get(user_id).ok_or_else(|| {
            LsError::NotFound(format!("user {user_id}"))
        })?;
        if user.org_id != org_id {
            return Ok(false);
        }
        let perms = user.role.permissions();
        if perms.contains(&"*") {
            return Ok(true);
        }
        Ok(perms.contains(&required_permission))
    }

    /// 验证项目是否属于指定组织.
    pub async fn validate_project_org(&self, project_id: &str, org_id: &str) -> LsResult<bool> {
        let projs = self.projects.read().await;
        match projs.get(project_id) {
            Some(proj) => Ok(proj.org_id == org_id),
            None => Ok(false),
        }
    }

    /// 统计信息.
    pub async fn stats(&self) -> TenantStats {
        let orgs = self.orgs.read().await;
        let projs = self.projects.read().await;
        let users = self.users.read().await;
        TenantStats {
            total_orgs: orgs.len(),
            total_projects: projs.len(),
            total_users: users.len(),
        }
    }
}

impl Default for TenantManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 租户统计.
#[derive(Debug, Clone, Default)]
#[derive(serde::Serialize)]
#[allow(dead_code)]
pub struct TenantStats {
    pub total_orgs: usize,
    pub total_projects: usize,
    pub total_users: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_organization() {
        let mgr = TenantManager::new();
        let org = mgr.create_organization("Acme Corp", "acme", "user-1").await.unwrap();
        assert_eq!(org.name, "Acme Corp");
        assert_eq!(org.slug, "acme");
    }

    #[tokio::test]
    async fn test_duplicate_slug() {
        let mgr = TenantManager::new();
        mgr.create_organization("Acme", "acme", "u1").await.unwrap();
        let result = mgr.create_organization("Acme 2", "acme", "u2").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_project_under_org() {
        let mgr = TenantManager::new();
        let org = mgr.create_organization("Acme", "acme", "u1").await.unwrap();
        let proj = mgr.create_project(&org.id, "Project X", "").await.unwrap();
        assert_eq!(proj.org_id, org.id);
    }

    #[tokio::test]
    async fn test_invite_user() {
        let mgr = TenantManager::new();
        let org = mgr.create_organization("Acme", "acme", "u1").await.unwrap();
        let user = mgr.invite_user(&org.id, "alice@acme.com", TenantRole::Member).await.unwrap();
        assert_eq!(user.email, "alice@acme.com");
        assert_eq!(user.role, TenantRole::Member);
    }

    #[tokio::test]
    async fn test_role_permissions() {
        assert!(TenantRole::Owner.permissions().contains(&"*"));
        assert!(TenantRole::Viewer.permissions().contains(&"ls.agent.read"));
        assert!(!TenantRole::Viewer.permissions().contains(&"ls.agent.run"));
    }

    #[tokio::test]
    async fn test_stats() {
        let mgr = TenantManager::new();
        mgr.create_organization("O1", "o1", "u1").await.unwrap();
        mgr.create_organization("O2", "o2", "u2").await.unwrap();
        let stats = mgr.stats().await;
        assert_eq!(stats.total_orgs, 2);
    }
}
