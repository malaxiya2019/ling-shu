//! Lingshu 全局 Criterion 基准测试套件
//!
//! 覆盖所有核心 crate 的关键路径，包括:
//! - Core 基础操作
//! - Runtime 会话管理
//! - Evaluator 评估与打分
//! - Cache 缓存操作
//! - Audit 审计日志
//! - Federation 联邦消息
//! - Tenant 多租户
//! - Vault 密钥管理
//! - TEE 加密内存
//! - Memory 内存存储
//! - Security 安全认证
//! - Database 数据库操作
//!
//! 运行: `cargo bench` 或 `make bench`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_core::{LsContext, LsId};
use std::time::Duration;

// ── bench: Core ID 生成 ────────────────────────────

fn bench_id_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("core");
    group.bench_function("uuid_v4", |b| {
        b.iter(|| LsId::new())
    });
    group.finish();
}

// ── bench: Session 创建 ────────────────────────────

fn bench_session(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("runtime/session_create", |b| {
        b.to_async(&rt).iter(|| async {
            let ctx = LsContext::with_session(LsId::new());
            black_box(ctx)
        })
    });
}

// ── bench: Context 克隆 ────────────────────────────

fn bench_context_clone(c: &mut Criterion) {
    let ctx = LsContext::with_session(LsId::new());
    c.bench_function("core/context_clone", |b| {
        b.iter(|| black_box(ctx.clone()))
    });
}

// ── bench: Evaluator 打分 ──────────────────────────

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
        b.iter(|| lingshu_evaluator::metrics::score_contains(black_box(&haystack), black_box(&needle)))
    });
    group.finish();
}

// ── bench: Evaluator 批量指标 ──────────────────────

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

// ── bench: Cache 操作 ──────────────────────────────

fn bench_cache_memory(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let cache = lingshu_cache::CacheLayer::in_memory();
    rt.block_on(async {
        for i in 0..1000 {
            let _ = cache.set(&format!("key-{i}"), &serde_json::json!({"value": i}), 3600).await;
        }
    });
    let mut group = c.benchmark_group("cache");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("get_hit", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = cache.get::<serde_json::Value>(black_box("key-42")).await;
        })
    });
    group.bench_function("get_miss", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = cache.get::<serde_json::Value>(black_box("nonexistent")).await;
        })
    });
    group.bench_function("set_sequential", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = cache.set("bench-key", &serde_json::json!({"val": 1}), 3600).await;
        })
    });
    group.finish();
}

// ── bench: Audit 追加 ──────────────────────────────

fn bench_audit_append(c: &mut Criterion) {
    use lingshu_audit::AuditLogStore;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let audit_log = lingshu_audit::AuditLog::new();
    c.bench_function("audit/append_100", |b| {
        b.to_async(&rt).iter(|| async {
            for _ in 0..100 {
                let entry = lingshu_audit::AuditEntry::new(
                    lingshu_audit::AuditEventType::ApiCall,
                    "bench",
                    "bench-user",
                    "resource",
                    "id-123",
                    "benchmark entry",
                );
                let _ = audit_log.append(entry).await;
            }
        })
    });
}

// ── bench: Federation 序列化 ───────────────────────

fn bench_federation_serialization(c: &mut Criterion) {
    use lingshu_federation::protocol::FederationMessage;
    use lingshu_federation::protocol::StateReplicatePayload;
    use lingshu_federation::types::RemoteExecRequest;

    let exec_req = RemoteExecRequest {
        request_id: "req-123".into(),
        target: "react-agent".into(),
        payload: serde_json::json!({"task": "analyze", "tools": ["search"]}),
        timeout_secs: 30,
        stream: false,
    };

    let state_repl = StateReplicatePayload {
        key: "session-456".into(),
        value: serde_json::json!({"memory": []}),
        version: 1,
        namespace: "lingshu".into(),
    };

    let mut group = c.benchmark_group("federation");
    group.bench_function("serialize_remote_exec", |b| {
        let msg = FederationMessage::RemoteExecRequest(exec_req.clone());
        b.iter(|| serde_json::to_string(black_box(&msg)))
    });
    group.bench_function("serialize_state_sync", |b| {
        let msg = FederationMessage::StateReplicate(state_repl.clone());
        b.iter(|| serde_json::to_string(black_box(&msg)))
    });
    group.finish();
}

// ── bench: Tenant 操作 ─────────────────────────────

fn bench_tenant_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let manager = lingshu_tenant::TenantManager::new();
    rt.block_on(async {
        for i in 0..50 {
            let _ = manager.create_organization(&format!("Org-{i}"), "u", "a").await;
        }
    });
    c.bench_function("tenant/list_orgs_50", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = manager.list_organizations().await;
        })
    });
}

// ── bench: Vault 操作 ──────────────────────────────

fn bench_vault_mock(c: &mut Criterion) {
    use lingshu_vault::VaultClientTrait;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let vault = lingshu_vault::MockVaultClient::new();
    rt.block_on(async {
        for i in 0..100 {
            let mut data = serde_json::Map::new();
            data.insert("key".into(), serde_json::json!(i));
            let _ = vault.write_secret(&format!("secret-{i}"), data).await;
        }
    });
    let mut group = c.benchmark_group("vault");
    group.bench_function("mock_read_100", |b| {
        b.to_async(&rt).iter(|| async {
            for i in 0..100 {
                let _ = vault.read_secret(&format!("secret-{i}")).await;
            }
        })
    });
    group.bench_function("mock_write", |b| {
        let mut data = serde_json::Map::new();
        data.insert("key".into(), serde_json::json!("val"));
        b.to_async(&rt).iter(|| async {
            let _ = vault.write_secret("bench-key", data.clone()).await;
        })
    });
    group.finish();
}

// ── bench: TEE 加密内存 ────────────────────────────

fn bench_tee_encrypted_memory(c: &mut Criterion) {
    let mem = lingshu_tee::EncryptedMemoryRegion::new();
    for i in 0..500 {
        let d = format!("data-{i}");
        let _ = mem.store(&d, b"sensitive-data");
    }
    let mut group = c.benchmark_group("tee");
    group.bench_function("encrypted_memory_read", |b| {
        b.iter(|| { let _ = mem.retrieve("data-42"); })
    });
    group.bench_function("encrypted_memory_write", |b| {
        b.iter(|| { let _ = mem.store("data-new", b"new-data"); })
    });
    group.finish();
}

// ── bench: Memory 存储 ─────────────────────────────

fn bench_memory_operations(c: &mut Criterion) {
    use lingshu_memory::{DefaultMemory as MemoryStore, Memory};
    use lingshu_memory::types::{MemoryConfig, MemoryItem, MemoryQuery};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let ctx = LsContext::with_session(LsId::new());
    let mem = MemoryStore::new("bench-session", MemoryConfig::default());

    rt.block_on(async {
        for i in 0..100 {
            let item = MemoryItem::new("session", "user", &format!("memory content {i}"));
            let _ = mem.store(&ctx, item).await;
        }
    });

    let mut group = c.benchmark_group("memory");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("store_single", |b| {
        b.to_async(&rt).iter(|| async {
            let item = MemoryItem::new("bench-session", "user", "bench");
            let _ = mem.store(&ctx, item).await;
        })
    });
    group.bench_function("recall_session", |b| {
        let query = MemoryQuery::default();
        b.to_async(&rt).iter(|| async {
            let _ = mem.recall(&ctx, &query).await;
        })
    });
    group.finish();
}

// ── bench: Security 认证 ───────────────────────────

fn bench_security_token(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let auth = lingshu_security::JwtService::new("test-secret-key-here", 3600);
    rt.block_on(async {
        let _ = auth.generate_token("user-1", None);
    });
    c.bench_function("security/create_token", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = auth.generate_token("user-1", None);
        })
    });
}

// ── bench: Database 基础操作 ───────────────────────

fn bench_database_connection(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("database/init_sqlite", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = lingshu_database::SqliteDatabase::in_memory();
        })
    });
}

// ── bench: Vector 相似度计算 ───────────────────────

fn bench_vector_cosine(c: &mut Criterion) {
    let dims = [128, 512, 1536];
    for &dim in &dims {
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

// ── bench: JSON 序列化 ─────────────────────────────

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

// ── 注册所有基准测试 ───────────────────────────────

criterion_group!(
    benches,
    bench_id_generation,
    bench_context_clone,
    bench_session,
    bench_evaluator_scoring,
    bench_evaluator_metrics,
    bench_cache_memory,
    bench_audit_append,
    bench_federation_serialization,
    bench_tenant_operations,
    bench_vault_mock,
    bench_tee_encrypted_memory,
    bench_memory_operations,
    bench_security_token,
    bench_database_connection,
    bench_vector_cosine,
    bench_json_ops,
);
criterion_main!(benches);
