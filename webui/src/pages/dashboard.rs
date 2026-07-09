use crate::api::client::{self, HealthResponse, PluginListResponse, VersionInfo};
use crate::components::status_card::StatusCard;
use crate::i18n::use_lang;
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
    let lang = use_lang();
    let strings = lang.strings();
    let data = use_state(DashboardData::default);

    {
        let data = data.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let mut d = DashboardData::default();
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

    let (status, status_label, version_str, uptime) =
        match (data.health.as_ref(), data.version.as_ref()) {
            (Some(h), Some(v)) => (
                if h.status == "ok" { "ok" } else { "warn" },
                if h.status == "ok" {
                    strings.dash_healthy
                } else {
                    strings.dash_degraded
                },
                v.version.clone(),
                h.uptime.clone(),
            ),
            _ => ("warn", strings.dash_degraded, "—".to_string(), "—".to_string()),
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

    let active_label = strings.dash_active_count.replace("{}", &active_plugins.to_string());

    html! {
        <div class="page">
            <h1 class="page-title">{ strings.dash_title }</h1>
            if !data.error.is_empty() {
                <div class="error-banner">{ data.error.as_str() }</div>
            }
            <div class="cards-row">
                <StatusCard
                    title={strings.dash_system_status.to_string()}
                    value={status_label.to_string()}
                    status={Some(status.to_string())}
                    icon={Some(if status == "ok" { "🟢".to_string() } else { "🟡".to_string() })}
                    subtitle={Some(uptime)}
                />
                <StatusCard
                    title={"Version".to_string()}
                    value={version_str}
                    status={Some("ok".to_string())}
                    icon={Some("🏷️".to_string())}
                />
                <StatusCard
                    title={strings.dash_plugins.to_string()}
                    value={plugin_count.to_string()}
                    status={Some("ok".to_string())}
                    icon={Some("🧩".to_string())}
                    subtitle={Some(active_label)}
                />
                <StatusCard
                    title={strings.dash_active_sessions.to_string()}
                    value={"—".to_string()}
                    status={Some("ok".to_string())}
                    icon={Some("👤".to_string())}
                />
                <StatusCard
                    title={strings.dash_agents.to_string()}
                    value={"—".to_string()}
                    status={Some("ok".to_string())}
                    icon={Some("🤖".to_string())}
                />
            </div>

            if plugin_count > 0 {
                <div class="section">
                    <h2>{ strings.dash_installed_plugins }</h2>
                    <div class="plugin-mini-list">
                        { for data.plugins.as_ref().map(|p| &p.plugins).into_iter().flatten().map(|pl| {
                            let is_on = pl.status == "running" || pl.status == "active";
                            let status_class = if is_on { "p-status-on" } else { "p-status-off" };
                            let badge_class = if is_on { "p-badge p-badge-on" } else { "p-badge p-badge-off" };
                            html! {
                                <div class="plugin-mini-item">
                                    <span class={status_class}></span>
                                    <span class="p-name">{ &pl.name }</span>
                                    <span class="p-version">{ &pl.version }</span>
                                    <span class={badge_class}>{ &pl.status }</span>
                                </div>
                            }
                        }) }
                    </div>
                </div>
            }

            <div class="section">
                <h2>{ strings.dash_quick_links }</h2>
                <div class="quick-links">
                    <div class="ql-card">
                        <span class="ql-icon">{"🌐"}</span>
                        <span class="ql-text">{ strings.dash_ql_federation }</span>
                        <span class="ql-desc">{ strings.dash_ql_federation_desc }</span>
                    </div>
                    <div class="ql-card">
                        <span class="ql-icon">{"📋"}</span>
                        <span class="ql-text">{ strings.dash_ql_eval }</span>
                        <span class="ql-desc">{ strings.dash_ql_eval_desc }</span>
                    </div>
                    <div class="ql-card">
                        <span class="ql-icon">{"🧩"}</span>
                        <span class="ql-text">{ strings.dash_ql_plugins }</span>
                        <span class="ql-desc">{ strings.dash_ql_plugins_desc }</span>
                    </div>
                    <div class="ql-card">
                        <span class="ql-icon">{"📖"}</span>
                        <span class="ql-text">{ strings.dash_ql_api_docs }</span>
                        <span class="ql-desc">{ strings.dash_ql_api_docs_desc }</span>
                    </div>
                </div>
            </div>

        </div>
    }
}
