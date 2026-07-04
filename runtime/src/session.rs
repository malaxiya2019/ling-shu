use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 会话状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Active,
    Expiring,
    Expired,
    Terminated,
}

/// 会话元信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: LsId,
    pub user_id: Option<String>,
    pub tenant_id: Option<String>,
    pub state: SessionState,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

/// 会话管理器.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<LsId, SessionInfo>>>,
    default_ttl_seconds: u64,
}

impl SessionManager {
    pub fn new(default_ttl_seconds: u64) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            default_ttl_seconds,
        }
    }

    /// 创建会话.
    pub async fn create(&self, ctx: &LsContext) -> LsResult<SessionInfo> {
        let now = chrono::Utc::now();
        let info = SessionInfo {
            session_id: ctx.session_id,
            user_id: ctx.user_id.clone(),
            tenant_id: ctx.tenant_id.clone(),
            state: SessionState::Active,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(self.default_ttl_seconds as i64),
            metadata: ctx.metadata.clone(),
        };
        let mut sessions = self.sessions.write().await;
        sessions.insert(ctx.session_id, info.clone());

        tracing::info!(
            trace_id = %ctx.trace_id,
            session_id = %ctx.session_id,
            user_id = %ctx.user_id.as_deref().unwrap_or("-"),
            ttl_seconds = self.default_ttl_seconds,
            "session created"
        );

        Ok(info)
    }

    /// 获取会话信息.
    pub async fn get(&self, session_id: LsId) -> LsResult<SessionInfo> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).cloned().ok_or_else(|| {
            LsError::SessionNotFound(session_id.to_string())
        })
    }

    /// 续期会话.
    pub async fn renew(&self, session_id: LsId, ttl_seconds: u64) -> LsResult<SessionInfo> {
        let mut sessions = self.sessions.write().await;
        let info = sessions.get_mut(&session_id).ok_or_else(|| {
            LsError::SessionNotFound(session_id.to_string())
        })?;
        if info.state == SessionState::Terminated {
            return Err(LsError::SessionExpired(session_id.to_string()));
        }
        info.expires_at = chrono::Utc::now() + chrono::Duration::seconds(ttl_seconds as i64);
        info.state = SessionState::Active;

        tracing::debug!(
            session_id = %session_id,
            ttl_seconds = ttl_seconds,
            "session renewed"
        );

        Ok(info.clone())
    }

    /// 终止会话.
    pub async fn terminate(&self, session_id: LsId) -> LsResult<()> {
        let mut sessions = self.sessions.write().await;
        let info = sessions.get_mut(&session_id).ok_or_else(|| {
            LsError::SessionNotFound(session_id.to_string())
        })?;
        info.state = SessionState::Terminated;

        tracing::info!(
            session_id = %session_id,
            "session terminated"
        );

        Ok(())
    }

    /// 清理过期会话.
    pub async fn clean_expired(&self) -> LsResult<u64> {
        let mut sessions = self.sessions.write().await;
        let now = chrono::Utc::now();
        let expired: Vec<LsId> = sessions
            .iter()
            .filter(|(_, info)| {
                info.expires_at < now && info.state != SessionState::Terminated
            })
            .map(|(id, _)| *id)
            .collect();
        let count = expired.len() as u64;
        for id in &expired {
            if let Some(info) = sessions.get_mut(id) {
                info.state = SessionState::Expired;
            }
        }

        if count > 0 {
            tracing::info!(expired_count = count, "expired sessions cleaned");
        }

        Ok(count)
    }

    /// 获取活跃会话数.
    pub async fn active_count(&self) -> u64 {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.state == SessionState::Active)
            .count() as u64
    }

    /// 获取所有会话信息.
    pub async fn list_all(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get() {
        let mgr = SessionManager::new(3600);
        let ctx = LsContext::with_session(LsId::new());
        let info = mgr.create(&ctx).await.unwrap();
        assert_eq!(info.session_id, ctx.session_id);
        assert_eq!(info.state, SessionState::Active);

        let fetched = mgr.get(ctx.session_id).await.unwrap();
        assert_eq!(fetched.session_id, ctx.session_id);
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let mgr = SessionManager::new(3600);
        let err = mgr.get(LsId::new()).await.unwrap_err();
        assert!(matches!(err, LsError::SessionNotFound(_)));
    }

    #[tokio::test]
    async fn test_renew_session() {
        let mgr = SessionManager::new(3600);
        let ctx = LsContext::with_session(LsId::new());
        mgr.create(&ctx).await.unwrap();

        let renewed = mgr.renew(ctx.session_id, 7200).await.unwrap();
        assert_eq!(renewed.state, SessionState::Active);
    }

    #[tokio::test]
    async fn test_terminate_session() {
        let mgr = SessionManager::new(3600);
        let ctx = LsContext::with_session(LsId::new());
        mgr.create(&ctx).await.unwrap();
        mgr.terminate(ctx.session_id).await.unwrap();
        let info = mgr.get(ctx.session_id).await.unwrap();
        assert_eq!(info.state, SessionState::Terminated);
    }

    #[tokio::test]
    async fn test_renew_terminated_fails() {
        let mgr = SessionManager::new(3600);
        let ctx = LsContext::with_session(LsId::new());
        mgr.create(&ctx).await.unwrap();
        mgr.terminate(ctx.session_id).await.unwrap();
        let err = mgr.renew(ctx.session_id, 3600).await.unwrap_err();
        assert!(matches!(err, LsError::SessionExpired(_)));
    }

    #[tokio::test]
    async fn test_active_count() {
        let mgr = SessionManager::new(3600);
        assert_eq!(mgr.active_count().await, 0);

        let ctx1 = LsContext::with_session(LsId::new());
        let ctx2 = LsContext::with_session(LsId::new());
        mgr.create(&ctx1).await.unwrap();
        mgr.create(&ctx2).await.unwrap();
        assert_eq!(mgr.active_count().await, 2);

        mgr.terminate(ctx1.session_id).await.unwrap();
        assert_eq!(mgr.active_count().await, 1);
    }

    #[tokio::test]
    async fn test_clean_expired() {
        let mgr = SessionManager::new(0); // 0 TTL = immediately expired
        let ctx = LsContext::with_session(LsId::new());
        mgr.create(&ctx).await.unwrap();

        // Small delay so expires_at passes
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let cleaned = mgr.clean_expired().await.unwrap();
        assert_eq!(cleaned, 1);
    }

    #[tokio::test]
    async fn test_list_all() {
        let mgr = SessionManager::new(3600);
        let ctx = LsContext::with_session(LsId::new());
        mgr.create(&ctx).await.unwrap();
        let all = mgr.list_all().await;
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_create_with_metadata() {
        let mgr = SessionManager::new(3600);
        let ctx = LsContext::with_session(LsId::new())
            .with_user("alice")
            .with_tenant("acme")
            .with_metadata("source", "test");
        let info = mgr.create(&ctx).await.unwrap();
        assert_eq!(info.user_id.as_deref(), Some("alice"));
        assert_eq!(info.tenant_id.as_deref(), Some("acme"));
        assert_eq!(info.metadata.get("source").map(|s| s.as_str()), Some("test"));
    }
}
