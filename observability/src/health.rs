//! Health — 健康检查体系。
//!
//! 提供统一的健康检查注册表和 HTTP 端点。
//!
//! ## 端点
//! - `GET /health` — 基础健康检查 (always 200)
//! - `GET /health/ready` — 就绪检查 (依赖所有注册的检查项)
//! - `GET /health/live` — 存活检查

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 健康检查结果.
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    /// 组件名称.
    pub component: String,
    /// 是否健康.
    pub healthy: bool,
    /// 详细信息.
    pub message: String,
    /// 检查时间戳.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

/// 健康检查响应.
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    /// 总体健康状态.
    pub healthy: bool,
    /// 服务名称.
    pub service: String,
    /// 服务版本.
    pub version: String,
    /// 检查时间.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub checked_at: chrono::DateTime<chrono::Utc>,
    /// 各组件状态.
    pub checks: Vec<HealthStatus>,
}

/// 健康检查器 trait — 各组件实现此接口注册检查.
#[async_trait]
pub trait HealthCheck: Send + Sync + 'static {
    /// 组件名称.
    fn component_name(&self) -> &str;
    /// 执行健康检查.
    async fn check(&self, ctx: &LsContext) -> LsResult<HealthStatus>;
}

/// 健康检查注册表.
pub struct HealthRegistry {
    checks: Arc<RwLock<Vec<Box<dyn HealthCheck>>>>,
    service_name: String,
    service_version: String,
}

impl HealthRegistry {
    /// 创建新的健康检查注册表.
    pub fn new(service_name: &str, service_version: &str) -> Self {
        Self {
            checks: Arc::new(RwLock::new(Vec::new())),
            service_name: service_name.to_string(),
            service_version: service_version.to_string(),
        }
    }

    /// 注册一个健康检查组件.
    pub async fn register(&self, check: Box<dyn HealthCheck>) {
        let mut checks = self.checks.write().await;
        checks.push(check);
    }

    /// 执行所有注册的健康检查.
    pub async fn check_all(&self) -> HealthResponse {
        let ctx = LsContext::new(LsId::new(), LsId::new());
        let mut checks = Vec::new();

        for checker in self.checks.read().await.iter() {
            let status = checker.check(&ctx).await.unwrap_or_else(|e| HealthStatus {
                component: checker.component_name().to_string(),
                healthy: false,
                message: format!("health check failed: {e}"),
                checked_at: chrono::Utc::now(),
            });
            checks.push(status);
        }

        let all_healthy = checks.iter().all(|c| c.healthy);

        HealthResponse {
            healthy: all_healthy,
            service: self.service_name.clone(),
            version: self.service_version.clone(),
            checked_at: chrono::Utc::now(),
            checks,
        }
    }

    /// 简单存活检查 (无需依赖).
    pub fn live(&self) -> HealthResponse {
        HealthResponse {
            healthy: true,
            service: self.service_name.clone(),
            version: self.service_version.clone(),
            checked_at: chrono::Utc::now(),
            checks: vec![HealthStatus {
                component: "liveness".into(),
                healthy: true,
                message: "alive".into(),
                checked_at: chrono::Utc::now(),
            }],
        }
    }

    /// 获取注册的检查数量.
    pub async fn check_count(&self) -> usize {
        self.checks.read().await.len()
    }
}

// ═══════════════════════════════════════════════════
// 内置健康检查器
// ═══════════════════════════════════════════════════

/// 运行时健康检查.
pub struct RuntimeHealth {
    name: String,
    is_ready: Arc<tokio::sync::watch::Receiver<bool>>,
}

impl RuntimeHealth {
    pub fn new(name: &str, is_ready: Arc<tokio::sync::watch::Receiver<bool>>) -> Self {
        Self {
            name: name.to_string(),
            is_ready,
        }
    }
}

#[async_trait]
impl HealthCheck for RuntimeHealth {
    fn component_name(&self) -> &str {
        &self.name
    }

    async fn check(&self, _ctx: &LsContext) -> LsResult<HealthStatus> {
        let ready = *self.is_ready.borrow();
        Ok(HealthStatus {
            component: self.name.clone(),
            healthy: ready,
            message: if ready {
                "runtime is ready".into()
            } else {
                "runtime not ready".into()
            },
            checked_at: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHealth {
        name: String,
        healthy: bool,
    }

    #[async_trait]
    impl HealthCheck for MockHealth {
        fn component_name(&self) -> &str {
            &self.name
        }
        async fn check(&self, _ctx: &LsContext) -> LsResult<HealthStatus> {
            Ok(HealthStatus {
                component: self.name.clone(),
                healthy: self.healthy,
                message: "mock check".into(),
                checked_at: chrono::Utc::now(),
            })
        }
    }

    #[tokio::test]
    async fn test_health_registry() {
        let reg = HealthRegistry::new("test", "1.0.0");
        reg.register(Box::new(MockHealth {
            name: "db".into(),
            healthy: true,
        }))
        .await;
        reg.register(Box::new(MockHealth {
            name: "cache".into(),
            healthy: true,
        }))
        .await;

        let resp = reg.check_all().await;
        assert!(resp.healthy);
        assert_eq!(resp.checks.len(), 2);
    }

    #[tokio::test]
    async fn test_unhealthy_component() {
        let reg = HealthRegistry::new("test", "1.0.0");
        reg.register(Box::new(MockHealth {
            name: "db".into(),
            healthy: false,
        }))
        .await;

        let resp = reg.check_all().await;
        assert!(!resp.healthy);
    }

    #[test]
    fn test_live_check() {
        let reg = HealthRegistry::new("test", "1.0.0");
        let resp = reg.live();
        assert!(resp.healthy);
        assert_eq!(resp.checks.len(), 1);
    }
}
