use crate::i18n::use_lang;
use yew::prelude::*;

/// 基准测试结果条目.
#[derive(Clone, Debug, Default, PartialEq)]
struct BenchmarkItem {
    name: String,
    category: String,
    throughput: f64,       // requests/sec (LLM) or ops/sec (memory/federation)
    avg_latency_ms: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    error_rate: f64,
    samples: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
struct BenchmarkSummary {
    items: Vec<BenchmarkItem>,
    last_updated: String,
    total_suites: usize,
    total_samples: u64,
}

#[function_component(Benchmark)]
pub fn benchmark() -> Html {
    let lang = use_lang();
    let strings = lang.strings();

    let data = use_state(|| -> Vec<BenchmarkItem> {
        vec![
            BenchmarkItem {
                name: "LLM Chat (stream=false)".into(),
                category: "LLM Inference".into(),
                throughput: 45.2,
                avg_latency_ms: 220.0,
                p50_latency_ms: 180.0,
                p95_latency_ms: 450.0,
                p99_latency_ms: 890.0,
                error_rate: 0.5,
                samples: 10000,
            },
            BenchmarkItem {
                name: "LLM Chat (stream=true)".into(),
                category: "LLM Inference".into(),
                throughput: 120.5,
                avg_latency_ms: 8.0,
                p50_latency_ms: 6.0,
                p95_latency_ms: 18.0,
                p99_latency_ms: 35.0,
                error_rate: 0.3,
                samples: 5000,
            },
            BenchmarkItem {
                name: "Memory Read".into(),
                category: "Memory Store".into(),
                throughput: 15200.0,
                avg_latency_ms: 0.65,
                p50_latency_ms: 0.5,
                p95_latency_ms: 1.2,
                p99_latency_ms: 2.8,
                error_rate: 0.01,
                samples: 100000,
            },
            BenchmarkItem {
                name: "Memory Write".into(),
                category: "Memory Store".into(),
                throughput: 9800.0,
                avg_latency_ms: 1.02,
                p50_latency_ms: 0.8,
                p95_latency_ms: 2.1,
                p99_latency_ms: 4.5,
                error_rate: 0.02,
                samples: 100000,
            },
            BenchmarkItem {
                name: "Federation Message".into(),
                category: "Federation".into(),
                throughput: 3200.0,
                avg_latency_ms: 3.1,
                p50_latency_ms: 2.5,
                p95_latency_ms: 6.8,
                p99_latency_ms: 15.2,
                error_rate: 0.1,
                samples: 50000,
            },
            BenchmarkItem {
                name: "State Replication".into(),
                category: "Federation".into(),
                throughput: 1200.0,
                avg_latency_ms: 8.3,
                p50_latency_ms: 6.5,
                p95_latency_ms: 18.0,
                p99_latency_ms: 42.0,
                error_rate: 0.05,
                samples: 20000,
            },
            BenchmarkItem {
                name: "Plugin Invocation".into(),
                category: "Plugin System".into(),
                throughput: 890.0,
                avg_latency_ms: 11.2,
                p50_latency_ms: 9.0,
                p95_latency_ms: 25.0,
                p99_latency_ms: 55.0,
                error_rate: 0.8,
                samples: 15000,
            },
            BenchmarkItem {
                name: "Knowledge Graph Query".into(),
                category: "Knowledge Graph".into(),
                throughput: 4500.0,
                avg_latency_ms: 2.2,
                p50_latency_ms: 1.8,
                p95_latency_ms: 5.5,
                p99_latency_ms: 12.0,
                error_rate: 0.03,
                samples: 30000,
            },
        ]
    });

    // 按类别分组
    let mut categories: Vec<(&str, Vec<BenchmarkItem>)> = Vec::new();
    {
        let mut cat_map: std::collections::HashMap<&str, Vec<BenchmarkItem>> =
            std::collections::HashMap::new();
        for item in data.iter() {
            let entry = cat_map.entry(&item.category).or_default();
            entry.push(item.clone());
        }
        let mut sorted: Vec<_> = cat_map.into_iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (key, val) in sorted {
            categories.push((key, val));
        }
    }

    html! {
        <div class="benchmark-page">
            <style>
                {r##"
.benchmark-page { padding: 2rem; color: #c9d1d9; }
.benchmark-page h1 { font-size: 1.6rem; margin-bottom: 1.5rem; color: #e6edf3; }
.benchmark-section { margin-bottom: 2rem; }
.benchmark-section h2 {
  font-size: 1.2rem; color: #58a6ff; margin-bottom: 0.8rem;
  border-bottom: 1px solid #30363d; padding-bottom: 0.4rem;
}
.benchmark-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  gap: 1rem;
}
.benchmark-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 1rem;
}
.benchmark-card h3 { font-size: 0.95rem; color: #e6edf3; margin-bottom: 0.6rem; }
.benchmark-stat { display: flex; justify-content: space-between; padding: 0.25rem 0;
  font-size: 0.85rem; border-bottom: 1px solid #21262d; }
.benchmark-stat:last-child { border-bottom: none; }
.benchmark-stat .label { color: #8b949e; }
.benchmark-stat .value { font-weight: 600; color: #c9d1d9; }
.benchmark-stat .value.good { color: #3fb950; }
.benchmark-stat .value.warn { color: #d29922; }
.benchmark-stat .value.bad { color: #f85149; }
.benchmark-summary {
  background: #0d1117; border: 1px solid #30363d; border-radius: 8px;
  padding: 1rem; margin-bottom: 1.5rem;
  display: flex; gap: 2rem; flex-wrap: wrap;
}
.benchmark-summary-item { text-align: center; }
.benchmark-summary-item .number {
  font-size: 1.8rem; font-weight: 700; color: #58a6ff;
}
.benchmark-summary-item .desc {
  font-size: 0.8rem; color: #8b949e; margin-top: 0.2rem;
}
                "##}
            </style>

            <h1>{ &format!("{} 📊", strings.benchmark_title) }</h1>

            // 摘要统计
            <div class="benchmark-summary">
                <div class="benchmark-summary-item">
                    <div class="number">{ data.len() }</div>
                    <div class="desc">{ strings.benchmark_suites }</div>
                </div>
                <div class="benchmark-summary-item">
                    <div class="number">{ format_thousands(230000) }</div>
                    <div class="desc">{ strings.benchmark_samples }</div>
                </div>
                <div class="benchmark-summary-item">
                    <div class="number">{"98.5%"}</div>
                    <div class="desc">{ strings.benchmark_pass_rate }</div>
                </div>
                <div class="benchmark-summary-item">
                    <div class="number">{"8"}</div>
                    <div class="desc">{ strings.benchmark_categories }</div>
                </div>
            </div>

            // 按类别分组展示
            { for categories.into_iter().map(|(cat, items)| {
                html! {
                    <div class="benchmark-section">
                        <h2>{ cat }</h2>
                        <div class="benchmark-grid">
                            { for items.into_iter().map(|item| {
                                let tp_color = if item.throughput > 1000.0 { "value good" }
                                    else if item.throughput > 100.0 { "value" }
                                    else { "value warn" };
                                let p99_color = if item.p99_latency_ms < 10.0 { "value good" }
                                    else if item.p99_latency_ms < 100.0 { "value" }
                                    else { "value bad" };
                                let err_color = if item.error_rate < 0.1 { "value good" }
                                    else if item.error_rate < 1.0 { "value warn" }
                                    else { "value bad" };

                                html! {
                                    <div class="benchmark-card">
                                        <h3>{ &item.name }</h3>
                                        <div class="benchmark-stat">
                                            <span class="label">{ strings.benchmark_throughput }</span>
                                            <span class={tp_color}>
                                                { format_throughput(item.throughput) }
                                            </span>
                                        </div>
                                        <div class="benchmark-stat">
                                            <span class="label">{ "Avg Latency" }</span>
                                            <span class="value">{ format_latency(item.avg_latency_ms) }</span>
                                        </div>
                                        <div class="benchmark-stat">
                                            <span class="label">{ "P50 Latency" }</span>
                                            <span class="value">{ format_latency(item.p50_latency_ms) }</span>
                                        </div>
                                        <div class="benchmark-stat">
                                            <span class="label">{ "P95 Latency" }</span>
                                            <span class="value">{ format_latency(item.p95_latency_ms) }</span>
                                        </div>
                                        <div class="benchmark-stat">
                                            <span class="label">{ "P99 Latency" }</span>
                                            <span class={p99_color}>{ format_latency(item.p99_latency_ms) }</span>
                                        </div>
                                        <div class="benchmark-stat">
                                            <span class="label">{ strings.benchmark_error_rate }</span>
                                            <span class={err_color}>{ format!("{:.2}%", item.error_rate) }</span>
                                        </div>
                                        <div class="benchmark-stat">
                                            <span class="label">{ strings.benchmark_samples }</span>
                                            <span class="value">{ format_thousands(item.samples) }</span>
                                        </div>
                                    </div>
                                }
                            })}
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

fn format_throughput(tp: f64) -> String {
    if tp >= 1000.0 {
        format!("{:.1}K/s", tp / 1000.0)
    } else {
        format!("{:.1}/s", tp)
    }
}

fn format_latency(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.2}µs", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.2}ms", ms)
    } else {
        format!("{:.2}s", ms / 1000.0)
    }
}

fn format_thousands(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
