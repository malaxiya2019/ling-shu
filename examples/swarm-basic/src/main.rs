//! 🐝 LingShu Swarm 基础示例
//!
//! 演示：创建 Swarm 引擎 → 添加 Agent → 拓扑切换 → 获取状态 → 停止

use lingshu_swarm::{
    SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine, SwarmStrategy, SwarmTopology,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("🚀 LingShu Swarm 示例");
    println!("====================\n");

    // 1. 创建配置
    let config = SwarmConfig {
        name: "demo-swarm".into(),
        strategy: SwarmStrategy::Democratic,
        topology: SwarmTopology::Mesh,
        min_agents: 1,
        max_agents: 10,
        task_timeout: Duration::from_secs(30),
        heartbeat_interval: Duration::from_secs(5),
        enable_autonomy: false,
        enable_emergent_specialization: true,
        idle_timeout: Duration::from_secs(300),
        consensus_threshold: 0.6,
        max_retries: 3,
        enable_audit_trail: false,
    };

    // 2. 创建引擎并启动
    let engine = SwarmEngine::new(config);
    engine.start().await.expect("swarm 启动失败");
    println!("✅ Swarm 引擎已启动");

    // 3. 添加 Agent
    let agents = vec![
        SwarmAgent::new("分析者-Alice", SwarmAgentRole::Analyst),
        SwarmAgent::new("规划者-Bob", SwarmAgentRole::Planner),
        SwarmAgent::new("执行者-Charlie", SwarmAgentRole::Executor),
    ];
    engine.add_agents(agents).await.expect("添加 Agent 失败");
    println!("✅ 已添加 3 个 Agent");

    // 4. 查看状态
    let state = engine.state().await;
    println!(
        "📊 Swarm 状态: {} Agent, 策略={:?}, 拓扑={:?}",
        state.agents.len(),
        state.strategy,
        state.topology,
    );

    // 5. 切换拓扑
    let agents = state.agents;
    engine
        .topology()
        .switch_topology(SwarmTopology::Star, &agents)
        .await;
    let stats = engine.topology().stats().await;
    println!("📡 拓扑切换完成: Star, 连接状态={}", stats.is_connected);

    // 6. 获取摘要
    let summary = engine.summary().await;
    println!(
        "📈 摘要: Agent={}, 完成={}, 成功率={:.1}%",
        summary.agent_count,
        summary.total_tasks_completed,
        summary.overall_success_rate * 100.0,
    );

    // 7. 停止
    engine.stop().await;
    println!("\n👋 Swarm 已停止");
}
