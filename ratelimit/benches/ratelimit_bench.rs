use criterion::{criterion_group, criterion_main, Criterion};
use lingshu_ratelimit::{MultiRateLimiter, RateLimiter, SlidingWindow, TokenBucket};
use std::sync::Arc;
use std::time::Duration;

fn bench_token_bucket(c: &mut Criterion) {
    let mut group = c.benchmark_group("ratelimit::token_bucket");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("check_under_capacity", |b| {
        let bucket = TokenBucket::new(1_000_000, 100_000.0);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = bucket.check("bench-key").await;
            });
    });

    group.bench_function("check_exhausted", |b| {
        let bucket = TokenBucket::new(10, 1.0);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = bucket.check("bench-key").await;
            });
    });

    group.bench_function("peek", |b| {
        let bucket = TokenBucket::new(1_000_000, 100_000.0);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = bucket.peek("bench-key").await;
            });
    });

    group.bench_function("reset", |b| {
        let bucket = TokenBucket::new(1_000_000, 100_000.0);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = bucket.reset("bench-key").await;
            });
    });

    group.finish();
}

fn bench_sliding_window(c: &mut Criterion) {
    let mut group = c.benchmark_group("ratelimit::sliding_window");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("check_under_limit", |b| {
        let sw = SlidingWindow::new(60, 100_000);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = sw.check("bench-key").await;
            });
    });

    group.bench_function("check_exhausted", |b| {
        let sw = SlidingWindow::new(60, 5);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = sw.check("bench-key").await;
            });
    });

    group.bench_function("peek", |b| {
        let sw = SlidingWindow::new(60, 100_000);
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = sw.peek("bench-key").await;
            });
    });

    group.finish();
}

fn bench_multi_ratelimiter(c: &mut Criterion) {
    let mut group = c.benchmark_group("ratelimit::multi");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("check_all_2_limiters", |b| {
        let mut multi = MultiRateLimiter::new();
        multi.add(
            "token-bucket",
            Arc::new(TokenBucket::new(100_000, 1000.0)),
        );
        multi.add(
            "sliding-window",
            Arc::new(SlidingWindow::new(60, 100_000)),
        );
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let _ = multi.check_all("bench-key").await;
            });
    });

    group.finish();
}

fn bench_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("ratelimit::contention");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(50);

    group.bench_function("token_bucket_10_concurrent_keys", |b| {
        let bucket = Arc::new(TokenBucket::new(1_000_000, 100_000.0));
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.to_async(rt).iter(|| {
            let bucket = bucket.clone();
            async move {
                let mut handles = Vec::new();
                for i in 0..10 {
                    let b = bucket.clone();
                    handles.push(tokio::spawn(async move {
                        let _ = b.check(&format!("key-{i}")).await;
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
    bench_token_bucket,
    bench_sliding_window,
    bench_multi_ratelimiter,
    bench_contention,
);
criterion_main!(benches);
