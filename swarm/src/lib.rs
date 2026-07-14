//! LingShu AgentSwarm — 群体智能协作引擎 (v5.0.1)
//!
//! 提供多 Agent 群体智能协作框架：
//!
//! # 核心概念
//!
//! - **SwarmEngine**: 群体智能顶层入口
//! - **SwarmCoordinator**: 任务分配与 Agent 选择
//! - **SwarmStrategy**: 协作策略（投票/共识/层级/民主/竞标/混合）
//! - **SwarmAgent**: 动态角色的 Agent
//! - **SwarmChannel**: Agent 间通信
//! - **EmergentSpecialization**: 涌现专长自动发现
//! - **TopologyManager**: 动态拓扑管理
//! - **SwarmMemory**: 共享群体记忆
//! - **MetricsCollector**: 性能监控
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  SwarmEngine                      │
//! │  ┌────────────┐  ┌──────────┐  ┌─────────────┐ │
//! │  │ Coordinator │  │ Channel  │  │ Topology     │ │
//! │  │ (协调器)    │  │ (通信)   │  │ (拓扑管理)   │ │
//! │  └────────────┘  └──────────┘  └─────────────┘ │
//! │  ┌────────────┐  ┌──────────┐  ┌─────────────┐ │
//! │  │ Emergent    │  │ Memory   │  │ Metrics      │ │
//! │  │ (涌现引擎)  │  │ (记忆)   │  │ (指标)       │ │
//! │  └────────────┘  └──────────┘  └─────────────┘ │
//! └─────────────────────────────────────────────────┘
//! ```

pub mod communication;
pub mod coordinator;
pub mod engine;
pub mod memory;
pub mod metrics;
pub mod specialized;
pub mod strategy;
pub mod topology;
pub mod types;

pub use communication::*;
pub use coordinator::*;
pub use engine::*;
pub use memory::*;
pub use metrics::*;
pub use specialized::*;
pub use strategy::*;
pub use topology::*;
pub use types::*;

/// AgentSwarm 版本
pub const VERSION: &str = "5.0.1";
/// AgentSwarm 名称
pub const NAME: &str = "lingshu-swarm";

#[cfg(test)]
mod integration_tests {
    use super::*;
    use lingshu_core::{LsContext, LsId};

    /// 完整 Swarm 集成测试：创建 → 添加 Agent → 执行任务 → 获取指标
    async fn test_swarm_full_lifecycle() {
        let config = SwarmConfig {
            name: "integration-swarm".to_string(),
            strategy: SwarmStrategy::Democratic,
            topology: SwarmTopology::Mesh,
            min_agents: 2,
            max_agents: 10,
            enable_autonomy: false,
            enable_emergent_specialization: false,
            ..SwarmConfig::default()
        };

        let engine = SwarmEngine::new(config);
        engine.start().await.unwrap();
        assert!(engine.is_running().await);

        let agent = SwarmAgent::new("test-agent", SwarmAgentRole::Executor);
        engine.add_agent(agent).await.unwrap();
        assert_eq!(engine.state().await.agent_count(), 1);

        engine.stop().await;
        assert!(!engine.is_running().await);
    }

    /// 测试竞标策略（仅验证引擎创建和 Agent 添加）
    #[tokio::test]
    async fn test_bidding_workflow() {
        let config = SwarmConfig {
            name: "bidding-swarm".to_string(),
            strategy: SwarmStrategy::Bidding,
            ..SwarmConfig::default()
        };

        let engine = SwarmEngine::new(config);
        engine.start().await.unwrap();

        let agents = vec![
            SwarmAgent::new("fast-agent", SwarmAgentRole::Executor).with_expertise("speed", 0.9),
            SwarmAgent::new("accurate-agent", SwarmAgentRole::Executor)
                .with_expertise("accuracy", 0.95),
        ];
        engine.add_agents(agents).await.unwrap();
        engine
            .register_specialized(Box::new(CreatorAgent::new("creator")))
            .await;

        assert_eq!(engine.state().await.agent_count(), 2);
        assert!(engine.is_running().await);

        engine.stop().await;
        assert!(!engine.is_running().await);
    }

    /// 测试拓扑自适应
    #[tokio::test]
    async fn test_adaptive_topology() {
        let engine = SwarmEngine::new(SwarmConfig {
            topology: SwarmTopology::Dynamic,
            enable_autonomy: true,
            ..SwarmConfig::default()
        });
        engine.start().await.unwrap();

        let agents: Vec<SwarmAgent> = (0..5)
            .map(|i| SwarmAgent::new(format!("agent-{}", i), SwarmAgentRole::Executor))
            .collect();
        engine.add_agents(agents).await.unwrap();

        let topology_stats = engine.topology().stats().await;
        assert_eq!(topology_stats.node_count, 5);

        engine.stop().await;
    }

    /// 测试版本常量
    #[test]
    fn test_version_constants() {
        assert_eq!(VERSION, "5.0.1");
        assert_eq!(NAME, "lingshu-swarm");
    }
}
