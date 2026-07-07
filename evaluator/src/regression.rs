//! LSEvaluator — 回归检测.
//!
//! 将当前评测结果与基线结果对比，检测性能退化。

use crate::types::*;
use std::path::Path;
use std::time::Duration;

/// 回归检测器.
pub struct RegressionDetector;

impl RegressionDetector {
    /// 对比当前结果与基线，检测回归.
    pub fn detect(
        current: &EvaluationResult,
        baseline: &EvaluationResult,
        thresholds: &RegressionThresholds,
    ) -> RegressionResult {
        let score_delta = current.overall_score - baseline.overall_score;
        let pass_rate_delta = current.pass_rate() - baseline.pass_rate();
        let latency_delta =
            duration_diff(&current.metrics.avg_latency, &baseline.metrics.avg_latency);
        let cost_delta = current.metrics.total_cost - baseline.metrics.total_cost;

        // 逐用例对比
        let mut comparisons = Vec::new();
        let mut has_regression = false;

        for cur_case in &current.case_results {
            let base_case = baseline
                .case_results
                .iter()
                .find(|b| b.case_id == cur_case.case_id);

            let status = match base_case {
                Some(base) => {
                    let base_passed = base.passed;
                    let cur_passed = cur_case.passed;
                    match (base_passed, cur_passed) {
                        (true, true) => ComparisonStatus::BothPassed,
                        (false, false) => ComparisonStatus::BothFailed,
                        (true, false) => {
                            has_regression = true;
                            ComparisonStatus::Regression
                        }
                        (false, true) => ComparisonStatus::Improvement,
                    }
                }
                None => ComparisonStatus::New,
            };

            comparisons.push(CaseComparison {
                case_id: cur_case.case_id.clone(),
                case_name: cur_case.case_name.clone(),
                baseline_passed: base_case.map(|b| b.passed),
                current_passed: cur_case.passed,
                baseline_score: base_case.map(|b| b.score),
                current_score: cur_case.score,
                status,
            });
        }

        // 阈值判断
        let exceeds_threshold = score_delta < -thresholds.max_score_degradation
            || pass_rate_delta < -thresholds.max_pass_rate_degradation
            || latency_delta > thresholds.max_latency_increase
            || cost_delta > thresholds.max_cost_increase;

        RegressionResult {
            has_regression: has_regression || exceeds_threshold,
            current_id: current.id,
            baseline_id: Some(baseline.id),
            score_delta,
            pass_rate_delta,
            latency_delta,
            cost_delta,
            comparisons,
        }
    }

    /// 从文件加载基线结果.
    pub fn load_baseline(path: impl AsRef<Path>) -> Result<EvaluationResult, String> {
        let data = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("failed to read baseline: {e}"))?;
        serde_json::from_str(&data).map_err(|e| format!("failed to parse baseline: {e}"))
    }

    /// 将当前结果保存为基线.
    pub fn save_baseline(result: &EvaluationResult, path: impl AsRef<Path>) -> Result<(), String> {
        let data = serde_json::to_string_pretty(result)
            .map_err(|e| format!("failed to serialize baseline: {e}"))?;
        std::fs::write(path.as_ref(), data)
            .map_err(|e| format!("failed to write baseline: {e}"))?;
        Ok(())
    }
}

/// 回归检测阈值.
#[derive(Debug, Clone)]
pub struct RegressionThresholds {
    /// 最大允许得分下降.
    pub max_score_degradation: f64,
    /// 最大允许通过率下降.
    pub max_pass_rate_degradation: f64,
    /// 最大允许延迟增加.
    pub max_latency_increase: Duration,
    /// 最大允许成本增加.
    pub max_cost_increase: f64,
}

impl Default for RegressionThresholds {
    fn default() -> Self {
        Self {
            max_score_degradation: 0.05,
            max_pass_rate_degradation: 0.02,
            max_latency_increase: Duration::from_millis(500),
            max_cost_increase: 0.01,
        }
    }
}

/// 计算 Duration 差值（current - baseline，负值表示变快）.
fn duration_diff(current: &Duration, baseline: &Duration) -> Duration {
    let diff_nanos =
        current.as_nanos().max(baseline.as_nanos()) - baseline.as_nanos().min(current.as_nanos());
    Duration::from_nanos(diff_nanos as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use std::time::Duration;

    fn make_result(
        id: &str,
        overall_score: f64,
        pass_rate: f64,
        cases: Vec<(bool, f64, &str)>,
    ) -> EvaluationResult {
        let total = cases.len();
        let passed = cases.iter().filter(|c| c.0).count();
        EvaluationResult {
            id: LsId::new(),
            suite_name: id.into(),
            target_name: "mock".into(),
            target_version: "1.0".into(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: Duration::ZERO,
            total_cases: total,
            passed_cases: passed,
            failed_cases: total - passed,
            overall_score,
            weighted_score: overall_score,
            metrics: MetricsSummary {
                accuracy: pass_rate,
                avg_latency: Duration::from_millis(100),
                total_cost: 0.01,
                ..Default::default()
            },
            case_results: cases
                .into_iter()
                .map(|(passed, score, name)| EvalCaseResult {
                    case_id: name.into(),
                    case_name: name.into(),
                    passed,
                    score,
                    latency: Duration::from_millis(100),
                    input_tokens: 10,
                    output_tokens: 10,
                    cost: 0.001,
                    ..default_case_result(name)
                })
                .collect(),
            metadata: Default::default(),
        }
    }

    fn default_case_result(case_id: &str) -> EvalCaseResult {
        EvalCaseResult {
            case_id: case_id.into(),
            case_name: case_id.into(),
            passed: false,
            score: 0.0,
            actual_output: serde_json::Value::Null,
            expected_output: None,
            error: None,
            latency: Duration::ZERO,
            input_tokens: 0,
            output_tokens: 0,
            cost: 0.0,
            details: Default::default(),
        }
    }

    #[test]
    fn test_regression_detected() {
        let baseline = make_result(
            "baseline",
            0.95,
            1.0,
            vec![(true, 1.0, "c1"), (true, 0.9, "c2")],
        );

        let current = make_result(
            "current",
            0.45,
            0.5,
            vec![(true, 0.9, "c1"), (false, 0.0, "c2")],
        );

        let result =
            RegressionDetector::detect(&current, &baseline, &RegressionThresholds::default());
        assert!(result.has_regression, "should detect regression");
        assert!(result.score_delta < 0.0, "score should decrease");
    }

    #[test]
    fn test_no_regression() {
        let baseline = make_result(
            "baseline",
            0.9,
            1.0,
            vec![(true, 1.0, "c1"), (true, 0.8, "c2")],
        );

        let current = make_result(
            "current",
            0.95,
            1.0,
            vec![(true, 1.0, "c1"), (true, 0.9, "c2")],
        );

        let result =
            RegressionDetector::detect(&current, &baseline, &RegressionThresholds::default());
        assert!(!result.has_regression, "should not detect regression");
        assert!(result.score_delta > 0.0, "score should increase");
    }

    #[test]
    fn test_save_load_baseline() {
        let dir = std::env::temp_dir().join("lingshu-regression-test");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("baseline.json");

        let result = make_result("test", 0.8, 0.8, vec![(true, 0.8, "c1")]);
        RegressionDetector::save_baseline(&result, &path).unwrap();

        let loaded = RegressionDetector::load_baseline(&path).unwrap();
        assert_eq!(loaded.suite_name, result.suite_name);
        assert!((loaded.overall_score - result.overall_score).abs() < 1e-6);

        std::fs::remove_dir_all(&dir).ok();
    }
}
