//! LSEvaluator — 评测运行器.
//!
//! 负责将测试套件分派给 Agent/LLM 执行，收集结果并计算指标。

use crate::metrics;
use crate::types::*;
use async_trait::async_trait;
use lingshu_core::{LsContext, LsId, LsResult};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// 可评测执行器 — 任何可以运行评测的目标.
#[async_trait]
pub trait Evaluable: Send + Sync {
    /// 执行单个测试用例，返回实际输出.
    async fn execute(
        &self,
        ctx: &LsContext,
        case: &TestCase,
    ) -> LsResult<ExecutedOutput>;

    /// 返回目标名称.
    fn target_name(&self) -> &str;

    /// 返回目标版本.
    fn target_version(&self) -> &str;
}

/// 执行输出.
#[derive(Debug, Clone)]
pub struct ExecutedOutput {
    pub output: serde_json::Value,
    pub latency: std::time::Duration,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: f64,
}

// ── 内置评分器 ─────────────────────────────────────

/// 根据 ExpectedType 计算评分.
pub fn score_output(actual: &serde_json::Value, expected: &serde_json::Value, kind: ExpectedType) -> f64 {
    match kind {
        ExpectedType::Exact => metrics::score_exact(actual, expected),
        ExpectedType::Contains => metrics::score_contains(actual, expected),
        ExpectedType::JsonStructure => metrics::score_json_structure(actual, expected),
        ExpectedType::Regex => {
            let pattern = expected.as_str().unwrap_or("");
            metrics::score_regex(actual, pattern)
        }
        ExpectedType::NumericRange => score_numeric_range(actual, expected),
        ExpectedType::Semantic | ExpectedType::Custom => {
            // 语义评分和自定义评分需要外部 LLM 或函数注入，默认返回精确匹配
            metrics::score_exact(actual, expected)
        }
    }
}

/// 数值范围评分：expected 格式为 `{"min": 0, "max": 100}`.
fn score_numeric_range(actual: &serde_json::Value, expected: &serde_json::Value) -> f64 {
    let actual_num = match actual.as_f64() {
        Some(n) => n,
        None => return 0.0,
    };
    let min = expected.get("min").and_then(|v| v.as_f64()).unwrap_or(f64::NEG_INFINITY);
    let max = expected.get("max").and_then(|v| v.as_f64()).unwrap_or(f64::INFINITY);
    if actual_num >= min && actual_num <= max {
        // 在范围内按位置给分（越靠近中间越高）
        if min.is_infinite() || max.is_infinite() {
            1.0
        } else if max > min {
            let mid = (min + max) / 2.0;
            let dist = (actual_num - mid).abs() / (max - min) * 2.0;
            (1.0 - dist).max(0.0)
        } else {
            1.0
        }
    } else {
        0.0
    }
}

// ── 运行器 ─────────────────────────────────────────

/// 评测运行器.
pub struct EvalRunner {
    /// 执行目标.
    pub target: Arc<dyn Evaluable>,
    /// 运行配置.
    pub config: EvalConfig,
}

impl EvalRunner {
    /// 创建评测运行器.
    pub fn new(target: Arc<dyn Evaluable>, config: EvalConfig) -> Self {
        Self { target, config }
    }

    /// 运行完整评测套件.
    pub async fn run_suite(&self, suite: &TestSuite, ctx: &LsContext) -> EvaluationResult {
        let started_at = chrono::Utc::now();
        let total = suite.cases.len();
        info!(
            suite = %suite.name,
            target = %self.target.target_name(),
            cases = total,
            "evaluation started"
        );

        let mut case_results = Vec::with_capacity(total);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.concurrency));
        let mut handles = Vec::new();

        for case in &suite.cases {
            let target = self.target.clone();
            let ctx = ctx.clone();
            let sem = semaphore.clone();
            let case = case.clone();
            let cfg = self.config.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                run_single_case(&*target, &case, &ctx, &cfg).await
            });
            handles.push(handle);
        }

        for handle in handles {
            match handle.await {
                Ok(result) => case_results.push(result),
                Err(e) => {
                    warn!(error = %e, "test case execution panicked");
                }
            }
        }

        let completed_at = chrono::Utc::now();
        let total_duration = completed_at - started_at;
        let total_duration = total_duration.to_std().unwrap_or_default();

        let passed = case_results.iter().filter(|r| r.passed).count();
        let failed = case_results.iter().filter(|r| !r.passed).count();

        let metrics_summary = metrics::compute_metrics(&case_results);
        let overall_score = if total > 0 {
            case_results.iter().map(|r| r.score).sum::<f64>() / total as f64
        } else {
            0.0
        };
        let total_weight: f64 = suite.cases.iter().map(|c| c.weight).sum();
        let weighted_score = if total_weight > 0.0 {
            suite
                .cases
                .iter()
                .zip(case_results.iter())
                .map(|(c, r)| c.weight * r.score)
                .sum::<f64>()
                / total_weight
        } else {
            overall_score
        };

        let result = EvaluationResult {
            id: LsId::new(),
            suite_name: suite.name.clone(),
            target_name: self.target.target_name().to_string(),
            target_version: self.target.target_version().to_string(),
            started_at,
            completed_at,
            total_duration,
            total_cases: total,
            passed_cases: passed,
            failed_cases: failed,
            overall_score,
            weighted_score,
            metrics: metrics_summary,
            case_results,
            metadata: suite.metadata.clone(),
        };

        info!(summary = %result.summary(), "evaluation completed");
        result
    }
}

/// 运行单个测试用例.
async fn run_single_case(
    target: &dyn Evaluable,
    case: &TestCase,
    ctx: &LsContext,
    config: &EvalConfig,
) -> EvalCaseResult {
    let mut last_error = None;
    let max_retries = config.max_retries;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            warn!(case = %case.id, attempt, "retrying test case");
        }

        let start = Instant::now();
        let exec_result = tokio::time::timeout(case.timeout, target.execute(ctx, case)).await;
        let latency = start.elapsed();

        match exec_result {
            Ok(Ok(output)) => {
                let score = match &case.expected {
                    Some(expected) => score_output(&output.output, expected, case.expected_type),
                    None => {
                        // 无期望输出，有输出即通过
                        if output.output.is_null() {
                            0.0
                        } else {
                            1.0
                        }
                    }
                };
                let passed = score >= 0.5;

                return EvalCaseResult {
                    case_id: case.id.clone(),
                    case_name: case.name.clone(),
                    passed,
                    score,
                    actual_output: output.output,
                    expected_output: case.expected.clone(),
                    error: None,
                    latency,
                    input_tokens: output.input_tokens,
                    output_tokens: output.output_tokens,
                    cost: output.cost,
                    details: Default::default(),
                };
            }
            Ok(Err(e)) => {
                last_error = Some(e.to_string());
                warn!(case = %case.id, error = %e, "test case failed");
            }
            Err(_) => {
                last_error = Some("timeout".into());
                warn!(case = %case.id, "test case timed out");
            }
        }

        // 非 fail_fast 时继续重试
        if config.fail_fast {
            break;
        }
    }

    // 所有重试均失败
    EvalCaseResult {
        case_id: case.id.clone(),
        case_name: case.name.clone(),
        passed: false,
        score: 0.0,
        actual_output: serde_json::Value::Null,
        expected_output: case.expected.clone(),
        error: last_error,
        latency: Duration::ZERO,
        input_tokens: 0,
        output_tokens: 0,
        cost: 0.0,
        details: Default::default(),
    }
}

use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use serde_json::json;

    struct MockEvaluable {
        name: String,
        version: String,
    }

    #[async_trait]
    impl Evaluable for MockEvaluable {
        async fn execute(&self, _ctx: &LsContext, case: &TestCase) -> LsResult<ExecutedOutput> {
            // 回声：返回输入
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(ExecutedOutput {
                output: case.input.clone(),
                latency: Duration::from_millis(10),
                input_tokens: 10,
                output_tokens: 10,
                cost: 0.001,
            })
        }

        fn target_name(&self) -> &str {
            &self.name
        }

        fn target_version(&self) -> &str {
            &self.version
        }
    }

    #[tokio::test]
    async fn test_runner_basic() {
        let target = Arc::new(MockEvaluable {
            name: "mock-agent".into(),
            version: "1.0.0".into(),
        });

        let mut suite = TestSuite::new("test-suite", "unit");
        suite.add_case(TestCase {
            id: "c1".into(),
            name: "echo-test".into(),
            input: json!("hello"),
            expected: Some(json!("hello")),
            expected_type: ExpectedType::Exact,
            ..Default::default()
        });

        let runner = EvalRunner::new(target, EvalConfig::default());
        let ctx = LsContext::with_session(LsId::new()).with_user("test");
        let result = runner.run_suite(&suite, &ctx).await;

        assert_eq!(result.total_cases, 1);
        assert_eq!(result.passed_cases, 1);
        assert!((result.overall_score - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_runner_timeout() {
        struct SlowEvaluable;

        #[async_trait]
        impl Evaluable for SlowEvaluable {
            async fn execute(&self, _ctx: &LsContext, _case: &TestCase) -> LsResult<ExecutedOutput> {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(ExecutedOutput {
                    output: json!("too late"),
                    latency: Duration::from_secs(60),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost: 0.0,
                })
            }

            fn target_name(&self) -> &str { "slow" }
            fn target_version(&self) -> &str { "1.0" }
        }

        let target = Arc::new(SlowEvaluable);
        let mut suite = TestSuite::new("timeout-test", "unit");
        suite.add_case(TestCase {
            id: "slow1".into(),
            name: "should-timeout".into(),
            input: json!("ping"),
            expected: None,
            timeout: Duration::from_millis(10),
            ..Default::default()
        });

        let runner = EvalRunner::new(target, EvalConfig::default());
        let ctx = LsContext::with_session(LsId::new()).with_user("test");
        let result = runner.run_suite(&suite, &ctx).await;

        assert_eq!(result.total_cases, 1);
        assert_eq!(result.passed_cases, 0);
        assert!(result.case_results[0].error.as_deref() == Some("timeout"));
    }

    #[test]
    fn test_numeric_range_score() {
        let range = json!({"min": 0, "max": 100});
        let score = score_numeric_range(&json!(50), &range);
        assert!((score - 1.0).abs() < 1e-6, "midpoint should score 1.0, got {score}");

        let score = score_numeric_range(&json!(0), &range);
        assert!((score - 0.0).abs() < 1e-6, "boundary should score 0.0, got {score}");

        let score = score_numeric_range(&json!(-1), &range);
        assert!((score - 0.0).abs() < 1e-6, "below min should score 0.0, got {score}");
    }
}
