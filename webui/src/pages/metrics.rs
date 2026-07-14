use crate::api::client;
use yew::prelude::*;

const HISTORY_LEN: usize = 60; // 5 min at 5s intervals

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct MetricSample {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct MetricsResponse {
    pub cpu_usage: f64,
    pub memory_mb: f64,
    pub active_sessions: u64,
    pub total_agents: u64,
    pub llm_requests_total: u64,
    pub llm_tokens_total: u64,
    pub federation_nodes: u64,
    pub uptime_secs: u64,
    #[serde(default)]
    pub custom_metrics: Vec<MetricSample>,
}

#[derive(Clone, Debug, PartialEq)]
struct DataPoint {
    cpu: f64,
    mem: f64,
    tokens: u64,
    reqs: u64,
}

#[function_component(Metrics)]
pub fn metrics() -> Html {
    let lang = crate::i18n::use_lang();
    let _strings = lang.strings();
    let current = use_state(|| None::<MetricsResponse>);
    let history: UseStateHandle<Vec<DataPoint>> = use_state(Vec::new);
    let error = use_state(String::new);

    // Fetch on mount and auto-refresh every 5 seconds
    {
        let current = current.clone();
        let history = history.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            let handle = gloo_timers::callback::Interval::new(5_000, move || {
                let current = current.clone();
                let history = history.clone();
                let error = error.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_metrics().await {
                        Ok(m) => {
                            current.set(Some(m.clone()));
                            let mut new_history = (*history).clone();
                            new_history.push(DataPoint {
                                cpu: m.cpu_usage,
                                mem: m.memory_mb,
                                tokens: m.llm_tokens_total,
                                reqs: m.llm_requests_total,
                            });
                            if new_history.len() > HISTORY_LEN {
                                new_history.remove(0);
                            }
                            history.set(new_history);
                        }
                        Err(e) => error.set(e),
                    }
                });
            });
            move || {
                handle.cancel();
            }
        });
    }

    // Initial fetch
    {
        let current = current.clone();
        let history = history.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_metrics().await {
                    Ok(m) => {
                        current.set(Some(m.clone()));
                        history.set(vec![DataPoint {
                            cpu: m.cpu_usage,
                            mem: m.memory_mb,
                            tokens: m.llm_tokens_total,
                            reqs: m.llm_requests_total,
                        }]);
                    }
                    Err(e) => error.set(e),
                }
            });
            || {}
        });
    }

    let cpu_pct = current.as_ref().map(|d| d.cpu_usage).unwrap_or(0.0);
    let mem_mb = current.as_ref().map(|d| d.memory_mb).unwrap_or(0.0);
    let sessions = current.as_ref().map(|d| d.active_sessions).unwrap_or(0);
    let agents = current.as_ref().map(|d| d.total_agents).unwrap_or(0);
    let llm_reqs = current.as_ref().map(|d| d.llm_requests_total).unwrap_or(0);
    let llm_tokens = current.as_ref().map(|d| d.llm_tokens_total).unwrap_or(0);
    let fed_nodes = current.as_ref().map(|d| d.federation_nodes).unwrap_or(0);

    let cpu_chart = render_line_chart(&history, "cpu", 0.0, 100.0, "#3fb950");
    let mem_chart = render_line_chart(&history, "mem", 0.0, 2048.0, "#58a6ff");
    let token_chart = render_token_chart(&history, "#d29922");

    html! {
        <div class="page">
            <h1 class="page-title">{ "📈 Live Metrics" }</h1>
            if !error.is_empty() {
                <div class="error-banner">{ error.as_str() }</div>
            }

            <div class="cards-row">
                <div class="metric-card gauge">
                    <div class="gauge-ring">
                        <svg viewBox="0 0 120 120" class="gauge-svg">
                            <circle cx="60" cy="60" r="50" fill="none" stroke="#21262d" stroke-width="8"/>
                            <circle cx="60" cy="60" r="50" fill="none" stroke="#3fb950" stroke-width="8"
                                stroke-dasharray={format!("{} {}", (cpu_pct.clamp(0.0, 100.0) / 100.0 * 314.0).max(0.0), 314.0)}
                                stroke-linecap="round" transform="rotate(-90 60 60)"/>
                            <text x="60" y="55" text-anchor="middle" fill="#c9d1d9" font-size="22" font-weight="bold">{format!("{:.0}%", cpu_pct)}</text>
                            <text x="60" y="72" text-anchor="middle" fill="#6e7681" font-size="10">{"CPU"}</text>
                        </svg>
                    </div>
                </div>
                <div class="metric-card gauge">
                    <div class="gauge-ring">
                        <svg viewBox="0 0 120 120" class="gauge-svg">
                            <circle cx="60" cy="60" r="50" fill="none" stroke="#21262d" stroke-width="8"/>
                            <circle cx="60" cy="60" r="50" fill="none" stroke="#58a6ff" stroke-width="8"
                                stroke-dasharray={format!("{} {}", ((mem_mb / 2048.0 * 100.0).clamp(0.0, 100.0) / 100.0 * 314.0).max(0.0), 314.0)}
                                stroke-linecap="round" transform="rotate(-90 60 60)"/>
                            <text x="60" y="55" text-anchor="middle" fill="#c9d1d9" font-size="16" font-weight="bold">{format!("{:.0}MB", mem_mb)}</text>
                            <text x="60" y="72" text-anchor="middle" fill="#6e7681" font-size="10">{"Memory"}</text>
                        </svg>
                    </div>
                </div>
                <div class="metric-card gauge">
                    <div class="gauge-ring">
                        <svg viewBox="0 0 120 120" class="gauge-svg">
                            <circle cx="60" cy="60" r="50" fill="none" stroke="#21262d" stroke-width="8"/>
                            <circle cx="60" cy="60" r="50" fill="none" stroke="#d29922" stroke-width="8"
                                stroke-dasharray={format!("{} {}", ((llm_tokens as f64 / 100000.0 * 100.0).clamp(0.0, 100.0) / 100.0 * 314.0).max(0.0), 314.0)}
                                stroke-linecap="round" transform="rotate(-90 60 60)"/>
                            <text x="60" y="55" text-anchor="middle" fill="#c9d1d9" font-size="16" font-weight="bold">{format_tokens(llm_tokens)}</text>
                            <text x="60" y="72" text-anchor="middle" fill="#6e7681" font-size="10">{"Tokens"}</text>
                        </svg>
                    </div>
                </div>
            </div>

            <div class="cards-row">
                <div class="stat-card">
                    <span class="stat-icon">{"👤"}</span>
                    <span class="stat-value">{sessions}</span>
                    <span class="stat-label">{"Active Sessions"}</span>
                </div>
                <div class="stat-card">
                    <span class="stat-icon">{"🤖"}</span>
                    <span class="stat-value">{agents}</span>
                    <span class="stat-label">{"Agents"}</span>
                </div>
                <div class="stat-card">
                    <span class="stat-icon">{"🌐"}</span>
                    <span class="stat-value">{fed_nodes}</span>
                    <span class="stat-label">{"Federation Nodes"}</span>
                </div>
                <div class="stat-card">
                    <span class="stat-icon">{"💬"}</span>
                    <span class="stat-value">{llm_reqs}</span>
                    <span class="stat-label">{"LLM Requests"}</span>
                </div>
                <div class="stat-card">
                    <span class="stat-icon">{"🔤"}</span>
                    <span class="stat-value">{format_tokens(llm_tokens)}</span>
                    <span class="stat-label">{"Tokens"}</span>
                </div>
            </div>

            // ── Real-time Charts ──
            <div class="charts-row">
                <div class="chart-card">
                    <h3 class="chart-title">{"CPU Usage (%) — Last 5 min"}</h3>
                    <div class="chart-container">
                        {cpu_chart}
                    </div>
                </div>
                <div class="chart-card">
                    <h3 class="chart-title">{"Memory Usage (MB) — Last 5 min"}</h3>
                    <div class="chart-container">
                        {mem_chart}
                    </div>
                </div>
            </div>
            <div class="charts-row">
                <div class="chart-card chart-card-wide">
                    <h3 class="chart-title">{"Token Usage — Last 5 min"}</h3>
                    <div class="chart-container">
                        {token_chart}
                    </div>
                </div>
            </div>

            {if let Some(ref resp) = *current {
                if !resp.custom_metrics.is_empty() {
                    html! {
                        <div class="section">
                            <h2>{ "📊 Custom Metrics" }</h2>
                            <table class="metrics-table">
                                <thead><tr><th>{ "Metric" }</th><th>{ "Value" }</th><th>{ "Labels" }</th></tr></thead>
                                <tbody>
                                    { for resp.custom_metrics.iter().map(|m| {
                                        let labels_str = m.labels.iter()
                                            .map(|(k, v)| format!("{k}={v}"))
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        html! {
                                            <tr>
                                                <td><code>{ &m.name }</code></td>
                                                <td class="metric-value">{ format!("{:.4}", m.value) }</td>
                                                <td class="metric-labels">{ labels_str }</td>
                                            </tr>
                                        }
                                    })}
                                </tbody>
                            </table>
                        </div>
                    }
                } else {
                    html! {}
                }
            } else {
                html! { <div class="loading">{ "Loading metrics..." }</div> }
            }}
        </div>
    }
}

// ── SVG Line Chart Rendering ───────────────────────

/// Render an SVG line chart for a single metric series.
fn render_line_chart(
    data: &[DataPoint],
    metric: &str,
    y_min: f64,
    y_max: f64,
    color: &str,
) -> Html {
    if data.len() < 2 {
        return html! { <div class="chart-placeholder">{ "Collecting data..." }</div> };
    }

    let w = 400.0;
    let h = 140.0;
    let pad_l = 40.0;
    let pad_r = 10.0;
    let pad_t = 10.0;
    let pad_b = 20.0;
    let plot_w = w - pad_l - pad_r;
    let plot_h = h - pad_t - pad_b;

    let range = if (y_max - y_min).abs() < f64::EPSILON {
        1.0
    } else {
        y_max - y_min
    };
    let n = data.len();

    let points: Vec<(f64, f64)> = data
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let val = match metric {
                "cpu" => d.cpu,
                "mem" => d.mem,
                _ => d.cpu,
            };
            let x = pad_l + (i as f64 / (n - 1).max(1) as f64) * plot_w;
            let y = pad_t + plot_h - ((val - y_min) / range * plot_h);
            (x, y)
        })
        .collect();

    // Clip to plot area
    let path_d = build_smooth_path(&points);
    let area_d = format!(
        "{} L {} {} L {} {} Z",
        path_d,
        pad_l + plot_w,
        pad_t + plot_h,
        pad_l,
        pad_t + plot_h
    );

    let y_ticks = 4;
    let y_labels: Vec<Html> = (0..=y_ticks).map(|i| {
        let val = y_min + (range * i as f64 / y_ticks as f64);
        let y = pad_t + plot_h - (i as f64 / y_ticks as f64) * plot_h;
        html! {
            <>
                <line x1={pad_l.to_string()} y1={y.to_string()} x2={(pad_l + plot_w).to_string()} y2={y.to_string()}
                    stroke="#21262d" stroke-width="1"/>
                <text x="35" y={(y + 3.0).to_string()} text-anchor="end" fill="#6e7681" font-size="9">
                    {format!("{:.0}", val)}
                </text>
            </>
        }
    }).collect();

    html! {
        <svg viewBox={format!("0 0 {} {}", w, h)} xmlns="http://www.w3.org/2000/svg">
            // Grid & Y-axis labels
            { for y_labels }
            // Area fill
            <path d={area_d} fill={color.to_owned()} fill-opacity="0.1"/>
            // Line
            <path d={path_d} fill="none" stroke={color.to_owned()} stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
    }
}

/// Render token chart with its own scale
fn render_token_chart(data: &[DataPoint], color: &str) -> Html {
    if data.len() < 2 {
        return html! { <div class="chart-placeholder">{ "Collecting data..." }</div> };
    }

    let w = 860.0;
    let h = 140.0;
    let pad_l = 50.0;
    let pad_r = 10.0;
    let pad_t = 10.0;
    let pad_b = 20.0;
    let plot_w = w - pad_l - pad_r;
    let plot_h = h - pad_t - pad_b;

    let min_t = data.iter().map(|d| d.tokens).min().unwrap_or(0);
    let max_t = data.iter().map(|d| d.tokens).max().unwrap_or(1);
    let range: f64 = if max_t == min_t {
        1.0
    } else {
        (max_t - min_t) as f64
    };
    let n = data.len();

    let points: Vec<(f64, f64)> = data
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let x = pad_l + (i as f64 / (n - 1).max(1) as f64) * plot_w;
            let y = pad_t + plot_h - ((d.tokens - min_t) as f64 / range * plot_h);
            (x, y)
        })
        .collect();

    let path_d = build_smooth_path(&points);
    let area_d = format!(
        "{} L {} {} L {} {} Z",
        path_d,
        pad_l + plot_w,
        pad_t + plot_h,
        pad_l,
        pad_t + plot_h
    );

    let y_ticks = 4;
    let y_labels: Vec<Html> = (0..=y_ticks).map(|i| {
        let val = min_t as f64 + range * i as f64 / y_ticks as f64;
        let y = pad_t + plot_h - (i as f64 / y_ticks as f64) * plot_h;
        let label = if val >= 1_000_000.0 {
            format!("{:.1}M", val / 1_000_000.0)
        } else if val >= 1_000.0 {
            format!("{:.1}K", val / 1_000.0)
        } else {
            format!("{:.0}", val)
        };
        html! {
            <>
                <line x1={pad_l.to_string()} y1={y.to_string()} x2={(pad_l + plot_w).to_string()} y2={y.to_string()}
                    stroke="#21262d" stroke-width="1"/>
                <text x="45" y={(y + 3.0).to_string()} text-anchor="end" fill="#6e7681" font-size="9">{label}</text>
            </>
        }
    }).collect();

    html! {
        <svg viewBox={format!("0 0 {} {}", w, h)} xmlns="http://www.w3.org/2000/svg">
            { for y_labels }
            <path d={area_d} fill={color.to_owned()} fill-opacity="0.1"/>
            <path d={path_d} fill="none" stroke={color.to_owned()} stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
    }
}

/// Build an SVG path from points using smooth cubic beziers.
fn build_smooth_path(points: &[(f64, f64)]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = format!("M {} {}", points[0].0, points[0].1);
    for i in 1..points.len() {
        let prev = points[i - 1];
        let curr = points[i];
        let cpx1 = prev.0 + (curr.0 - prev.0) * 0.4;
        let cpy1 = prev.1;
        let cpx2 = curr.0 - (curr.0 - prev.0) * 0.4;
        let cpy2 = curr.1;
        d.push_str(&format!(
            " C {} {} {} {} {} {}",
            cpx1, cpy1, cpx2, cpy2, curr.0, curr.1
        ));
    }
    d
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

async fn fetch_metrics() -> Result<MetricsResponse, String> {
    client::get_json::<MetricsResponse>("/v1/metrics").await
}
