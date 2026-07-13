//! LSAutonomy — Criterion 基准测试.
//!
//! 覆盖：
//! - ExperienceType / ExperienceSeverity 基本操作
//! - ExperienceEntry 创建与序列化
//! - ExperienceStore 存储与查询
//! - ReflectionEngine 模式检测
//! - EvolutionEngine 进化计划生成

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_autonomy::evolution::{EvolutionAction, EvolutionConfig, EvolutionEngine};
use lingshu_autonomy::experience::{ExperienceEntry, ExperienceOutcome, ExperienceSeverity, ExperienceStore, ExperienceType};
use lingshu_autonomy::reflection::{ReflectionConfig, ReflectionEngine};
use std::sync::Arc;
use tokio::runtime::Runtime;

// ── 枚举类型操作 ───────────────────────────────────

fn bench_enum_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("autonomy::enum");

    group.bench_function("experience_type_as_str", |b| {
        let types = [
            ExperienceType::TaskExecution, ExperienceType::Decision,
            ExperienceType::Conversation, ExperienceType::Error,
            ExperienceType::Performance, ExperienceType::Feedback,
            ExperienceType::Collaboration,
        ];
        b.iter(|| { for t in &types { black_box(t.as_str()); } })
    });

    group.bench_function("severity_as_str", |b| {
        let sevs = [
            ExperienceSeverity::Info, ExperienceSeverity::Notice,
            ExperienceSeverity::Warning, ExperienceSeverity::Error,
            ExperienceSeverity::Critical,
        ];
        b.iter(|| { for s in &sevs { black_box(s.as_str()); } })
    });

    group.bench_function("entry_serde", |b| {
        let entry = ExperienceEntry::new(
            "agent-a", ExperienceType::TaskExecution,
            "Task completed successfully", "description", ExperienceOutcome::Success,
        ).with_severity(ExperienceSeverity::Info)
         .with_duration(1500);
        b.iter(|| {
            let json = serde_json::to_string(black_box(&entry)).unwrap();
            let _: ExperienceEntry = serde_json::from_str(&json).unwrap();
        })
    });

    group.bench_function("evolution_action_serde", |b| {
        let actions = [
            EvolutionAction::AdjustParameter, EvolutionAction::SwitchStrategy,
            EvolutionAction::UpdateBehavior, EvolutionAction::AddRetry,
            EvolutionAction::AdjustTimeout, EvolutionAction::AddValidation,
            EvolutionAction::OptimizeCollaboration, EvolutionAction::ScaleResource,
            EvolutionAction::LearnCapability,
        ];
        b.iter(|| {
            for a in &actions {
                let json = serde_json::to_string(black_box(a)).unwrap();
                let _: EvolutionAction = serde_json::from_str(&json).unwrap();
            }
        })
    });

    group.finish();
}

// ── ExperienceEntry 创建 ──────────────────────────

fn bench_entry_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("autonomy::entry");

    group.bench_function("new_minimal", |b| {
        b.iter(|| {
            ExperienceEntry::new(
                black_box("agent-x"),
                ExperienceType::TaskExecution,
                black_box("simple task"),
                "",
                ExperienceOutcome::Success,
            )
        })
    });

    group.bench_function("new_full", |b| {
        b.iter(|| {
            ExperienceEntry::new("agent-y", ExperienceType::Error, "Timeout after 30s", "connection timeout", ExperienceOutcome::Failure("timeout".into()))
                .with_severity(ExperienceSeverity::Error)
                .with_tag("timeout")
                .with_tag("critical")
                .with_duration(30000)
                .with_context(serde_json::json!({"task_id": "t-123", "attempt": 3}))
        })
    });

    group.finish();
}

// ── ExperienceStore 操作 ──────────────────────────

fn bench_store_ops(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("autonomy::store");

    group.bench_function("store_single", |b| {
        let store = ExperienceStore::new(10000);
        let entry = ExperienceEntry::new("agent-a", ExperienceType::TaskExecution, "bench", "", ExperienceOutcome::Success);
        b.iter(|| {
            rt.block_on(async {
                store.store(black_box(entry.clone())).await;
            })
        })
    });

    group.bench_function("store_batch_100", |b| {
        let store = ExperienceStore::new(10000);
        let entries: Vec<ExperienceEntry> = (0..100)
            .map(|i| {
                ExperienceEntry::new(
                    &format!("agent-{}", i % 5),
                    match i % 7 { 0 => ExperienceType::TaskExecution, 1 => ExperienceType::Decision,
                        2 => ExperienceType::Conversation, 3 => ExperienceType::Error,
                        4 => ExperienceType::Performance, 5 => ExperienceType::Feedback,
                        _ => ExperienceType::Collaboration },
                    &format!("Experience {i}"),
                    "",
                    ExperienceOutcome::Success,
                ).with_severity(match i % 5 { 0 => ExperienceSeverity::Info, 1 => ExperienceSeverity::Notice,
                    2 => ExperienceSeverity::Warning, 3 => ExperienceSeverity::Error,
                    _ => ExperienceSeverity::Critical })
                 .with_duration((i * 100) as u64)
            })
            .collect();
        b.iter(|| {
            rt.block_on(async {
                store.store_batch(black_box(entries.clone())).await;
            })
        })
    });

    group.bench_function("query_by_agent_1000", |b| {
        let store = ExperienceStore::new(10000);
        rt.block_on(async {
            let mut entries = Vec::new();
            for i in 0..500 {
                entries.push(ExperienceEntry::new(
                    &format!("agent-{}", i % 10),
                    ExperienceType::TaskExecution, "query-bench", "", ExperienceOutcome::Success,
                ));
            }
            store.store_batch(entries).await;
        });
        b.iter(|| {
            rt.block_on(async {
                black_box(store.get_agent_experiences(black_box("agent-3")).await)
            })
        })
    });

    group.finish();
}

// ── ReflectionEngine ──────────────────────────────

fn bench_reflection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("autonomy::reflection");

    group.bench_function("reflect_100_entries", |b| {
        let store = Arc::new(ExperienceStore::new(10000));
        let engine = ReflectionEngine::new(ReflectionConfig::default(), store.clone());
        let entries: Vec<ExperienceEntry> = (0..100)
            .map(|i| {
                ExperienceEntry::new("agent-r", ExperienceType::TaskExecution, &format!("run-{i}"), "", ExperienceOutcome::Success)
                    .with_severity(if i % 4 == 0 { ExperienceSeverity::Error } else { ExperienceSeverity::Info })
                    .with_duration((100 + i * 10) as u64)
                    .with_context(serde_json::json!({"attempt": i % 3}))
            })
            .collect();

        // 预存经验到 store
        rt.block_on(async {
            for entry in &entries {
                store.store(entry.clone()).await;
            }
        });

        b.iter(|| {
            rt.block_on(async {
                black_box(engine.reflect("agent-r").await)
            })
        })
    });

    group.finish();
}

// ── EvolutionEngine ───────────────────────────────

fn bench_evolution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("autonomy::evolution");

    group.bench_function("evolve", |b| {
        let store = Arc::new(ExperienceStore::new(10000));
        let reflection = Arc::new(ReflectionEngine::new(ReflectionConfig::default(), store.clone()));
        let engine = EvolutionEngine::new(EvolutionConfig::default(), store.clone(), reflection.clone());

        // 预存经验触发进化
        rt.block_on(async {
            for i in 0..6 {
                let entry = ExperienceEntry::new(
                    "agent-e", ExperienceType::TaskExecution,
                    &format!("exp-{}", i), "test",
                    if i % 2 == 0 { ExperienceOutcome::Success } else { ExperienceOutcome::Failure("err".into()) },
                );
                store.store(entry).await;
            }
        });

        b.iter(|| {
            rt.block_on(async {
                black_box(engine.evolve("agent-e").await)
            })
        })
    });

    group.finish();
}

criterion_group!(
    autonomy_benches,
    bench_enum_ops,
    bench_entry_create,
    bench_store_ops,
    bench_reflection,
    bench_evolution,
);
criterion_main!(autonomy_benches);
