use crate::id::LsId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// LsContext — 异步请求上下文，绑定 session_id + trace_id。
///
/// 所有跨越模块边界的调用必须携带 LsContext，确保全链路可追溯。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsContext {
    /// 会话唯一标识.
    pub session_id: LsId,
    /// 全链路追踪 ID.
    pub trace_id: LsId,
    /// 可选租户 ID (多租户场景).
    pub tenant_id: Option<String>,
    /// 可选用户 ID.
    pub user_id: Option<String>,
    /// 上下文创建时间.
    pub created_at: DateTime<Utc>,
    /// 扩展属性 (自定义元数据).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl LsContext {
    /// 创建一个新的 LsContext (自动生成 session_id 与 trace_id).
    pub fn new(session_id: LsId, trace_id: LsId) -> Self {
        Self {
            session_id,
            trace_id,
            tenant_id: None,
            user_id: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// 快速构建上下文，自动生成 trace_id.
    pub fn with_session(session_id: LsId) -> Self {
        Self::new(session_id, LsId::new())
    }

    /// 设置租户 ID.
    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// 设置用户 ID.
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// 插入扩展元数据.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// 派生一个子上下文 (新 trace_id, 继承 session).
    pub fn child(&self) -> Self {
        let mut ctx = Self::new(self.session_id, LsId::new());
        ctx.tenant_id.clone_from(&self.tenant_id);
        ctx.user_id.clone_from(&self.user_id);
        ctx.metadata.clone_from(&self.metadata);
        ctx
    }
}

/// 线程安全上下文引用，用于跨任务传递.
pub type SharedContext = Arc<LsContext>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let sid = LsId::new();
        let ctx = LsContext::with_session(sid);
        assert_eq!(ctx.session_id, sid);
        assert!(!ctx.trace_id.is_nil());
    }

    #[test]
    fn test_child_inherits_session() {
        let ctx = LsContext::with_session(LsId::new())
            .with_user("alice")
            .with_tenant("acme");
        let child = ctx.child();
        assert_eq!(child.session_id, ctx.session_id);
        assert_eq!(child.user_id, ctx.user_id);
        assert_eq!(child.tenant_id, ctx.tenant_id);
        assert_ne!(child.trace_id, ctx.trace_id);
    }
}
