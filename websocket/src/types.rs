//! WebSocket message types and protocol definitions

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Connection established
    Connected {
        session_id: String,
        protocol_version: String,
    },
    /// LLM response chunk (streaming)
    Chunk { content: String, index: u32 },
    /// Tool call from LLM
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    /// Tool execution result
    ToolResult { id: String, content: String },
    /// Stream complete
    Done {
        full_content: String,
        finish_reason: Option<String>,
        usage: UsageInfo,
    },
    /// Server event (non-chat)
    Event { event: String, data: Value },
    /// Heartbeat (ping)
    Ping { timestamp: i64 },
    /// Error message
    Error { message: String, code: String },
}

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Chat prompt
    Prompt {
        prompt: String,
        model: Option<String>,
        stream: Option<bool>,
    },
    /// Cancel current stream
    Cancel,
    /// Tool result response
    ToolResult { id: String, content: String },
    /// Heartbeat (pong)
    Pong { timestamp: i64 },
    /// Close connection
    Close { reason: Option<String> },
}

/// Usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// SSE event for server-sent events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    pub event: String,
    pub data: Value,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

impl SseEvent {
    pub fn new(event: &str, data: Value) -> Self {
        Self {
            event: event.to_string(),
            data,
            id: Some(uuid::Uuid::new_v4().to_string()),
            retry: Some(3000),
        }
    }

    pub fn to_sse_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("event: {}\n", self.event));
        s.push_str(&format!("data: {}\n", self.data.to_string()));
        if let Some(ref id) = self.id {
            s.push_str(&format!("id: {}\n", id));
        }
        if let Some(ref retry) = self.retry {
            s.push_str(&format!("retry: {}\n", retry));
        }
        s.push('\n');
        s
    }
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Streaming,
    Cancelling,
    Closed,
}
