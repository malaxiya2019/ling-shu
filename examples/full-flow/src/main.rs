//! 🎯 LingShu 全流程示例 — Swarm + 分布式调度 + 自治引擎
//!
//! 演示 LingShu v5.0 三大核心能力的端到端集成：
//! 1. Swarm — Agent 群体协作
//! 2. Distributed — 分布式集群调度
//! 3. Autonomy — 自我反思与进化

use lingshu_swarm::{
    SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine,
    SwarmStrategy, SwarmTopology,
};
use lingshu_distributed::{
    Cluster, ClusterConfig, DistScheduler, DistSchedulerConfig,
    DistScheduleStrategy, DistTask,
};
use lingshu_autonomy::{
    AutonomyEngine, EvolutionConfig, ReflectionConfig,
    ExperienceEntry, ExperienceType, ExperienceOutcome, ExperienceSeverity,
    AgentParameters,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("🎯 LingShu v5.0 全流程集成示例");
    println!("================================\n");

    // ═══════════════════════════════════════════════
    // 第一阶段：启动基础设施
    // ═══════════════════════════════════════════════

    // 1. 启动分布式集群
    println!("[1/6] 启动分布式集群...");
    let cluster_config = ClusterConfig {
        node_id: "master-node".into(),
        bind_addr: "127.0.0.1:0".into(),
        seed_nodes: vec![],
        heartbeat_interval: Duration::from_secs(1),
        suspicion_mult: 3,
        cleanup_interval: Duration::from_secs(10),
        gossip_interval: Duration::from_millis(500),
        gossip_fanout: 3,
    };
    let cluster = Arc::new(Cluster::new(cluster_config));
    cluster.start().await;
    println!("   ✅ 集群已启动 (master-node)");

    // 2. 启动分布式调度器
    println!("\n[2/6] 启动分布式调度器...");
    let scheduler_config = DistSchedulerConfig {
        strategy: DistScheduleStrategy::Adaptive,
        local_node_id: "master-node".into(),
        max_retries: 3,
        task_timeout_secs: 60,
        node_timeout_secs: 30,
        batch_size: 10,
        enable_auto_failover: true,
        health_check_interval: Duration::from_secs(5),
    };
    let scheduler = DistScheduler::new(scheduler_config, cluster.clone());
    scheduler.start().await;
    println!("   ✅ 调度器已启动 (策略=Adaptive)");

    // 3. 创建 Swarm 引擎
    println!("\n[3/6] 创建 Swarm 协作引擎...");
    let swarm_config = SwarmConfig {
        name: "production-swarm".into(),
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
    let swarm = SwarmEngine::new(swarm_config);
    swarm.start().await.expect("swarm 启动失败");
    println!("   ✅ Swarm 引擎已启动");

    // 4. 创建自治引擎
    println!("\n[4/6] 创建自治引擎...");
    let autonomy = AutonomyEngine::new(
        ReflectionConfig {
            confidence_threshold: 0.3,
            failure_pattern_window: 10,
            degradation_window: 20,
            min_experiences_for_reflection: 3,
            auto_mark_analyzed: true,
        },
        EvolutionConfig {
            auto_apply_threshold: 8,
            verification_wait: Duration::from_secs(60),
            max_concurrent_plans: 5,
            cooldown: Duration::from_secs(300),
            enable_auto_rollback: true,
            rollback_threshold: -0.3,
        },
        1000,
    );
    println!("   ✅ 自治引擎已创建");

    // ═══════════════════════════════════════════════
    // 第二阶段：注册 Agent
    // ═══════════════════════════════════════════════

    println!("\n[5/6] 注册 Agent...");

    // 注册 Swarm Agent
    let agents = vec![
        SwarmAgent::new("分析者-Alice", SwarmAgentRole::Analyst),
        SwarmAgent::new("规划者-Bob", SwarmAgentRole::Planner),
        SwarmAgent::new("执行者-Charlie", SwarmAgentRole::Executor),
        SwarmAgent::new("审查者-Diana", SwarmAgentRole::Validator),
    ];
    swarm.add_agents(agents).await.expect("添加 Agent 失败");
    println!("   ✅ Swarm: 4 个 Agent 已加入");

    // 注册自治参数
    for name in &["分析者-Alice", "规划者-Bob", "执行者-Charlie", "审查者-Diana"] {
        let params = AgentParameters {
            agent_id: name.to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 300,
            max_retries: 3,
            collaboration_strategy: "default".into(),
            default_priority: 5,
            version: 1,
            updated_at: chrono::Utc::now().timestamp(),
        };
        autonomy.evolution_engine.register_agent(name, params).await;
    }
    println!("   ✅ Autonomy: 4 个 Agent 参数已注册");

    // ═══════════════════════════════════════════════
    // 第三阶段：执行任务 + 记录经验 + 调度
    // ═══════════════════════════════════════════════

    println!("\n[6/6] 执行端到端任务流程...\n");

    // 任务 1：数据分析
    println!("─── 任务 1: 数据分析 ───");
    let task1 = DistTask::new(
        "Q3 销售数据分析",
        "analysis",
        serde_json::json!({"dataset": "sales_q3", "period": "2025-Q3"}),
    );
    let result1 = scheduler.submit_task(task1).await;
    match &result1 {
        Ok(r) => println!("   📋 调度: Q3 销售数据分析 → {}", r.assigned_node_id),
        Err(e) => println!("   ❌ 调度失败: {}", e),
    }

    // 记录经验
    let exp1 = ExperienceEntry::new(
        "分析者-Alice",
        ExperienceType::TaskExecution,
        "Q3 销售数据分析完成",
        "成功完成销售数据分析",
        ExperienceOutcome::Success,
    )
    .with_severity(ExperienceSeverity::Info)
    .with_tag("analysis");
    autonomy.experience_store.store(exp1).await;
    println!("   💾 经验已记录");

    // 任务 2：方案规划
    println!("\n─── 任务 2: 方案规划 ───");
    let task2 = DistTask::new(
        "Q4 营销策略规划",
        "planning",
        serde_json::json!({"budget": 500000, "channels": ["social", "email"]}),
    )
    .with_priority(8);
    let result2 = scheduler.submit_task(task2).await;
    match &result2 {
        Ok(r) => println!("   📋 调度: Q4 营销策略规划 → {}", r.assigned_node_id),
        Err(e) => println!("   ❌ 调度失败: {}", e),
    }

    let exp2 = ExperienceEntry::new(
        "规划者-Bob",
        ExperienceType::TaskExecution,
        "Q4 营销策略规划完成",
        "成功规划营销策略",
        ExperienceOutcome::Success,
    )
    .with_severity(ExperienceSeverity::Info)
    .with_tag("planning");
    autonomy.experience_store.store(exp2).await;
    println!("   💾 经验已记录");

    // 任务 3：执行部署
    println!("\n─── 任务 3: 代码部署 ───");
    let task3 = DistTask::new(
        "生产环境部署 v5.0",
        "deployment",
        serde_json::json!({"version": "5.0.0", "env": "production"}),
    )
    .with_priority(10);
    let result3 = scheduler.submit_task(task3).await;
    match &result3 {
        Ok(r) => println!("   📋 调度: 生产环境部署 v5.0 → {}", r.assigned_node_id),
        Err(e) => println!("   ❌ 调度失败: {}", e),
    }

    let exp3 = ExperienceEntry::new(
        "执行者-Charlie",
        ExperienceType::TaskExecution,
        "生产环境部署 v5.0",
        "成功执行生产环境部署",
        ExperienceOutcome::Success,
    )
    .with_severity(ExperienceSeverity::Critical)
    .with_tag("deployment");
    autonomy.experience_store.store(exp3).await;
    println!("   💾 经验已记录");

    // 任务 4：代码审查 — 模拟失败
    println!("\n─── 任务 4: 代码审查 (模拟失败) ───");
    let exp4 = ExperienceEntry::new(
        "审查者-Diana",
        ExperienceType::Feedback,
        "PR #142 审查发现关键漏洞",
        "发现 SQL 注入漏洞，需紧急修复",
        ExperienceOutcome::Failure("security_risk".into()),
    )
    .with_severity(ExperienceSeverity::Critical)
    .with_tag("security")
    .with_context(serde_json::json!({
        "pr": 142,
        "vulnerability": "SQL injection in user input handler",
        "severity": "critical"
    }));
    autonomy.experience_store.store(exp4).await;
    println!("   💾 经验已记录 (失败)");

    // ═══════════════════════════════════════════════
    // 第四阶段：监控与反馈
    // ═══════════════════════════════════════════════

    println!("\n══════════ 监控与反馈 ══════════\n");

    // Swarm 状态
    let s_state = swarm.state().await;
    println!("📊 Swarm 状态: {} Agent, 策略={:?}, 拓扑={:?}",
        s_state.agents.len(), s_state.strategy, s_state.topology);

    let summary = swarm.summary().await;
    println!("📈 Swarm 摘要: 完成={}, 成功率={:.1}%",
        summary.total_tasks_completed, summary.overall_success_rate * 100.0);

    // 调度器统计
    let s_summary = scheduler.summary().await;
    println!("\n📊 调度器统计: 在线节点={}/{} | 待处理={}, 活跃={}",
        s_summary.online_nodes,
        s_summary.total_nodes,
        s_summary.total_pending_tasks,
        s_summary.total_active_tasks,
    );

    let loads = scheduler.get_all_loads().await;
    for load in &loads {
        println!("   ⚡ 节点 {}: 活跃任务={}, CPU={:.1}%, 内存={:.1}%",
            load.node_id, load.active_tasks,
            load.cpu_usage * 100.0, load.memory_usage * 100.0);
    }

    // 自治引擎 — 经验摘要
    for name in &["分析者-Alice", "规划者-Bob", "执行者-Charlie", "审查者-Diana"] {
        let exp_summary = autonomy.experience_store.summarize(name).await;
        println!("\n📊 [{}] 经验: 总计={}, 成功={}, 失败={}, 成功率={:.1}%",
            name,
            exp_summary.total_count,
            exp_summary.success_count,
            exp_summary.failure_count,
            exp_summary.success_rate * 100.0,
        );
    }

    // 自治引擎 — 反思
    println!("\n🔍 执行自治反思...");
    for name in &["分析者-Alice", "审查者-Diana"] {
        let report = autonomy.reflect_only(name).await;
        println!("   [{2}] 健康评分={0:.1}/1.0, 洞察数={1}",
            report.health_score, report.insights.len(), name);
        for insight in &report.insights {
            println!("      💡 {1} (优先级={0})", insight.priority, insight.title);
        }
    }

    // ═══════════════════════════════════════════════
    // 第五阶段：清理
    // ═══════════════════════════════════════════════

    println!("\n══════════ 清理 ══════════\n");

    scheduler.stop().await;
    println!("👋 调度器已停止");

    swarm.stop().await;
    println!("👋 Swarm 已停止");

    println!("\n✅ 全流程集成示例完成");
    println!("   已演示: Swarm + 分布式调度 + 自治引擎");
}
