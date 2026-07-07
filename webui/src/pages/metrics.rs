use crate::api::client;
use yew::prelude::*;

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

#[function_component(Metrics)]
pub fn metrics() -> Html {
    let lang = crate::i18n::use_lang();
    let strings = lang.strings;
    let data = use_state(|| None::<MetricsResponse>);
    let error = use_state(String::new);

    // Fetch on mount
    {
        let data = data.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match fetch_metrics().await {
                    Ok(m) => data.set(Some(m)),
                    Err(e) => error.set(e),
                }
            });
            || {}
        });
    }

    // Auto-refresh every 5 seconds via gloo timer
    {
        let data = data.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            let handle = gloo_timers::callback::Interval::new(5_000, move || {
                let data = data.clone();
                let error = error.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_metrics().await {
                        Ok(m) => data.set(Some(m)),
                        Err(e) => error.set(e),
                    }
                });
            });
            move || {
                handle.cancel();
            }
        });
    }

    let cpu_pct = data.as_ref().map(|d| d.cpu_usage).unwrap_or(0.0);
    let mem_mb = data.as_ref().map(|d| d.memory_mb).unwrap_or(0.0);
    let sessions = data.as_ref().map(|d| d.active_sessions).unwrap_or(0);
    let agents = data.as_ref().map(|d| d.total_agents).unwrap_or(0);
    let llm_reqs = data.as_ref().map(|d| d.llm_requests_total).unwrap_or(0);
    let llm_tokens = data.as_ref().map(|d| d.llm_tokens_total).unwrap_or(0);
    let fed_nodes = data.as_ref().map(|d| d.federation_nodes).unwrap_or(0);

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
                                stroke-dasharray={format!("{} {}", cpu_pct.max(0.0).min(100.0) / 100.0 * 314.0, 314.0)}
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
                                stroke-dasharray={format!("{} {}", (mem_mb / 1024.0 * 100.0).max(0.0).min(100.0) / 100.0 * 314.0, 314.0)}
                                stroke-linecap="round" transform="rotate(-90 60 60)"/>
                            <text x="60" y="55" text-anchor="middle" fill="#c9d1d9" font-size="18" font-weight="bold">{format!("{:.0}MB", mem_mb)}</text>
                            <text x="60" y="72" text-anchor="middle" fill="#6e7681" font-size="10">{"Memory"}</text>
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

            {if let Some(ref resp) = *data {
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

            <style>
                {r##"
.page { margin-left: 240px; padding: 2rem; }
.page-title { font-size: 1.5rem; margin-bottom: 1.5rem; color: #c9d1d9; }
.error-banner { background: #f8514933; color: #f85149; padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1rem; }
.loading { color: #8b949e; font-size: 0.9rem; text-align: center; padding: 2rem; }
.cards-row { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1.5rem; }
.gauge { background: #161b22; border-radius: 12px; padding: 1rem; flex: 0 0 auto; }
.gauge-svg { width: 120px; height: 120px; }
.stat-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 1rem 1.2rem; min-width: 140px; text-align: center;
  transition: border-color 0.15s;
}
.stat-card:hover { border-color: #58a6ff; }
.stat-icon { font-size: 1.3rem; display: block; margin-bottom: 0.3rem; }
.stat-value { font-size: 1.5rem; font-weight: 700; color: #c9d1d9; display: block; }
.stat-label { font-size: 0.75rem; color: #8b949e; text-transform: uppercase; letter-spacing: 0.04em; }
.section { margin: 1.5rem 0; }
.section h2 { font-size: 1.1rem; color: #c9d1d9; margin-bottom: 0.8rem; }
.metrics-table { width: 100%; border-collapse: collapse; }
.metrics-table th, .metrics-table td { text-align: left; padding: 0.5rem; border-bottom: 1px solid #21262d; }
.metrics-table th { color: #58a6ff; font-size: 0.8rem; text-transform: uppercase; }
.metrics-table td { color: #c9d1d9; }
.metric-value { font-family: monospace; color: #3fb950; }
.metric-labels { font-family: monospace; color: #6e7681; font-size: 0.85em; }
.metric-card { background: #161b22; border-radius: 8px; padding: 1rem; }
                "##}
            </style>
        </div>
    }
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
    client::get_json::<MetricsResponse>("/metrics").await
}
