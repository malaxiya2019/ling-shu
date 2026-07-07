//! LSEvaluator — Lingshu Agent 评测框架.
//!
//! 提供测试套件定义、评测运行器、指标计算、报告生成和回归检测功能。
//!
//! ## 架构
//!
//! ```text
//! ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
//! │  TestSuite   │ →  │  EvalRunner  │ →  │ EvalResult   │
//! │  (用例集合)   │    │  (运行器)    │    │  (评测结果)   │
//! └──────────────┘    └──────────────┘    └──────┬───────┘
//!                                                │
//!                          ┌─────────────────────┼──────────┐
//!                          ▼                     ▼          ▼
//!                   ┌────────────┐     ┌────────────┐ ┌──────────┐
//!                   │  Metrics   │     │  Report    │ │Regression│
//!                   │  (指标计算) │     │  (报告生成) │ │(回归检测) │
//!                   └────────────┘     └────────────┘ └──────────┘
//! ```
//!
//! ## 快速开始
//!
//! ```ignore
//! use lingshu_evaluator::*;
//!
//! // 定义测试套件
//! let mut suite = TestSuite::new("数学测试", "math");
//! suite.add_case(TestCase {
//!     id: "add-1".into(),
//!     name: "1+1=2".into(),
//!     input: json!("1+1=?"),
//!     expected: Some(json!("2")),
//!     expected_type: ExpectedType::Exact,
//!     ..Default::default()
//! });
//!
//! // 创建运行器并执行评测
//! let runner = EvalRunner::new(target, EvalConfig::default());
//! let result = runner.run_suite(&suite, &ctx).await;
//!
//! // 生成报告
//! let gen = ReportGenerator::new("./reports");
//! gen.generate(&result, None, &[ReportFormat::Json, ReportFormat::Markdown]);
//!
//! // 回归检测
//! let regression = RegressionDetector::detect(&result, &baseline, &RegressionThresholds::default());
//! ```

pub mod metrics;
pub mod regression;
pub mod report;
pub mod runner;
pub mod types;

pub use metrics::compute_metrics;
pub use regression::{RegressionDetector, RegressionThresholds};
pub use report::ReportGenerator;
pub use runner::{score_output, EvalRunner, Evaluable, ExecutedOutput};
pub use types::*;
