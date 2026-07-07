use yew::prelude::*;
use crate::api::client::{self, HealthResponse, VersionInfo};
use crate::components::status_card::StatusCard;

#[function_component(Dashboard)]
pub fn dashboard() -> Html {
    let health = use_state(|| None::<HealthResponse>);
    let version = use_state(|| None::<VersionInfo>);
    let error = use_state(String::new);

    {
        let health = health.clone();
        let version = version.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match client::get_health().await {
                    Ok(h) => health.set(Some(h)),
                    Err(e) => error.set(e),
                }
                match client::get_version().await {
                    Ok(v) => version.set(Some(v)),
                    Err(e) => error.set(format!("version: {e}")),
                }
            });
            || ()
        });
    }

    let (status, version_str, uptime) = match (health.as_ref(), version.as_ref()) {
        (Some(h), Some(v)) => (
            if h.status == "ok" { "ok" } else { "warn" },
            v.version.clone(),
            h.uptime.clone(),
        ),
        _ => ("warn", "—".to_string(), "—".to_string()),
    };

    html! {
        <div class="page">
            <h1 class="page-title">{ "📊 Dashboard" }</h1>
            if !error.is_empty() {
                <div class="error-banner">{ error.as_str() }</div>
            }
            <div class="cards-row">
                <StatusCard title="System Status"   value={if status == "ok" { "Healthy" } else { "Degraded" }.to_string()} status={Some(status.to_string())} icon={Some("🟢".to_string())} subtitle={Some(uptime)} />
                <StatusCard title="Version"         value={version_str}   status={Some("ok".to_string())} icon={Some("🏷️".to_string())} />
                <StatusCard title="Active Sessions" value={"—".to_string()} status={Some("ok".to_string())} icon={Some("👤".to_string())} />
                <StatusCard title="Agents"          value={"—".to_string()} status={Some("ok".to_string())} icon={Some("🤖".to_string())} />
            </div>

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
