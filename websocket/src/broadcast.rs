//! Event broadcaster — bridge between EventBus and WebSocket/SSE clients

use crate::connection::ConnectionManager;
use crate::types::{ServerMessage, SseEvent};
use lingshu_traits::event_bus::EventBus;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// SSE broadcast channel for server-sent events
pub struct SseBroadcaster {
    tx: broadcast::Sender<SseEvent>,
}

impl SseBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Subscribe to all SSE events
    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.tx.subscribe()
    }

    /// Publish an SSE event
    pub fn publish(&self, event: SseEvent) {
        if let Err(e) = self.tx.send(event) {
            warn!("SSE broadcast: no subscribers (channel closed): {}", e);
        }
    }
}

/// Bridge: listens to EventBus and forwards to WS/SSe
pub struct EventBridge {
    ws_manager: Arc<ConnectionManager>,
    sse_broadcaster: Arc<SseBroadcaster>,
}

impl EventBridge {
    pub fn new(ws_manager: Arc<ConnectionManager>, sse_broadcaster: Arc<SseBroadcaster>) -> Self {
        Self {
            ws_manager,
            sse_broadcaster,
        }
    }

    /// Forward an event to all WebSocket clients
    pub async fn broadcast_ws(&self, event: &str, data: Value) {
        let msg = ServerMessage::Event {
            event: event.to_string(),
            data,
        };
        self.ws_manager.broadcast(&msg).await;
    }

    /// Forward an event to SSE subscribers
    pub fn broadcast_sse(&self, event: &str, data: Value) {
        let sse = SseEvent::new(event, data);
        self.sse_broadcaster.publish(sse);
    }

    /// Forward an event to both WS and SSE
    pub async fn broadcast_all(&self, event: &str, data: Value) {
        self.broadcast_sse(event, data.clone());
        self.broadcast_ws(event, data).await;
    }

    /// Start listening to the internal EventBus
    pub async fn listen_to_eventbus<E: EventBus>(&self, _bus: Arc<E>) {
        // Subscribe to all events using the bus's event stream
        tokio::spawn(async move {
            // Poll for events - in production use bus.subscribe()
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                // Forward events from bus to WS/SSE
                // This is a placeholder — in production, subscribe to bus.topics()
            }
        });

        info!("EventBridge started — listening to EventBus");
    }
}

/// Predefined SSE event types
pub mod events {
    use crate::types::SseEvent;
    use serde_json::json;

    /// Agent state change notification
    pub fn agent_state_change(agent_id: &str, state: &str) -> SseEvent {
        SseEvent::new(
            "agent.state_change",
            json!({
                "agent_id": agent_id,
                "state": state,
            }),
        )
    }

    /// Workflow progress update
    pub fn workflow_progress(workflow_id: &str, step: &str, progress: f64) -> SseEvent {
        SseEvent::new(
            "workflow.progress",
            json!({
                "workflow_id": workflow_id,
                "step": step,
                "progress": progress,
            }),
        )
    }

    /// Token usage report
    pub fn token_usage(session_id: &str, prompt_tokens: u64, completion_tokens: u64) -> SseEvent {
        SseEvent::new(
            "token.usage",
            json!({
                "session_id": session_id,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "total_tokens": prompt_tokens + completion_tokens,
            }),
        )
    }

    /// System status notification
    pub fn system_status(level: &str, message: &str) -> SseEvent {
        SseEvent::new(
            "system.status",
            json!({
                "level": level,
                "message": message,
            }),
        )
    }

    /// Log entry (for real-time log streaming)
    pub fn log_entry(target: &str, level: &str, message: &str) -> SseEvent {
        SseEvent::new(
            "log.entry",
            json!({
                "target": target,
                "level": level,
                "message": message,
            }),
        )
    }

    /// Knowledge graph update notification
    pub fn graph_updated(project: &str, node_count: usize, edge_count: usize) -> SseEvent {
        SseEvent::new(
            "graph.updated",
            json!({
                "project": project,
                "node_count": node_count,
                "edge_count": edge_count,
            }),
        )
    }
}
