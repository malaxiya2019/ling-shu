//! AgentSwarm — 群体内部通信协议
//!
//! 提供 Swarm 内部 Agent 之间的消息传递：
//! - 点对点消息（Direct）
//! - 广播消息（Broadcast）
//! - 组播消息（Multicast）
//! - 事件驱动通信（Event-based）

use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

// ── 消息类型 ────────────────────────────────────────

/// Swarm 内部消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmMessage {
    /// 消息 ID
    pub id: LsId,
    /// 消息类型
    pub msg_type: SwarmMsgType,
    /// 发送者 Agent ID
    pub sender_id: LsId,
    /// 发送者名称
    pub sender_name: String,
    /// 目标 Agent ID（None = 广播）
    pub target_id: Option<LsId>,
    /// 消息负载
    pub payload: serde_json::Value,
    /// 时间戳
    pub timestamp: i64,
    /// 是否需确认
    pub requires_ack: bool,
    /// TTL（跳数）
    pub ttl: u8,
}

/// Swarm 消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwarmMsgType {
    /// 任务分配
    TaskAssignment,
    /// 任务结果
    TaskResult,
    /// 竞标
    Bid,
    /// 投票
    Vote,
    /// 协商
    Negotiation,
    /// 通知
    Notification,
    /// 心跳
    Heartbeat,
    /// 状态同步
    StateSync,
    /// 能力声明
    CapabilityAnnounce,
    /// 错误
    Error,
    /// 查询
    Query,
    /// 响应
    Response,
    /// 紧急（高优先级）
    Emergency,
}

impl SwarmMsgType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SwarmMsgType::TaskAssignment => "task_assignment",
            SwarmMsgType::TaskResult => "task_result",
            SwarmMsgType::Bid => "bid",
            SwarmMsgType::Vote => "vote",
            SwarmMsgType::Negotiation => "negotiation",
            SwarmMsgType::Notification => "notification",
            SwarmMsgType::Heartbeat => "heartbeat",
            SwarmMsgType::StateSync => "state_sync",
            SwarmMsgType::CapabilityAnnounce => "capability_announce",
            SwarmMsgType::Error => "error",
            SwarmMsgType::Query => "query",
            SwarmMsgType::Response => "response",
            SwarmMsgType::Emergency => "emergency",
        }
    }
}

impl SwarmMessage {
    pub fn new(
        msg_type: SwarmMsgType,
        sender_id: LsId,
        sender_name: String,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: LsId::new(),
            msg_type,
            sender_id,
            sender_name,
            target_id: None,
            payload,
            timestamp: chrono::Utc::now().timestamp(),
            requires_ack: false,
            ttl: 3,
        }
    }

    pub fn to(mut self, target_id: LsId) -> Self {
        self.target_id = Some(target_id);
        self
    }

    pub fn with_ack(mut self) -> Self {
        self.requires_ack = true;
        self
    }

    pub fn broadcast(msg_type: SwarmMsgType, sender_id: LsId, sender_name: String, payload: serde_json::Value) -> Self {
        Self::new(msg_type, sender_id, sender_name, payload)
    }
}

// ── 消息收据 ────────────────────────────────────────

/// 消息投递确认
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReceipt {
    /// 原始消息 ID
    pub message_id: LsId,
    /// 接收者 Agent ID
    pub receiver_id: LsId,
    /// 接收时间
    pub received_at: i64,
    /// 投递状态
    pub status: DeliveryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    Delivered,
    Read,
    Processing,
    Failed(String),
}

// ── 通信通道 ────────────────────────────────────────

/// Swarm 通信通道
pub struct SwarmChannel {
    /// 广播通道
    broadcast_tx: broadcast::Sender<SwarmMessage>,
    /// 点对点通道（Agent ID → Receiver）
    direct_channels: RwLock<Vec<(LsId, mpsc::UnboundedSender<SwarmMessage>)>>,
    /// 消息历史
    message_history: RwLock<Vec<SwarmMessage>>,
    /// 最大历史记录数
    max_history: usize,
}

impl SwarmChannel {
    pub fn new(max_history: usize) -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            broadcast_tx: tx,
            direct_channels: RwLock::new(Vec::new()),
            message_history: RwLock::new(Vec::new()),
            max_history,
        }
    }

    /// 订阅广播
    pub fn subscribe(&self) -> broadcast::Receiver<SwarmMessage> {
        self.broadcast_tx.subscribe()
    }

    /// 注册 Agent 的直接通道
    pub async fn register_agent(&self, agent_id: LsId) -> mpsc::UnboundedReceiver<SwarmMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.direct_channels.write().await.push((agent_id, tx));
        rx
    }

    /// 注销 Agent
    pub async fn unregister_agent(&self, agent_id: &LsId) {
        let mut channels = self.direct_channels.write().await;
        channels.retain(|(id, _)| id != agent_id);
    }

    /// 发送消息（自动判断广播或点对点）
    pub async fn send(&self, message: SwarmMessage) -> Result<(), String> {
        let is_broadcast = message.target_id.is_none();

        if is_broadcast {
            self.broadcast(message.clone())?;
        } else {
            self.send_direct(message.clone()).await?;
        }

        // 记录历史
        let mut history = self.message_history.write().await;
        history.push(message);
        if history.len() > self.max_history {
            history.remove(0);
        }

        Ok(())
    }

    /// 广播消息
    fn broadcast(&self, message: SwarmMessage) -> Result<(), String> {
        self.broadcast_tx
            .send(message)
            .map_err(|e| format!("broadcast failed: {}", e))?;
        Ok(())
    }

    /// 点对点发送
    async fn send_direct(&self, message: SwarmMessage) -> Result<(), String> {
        let target_id = message
            .target_id
            .as_ref()
            .ok_or_else(|| "No target ID for direct message".to_string())?;

        let channels = self.direct_channels.read().await;
        let sender = channels
            .iter()
            .find(|(id, _)| id == target_id)
            .map(|(_, tx)| tx)
            .ok_or_else(|| format!("Agent {} not registered", target_id))?;

        sender
            .send(message)
            .map_err(|_| "Receiver channel closed".to_string())
    }

    /// 获取消息历史
    pub async fn get_history(&self) -> Vec<SwarmMessage> {
        self.message_history.read().await.clone()
    }

    /// 按类型过滤历史消息
    pub async fn get_messages_by_type(&self, msg_type: SwarmMsgType) -> Vec<SwarmMessage> {
        let history = self.message_history.read().await;
        history.iter().filter(|m| m.msg_type == msg_type).cloned().collect()
    }
}

// ── 通信管理器 ──────────────────────────────────────

/// Swarm 通信管理器 - Agent 端的通信接口
pub struct SwarmCommunicator {
    /// Agent ID
    agent_id: LsId,
    /// Agent 名称
    agent_name: String,
    /// 对通信通道的引用
    channel: Arc<SwarmChannel>,
    /// 广播接收器
    broadcast_rx: tokio::sync::broadcast::Receiver<SwarmMessage>,
    /// 直接消息接收器
    direct_rx: Option<mpsc::UnboundedReceiver<SwarmMessage>>,
}

impl SwarmCommunicator {
    pub fn new(agent_id: LsId, agent_name: String, channel: Arc<SwarmChannel>) -> Self {
        let broadcast_rx = channel.subscribe();
        Self {
            agent_id,
            agent_name,
            channel,
            broadcast_rx,
            direct_rx: None,
        }
    }

    /// 初始化并注册到通道
    pub async fn init(&mut self) {
        let rx = self.channel.register_agent(self.agent_id.clone()).await;
        self.direct_rx = Some(rx);
    }

    /// 发送消息
    pub async fn send(&self, msg_type: SwarmMsgType, payload: serde_json::Value) -> Result<(), String> {
        let msg = SwarmMessage::new(msg_type, self.agent_id.clone(), self.agent_name.clone(), payload);
        self.channel.send(msg).await
    }

    /// 发送到指定 Agent
    pub async fn send_to(
        &self,
        target_id: LsId,
        msg_type: SwarmMsgType,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let msg = SwarmMessage::new(msg_type, self.agent_id.clone(), self.agent_name.clone(), payload)
            .to(target_id);
        self.channel.send(msg).await
    }

    /// 广播消息
    pub async fn broadcast(&self, msg_type: SwarmMsgType, payload: serde_json::Value) -> Result<(), String> {
        let msg = SwarmMessage::broadcast(msg_type, self.agent_id.clone(), self.agent_name.clone(), payload);
        self.channel.send(msg).await
    }

    /// 接收广播消息（非阻塞）
    pub fn try_recv_broadcast(&mut self) -> Option<SwarmMessage> {
        match self.broadcast_rx.try_recv() {
            Ok(msg) => Some(msg),
            Err(_) => None,
        }
    }

    /// 接收直接消息（非阻塞）
    pub fn try_recv_direct(&mut self) -> Option<SwarmMessage> {
        if let Some(ref mut rx) = self.direct_rx {
            match rx.try_recv() {
                Ok(msg) => Some(msg),
                Err(_) => None,
            }
        } else {
            None
        }
    }

    /// Agent 下线
    pub async fn shutdown(&self) {
        self.channel.unregister_agent(&self.agent_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_swarm_channel_broadcast() {
        let channel = Arc::new(SwarmChannel::new(100));
        let mut rx1 = channel.subscribe();
        let mut rx2 = channel.subscribe();

        let msg = SwarmMessage::new(
            SwarmMsgType::Notification,
            LsId::new(),
            "sender".into(),
            serde_json::json!({"hello": "world"}),
        );

        channel.send(msg.clone()).await.unwrap();

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();
        assert_eq!(received1.payload, serde_json::json!({"hello": "world"}));
        assert_eq!(received2.payload, serde_json::json!({"hello": "world"}));
    }

    #[tokio::test]
    async fn test_swarm_channel_direct() {
        let channel = Arc::new(SwarmChannel::new(100));
        let agent_id = LsId::new();
        let mut rx = channel.register_agent(agent_id.clone()).await;

        let msg = SwarmMessage::new(
            SwarmMsgType::TaskAssignment,
            LsId::new(),
            "coordinator".into(),
            serde_json::json!({"task": "do_something"}),
        )
        .to(agent_id);

        channel.send(msg).await.unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received.msg_type, SwarmMsgType::TaskAssignment);
    }

    #[tokio::test]
    async fn test_communicator() {
        let channel = Arc::new(SwarmChannel::new(100));
        let agent_id = LsId::new();
        let mut comm = SwarmCommunicator::new(agent_id.clone(), "test-agent".into(), channel.clone());
        comm.init().await;

        // Subscribe before sending
        let mut rx = channel.subscribe();

        // Send broadcast
        comm.broadcast(SwarmMsgType::Heartbeat, serde_json::json!({"status": "alive"})).await.unwrap();

        // Should be able to receive
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.msg_type, SwarmMsgType::Heartbeat);
    }

    #[tokio::test]
    async fn test_unregister() {
        let channel = Arc::new(SwarmChannel::new(100));
        let agent_id = LsId::new();
        let mut comm = SwarmCommunicator::new(agent_id.clone(), "test-agent".into(), channel.clone());
        comm.init().await;
        comm.shutdown().await;

        // After shutdown, should not be able to send direct messages
        let msg = SwarmMessage::new(
            SwarmMsgType::Notification,
            LsId::new(),
            "sender".into(),
            serde_json::json!("ping"),
        )
        .to(agent_id);

        let result = channel.send(msg).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_swarm_msg_type_display() {
        assert_eq!(SwarmMsgType::TaskAssignment.as_str(), "task_assignment");
        assert_eq!(SwarmMsgType::Emergency.as_str(), "emergency");
    }
}
