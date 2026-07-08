//! Lingshu evaluator 端到端集成测试.
//!
//! 测试套件定义、运行器、指标计算、报告生成和回归检测的全链路。

use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use lingshu_evaluator::*;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// 模拟可评测目标 — 对 TestCase 做回声响应.
struct MockEvaluable {
    name: String,
    version: String,
    /// 模拟延迟.
    latency_ms: u64,
    /// 模拟输入 Token.
    input_tokens: u64,
    /// 模拟输出 Token.
    output_tokens: u64,
    /// 模拟成本.
    cost: f64,
}

#[async_trait]
impl Evaluable for MockEvaluable {
    async fn execute(&self, _ctx: &LsContext, _case: &TestCase) -> LsResult<ExecutedOutput> {
        tokio::time::sleep(Duration::from_millis(self.latency_ms)).await;
        Ok(ExecutedOutput {
            output: _case.expected.clone().unwrap_or(_case.input.clone()),
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

/// 构建一个多用例测试套件.
fn build_math_suite() -> TestSuite {
    let mut suite = TestSuite::new("数学测试", "math");
    suite.description = "基础数学运算评测".into();

    suite.add_case(TestCase {
        id: "add-1".into(),
        name: "1+1=2".into(),
        input: json!("1+1=?"),
        expected: Some(json!("2")),
        expected_type: ExpectedType::Exact,
        weight: 1.0,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite.add_case(TestCase {
        id: "sub-1".into(),
        name: "3-1=2".into(),
        input: json!("3-1=?"),
        expected: Some(json!("2")),
        expected_type: ExpectedType::Contains,
        weight: 1.0,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite.add_case(TestCase {
        id: "mul-1".into(),
        name: "2*3=6".into(),
        input: json!("2*3=?"),
        expected: Some(json!("6")),
        expected_type: ExpectedType::Exact,
        weight: 2.0,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite.add_case(TestCase {
        id: "div-1".into(),
        name: "6/2=3".into(),
        input: json!("6/2=?"),
        expected: Some(json!("3")),
        expected_type: ExpectedType::Exact,
        weight: 1.0,
        timeout: Duration::from_secs(5),
        ..Default::default()
    });

    suite
}

// ── 测试: 运行器全链路 ────────────────────────────

#[tokio::test]
async fn test_evaluator_run_suite() {
    let target = Arc::new(MockEvaluable {
        name: "mock-agent".into(),
        version: "1.0.0".into(),
        latency_ms: 10,
        input_tokens: 10,
        output_tokens: 20,
        cost: 0.001,
    });

    let suite = build_math_suite();
    let runner = EvalRunner::new(target, EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new()).with_user("test");
    let result = runner.run_suite(&suite, &ctx).await;

    assert_eq!(result.total_cases, 4, "should have 4 cases");
    assert_eq!(
        result.passed_cases, 4,
        "all mock cases should pass (echo match)"
    );
    assert!((result.overall_score - 1.0).abs() < 1e-6, "perfect score");
    assert!(result.total_duration > Duration::ZERO, "should have duration");
}

#[tokio::test]
async fn test_evaluator_with_failures() {
    // 一个会返回错误的目标
    struct FailingEvaluable;

    #[async_trait]
    impl Evaluable for FailingEvaluable {
        async fn execute(&self, _ctx: &LsContext, _case: &TestCase) -> LsResult<ExecutedOutput> {
            // 返回与期望不同的输出
            Ok(ExecutedOutput {
                output: json!("wrong_answer"),
                latency: Duration::from_millis(5),
                input_tokens: 5,
                output_tokens: 5,
                cost: 0.0005,
            })
        }

        fn target_name(&self) -> &str {
            "failing-mock"
        }

        fn target_version(&self) -> &str {
            "1.0"
        }
    }

    let target = Arc::new(FailingEvaluable);
    let suite = build_math_suite();
    let runner = EvalRunner::new(target, EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new()).with_user("test");
    let result = runner.run_suite(&suite, &ctx).await;

    assert_eq!(result.total_cases, 4);
    assert_eq!(result.passed_cases, 0, "all should fail");
    assert!((result.overall_score - 0.0).abs() < 1e-6, "zero score");
}

// ── 测试: 指标计算 ────────────────────────────────

#[tokio::test]
async fn test_evaluator_metrics() {
    let target = Arc::new(MockEvaluable {
        name: "metrics-test".into(),
        version: "1.0".into(),
        latency_ms: 15,
        input_tokens: 20,
        output_tokens: 30,
        cost: 0.002,
    });

    let suite = build_math_suite();
    let runner = EvalRunner::new(target, EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new());
    let result = runner.run_suite(&suite, &ctx).await;

    let metrics = &result.metrics;
    assert!((metrics.accuracy - 1.0).abs() < 1e-6, "accuracy should be 1.0");
    assert!((metrics.avg_input_tokens - 20.0).abs() < 1e-6, "avg input tokens");
    assert!((metrics.avg_output_tokens - 30.0).abs() < 1e-6, "avg output tokens");
    assert!(metrics.total_tokens >= 200, "total tokens ({}) >= 200", metrics.total_tokens);
    assert!(metrics.avg_latency >= Duration::from_millis(15), "avg latency");
    assert!(metrics.p50_latency >= Duration::from_millis(15), "p50 latency");
}

// ── 测试: 回归检测 ────────────────────────────────

#[test]
fn test_evaluator_regression_detection() {
    use lingshu_core::LsId;
    use std::time::Duration;

    // 构建基线结果：全部通过
    let baseline = EvaluationResult {
        id: LsId::new(),
        suite_name: "regression-test".into(),
        target_name: "mock".into(),
        target_version: "1.0".into(),
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
        total_duration: Duration::from_millis(100),
        total_cases: 3,
        passed_cases: 3,
        failed_cases: 0,
        overall_score: 1.0,
        weighted_score: 1.0,
        metrics: MetricsSummary {
            accuracy: 1.0,
            avg_latency: Duration::from_millis(50),
            total_cost: 0.003,
            ..Default::default()
        },
        case_results: vec![
            EvalCaseResult {
                case_id: "c1".into(),
                case_name: "pass-1".into(),
                passed: true,
                score: 1.0,
                latency: Duration::from_millis(40),
                ..default_case_detail()
            },
            EvalCaseResult {
                case_id: "c2".into(),
                case_name: "pass-2".into(),
                passed: true,
                score: 1.0,
                latency: Duration::from_millis(50),
                ..default_case_detail()
            },
            EvalCaseResult {
                case_id: "c3".into(),
                case_name: "pass-3".into(),
                passed: true,
                score: 1.0,
                latency: Duration::from_millis(60),
                ..default_case_detail()
            },
        ],
        metadata: Default::default(),
    };

    // 当前结果：c3 回归失败
    let current = EvaluationResult {
        id: LsId::new(),
        suite_name: "regression-test".into(),
        target_name: "mock".into(),
        target_version: "1.0".into(),
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
        total_duration: Duration::from_millis(120),
        total_cases: 3,
        passed_cases: 2,
        failed_cases: 1,
        overall_score: 0.67,
        weighted_score: 0.67,
        metrics: MetricsSummary {
            accuracy: 0.67,
            avg_latency: Duration::from_millis(55),
            total_cost: 0.003,
            ..Default::default()
        },
        case_results: vec![
            EvalCaseResult {
                case_id: "c1".into(),
                case_name: "pass-1".into(),
                passed: true,
                score: 1.0,
                latency: Duration::from_millis(45),
                ..default_case_detail()
            },
            EvalCaseResult {
                case_id: "c2".into(),
                case_name: "pass-2".into(),
                passed: true,
                score: 1.0,
                latency: Duration::from_millis(50),
                ..default_case_detail()
            },
            EvalCaseResult {
                case_id: "c3".into(),
                case_name: "pass-3".into(),
                passed: false,
                score: 0.0,
                latency: Duration::from_millis(70),
                ..default_case_detail()
            },
        ],
        metadata: Default::default(),
    };

    let thresholds = RegressionThresholds {
        max_score_degradation: 0.05,
        max_pass_rate_degradation: 0.02,
        max_latency_increase: Duration::from_millis(500),
        max_cost_increase: 0.01,
    };

    let result = RegressionDetector::detect(&current, &baseline, &thresholds);
    assert!(result.has_regression, "should detect regression");
    assert!(result.score_delta < 0.0, "score should decrease");
    assert!(result.pass_rate_delta < 0.0, "pass rate should decrease");

    // 检查逐用例对比
    let c3_comp = result
        .comparisons
        .iter()
        .find(|c| c.case_id == "c3")
        .expect("c3 should be in comparisons");
    assert!(
        matches!(c3_comp.status, ComparisonStatus::Regression),
        "c3 should be regression"
    );
}

// ── 测试: 报告生成 ────────────────────────────────

#[tokio::test]
async fn test_evaluator_report_generation() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let dir_path = dir.path().to_path_buf();

    let target = Arc::new(MockEvaluable {
        name: "report-test".into(),
        version: "2.0".into(),
        latency_ms: 5,
        input_tokens: 10,
        output_tokens: 15,
        cost: 0.001,
    });

    let suite = build_math_suite();
    let runner = EvalRunner::new(target, EvalConfig {
        output_dir: Some(dir_path.to_string_lossy().to_string()),
        report_formats: vec![ReportFormat::Json, ReportFormat::Markdown],
        ..Default::default()
    });
    let ctx = LsContext::with_session(LsId::new()).with_user("test");
    let result = runner.run_suite(&suite, &ctx).await;

    // 使用 ReportGenerator 生成文件
    let gen = ReportGenerator::new(&dir_path);
    let paths = gen
        .generate(&result, None, &[ReportFormat::Json, ReportFormat::Markdown])
        .expect("generate reports");

    assert_eq!(paths.len(), 2, "should generate 2 report files");

    let json_path = dir_path.join("evaluation_report.json");
    let md_path = dir_path.join("evaluation_report.md");
    assert!(json_path.exists(), "JSON report should exist");
    assert!(md_path.exists(), "Markdown report should exist");

    // 验证 JSON 内容
    let json_content = std::fs::read_to_string(&json_path).unwrap();
    assert!(json_content.contains("数学测试"));
    assert!(json_content.contains("overall_score"));

    // 验证 Markdown 内容
    let md_content = std::fs::read_to_string(&md_path).unwrap();
    assert!(md_content.contains("数学测试"));
    assert!(md_content.contains("1+1=2"));
}


/// 回声可评测目标 — 返回输入值 (而非预期值).
struct EchoEvaluable {
    name: String,
    version: String,
    latency_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
}

#[async_trait]
impl Evaluable for EchoEvaluable {
    async fn execute(&self, _ctx: &LsContext, case: &TestCase) -> LsResult<ExecutedOutput> {
        tokio::time::sleep(Duration::from_millis(self.latency_ms)).await;
        Ok(ExecutedOutput {
            output: case.input.clone(),
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

// ── 测试: 加权评分 ────────────────────────────────

#[tokio::test]
async fn test_evaluator_weighted_scoring() {
    let target = Arc::new(EchoEvaluable {
        name: "weight-test".into(),
        version: "1.0".into(),
        latency_ms: 5,
        input_tokens: 10,
        output_tokens: 10,
        cost: 0.001,
    });

    // 一个通过、一个失败，权重不同
    let mut suite = TestSuite::new("weight-test", "weight");
    suite.add_case(TestCase {
        id: "pass".into(),
        name: "passing".into(),
        input: json!("ok"),
        expected: Some(json!("ok")),
        expected_type: ExpectedType::Exact,
        weight: 3.0,
        ..Default::default()
    });
    suite.add_case(TestCase {
        id: "fail".into(),
        name: "failing".into(),
        input: json!("ok"),
        expected: Some(json!("not_ok")),
        expected_type: ExpectedType::Contains,
        weight: 1.0,
        ..Default::default()
    });

    let runner = EvalRunner::new(target, EvalConfig::default());
    let ctx = LsContext::with_session(LsId::new());
    let result = runner.run_suite(&suite, &ctx).await;

    assert_eq!(result.total_cases, 2);
    assert_eq!(result.passed_cases, 1);
    // 加权: (3*1.0 + 1*0.0) / 4 = 0.75
    assert!(
        (result.weighted_score - 0.75).abs() < 1e-6,
        "weighted score should be 0.75, got {}",
        result.weighted_score
    );
}

// ── 辅助函数 ──────────────────────────────────────

fn default_case_detail() -> EvalCaseResult {
    EvalCaseResult {
        case_id: String::new(),
        case_name: String::new(),
        passed: false,
        score: 0.0,
        actual_output: serde_json::Value::Null,
        expected_output: None,
        error: None,
        latency: Duration::ZERO,
        input_tokens: 0,
        output_tokens: 0,
        cost: 0.0,
        details: std::collections::HashMap::new(),
    }
}
