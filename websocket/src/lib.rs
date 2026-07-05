//! LSWs — WebSocket connection management and event broadcasting.
//!
//! ## Modules
//! - `types` — Protocol types (ServerMessage, ClientMessage, SseEvent)
//! - `connection` — ConnectionManager: track, broadcast, reap idle
//! - `broadcast` — SseBroadcaster + EventBridge: bridge EventBus → WS/SSE
//!
//! ## Example
//! ```rust,no_run
//! use lingshu_websocket::{ConnectionManager, SseBroadcaster, EventBridge};
//! use std::sync::Arc;
//!
//! let ws = Arc::new(ConnectionManager::new(300));
//! let sse = Arc::new(SseBroadcaster::new(1024));
//! let bridge = EventBridge::new(ws.clone(), sse.clone());
//! ```

pub mod broadcast;
pub mod connection;
pub mod types;

pub use broadcast::{EventBridge, SseBroadcaster};
pub use connection::{Connection, ConnectionManager};
pub use types::{ClientMessage, ConnectionState, ServerMessage, SseEvent, UsageInfo};
