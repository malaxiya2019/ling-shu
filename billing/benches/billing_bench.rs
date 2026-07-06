use criterion::{criterion_group, criterion_main, Criterion};
use lingshu_billing::{BillingPlan, BillingSystem, PeriodType, UsageTracker};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

fn bench_usage_tracker(c: &mut Criterion) {
    let mut group = c.benchmark_group("billing::usage_tracker");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("record", |b| {
        let tracker = UsageTracker::new();
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            let _ = tracker.record("user-1", "gpt-4", 500, 200).await;
        });
    });

    group.bench_function("get_summary_missing", |b| {
        let tracker = UsageTracker::new();
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            let _ = tracker.get_summary("no-such-user", "gpt-4").await;
        });
    });

    group.finish();
}

fn bench_quota_manager(c: &mut Criterion) {
    let plans = vec![
        BillingPlan::free(),
        BillingPlan::basic(),
        BillingPlan::pro(),
    ];

    let mut group = c.benchmark_group("billing::quota_manager");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("check_quota_free", |b| {
        let manager = lingshu_billing::QuotaManager::new(plans.clone());
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            let _ = manager.check_quota("user-1", "free").await;
        });
    });

    group.bench_function("check_quota_pro", |b| {
        let manager = lingshu_billing::QuotaManager::new(plans.clone());
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            let _ = manager.check_quota("user-2", "pro").await;
        });
    });

    group.bench_function("consume_1000_tokens", |b| {
        let manager = lingshu_billing::QuotaManager::new(plans.clone());
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            manager.consume("user-1", 1000).await;
        });
    });

    group.finish();
}

fn bench_billing_system(c: &mut Criterion) {
    let plans = vec![
        BillingPlan::free(),
        BillingPlan::basic(),
        BillingPlan::pro(),
    ];

    let mut group = c.benchmark_group("billing::system");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("record_usage_free_tier", |b| {
        let system = BillingSystem::new(plans.clone()).unwrap();
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            let _ = system
                .record_usage("user-1", "deepseek-chat", 1024, 512)
                .await;
        });
    });

    group.bench_function("check_quota_free_tier", |b| {
        let system = BillingSystem::new(plans.clone()).unwrap();
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            let _ = system.check_quota("user-1", "free").await;
        });
    });

    group.bench_function("generate_report_user", |b| {
        let system = BillingSystem::new(plans.clone()).unwrap();
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| async {
            // Pre-seed some usage records
            for i in 0..100 {
                let _ = system
                    .record_usage(
                        &format!("user-{}", i % 10),
                        "gpt-4",
                        100,
                        50,
                    )
                    .await;
            }
            let _ = system
                .generate_report("user-1", PeriodType::Daily)
                .await;
        });
    });

    group.bench_function("record_usage_high_contention", |b| {
        let system = Arc::new(BillingSystem::new(plans.clone()).unwrap());
        let rt = Runtime::new().unwrap();
        b.to_async(rt).iter(|| {
            let sys = system.clone();
            async move {
                let mut handles = Vec::new();
                for i in 0..20 {
                    let s = sys.clone();
                    handles.push(tokio::spawn(async move {
                        let _ = s
                            .record_usage(
                                &format!("user-{i}"),
                                "deepseek-chat",
                                256,
                                128,
                            )
                            .await;
                    }));
                }
                for h in handles {
                    let _ = h.await;
                }
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_usage_tracker,
    bench_quota_manager,
    bench_billing_system,
);
criterion_main!(benches);
