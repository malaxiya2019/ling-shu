//! LSDistributed — Criterion 基准测试.
//!
//! 覆盖：
//! - DistScheduleStrategy / NodeRole / NodeStatus 基本操作
//! - ClusterState 节点管理操作
//! - DistScheduler select_node 性能（多策略）
//! - DistTask 创建
//! - DistributedCache 读写
//! - DistributedStore 读写

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_distributed::cache::{CacheConfig, DistributedCache};
use lingshu_distributed::cluster::*;
use lingshu_distributed::scheduler::*;
use lingshu_distributed::store::{DistributedStore, StoreConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

// ── 枚举类型操作 ───────────────────────────────────

fn bench_enum_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("distributed::enum");

    group.bench_function("strategy_as_str", |b| {
        let variants = [
            DistScheduleStrategy::LeastTasks, DistScheduleStrategy::RoundRobin,
            DistScheduleStrategy::Weighted, DistScheduleStrategy::ConsistentHash,
            DistScheduleStrategy::LocalFirst, DistScheduleStrategy::Adaptive,
        ];
        b.iter(|| { for v in &variants { black_box(v.as_str()); } })
    });

    group.bench_function("cluster_node_serde", |b| {
        let node = ClusterNode {
            id: "node-42".into(), addr: "10.0.0.42:9550".into(),
            role: NodeRole::Follower, status: NodeStatus::Alive,
            last_heartbeat: 1000, version: 5,
        };
        b.iter(|| {
            let json = serde_json::to_string(black_box(&node)).unwrap();
            let _: ClusterNode = serde_json::from_str(&json).unwrap();
        })
    });

    group.finish();
}

fn make_cluster_config() -> ClusterConfig {
    ClusterConfig {
        node_id: "local".into(),
        bind_addr: "0.0.0.0:9550".into(),
        seed_nodes: vec!["10.0.0.1:9550".into()],
        heartbeat_interval: Duration::from_secs(1),
        suspicion_mult: 3,
        cleanup_interval: Duration::from_secs(60),
        gossip_interval: Duration::from_secs(1),
        gossip_fanout: 3,
    }
}

fn make_cluster_config_empty() -> ClusterConfig {
    ClusterConfig {
        node_id: "local".into(),
        bind_addr: "0.0.0.0:9550".into(),
        seed_nodes: vec![],
        heartbeat_interval: Duration::from_secs(1),
        suspicion_mult: 3,
        cleanup_interval: Duration::from_secs(60),
        gossip_interval: Duration::from_secs(1),
        gossip_fanout: 3,
    }
}

// ── ClusterState 操作 ──────────────────────────────

fn bench_cluster_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("distributed::cluster");

    group.bench_function("new_state", |b| {
        b.iter(|| {
            ClusterState::new(make_cluster_config())
        })
    });

    group.bench_function("add_100_members", |b| {
        b.iter(|| {
            let mut state = ClusterState::new(make_cluster_config_empty());
            for i in 0..100 {
                state.add_member(ClusterNode {
                    id: format!("node-{i}"), addr: format!("10.0.0.{i}:9550"),
                    role: NodeRole::Follower, status: NodeStatus::Alive,
                    last_heartbeat: i as i64, version: i as u64,
                });
            }
            black_box(state)
        })
    });

    group.bench_function("live_members_50", |b| {
        let mut state = ClusterState::new(make_cluster_config_empty());
        for i in 0..50 {
            state.add_member(ClusterNode {
                id: format!("node-{i}"), addr: format!("10.0.0.{i}:9550"),
                role: if i == 0 { NodeRole::Leader } else { NodeRole::Follower },
                status: if i % 10 == 0 { NodeStatus::Suspect } else { NodeStatus::Alive },
                last_heartbeat: i as i64, version: i as u64,
            });
        }
        b.iter(|| black_box(state.live_members()))
    });

    group.finish();
}

// ── DistTask 创建 ──────────────────────────────────

fn bench_task_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("distributed::task");

    group.bench_function("new_basic", |b| {
        b.iter(|| {
            DistTask::new(
                black_box("bench-task"),
                "general",
                black_box(serde_json::json!({"cmd": "echo"})),
            )
        })
    });

    group.bench_function("new_with_options", |b| {
        b.iter(|| {
            DistTask::new("optimized-task", "compute", serde_json::json!({"data": "x".repeat(500)}))
                .with_priority(8)
                .with_capability("gpu")
                .with_affinity("node-fast")
        })
    });

    group.finish();
}

// ── DistScheduler select_node ──────────────────────

fn bench_scheduler_select(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("distributed::scheduler");

    // Build a shared cluster state with 50 members
    let cluster = Arc::new(Cluster::new(make_cluster_config_empty()));
    {
        let mut state = rt.block_on(async { cluster.state().write().await });
        for i in 0..50 {
            state.add_member(ClusterNode {
                id: format!("node-{i}"), addr: format!("10.0.0.{i}:9550"),
                role: NodeRole::Follower, status: NodeStatus::Alive,
                last_heartbeat: i as i64, version: i as u64,
            });
        }
    }

    let task = DistTask::new("select-task", "general", serde_json::json!({}));

    for strategy in [
        DistScheduleStrategy::LeastTasks,
        DistScheduleStrategy::RoundRobin,
        DistScheduleStrategy::Weighted,
        DistScheduleStrategy::ConsistentHash,
        DistScheduleStrategy::LocalFirst,
        DistScheduleStrategy::Adaptive,
    ] {
        let config = DistSchedulerConfig {
            strategy,
            local_node_id: "local".into(),
            max_retries: 3,
            task_timeout_secs: 30,
            node_timeout_secs: 30,
            batch_size: 10,
            enable_auto_failover: true,
            health_check_interval: Duration::from_secs(5),
        };
        let scheduler = DistScheduler::new(config, cluster.clone());
        let name = strategy.as_str();

        group.bench_function(format!("select_{name}_50nodes"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    black_box(scheduler.select_node(black_box(&task)).await)
                })
            })
        });
    }

    group.finish();
}

// ── DistributedCache ──────────────────────────────

fn bench_cache(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("distributed::cache");

    group.bench_function("set_1000", |b| {
        let cache = DistributedCache::new(CacheConfig::default());
        b.iter(|| {
            rt.block_on(async {
                for i in 0..100 {
                    cache.set(&format!("key-{i}"), &format!("value-{i}"), None).await;
                }
            })
        })
    });

    group.bench_function("get_hit", |b| {
        let cache = DistributedCache::new(CacheConfig::default());
        rt.block_on(async { cache.set("hit-key", "hit-value", None).await; });
        b.iter(|| rt.block_on(async { black_box(cache.get("hit-key").await) }))
    });

    group.bench_function("get_miss", |b| {
        let cache = DistributedCache::new(CacheConfig::default());
        b.iter(|| rt.block_on(async { black_box(cache.get("no-such-key").await) }))
    });

    group.finish();
}

// ── DistributedStore ───────────────────────────────

fn bench_store(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("distributed::store");

    group.bench_function("set_read_roundtrip", |b| {
        let store = DistributedStore::new(StoreConfig::default());
        let value = b"benchmark-value-data-1234567890";
        b.iter(|| {
            rt.block_on(async {
                store.set("bench-key", black_box(value)).await;
                black_box(store.get("bench-key").await)
            })
        })
    });

    group.bench_function("set_batch_50", |b| {
        let store = DistributedStore::new(StoreConfig::default());
        let data = b"batch-data";
        b.iter(|| {
            rt.block_on(async {
                for i in 0..50 {
                    store.set(&format!("k-{i}"), black_box(data)).await;
                }
            })
        })
    });

    group.bench_function("exists", |b| {
        let store = DistributedStore::new(StoreConfig::default());
        rt.block_on(async { store.set("exists-key", b"yes").await; });
        b.iter(|| rt.block_on(async { black_box(store.exists("exists-key").await) }))
    });

    group.finish();
}

criterion_group!(
    distributed_benches,
    bench_enum_ops,
    bench_cluster_ops,
    bench_task_create,
    bench_scheduler_select,
    bench_cache,
    bench_store,
);
criterion_main!(distributed_benches);
