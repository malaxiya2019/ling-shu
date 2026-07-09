use crate::i18n::use_lang;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use yew::prelude::*;

/// 审计日志条目.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub event_name: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub resource_type: String,
    #[serde(default)]
    pub resource_id: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub result: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct AuditResponse {
    entries: Vec<AuditEntry>,
    total: usize,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct AuditState {
    entries: Vec<AuditEntry>,
    total: usize,
    filter_actor: String,
    filter_event: String,
    filter_result: String,
    loading: bool,
    error: String,
}

#[function_component(AuditDashboard)]
pub fn audit_dashboard() -> Html {
    let lang = use_lang();
    let strings = lang.strings();
    let state = use_state(AuditState::default);
    let refresh = use_state(|| true);

    // 初始加载 + 刷新
    {
        let state = state.clone();
        let refresh = refresh.clone();
        use_effect_with(refresh.clone(), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                let mut s = AuditState::default();
                s.loading = true;

                // 构建查询参数
                let _actor_clone = state.filter_actor.clone();
                let params = vec![
                    ("limit", "200"),
                ];

                let url = format!("/v1/audit/logs?{}", params.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&"));

                match Request::get(&url).send().await {
                    Ok(resp) => {
                        match resp.json::<AuditResponse>().await {
                            Ok(data) => {
                                s.entries = data.entries;
                                s.total = data.total;
                            }
                            Err(e) => s.error = format!("parse error: {e}"),
                        }
                    }
                    Err(e) => s.error = format!("fetch error: {e}"),
                }
                s.loading = false;
                state.set(s);
            });
            || ()
        });
    }

    let _event_types = vec![
        "", "user_login", "user_logout", "api_call", "agent_execution",
        "admin_action", "config_change", "permission_change", "system",
    ];
    let _result_types = vec!["", "success", "failure"];

    html! {
        <div class="audit-page">
            <style>
                {r##"
.audit-page { padding: 2rem; color: #c9d1d9; }
.audit-page h1 { font-size: 1.6rem; margin-bottom: 1.5rem; color: #e6edf3; }
.audit-filters {
  display: flex; gap: 0.8rem; margin-bottom: 1.2rem; flex-wrap: wrap;
  align-items: center;
}
.audit-filters select, .audit-filters input {
  background: #0d1117; border: 1px solid #30363d; border-radius: 6px;
  color: #c9d1d9; padding: 0.4rem 0.8rem; font-size: 0.85rem;
}
.audit-table {
  width: 100%; border-collapse: collapse; font-size: 0.85rem;
}
.audit-table th {
  background: #161b22; padding: 0.6rem 0.8rem; text-align: left;
  color: #8b949e; font-weight: 600; border-bottom: 2px solid #30363d;
  position: sticky; top: 0; z-index: 1;
}
.audit-table td {
  padding: 0.5rem 0.8rem; border-bottom: 1px solid #21262d;
}
.audit-table tr:hover { background: #161b22; }
.audit-badge {
  display: inline-block; padding: 0.15rem 0.5rem; border-radius: 12px;
  font-size: 0.75rem; font-weight: 600;
}
.audit-badge.success { background: #1b3a2d; color: #3fb950; }
.audit-badge.failure { background: #3d1c1c; color: #f85149; }
.audit-badge.api_call { background: #1c2d3d; color: #58a6ff; }
.audit-badge.admin_action { background: #2d1c3d; color: #bc8cff; }
.audit-badge.system { background: #2d2d1c; color: #d29922; }
.audit-timestamp { color: #8b949e; font-size: 0.8rem; white-space: nowrap; }
.audit-loading { text-align: center; padding: 2rem; color: #8b949e; }
.audit-error { color: #f85149; padding: 1rem; background: #3d1c1c; border-radius: 6px; margin-bottom: 1rem; }
.audit-summary {
  display: flex; gap: 1.5rem; margin-bottom: 1rem; flex-wrap: wrap;
}
.audit-summary-item {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 0.6rem 1.2rem;
}
.audit-summary-item .num { font-size: 1.4rem; font-weight: 700; color: #58a6ff; }
.audit-summary-item .label { font-size: 0.75rem; color: #8b949e; }
                "##}
            </style>

            <h1>{ &format!("📋 {}", strings.audit_title) }</h1>

            // 摘要
            <div class="audit-summary">
                <div class="audit-summary-item">
                    <div class="num">{ state.total }</div>
                    <div class="label">{ strings.audit_total_entries }</div>
                </div>
                <div class="audit-summary-item">
                    <div class="num">{ state.entries.len() }</div>
                    <div class="label">{ strings.audit_displayed }</div>
                </div>
            </div>

            // 错误提示
            { if !state.error.is_empty() {
                html! { <div class="audit-error">{ &state.error }</div> }
            } else { html! {} } }

            // 表格
            { if state.loading {
                html! { <div class="audit-loading">{ strings.loading }</div> }
            } else if state.entries.is_empty() {
                html! { <div class="audit-loading">{ strings.audit_no_entries }</div> }
            } else {
                html! {
                    <table class="audit-table">
                        <thead>
                            <tr>
                                <th>{ strings.audit_col_time }</th>
                                <th>{ strings.audit_col_type }</th>
                                <th>{ strings.audit_col_event }</th>
                                <th>{ strings.audit_col_actor }</th>
                                <th>{ strings.audit_col_resource }</th>
                                <th>{ strings.audit_col_result }</th>
                                <th>{ strings.audit_col_detail }</th>
                            </tr>
                        </thead>
                        <tbody>
                            { for state.entries.iter().map(|entry| {
                                let result_class = if entry.result == "success" { "success" }
                                    else if entry.result == "failure" { "failure" }
                                    else { "" };
                                let type_class = entry.event_type.replace(" ", "_");
                                let detail_preview = if entry.detail.len() > 60 {
                                    format!("{}...", &entry.detail[..60])
                                } else {
                                    entry.detail.clone()
                                };

                                html! {
                                    <tr>
                                        <td class="audit-timestamp">{ &entry.timestamp[..19].replace("T", " ") }</td>
                                        <td><span class={format!("audit-badge {}", type_class)}>{ &entry.event_type }</span></td>
                                        <td>{ &entry.event_name }</td>
                                        <td>{ &entry.actor }</td>
                                        <td>{ format!("{} / {}", &entry.resource_type, &entry.resource_id) }</td>
                                        <td><span class={format!("audit-badge {}", result_class)}>{ &entry.result }</span></td>
                                        <td style="max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.8rem; color: #8b949e;">
                                            { detail_preview }
                                        </td>
                                    </tr>
                                }
                            })}
                        </tbody>
                    </table>
                }
            } }
        </div>
    }
}
