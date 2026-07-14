//! Lingshu 全局 Criterion 基准测试套件 (v5.0)
//!
//! 覆盖所有核心 crate 的关键路径：
//! - Core 基础操作 (ID、Context、Session)
//! - Runtime 会话管理
//! - Evaluator 评估与打分
//! - Cache/Memory 存储
//! - Security 认证
//! - Federation 序列化
//! - v5.0 Swarm / Distributed / Autonomy
//!
//! 运行: `cargo bench` 或 `make bench`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_core::{LsContext, LsId};
use std::time::Duration;

// ════════════════════════════════════════════════════════════
// Core 基础类型
// ════════════════════════════════════════════════════════════

fn bench_id_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("core");
    group.bench_function("uuid_v4", |b| b.iter(LsId::new));
    group.finish();
}

fn bench_context_clone(c: &mut Criterion) {
    let ctx = LsContext::with_session(LsId::new());
    c.bench_function("core/context_clone", |b| b.iter(|| black_box(ctx.clone())));
}

fn bench_context_with_metadata(c: &mut Criterion) {
    c.bench_function("core/context_with_metadata", |b| {
        b.iter(|| {
            LsContext::with_session(LsId::new())
                .with_user("bench-user")
                .with_metadata("key", "value")
        })
    });
}

// ════════════════════════════════════════════════════════════
// JSON 序列化
// ════════════════════════════════════════════════════════════

fn bench_json_ops(c: &mut Criterion) {
    let data = serde_json::json!({
        "id": "test-id-123",
        "name": "benchmark",
        "messages": [
            {"role": "user", "content": "Hello, how are you?"},
            {"role": "assistant", "content": "I'm doing great, thanks for asking!"}
        ],
        "metadata": {
            "session": "session-xyz",
            "model": "gpt-4",
            "tokens": 150
        }
    });
    let mut group = c.benchmark_group("json");
    group.bench_function("serialize_large", |b| {
        b.iter(|| serde_json::to_string(black_box(&data)))
    });
    group.bench_function("deserialize_large", |b| {
        let s = serde_json::to_string(&data).unwrap();
        b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(&s)))
    });
    group.finish();
}

// ════════════════════════════════════════════════════════════
// Evaluator 打分
// ════════════════════════════════════════════════════════════

fn bench_evaluator_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("evaluator");
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("exact_match", |b| {
        let val = serde_json::json!("hello world");
        b.iter(|| lingshu_evaluator::metrics::score_exact(black_box(&val), black_box(&val)))
    });
    group.bench_function("contains_match", |b| {
        let haystack = serde_json::json!("the quick brown fox");
        let needle = serde_json::json!("quick brown");
        b.iter(|| {
            lingshu_evaluator::metrics::score_contains(black_box(&haystack), black_box(&needle))
        })
    });
    group.finish();
}

fn bench_evaluator_metrics(c: &mut Criterion) {
    use lingshu_evaluator::types::EvalCaseResult;
    let results: Vec<EvalCaseResult> = (0..1000)
        .map(|i| EvalCaseResult {
            case_id: format!("c{i}"),
            case_name: format!("test-{i}"),
            passed: i % 3 != 0,
            score: if i % 3 == 0 { 0.2 } else { 0.9 },
            actual_output: serde_json::json!("output"),
            expected_output: Some(serde_json::json!("expected")),
            error: None,
            latency: Duration::from_millis((i as u64) * 10),
            input_tokens: (i as u64) * 5,
            output_tokens: (i as u64) * 3,
            cost: (i as f64) * 0.001,
            details: Default::default(),
        })
        .collect();
    c.bench_function("evaluator/metrics_1000", |b| {
        b.iter(|| lingshu_evaluator::metrics::compute_metrics(black_box(&results)))
    });
}

// ════════════════════════════════════════════════════════════
// Vector 相似度计算
// ════════════════════════════════════════════════════════════

fn bench_vector_cosine(c: &mut Criterion) {
    for &dim in &[128, 512, 1536] {
        let v1: Vec<f32> = (0..dim).map(|i| (i as f32).sin()).collect();
        let v2: Vec<f32> = (0..dim).map(|i| (i as f32).cos()).collect();
        c.bench_function(&format!("vector/cosine_{}", dim), |b| {
            b.iter(|| {
                let dot: f32 = v1.iter().zip(v2.iter()).map(|(x, y)| x * y).sum();
                let n1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
                let n2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();
                black_box(dot / (n1 * n2 + 1e-10))
            })
        });
    }
}

// ════════════════════════════════════════════════════════════
// v5.0 — Swarm
// ════════════════════════════════════════════════════════════

fn bench_swarm_create_engine(c: &mut Criterion) {
    use lingshu_swarm::{SwarmConfig, SwarmEngine, SwarmStrategy, SwarmTopology};

    let config = SwarmConfig {
        name: "bench-swarm".into(),
        strategy: SwarmStrategy::Democratic,
        topology: SwarmTopology::Mesh,
        min_agents: 1,
        max_agents: 100,
        task_timeout: Duration::from_secs(30),
        heartbeat_interval: Duration::from_secs(5),
        enable_autonomy: false,
        enable_emergent_specialization: false,
        idle_timeout: Duration::from_secs(300),
        consensus_threshold: 0.6,
        max_retries: 3,
        enable_audit_trail: false,
    };

    c.bench_function("swarm/create_engine", |b| {
        b.iter(|| SwarmEngine::new(config.clone()))
    });
}

fn bench_swarm_full_lifecycle_sync(c: &mut Criterion) {
    use lingshu_swarm::{
        SwarmAgent, SwarmAgentRole, SwarmConfig, SwarmEngine, SwarmStrategy, SwarmTopology,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let config = SwarmConfig {
        name: "bench-swarm".into(),
        strategy: SwarmStrategy::Democratic,
        topology: SwarmTopology::Mesh,
        min_agents: 1,
        max_agents: 50,
        task_timeout: Duration::from_secs(30),
        heartbeat_interval: Duration::from_secs(5),
        enable_autonomy: false,
        enable_emergent_specialization: false,
        idle_timeout: Duration::from_secs(300),
        consensus_threshold: 0.6,
        max_retries: 3,
        enable_audit_trail: false,
    };

    c.bench_function("swarm/start_add_stop", |b| {
        b.iter(|| {
            rt.block_on(async {
                let engine = SwarmEngine::new(config.clone());
                engine.start().await.unwrap();
                let agent = SwarmAgent::new("bench-agent", SwarmAgentRole::Executor);
                engine.add_agent(agent).await.unwrap();
                let _ = engine.state().await;
                engine.stop().await;
            })
        })
    });
}

// ════════════════════════════════════════════════════════════
// v5.0 — Distributed
// ════════════════════════════════════════════════════════════

fn bench_distributed_scheduler_sync(c: &mut Criterion) {
    use lingshu_distributed::{
        Cluster, ClusterConfig, DistScheduleStrategy, DistScheduler, DistSchedulerConfig, DistTask,
    };
    use serde_json::json;
    use std::sync::Arc;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let cluster = Arc::new(Cluster::new(ClusterConfig {
        node_id: "bench-node".into(),
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
        local_node_id: "bench-node".into(),
        max_retries: 3,
        task_timeout_secs: 60,
        node_timeout_secs: 10,
        batch_size: 10,
        enable_auto_failover: true,
        health_check_interval: Duration::from_secs(5),
    };

    let scheduler = Arc::new(DistScheduler::new(config, cluster));
    rt.block_on(async {
        scheduler.start().await;
    });

    c.bench_function("distributed/submit_task", |b| {
        b.iter(|| {
            rt.block_on(async {
                let task = DistTask::new("bench-task", "generic", json!({"msg": "bench"}));
                let _ = scheduler.submit_task(task).await;
            })
        })
    });

    rt.block_on(async {
        scheduler.stop().await;
    });
}

// ════════════════════════════════════════════════════════════
// v5.0 — Autonomy
// ════════════════════════════════════════════════════════════

fn bench_autonomy_reflection_sync(c: &mut Criterion) {
    use lingshu_autonomy::{
        AutonomyEngine, EvolutionConfig, ExperienceEntry, ExperienceOutcome, ExperienceType,
        ReflectionConfig,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let evolution_config = EvolutionConfig {
        auto_apply_threshold: 8,
        verification_wait: Duration::from_secs(10),
        max_concurrent_plans: 5,
        cooldown: Duration::from_secs(0),
        enable_auto_rollback: false,
        rollback_threshold: -0.5,
    };
    let engine = AutonomyEngine::new(ReflectionConfig::default(), evolution_config, 500);
    let agent_id = "bench-agent";

    // Pre-load 100 experiences
    rt.block_on(async {
        for i in 0..100 {
            let entry = ExperienceEntry::new(
                agent_id,
                if i % 2 == 0 {
                    ExperienceType::TaskExecution
                } else {
                    ExperienceType::Error
                },
                format!("bench-exp-{i}"),
                format!("benchmark"),
                if i % 3 == 0 {
                    ExperienceOutcome::Success
                } else {
                    ExperienceOutcome::Failure("err".into())
                },
            );
            engine.autonomy_cycle(agent_id, entry).await;
        }
    });

    c.bench_function("autonomy/reflect_100_experiences", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = engine.reflect_only(agent_id).await;
            })
        })
    });
}

// ════════════════════════════════════════════════════════════
// Security 认证
// ════════════════════════════════════════════════════════════

fn bench_security_sync(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let auth = lingshu_security::JwtService::new("test-secret-key-here", 3600);
    rt.block_on(async {
        let _ = auth.generate_token("user-1", None);
    });

    c.bench_function("security/create_token", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = auth.generate_token("user-1", None);
            })
        })
    });
}

// ════════════════════════════════════════════════════════════
// 注册所有基准测试
// ════════════════════════════════════════════════════════════

criterion_group!(
    benches,
    bench_id_generation,
    bench_context_clone,
    bench_context_with_metadata,
    bench_json_ops,
    bench_evaluator_scoring,
    bench_evaluator_metrics,
    bench_vector_cosine,
    bench_security_sync,
);

criterion_group!(
    v5_benches,
    bench_swarm_create_engine,
    bench_swarm_full_lifecycle_sync,
    bench_distributed_scheduler_sync,
    bench_autonomy_reflection_sync,
);

criterion_main!(benches, v5_benches);
