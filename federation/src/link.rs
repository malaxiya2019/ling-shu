//! LSFed — 跨集群连接管理.
//!
//! 管理联邦节点间的 TCP 连接，处理心跳和重连。

use crate::protocol::{
    self, encode_frame, FederationMessage, HelloAckPayload, HelloPayload, ProtocolError,
};
use crate::types::{FederationConfig, FederationLink, FederationNode, FederationNodeStatus};
use lingshu_core::{LsId, LsResult};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// 链路事件.
#[derive(Debug, Clone)]
pub enum LinkEvent {
    /// 节点上线.
    NodeOnline(FederationNode),
    /// 节点离线.
    NodeOffline(FederationNode),
    /// 链路健康变化.
    HealthChanged(FederationLink),
    /// 收到消息.
    MessageReceived(FederationNode, FederationMessage),
}

/// 连接管理器 — 管理所有出站/入站联邦连接.
pub struct LinkManager {
    cancel_token: CancellationToken,
    local_id: LsId,
    local_name: String,
    config: FederationConfig,
    nodes: Arc<RwLock<HashMap<String, FederationNode>>>,
    connections: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<FederationMessage>>>>,
    links: Arc<RwLock<HashMap<String, FederationLink>>>,
    event_tx: mpsc::UnboundedSender<LinkEvent>,
    #[allow(dead_code)]
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<LinkEvent>>>>,
    capabilities: Arc<RwLock<HashMap<String, Vec<crate::types::Capability>>>>,
}

impl LinkManager {
    /// 创建连接管理器.
    pub fn new(local_id: LsId, local_name: &str, config: FederationConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            local_id,
            cancel_token: CancellationToken::new(),
            local_name: local_name.to_string(),
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            links: Arc::new(RwLock::new(HashMap::new())),
            event_tx: tx,
            event_rx: Arc::new(RwLock::new(Some(rx))),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取链路事件接收器.
    pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<LinkEvent>> {
        self.event_rx.try_write().ok()?.take()
    }

    /// 启动联邦服务器监听（TCP 端口绑定 + 连接接受循环）.
    pub async fn start_server(&self) -> LsResult<()> {
        let addr = self.config.listen_addr;
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("bind {addr} failed: {e}")))?;
        info!(addr = %addr, "federation server listening");

        let cancel = self.cancel_token.clone();
        let nodes = self.nodes.clone();
        let connections = self.connections.clone();
        let links = self.links.clone();
        let event_tx = self.event_tx.clone();
        let capabilities = self.capabilities.clone();
        let local_id = self.local_id;
        let local_name = self.local_name.clone();
        let listen_addr = self.config.listen_addr;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("federation server shutting down");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((stream, peer_addr)) => {
                                debug!(peer = %peer_addr, "incoming federation connection");
                                let nodes = nodes.clone();
                                let connections = connections.clone();
                                let links = links.clone();
                                let event_tx = event_tx.clone();
                                let capabilities = capabilities.clone();
                                let name = local_name.clone();

                                tokio::spawn(async move {
                                    if let Err(e) = handle_inbound(
                                        stream, peer_addr, &local_id, &name, &listen_addr,
                                        &nodes, &connections, &links, &event_tx, &capabilities,
                                    ).await {
                                        warn!("inbound handler failed: {e}");
                                    }
                                });
                            }
                            Err(e) => {
                                error!("accept failed: {e}");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// 停止联邦服务器监听，释放端口.
    pub async fn stop(&self) {
        info!("federation link manager stopping");
        self.cancel_token.cancel();
    }

    /// 连接到指定联邦节点.
    pub async fn connect(&self, node: &FederationNode) -> LsResult<()> {
        let target_id = node.cluster_id.to_string();
        let addr = match node.addrs.first() {
            Some(a) => *a,
            None => return Err(lingshu_core::LsError::Internal("no address".into())),
        };

        let stream = match TcpStream::connect(addr).await {
            Ok(s) => s,
            Err(e) => {
                warn!(peer = %addr, error = %e, "connect failed");
                return Err(lingshu_core::LsError::Internal(format!(
                    "connect failed: {e}"
                )));
            }
        };

        info!(peer = %addr, cluster = %target_id, "outbound connection established");

        let nodes = self.nodes.clone();
        let connections = self.connections.clone();
        let links = self.links.clone();
        let event_tx = self.event_tx.clone();
        let capabilities = self.capabilities.clone();
        let local_id = self.local_id;
        let local_name = self.local_name.clone();
        let listen_addr = self.config.listen_addr;

        handle_outbound(
            stream,
            &target_id,
            &local_id,
            &local_name,
            &listen_addr,
            &nodes,
            &connections,
            &links,
            &event_tx,
            &capabilities,
        )
        .await
    }

    /// 连接到所有已注册节点.
    pub async fn connect_all(&self) {
        let nodes = self.nodes.read().await.clone();
        for node in nodes.values() {
            if node.cluster_id == self.local_id {
                continue;
            }
            if node.status != FederationNodeStatus::Online {
                if let Err(e) = self.connect(node).await {
                    warn!("connect_all failed for {}: {e}", node.name);
                }
            }
        }
    }

    /// 向指定集群发送联邦消息.
    pub fn send(&self, cluster_id: &str, msg: FederationMessage) -> bool {
        if let Some(tx) = self
            .connections
            .try_read()
            .ok()
            .and_then(|c| c.get(cluster_id).cloned())
        {
            tx.send(msg).is_ok()
        } else {
            false
        }
    }

    /// 广播消息到所有已连接节点.
    pub fn broadcast(&self, msg: FederationMessage) {
        let conns = self.connections.try_read();
        if let Ok(conns) = conns {
            for (id, tx) in conns.iter() {
                if let Err(e) = tx.send(msg.clone()) {
                    debug!(peer = %id, error = %e, "broadcast send failed");
                }
            }
        }
    }

    /// 注册一个联邦节点.
    pub async fn register_node(&self, node: FederationNode) {
        let id = node.cluster_id.to_string();
        let mut nodes = self.nodes.write().await;
        nodes.insert(id, node);
    }

    /// 获取所有在线节点列表.
    pub async fn online_nodes(&self) -> Vec<FederationNode> {
        let nodes = self.nodes.read().await;
        nodes
            .values()
            .filter(|n| n.status == FederationNodeStatus::Online)
            .cloned()
            .collect()
    }

    #[allow(dead_code)]
    /// 获取指定集群的能力列表.
    pub async fn node_capabilities(
        &self,
        cluster_id: &str,
    ) -> Option<Vec<crate::types::Capability>> {
        self.capabilities.read().await.get(cluster_id).cloned()
    }

    /// 获取所有链路信息.
    pub async fn all_links(&self) -> Vec<FederationLink> {
        self.links.read().await.values().cloned().collect()
    }
}

// ── 内部函数 ───────────────────────────────────────

type NodeMap = Arc<RwLock<HashMap<String, FederationNode>>>;
type ConnMap = Arc<RwLock<HashMap<String, mpsc::UnboundedSender<FederationMessage>>>>;
type LinkMap = Arc<RwLock<HashMap<String, FederationLink>>>;
type CapMap = Arc<RwLock<HashMap<String, Vec<crate::types::Capability>>>>;
type EvtTx = mpsc::UnboundedSender<LinkEvent>;

#[allow(clippy::too_many_arguments)]
async fn handle_inbound(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    local_id: &LsId,
    local_name: &str,
    _listen_addr: &SocketAddr,
    nodes: &NodeMap,
    connections: &ConnMap,
    _links: &LinkMap,
    _event_tx: &EvtTx,
    _capabilities: &CapMap,
) -> Result<(), String> {
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("read hello failed: {e}"))?;
    let (msg, _consumed) = protocol::decode_frame(&buf[..n])?;

    let hello = match msg {
        FederationMessage::Hello(h) => h,
        other => {
            let err = ProtocolError::new(100, "expected Hello");
            let _ =
                encode_frame(&FederationMessage::Error(err)).map(|frame| stream.try_write(&frame));
            return Err(format!("expected Hello, got {other:?}"));
        }
    };

    let compatible = hello.protocol_version == protocol::FEDERATION_PROTOCOL_VERSION;
    let ack = FederationMessage::HelloAck(HelloAckPayload {
        peer_cluster_id: local_id.to_string(),
        peer_cluster_name: local_name.to_string(),
        protocol_version: protocol::FEDERATION_PROTOCOL_VERSION.into(),
        compatible,
        incompatible_reason: if compatible {
            None
        } else {
            Some(format!(
                "version mismatch: local={}, remote={}",
                protocol::FEDERATION_PROTOCOL_VERSION,
                hello.protocol_version
            ))
        },
    });

    let ack_frame = encode_frame(&ack)?;
    stream
        .write_all(&ack_frame)
        .await
        .map_err(|e| format!("write ack failed: {e}"))?;

    if !compatible {
        return Err(format!(
            "incompatible version: remote={}",
            hello.protocol_version
        ));
    }

    let cluster_id = hello.cluster_id.clone();
    let remote_node = FederationNode::new(LsId::new(), &hello.cluster_name, vec![peer_addr]);
    nodes.write().await.insert(cluster_id.clone(), remote_node);

    let (reader, writer) = stream.into_split();
    let (tx, mut rx): (
        mpsc::UnboundedSender<FederationMessage>,
        mpsc::UnboundedReceiver<FederationMessage>,
    ) = mpsc::unbounded_channel();
    connections.write().await.insert(cluster_id.clone(), tx);

    let cid = cluster_id.clone();
    tokio::spawn(async move {
        let mut writer = writer;
        while let Some(msg) = rx.recv().await {
            match encode_frame(&msg) {
                Ok(frame) => {
                    if writer.write_all(&frame).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("encode failed in write loop: {e}");
                    break;
                }
            }
        }
        debug!(cluster = %cid, "inbound write loop ended");
    });

    read_loop(reader, &cluster_id, connections).await;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn handle_outbound(
    mut stream: TcpStream,
    cluster_id: &str,
    local_id: &LsId,
    local_name: &str,
    listen_addr: &SocketAddr,
    _nodes: &NodeMap,
    connections: &ConnMap,
    _links: &LinkMap,
    _event_tx: &EvtTx,
    _capabilities: &CapMap,
) -> LsResult<()> {
    let hello = FederationMessage::Hello(HelloPayload {
        cluster_id: local_id.to_string(),
        cluster_name: local_name.to_string(),
        protocol_version: protocol::FEDERATION_PROTOCOL_VERSION.into(),
        listen_addrs: vec![listen_addr.to_string()],
        capabilities: Vec::new(),
    });

    let frame = encode_frame(&hello)
        .map_err(|e| lingshu_core::LsError::Internal(format!("encode hello failed: {e}")))?;
    stream
        .write_all(&frame)
        .await
        .map_err(|e| lingshu_core::LsError::Internal(format!("write hello failed: {e}")))?;

    let cid = cluster_id.to_string();
    let (reader, writer) = stream.into_split();
    let (tx, mut rx): (
        mpsc::UnboundedSender<FederationMessage>,
        mpsc::UnboundedReceiver<FederationMessage>,
    ) = mpsc::unbounded_channel();
    connections.write().await.insert(cid.clone(), tx);

    let cid_w = cid.clone();
    tokio::spawn(async move {
        let mut writer = writer;
        while let Some(msg) = rx.recv().await {
            match encode_frame(&msg) {
                Ok(frame) => {
                    if writer.write_all(&frame).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("encode failed in write loop: {e}");
                    break;
                }
            }
        }
        debug!(cluster = %cid_w, "outbound write loop ended");
    });

    read_loop(reader, &cid, connections).await;
    Ok(())
}

async fn read_loop(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    cluster_id: &str,
    connections: &ConnMap,
) {
    let cid = cluster_id.to_string();
    let mut buf = vec![0u8; 8192];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => {
                debug!(cluster = %cid, "connection closed");
                break;
            }
            Ok(_n) => {}
            Err(e) => {
                warn!(cluster = %cid, "read error: {e}");
                break;
            }
        }
    }
    connections.write().await.remove(&cid);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_link_manager_create() {
        let config = FederationConfig::default();
        let mgr = LinkManager::new(LsId::new(), "test-cluster", config);
        let nodes = mgr.online_nodes().await;
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_send_no_connection() {
        let config = FederationConfig::default();
        let mgr = LinkManager::new(LsId::new(), "test-cluster", config);
        let sent = mgr.send("nonexistent", FederationMessage::HeartbeatAck);
        assert!(!sent, "should fail to send to nonexistent cluster");
    }
}
