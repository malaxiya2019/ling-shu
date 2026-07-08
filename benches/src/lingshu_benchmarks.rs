//! Lingshu 全局 Criterion 基准测试套件
//!
//! 覆盖所有核心 crate 的关键路径.
//! 运行: `cargo bench` 或 `make bench`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;

fn bench_id_generation(c: &mut Criterion) {
    c.bench_function("core/uuid_v4", |b| {
        b.iter(|| lingshu_core::LsId::new())
    });
}

fn bench_session(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("runtime/session_create", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = lingshu_core::LsContext::default();
            black_box(ctx)
        })
    });
}

fn bench_evaluator_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("evaluator/scoring");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("exact_match", |b| {
        let a = serde_json::json!("hello world");
        let b = serde_json::json!("hello world");
        b.iter(|| lingshu_evaluator::metrics::score_exact(black_box(&a), black_box(&b)))
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

fn bench_cache_memory(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let cache = lingshu_cache::MemoryCache::new(10_000);
    rt.block_on(async {
        for i in 0..1000 {
            let _ = cache.set(&format!("key-{i}"), &serde_json::json!({"value": i}), None).await;
        }
    });
    let mut group = c.benchmark_group("cache/memory");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("get_hit", |b| {
        b.to_async(&rt).iter(|| async { let _ = cache.get::<serde_json::Value>(black_box("key-42")).await; })
    });
    group.finish();
}

fn bench_audit_append(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let audit_log = lingshu_audit::AuditLog::new();
    c.bench_function("audit/append_100", |b| {
        b.to_async(&rt).iter(|| async {
            for _ in 0..100 {
                let entry = lingshu_audit::AuditEntry::new(
                    lingshu_audit::AuditEventType::ApiCall, "bench", "r", "id", None,
                );
                let _ = audit_log.append(entry).await;
            }
        })
    });
}

fn bench_federation_serialization(c: &mut Criterion) {
    use lingshu_federation::message::FederationMessage;
    let msg = FederationMessage::RemoteExec {
        source_node: "node-a".into(), target_node: "node-b".into(),
        execution_id: "exec-123".into(), agent_type: "react-agent".into(),
        payload: serde_json::json!({"task": "analyze", "tools": ["search"]}),
        ttl: 60, timeout_ms: 30_000,
    };
    c.bench_function("federation/serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&msg)))
    });
}

fn bench_tenant_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let manager = lingshu_tenant::TenantManager::new();
    rt.block_on(async {
        for i in 0..50 { let _ = manager.create_organization(&format!("Org-{i}"), "u", "a").await; }
    });
    c.bench_function("tenant/list_orgs_50", |b| {
        b.to_async(&rt).iter(|| async { let _ = manager.list_organizations().await; })
    });
}

fn bench_vault_mock(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let vault = lingshu_vault::MockVaultClient::new();
    rt.block_on(async {
        for i in 0..100 {
            let mut data = serde_json::Map::new();
            data.insert("key".into(), serde_json::json!(i));
            let _ = vault.write_secret(&format!("secret-{i}"), data).await;
        }
    });
    c.bench_function("vault/mock_read_100", |b| {
        b.to_async(&rt).iter(|| async {
            for i in 0..100 { let _ = vault.read_secret(&format!("secret-{i}")).await; }
        })
    });
}

fn bench_tee_encrypted_memory(c: &mut Criterion) {
    let mem = lingshu_tee::EncryptedMemoryRegion::new();
    for i in 0..500 { let d = format!("data-{i}"); let _ = mem.store(&d, b"sensitive"); }
    c.bench_function("tee/encrypted_memory_read", |b| {
        b.iter(|| { let _ = mem.retrieve("data-42"); })
    });
}

criterion_group!(
    benches,
    bench_id_generation,
    bench_session,
    bench_evaluator_scoring,
    bench_evaluator_metrics,
    bench_cache_memory,
    bench_audit_append,
    bench_federation_serialization,
    bench_tenant_operations,
    bench_vault_mock,
    bench_tee_encrypted_memory,
);
criterion_main!(benches);
