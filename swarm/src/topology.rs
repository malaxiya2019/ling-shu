//! AgentSwarm — 动态拓扑管理
//!
//! 管理 Swarm 中 Agent 之间的通信拓扑。
//! 支持 Star / Mesh / Ring / Tree / Dynamic 拓扑，
//! 可根据任务特征和 Swarm 状态自适应调整。

use crate::types::*;
use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use tracing::{debug, info};

// ── 拓扑节点 ────────────────────────────────────────

/// 拓扑中的节点连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    /// Agent ID
    pub agent_id: LsId,
    /// 连接的邻居 Agent ID 列表
    pub neighbors: Vec<LsId>,
    /// 节点层级（Tree 拓扑使用）
    pub level: usize,
    /// 父节点（Tree 拓扑使用）
    pub parent: Option<LsId>,
}

/// 拓扑统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyStats {
    /// 拓扑类型
    pub topology: SwarmTopology,
    /// 节点数
    pub node_count: usize,
    /// 连接数
    pub edge_count: usize,
    /// 平均度数
    pub avg_degree: f64,
    /// 网络直径
    pub diameter: usize,
    /// 是否连通
    pub is_connected: bool,
}

// ── 拓扑管理器 ──────────────────────────────────────

/// 动态拓扑管理器
pub struct TopologyManager {
    /// 当前拓扑类型
    current: RwLock<SwarmTopology>,
    /// 拓扑节点映射
    nodes: RwLock<HashMap<LsId, TopologyNode>>,
    /// 切换历史
    switches: RwLock<Vec<TopologySwitch>>,
}

/// 拓扑切换记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySwitch {
    pub from: SwarmTopology,
    pub to: SwarmTopology,
    pub reason: String,
    pub timestamp: i64,
}

impl TopologyManager {
    pub fn new(initial: SwarmTopology) -> Self {
        Self {
            current: RwLock::new(initial),
            nodes: RwLock::new(HashMap::new()),
            switches: RwLock::new(Vec::new()),
        }
    }

    /// 获取当前拓扑
    pub async fn current_topology(&self) -> SwarmTopology {
        *self.current.read().await
    }

    /// 注册 Agent 到拓扑
    pub async fn register_agent(&self, agent: &SwarmAgent, all_agents: &[SwarmAgent]) {
        let mut nodes = self.nodes.write().await;
        let topology = *self.current.read().await;

        let neighbors = Self::calculate_neighbors(&agent.id, all_agents, topology);
        let (level, parent) = Self::calculate_hierarchy(&agent.id, all_agents, topology);

        nodes.insert(
            agent.id,
            TopologyNode {
                agent_id: agent.id,
                neighbors,
                level,
                parent,
            },
        );

        debug!(
            "registered agent '{}' in {:?} topology with {} neighbors",
            agent.name,
            topology,
            nodes.get(&agent.id).map(|n| n.neighbors.len()).unwrap_or(0)
        );
    }

    /// 移除 Agent
    pub async fn remove_agent(&self, agent_id: &LsId) {
        let mut nodes = self.nodes.write().await;
        nodes.remove(agent_id);

        // 更新其他节点的邻居列表
        for node in nodes.values_mut() {
            node.neighbors.retain(|n| n != agent_id);
        }
    }

    /// 获取 Agent 的邻居
    pub async fn get_neighbors(&self, agent_id: &LsId) -> Vec<LsId> {
        let nodes = self.nodes.read().await;
        nodes
            .get(agent_id)
            .map(|n| n.neighbors.clone())
            .unwrap_or_default()
    }

    /// 切换拓扑
    pub async fn switch_topology(&self, new_topology: SwarmTopology, agents: &[SwarmAgent]) {
        let mut current = self.current.write().await;
        let old = *current;

        if old == new_topology {
            return;
        }

        info!("switching topology: {:?} → {:?}", old, new_topology);
        *current = new_topology;

        // 重新计算所有节点的邻居
        let mut nodes = self.nodes.write().await;
        for agent in agents {
            let neighbors = Self::calculate_neighbors(&agent.id, agents, new_topology);
            let (level, parent) = Self::calculate_hierarchy(&agent.id, agents, new_topology);
            nodes.insert(
                agent.id,
                TopologyNode {
                    agent_id: agent.id,
                    neighbors,
                    level,
                    parent,
                },
            );
        }

        // 记录切换
        let mut switches = self.switches.write().await;
        switches.push(TopologySwitch {
            from: old,
            to: new_topology,
            reason: format!("Topology change: {:?} → {:?}", old, new_topology),
            timestamp: chrono::Utc::now().timestamp(),
        });
    }

    /// 自适应拓扑选择：根据 Swarm 状态选择最优拓扑
    pub async fn adaptive_topology(&self, state: &SwarmState) -> SwarmTopology {
        let agent_count = state.agent_count();
        let available = state.available_agent_count();
        let busy = state.busy_agent_count();

        match agent_count {
            0..=3 => SwarmTopology::Mesh,    // 小型 Swarm 用全互联
            4..=10 => SwarmTopology::Star,   // 中型用星型
            11..=20 => SwarmTopology::Tree,   // 较大用树型
            _ => {
                if busy > available * 2 {
                    SwarmTopology::Ring // 高负载时用环形减少连接
                } else {
                    SwarmTopology::Dynamic
                }
            }
        }
    }

    /// 获取拓扑统计
    pub async fn stats(&self) -> TopologyStats {
        let nodes = self.nodes.read().await;
        let topology = *self.current.read().await;

        let node_count = nodes.len();
        let edge_count: usize = nodes.values().map(|n| n.neighbors.len()).sum::<usize>() / 2;
        let avg_degree = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        // 简单连通性检查
        let is_connected = if node_count <= 1 {
            true
        } else {
            let mut visited = HashSet::new();
            let mut stack = vec![nodes.keys().next().copied().unwrap()];
            while let Some(id) = stack.pop() {
                if visited.contains(&id) {
                    continue;
                }
                visited.insert(id);
                if let Some(node) = nodes.get(&id) {
                    for neighbor in &node.neighbors {
                        if !visited.contains(neighbor) {
                            stack.push(*neighbor);
                        }
                    }
                }
            }
            visited.len() == node_count
        };

        TopologyStats {
            topology,
            node_count,
            edge_count,
            avg_degree,
            diameter: node_count, // 简化计算
            is_connected,
        }
    }

    /// 获取切换历史
    pub async fn get_switches(&self) -> Vec<TopologySwitch> {
        self.switches.read().await.clone()
    }

    // ── 辅助方法 ──

    fn calculate_neighbors(agent_id: &LsId, agents: &[SwarmAgent], topology: SwarmTopology) -> Vec<LsId> {
        let other_ids: Vec<LsId> = agents.iter().filter(|a| a.id != *agent_id).map(|a| a.id).collect();

        match topology {
            SwarmTopology::Mesh => other_ids, // 全连接
            SwarmTopology::Star => {
                // 星型：所有节点连接到一个中心节点（第一个注册的 Agent）
                if let Some(center) = agents.first() {
                    if center.id == *agent_id {
                        other_ids // 中心连接所有
                    } else {
                        vec![center.id] // 非中心只连中心
                    }
                } else {
                    Vec::new()
                }
            }
            SwarmTopology::Ring => {
                // 环形：前后各一个邻居
                let positions: Vec<&LsId> = agents.iter().map(|a| &a.id).collect();
                if let Some(pos) = positions.iter().position(|id| *id == agent_id) {
                    let prev = positions[(pos + positions.len() - 1) % positions.len()];
                    let next = positions[(pos + 1) % positions.len()];
                    vec![*prev, *next]
                } else {
                    Vec::new()
                }
            }
            SwarmTopology::Tree => {
                // 树型：只连父节点和子节点（按层级）
                let mut neighbors = Vec::new();
                let my_idx = agents.iter().position(|a| a.id == *agent_id);
                if let Some(idx) = my_idx {
                    // 父节点
                    if idx > 0 {
                        let parent_idx = (idx - 1) / 2;
                        neighbors.push(agents[parent_idx].id);
                    }
                    // 子节点
                    let left_child = 2 * idx + 1;
                    let right_child = 2 * idx + 2;
                    if left_child < agents.len() {
                        neighbors.push(agents[left_child].id);
                    }
                    if right_child < agents.len() {
                        neighbors.push(agents[right_child].id);
                    }
                }
                neighbors
            }
            SwarmTopology::Dynamic => {
                // 动态：基于能力相似度连接
                let my_agent = agents.iter().find(|a| a.id == *agent_id);
                if let Some(agent) = my_agent {
                    let mut scored: Vec<(f64, &LsId)> = agents
                        .iter()
                        .filter(|a| a.id != *agent_id)
                        .map(|a| {
                            let score = (agent.capability_score - a.capability_score).abs();
                            (score, &a.id)
                        })
                        .collect();
                    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

                    // 连接能力最接近的 3 个 Agent
                    scored.iter().take(3).map(|(_, id)| **id).collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn calculate_hierarchy(agent_id: &LsId, agents: &[SwarmAgent], topology: SwarmTopology) -> (usize, Option<LsId>) {
        match topology {
            SwarmTopology::Tree | SwarmTopology::Star => {
                let idx = agents.iter().position(|a| a.id == *agent_id);
                if let Some(idx) = idx {
                    if idx == 0 {
                        (0, None) // Root
                    } else {
                        let parent_idx = (idx - 1) / 2;
                        let level = (idx as f64 + 1.0).log2().ceil() as usize - 1;
                        (level, Some(agents[parent_idx].id))
                    }
                } else {
                    (0, None)
                }
            }
            _ => (0, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_agents(count: usize) -> Vec<SwarmAgent> {
        (0..count)
            .map(|i| {
                let mut agent = SwarmAgent::new(format!("agent-{}", i), SwarmAgentRole::Executor);
                agent.capability_score = 0.5 + (i as f64 * 0.1);
                agent
            })
            .collect()
    }

    #[tokio::test]
    async fn test_topology_mesh() {
        let manager = TopologyManager::new(SwarmTopology::Mesh);
        let agents = create_agents(4);

        for agent in &agents {
            manager.register_agent(agent, &agents).await;
        }

        let stats = manager.stats().await;
        assert_eq!(stats.node_count, 4);
        assert!(stats.is_connected);

        // Mesh: each node connects to all others = n*(n-1)/2 edges
        assert_eq!(stats.edge_count, 6); // 4*3/2 = 6
    }

    #[tokio::test]
    async fn test_topology_star() {
        let manager = TopologyManager::new(SwarmTopology::Star);
        let agents = create_agents(5);

        for agent in &agents {
            manager.register_agent(agent, &agents).await;
        }

        // Center (agent-0) should have 4 neighbors, others should have 1
        let center_neighbors = manager.get_neighbors(&agents[0].id).await;
        assert_eq!(center_neighbors.len(), 4);

        let leaf_neighbors = manager.get_neighbors(&agents[1].id).await;
        assert_eq!(leaf_neighbors.len(), 1);
    }

    #[tokio::test]
    async fn test_topology_ring() {
        let manager = TopologyManager::new(SwarmTopology::Ring);
        let agents = create_agents(3);

        for agent in &agents {
            manager.register_agent(agent, &agents).await;
        }

        let stats = manager.stats().await;
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 3); // 3 nodes in a ring = 3 edges
    }

    #[tokio::test]
    async fn test_topology_tree() {
        let manager = TopologyManager::new(SwarmTopology::Tree);
        let agents = create_agents(7);

        for agent in &agents {
            manager.register_agent(agent, &agents).await;
        }

        let stats = manager.stats().await;
        assert_eq!(stats.node_count, 7);
    }

    #[tokio::test]
    async fn test_remove_agent() {
        let manager = TopologyManager::new(SwarmTopology::Mesh);
        let agents = create_agents(3);

        for agent in &agents {
            manager.register_agent(agent, &agents).await;
        }

        manager.remove_agent(&agents[0].id).await;

        let stats = manager.stats().await;
        assert_eq!(stats.node_count, 2);
    }

    #[tokio::test]
    async fn test_switch_topology() {
        let manager = TopologyManager::new(SwarmTopology::Mesh);
        let agents = create_agents(4);

        for agent in &agents {
            manager.register_agent(agent, &agents).await;
        }

        assert_eq!(manager.current_topology().await, SwarmTopology::Mesh);
        manager.switch_topology(SwarmTopology::Star, &agents).await;
        assert_eq!(manager.current_topology().await, SwarmTopology::Star);

        let switches = manager.get_switches().await;
        assert_eq!(switches.len(), 1);
    }

    #[tokio::test]
    async fn test_adaptive_topology() {
        let manager = TopologyManager::new(SwarmTopology::Dynamic);

        // Small swarm (<=3 agents) → Mesh
        let mut state = SwarmState::new("small", SwarmStrategy::Democratic, SwarmTopology::Dynamic);
        for _i in 0..3 {
            state.agents.push(create_agents(1)[0].clone());
        }
        let topology = manager.adaptive_topology(&state).await;
        assert_eq!(topology, SwarmTopology::Mesh);
    }

    #[test]
    fn test_topology_stats_display() {
        let stats = TopologyStats {
            topology: SwarmTopology::Mesh,
            node_count: 5,
            edge_count: 10,
            avg_degree: 4.0,
            diameter: 1,
            is_connected: true,
        };
        assert_eq!(stats.node_count, 5);
        assert!(stats.is_connected);
    }
}
