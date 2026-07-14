//! AgentSwarm — Criterion 基准测试.
//!
//! 覆盖：
//! - SwarmStrategy / SwarmTopology / SwarmAgentRole 基本操作
//! - SwarmAgent 创建与配置
//! - SwarmTask 创建
//! - 协作策略决策 (select_agent)
//! - SwarmMemory 结果记录
//! - MetricsCollector 指标操作
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_core::LsId;
use lingshu_swarm::memory::SwarmMemory;
use lingshu_swarm::metrics::MetricsCollector;
use lingshu_swarm::strategy::*;
use lingshu_swarm::topology::TopologyManager;
use lingshu_swarm::types::*;
use tokio::runtime::Runtime;
// ── 枚举类型操作 ───────────────────────────────────
fn bench_enum_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("swarm::enum");
    group.bench_function("strategy_as_str", |b| {
        let variants = [
            SwarmStrategy::Voting,
            SwarmStrategy::Consensus,
            SwarmStrategy::Hierarchical,
            SwarmStrategy::Democratic,
            SwarmStrategy::Bidding,
            SwarmStrategy::Hybrid,
        ];
        b.iter(|| {
            for v in &variants {
                black_box(v.as_str());
            }
        })
    });
    group.bench_function("topology_as_str", |b| {
        let variants = [
            SwarmTopology::Star,
            SwarmTopology::Mesh,
            SwarmTopology::Ring,
            SwarmTopology::Tree,
            SwarmTopology::Dynamic,
        ];
        b.iter(|| {
            for v in &variants {
                black_box(v.as_str());
            }
        })
    });
    group.bench_function("role_as_str", |b| {
        let roles = [
            SwarmAgentRole::Analyst,
            SwarmAgentRole::Creator,
            SwarmAgentRole::Validator,
            SwarmAgentRole::Negotiator,
            SwarmAgentRole::Planner,
            SwarmAgentRole::Executor,
            SwarmAgentRole::Tester,
            SwarmAgentRole::Observer,
            SwarmAgentRole::Aggregator,
            SwarmAgentRole::Router,
        ];
        b.iter(|| {
            for r in &roles {
                black_box(r.as_str());
            }
        })
    });
    group.bench_function("serde_roundtrip", |b| {
        let config = SwarmConfig {
            name: "bench".into(),
            strategy: SwarmStrategy::Bidding,
            topology: SwarmTopology::Mesh,
            ..SwarmConfig::default()
        };
        b.iter(|| {
            let json = serde_json::to_string(black_box(&config)).unwrap();
            let _: SwarmConfig = serde_json::from_str(&json).unwrap();
        })
    });
    group.finish();
}
// ── SwarmAgent 创建 ────────────────────────────────
fn bench_agent_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("swarm::agent");
    group.bench_function("new_basic", |b| {
        b.iter(|| SwarmAgent::new(black_box("agent-x"), SwarmAgentRole::Executor))
    });
    group.bench_function("new_with_expertise", |b| {
        b.iter(|| {
            SwarmAgent::new(black_box("expert-a"), SwarmAgentRole::Analyst)
                .with_expertise("code", 0.95)
                .with_expertise("math", 0.85)
                .with_expertise("reasoning", 0.90)
                .with_alternative_role(SwarmAgentRole::Validator)
        })
    });
    group.bench_function("is_available", |b| {
        let agent = SwarmAgent::new("idle-agent", SwarmAgentRole::Executor);
        b.iter(|| black_box(agent.is_available()))
    });
    group.finish();
}
// ── SwarmTask 创建 ─────────────────────────────────
fn bench_task_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("swarm::task");
    group.bench_function("new_simple", |b| {
        b.iter(|| {
            SwarmTask::new(
                black_box("task-1"),
                black_box("Analyze data"),
                black_box(serde_json::json!({"input": "bench"})),
            )
        })
    });
    group.bench_function("new_with_options", |b| {
        b.iter(|| {
            SwarmTask::new(
                "opt-task",
                "Complex analysis",
                serde_json::json!({"data": "x".repeat(200)}),
            )
            .with_required_role(SwarmAgentRole::Analyst)
            .with_priority(8)
        })
    });
    group.finish();
}
// ── 协作策略决策 ────────────────────────────────────
fn bench_decision(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("swarm::decision");
    let task = SwarmTask::new("dec-task", "Pick best agent", serde_json::json!({}))
        .with_required_role(SwarmAgentRole::Executor);
    let agents: Vec<SwarmAgent> = (0..20)
        .map(|i| {
            let role = if i == 0 {
                SwarmAgentRole::Planner
            } else {
                SwarmAgentRole::Executor
            };
            SwarmAgent::new(format!("agent-{i}"), role)
                .with_expertise("default", 0.5 + i as f64 * 0.025)
        })
        .collect();
    let bids: Vec<SwarmBid> = agents
        .iter()
        .map(|a| SwarmBid {
            agent_id: LsId::new(),
            agent_name: a.name.clone(),
            bid_score: 0.5 + (a.name.len() as f64 * 0.02).min(0.45),
            estimated_ms: 100 + a.name.len() as u64 * 10,
            rationale: "benchmark bid".into(),
            timestamp: chrono::Utc::now().timestamp(),
        })
        .collect();
    let voting = VotingStrategy::new(0.5);
    let consensus = ConsensusStrategy::new(0.6);
    let bidding = BiddingStrategy;
    // Import trait to call methods
    {
        use lingshu_swarm::SwarmDecisionStrategy;
        group.bench_function("voting_select_20agents", |b| {
            b.iter(|| {
                rt.block_on(async {
                    black_box(voting.select_agent(&task, &agents, &bids).await.unwrap())
                })
            })
        });
        group.bench_function("consensus_select_20agents", |b| {
            b.iter(|| {
                rt.block_on(async {
                    black_box(consensus.select_agent(&task, &agents, &bids).await.unwrap())
                })
            })
        });
        group.bench_function("bidding_select_20agents", |b| {
            b.iter(|| {
                rt.block_on(async {
                    black_box(bidding.select_agent(&task, &agents, &bids).await.unwrap())
                })
            })
        });
    }
    group.finish();
}
// ── TopologyManager ────────────────────────────────
fn bench_topology(c: &mut Criterion) {
    let mut group = c.benchmark_group("swarm::topology");
    group.bench_function("new_star", |b| {
        b.iter(|| TopologyManager::new(SwarmTopology::Star))
    });
    group.bench_function("new_mesh", |b| {
        b.iter(|| TopologyManager::new(SwarmTopology::Mesh))
    });
    group.bench_function("new_adaptive", |b| {
        b.iter(|| TopologyManager::new(SwarmTopology::Dynamic))
    });
    group.finish();
}
// ── SwarmMemory ────────────────────────────────────
fn bench_memory(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("swarm::memory");
    group.bench_function("record_single_result", |b| {
        let memory = SwarmMemory::new();
        let task = SwarmTask::new("mem-task", "test", serde_json::json!({}));
        let result = SwarmTaskResult {
            task_id: task.id,
            agent_id: LsId::new(),
            agent_name: "agent-a".into(),
            output: serde_json::json!({"status": "ok"}),
            success: true,
            execution_ms: 150,
            confidence: 0.95,
            error: None,
            started_at: 1000,
            completed_at: 1150,
        };
        b.iter(|| {
            rt.block_on(async {
                memory
                    .record_result(black_box(&task), black_box(&result))
                    .await;
            })
        })
    });
    group.finish();
}
// ── MetricsCollector ──────────────────────────────
fn bench_metrics(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("swarm::metrics");
    group.bench_function("record_and_summary", |b| {
        let collector = MetricsCollector::new(1000, 60);
        b.iter(|| {
            rt.block_on(async {
                for i in 0..100 {
                    collector.record_execution(true, i as f64).await;
                }
                let state = SwarmState::new(
                    "bench-swarm",
                    SwarmStrategy::Democratic,
                    SwarmTopology::Mesh,
                );
                black_box(collector.metrics(LsId::new(), &state).await)
            })
        })
    });
    group.finish();
}
criterion_group!(
    swarm_benches,
    bench_enum_ops,
    bench_agent_create,
    bench_task_create,
    bench_decision,
    bench_topology,
    bench_memory,
    bench_metrics,
);
criterion_main!(swarm_benches);
