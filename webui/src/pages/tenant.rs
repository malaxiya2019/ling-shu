//! 🏢 多租户管理 Dashboard — v4.3 Enterprise
//!
//! 组织/项目/用户三级管理界面。

use crate::i18n::use_lang;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use yew::prelude::*;

// ── 数据模型 ────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub settings: OrgSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrgSettings {
    pub max_projects: u32,
    pub max_users: u32,
    pub max_agents: u32,
    pub enable_audit_log: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TenantUser {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TenantStats {
    pub total_orgs: u64,
    pub total_projects: u64,
    pub total_users: u64,
    pub active_orgs: u64,
}

// ── 视图状态 ────────────────────────────────────────

enum ActiveView {
    OrgList,
    OrgDetail { org: Organization },
    CreateOrg,
    CreateProject { org_id: String },
    InviteUser { org_id: String },
}

#[derive(Clone, Debug, Default)]
struct TenantState {
    orgs: Vec<Organization>,
    projects: Vec<Project>,
    users: Vec<TenantUser>,
    stats: Option<TenantStats>,
    selected_org: Option<Organization>,
    loading: bool,
    error: String,
    selected_tab: OrgTab,
}

#[derive(Clone, Debug, PartialEq)]
enum OrgTab {
    Overview,
    Projects,
    Users,
}

impl Default for OrgTab {
    fn default() -> Self {
        Self::Overview
    }
}

// ── 组件 ────────────────────────────────────────────

#[function_component(TenantDashboard)]
pub fn tenant_dashboard() -> Html {
    let lang = use_lang();
    let strings = lang.strings();
    let state = use_state(TenantState::default);
    let view = use_state(|| ActiveView::OrgList);

    // 加载组织列表
    {
        let state = state.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let mut s = TenantState { loading: true, ..TenantState::default() };

                // 获取组织列表
                match Request::get("/v1/tenant/orgs").send().await {
                    Ok(resp) => {
                        if let Ok(orgs) = resp.json::<Vec<Organization>>().await {
                            s.orgs = orgs;
                        }
                    }
                    Err(e) => s.error = format!("fetch orgs error: {e}"),
                }

                // 获取统计
                match Request::get("/v1/tenant/stats").send().await {
                    Ok(resp) => {
                        if let Ok(stats) = resp.json::<TenantStats>().await {
                            s.stats = Some(stats);
                        }
                    }
                    Err(e) => {
                        if s.error.is_empty() {
                            s.error = format!("fetch stats error: {e}");
                        }
                    }
                }

                s.loading = false;
                state.set(s);
            });
            || ()
        });
    }

    let on_create_org = {
        let view = view.clone();
        Callback::from(move |_| view.set(ActiveView::CreateOrg))
    };

    let on_org_click = {
        let state = state.clone();
        let view = view.clone();
        Callback::from(move |org: Organization| {
            let org_id = org.id.clone();
            let view = view.clone();
            let state = state.clone();
            let org_clone = org.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut s = TenantState { loading: true, ..TenantState::default() };

                // 获取项目列表
                match Request::get(&format!("/v1/tenant/orgs/{org_id}/projects")).send().await {
                    Ok(resp) => {
                        if let Ok(projects) = resp.json::<Vec<Project>>().await {
                            s.projects = projects;
                        }
                    }
                    Err(e) => s.error = format!("fetch projects error: {e}"),
                }

                // 获取用户列表
                match Request::get(&format!("/v1/tenant/orgs/{org_id}/users")).send().await {
                    Ok(resp) => {
                        if let Ok(users) = resp.json::<Vec<TenantUser>>().await {
                            s.users = users;
                        }
                    }
                    Err(e) => {
                        if s.error.is_empty() {
                            s.error = format!("fetch users error: {e}");
                        }
                    }
                }

                s.selected_org = Some(org_clone.clone());
                s.loading = false;
                state.set(s);
                view.set(ActiveView::OrgDetail { org: org_clone });
            });
        })
    };

    let on_back = {
        let view = view.clone();
        let state = state.clone();
        Callback::from(move |_| {
            view.set(ActiveView::OrgList);
            state.set(TenantState::default());
        })
    };

    let on_tab_change = {
        let state = state.clone();
        Callback::from(move |tab: OrgTab| {
            let mut s = (*state).clone();
            s.selected_tab = tab;
            state.set(s);
        })
    };

    html! {
        <div class="tenant-page">
            <style>
                {r##"
.tenant-page { padding: 2rem; color: #c9d1d9; }
.tenant-page h1 { font-size: 1.6rem; margin-bottom: 1.5rem; color: #e6edf3; }
.tenant-page h2 { font-size: 1.2rem; margin-bottom: 1rem; color: #e6edf3; }
.tenant-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1.5rem; }
.tenant-stats { display: flex; gap: 1rem; margin-bottom: 1.5rem; flex-wrap: wrap; }
.tenant-stat-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 1rem 1.5rem; min-width: 140px; text-align: center;
}
.tenant-stat-card .num { font-size: 1.8rem; font-weight: 700; color: #58a6ff; }
.tenant-stat-card .label { font-size: 0.8rem; color: #8b949e; margin-top: 0.3rem; }
.tenant-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 1rem; }
.tenant-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 1.2rem; cursor: pointer; transition: border-color 0.15s;
}
.tenant-card:hover { border-color: #58a6ff; }
.tenant-card .name { font-size: 1.1rem; font-weight: 600; color: #e6edf3; }
.tenant-card .slug { font-size: 0.8rem; color: #8b949e; margin-top: 0.2rem; }
.tenant-card .desc { font-size: 0.85rem; color: #8b949e; margin-top: 0.5rem; }
.tenant-card .meta { display: flex; gap: 1rem; margin-top: 0.8rem; font-size: 0.8rem; color: #8b949e; }
.tenant-badge {
  display: inline-block; padding: 0.15rem 0.5rem; border-radius: 12px; font-size: 0.75rem; font-weight: 600;
}
.tenant-badge.active { background: #1b3a2d; color: #3fb950; }
.tenant-badge.suspended { background: #3d1c1c; color: #f85149; }
.btn {
  display: inline-flex; align-items: center; gap: 0.4rem;
  padding: 0.5rem 1rem; border-radius: 6px; font-size: 0.85rem; font-weight: 600;
  cursor: pointer; border: 1px solid #30363d; background: #21262d; color: #c9d1d9;
  transition: background 0.15s, border-color 0.15s;
}
.btn:hover { background: #30363d; border-color: #58a6ff; }
.btn-primary { background: #238636; border-color: #2ea043; color: #fff; }
.btn-primary:hover { background: #2ea043; }
.btn-back { margin-bottom: 1rem; }
.detail-header { display: flex; align-items: center; gap: 1rem; margin-bottom: 1.5rem; }
.tabs { display: flex; gap: 0.5rem; margin-bottom: 1.5rem; border-bottom: 1px solid #30363d; padding-bottom: 0.5rem; }
.tab {
  padding: 0.5rem 1rem; cursor: pointer; border-radius: 6px 6px 0 0;
  color: #8b949e; font-size: 0.9rem; transition: color 0.15s, background 0.15s;
}
.tab:hover { color: #e6edf3; background: #161b22; }
.tab.active { color: #58a6ff; border-bottom: 2px solid #58a6ff; }
.data-table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
.data-table th {
  background: #161b22; padding: 0.5rem 0.8rem; text-align: left;
  color: #8b949e; font-weight: 600; border-bottom: 2px solid #30363d;
}
.data-table td { padding: 0.4rem 0.8rem; border-bottom: 1px solid #21262d; }
.data-table tr:hover { background: #161b22; }
.tenant-loading { text-align: center; padding: 3rem; color: #8b949e; }
.tenant-error { color: #f85149; padding: 1rem; background: #3d1c1c; border-radius: 6px; margin-bottom: 1rem; }
.empty-state { text-align: center; padding: 3rem; color: #8b949e; }
.detail-info { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1rem; margin-bottom: 1.5rem; }
.detail-info .row { display: flex; padding: 0.4rem 0; }
.detail-info .label { width: 140px; color: #8b949e; font-size: 0.85rem; }
.detail-info .value { color: #c9d1d9; font-size: 0.85rem; }
                "##}
            </style>

            { match &*view {
                ActiveView::OrgList => render_org_list(&state, strings, on_create_org, on_org_click),
                ActiveView::OrgDetail { org } => render_org_detail(
                    org, &state, strings, on_back.clone(), on_tab_change.clone(),
                ),
                ActiveView::CreateOrg => render_create_org(strings, on_back.clone()),
                ActiveView::CreateProject { org_id } => render_create_project(org_id, strings, on_back.clone()),
                ActiveView::InviteUser { org_id } => render_invite_user(org_id, strings, on_back.clone()),
            }}
        </div>
    }
}

// ── 渲染函数 ────────────────────────────────────────

fn render_org_list(
    state: &TenantState,
    strings: &'static crate::i18n::Translations,
    on_create: Callback<()>,
    on_org_click: Callback<Organization>,
) -> Html {
    let stats = &state.stats;

    html! {
        <>
            <div class="tenant-header">
                <h1>{ "🏢 " } { strings.app_title } { " — Multi-Tenant" }</h1>
                <button class="btn btn-primary" onclick={on_create}>
                    { "+ " } { strings.plugins_install }
                </button>
            </div>

            // 统计卡片
            <div class="tenant-stats">
                <div class="tenant-stat-card">
                    <div class="num">{ stats.as_ref().map(|s| s.total_orgs).unwrap_or(0) }</div>
                    <div class="label">{ "Organizations" }</div>
                </div>
                <div class="tenant-stat-card">
                    <div class="num">{ stats.as_ref().map(|s| s.total_projects).unwrap_or(0) }</div>
                    <div class="label">{ "Projects" }</div>
                </div>
                <div class="tenant-stat-card">
                    <div class="num">{ stats.as_ref().map(|s| s.total_users).unwrap_or(0) }</div>
                    <div class="label">{ "Users" }</div>
                </div>
                <div class="tenant-stat-card">
                    <div class="num">{ stats.as_ref().map(|s| s.active_orgs).unwrap_or(0) }</div>
                    <div class="label">{ "Active Orgs" }</div>
                </div>
            </div>

            // 错误提示
            { if !state.error.is_empty() {
                html! { <div class="tenant-error">{ &state.error }</div> }
            } else { html! {} } }

            // 组织列表
            { if state.loading {
                html! { <div class="tenant-loading">{ strings.loading }</div> }
            } else if state.orgs.is_empty() {
                html! {
                    <div class="empty-state">
                        <p>{ "No organizations yet. Create your first organization to get started." }</p>
                    </div>
                }
            } else {
                html! {
                    <div class="tenant-grid">
                        { for state.orgs.iter().map(|org| {
                            let org_clone = org.clone();
                            let onclick = {
                                let cb = on_org_click.clone();
                                Callback::from(move |_| cb.emit(org_clone.clone()))
                            };
                            let status_class = org.status.to_lowercase();

                            html! {
                                <div class="tenant-card" onclick={onclick}>
                                    <div class="name">{ &org.name }</div>
                                    <div class="slug">{ &org.slug }</div>
                                    <div class="desc">{ &org.description }</div>
                                    <div class="meta">
                                        <span><span class={format!("tenant-badge {}", status_class)}>{ &org.status }</span></span>
                                        <span>{ format!("Max {} projects", org.settings.max_projects) }</span>
                                        <span>{ format!("Max {} users", org.settings.max_users) }</span>
                                    </div>
                                </div>
                            }
                        })}
                    </div>
                }
            } }
        </>
    }
}

fn render_org_detail(
    org: &Organization,
    state: &TenantState,
    strings: &'static crate::i18n::Translations,
    on_back: Callback<()>,
    on_tab_change: Callback<OrgTab>,
) -> Html {
    let org_id = org.id.clone();
    let tab = &state.selected_tab;

    let on_tab_overview = { let cb = on_tab_change.clone(); Callback::from(move |_| cb.emit(OrgTab::Overview)) };
    let on_tab_projects = { let cb = on_tab_change.clone(); Callback::from(move |_| cb.emit(OrgTab::Projects)) };
    let on_tab_users = { let cb = on_tab_change.clone(); Callback::from(move |_| cb.emit(OrgTab::Users)) };

    html! {
        <>
            <button class="btn btn-back" onclick={on_back}>{ "← Back" }</button>

            <div class="detail-header">
                <h1>{ &org.name }</h1>
                <span class={format!("tenant-badge {}", org.status.to_lowercase())}>{ &org.status }</span>
            </div>

            <div class="detail-info">
                <div class="row"><span class="label">{"ID"}</span><span class="value">{ &org.id }</span></div>
                <div class="row"><span class="label">{"Slug"}</span><span class="value">{ &org.slug }</span></div>
                <div class="row"><span class="label">{"Description"}</span><span class="value">{ &org.description }</span></div>
                <div class="row"><span class="label">{"Max Projects"}</span><span class="value">{ org.settings.max_projects }</span></div>
                <div class="row"><span class="label">{"Max Users"}</span><span class="value">{ org.settings.max_users }</span></div>
                <div class="row"><span class="label">{"Max Agents"}</span><span class="value">{ org.settings.max_agents }</span></div>
                <div class="row"><span class="label">{"Audit Log"}</span><span class="value">{ if org.settings.enable_audit_log { "✅ Enabled" } else { "❌ Disabled" } }</span></div>
            </div>

            <div class="tabs">
                <div class={format!("tab{}", if *tab == OrgTab::Overview { " active" } else { "" })} onclick={on_tab_overview}>{ "Overview" }</div>
                <div class={format!("tab{}", if *tab == OrgTab::Projects { " active" } else { "" })} onclick={on_tab_projects}>{ "Projects" }</div>
                <div class={format!("tab{}", if *tab == OrgTab::Users { " active" } else { "" })} onclick={on_tab_users}>{ "Users" }</div>
            </div>

            { match tab {
                OrgTab::Overview => {
                    html! {
                        <div class="tenant-stats">
                            <div class="tenant-stat-card">
                                <div class="num">{ state.projects.len() }</div>
                                <div class="label">{"Projects"}</div>
                            </div>
                            <div class="tenant-stat-card">
                                <div class="num">{ state.users.len() }</div>
                                <div class="label">{"Users"}</div>
                            </div>
                        </div>
                    }
                }
                OrgTab::Projects => {
                    let on_create_project = {
                        let org_id = org_id.clone();
                        Callback::from(move |_| {
                            gloo_console::log!("Create project for org:", &org_id);
                        })
                    };

                    html! {
                        <>
                            <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:1rem;">
                                <h2>{ "Projects" }</h2>
                                <button class="btn btn-primary" onclick={on_create_project}>{ "+ New Project" }</button>
                            </div>
                            { if state.projects.is_empty() {
                                html! { <div class="empty-state"><p>{ "No projects yet." }</p></div> }
                            } else {
                                html! {
                                    <table class="data-table">
                                        <thead><tr>
                                            <th>{"Name"}</th><th>{"Description"}</th><th>{"Status"}</th><th>{"Created"}</th>
                                        </tr></thead>
                                        <tbody>{ for state.projects.iter().map(|p| {
                                            let status_class = p.status.to_lowercase();
                                            html! {
                                                <tr>
                                                    <td style="font-weight:600;">{ &p.name }</td>
                                                    <td style="color:#8b949e;">{ &p.description }</td>
                                                    <td><span class={format!("tenant-badge {}", status_class)}>{ &p.status }</span></td>
                                                    <td style="color:#8b949e;font-size:0.8rem;">{ &p.created_at[..10] }</td>
                                                </tr>
                                            }
                                        })}</tbody>
                                    </table>
                                }
                            } }
                        </>
                    }
                }
                OrgTab::Users => {
                    html! {
                        <>
                            <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:1rem;">
                                <h2>{ "Users" }</h2>
                                <button class="btn btn-primary">{ "+ Invite User" }</button>
                            </div>
                            { if state.users.is_empty() {
                                html! { <div class="empty-state"><p>{ "No users yet." }</p></div> }
                            } else {
                                html! {
                                    <table class="data-table">
                                        <thead><tr>
                                            <th>{"Email"}</th><th>{"Name"}</th><th>{"Role"}</th><th>{"Status"}</th><th>{"Joined"}</th>
                                        </tr></thead>
                                        <tbody>{ for state.users.iter().map(|u| {
                                            let status_class = u.status.to_lowercase();
                                            html! {
                                                <tr>
                                                    <td>{ &u.email }</td>
                                                    <td>{ &u.display_name }</td>
                                                    <td><span class={format!("tenant-badge active")}>{ &u.role }</span></td>
                                                    <td><span class={format!("tenant-badge {}", status_class)}>{ &u.status }</span></td>
                                                    <td style="color:#8b949e;font-size:0.8rem;">{ &u.created_at[..10] }</td>
                                                </tr>
                                            }
                                        })}</tbody>
                                    </table>
                                }
                            } }
                        </>
                    }
                }
            }}
        </>
    }
}

fn render_create_org(
    strings: &'static crate::i18n::Translations,
    on_back: Callback<()>,
) -> Html {
    html! {
        <>
            <button class="btn btn-back" onclick={on_back}>{ "← Back" }</button>
            <h1>{ "Create Organization" }</h1>
            <p style="color:#8b949e;">{"This feature requires the WebUI to be compiled with full backend support."}</p>
            <div class="empty-state">
                <p>{ "Use the API directly:" }</p>
                <code style="background:#0d1117;padding:1rem;display:block;border-radius:6px;margin-top:0.5rem;text-align:left;">
                    { "POST /v1/tenant/orgs\n{\n  \"name\": \"My Org\",\n  \"slug\": \"my-org\",\n  \"description\": \"...\"\n}" }
                </code>
            </div>
        </>
    }
}

fn render_create_project(
    _org_id: &str,
    strings: &'static crate::i18n::Translations,
    on_back: Callback<()>,
) -> Html {
    html! {
        <>
            <button class="btn btn-back" onclick={on_back}>{ "← Back" }</button>
            <h1>{ "Create Project" }</h1>
            <p style="color:#8b949e;">{"Use the API to create projects."}</p>
        </>
    }
}

fn render_invite_user(
    _org_id: &str,
    strings: &'static crate::i18n::Translations,
    on_back: Callback<()>,
) -> Html {
    html! {
        <>
            <button class="btn btn-back" onclick={on_back}>{ "← Back" }</button>
            <h1>{ "Invite User" }</h1>
            <p style="color:#8b949e;">{"Use the API to invite users."}</p>
        </>
    }
}
