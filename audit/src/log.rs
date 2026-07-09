//! 审计日志 — 不可变、追加写入的审计事件存储.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 审计事件类型.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// 用户登录.
    UserLogin,
    /// 用户登出.
    UserLogout,
    /// API 调用.
    ApiCall,
    /// Agent 执行.
    AgentExecution,
    /// 管理员操作.
    AdminAction,
    /// 配置变更.
    ConfigChange,
    /// 权限变更.
    PermissionChange,
    /// 系统事件.
    System,
    /// 自定义事件.
    Custom(String),
}

/// 审计日志条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// 全局唯一 ID.
    pub id: Uuid,
    /// 事件发生时间.
    pub timestamp: DateTime<Utc>,
    /// 事件类型.
    pub event_type: AuditEventType,
    /// 事件名称（如 "user.login", "agent.run"）.
    pub event_name: String,
    /// 操作者（用户 ID 或服务名）.
    pub actor: String,
    /// 操作目标资源类型.
    pub resource_type: String,
    /// 操作目标资源 ID.
    pub resource_id: String,
    /// 操作详情（JSON 字符串）.
    pub detail: String,
    /// 请求追踪 ID（用于关联）.
    pub trace_id: Option<String>,
    /// 来源 IP 或服务.
    pub source: Option<String>,
    /// 操作结果 (success / failure).
    pub result: String,
}

impl AuditEntry {
    /// 创建新的审计日志条目.
    pub fn new(
        event_type: AuditEventType,
        event_name: &str,
        actor: &str,
        resource_type: &str,
        resource_id: &str,
        detail: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            event_name: event_name.to_string(),
            actor: actor.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            detail: detail.to_string(),
            trace_id: None,
            source: None,
            result: "success".into(),
        }
    }

    /// 设置追踪 ID.
    pub fn with_trace_id(mut self, trace_id: &str) -> Self {
        self.trace_id = Some(trace_id.to_string());
        self
    }

    /// 设置来源.
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// 标记结果为失败.
    pub fn with_failure(mut self) -> Self {
        self.result = "failure".into();
        self
    }
}

/// 审计日志存储 trait.
#[async_trait]
pub trait AuditLogStore: Send + Sync {
    /// 追加写入审计条目.
    async fn append(&self, entry: AuditEntry) -> LsResult<()>;

    /// 根据 ID 查询条目.
    async fn get_by_id(&self, id: Uuid) -> LsResult<AuditEntry>;

    /// 查询条目列表.
    async fn query(&self, q: &super::query::AuditQuery) -> LsResult<Vec<AuditEntry>>;

    /// 获取条目总数.
    async fn count(&self, q: &super::query::AuditQuery) -> LsResult<u64>;
}

/// 内存审计日志实现.
#[derive(Clone)]
pub struct AuditLog {
    entries: Arc<RwLock<Vec<AuditEntry>>>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditLogStore for AuditLog {
    async fn append(&self, entry: AuditEntry) -> LsResult<()> {
        self.entries.write().await.push(entry);
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> LsResult<AuditEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("audit entry {id}")))
    }

    async fn query(&self, q: &super::query::AuditQuery) -> LsResult<Vec<AuditEntry>> {
        let entries = self.entries.read().await;
        let filtered = apply_query(&entries, q);
        Ok(filtered)
    }

    async fn count(&self, q: &super::query::AuditQuery) -> LsResult<u64> {
        let entries = self.entries.read().await;
        let filtered = apply_query(&entries, q);
        Ok(filtered.len() as u64)
    }
}

fn apply_query(entries: &[AuditEntry], q: &super::query::AuditQuery) -> Vec<AuditEntry> {
    let mut results: Vec<AuditEntry> = entries
        .iter()
        .filter(|e| {
            if let Some(ref actor) = q.actor {
                if e.actor != *actor {
                    return false;
                }
            }
            if let Some(ref event_type) = q.event_type {
                if e.event_type != *event_type {
                    return false;
                }
            }
            if let Some(ref event_name) = q.event_name {
                if e.event_name != *event_name {
                    return false;
                }
            }
            if let Some(ref resource_type) = q.resource_type {
                if e.resource_type != *resource_type {
                    return false;
                }
            }
            if let Some(ref resource_id) = q.resource_id {
                if e.resource_id != *resource_id {
                    return false;
                }
            }
            if let Some(ref start) = q.start_time {
                if e.timestamp < *start {
                    return false;
                }
            }
            if let Some(ref end) = q.end_time {
                if e.timestamp > *end {
                    return false;
                }
            }
            if let Some(ref result) = q.result {
                if e.result != *result {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    // 按时间降序排列
    results.sort_by_key(|a| std::cmp::Reverse(a.timestamp));

    // 分页
    let offset = q.offset.unwrap_or(0) as usize;
    let limit = q.limit.unwrap_or(100) as usize;
    results.into_iter().skip(offset).take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::AuditQueryBuilder;

    #[tokio::test]
    async fn test_append_and_query() {
        let log = AuditLog::new();
        let entry = AuditEntry::new(
            AuditEventType::UserLogin,
            "user.login",
            "alice",
            "user",
            "user-123",
            r#"{"ip": "192.168.1.1"}"#,
        );
        log.append(entry).await.unwrap();

        let q = AuditQueryBuilder::new().with_actor("alice").build();
        let results = log.query(&q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, "alice");
    }

    #[tokio::test]
    async fn test_get_by_id() {
        let log = AuditLog::new();
        let entry = AuditEntry::new(
            AuditEventType::System,
            "system.startup",
            "system",
            "service",
            "lingshu",
            "{}",
        );
        let id = entry.id;
        log.append(entry).await.unwrap();

        let found = log.get_by_id(id).await.unwrap();
        assert_eq!(found.id, id);
    }

    #[tokio::test]
    async fn test_not_found() {
        let log = AuditLog::new();
        let result = log.get_by_id(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filter_by_event_type() {
        let log = AuditLog::new();
        log.append(AuditEntry::new(
            AuditEventType::UserLogin,
            "user.login",
            "alice",
            "user",
            "u1",
            "{}",
        ))
        .await
        .unwrap();
        log.append(AuditEntry::new(
            AuditEventType::AdminAction,
            "admin.delete",
            "admin",
            "user",
            "u2",
            "{}",
        ))
        .await
        .unwrap();

        let q = AuditQueryBuilder::new()
            .with_event_type(AuditEventType::AdminAction)
            .build();
        let results = log.query(&q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_name, "admin.delete");
    }

    #[tokio::test]
    async fn test_pagination() {
        let log = AuditLog::new();
        for i in 0..10 {
            log.append(AuditEntry::new(
                AuditEventType::System,
                &format!("event.{i}"),
                "system",
                "test",
                &format!("res-{i}"),
                "{}",
            ))
            .await
            .unwrap();
        }

        let q = AuditQueryBuilder::new().with_limit(3).build();
        let results = log.query(&q).await.unwrap();
        assert_eq!(results.len(), 3);
    }
}
