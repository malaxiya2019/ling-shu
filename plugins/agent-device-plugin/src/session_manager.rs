//! 📱 Session Manager — 管理多个 agent-device 设备会话
//!
//! 跟踪多设备多应用的生命周期，确保工具调用在正确的会话上下文中执行。
//! 自动管理 `open`/`close` 配对，提供会话隔离和资源清理。

use std::collections::HashMap;
use std::sync::Arc;

use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 会话平台类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SessionPlatform {
    /// iOS 模拟器/真机
    Ios,
    /// Android 模拟器/真机
    Android,
    /// Linux 桌面
    Linux,
    /// Web 浏览器
    Web,
    /// tvOS
    Tvos,
    /// 未知平台
    Unknown(String),
}

impl From<&str> for SessionPlatform {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ios" | "ipados" => SessionPlatform::Ios,
            "android" => SessionPlatform::Android,
            "linux" => SessionPlatform::Linux,
            "web" => SessionPlatform::Web,
            "tvos" => SessionPlatform::Tvos,
            other => SessionPlatform::Unknown(other.to_string()),
        }
    }
}

impl std::fmt::Display for SessionPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionPlatform::Ios => write!(f, "ios"),
            SessionPlatform::Android => write!(f, "android"),
            SessionPlatform::Linux => write!(f, "linux"),
            SessionPlatform::Web => write!(f, "web"),
            SessionPlatform::Tvos => write!(f, "tvos"),
            SessionPlatform::Unknown(s) => write!(f, "{}", s),
        }
    }
}

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// 会话唯一 ID
    pub id: String,
    /// 显示的会话名称
    pub name: String,
    /// 平台
    pub platform: SessionPlatform,
    /// 当前运行的 App bundle ID 或包名
    pub app: Option<String>,
    /// 创建时间
    pub created_at: String,
    /// 最后活动时间
    pub last_active: String,
    /// 已使用的工具列表
    pub used_tools: Vec<String>,
    /// 会话标签
    pub tags: Vec<String>,
    /// 是否活跃
    pub active: bool,
}

/// 会话管理器
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建新会话
    pub async fn create_session(
        &self,
        name: String,
        platform: SessionPlatform,
        app: Option<String>,
        tags: Vec<String>,
    ) -> SessionInfo {
        let id = LsId::new().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let session = SessionInfo {
            id: id.clone(),
            name,
            platform,
            app,
            created_at: now.clone(),
            last_active: now,
            used_tools: Vec::new(),
            tags,
            active: true,
        };

        self.sessions
            .write()
            .await
            .insert(id.clone(), session.clone());
        info!(session_id = %id, "Agent-device session created");
        session
    }

    /// 获取会话
    pub async fn get_session(&self, id: &str) -> Option<SessionInfo> {
        self.sessions.read().await.get(id).cloned()
    }

    /// 获取所有会话
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// 获取活跃会话
    pub async fn active_sessions(&self) -> Vec<SessionInfo> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| s.active)
            .cloned()
            .collect()
    }

    /// 记录工具使用
    pub async fn record_tool_use(&self, session_id: &str, tool_name: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_active = chrono::Utc::now().to_rfc3339();
            session.used_tools.push(tool_name.to_string());
            debug!(
                session_id = %session_id,
                tool = %tool_name,
                "Tool use recorded"
            );
        }
    }

    /// 更新会话的 App
    pub async fn update_app(&self, session_id: &str, app: String) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.app = Some(app);
            session.last_active = chrono::Utc::now().to_rfc3339();
        }
    }

    /// 关闭会话
    pub async fn close_session(&self, session_id: &str) -> Option<SessionInfo> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.active = false;
            session.last_active = chrono::Utc::now().to_rfc3339();
            info!(session_id = %session_id, "Agent-device session closed");
            Some(session.clone())
        } else {
            warn!(session_id = %session_id, "Attempted to close non-existent session");
            None
        }
    }

    /// 删除会话
    pub async fn remove_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
        info!(session_id = %session_id, "Agent-device session removed");
    }

    /// 清理过期会话（超过指定时长未活跃）
    pub async fn clean_stale_sessions(&self, max_idle_secs: u64) -> Vec<String> {
        let now = chrono::Utc::now();
        let mut to_remove = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                if let Ok(last) = chrono::DateTime::parse_from_rfc3339(&session.last_active) {
                    let duration = now.signed_duration_since(last);
                    if duration.num_seconds() > max_idle_secs as i64 {
                        to_remove.push(id.clone());
                    }
                }
            }
        }

        for id in &to_remove {
            self.sessions.write().await.remove(id);
            info!(session_id = %id, "Stale session cleaned up");
        }

        to_remove
    }

    /// 获取会话统计
    pub async fn stats(&self) -> SessionStats {
        let sessions = self.sessions.read().await;
        let total = sessions.len();
        let active = sessions.values().filter(|s| s.active).count();
        let platform_count = {
            let mut counts = HashMap::new();
            for s in sessions.values() {
                *counts.entry(s.platform.to_string()).or_insert(0) += 1;
            }
            counts
        };

        SessionStats {
            total_sessions: total,
            active_sessions: active,
            platform_distribution: platform_count,
        }
    }
}

/// 会话统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub platform_distribution: HashMap<String, usize>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_platform_from_str() {
        assert_eq!(SessionPlatform::from("ios"), SessionPlatform::Ios);
        assert_eq!(SessionPlatform::from("android"), SessionPlatform::Android);
        assert_eq!(SessionPlatform::from("linux"), SessionPlatform::Linux);
        assert_eq!(SessionPlatform::from("web"), SessionPlatform::Web);
        assert_eq!(SessionPlatform::from("tvos"), SessionPlatform::Tvos);
        assert_eq!(
            SessionPlatform::from("windows"),
            SessionPlatform::Unknown("windows".to_string())
        );
    }

    #[tokio::test]
    async fn test_create_and_get_session() {
        let mgr = SessionManager::new();
        let session = mgr
            .create_session(
                "test-session".into(),
                SessionPlatform::Ios,
                Some("com.example.app".into()),
                vec!["test".into()],
            )
            .await;

        assert!(session.active);
        assert_eq!(session.platform, SessionPlatform::Ios);
        assert_eq!(session.app.as_deref(), Some("com.example.app"));

        let fetched = mgr.get_session(&session.id).await;
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "test-session");
    }

    #[tokio::test]
    async fn test_close_session() {
        let mgr = SessionManager::new();
        let session = mgr
            .create_session("close-test".into(), SessionPlatform::Android, None, vec![])
            .await;

        let closed = mgr.close_session(&session.id).await;
        assert!(closed.is_some());
        assert!(!closed.unwrap().active);

        // 再次获取应该显示 inactive
        let fetched = mgr.get_session(&session.id).await;
        assert!(fetched.is_some());
        assert!(!fetched.unwrap().active);
    }

    #[tokio::test]
    async fn test_close_nonexistent_session() {
        let mgr = SessionManager::new();
        let result = mgr.close_session("nonexistent-id").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_record_tool_use() {
        let mgr = SessionManager::new();
        let session = mgr
            .create_session("tool-test".into(), SessionPlatform::Ios, None, vec![])
            .await;

        mgr.record_tool_use(&session.id, "snapshot").await;
        mgr.record_tool_use(&session.id, "click").await;

        let fetched = mgr.get_session(&session.id).await.unwrap();
        assert_eq!(fetched.used_tools.len(), 2);
        assert_eq!(fetched.used_tools[0], "snapshot");
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let mgr = SessionManager::new();
        mgr.create_session("s1".into(), SessionPlatform::Ios, None, vec![])
            .await;
        mgr.create_session("s2".into(), SessionPlatform::Android, None, vec![])
            .await;

        let sessions = mgr.list_sessions().await;
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_active_sessions() {
        let mgr = SessionManager::new();
        mgr.create_session("active-1".into(), SessionPlatform::Ios, None, vec![])
            .await;
        let s2 = mgr
            .create_session("to-close".into(), SessionPlatform::Android, None, vec![])
            .await;
        mgr.close_session(&s2.id).await;

        let active = mgr.active_sessions().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "active-1");
    }

    #[tokio::test]
    async fn test_stats() {
        let mgr = SessionManager::new();
        mgr.create_session("s1".into(), SessionPlatform::Ios, None, vec![])
            .await;
        mgr.create_session("s2".into(), SessionPlatform::Android, None, vec![])
            .await;
        let s3 = mgr
            .create_session("s3".into(), SessionPlatform::Ios, None, vec![])
            .await;
        mgr.close_session(&s3.id).await;

        let stats = mgr.stats().await;
        assert_eq!(stats.total_sessions, 3);
        assert_eq!(stats.active_sessions, 2);
        assert_eq!(*stats.platform_distribution.get("ios").unwrap(), 2);
        assert_eq!(*stats.platform_distribution.get("android").unwrap(), 1);
    }

    #[tokio::test]
    async fn test_stale_session_cleanup() {
        let mgr = SessionManager::new();

        // 创建一个会话（默认创建时间）
        let s = mgr
            .create_session("stale".into(), SessionPlatform::Ios, None, vec![])
            .await;

        // 手动将 last_active 设置为较旧的时间来模拟过期
        {
            let mut sessions = mgr.sessions.write().await;
            if let Some(session) = sessions.get_mut(&s.id) {
                // 设置为 1 小时前
                let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
                session.last_active = past;
            }
        }

        // 清理 30 秒空闲的会话 → 应该清除这个 1 小时前的
        let cleaned = mgr.clean_stale_sessions(30).await;
        assert_eq!(cleaned.len(), 1);

        // 验证已删除
        let fetched = mgr.get_session(&s.id).await;
        assert!(fetched.is_none());
    }
}
