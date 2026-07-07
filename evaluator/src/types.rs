//! LSEvaluator — 评测核心类型.
//!
//! ## 数据模型
//!
//! ```text
//! ┌──────────────┐       ┌──────────────────┐
//! │  TestSuite   │ ──1:N→│   TestCase       │
//! │ (名称/分类)   │       │ (输入/期望/权重)  │
//! └──────────────┘       └────────┬─────────┘
//!                                  │ 运行后
//!                                  ▼
//!                         ┌──────────────────┐
//!                         │ EvaluationResult │
//!                         │ (输出/得分/延迟)  │
//!                         └──────────────────┘
//! ```

use lingshu_core::LsId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

// ── 评测用例 ───────────────────────────────────────

/// 测试用例 — 单个评测样本.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// 唯一标识.
    pub id: String,
    /// 用例名称.
    pub name: String,
    /// 用例描述.
    pub description: String,
    /// 分类标签.
    pub tags: Vec<String>,
    /// 输入数据（传递给 Agent/LLM）.
    pub input: serde_json::Value,
    /// 期望输出（用于自动评分）.
    pub expected: Option<serde_json::Value>,
    /// 期望输出类型.
    pub expected_type: ExpectedType,
    /// 用例权重（影响加权总分）.
    pub weight: f64,
    /// 超时时间.
    pub timeout: Duration,
    /// 自定义评分函数（JSON 路径）.
    pub scorer_path: Option<String>,
}

impl Default for TestCase {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            tags: Vec::new(),
            input: serde_json::Value::Null,
            expected: None,
            expected_type: ExpectedType::Exact,
            weight: 1.0,
            timeout: Duration::from_secs(60),
            scorer_path: None,
        }
    }
}

/// 期望输出匹配类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpectedType {
    /// 精确匹配.
    Exact,
    /// 语义相似（LLM 判断）.
    Semantic,
    /// 包含子串.
    Contains,
    /// JSON 结构匹配.
    JsonStructure,
    /// 正则匹配.
    Regex,
    /// 数值范围.
    NumericRange,
    /// 自定义评分函数.
    Custom,
}

impl fmt::Display for ExpectedType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Semantic => write!(f, "semantic"),
            Self::Contains => write!(f, "contains"),
            Self::JsonStructure => write!(f, "json_structure"),
            Self::Regex => write!(f, "regex"),
            Self::NumericRange => write!(f, "numeric_range"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

// ── 评测套件 ───────────────────────────────────────

/// 测试套件 — 一组相关测试用例的集合.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// 套件唯一标识.
    pub id: LsId,
    /// 套件名称.
    pub name: String,
    /// 套件描述.
    pub description: String,
    /// 分类.
    pub category: String,
    /// 用例列表.
    pub cases: Vec<TestCase>,
    /// 元数据.
    pub metadata: HashMap<String, String>,
    /// 创建时间.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TestSuite {
    /// 创建空套件.
    pub fn new(name: &str, category: &str) -> Self {
        Self {
            id: LsId::new(),
            name: name.to_string(),
            description: String::new(),
            category: category.to_string(),
            cases: Vec::new(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// 添加测试用例.
    pub fn add_case(&mut self, case: TestCase) {
        self.cases.push(case);
    }

    /// 用例数量.
    pub fn len(&self) -> usize {
        self.cases.len()
    }

    /// 是否为空.
    pub fn is_empty(&self) -> bool {
        self.cases.is_empty()
    }
}

// ── 评测结果 ───────────────────────────────────────

/// 单个测试用例的评测结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCaseResult {
    /// 对应 TestCase.id.
    pub case_id: String,
    /// 用例名称.
    pub case_name: String,
    /// 是否通过.
    pub passed: bool,
    /// 得分 (0.0 ~ 1.0).
    pub score: f64,
    /// Agent/LLM 实际输出.
    pub actual_output: serde_json::Value,
    /// 期望输出.
    pub expected_output: Option<serde_json::Value>,
    /// 错误信息.
    pub error: Option<String>,
    /// 延迟.
    pub latency: Duration,
    /// 输入 Token 数.
    pub input_tokens: u64,
    /// 输出 Token 数.
    pub output_tokens: u64,
    /// 成本（美元）.
    pub cost: f64,
    /// 详细评分信息.
    pub details: HashMap<String, serde_json::Value>,
}

/// 完整评测结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// 评测唯一标识.
    pub id: LsId,
    /// 套件名称.
    pub suite_name: String,
    /// 目标名称（Agent/Model 名称）.
    pub target_name: String,
    /// 目标版本.
    pub target_version: String,
    /// 开始时间.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// 结束时间.
    pub completed_at: chrono::DateTime<chrono::Utc>,
    /// 总延迟.
    pub total_duration: Duration,
    /// 总用例数.
    pub total_cases: usize,
    /// 通过数.
    pub passed_cases: usize,
    /// 失败数.
    pub failed_cases: usize,
    /// 总体得分 (0.0 ~ 1.0).
    pub overall_score: f64,
    /// 加权得分.
    pub weighted_score: f64,
    /// 汇总指标.
    pub metrics: MetricsSummary,
    /// 单个用例结果.
    pub case_results: Vec<EvalCaseResult>,
    /// 元数据.
    pub metadata: HashMap<String, String>,
}

impl EvaluationResult {
    /// 通过率.
    pub fn pass_rate(&self) -> f64 {
        if self.total_cases == 0 {
            return 1.0;
        }
        self.passed_cases as f64 / self.total_cases as f64
    }

    /// 生成摘要文本.
    pub fn summary(&self) -> String {
        format!(
            "📊 {} v{} — {:.1}% passed ({}/{}) | score: {:.3} | latency: {:?} | cost: ${:.4}",
            self.target_name,
            self.target_version,
            self.pass_rate() * 100.0,
            self.passed_cases,
            self.total_cases,
            self.overall_score,
            self.total_duration,
            self.cost()
        )
    }

    /// 总成本.
    pub fn cost(&self) -> f64 {
        self.case_results.iter().map(|r| r.cost).sum()
    }
}

// ── 指标汇总 ───────────────────────────────────────

/// 评测指标汇总.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// 准确率 (Accuracy).
    pub accuracy: f64,
    /// 精确率 (Precision).
    pub precision: f64,
    /// 召回率 (Recall).
    pub recall: f64,
    /// F1 分数.
    pub f1_score: f64,
    /// 平均延迟.
    pub avg_latency: Duration,
    /// P50 延迟.
    pub p50_latency: Duration,
    /// P95 延迟.
    pub p95_latency: Duration,
    /// P99 延迟.
    pub p99_latency: Duration,
    /// 平均输入 Token.
    pub avg_input_tokens: f64,
    /// 平均输出 Token.
    pub avg_output_tokens: f64,
    /// 总 Token 数.
    pub total_tokens: u64,
    /// 总成本（美元）.
    pub total_cost: f64,
    /// 平均成本.
    pub avg_cost: f64,
}

impl Default for MetricsSummary {
    fn default() -> Self {
        Self {
            accuracy: 0.0,
            precision: 0.0,
            recall: 0.0,
            f1_score: 0.0,
            avg_latency: Duration::ZERO,
            p50_latency: Duration::ZERO,
            p95_latency: Duration::ZERO,
            p99_latency: Duration::ZERO,
            avg_input_tokens: 0.0,
            avg_output_tokens: 0.0,
            total_tokens: 0,
            total_cost: 0.0,
            avg_cost: 0.0,
        }
    }
}

// ── 评测配置 ───────────────────────────────────────

/// 评测运行配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    /// 并行度.
    pub concurrency: usize,
    /// 是否启用语义评分（需要 LLM）.
    pub enable_semantic_scoring: bool,
    /// 是否保存实际输出.
    pub save_outputs: bool,
    /// 失败时是否停止.
    pub fail_fast: bool,
    /// 重试次数.
    pub max_retries: u32,
    /// 基线文件路径（回归检测）.
    pub baseline_path: Option<String>,
    /// 报告输出目录.
    pub output_dir: Option<String>,
    /// 报告格式.
    pub report_formats: Vec<ReportFormat>,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            enable_semantic_scoring: false,
            save_outputs: true,
            fail_fast: false,
            max_retries: 0,
            baseline_path: None,
            output_dir: None,
            report_formats: vec![ReportFormat::Json, ReportFormat::Markdown],
        }
    }
}

/// 报告格式.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    Json,
    Markdown,
    Html,
}

impl fmt::Display for ReportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json => write!(f, "json"),
            Self::Markdown => write!(f, "markdown"),
            Self::Html => write!(f, "html"),
        }
    }
}

// ── 回归检测结果 ───────────────────────────────────

/// 回归检测结果.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionResult {
    /// 是否检测到回归.
    pub has_regression: bool,
    /// 当前评测结果 ID.
    pub current_id: LsId,
    /// 基线结果 ID.
    pub baseline_id: Option<LsId>,
    /// 得分变化.
    pub score_delta: f64,
    /// 通过率变化.
    pub pass_rate_delta: f64,
    /// 延迟变化.
    pub latency_delta: Duration,
    /// 成本变化.
    pub cost_delta: f64,
    /// 详细对比.
    pub comparisons: Vec<CaseComparison>,
}

/// 单个用例对比.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseComparison {
    pub case_id: String,
    pub case_name: String,
    pub baseline_passed: Option<bool>,
    pub current_passed: bool,
    pub baseline_score: Option<f64>,
    pub current_score: f64,
    pub status: ComparisonStatus,
}

/// 对比状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonStatus {
    /// 都通过.
    BothPassed,
    /// 都失败.
    BothFailed,
    /// 基线通过，当前失败（回归）.
    Regression,
    /// 基线失败，当前通过（改进）.
    Improvement,
    /// 新用例（无基线）.
    New,
}

impl fmt::Display for ComparisonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BothPassed => write!(f, "✓"),
            Self::BothFailed => write!(f, "✗"),
            Self::Regression => write!(f, "↓ REGRESSION"),
            Self::Improvement => write!(f, "↑ improvement"),
            Self::New => write!(f, "● new"),
        }
    }
}
