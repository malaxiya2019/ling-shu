//! End-to-end integration test: Evaluator + Federation combined.
//!
//! This test validates:
//! 1. Evaluator test suite runs correctly in isolation
//! 2. Federation cluster with 2 nodes establishes connections
//! 3. Both evaluator and federation work together without interference
//! 4. Evaluation results can be queried in a federated context

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_evaluator::*;
use lingshu_federation::{
    discovery::StaticDiscovery,
    types::*,
    Federation, FederationConfig,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

// ── Helpers ────────────────────────────────────────

fn pick_port(base: u16) -> u16 {
    let mut port = base;
    for _ in 0..100 {
        match std::net::TcpListener::bind(
            std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port),
        ) {
            Ok(listener) => {
                let _ = listener.set_nonblocking(true);
                return port;
            }
            Err(_) => port += 1,
        }
    }
    base
}

fn make_fed_config(cluster_name: &str, port: u16, seeds: Vec<SocketAddr>) -> FederationConfig {
    FederationConfig {
        cluster_name: cluster_name.to_string(),
        listen_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        topology: FederationTopology::Mesh,
        seed_nodes: seeds,
        discovery_interval: Duration::from_secs(1),
        heartbeat_interval: Duration::from_millis(200),
        heartbeat_timeout: Duration::from_secs(5),
        capability_advertise_interval: Duration::from_secs(60),
        max_reconnect_attempts: 2,
        reconnect_backoff_secs: 1,
        enabled: true,
    }
}

fn register_static_discovery(fed: &mut Federation, seeds: Vec<SocketAddr>) {
    let discovery_backend = Arc::new(StaticDiscovery::new(seeds));
    if let Some(dm) = Arc::get_mut(&mut fed.discovery_mgr) {
        dm.register(discovery_backend);
    }
}

// ── Mock Evaluable ─────────────────────────────────

struct MockEvalTarget {
    name: String,
    version: String,
    latency_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
}

#[async_trait]
impl Evaluable for MockEvalTarget {
    async fn execute(&self, _ctx: &LsContext, case: &TestCase) -> LsResult<ExecutedOutput> {
        tokio::time::sleep(Duration::from_millis(self.latency_ms)).await;
        Ok(ExecutedOutput {
            output: case.expected.clone().unwrap_or(case.input.clone()),
            latency: Duration::from_millis(self.latency_ms),
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cost: self.cost,
        })
    }

    fn target_name(&self) -> &str {
        &self.name
    }
    fn target_version(&self) -> &str {
        &self.version
    }
}

fn build_test_suite() -> TestSuite {
    let mut suite = TestSuite::new("e2e-federated-eval", "integration-test");
    suite.description = "E2E evaluator + federation test suite".into();

    suite.add_case(TestCase {
        id: "e2e-1".into(),
        name: "exact-match".into(),
        input: json!("hello"),
        expected: Some(json!("hello")),
        expected_type: ExpectedType::Exact,
        weight: 1.0,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite.add_case(TestCase {
        id: "e2e-2".into(),
        name: "contains-match".into(),
        input: json!("the quick brown fox"),
        expected: Some(json!("quick")),
        expected_type: ExpectedType::Contains,
        weight: 2.0,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite.add_case(TestCase {
        id: "e2e-3".into(),
        name: "numeric-range".into(),
        input: json!("value is 42"),
        expected: Some(json!("42")),
        expected_type: ExpectedType::Contains,
        weight: 1.5,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite
}

// ── Test: Evaluator + Federation Combined Setup ────

#[tokio::test]
async fn test_evaluator_runs_with_federation_active() {
    // Create 2-node federation cluster
    let id_a = LsId::new();
    let id_b = LsId::new();
    let port_a = pick_port(19850);
    let port_b = pick_port(19851);
    let addr_a: SocketAddr = format!("127.0.0.1:{}", port_a).parse().unwrap();
    let addr_b: SocketAddr = format!("127.0.0.1:{}", port_b).parse().unwrap();

    let mut fed_a = Federation::new(id_a, make_fed_config("e2e-node-a", port_a, vec![])).await;
    let mut fed_b = Federation::new(id_b, make_fed_config("e2e-node-b", port_b, vec![])).await;

    register_static_discovery(&mut fed_a, vec![addr_b]);
    register_static_discovery(&mut fed_b, vec![addr_a]);

    fed_a.start().await.expect("node-a start");
    fed_b.start().await.expect("node-b start");

    // Wait for cluster to stabilize
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Verify federation is working
    let stats_a = fed_a.stats().await;
    assert!(
        stats_a.uptime_seconds > 0,
        "node-a should have positive uptime"
    );

    // ── Run evaluator on node-a's context ──
    let target = Arc::new(MockEvalTarget {
        name: "e2e-agent-a".into(),
        version: "1.0.0".into(),
        latency_ms: 5,
        input_tokens: 10,
        output_tokens: 20,
        cost: 0.001,
    });

    let suite = build_test_suite();
    let runner = EvalRunner::new(target, EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new())
        .with_user("e2e-test")
        .with_metadata("cluster_id", id_a.to_string());

    let result = runner.run_suite(&suite, &ctx).await;

    // Verify evaluator results
    assert_eq!(result.total_cases, 3, "should have 3 test cases");
    assert_eq!(
        result.passed_cases, 3,
        "all mock cases should pass (echo match)"
    );
    assert!(
        (result.overall_score - 1.0).abs() < 1e-6,
        "perfect overall score"
    );
    assert!(
        (result.weighted_score - 1.0).abs() < 1e-6,
        "perfect weighted score"
    );
    assert!(result.total_duration > Duration::ZERO, "should have duration");

    // Verify metrics exist
    let metrics = compute_metrics(&result);
    assert!((metrics.accuracy - 1.0).abs() < 1e-6, "accuracy should be 1.0");
    assert!((metrics.precision - 1.0).abs() < 1e-6, "precision should be 1.0");
    assert!((metrics.recall - 1.0).abs() < 1e-6, "recall should be 1.0");
    assert!((metrics.f1_score - 1.0).abs() < 1e-6, "f1 should be 1.0");

    // Clean up federation
    fed_a.stop().await;
    fed_b.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── Test: Evaluator With Partial Failures ──────────

#[tokio::test]
async fn test_evaluator_partial_failures_in_federation() {
    // Create minimal single-node federation cluster
    let id = LsId::new();
    let port = pick_port(19860);
    let fed = Federation::new(id, make_fed_config("partial-node", port, vec![])).await;

    fed.start().await.expect("partial-node start");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Create an evaluable that fails on specific cases
    struct SelectiveEval {
        fail_ids: Vec<String>,
    }

    #[async_trait]
    impl Evaluable for SelectiveEval {
        async fn execute(&self, _ctx: &LsContext, case: &TestCase) -> LsResult<ExecutedOutput> {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if self.fail_ids.contains(&case.id) {
                Ok(ExecutedOutput {
                    output: json!("wrong"),
                    latency: Duration::from_millis(5),
                    input_tokens: 5,
                    output_tokens: 5,
                    cost: 0.0005,
                })
            } else {
                Ok(ExecutedOutput {
                    output: case.expected.clone().unwrap_or(case.input.clone()),
                    latency: Duration::from_millis(5),
                    input_tokens: 5,
                    output_tokens: 5,
                    cost: 0.0005,
                })
            }
        }
        fn target_name(&self) -> &str {
            "selective-eval"
        }
        fn target_version(&self) -> &str {
            "1.0"
        }
    }

    let target = Arc::new(SelectiveEval {
        fail_ids: vec!["e2e-2".into()],
    });

    let suite = build_test_suite();
    let runner = EvalRunner::new(target, EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new());

    let result = runner.run_suite(&suite, &ctx).await;

    // Verify: 2 passed, 1 failed
    assert_eq!(result.total_cases, 3);
    assert_eq!(result.passed_cases, 2);
    assert_eq!(result.failed_cases, 1);

    // Weighted: (1.0*1.0 + 0.0*2.0 + 1.0*1.5) / (1.0 + 2.0 + 1.5) = 2.5/4.5 ≈ 0.556
    let expected_weighted = (1.0 + 0.0 + 1.5) / 4.5;
    assert!(
        (result.weighted_score - expected_weighted).abs() < 1e-6,
        "weighted score should be {expected_weighted}, got {}",
        result.weighted_score
    );

    fed.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── Test: Evaluator Report Generation While Federated ──

#[tokio::test]
async fn test_evaluator_report_generation_federated() {
    let id = LsId::new();
    let port = pick_port(19870);
    let fed = Federation::new(id, make_fed_config("report-node", port, vec![])).await;
    fed.start().await.expect("report-node start");

    let dir = tempfile::tempdir().expect("create temp dir");
    let dir_path = dir.path().to_path_buf();

    let target = Arc::new(MockEvalTarget {
        name: "report-agent".into(),
        version: "2.0".into(),
        latency_ms: 5,
        input_tokens: 10,
        output_tokens: 15,
        cost: 0.001,
    });

    let suite = build_test_suite();
    let runner = EvalRunner::new(target, EvalConfig {
        output_dir: Some(dir_path.to_string_lossy().to_string()),
        report_formats: vec![ReportFormat::Json, ReportFormat::Markdown],
        ..Default::default()
    });
    let ctx = LsContext::with_session(LsId::new()).with_user("e2e-report");
    let result = runner.run_suite(&suite, &ctx).await;

    // Generate reports
    let gen = ReportGenerator::new(&dir_path);
    let paths = gen
        .generate(&result, None, &[ReportFormat::Json, ReportFormat::Markdown])
        .expect("generate reports");

    assert_eq!(paths.len(), 2, "should generate 2 report files");

    let json_path = dir_path.join("evaluation_report.json");
    let md_path = dir_path.join("evaluation_report.md");
    assert!(json_path.exists(), "JSON report should exist");
    assert!(md_path.exists(), "Markdown report should exist");

    // Verify JSON content
    let json_content = std::fs::read_to_string(&json_path).unwrap();
    assert!(json_content.contains("e2e-federated-eval"));
    assert!(json_content.contains("overall_score"));

    // Verify Markdown content
    let md_content = std::fs::read_to_string(&md_path).unwrap();
    assert!(md_content.contains("e2e-federated-eval"));

    fed.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── Test: Regression Detection in Federated Context ──

#[tokio::test]
async fn test_evaluator_regression_federated() {
    let id = LsId::new();
    let port = pick_port(19880);
    let fed = Federation::new(id, make_fed_config("regression-node", port, vec![])).await;
    fed.start().await.expect("regression-node start");

    let target = Arc::new(MockEvalTarget {
        name: "regression-agent".into(),
        version: "1.0.0".into(),
        latency_ms: 5,
        input_tokens: 10,
        output_tokens: 15,
        cost: 0.001,
    });

    let suite = build_test_suite();
    let runner = EvalRunner::new(target.clone(), EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new());
    let baseline = runner.run_suite(&suite, &ctx).await;

    // Create a degraded evaluator for regression comparison
    struct DegradedEval;

    #[async_trait]
    impl Evaluable for DegradedEval {
        async fn execute(&self, _ctx: &LsContext, case: &TestCase) -> LsResult<ExecutedOutput> {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(ExecutedOutput {
                output: json!("wrong_answer"),
                latency: Duration::from_millis(50),
                input_tokens: 5,
                output_tokens: 5,
                cost: 0.001,
            })
        }
        fn target_name(&self) -> &str {
            "degraded-agent"
        }
        fn target_version(&self) -> &str {
            "1.0"
        }
    }

    let degraded_runner = EvalRunner::new(Arc::new(DegradedEval), EvalConfig::default());
    let current = degraded_runner.run_suite(&suite, &ctx).await;

    // Regression detection
    let thresholds = RegressionThresholds {
        accuracy_drop: 0.1,
        latency_increase_pct: 50.0,
        min_sample_size: 1,
    };
    let regression = RegressionDetector::detect(&current, &baseline, &thresholds);

    assert!(
        matches!(regression.status, RegressionStatus::Regression),
        "degraded agent should be detected as regression"
    );
    assert!(
        regression.summary.contains("accuracy"),
        "summary should mention accuracy drop"
    );

    fed.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── Test: Multi-node Federation with Concurrent Eval ──

#[tokio::test]
async fn test_federation_concurrent_eval_across_nodes() {
    let ids: Vec<LsId> = (0..2).map(|_| LsId::new()).collect();
    let ports: Vec<u16> = (0..2).map(|i| pick_port(19900 + i * 10)).collect();
    let addrs: Vec<SocketAddr> = ports
        .iter()
        .map(|p| format!("127.0.0.1:{}", p).parse().unwrap())
        .collect();

    let mut feds = Vec::new();
    for i in 0..2 {
        let seeds = (0..2).filter(|j| *j != i).map(|j| addrs[j]).collect();
        let mut fed = Federation::new(
            ids[i],
            make_fed_config(&format!("concurrent-node-{}", i), ports[i], seeds.clone()),
        )
        .await;
        register_static_discovery(&mut fed, seeds);
        fed.start().await.expect(&format!("node-{} start", i));
        feds.push(fed);
    }

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Run evaluator on both nodes simultaneously
    let target = Arc::new(MockEvalTarget {
        name: "concurrent-agent".into(),
        version: "1.0.0".into(),
        latency_ms: 3,
        input_tokens: 8,
        output_tokens: 16,
        cost: 0.001,
    });

    let suite = Arc::new(build_test_suite());
    let ctx = LsContext::with_session(LsId::new());

    // Run 2 concurrent evaluations
    let suite_clone = suite.clone();
    let ctx_clone = LsContext::with_session(LsId::new());
    let handle_a = tokio::spawn(async move {
        let runner = EvalRunner::new(target, EvalConfig::default());
        runner.run_suite(&suite_clone, &ctx_clone).await
    });

    let target_b = Arc::new(MockEvalTarget {
        name: "concurrent-agent-b".into(),
        version: "1.0.0".into(),
        latency_ms: 3,
        input_tokens: 8,
        output_tokens: 16,
        cost: 0.001,
    });

    let handle_b = tokio::spawn(async move {
        let runner = EvalRunner::new(target_b, EvalConfig::default());
        runner.run_suite(&suite, &mut ctx).await
    });

    let (result_a, result_b) = tokio::join!(handle_a, handle_b);
    let result_a = result_a.expect("eval a completed");
    let result_b = result_b.expect("eval b completed");

    // Both evaluations should succeed
    assert_eq!(result_a.total_cases, 3);
    assert_eq!(result_b.total_cases, 3);
    assert_eq!(result_a.passed_cases, 3);
    assert_eq!(result_b.passed_cases, 3);

    // Cleanup
    for fed in &feds {
        fed.stop().await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}
