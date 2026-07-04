use chrono::{DateTime, Utc};
use lingshu_core::{LsId, LsResult};
use serde::{Deserialize, Serialize};

/// 审计日志条目.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub audit_id: LsId,
    /// 操作主体 ID.
    pub subject_id: String,
    /// 所属会话.
    pub session_id: LsId,
    /// 全链路追踪 ID.
    pub trace_id: LsId,
    /// 操作资源.
    pub resource: String,
    /// 操作动作.
    pub action: String,
    /// 操作结果 (success / denied / error).
    pub result: String,
    /// 来源 IP.
    pub source_ip: String,
    /// 操作时间.
    pub timestamp: DateTime<Utc>,
    /// 附加详情.
    pub details: Option<String>,
}

/// 审计日志记录器.
#[derive(Debug)]
pub struct AuditLogger;

impl AuditLogger {
    /// 记录审计条目.
    pub fn log(entry: AuditEntry) -> LsResult<()> {
        // 实际实现写入持久化审计存储
        tracing::info!(
            audit_id = %entry.audit_id,
            subject = %entry.subject_id,
            resource = %entry.resource,
            action = %entry.action,
            result = %entry.result,
            "audit log"
        );
        Ok(())
    }

    /// 快速记录权限拒绝.
    pub fn log_permission_denied(
        subject_id: &str,
        session_id: LsId,
        trace_id: LsId,
        resource: &str,
        action: &str,
        source_ip: &str,
    ) -> LsResult<()> {
        Self::log(AuditEntry {
            audit_id: LsId::new(),
            subject_id: subject_id.to_string(),
            session_id,
            trace_id,
            resource: resource.to_string(),
            action: action.to_string(),
            result: "denied".into(),
            source_ip: source_ip.to_string(),
            timestamp: Utc::now(),
            details: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_fields() {
        let entry = AuditEntry {
            audit_id: LsId::new(),
            subject_id: "user_abc".into(),
            session_id: LsId::new(),
            trace_id: LsId::new(),
            resource: "ls.runtime.agent".into(),
            action: "run".into(),
            result: "success".into(),
            source_ip: "192.168.1.1".into(),
            timestamp: Utc::now(),
            details: None,
        };
        assert_eq!(entry.resource, "ls.runtime.agent");
    }
}
