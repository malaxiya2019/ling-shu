//! LSEvaluator — 评测指标计算.
//!
//! 提供准确率/精确率/召回率/F1、延迟百分位、成本核算等指标计算。

use crate::types::{EvalCaseResult, MetricsSummary};
use std::time::Duration;

/// 从一组用例结果计算指标汇总.
pub fn compute_metrics(results: &[EvalCaseResult]) -> MetricsSummary {
    if results.is_empty() {
        return MetricsSummary::default();
    }

    let n = results.len() as f64;
    let passed = results.iter().filter(|r| r.passed).count() as f64;
    let accuracy = passed / n;

    // 精确率 & 召回率: 将 score >= 0.5 视为正例
    let tp = results.iter().filter(|r| r.score >= 0.5 && r.passed).count() as f64;
    let fp = results.iter().filter(|r| r.score >= 0.5 && !r.passed).count() as f64;
    let fn_val = results.iter().filter(|r| r.score < 0.5 && r.passed).count() as f64;

    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if tp + fn_val > 0.0 { tp / (tp + fn_val) } else { 0.0 };
    let f1_score = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    // 延迟统计
    let mut latencies: Vec<Duration> = results.iter().map(|r| r.latency).collect();
    latencies.sort();

    let avg_latency = if n > 0.0 {
        let total_nanos: u128 = latencies.iter().map(|d| d.as_nanos()).sum();
        Duration::from_nanos((total_nanos / n as u128) as u64)
    } else {
        Duration::ZERO
    };

    let p50_latency = percentile(&latencies, 50);
    let p95_latency = percentile(&latencies, 95);
    let p99_latency = percentile(&latencies, 99);

    // Token 统计
    let avg_input_tokens = results.iter().map(|r| r.input_tokens as f64).sum::<f64>() / n;
    let avg_output_tokens = results.iter().map(|r| r.output_tokens as f64).sum::<f64>() / n;
    let total_tokens: u64 = results.iter().map(|r| r.input_tokens + r.output_tokens).sum();

    // 成本统计
    let total_cost: f64 = results.iter().map(|r| r.cost).sum();
    let avg_cost = total_cost / n;

    MetricsSummary {
        accuracy,
        precision,
        recall,
        f1_score,
        avg_latency,
        p50_latency,
        p95_latency,
        p99_latency,
        avg_input_tokens,
        avg_output_tokens,
        total_tokens,
        total_cost,
        avg_cost,
    }
}

/// 计算升序排列的延迟列表的百分位值.
fn percentile(sorted: &[Duration], p: usize) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((p as f64 / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = idx.saturating_sub(1).min(sorted.len().saturating_sub(1));
    sorted[idx]
}

/// 评分函数：精确匹配.
pub fn score_exact(actual: &serde_json::Value, expected: &serde_json::Value) -> f64 {
    if actual == expected {
        1.0
    } else {
        // 字符串比较时忽略末尾空白
        if let (Some(a_str), Some(e_str)) = (actual.as_str(), expected.as_str()) {
            if a_str.trim() == e_str.trim() {
                return 1.0;
            }
        }
        0.0
    }
}

/// 评分函数：包含匹配.
pub fn score_contains(actual: &serde_json::Value, expected: &serde_json::Value) -> f64 {
    let actual_str = match actual.as_str() {
        Some(s) => s.to_lowercase(),
        None => return score_exact(actual, expected),
    };
    let expected_str = match expected.as_str() {
        Some(s) => s.to_lowercase(),
        None => return score_exact(actual, expected),
    };
    if actual_str.contains(&expected_str) {
        1.0
    } else {
        0.0
    }
}

/// 评分函数：JSON 结构匹配（忽略值，只比类型和结构）.
pub fn score_json_structure(actual: &serde_json::Value, expected: &serde_json::Value) -> f64 {
    match (actual, expected) {
        // 都是 Null
        (serde_json::Value::Null, serde_json::Value::Null) => 1.0,
        // 都是 Bool
        (serde_json::Value::Bool(_), serde_json::Value::Bool(_)) => 1.0,
        // 都是 Number
        (serde_json::Value::Number(_), serde_json::Value::Number(_)) => 1.0,
        // 都是 String
        (serde_json::Value::String(_), serde_json::Value::String(_)) => 1.0,
        // 都是 Array — 递归比较每个元素结构
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            if a.len() != b.len() {
                // 长度不同，按比例降分
                let min_len = a.len().min(b.len());
                if min_len == 0 {
                    return 0.0;
                }
                let matched: f64 = a
                    .iter()
                    .zip(b.iter())
                    .map(|(x, y)| score_json_structure(x, y))
                    .sum();
                matched / a.len().max(b.len()) as f64
            } else if a.is_empty() {
                1.0
            } else {
                a.iter()
                    .zip(b.iter())
                    .map(|(x, y)| score_json_structure(x, y))
                    .sum::<f64>()
                    / a.len() as f64
            }
        }
        // 都是 Object — 递归比较每个键的结构
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            let mut total = 0.0_f64;
            let mut count = 0_usize;
            for (key, val_a) in a.iter() {
                if let Some(val_b) = b.get(key) {
                    total += score_json_structure(val_a, val_b);
                    count += 1;
                }
                // 缺失键得 0 分
            }
            // Object 中多余键不计分
            if count == 0 {
                return 0.0;
            }
            total / count as f64
        }
        // 类型不同
        _ => 0.0,
    }
}

/// 评分函数：正则匹配.
pub fn score_regex(actual: &serde_json::Value, pattern: &str) -> f64 {
    let actual_str = match actual.as_str() {
        Some(s) => s,
        None => return 0.0,
    };
    match regex_lite(pattern) {
        Ok(re) => {
            if re.is_match(actual_str) {
                1.0
            } else {
                0.0
            }
        }
        Err(_) => 0.0,
    }
}

/// 简易正则匹配（不使用 regex crate 依赖）.
fn regex_lite(pattern: &str) -> Result<RegexLite, String> {
    RegexLite::new(pattern)
}

struct RegexLite {
    pattern: String,
}

impl RegexLite {
    fn new(pattern: &str) -> Result<Self, String> {
        if pattern.is_empty() {
            return Err("empty pattern".into());
        }
        Ok(Self {
            pattern: pattern.to_string(),
        })
    }

    fn is_match(&self, text: &str) -> bool {
        // 简易实现：支持 * 和 ? 通配符匹配
        self.wildcard_match(text)
    }

    fn wildcard_match(&self, text: &str) -> bool {
        let pattern = self.pattern.as_bytes();
        let text = text.as_bytes();
        let mut p_idx = 0;
        let mut t_idx = 0;
        let mut star_idx = None;
        let mut match_idx = 0;

        while t_idx < text.len() {
            if p_idx < pattern.len()
                && (pattern[p_idx] == text[t_idx] || pattern[p_idx] == b'?')
            {
                p_idx += 1;
                t_idx += 1;
            } else if p_idx < pattern.len() && pattern[p_idx] == b'*' {
                star_idx = Some(p_idx);
                match_idx = t_idx;
                p_idx += 1;
            } else if let Some(si) = star_idx {
                p_idx = si + 1;
                match_idx += 1;
                t_idx = match_idx;
            } else {
                return false;
            }
        }

        while p_idx < pattern.len() && pattern[p_idx] == b'*' {
            p_idx += 1;
        }

        p_idx == pattern.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_score_exact() {
        assert!((score_exact(&json!("hello"), &json!("hello")) - 1.0).abs() < 1e-6);
        assert!((score_exact(&json!("hello"), &json!("world")) - 0.0).abs() < 1e-6);
        assert!((score_exact(&json!(42), &json!(42)) - 1.0).abs() < 1e-6);
        assert!((score_exact(&json!(true), &json!(false)) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_score_contains() {
        assert!((score_contains(&json!("hello world"), &json!("world")) - 1.0).abs() < 1e-6);
        assert!((score_contains(&json!("hello"), &json!("world")) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_score_json_structure() {
        let actual = json!({"name": "Alice", "age": 30});
        let expected = json!({"name": "Bob", "age": 25});
        let score = score_json_structure(&actual, &expected);
        assert!((score - 1.0).abs() < 1e-6, "structure should match regardless of values");

        let actual = json!({"name": "Alice", "extra": true});
        let expected = json!({"name": "Bob"});
        let score = score_json_structure(&actual, &expected);
        assert!((score - 1.0).abs() < 1e-6, "extra keys in actual are ignored");
    }

    #[test]
    fn test_compute_metrics_empty() {
        let m = compute_metrics(&[]);
        assert!((m.accuracy - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_metrics() {
        use std::time::Duration;
        let results = vec![
            EvalCaseResult {
                passed: true,
                score: 1.0,
                latency: Duration::from_millis(100),
                input_tokens: 10,
                output_tokens: 20,
                cost: 0.001,
                ..default_case_result("c1", "test1")
            },
            EvalCaseResult {
                passed: true,
                score: 0.8,
                latency: Duration::from_millis(200),
                input_tokens: 20,
                output_tokens: 30,
                cost: 0.002,
                ..default_case_result("c2", "test2")
            },
            EvalCaseResult {
                passed: false,
                score: 0.3,
                latency: Duration::from_millis(150),
                input_tokens: 15,
                output_tokens: 25,
                cost: 0.0015,
                ..default_case_result("c3", "test3")
            },
        ];

        let m = compute_metrics(&results);
        assert!((m.accuracy - 2.0 / 3.0).abs() < 1e-6);
        assert!((m.avg_input_tokens - 15.0).abs() < 1e-6);
        assert!((m.avg_output_tokens - 25.0).abs() < 1e-6);
        assert!((m.total_cost - 0.0045).abs() < 1e-6);
    }

    fn default_case_result(case_id: &str, case_name: &str) -> EvalCaseResult {
        EvalCaseResult {
            case_id: case_id.to_string(),
            case_name: case_name.to_string(),
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
}
