//! 🐝 v5.0 端到端集成测试 — Swarm + Distributed + Autonomy
//!
//! 这些测试验证跨 crate 的集成点，不依赖外部 LLM 后端。
//! Swarm submit_task 需要 LLM 后端，在集成测试中跳过。

// ── 1. Swarm 基础生命周期 ──────────────────────────

#[tokio::test]
async fn e2e_swarm_create_and_manage_agents() {
    use lingshu_swarm::{
        SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine, SwarmStrategy, SwarmTopology,
    };
    use std::time::Duration;

    let config = SwarmConfig {
        name: "e2e-test-swarm".into(),
        strategy: SwarmStrategy::Consensus,
        topology: SwarmTopology::Mesh,
        min_agents: 1,
        max_agents: 10,
        task_timeout: Duration::from_secs(10),
        heartbeat_interval: Duration::from_secs(1),
        enable_autonomy: false,
        enable_emergent_specialization: false,
        idle_timeout: Duration::from_secs(60),
        consensus_threshold: 0.6,
        max_retries: 2,
        enable_audit_trail: false,
    };

    let engine = SwarmEngine::new(config);
    engine.start().await.unwrap();

    let mut agents = Vec::new();
    for i in 0..3 {
        let role = match i {
            0 => SwarmAgentRole::Planner,
            1 => SwarmAgentRole::Executor,
            _ => SwarmAgentRole::Validator,
        };
        agents.push(SwarmAgent::new(format!("agent-{i}"), role));
    }
    engine.add_agents(agents).await.unwrap();

    let state = engine.state().await;
    assert_eq!(state.agents.len(), 3);
    assert!(engine.is_running().await);

    let summary = engine.summary().await;
    assert_eq!(summary.agent_count, 3);

    engine.stop().await;
    assert!(!engine.is_running().await);
}

#[tokio::test]
async fn e2e_swarm_topology_switch() {
    use lingshu_swarm::{
        SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine, SwarmStrategy, SwarmTopology,
    };
    use std::time::Duration;

    let config = SwarmConfig {
        name: "e2e-topology-swarm".into(),
        strategy: SwarmStrategy::Democratic,
        topology: SwarmTopology::Star,
        min_agents: 1,
        max_agents: 5,
        task_timeout: Duration::from_secs(10),
        heartbeat_interval: Duration::from_secs(1),
        enable_autonomy: false,
        enable_emergent_specialization: false,
        idle_timeout: Duration::from_secs(60),
        consensus_threshold: 0.5,
        max_retries: 2,
        enable_audit_trail: false,
    };

    let engine = SwarmEngine::new(config);
    engine.start().await.unwrap();

    let mut agents = Vec::new();
    for i in 0..4 {
        let agent = SwarmAgent::new(format!("topo-{i}"), SwarmAgentRole::Executor);
        engine.add_agent(agent.clone()).await.unwrap();
        agents.push(agent);
    }

    let summary = engine.summary().await;
    assert_eq!(summary.topology.as_str(), "star");
    assert_eq!(summary.agent_count, 4);

    // 切换拓扑
    engine.topology().switch_topology(SwarmTopology::Ring, &agents).await;
    let stats = engine.topology().stats().await;
    assert!(stats.is_connected);

    engine.stop().await;
}

#[tokio::test]
async fn e2e_swarm_emergent_specialization() {
    use lingshu_swarm::{
        SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine, SwarmStrategy, SwarmTopology,
    };
    use std::time::Duration;

    let config = SwarmConfig {
        name: "e2e-emergent-swarm".into(),
        strategy: SwarmStrategy::Bidding,
        topology: SwarmTopology::Dynamic,
        min_agents: 1,
        max_agents: 10,
        task_timeout: Duration::from_secs(10),
        heartbeat_interval: Duration::from_secs(1),
        enable_autonomy: false,
        enable_emergent_specialization: true,
        idle_timeout: Duration::from_secs(60),
        consensus_threshold: 0.6,
        max_retries: 2,
        enable_audit_trail: false,
    };

    let engine = SwarmEngine::new(config);
    engine.start().await.unwrap();

    let agents: Vec<SwarmAgent> = (0..3)
        .map(|i| SwarmAgent::new(format!("emergent-{i}"), SwarmAgentRole::Executor))
        .collect();
    engine.add_agents(agents).await.unwrap();

    let state = engine.state().await;
    assert_eq!(state.agents.len(), 3);

    // 验证涌现引擎已启用
    assert!(engine.is_running().await);

    engine.stop().await;
}

// ── 2. Distributed Scheduler ────────────────────────

#[tokio::test]
async fn e2e_distributed_scheduler_submit_task() {
    use lingshu_core::LsId;
    use lingshu_distributed::{
        Cluster, ClusterConfig, DistScheduler, DistSchedulerConfig, DistScheduleStrategy, DistTask,
    };
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    let cluster = Arc::new(Cluster::new(ClusterConfig {
        node_id: "test-node".into(),
        bind_addr: "127.0.0.1:0".into(),
        seed_nodes: vec![],
        heartbeat_interval: Duration::from_secs(1),
        suspicion_mult: 3,
        cleanup_interval: Duration::from_secs(10),
        gossip_interval: Duration::from_secs(1),
        gossip_fanout: 3,
    }));

    let config = DistSchedulerConfig {
        strategy: DistScheduleStrategy::RoundRobin,
        local_node_id: "test-node".into(),
        max_retries: 1,
        task_timeout_secs: 30,
        node_timeout_secs: 10,
        batch_size: 10,
        enable_auto_failover: true,
        health_check_interval: Duration::from_secs(5),
    };

    let scheduler = DistScheduler::new(config, cluster);
    scheduler.start().await;

    let task = DistTask::new("e2e-task", "generic", json!({"msg": "hello"}))
        .with_priority(5);
    let result = scheduler.submit_task(task).await;
    assert!(result.is_ok(), "submit task should succeed: {:?}", result.err());

    let schedule = result.unwrap();
    assert!(!schedule.assigned_node_id.is_empty());
    scheduler.stop().await;
}

// ── 3. Autonomy 端到端 ──────────────────────────────

#[tokio::test]
async fn e2e_autonomy_experience_cycle() {
    use lingshu_autonomy::{
        AutonomyEngine, EvolutionConfig, ExperienceEntry, ExperienceOutcome,
        ExperienceSeverity, ExperienceType, ReflectionConfig,
    };
    use lingshu_core::LsId;
    use std::time::Duration;

    let reflection_config = ReflectionConfig {
        confidence_threshold: 0.3,
        failure_pattern_window: 10,
        degradation_window: 20,
        min_experiences_for_reflection: 3,
        auto_mark_analyzed: true,
    };
    let evolution_config = EvolutionConfig {
        auto_apply_threshold: 8,
        verification_wait: Duration::from_secs(10),
        max_concurrent_plans: 5,
        cooldown: Duration::from_secs(0),
        enable_auto_rollback: false,
        rollback_threshold: -0.5,
    };

    let engine = AutonomyEngine::new(reflection_config, evolution_config, 100);
    let agent_id = "autonomy-test-agent";

    for i in 0..5 {
        let entry = ExperienceEntry {
            id: LsId::new(),
            agent_id: agent_id.into(),
            exp_type: if i % 2 == 0 { ExperienceType::TaskExecution } else { ExperienceType::Error },
            severity: if i > 2 { ExperienceSeverity::Error } else { ExperienceSeverity::Warning },
            timestamp: chrono::Utc::now().timestamp_millis(),
            title: format!("experience {i}"),
            description: format!("test execution {i}"),
            context: serde_json::json!({"attempt": i}),
            outcome: if i % 2 == 0 { ExperienceOutcome::Success } else { ExperienceOutcome::Failure(format!("error {i}")) },
            tags: vec!["integration-test".into()],
            related_task_id: None,
            related_agent_ids: vec![],
            duration_ms: 100 + (i * 50) as u64,
            analyzed: false,
        };
        engine.autonomy_cycle(agent_id, entry).await;
    }

    let report = engine.reflect_only(agent_id).await;
    assert_eq!(report.agent_id, agent_id);
    assert!(report.analyzed_count >= 5, "should analyze at least 5");
    assert!(report.health_score >= 0.0);
}

#[tokio::test]
async fn e2e_autonomy_evolution() {
    use lingshu_autonomy::{
        AutonomyEngine, EvolutionConfig, ExperienceEntry, ExperienceOutcome,
        ExperienceSeverity, ExperienceType, ReflectionConfig,
    };
    use lingshu_core::LsId;
    use std::time::Duration;

    let reflection_config = ReflectionConfig {
        confidence_threshold: 0.2,
        failure_pattern_window: 5,
        degradation_window: 10,
        min_experiences_for_reflection: 2,
        auto_mark_analyzed: true,
    };
    let evolution_config = EvolutionConfig {
        auto_apply_threshold: 6,
        verification_wait: Duration::from_secs(1),
        max_concurrent_plans: 5,
        cooldown: Duration::from_secs(0),
        enable_auto_rollback: false,
        rollback_threshold: -0.5,
    };

    let engine = AutonomyEngine::new(reflection_config, evolution_config, 50);
    let agent_id = "evolution-test-agent";

    // 注册 Agent 以便进化
    use lingshu_autonomy::AgentParameters;
    engine.evolution_engine.register_agent(agent_id, AgentParameters::new(agent_id)).await;

    for i in 0..5 {
        let entry = ExperienceEntry {
            id: LsId::new(),
            agent_id: agent_id.into(),
            exp_type: ExperienceType::Error,
            severity: ExperienceSeverity::Critical,
            timestamp: chrono::Utc::now().timestamp_millis(),
            title: format!("failure {i}"),
            description: format!("timeout error {i}"),
            context: serde_json::json!({"error": "timeout", "attempt": i}),
            outcome: ExperienceOutcome::Failure(format!("timeout {i}")),
            tags: vec!["timeout".into(), "critical".into()],
            related_task_id: None,
            related_agent_ids: vec![],
            duration_ms: 5000,
            analyzed: false,
        };
        engine.autonomy_cycle(agent_id, entry).await;
    }

    let report = engine.reflect_only(agent_id).await;
    assert!(report.health_score >= 0.0);
    assert!(report.analyzed_count > 0);
}

// ── 4. 跨组件全链路 E2E ────────────────────────────
//   Swarm(management) + Distributed(scheduler) + Autonomy(experience)

#[tokio::test]
async fn e2e_swarm_autonomy_distributed_full_flow() {
    use lingshu_autonomy::{
        AutonomyEngine, EvolutionConfig, ExperienceEntry, ExperienceOutcome,
        ExperienceSeverity, ExperienceType, ReflectionConfig,
    };
    use lingshu_core::LsId;
    use lingshu_distributed::{
        Cluster, ClusterConfig, DistScheduler, DistSchedulerConfig, DistScheduleStrategy, DistTask,
    };
    use lingshu_swarm::{
        SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine, SwarmStrategy,
        SwarmTopology,
    };
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    // 1. Swarm 引擎（仅管理生命周期，不提交任务）
    let swarm_config = SwarmConfig {
        name: "full-flow-swarm".into(),
        strategy: SwarmStrategy::Consensus,
        topology: SwarmTopology::Mesh,
        min_agents: 1, max_agents: 5,
        task_timeout: Duration::from_secs(15),
        heartbeat_interval: Duration::from_secs(1),
        enable_autonomy: false,
        enable_emergent_specialization: false,
        idle_timeout: Duration::from_secs(60),
        consensus_threshold: 0.5,
        max_retries: 2,
        enable_audit_trail: false,
    };
    let swarm = Arc::new(SwarmEngine::new(swarm_config));
    swarm.start().await.unwrap();
    for i in 0..3 {
        let role = match i { 0 => SwarmAgentRole::Planner, 1 => SwarmAgentRole::Executor, _ => SwarmAgentRole::Validator };
        swarm.add_agent(SwarmAgent::new(format!("full-agent-{i}"), role)).await.unwrap();
    }

    // 2. 分布式调度器
    let cluster = Arc::new(Cluster::new(ClusterConfig {
        node_id: "swarm-node".into(),
        bind_addr: "127.0.0.1:0".into(),
        seed_nodes: vec![],
        heartbeat_interval: Duration::from_secs(1),
        suspicion_mult: 3,
        cleanup_interval: Duration::from_secs(10),
        gossip_interval: Duration::from_secs(1),
        gossip_fanout: 3,
    }));
    let sched_config = DistSchedulerConfig {
        strategy: DistScheduleStrategy::RoundRobin,
        local_node_id: "swarm-node".into(),
        max_retries: 1, task_timeout_secs: 30,
        node_timeout_secs: 10, batch_size: 10,
        enable_auto_failover: true,
        health_check_interval: Duration::from_secs(5),
    };
    let scheduler = Arc::new(DistScheduler::new(sched_config, cluster));
    scheduler.start().await;

    // 3. Autonomy
    let reflection_config = ReflectionConfig {
        confidence_threshold: 0.3, failure_pattern_window: 10,
        degradation_window: 20, min_experiences_for_reflection: 2,
        auto_mark_analyzed: true,
    };
    let evolution_config = EvolutionConfig {
        auto_apply_threshold: 8, verification_wait: Duration::from_secs(10),
        max_concurrent_plans: 5, cooldown: Duration::from_secs(0),
        enable_auto_rollback: false,
        rollback_threshold: -0.5,
    };
    let autonomy = Arc::new(AutonomyEngine::new(reflection_config, evolution_config, 100));

    // 4. 全链路操作
    let agent_id = "full-flow-agent";

    // 4a. 调度器任务
    for i in 0..3 {
        let task = DistTask::new(format!("full-flow-task-{i}"), "generic", json!({"index": i}))
            .with_priority(1);
        let result = scheduler.submit_task(task).await;
        assert!(result.is_ok(), "scheduler task {i} should succeed");
    }

    // 4b. Autonomy 经验
    for i in 0..4 {
        let entry = ExperienceEntry {
            id: LsId::new(),
            agent_id: agent_id.into(),
            exp_type: if i < 3 { ExperienceType::TaskExecution } else { ExperienceType::Error },
            severity: if i == 3 { ExperienceSeverity::Error } else { ExperienceSeverity::Warning },
            timestamp: chrono::Utc::now().timestamp_millis(),
            title: format!("flow exec {i}"),
            description: format!("step {i}"),
            context: json!({"step": i}),
            outcome: if i < 3 { ExperienceOutcome::Success } else { ExperienceOutcome::Failure("error".into()) },
            tags: vec!["full-flow".into()],
            related_task_id: None,
            related_agent_ids: vec![],
            duration_ms: 200 + (i * 30) as u64,
            analyzed: false,
        };
        autonomy.autonomy_cycle(agent_id, entry).await;
    }

    // 4c. 反思验证
    let report = autonomy.reflect_only(agent_id).await;
    assert_eq!(report.agent_id, agent_id);
    assert!(report.analyzed_count >= 4);
    assert!(report.health_score >= 0.0);

    // 5. 验证 Swarm 状态
    let swarm_summary = swarm.summary().await;
    assert_eq!(swarm_summary.agent_count, 3);

    // 6. 清理
    scheduler.stop().await;
    swarm.stop().await;
}
