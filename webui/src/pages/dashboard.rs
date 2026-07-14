use crate::api::client::{
    self, AgentListResponse, HealthResponse, PluginListResponse, RuntimeStatusResponse,
    SessionListResponse, VersionInfo,
};
use crate::components::status_card::StatusCard;
use crate::i18n::use_lang;
use yew::prelude::*;

#[derive(Clone, Debug, Default, PartialEq)]
struct DashboardData {
    health: Option<HealthResponse>,
    version: Option<VersionInfo>,
    plugins: Option<PluginListResponse>,
    runtime_status: Option<RuntimeStatusResponse>,
    agents: Option<AgentListResponse>,
    sessions: Option<SessionListResponse>,
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

                // 健康检查
                match client::get_health().await {
                    Ok(h) => d.health = Some(h),
                    Err(e) => d.error = e,
                }
                // 版本信息
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
                // 插件列表
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
                // Runtime 状态（agent/session 计数）
                match client::get_runtime_status().await {
                    Ok(rs) => d.runtime_status = Some(rs),
                    Err(e) => {
                        let err = d.error.clone();
                        d.error = if err.is_empty() {
                            format!("runtime: {e}")
                        } else {
                            format!("{err}; runtime: {e}")
                        };
                    }
                }
                // Agent 列表
                match client::get_agents().await {
                    Ok(a) => d.agents = Some(a),
                    Err(e) => {
                        let err = d.error.clone();
                        d.error = if err.is_empty() {
                            format!("agents: {e}")
                        } else {
                            format!("{err}; agents: {e}")
                        };
                    }
                }
                // 会话列表
                match client::get_sessions().await {
                    Ok(s) => d.sessions = Some(s),
                    Err(e) => {
                        let err = d.error.clone();
                        d.error = if err.is_empty() {
                            format!("sessions: {e}")
                        } else {
                            format!("{err}; sessions: {e}")
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
            _ => (
                "warn",
                strings.dash_degraded,
                "—".to_string(),
                "—".to_string(),
            ),
        };

    // Runtime 状态
    let runtime_state = data
        .runtime_status
        .as_ref()
        .map(|rs| rs.state.as_str())
        .unwrap_or("—");
    let is_running = runtime_state == "Running";

    // Agent 统计
    let agent_count = data
        .runtime_status
        .as_ref()
        .map(|rs| rs.agent_count)
        .or_else(|| data.agents.as_ref().map(|a| a.agents.len()))
        .unwrap_or(0);

    let agent_status = if agent_count > 0 { "ok" } else { "warn" };

    // 会话统计
    let session_count = data
        .runtime_status
        .as_ref()
        .map(|rs| rs.session_count)
        .or_else(|| data.sessions.as_ref().map(|s| s.sessions.len()))
        .unwrap_or(0);

    let session_status = if session_count > 0 { "ok" } else { "warn" };

    // 插件统计
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

    let active_label = strings
        .dash_active_count
        .replace("{}", &active_plugins.to_string());

    // Runtime 状态标签
    let runtime_label = if is_running { "Running" } else { runtime_state };

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
                    title={"Runtime".to_string()}
                    value={runtime_label.to_string()}
                    status={Some(if is_running { "ok" } else { "warn" }.to_string())}
                    icon={Some("⚙️".to_string())}
                />
                <StatusCard
                    title={strings.dash_agents.to_string()}
                    value={agent_count.to_string()}
                    status={Some(agent_status.to_string())}
                    icon={Some("🤖".to_string())}
                />
                <StatusCard
                    title={strings.dash_active_sessions.to_string()}
                    value={session_count.to_string()}
                    status={Some(session_status.to_string())}
                    icon={Some("👤".to_string())}
                />
                <StatusCard
                    title={strings.dash_plugins.to_string()}
                    value={plugin_count.to_string()}
                    status={Some("ok".to_string())}
                    icon={Some("🧩".to_string())}
                    subtitle={Some(active_label)}
                />
            </div>

            // Agent 迷你列表
            if let Some(ref agents_resp) = data.agents {
                if !agents_resp.agents.is_empty() {
                    <div class="section">
                        <h2>{ "🤖 Agents" }</h2>
                        <div class="plugin-mini-list">
                            { for agents_resp.agents.iter().map(|a| {
                                let is_running = a.status == "Running" || a.status == "Idle";
                                let status_class = if is_running { "p-status-on" } else { "p-status-off" };
                                let badge_class = if is_running { "p-badge p-badge-on" } else { "p-badge p-badge-off" };
                                html! {
                                    <div class="plugin-mini-item">
                                        <span class={status_class}></span>
                                        <span class="p-name">{ &a.name }</span>
                                        <span class="p-version" style="font-size:0.75rem;">{ &a.agent_id[..8] }{"…"}</span>
                                        <span class={badge_class}>{ &a.status }</span>
                                    </div>
                                }
                            }) }
                        </div>
                    </div>
                }
            }

            // 已安装插件列表
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
