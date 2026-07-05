//! Connection manager — track active WebSocket connections and sessions

use crate::types::{ConnectionState, ServerMessage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// A single WebSocket connection
pub struct Connection {
    pub session_id: String,
    pub user_id: String,
    pub state: ConnectionState,
    pub connected_at: Instant,
    pub last_activity: Instant,
    pub tx: mpsc::UnboundedSender<String>,
    pub remote_addr: String,
    pub user_agent: String,
}

impl Connection {
    pub fn new(
        session_id: String,
        user_id: String,
        tx: mpsc::UnboundedSender<String>,
        remote_addr: String,
        user_agent: String,
    ) -> Self {
        Self {
            session_id,
            user_id,
            state: ConnectionState::Connected,
            connected_at: Instant::now(),
            last_activity: Instant::now(),
            tx,
            remote_addr,
            user_agent,
        }
    }

    /// Send a JSON message to this connection
    pub fn send(&self, msg: &ServerMessage) -> Result<(), String> {
        let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        self.tx.send(json).map_err(|e| format!("send failed: {}", e))
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
}

/// Connection manager — thread-safe registry of active WS connections
pub struct ConnectionManager {
    by_session: RwLock<HashMap<String, Arc<Connection>>>,
    by_user: RwLock<HashMap<String, Vec<String>>>, // user_id -> [session_ids]
    idle_timeout_secs: u64,
}

impl ConnectionManager {
    pub fn new(idle_timeout_secs: u64) -> Self {
        Self {
            by_session: RwLock::new(HashMap::new()),
            by_user: RwLock::new(HashMap::new()),
            idle_timeout_secs,
        }
    }

    /// Register a new connection
    pub async fn register(&self, conn: Connection) {
        let session_id = conn.session_id.clone();
        let user_id = conn.user_id.clone();
        let conn = Arc::new(conn);

        self.by_session.write().await.insert(session_id.clone(), conn);

        let mut by_user = self.by_user.write().await;
        by_user.entry(user_id).or_default().push(session_id);

        info!("WS connection registered");
    }

    /// Remove a connection
    pub async fn unregister(&self, session_id: &str) {
        self.by_session.write().await.remove(session_id);
        // Clean up user index (inefficient but works for now)
        let mut by_user = self.by_user.write().await;
        by_user.retain(|_, sessions| {
            sessions.retain(|s| s != session_id);
            !sessions.is_empty()
        });
        debug!("WS connection unregistered: {}", session_id);
    }

    /// Get a connection by session ID
    pub async fn get(&self, session_id: &str) -> Option<Arc<Connection>> {
        self.by_session.read().await.get(session_id).cloned()
    }

    /// Get all connections for a user
    pub async fn get_user_connections(&self, user_id: &str) -> Vec<Arc<Connection>> {
        let by_user = self.by_user.read().await;
        let sessions = match by_user.get(user_id) {
            Some(s) => s.clone(),
            None => return vec![],
        };
        drop(by_user);
        let by_session = self.by_session.read().await;
        sessions
            .iter()
            .filter_map(|s| by_session.get(s).cloned())
            .collect()
    }

    /// Broadcast a message to all connected clients
    pub async fn broadcast(&self, msg: &ServerMessage) {
        let sessions = self.by_session.read().await;
        let json = serde_json::to_string(msg).unwrap_or_default();
        for conn in sessions.values() {
            let _ = conn.tx.send(json.clone());
        }
    }

    /// Broadcast a message to a specific user's connections
    pub async fn broadcast_to_user(&self, user_id: &str, msg: &ServerMessage) {
        let conns = self.get_user_connections(user_id).await;
        let json = serde_json::to_string(msg).unwrap_or_default();
        for conn in &conns {
            let _ = conn.tx.send(json.clone());
        }
    }

    /// Get total active connections
    pub async fn active_count(&self) -> usize {
        self.by_session.read().await.len()
    }

    /// Get all session IDs
    pub async fn all_sessions(&self) -> Vec<String> {
        self.by_session.read().await.keys().cloned().collect()
    }

    /// Clean up stale connections (run periodically)
    pub async fn reap_idle(&self) -> Vec<String> {
        let mut reaped = Vec::new();
        let now = Instant::now();
        let timeout = std::time::Duration::from_secs(self.idle_timeout_secs);

        let sessions = self.by_session.read().await;
        for (sid, conn) in sessions.iter() {
            if now.duration_since(conn.last_activity) > timeout {
                reaped.push(sid.clone());
            }
        }
        drop(sessions);

        for sid in &reaped {
            self.unregister(sid).await;
            warn!("Reaped idle connection: {}", sid);
        }
        reaped
    }

    /// Update connection state
    pub async fn update_state(&self, session_id: &str, state: ConnectionState) {
        if let Some(conn) = self.by_session.write().await.get_mut(session_id) {
            if let Some(conn) = Arc::get_mut(conn) {
                conn.state = state;
            }
        }
    }

    /// Get connection state
    pub async fn get_state(&self, session_id: &str) -> Option<ConnectionState> {
        self.by_session.read().await.get(session_id).map(|c| c.state)
    }

    /// Update last activity timestamp
    pub async fn update_activity(&self, session_id: &str) {
        if let Some(conn) = self.by_session.write().await.get_mut(session_id) {
            if let Some(conn) = Arc::get_mut(conn) {
                conn.touch();
            }
        }
    }
}
