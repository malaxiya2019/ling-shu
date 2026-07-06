//! InterAgentComm — 智能体间通信.
//!
//! 支持点对点、广播、请求-响应三种消息模式。
//! 基于 EventBus 实现可靠消息投递。

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn};

/// 消息投递状态.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Read,
    Failed(String),
}

/// 智能体消息.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub source: LsId,
    pub destination: Option<LsId>, // None = broadcast
    pub message_type: String,      // "request", "response", "event", "command"
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>, // 用于请求-响应配对
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub ttl_seconds: Option<u64>,
}

/// 消息信封 (内部封装).
#[derive(Debug)]
pub struct MessageEnvelope {
    pub message: AgentMessage,
    pub sender: mpsc::Sender<AgentMessage>,
    pub ctx: LsContext,
}

/// InterAgentComm — 智能体间消息总线.
pub struct InterAgentComm {
    /// inbox: agent_id -> 消息接收通道
    inboxes: RwLock<HashMap<LsId, mpsc::Sender<AgentMessage>>>,
    /// 挂起的请求 (correlation_id -> response sender)
    pending_requests: RwLock<HashMap<String, oneshot::Sender<AgentMessage>>>,
    message_counter: AtomicU64,
}

impl InterAgentComm {
    pub fn new() -> Self {
        Self {
            inboxes: RwLock::new(HashMap::new()),
            pending_requests: RwLock::new(HashMap::new()),
            message_counter: AtomicU64::new(0),
        }
    }

    /// 注册智能体, 为其创建 inbox 通道.
    pub async fn register_agent(&self, agent_id: LsId) -> mpsc::Receiver<AgentMessage> {
        let (tx, rx) = mpsc::channel(256);
        self.inboxes.write().await.insert(agent_id, tx);
        rx
    }

    /// 注销智能体.
    pub async fn unregister_agent(&self, agent_id: &LsId) {
        self.inboxes.write().await.remove(agent_id);
    }

    /// 发送消息到指定智能体.
    pub async fn send(
        &self,
        destination: &LsId,
        message: AgentMessage,
    ) -> LsResult<DeliveryStatus> {
        let inboxes = self.inboxes.read().await;
        let tx = inboxes
            .get(destination)
            .ok_or_else(|| LsError::NotFound(format!("agent inbox {destination}")))?;

        tx.send(message)
            .await
            .map(|_| DeliveryStatus::Delivered)
            .map_err(|_| LsError::Internal("agent inbox channel closed".into()))
    }

    /// 广播消息给所有注册的智能体.
    pub async fn broadcast(&self, message: AgentMessage) -> Vec<LsResult<DeliveryStatus>> {
        let inboxes = self.inboxes.read().await;
        let mut results = Vec::with_capacity(inboxes.len());

        for (agent_id, tx) in inboxes.iter() {
            let mut msg = message.clone();
            msg.destination = Some(*agent_id);
            match tx.send(msg).await {
                Ok(()) => results.push(Ok(DeliveryStatus::Delivered)),
                Err(e) => {
                    warn!(agent_id = %agent_id, "broadcast send failed");
                    results.push(Err(LsError::Internal(format!("broadcast failed: {e}"))));
                }
            }
        }
        results
    }

    /// 发送请求并等待响应 (请求-响应模式).
    pub async fn request(
        &self,
        destination: &LsId,
        payload: serde_json::Value,
        source: LsId,
        timeout: std::time::Duration,
    ) -> LsResult<AgentMessage> {
        let correlation_id = uuid::Uuid::now_v7().to_string();
        let (resp_tx, resp_rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(correlation_id.clone(), resp_tx);
        }

        let msg = AgentMessage {
            id: self.next_message_id(),
            source,
            destination: Some(*destination),
            message_type: "request".into(),
            payload,
            correlation_id: Some(correlation_id.clone()),
            timestamp: chrono::Utc::now(),
            ttl_seconds: Some(timeout.as_secs()),
        };

        self.send(destination, msg).await?;

        tokio::time::timeout(timeout, resp_rx)
            .await
            .map_err(|_| LsError::Timeout("agent request timed out".into()))?
            .map_err(|_| LsError::Internal("response channel closed".into()))
    }

    /// 发送响应 (回复请求).
    pub async fn respond(
        &self,
        correlation_id: &str,
        payload: serde_json::Value,
        source: LsId,
        destination: LsId,
    ) -> LsResult<()> {
        // 找到等待响应的 sender 并直接投递
        let mut pending = self.pending_requests.write().await;
        if let Some(tx) = pending.remove(correlation_id) {
            let msg = AgentMessage {
                id: self.next_message_id(),
                source,
                destination: Some(destination),
                message_type: "response".into(),
                payload,
                correlation_id: Some(correlation_id.to_string()),
                timestamp: chrono::Utc::now(),
                ttl_seconds: None,
            };
            tx.send(msg)
                .map_err(|_| LsError::Internal("response receiver dropped".into()))?;
            info!(correlation_id = %correlation_id, "response delivered");
            Ok(())
        } else {
            Err(LsError::NotFound(format!(
                "pending request {correlation_id}"
            )))
        }
    }

    /// 获取收件箱中待处理的消息数量.
    pub async fn pending_count(&self, _agent_id: &LsId) -> usize {
        // 这个功能通过 mpsc::Receiver 的 len() 方法不可用
        // 此处简化处理
        0
    }

    fn next_message_id(&self) -> String {
        let n = self.message_counter.fetch_add(1, Ordering::Relaxed);
        format!("msg-{}", n)
    }
}

impl Default for InterAgentComm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_register_and_send() {
        let comm = InterAgentComm::new();
        let id = LsId::new();
        let mut rx = comm.register_agent(id).await;

        let msg = AgentMessage {
            id: "test-1".into(),
            source: LsId::new(),
            destination: Some(id),
            message_type: "event".into(),
            payload: json!({"hello": "world"}),
            correlation_id: None,
            timestamp: chrono::Utc::now(),
            ttl_seconds: None,
        };

        let status = comm.send(&id, msg.clone()).await.unwrap();
        assert_eq!(status, DeliveryStatus::Delivered);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.payload, msg.payload);
    }

    #[tokio::test]
    async fn test_send_unregistered() {
        let comm = InterAgentComm::new();
        let id = LsId::new();
        let msg = AgentMessage {
            id: "test".into(),
            source: LsId::new(),
            destination: Some(id),
            message_type: "event".into(),
            payload: json!({}),
            correlation_id: None,
            timestamp: chrono::Utc::now(),
            ttl_seconds: None,
        };

        assert!(comm.send(&id, msg).await.is_err());
    }

    #[tokio::test]
    async fn test_broadcast() {
        let comm = InterAgentComm::new();
        let id1 = LsId::new();
        let id2 = LsId::new();
        let _rx1 = comm.register_agent(id1).await;
        let mut rx2 = comm.register_agent(id2).await;

        let msg = AgentMessage {
            id: "broadcast-1".into(),
            source: LsId::new(),
            destination: None,
            message_type: "event".into(),
            payload: serde_json::json!("hello all"),
            correlation_id: None,
            timestamp: chrono::Utc::now(),
            ttl_seconds: None,
        };

        let results = comm.broadcast(msg).await;
        assert_eq!(results.len(), 2);
        for r in results {
            assert!(r.is_ok());
        }

        let _received = rx2.recv().await.unwrap();
    }

    #[tokio::test]
    async fn test_request_response() {
        let comm = Arc::new(InterAgentComm::new());
        let server_id = LsId::new();
        let client_id = LsId::new();
        let mut server_rx = comm.register_agent(server_id).await;

        let comm_clone = comm.clone();
        let server_id_clone = server_id;
        tokio::spawn(async move {
            // 服务器接收请求
            if let Some(req) = server_rx.recv().await {
                let corr = req.correlation_id.unwrap();
                // 发送响应
                comm_clone
                    .respond(&corr, json!({"result": "ok"}), server_id_clone, req.source)
                    .await
                    .unwrap();
            }
        });

        let resp = comm
            .request(
                &server_id,
                json!({"query": "ping"}),
                client_id,
                std::time::Duration::from_secs(5),
            )
            .await
            .unwrap();

        assert_eq!(resp.message_type, "response");
        assert_eq!(resp.payload, json!({"result": "ok"}));
    }

    #[tokio::test]
    async fn test_unregister() {
        let comm = InterAgentComm::new();
        let id = LsId::new();
        comm.register_agent(id).await;
        comm.unregister_agent(&id).await;

        let msg = AgentMessage {
            id: "test".into(),
            source: LsId::new(),
            destination: Some(id),
            message_type: "event".into(),
            payload: json!({}),
            correlation_id: None,
            timestamp: chrono::Utc::now(),
            ttl_seconds: None,
        };
        assert!(comm.send(&id, msg).await.is_err());
    }
}
