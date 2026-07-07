use crate::api::client::{self, HealthResponse, PluginListResponse, VersionInfo};
use crate::components::status_card::StatusCard;
use yew::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
struct DashboardData {
    health: Option<HealthResponse>,
    version: Option<VersionInfo>,
    plugins: Option<PluginListResponse>,
    error: String,
}

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
    let data = use_state(DashboardData::default);

    {
        let data = data.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let mut d = DashboardData::default();
                // Fetch health & version
                match client::get_health().await {
                    Ok(h) => d.health = Some(h),
                    Err(e) => d.error = e,
                }
                match client::get_version().await {
                    Ok(v) => d.version = Some(v),
                    Err(e) => {
                        let err = d.error.clone();
                        d.error = if err.is_empty() {
                            format!("version: {e}")
                        } else {
                            format!("{err}; version: {e}")
                        };
                    }
                }
                // Fetch plugins
                match client::get_plugins().await {
                    Ok(p) => d.plugins = Some(p),
                    Err(e) => {
                        let err = d.error.clone();
                        d.error = if err.is_empty() {
                            format!("plugins: {e}")
                        } else {
                            format!("{err}; plugins: {e}")
                        };
                    }
                }
                data.set(d);
            });
            || ()
        });
    }

    let (status, version_str, uptime) = match (data.health.as_ref(), data.version.as_ref()) {
        (Some(h), Some(v)) => (
            if h.status == "ok" { "ok" } else { "warn" },
            v.version.clone(),
            h.uptime.clone(),
        ),
        _ => ("warn", "—".to_string(), "—".to_string()),
    };

    let plugin_count = data.plugins.as_ref().map(|p| p.total).unwrap_or(0);
    let active_plugins = data
        .plugins
        .as_ref()
        .map(|p| {
            p.plugins
                .iter()
                .filter(|pl| pl.status == "running" || pl.status == "active")
                .count()
        })
        .unwrap_or(0);

    html! {
        <div class="page">
            <h1 class="page-title">{ "📊 Dashboard" }</h1>
            if !data.error.is_empty() {
                <div class="error-banner">{ data.error.as_str() }</div>
            }
            <div class="cards-row">
                <StatusCard title="System Status"   value={if status == "ok" { "Healthy" } else { "Degraded" }.to_string()} status={Some(status.to_string())} icon={Some("🟢".to_string())} subtitle={Some(uptime)} />
                <StatusCard title="Version"         value={version_str}   status={Some("ok".to_string())} icon={Some("🏷️".to_string())} />
                <StatusCard title="Plugins"         value={plugin_count.to_string()} status={Some("ok".to_string())} icon={Some("🧩".to_string())} subtitle={Some(format!("{} active", active_plugins))} />
                <StatusCard title="Active Sessions" value={"—".to_string()} status={Some("ok".to_string())} icon={Some("👤".to_string())} />
                <StatusCard title="Agents"          value={"—".to_string()} status={Some("ok".to_string())} icon={Some("🤖".to_string())} />
            </div>

            if plugin_count > 0 {
                <div class="section">
                    <h2>{ "🧩 Installed Plugins" }</h2>
                    <div class="plugin-mini-list">
                        { for data.plugins.as_ref().map(|p| &p.plugins).into_iter().flatten().map(|pl| {
                            let status_class = if pl.status == "running" || pl.status == "active" { "p-status-on" } else { "p-status-off" };
                            html! {
                                <div class="plugin-mini-item">
                                    <span class={status_class}></span>
                                    <span class="p-name">{ &pl.name }</span>
                                    <span class="p-version">{ &pl.version }</span>
                                    <span class={if pl.status == "running" || pl.status == "active" { "p-badge p-badge-on" } else { "p-badge p-badge-off" }}>{ &pl.status }</span>
                                </div>
                            }
                        }) }
                    </div>
                </div>
            }

            <div class="section">
                <h2>{ "📈 Quick Links" }</h2>
                <div class="quick-links">
                    <div class="ql-card">
                        <span class="ql-icon">{"🌐"}</span>
                        <span class="ql-text">{"Federation"}</span>
                        <span class="ql-desc">{"View cluster status and peers"}</span>
                    </div>
                    <div class="ql-card">
                        <span class="ql-icon">{"📋"}</span>
                        <span class="ql-text">{"Eval Reports"}</span>
                        <span class="ql-desc">{"Browse evaluation results"}</span>
                    </div>
                    <div class="ql-card">
                        <span class="ql-icon">{"🧩"}</span>
                        <span class="ql-text">{"Plugins"}</span>
                        <span class="ql-desc">{"Manage plugins and hot reload"}</span>
                    </div>
                    <div class="ql-card">
                        <span class="ql-icon">{"📖"}</span>
                        <span class="ql-text">{"API Docs"}</span>
                        <span class="ql-desc">{"View API documentation"}</span>
                    </div>
                </div>
            </div>

            <style>
                {r##"
.page { margin-left: 240px; padding: 2rem; }
.page-title { font-size: 1.5rem; margin-bottom: 1.5rem; color: #c9d1d9; }
.error-banner { background: #f8514933; color: #f85149; padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1rem; }
.cards-row { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 2rem; }
.section { margin-top: 1rem; }
.section h2 { font-size: 1.1rem; color: #c9d1d9; margin-bottom: 1rem; }
.plugin-mini-list { display: flex; flex-direction: column; gap: 0.3rem; max-width: 500px; }
.plugin-mini-item {
  display: flex; align-items: center; gap: 0.5rem;
  background: #161b22; border: 1px solid #30363d; border-radius: 6px;
  padding: 0.5rem 0.8rem;
}
.p-status-on { width: 8px; height: 8px; border-radius: 50%; background: #3fb950; flex-shrink: 0; }
.p-status-off { width: 8px; height: 8px; border-radius: 50%; background: #6e7681; flex-shrink: 0; }
.p-name { font-weight: 600; color: #c9d1d9; flex: 1; }
.p-version { font-size: 0.8rem; color: #6e7681; }
.p-badge { font-size: 0.7rem; padding: 0.1rem 0.4rem; border-radius: 4px; }
.p-badge-on { background: #3fb95033; color: #3fb950; }
.p-badge-off { background: #6e768133; color: #6e7681; }
.quick-links { display: flex; gap: 1rem; flex-wrap: wrap; }
.ql-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1.2rem; width: 200px;
  transition: border-color 0.15s;
}
.ql-card:hover { border-color: #58a6ff; }
.ql-icon { font-size: 1.5rem; }
.ql-text { font-weight: 600; color: #c9d1d9; display: block; }
.ql-desc  { font-size: 0.8rem; color: #6e7681; }
                "##}
            </style>
        </div>
    }
}
