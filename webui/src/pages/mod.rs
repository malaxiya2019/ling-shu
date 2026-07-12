pub mod audit;
pub mod benchmark;
pub mod dashboard;
pub mod eval_reports;
pub mod federation;
pub mod metrics;
pub mod plugins;
pub mod security;
pub mod tenant;

pub use audit::AuditDashboard;
pub use benchmark::Benchmark;
pub use dashboard::Dashboard;
pub use eval_reports::EvalReports;
pub use federation::Federation;
pub use metrics::Metrics;
pub use plugins::Plugins;
pub use security::Security;
pub use tenant::TenantDashboard;

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Audit,
    Dashboard,
    Federation,
    EvalReports,
    Metrics,
    Plugins,
    Security,
    Benchmark,
    Tenant,
}
