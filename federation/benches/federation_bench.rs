//! LSFed — 联邦 Criterion 基准测试.
//!
//! 覆盖：
//! - FederationNode 创建与序列化
//! - Capability 创建与查询
//! - FederationConfig 序列化/反序列化
//! - RemoteExecRequest / Response 创建
//! - FederationStats 操作
//! - FederationTopology 操作

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_core::LsId;
use lingshu_federation::{
    Capability, CapabilityType, FederationConfig, FederationNode, FederationStats,
    FederationTopology, RemoteExecRequest, RemoteExecResponse,
};

// ── FederationNode 创建与操作 ─────────────────────

fn bench_federation_node_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("federation::node");

    group.bench_function("create_new", |b| {
        b.iter(|| {
            FederationNode::new(
                LsId::new(),
                black_box("cluster-east"),
                black_box(vec!["10.0.0.1:9550".parse().unwrap()]),
            )
        })
    });

    group.bench_function("create_with_capabilities", |b| {
        b.iter(|| {
            let mut node = FederationNode::new(
                LsId::new(),
                black_box("cluster-west"),
                black_box(vec![
                    "10.0.0.2:9550".parse().unwrap(),
                    "10.0.0.3:9550".parse().unwrap(),
                ]),
            );
            for i in 0..10 {
                let cap = Capability::new(
                    &format!("cap-{i}"),
                    &format!("Capability {i}"),
                    if i % 2 == 0 {
                        CapabilityType::LlmModel
                    } else {
                        CapabilityType::Agent
                    },
                );
                node.capabilities.push(cap);
            }
            black_box(node)
        })
    });

    group.bench_function("has_capability_exists", |b| {
        let mut node = FederationNode::new(
            LsId::new(),
            "bench-cluster",
            vec!["127.0.0.1:9550".parse().unwrap()],
        );
        for i in 0..50 {
            node.capabilities.push(Capability::new(
                &format!("cap-{i}"),
                &format!("Cap {i}"),
                CapabilityType::Custom,
            ));
        }
        b.iter(|| black_box(node.has_capability(black_box("cap-25"))))
    });

    group.bench_function("has_capability_missing", |b| {
        let node = FederationNode::new(
            LsId::new(),
            "bench-cluster",
            vec!["127.0.0.1:9550".parse().unwrap()],
        );
        b.iter(|| black_box(node.has_capability(black_box("no-such-cap"))))
    });

    group.bench_function("is_healthy", |b| {
        let node = FederationNode::new(
            LsId::new(),
            "bench-cluster",
            vec!["127.0.0.1:9550".parse().unwrap()],
        );
        b.iter(|| black_box(node.is_healthy()))
    });

    group.finish();
}

// ── Capability ────────────────────────────────────

fn bench_capability(c: &mut Criterion) {
    let mut group = c.benchmark_group("federation::capability");

    group.bench_function("new_llm", |b| {
        b.iter(|| {
            Capability::new(
                black_box("gpt-4o"),
                black_box("GPT-4o"),
                CapabilityType::LlmModel,
            )
        })
    });

    group.bench_function("new_custom", |b| {
        b.iter(|| {
            let mut cap = Capability::new(
                black_box("my-tool"),
                black_box("My Tool"),
                CapabilityType::Custom,
            );
            cap.description = "A custom tool for benchmarking".into();
            cap.max_rps = 1000;
            cap.config.insert("api_key_required".into(), "true".into());
            black_box(cap)
        })
    });

    group.finish();
}

// ── FederationConfig ─────────────────────────────

fn bench_federation_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("federation::config");

    group.bench_function("default", |b| b.iter(|| FederationConfig::default()));

    group.bench_function("serde_serialize", |b| {
        let config = FederationConfig::default();
        b.iter(|| serde_json::to_string(black_box(&config)))
    });

    group.bench_function("serde_deserialize", |b| {
        let json = serde_json::to_string(&FederationConfig::default()).unwrap();
        b.iter(|| serde_json::from_str::<FederationConfig>(black_box(&json)))
    });

    group.finish();
}

// ── RemoteExecRequest / Response ──────────────────

fn bench_remote_exec_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("federation::remote_exec");

    group.bench_function("request_create", |b| {
        b.iter(|| RemoteExecRequest {
            request_id: LsId::new().to_string(),
            target: black_box("my-agent".into()),
            payload: serde_json::json!({"prompt": "hello world", "max_tokens": 1024}),
            timeout_secs: 30,
            stream: false,
        })
    });

    group.bench_function("request_serde", |b| {
        let req = RemoteExecRequest {
            request_id: "req-123".into(),
            target: "agent-1".into(),
            payload: serde_json::json!({"prompt": "hello"}),
            timeout_secs: 30,
            stream: false,
        };
        b.iter(|| serde_json::to_string(black_box(&req)))
    });

    group.bench_function("response_create", |b| {
        b.iter(|| RemoteExecResponse {
            request_id: "req-123".into(),
            result: serde_json::json!({"output": "executed successfully", "tokens": 150}),
            success: true,
            error: None,
            latency_ms: 42,
        })
    });

    group.finish();
}

// ── FederationStats ─────────────────────────────

fn bench_federation_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("federation::stats");

    group.bench_function("clone", |b| {
        let stats = FederationStats {
            connected_nodes: 10,
            total_nodes: 25,
            total_capabilities: 100,
            total_messages: 10000,
            total_errors: 5,
            active_links: 8,
            p50_latency_ms: 12.5,
            uptime_seconds: 86400,
        };
        b.iter(|| black_box(stats.clone()))
    });

    group.bench_function("serde", |b| {
        let stats = FederationStats {
            connected_nodes: 10,
            total_nodes: 25,
            total_capabilities: 100,
            total_messages: 10000,
            total_errors: 5,
            active_links: 8,
            p50_latency_ms: 12.5,
            uptime_seconds: 86400,
        };
        b.iter(|| {
            let json = serde_json::to_string(black_box(&stats)).unwrap();
            let _: FederationStats = serde_json::from_str(&json).unwrap();
        })
    });

    group.finish();
}

// ── 拓扑操作 ────────────────────────────────────

fn bench_topology(c: &mut Criterion) {
    let mut group = c.benchmark_group("federation::topology");

    group.bench_function("as_str", |b| {
        let topo = FederationTopology::Mesh;
        b.iter(|| black_box(topo.as_str()))
    });

    group.bench_function("serde_roundtrip", |b| {
        let variants = [
            FederationTopology::Mesh,
            FederationTopology::HubSpoke,
            FederationTopology::Partial,
        ];
        b.iter(|| {
            for v in &variants {
                let json = serde_json::to_string(v).unwrap();
                let _: FederationTopology = serde_json::from_str(&json).unwrap();
            }
        })
    });

    group.finish();
}

criterion_group!(
    federation_benches,
    bench_federation_node_create,
    bench_capability,
    bench_federation_config,
    bench_remote_exec_types,
    bench_federation_stats,
    bench_topology,
);
criterion_main!(federation_benches);
