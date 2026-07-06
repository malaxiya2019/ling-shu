//! SessionMemoryManager — Maps session IDs to per-session memory instances

use crate::memory::{DefaultMemory, Memory};
use crate::types::MemoryConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages per-session memory instances
pub struct SessionMemoryManager {
    sessions: RwLock<HashMap<String, Arc<dyn Memory>>>,
    default_config: MemoryConfig,
}

impl SessionMemoryManager {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            default_config: config,
        }
    }

    /// Get or create memory for a session
    pub async fn get_or_create(&self, session_id: &str) -> Arc<dyn Memory> {
        let sessions = self.sessions.read().await;
        if let Some(mem) = sessions.get(session_id) {
            return mem.clone();
        }
        drop(sessions);

        let mut sessions = self.sessions.write().await;
        // Double-check after acquiring write lock
        if let Some(mem) = sessions.get(session_id) {
            return mem.clone();
        }
        let mem = Arc::new(DefaultMemory::new(session_id, self.default_config.clone()));
        sessions.insert(session_id.to_string(), mem.clone());
        mem
    }

    /// Get memory for a session (returns None if not found)
    pub async fn get(&self, session_id: &str) -> Option<Arc<dyn Memory>> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// Remove a session's memory
    pub async fn remove(&self, session_id: &str) -> bool {
        self.sessions.write().await.remove(session_id).is_some()
    }

    /// Clear all session memories
    pub async fn clear_all(&self) {
        self.sessions.write().await.clear();
    }

    /// Get active session count
    pub async fn active_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// List all active session IDs
    pub async fn list_sessions(&self) -> Vec<String> {
        self.sessions.read().await.keys().cloned().collect()
    }
}

impl Default for SessionMemoryManager {
    fn default() -> Self {
        Self::new(MemoryConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use lingshu_core::LsContext;
    use super::*;
    use lingshu_core::LsId;

    #[tokio::test]
#[allow(unused_variables)]
    async fn test_get_or_create() {
        let mgr = SessionMemoryManager::default();
        let mem = mgr.get_or_create("session-1").await;
        assert_eq!(mgr.active_count().await, 1);

        // Same session returns same instance
        let mem2 = mgr.get_or_create("session-1").await;
        assert_eq!(mgr.active_count().await, 1);

        // Different session creates new
        let mem3 = mgr.get_or_create("session-2").await;
        assert_eq!(mgr.active_count().await, 2);
    }

    #[tokio::test]
    async fn test_remove() {
        let mgr = SessionMemoryManager::default();
        mgr.get_or_create("session-1").await;
        assert!(mgr.remove("session-1").await);
        assert!(!mgr.remove("session-1").await);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let mgr = SessionMemoryManager::default();
        mgr.get_or_create("a").await;
        mgr.get_or_create("b").await;
        let sessions = mgr.list_sessions().await;
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"a".to_string()));
        assert!(sessions.contains(&"b".to_string()));
    }

    #[tokio::test]
    async fn test_store_and_recall() {
        let mgr = SessionMemoryManager::default();
        let ctx = LsContext::with_session(LsId::new());
        let mem = mgr.get_or_create("test-session").await;

        mem.store_message(&ctx, "user", "Hello").await.unwrap();
        let result = mem.recall(&ctx, &crate::types::MemoryQuery::default()).await.unwrap();
        assert_eq!(result.total, 1);
    }
}
