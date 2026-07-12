//! LSFed — 联邦协议消息.
//!
//! 定义了集群间通信的消息格式，基于 JSON-RPC 2.0 风格。
//!
//! ## 消息流
//!
//! ```text
//!  Cluster A                      Cluster B
//!     │                              │
//!     │── Hello (身份+能力) ──────►  │
//!     │◄── Hello Ack ───────────────│
//!     │── Heartbeat ───────────────►│
//!     │◄── Heartbeat Ack ──────────│
//!     │── CapabilityUpdate ────────►│
//!     │── RemoteExec ─────────────►│
//!     │◄── RemoteExec Result ──────│
//!     │── StateReplicate ─────────►│
//!     │◄── StateReplicate Ack ────│
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 联邦协议版本.
pub const FEDERATION_PROTOCOL_VERSION: &str = "2.0.0";

/// 联邦消息类型.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FederationMessage {
    // ── 握手 ──
    /// 握手：Hello.
    Hello(HelloPayload),
    /// 握手：Hello 确认.
    HelloAck(HelloAckPayload),

    // ── 心跳 ──
    /// 心跳.
    Heartbeat(HeartbeatPayload),
    /// 心跳确认.
    HeartbeatAck,

    // ── 能力 ──
    /// 能力更新广播.
    CapabilityUpdate(CapabilityUpdatePayload),
    /// 能力查询.
    CapabilityQuery,
    /// 能力查询响应.
    CapabilityList(CapabilityListPayload),

    // ── 远程执行 ──
    /// 远程执行请求.
    RemoteExecRequest(crate::types::RemoteExecRequest),
    /// 远程执行响应.
    RemoteExecResponse(crate::types::RemoteExecResponse),

    // ── 状态复制 ──
    /// 状态复制.
    StateReplicate(StateReplicatePayload),
    /// 状态复制确认.
    StateReplicateAck(StateReplicateAckPayload),

    // ── 错误 ──
    /// 协议错误.
    Error(ProtocolError),
}

// ── Hello ──────────────────────────────────────────

/// Hello 消息载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloPayload {
    /// 集群 ID.
    pub cluster_id: String,
    /// 集群名称.
    pub cluster_name: String,
    /// 协议版本.
    pub protocol_version: String,
    /// 监听地址.
    pub listen_addrs: Vec<String>,
    /// 能力摘要.
    pub capabilities: Vec<String>,
}

/// Hello 确认载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloAckPayload {
    /// 对端集群 ID.
    pub peer_cluster_id: String,
    /// 对端集群名称.
    pub peer_cluster_name: String,
    /// 协议版本.
    pub protocol_version: String,
    /// 是否兼容.
    pub compatible: bool,
    /// 不兼容原因.
    pub incompatible_reason: Option<String>,
}

// ── 心跳 ──────────────────────────────────────────

/// 心跳载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    /// 集群 ID.
    pub cluster_id: String,
    /// 时间戳.
    pub timestamp: i64,
    /// 负载指标（当前连接数、待处理任务数等）.
    pub load: LoadInfo,
}

/// 负载信息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadInfo {
    /// 活跃连接数.
    pub active_connections: u32,
    /// 待处理任务数.
    pub pending_tasks: u32,
    /// CPU 使用率（0-100）.
    pub cpu_percent: f64,
    /// 内存使用率（0-100）.
    pub memory_percent: f64,
}

// ── 能力 ──────────────────────────────────────────

/// 能力更新载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityUpdatePayload {
    /// 集群 ID.
    pub cluster_id: String,
    /// 能力完整列表.
    pub capabilities: Vec<crate::types::Capability>,
    /// 变更类型.
    pub change_type: CapabilityChange,
}

/// 能力变更类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityChange {
    /// 全量更新.
    Full,
    /// 新增.
    Added,
    /// 移除.
    Removed,
    /// 更新.
    Updated,
}

/// 能力列表载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListPayload {
    pub cluster_id: String,
    pub capabilities: Vec<crate::types::Capability>,
}

// ── 状态复制 ──────────────────────────────────────

/// 状态复制载荷.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateReplicatePayload {
    /// 状态键.
    pub key: String,
    /// 状态值.
    pub value: Value,
    /// 版本戳.
    pub version: u64,
    /// 所属命名空间.
    pub namespace: String,
}

/// 状态复制确认.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateReplicateAckPayload {
    pub key: String,
    pub version: u64,
    pub accepted: bool,
    pub error: Option<String>,
}

// ── 错误 ──────────────────────────────────────────

/// 协议错误.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolError {
    /// 错误码.
    pub code: i32,
    /// 错误消息.
    pub message: String,
    /// 关联消息 ID.
    pub related_message_id: Option<String>,
}

impl ProtocolError {
    pub fn new(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            related_message_id: None,
        }
    }
}

// ── 消息编解码 ────────────────────────────────────

/// 将联邦消息序列化为 JSON 字符串.
/// 编码联邦消息为 JSON 字符串.
pub fn encode_message(msg: &FederationMessage) -> Result<String, String> {
    serde_json::to_string(msg).map_err(|e| format!("encode failed: {e}"))
}

/// 从 JSON 字符串反序列化联邦消息.
/// 从 JSON 字符串解码联邦消息.
pub fn decode_message(data: &str) -> Result<FederationMessage, String> {
    serde_json::from_str(data).map_err(|e| format!("decode failed: {e}"))
}

/// 帧编码：在 JSON 消息前附加 4 字节长度头.
/// 编码联邦消息为长度前缀帧 (length-prefixed frame).
pub fn encode_frame(msg: &FederationMessage) -> Result<Vec<u8>, String> {
    let json = encode_message(msg)?;
    let json_bytes = json.as_bytes();
    let len = json_bytes.len() as u32;
    let mut frame = Vec::with_capacity(4 + json_bytes.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(json_bytes);
    Ok(frame)
}

/// 帧解码：从带长度头的字节流中提取消息.
/// 从字节流解码长度前缀帧，返回 (消息, 已消耗字节数).
pub fn decode_frame(data: &[u8]) -> Result<(FederationMessage, usize), String> {
    if data.len() < 4 {
        return Err("frame too short: missing length header".into());
    }
    let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + len {
        return Err(format!(
            "frame too short: need {len} bytes, got {}",
            data.len() - 4
        ));
    }
    let json = std::str::from_utf8(&data[4..4 + len]).map_err(|e| format!("invalid utf-8: {e}"))?;
    let msg = decode_message(json)?;
    Ok((msg, 4 + len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RemoteExecRequest;

    #[test]
    fn test_hello_roundtrip() {
        let msg = FederationMessage::Hello(HelloPayload {
            cluster_id: "cluster-1".into(),
            cluster_name: "East".into(),
            protocol_version: FEDERATION_PROTOCOL_VERSION.into(),
            listen_addrs: vec!["10.0.0.1:9550".into()],
            capabilities: vec!["gpt-4".into()],
        });

        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        match decoded {
            FederationMessage::Hello(p) => {
                assert_eq!(p.cluster_id, "cluster-1");
                assert_eq!(p.cluster_name, "East");
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn test_frame_roundtrip() {
        let msg = FederationMessage::HeartbeatAck;
        let frame = encode_frame(&msg).unwrap();
        let (decoded, consumed) = decode_frame(&frame).unwrap();
        assert_eq!(consumed, frame.len());
        assert!(matches!(decoded, FederationMessage::HeartbeatAck));
    }

    #[test]
    fn test_remote_exec_message() {
        let msg = FederationMessage::RemoteExecRequest(RemoteExecRequest {
            request_id: "req-1".into(),
            target: "code-analyzer".into(),
            payload: serde_json::json!({"project": "my-app"}),
            timeout_secs: 30,
            stream: false,
        });

        let encoded = encode_message(&msg).unwrap();
        let decoded = decode_message(&encoded).unwrap();
        assert!(matches!(decoded, FederationMessage::RemoteExecRequest(_)));
    }

    #[test]
    fn test_error_message() {
        let err = ProtocolError::new(-1, "protocol version mismatch");
        let msg = FederationMessage::Error(err);
        let encoded = encode_message(&msg).unwrap();
        assert!(encoded.contains("protocol version mismatch"));
    }
}
