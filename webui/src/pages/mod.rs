pub mod dashboard;
pub mod federation;
pub mod eval_reports;
pub mod metrics;

pub use dashboard::Dashboard;
pub use federation::Federation;
pub use eval_reports::EvalReports;
pub use metrics::Metrics;

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Dashboard,
    Federation,
    EvalReports,
    Metrics,
}
