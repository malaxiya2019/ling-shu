//! LSEvaluator — 评测报告生成.
//!
//! 支持 JSON 和 Markdown 格式的报告输出。

use crate::types::{EvaluationResult, RegressionResult, ReportFormat};
use std::path::PathBuf;

/// 报告生成器.
pub struct ReportGenerator {
    /// 输出目录.
    output_dir: PathBuf,
}

impl ReportGenerator {
    /// 创建报告生成器.
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
        }
    }

    /// 生成报告到文件.
    pub fn generate(
        &self,
        result: &EvaluationResult,
        regression: Option<&RegressionResult>,
        formats: &[ReportFormat],
    ) -> std::io::Result<Vec<PathBuf>> {
        std::fs::create_dir_all(&self.output_dir)?;
        let mut paths = Vec::new();

        for fmt in formats {
            let path = match fmt {
                ReportFormat::Json => {
                    let p = self.output_dir.join("evaluation_report.json");
                    let report = self.render_json(result, regression);
                    std::fs::write(&p, report)?;
                    p
                }
                ReportFormat::Markdown => {
                    let p = self.output_dir.join("evaluation_report.md");
                    let report = self.render_markdown(result, regression);
                    std::fs::write(&p, report)?;
                    p
                }
                ReportFormat::Html => {
                    // HTML 暂不支持
                    tracing::warn!("HTML report format not yet implemented");
                    continue;
                }
            };
            paths.push(path);
        }

        Ok(paths)
    }

    /// 渲染 JSON 报告.
    pub fn render_json(
        &self,
        result: &EvaluationResult,
        regression: Option<&RegressionResult>,
    ) -> String {
        let mut report = serde_json::json!({
            "id": result.id.to_string(),
            "suite": result.suite_name,
            "target": {
                "name": result.target_name,
                "version": result.target_version,
            },
            "time": {
                "started_at": result.started_at.to_rfc3339(),
                "completed_at": result.completed_at.to_rfc3339(),
                "duration_ms": result.total_duration.as_millis(),
            },
            "summary": {
                "total_cases": result.total_cases,
                "passed_cases": result.passed_cases,
                "failed_cases": result.failed_cases,
                "pass_rate": result.pass_rate(),
                "overall_score": result.overall_score,
                "weighted_score": result.weighted_score,
            },
            "metrics": {
                "accuracy": result.metrics.accuracy,
                "precision": result.metrics.precision,
                "recall": result.metrics.recall,
                "f1_score": result.metrics.f1_score,
                "avg_latency_ms": result.metrics.avg_latency.as_millis(),
                "p50_latency_ms": result.metrics.p50_latency.as_millis(),
                "p95_latency_ms": result.metrics.p95_latency.as_millis(),
                "p99_latency_ms": result.metrics.p99_latency.as_millis(),
                "avg_input_tokens": result.metrics.avg_input_tokens,
                "avg_output_tokens": result.metrics.avg_output_tokens,
                "total_tokens": result.metrics.total_tokens,
                "total_cost_usd": result.metrics.total_cost,
            },
            "cases": result.case_results.iter().map(|c| serde_json::json!({
                "case_id": c.case_id,
                "case_name": c.case_name,
                "passed": c.passed,
                "score": c.score,
                "latency_ms": c.latency.as_millis(),
                "input_tokens": c.input_tokens,
                "output_tokens": c.output_tokens,
                "cost_usd": c.cost,
                "error": c.error,
            })).collect::<Vec<_>>(),
        });

        if let Some(reg) = regression {
            report["regression"] = serde_json::json!({
                "has_regression": reg.has_regression,
                "score_delta": reg.score_delta,
                "pass_rate_delta": reg.pass_rate_delta,
                "latency_delta_ms": reg.latency_delta.as_millis(),
                "cost_delta": reg.cost_delta,
            });
        }

        serde_json::to_string_pretty(&report).unwrap_or_default()
    }

    /// 渲染 Markdown 报告.
    pub fn render_markdown(
        &self,
        result: &EvaluationResult,
        regression: Option<&RegressionResult>,
    ) -> String {
        let mut md = String::new();

        // 标题
        md.push_str(&format!("# 📊 评测报告: {}\n\n", result.suite_name));
        md.push_str(&format!("**目标**: `{}` v{}  \n", result.target_name, result.target_version));
        md.push_str(&format!(
            "**时间**: {} — {}  \n",
            result.started_at.format("%Y-%m-%d %H:%M:%S"),
            result.completed_at.format("%Y-%m-%d %H:%M:%S"),
        ));
        md.push_str(&format!("**耗时**: {:?}\n\n", result.total_duration));

        // 概要
        md.push_str("## 📈 概要\n\n");
        md.push_str(&format!(
            "| 指标 | 值 |\n|---|---|\n\
             | 总用例 | {} |\n\
             | ✅ 通过 | {} |\n\
             | ❌ 失败 | {} |\n\
             | 通过率 | {:.1}% |\n\
             | 总体得分 | {:.3} |\n\
             | 加权得分 | {:.3} |\n",
            result.total_cases,
            result.passed_cases,
            result.failed_cases,
            result.pass_rate() * 100.0,
            result.overall_score,
            result.weighted_score,
        ));

        // 指标
        md.push_str("\n## 📊 指标\n\n");
        md.push_str(&format!(
            "| 指标 | 值 |\n|---|---|\n\
             | Accuracy | {:.3} |\n\
             | Precision | {:.3} |\n\
             | Recall | {:.3} |\n\
             | F1 Score | {:.3} |\n\
             | 平均延迟 | {:?} |\n\
             | P50 延迟 | {:?} |\n\
             | P95 延迟 | {:?} |\n\
             | P99 延迟 | {:?} |\n\
             | 平均输入 Token | {:.1} |\n\
             | 平均输出 Token | {:.1} |\n\
             | 总 Token 数 | {} |\n\
             | 总成本 | ${:.4} |\n",
            result.metrics.accuracy,
            result.metrics.precision,
            result.metrics.recall,
            result.metrics.f1_score,
            result.metrics.avg_latency,
            result.metrics.p50_latency,
            result.metrics.p95_latency,
            result.metrics.p99_latency,
            result.metrics.avg_input_tokens,
            result.metrics.avg_output_tokens,
            result.metrics.total_tokens,
            result.metrics.total_cost,
        ));

        // 回归检测
        if let Some(reg) = regression {
            md.push_str("\n## 🔄 回归检测\n\n");
            let icon = if reg.has_regression { "⚠️" } else { "✅" };
            md.push_str(&format!("**状态**: {}\n\n", icon));
            md.push_str(&format!(
                "| 指标 | 变化 |\n|---|---|\n\
                 | 得分变化 | {:.3} |\n\
                 | 通过率变化 | {:.1}% |\n\
                 | 延迟变化 | {:?} |\n\
                 | 成本变化 | ${:.4} |\n",
                reg.score_delta,
                reg.pass_rate_delta * 100.0,
                reg.latency_delta,
                reg.cost_delta,
            ));

            if reg.has_regression {
                md.push_str("\n### 回归用例\n\n");
                md.push_str("| 用例 | 基线 | 当前 |\n|---|---|---|\n");
                for comp in &reg.comparisons {
                    if matches!(comp.status, crate::types::ComparisonStatus::Regression) {
                        md.push_str(&format!(
                            "| {} | ✅ ({:.2}) | ❌ ({:.2}) |\n",
                            comp.case_name,
                            comp.baseline_score.unwrap_or(0.0),
                            comp.current_score,
                        ));
                    }
                }
            }
        }

        // 用例详情
        md.push_str("\n## 📋 用例详情\n\n");
        md.push_str("| # | 用例 | 状态 | 得分 | 延迟 | Token | 成本 |\n|---|---|---|---|---|---|---|\n");
        for (i, c) in result.case_results.iter().enumerate() {
            let status = if c.passed { "✅" } else { "❌" };
            md.push_str(&format!(
                "| {} | {} | {} | {:.2} | {:?} | {}+{} | ${:.4} |\n",
                i + 1,
                c.case_name,
                status,
                c.score,
                c.latency,
                c.input_tokens,
                c.output_tokens,
                c.cost,
            ));
        }

        // 失败详情
        let failed: Vec<_> = result.case_results.iter().filter(|c| !c.passed).collect();
        if !failed.is_empty() {
            md.push_str("\n## ❌ 失败详情\n\n");
            for c in &failed {
                md.push_str(&format!("### {} ({})\n\n", c.case_name, c.case_id));
                if let Some(err) = &c.error {
                    md.push_str(&format!("**错误**: `{}`\n\n", err));
                }
            }
        }

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingshu_core::LsId;
    use crate::types::*;
    use std::time::Duration;

    fn sample_result() -> EvaluationResult {
        EvaluationResult {
            id: LsId::new(),
            suite_name: "test-suite".into(),
            target_name: "mock".into(),
            target_version: "1.0".into(),
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            total_duration: Duration::from_secs(1),
            total_cases: 2,
            passed_cases: 1,
            failed_cases: 1,
            overall_score: 0.5,
            weighted_score: 0.5,
            metrics: MetricsSummary {
                accuracy: 0.5,
                precision: 1.0,
                recall: 0.5,
                f1_score: 0.666,
                avg_latency: Duration::from_millis(100),
                p50_latency: Duration::from_millis(50),
                p95_latency: Duration::from_millis(200),
                p99_latency: Duration::from_millis(300),
                avg_input_tokens: 50.0,
                avg_output_tokens: 100.0,
                total_tokens: 300,
                total_cost: 0.005,
                avg_cost: 0.0025,
            },
            case_results: vec![
                EvalCaseResult {
                    case_id: "c1".into(),
                    case_name: "passing-test".into(),
                    passed: true,
                    score: 1.0,
                    actual_output: serde_json::json!("ok"),
                    expected_output: Some(serde_json::json!("ok")),
                    error: None,
                    latency: Duration::from_millis(50),
                    input_tokens: 10,
                    output_tokens: 20,
                    cost: 0.001,
                    details: Default::default(),
                },
                EvalCaseResult {
                    case_id: "c2".into(),
                    case_name: "failing-test".into(),
                    passed: false,
                    score: 0.0,
                    actual_output: serde_json::json!("wrong"),
                    expected_output: Some(serde_json::json!("correct")),
                    error: Some("mismatch".into()),
                    latency: Duration::from_millis(150),
                    input_tokens: 15,
                    output_tokens: 25,
                    cost: 0.002,
                    details: Default::default(),
                },
            ],
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_json_report() {
        let gen = ReportGenerator::new("/tmp");
        let result = sample_result();
        let json = gen.render_json(&result, None);
        assert!(json.contains("passing-test"));
        assert!(json.contains("failing-test"));
        assert!(json.contains("\"overall_score\": 0.5"));
    }

    #[test]
    fn test_markdown_report() {
        let gen = ReportGenerator::new("/tmp");
        let result = sample_result();
        let md = gen.render_markdown(&result, None);
        assert!(md.contains("passing-test"));
        assert!(md.contains("failing-test"));
        assert!(md.contains("50.0%"));
    }

    #[test]
    fn test_generate_files() {
        let dir = std::env::temp_dir().join("lingshu-eval-test");
        let gen = ReportGenerator::new(&dir);
        let result = sample_result();
        let paths = gen.generate(&result, None, &[ReportFormat::Json]).unwrap();
        assert!(!paths.is_empty());
        assert!(paths[0].exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
