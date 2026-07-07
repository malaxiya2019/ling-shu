//! LSEvaluator — Criterion 基准测试.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lingshu_evaluator::metrics::{self, score_exact, score_contains, score_json_structure};
use serde_json::json;

fn bench_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("scoring");

    group.bench_function("exact_match", |b| {
        b.iter(|| {
            score_exact(
                black_box(&json!("hello world")),
                black_box(&json!("hello world")),
            )
        })
    });

    group.bench_function("contains_match", |b| {
        b.iter(|| {
            score_contains(
                black_box(&json!("the quick brown fox jumps over the lazy dog")),
                black_box(&json!("fox")),
            )
        })
    });

    group.bench_function("json_structure", |b| {
        let actual = json!({
            "name": "Alice",
            "age": 30,
            "address": {"city": "NYC", "zip": "10001"},
            "tags": ["a", "b", "c"]
        });
        let expected = json!({
            "name": "Bob",
            "age": 25,
            "address": {"city": "LA", "zip": "90001"},
            "tags": ["x", "y", "z"]
        });
        b.iter(|| score_json_structure(black_box(&actual), black_box(&expected)))
    });

    group.finish();
}

fn bench_metrics(c: &mut Criterion) {
    use lingshu_evaluator::types::EvalCaseResult;
    use std::time::Duration;

    let results: Vec<EvalCaseResult> = (0..100)
        .map(|i| EvalCaseResult {
            case_id: format!("c{i}"),
            case_name: format!("test-{i}"),
            passed: i % 3 != 0,
            score: if i % 3 == 0 { 0.2 } else { 0.9 },
            actual_output: json!("output"),
            expected_output: Some(json!("expected")),
            error: None,
            latency: Duration::from_millis((i as u64) * 10),
            input_tokens: (i as u64) * 5,
            output_tokens: (i as u64) * 3,
            cost: (i as f64) * 0.001,
            details: Default::default(),
        })
        .collect();

    c.bench_function("compute_metrics_100", |b| {
        b.iter(|| metrics::compute_metrics(black_box(&results)))
    });
}

criterion_group!(benches, bench_scoring, bench_metrics);
criterion_main!(benches);
