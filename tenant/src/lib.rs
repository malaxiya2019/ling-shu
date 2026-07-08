//! LSTenant — 组织/项目/用户三级多租户隔离。
//!
//! ## 层级结构
//! ```text
//! Organization (组织)
//!   └── Project (项目)
//!         └── User (用户)
//! ```
//!
//! ## Feature
//! - `LsContext` 已内置 `tenant_id` 字段
//! - 每个 API 请求自动注入 tenant 上下文
//! - RBAC 权限在租户范围内生效

pub mod manager;
pub mod models;

pub use manager::TenantManager;
pub use models::*;

use lingshu_core::LsResult;
use std::sync::Arc;

/// 租户系统初始化结果.
pub struct TenantSystem {
    pub manager: Arc<TenantManager>,
    pub default_org_id: String,
    pub default_project_id: String,
}

impl TenantSystem {
    /// 初始化租户系统，创建默认组织/项目.
    pub async fn initialize() -> LsResult<Self> {
        let manager = Arc::new(TenantManager::new());

        // 创建默认组织
        let default_org = manager
            .create_organization("Default Organization", "system", "system")
            .await?;
        let default_org_id = default_org.id.clone();

        // 创建默认项目
        let default_project = manager
            .create_project(&default_org_id, "Default Project", "system")
            .await?;
        let default_project_id = default_project.id.clone();

        Ok(Self {
            manager,
            default_org_id,
            default_project_id,
        })
    }
}
