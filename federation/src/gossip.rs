//! 八卦协议 — 去中心化节点发现和状态传播.
//!
//! 基于 SWIM 风格八卦协议 (Scalable Weakly-consistent Infection-style
//! Process Group Membership Protocol)，用于联邦集群节点发现和元数据传播。
//!
//! ## 流程
//! 1. 每个节点定期选择一个随机节点发送 Ping
//! 2. 目标节点回复 Ack，包含其已知的节点状态
//! 3. 若 Ping 超时，选择替代节点进行间接探测
//! 4. 定期全量同步合并节点列表
//!
//! ## 参考
//! - [SWIM Protocol](https://www.cs.cornell.edu/projects/Quicksilver/public_pdfs/SWIM.pdf)

use crate::types::FederationNode;
use lingshu_core::LsResult;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// 八卦状态.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipState {
    /// 节点存活
    Alive,
    /// 节点疑似故障（正在确认）
    Suspect,
    /// 节点已下线
    Dead,
}

/// 八卦成员条目.
#[derive(Debug, Clone)]
pub struct GossipMember {
    /// 节点 ID
    pub node_id: String,
    /// 节点地址
    pub addr: String,
    /// 当前状态
    pub state: GossipState,
    /// 协议版本号 (用于冲突解决)
    pub incarnation: u64,
    /// 最后更新时间
    pub last_updated: Instant,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl GossipMember {
    fn new(node_id: &str, addr: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            addr: addr.to_string(),
            state: GossipState::Alive,
            incarnation: 0,
            last_updated: Instant::now(),
            metadata: HashMap::new(),
        }
    }

    fn merge(&mut self, other: &GossipMember) -> bool {
        if other.incarnation > self.incarnation
            || (other.incarnation == self.incarnation && other.state != self.state)
        {
            self.state = other.state.clone();
            self.incarnation = other.incarnation;
            self.last_updated = Instant::now();
            self.metadata = other.metadata.clone();
            return true;
        }
        false
    }
}

/// 八卦协议事件.
#[derive(Debug, Clone)]
pub enum GossipEvent {
    /// 节点加入集群
    MemberJoined(GossipMember),
    /// 节点状态变更
    MemberUpdated(GossipMember),
    /// 节点离开集群
    MemberLeft(GossipMember),
    /// 节点疑似故障
    MemberSuspected(GossipMember),
}

/// 八卦协议引擎.
pub struct GossipEngine {
    /// 本地节点 ID
    local_id: String,
    /// 本地节点地址
    local_addr: String,
    /// 成员列表
    members: Arc<RwLock<HashMap<String, GossipMember>>>,
    /// 事件发送器
    event_tx: mpsc::UnboundedSender<GossipEvent>,
    /// 事件接收器
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<GossipEvent>>>>,
    /// 配置
    config: GossipConfig,
}

/// 八卦协议配置.
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// Ping 间隔 (秒)
    pub ping_interval_secs: u64,
    /// Ping 超时 (秒)
    pub ping_timeout_secs: u64,
    /// 怀疑超时 (秒) — 超时后标记 Dead
    pub suspect_timeout_secs: u64,
    /// 全量同步间隔 (秒)
    pub full_sync_interval_secs: u64,
    /// 清理下线节点间隔 (秒)
    pub cleanup_interval_secs: u64,
    /// 最大下线节点保留时间 (秒)
    pub dead_node_ttl_secs: u64,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            ping_interval_secs: 1,
            ping_timeout_secs: 3,
            suspect_timeout_secs: 10,
            full_sync_interval_secs: 30,
            cleanup_interval_secs: 60,
            dead_node_ttl_secs: 120,
        }
    }
}

impl GossipEngine {
    /// 创建八卦引擎.
    pub fn new(local_id: &str, local_addr: &str) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut members = HashMap::new();
        members.insert(
            local_id.to_string(),
            GossipMember::new(local_id, local_addr),
        );

        Self {
            local_id: local_id.to_string(),
            local_addr: local_addr.to_string(),
            members: Arc::new(RwLock::new(members)),
            event_tx: tx,
            event_rx: Arc::new(RwLock::new(Some(rx))),
            config: GossipConfig::default(),
        }
    }

    /// 使用自定义配置.
    pub fn with_config(mut self, config: GossipConfig) -> Self {
        self.config = config;
        self
    }

    /// 获取事件接收器.
    pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<GossipEvent>> {
        self.event_rx.try_write().ok()?.take()
    }

    /// 注册一个已知节点.
    pub async fn add_seed(&self, node_id: &str, addr: &str) {
        let mut members = self.members.write().await;
        if !members.contains_key(node_id) {
            let member = GossipMember::new(node_id, addr);
            members.insert(node_id.to_string(), member.clone());
            let _ = self.event_tx.send(GossipEvent::MemberJoined(member));
            debug!(node_id, addr, "seed node added to gossip pool");
        }
    }

    /// 处理接收到的八卦消息.
    pub async fn handle_gossip_message(&self, from: &str, remote_members: Vec<GossipMember>) {
        let mut members = self.members.write().await;
        for remote in remote_members {
            if remote.node_id == self.local_id {
                continue; // 忽略自身的旧状态
            }
            let changed = match members.get(&remote.node_id) {
                Some(local) => {
                    let mut local_clone = local.clone();
                    let changed = local_clone.merge(&remote);
                    if changed {
                        members.insert(remote.node_id.clone(), local_clone.clone());
                    }
                    changed
                }
                None => {
                    members.insert(remote.node_id.clone(), remote.clone());
                    true
                }
            };
            if changed {
                let event = match &remote.state {
                    GossipState::Alive => GossipEvent::MemberJoined(remote.clone()),
                    GossipState::Suspect => GossipEvent::MemberSuspected(remote.clone()),
                    GossipState::Dead => GossipEvent::MemberLeft(remote.clone()),
                };
                let _ = self.event_tx.send(event);
            }
        }
    }

    /// 获取当前所有成员的快照.
    pub async fn get_members(&self) -> Vec<GossipMember> {
        self.members.read().await.values().cloned().collect()
    }

    /// 获取活跃成员列表.
    pub async fn get_alive_members(&self) -> Vec<GossipMember> {
        self.members
            .read()
            .await
            .values()
            .filter(|m| m.state == GossipState::Alive)
            .cloned()
            .collect()
    }

    /// 将自已的成员条目转为八卦消息.
    pub async fn get_local_member(&self) -> GossipMember {
        let members = self.members.read().await;
        members.get(&self.local_id).cloned().unwrap_or_else(|| {
            GossipMember::new(&self.local_id, &self.local_addr)
        })
    }

    /// 标记节点为 Suspect.
    pub async fn mark_suspect(&self, node_id: &str) {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(node_id) {
            if member.state != GossipState::Dead {
                member.state = GossipState::Suspect;
                member.incarnation += 1;
                let _ = self.event_tx.send(GossipEvent::MemberSuspected(member.clone()));
            }
        }
    }

    /// 标记节点为 Dead.
    pub async fn mark_dead(&self, node_id: &str) {
        let mut members = self.members.write().await;
        if let Some(member) = members.get_mut(node_id) {
            member.state = GossipState::Dead;
            member.incarnation += 1;
            let _ = self.event_tx.send(GossipEvent::MemberLeft(member.clone()));
        }
    }

    /// 启动八卦循环 (后台任务).
    pub async fn start_gossip_loop(
        self: &Arc<Self>,
        ping_fn: impl Fn(String) -> futures::future::BoxFuture<'static, LsResult<Vec<GossipMember>>> + Send + Sync + 'static,
    ) where
        Self: Sized,
    {
        let ping_interval = Duration::from_secs(self.config.ping_interval_secs);
        let ping_timeout = Duration::from_secs(self.config.ping_timeout_secs);
        let suspect_timeout = Duration::from_secs(self.config.suspect_timeout_secs);
        let cleanup_interval = Duration::from_secs(self.config.cleanup_interval_secs);
        let dead_ttl = Duration::from_secs(self.config.dead_node_ttl_secs);

        let self_arc = self.clone();
        tokio::spawn(async move {
            let mut ping_interval_timer = tokio::time::interval(ping_interval);
            let mut cleanup_timer = tokio::time::interval(cleanup_interval);

            loop {
                tokio::select! {
                    _ = ping_interval_timer.tick() => {
                        // 选择一个随机活跃节点发送 Ping
                        let alive = self_arc.get_alive_members().await;
                        let peers: Vec<&GossipMember> = alive.iter()
                            .filter(|m| m.node_id != self_arc.local_id)
                            .collect();

                        if peers.is_empty() {
                            continue;
                        }

                        let target = peers[fastrand::usize(..peers.len())];
                        debug!(target = %target.node_id, "gossip ping");

                        // 发送 Ping 并等待 Ack
                        match tokio::time::timeout(ping_timeout, ping_fn(target.node_id.clone())).await {
                            Ok(Ok(remote_members)) => {
                                self_arc.handle_gossip_message(&target.node_id, remote_members).await;
                            }
                            Ok(Err(e)) => {
                                warn!(target = %target.node_id, error = %e, "gossip ping failed");
                            }
                            Err(_) => {
                                warn!(target = %target.node_id, "gossip ping timeout");
                                self_arc.mark_suspect(&target.node_id).await;
                            }
                        }
                    }
                    _ = cleanup_timer.tick() => {
                        // 清理疑似的节点
                        let now = Instant::now();
                        let members = self_arc.members.read().await;
                        let suspect_ids: Vec<String> = members.values()
                            .filter(|m| m.state == GossipState::Suspect
                                && now.duration_since(m.last_updated) > suspect_timeout)
                            .map(|m| m.node_id.clone())
                            .collect();
                        drop(members);

                        for id in suspect_ids {
                            self_arc.mark_dead(&id).await;
                        }

                        // 清理过期的 Dead 节点
                        let dead_ids: Vec<String> = self_arc.members.read().await.values()
                            .filter(|m| m.state == GossipState::Dead
                                && now.duration_since(m.last_updated) > dead_ttl)
                            .map(|m| m.node_id.clone())
                            .collect();

                        if !dead_ids.is_empty() {
                            let mut members = self_arc.members.write().await;
                            for id in dead_ids {
                                members.remove(&id);
                                debug!(node_id = %id, "removed dead node from gossip pool");
                            }
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gossip_engine_creation() {
        let engine = GossipEngine::new("local-node", "127.0.0.1:9000");
        let members = engine.get_members().await;
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].node_id, "local-node");
        assert_eq!(members[0].state, GossipState::Alive);
    }

    #[tokio::test]
    async fn test_add_seed() {
        let engine = GossipEngine::new("local", "127.0.0.1:9000");
        engine.add_seed("seed-1", "10.0.0.1:9000").await;
        let members = engine.get_members().await;
        assert_eq!(members.len(), 2);
    }

    #[tokio::test]
    async fn test_handle_gossip_message_new_node() {
        let engine = GossipEngine::new("local", "127.0.0.1:9000");
        let remote = vec![GossipMember::new("remote-1", "10.0.0.2:9000")];
        engine.handle_gossip_message("remote-1", remote).await;
        let members = engine.get_members().await;
        assert_eq!(members.len(), 2);
    }

    #[tokio::test]
    async fn test_handle_gossip_message_ignores_self() {
        let engine = GossipEngine::new("local", "127.0.0.1:9000");
        let self_msg = vec![GossipMember::new("local", "old-addr:9000")];
        engine.handle_gossip_message("local", self_msg).await;
        let members = engine.get_members().await;
        assert_eq!(members.len(), 1);
        // Should not overwrite with old data
        assert_eq!(members[0].addr, "127.0.0.1:9000");
    }

    #[tokio::test]
    async fn test_mark_suspect_and_dead() {
        let engine = GossipEngine::new("local", "127.0.0.1:9000");
        engine.add_seed("peer", "10.0.0.1:9000").await;

        engine.mark_suspect("peer").await;
        let members = engine.get_members().await;
        let peer = members.iter().find(|m| m.node_id == "peer").unwrap();
        assert_eq!(peer.state, GossipState::Suspect);

        engine.mark_dead("peer").await;
        let members = engine.get_members().await;
        let peer = members.iter().find(|m| m.node_id == "peer").unwrap();
        assert_eq!(peer.state, GossipState::Dead);
    }

    #[tokio::test]
    async fn test_alive_members_filter() {
        let engine = GossipEngine::new("local", "127.0.0.1:9000");
        engine.add_seed("alive-1", "10.0.0.1:9000").await;
        engine.add_seed("alive-2", "10.0.0.2:9000").await;
        engine.mark_dead("alive-2").await;

        let alive = engine.get_alive_members().await;
        assert_eq!(alive.len(), 2); // local + alive-1
        assert!(alive.iter().any(|m| m.node_id == "local"));
        assert!(alive.iter().any(|m| m.node_id == "alive-1"));
    }

    #[test]
    fn test_member_merge_newer_incarnation() {
        let mut local = GossipMember::new("node", "addr1");
        let mut remote = GossipMember::new("node", "addr2");
        remote.incarnation = 1;
        assert!(local.merge(&remote));
        assert_eq!(local.addr, "addr2");
    }

    #[test]
    fn test_member_merge_older_incarnation() {
        let mut local = GossipMember::new("node", "addr1");
        local.incarnation = 2;
        let remote = GossipMember::new("node", "addr2");
        assert!(!local.merge(&remote)); // Should not change (local is newer)
        assert_eq!(local.addr, "addr1");
    }

    #[test]
    fn test_gossip_event_channel() {
        let engine = GossipEngine::new("local", "127.0.0.1:9000");
        let rx = engine.take_event_rx();
        assert!(rx.is_some());
        // Second call should return None (already taken)
        assert!(engine.take_event_rx().is_none());
    }
}
