//! Audit Dashboard — Yew WASM 组件
//!
//! 展示审计日志的统计卡片、过滤面板、详情弹窗、导出和分页。

use crate::api::client::{
    get_audit_entry, get_audit_logs, get_audit_stats, AuditEntry, AuditStatsResponse,
};
use crate::i18n::use_lang;
use gloo_net::http::Request;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

/// 审计面板内部状态
#[derive(Clone, Debug)]
struct AuditViewState {
    entries: Vec<AuditEntry>,
    total: usize,
    filter_actor: String,
    filter_event_type: String,
    filter_result: String,
    offset: u64,
    limit: u64,
    loading: bool,
    error: String,
    stats: Option<AuditStatsResponse>,
    detail_id: Option<String>,
    detail_entry: Option<AuditEntry>,
    detail_loading: bool,
}

impl Default for AuditViewState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            total: 0,
            filter_actor: String::new(),
            filter_event_type: String::new(),
            filter_result: String::new(),
            offset: 0,
            limit: 50,
            loading: true,
            error: String::new(),
            stats: None,
            detail_id: None,
            detail_entry: None,
            detail_loading: false,
        }
    }
}

#[function_component(AuditDashboard)]
pub fn audit_dashboard() -> Html {
    let lang = use_lang();
    let strings = lang.strings();

    let state = use_state(AuditViewState::default);
    let refresh_trigger = use_state(|| 0u32);

    // ── 加载审计日志 + 统计 ──
    {
        let state = state.clone();
        let trigger = refresh_trigger.clone();
        use_effect_with(*trigger, move |_| {
            let state = state.clone();
            spawn_local(async move {
                let actor = state.filter_actor.clone();
                let et = state.filter_event_type.clone();
                let result = state.filter_result.clone();
                let offset = state.offset;
                let limit = state.limit;

                let (logs_result, stats_result) = futures::join!(
                    get_audit_logs(limit, offset, &actor, &et, &result),
                    get_audit_stats(),
                );

                let mut new_state = AuditViewState {
                    filter_actor: actor,
                    filter_event_type: et,
                    filter_result: result,
                    offset,
                    limit,
                    loading: false,
                    ..Default::default()
                };

                match logs_result {
                    Ok(resp) => {
                        new_state.entries = resp.entries;
                        new_state.total = resp.total;
                    }
                    Err(e) => new_state.error = format!("fetch error: {e}"),
                }

                if let Ok(stats) = stats_result {
                    new_state.stats = Some(stats);
                }

                state.set(new_state);
            });
            || ()
        });
    }

    // ── 加载详情 ──
    {
        let state = state.clone();
        let detail_id = state.detail_id.clone();
        use_effect_with(detail_id, move |detail_id| {
            if let Some(id) = detail_id {
                let state = state.clone();
                let id = id.clone();
                spawn_local(async move {
                    state.set(AuditViewState {
                        detail_loading: true,
                        ..(*state).clone()
                    });
                    let result = get_audit_entry(&id).await;
                    let mut s = (*state).clone();
                    if let Ok(entry) = result {
                        s.detail_entry = Some(entry);
                    } else {
                        s.error = format!("detail error: {}", result.err().unwrap_or_default());
                    }
                    s.detail_loading = false;
                    state.set(s);
                });
            }
            || ()
        });
    }

    // ── 触发刷新 ──
    let do_refresh = {
        let state = state.clone();
        let trigger = refresh_trigger.clone();
        Callback::from(move |_| {
            let mut s = (*state).clone();
            s.loading = true;
            s.error = String::new();
            state.set(s);
            trigger.set(*trigger + 1);
        })
    };

    // ── 设置过滤器 ──
    let set_actor = {
        let state = state.clone();
        Callback::from(move |val: String| {
            let mut s = (*state).clone();
            s.filter_actor = val;
            state.set(s);
        })
    };

    let set_event_type = {
        let state = state.clone();
        Callback::from(move |val: String| {
            let mut s = (*state).clone();
            s.filter_event_type = val;
            state.set(s);
        })
    };

    let set_result = {
        let state = state.clone();
        Callback::from(move |val: String| {
            let mut s = (*state).clone();
            s.filter_result = val;
            state.set(s);
        })
    };

    // ── 分页 ──
    let go_prev = {
        let state = state.clone();
        let trigger = refresh_trigger.clone();
        Callback::from(move |_| {
            let mut s = (*state).clone();
            if s.offset >= s.limit {
                s.offset = s.offset.saturating_sub(s.limit);
                s.loading = true;
                state.set(s);
                trigger.set(*trigger + 1);
            }
        })
    };

    let go_next = {
        let state = state.clone();
        let trigger = refresh_trigger.clone();
        Callback::from(move |_| {
            let mut s = (*state).clone();
            let next_offset = s.offset + s.limit;
            if next_offset < s.total as u64 {
                s.offset = next_offset;
                s.loading = true;
                state.set(s);
                trigger.set(*trigger + 1);
            }
        })
    };

    // ── 详情弹窗控制 ──
    let open_detail = {
        let state = state.clone();
        Callback::from(move |id: String| {
            let mut s = (*state).clone();
            s.detail_id = Some(id);
            s.detail_entry = None;
            state.set(s);
        })
    };

    let close_detail = {
        let state = state.clone();
        Callback::from(move |_| {
            let mut s = (*state).clone();
            s.detail_id = None;
            s.detail_entry = None;
            state.set(s);
        })
    };

    // ── 统计卡片 ──
    let stats_html = {
        let stats = state.stats.clone();
        if let Some(st) = stats {
            let mut cards = Vec::new();
            cards.push(html! {
                <div class="audit-summary-item">
                    <div class="num">{ st.total }</div>
                    <div class="label">{ strings.audit_total_entries }</div>
                </div>
            });
            let mut sorted: Vec<_> = st.by_event_type.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));
            for (k, v) in sorted.iter().take(4) {
                let label = k.replace('_', " ");
                cards.push(html! {
                    <div class="audit-summary-item">
                        <div class="num">{ v }</div>
                        <div class="label">{ label }</div>
                    </div>
                });
            }
            html! { <div class="audit-summary">{ for cards }</div> }
        } else {
            html! {}
        }
    };

    // ── 过滤器面板 ──
    let filter_html = {
        let actor_val = state.filter_actor.clone();
        let et_val = state.filter_event_type.clone();
        let result_val = state.filter_result.clone();
        let set_a = set_actor.clone();
        let set_et = set_event_type.clone();
        let set_r = set_result.clone();
        let refresh = do_refresh.clone();

        html! {
            <div class="audit-filters">
                <input type="text" placeholder="Actor"
                    value={actor_val}
                    oninput={Callback::from(move |e: InputEvent| {
                        let input = e.target_unchecked_into::<web_sys::HtmlInputElement>();
                        set_a.emit(input.value());
                    })} />
                <select value={et_val}
                    onchange={Callback::from(move |e: Event| {
                        let select = e.target_unchecked_into::<web_sys::HtmlSelectElement>();
                        set_et.emit(select.value());
                    })}>
                    <option value="">{"All Types"}</option>
                    <option value="api_call">{"API Call"}</option>
                    <option value="admin_action">{"Admin Action"}</option>
                    <option value="user_login">{"User Login"}</option>
                    <option value="user_logout">{"User Logout"}</option>
                    <option value="agent_execution">{"Agent Execution"}</option>
                    <option value="config_change">{"Config Change"}</option>
                    <option value="permission_change">{"Permission Change"}</option>
                    <option value="system">{"System"}</option>
                </select>
                <select value={result_val}
                    onchange={Callback::from(move |e: Event| {
                        let select = e.target_unchecked_into::<web_sys::HtmlSelectElement>();
                        set_r.emit(select.value());
                    })}>
                    <option value="">{"All Results"}</option>
                    <option value="success">{"Success"}</option>
                    <option value="failure">{"Failure"}</option>
                </select>
                <button class="audit-btn" onclick={refresh.clone()}>{"🔍 Filter"}</button>
                <button class="audit-btn" onclick={{
                    let refresh = refresh.clone();
                    let set_a = set_actor.clone();
                    let set_et = set_event_type.clone();
                    let set_r = set_result.clone();
                    Callback::from(move |_| {
                        set_a.emit(String::new());
                        set_et.emit(String::new());
                        set_r.emit(String::new());
                        // Also reset state and trigger refresh
                        let mut s = (*state).clone();
                        s.filter_actor = String::new();
                        s.filter_event_type = String::new();
                        s.filter_result = String::new();
                        s.offset = 0;
                        s.loading = true;
                        state.set(s);
                        refresh.emit(());
                    })
                }}>{"✕ Clear"}</button>
                <span style="flex:1"></span>
                <a class="audit-btn audit-export" href="/v1/audit/export?format=json&limit=10000" target="_blank">
                    {"📥 JSON"}
                </a>
                <a class="audit-btn audit-export" href="/v1/audit/export?format=csv&limit=10000" target="_blank">
                    {"📥 CSV"}
                </a>
            </div>
        }
    };

    // ── 表格 ──
    let table_html = {
        if state.loading {
            html! { <div class="audit-loading">{ strings.loading }</div> }
        } else if !state.error.is_empty() {
            html! { <div class="audit-error">{ &state.error }</div> }
        } else if state.entries.is_empty() {
            html! { <div class="audit-loading">{ strings.audit_no_entries }</div> }
        } else {
            let rows: Vec<Html> = state
                .entries
                .iter()
                .map(|entry| {
                    let result_class = if entry.result == "success" {
                        "success"
                    } else if entry.result == "failure" {
                        "failure"
                    } else {
                        ""
                    };
                    let type_class = entry.event_type.replace(' ', "_");
                    let detail_short = if entry.detail.len() > 60 {
                        format!("{}...", &entry.detail[..60])
                    } else {
                        entry.detail.clone()
                    };
                    let ts_display = if entry.timestamp.len() >= 19 {
                        entry.timestamp[..19].replace('T', " ")
                    } else {
                        entry.timestamp.clone()
                    };
                    let id = entry.id.clone();
                    let open = open_detail.clone();
                    let onclick = Callback::from(move |_| open.emit(id.clone()));

                    html! {
                        <tr onclick={onclick} style="cursor:pointer">
                            <td class="audit-ts">{ ts_display }</td>
                            <td><span class={classes!("audit-badge", type_class.clone())}>{ &entry.event_type }</span></td>
                            <td>{ &entry.event_name }</td>
                            <td>{ &entry.actor }</td>
                            <td class="audit-tt">{ format!("{} / {}", &entry.resource_type, &entry.resource_id) }</td>
                            <td><span class={classes!("audit-badge", result_class)}>{ &entry.result }</span></td>
                            <td class="audit-detail-preview">{ detail_short }</td>
                        </tr>
                    }
                })
                .collect();

            html! {
                <div class="audit-table-wrap">
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
                        <tbody>{ for rows }</tbody>
                    </table>
                </div>
            }
        }
    };

    // ── 分页 ──
    let page = if state.limit > 0 {
        (state.offset / state.limit) + 1
    } else {
        1
    };
    let total_pages = if state.limit > 0 && state.total > 0 {
        ((state.total as f64) / (state.limit as f64)).ceil() as u64
    } else {
        1
    };
    let has_prev = state.offset > 0;
    let has_next = state.offset + state.limit < state.total as u64;

    let pagination_html = if !state.loading && !state.entries.is_empty() {
        html! {
            <div class="audit-pagination">
                <button class="audit-btn" onclick={go_prev} disabled={!has_prev}>
                    {"⬅ Prev"}
                </button>
                <span>{ format!(" Page {} of {} ({} entries) ", page, total_pages, state.total) }</span>
                <button class="audit-btn" onclick={go_next} disabled={!has_next}>
                    {"Next ➡"}
                </button>
            </div>
        }
    } else {
        html! {}
    };

    // ── 详情弹窗 ──
    let detail_html = {
        if let Some(ref entry) = state.detail_entry {
            let close = close_detail.clone();
            html! {
                <div class="modal-overlay" onclick={move |_| close.emit(())}>
                    <div class="audit-modal" onclick={|e: MouseEvent| e.stop_propagation()}>
                        <div class="audit-modal-header">
                            <h2>{"📋 Audit Entry Detail"}</h2>
                            <button class="audit-btn" onclick={move |_| close.emit(())}>{"✕"}</button>
                        </div>
                        <div class="audit-modal-body">
                            <table class="audit-detail-table">
                                <tr><td class="dl">{"ID"}</td><td class="tt">{ &entry.id }</td></tr>
                                <tr><td class="dl">{"Time"}</td><td>{ &entry.timestamp }</td></tr>
                                <tr><td class="dl">{"Type"}</td><td><span class={classes!("audit-badge", entry.event_type.replace(' ', "_"))}>{ &entry.event_type }</span></td></tr>
                                <tr><td class="dl">{"Event"}</td><td>{ &entry.event_name }</td></tr>
                                <tr><td class="dl">{"Actor"}</td><td>{ &entry.actor }</td></tr>
                                <tr><td class="dl">{"Resource"}</td><td class="tt">{ format!("{} / {}", &entry.resource_type, &entry.resource_id) }</td></tr>
                                <tr><td class="dl">{"Result"}</td><td><span class={classes!("audit-badge", if entry.result == "success" { "success" } else { "failure" })}>{ &entry.result }</span></td></tr>
                                <tr><td class="dl">{"Trace ID"}</td><td class="tt">{ entry.trace_id.as_deref().unwrap_or("—") }</td></tr>
                                <tr><td class="dl">{"Source"}</td><td>{ entry.source.as_deref().unwrap_or("—") }</td></tr>
                                <tr><td class="dl">{"Detail"}</td><td><pre class="audit-detail-json">{ &entry.detail }</pre></td></tr>
                            </table>
                        </div>
                    </div>
                </div>
            }
        } else if state.detail_id.is_some() && state.detail_loading {
            html! {
                <div class="modal-overlay">
                    <div class="audit-modal">
                        <div class="audit-modal-body">
                            <p>{ "Loading..." }</p>
                        </div>
                    </div>
                </div>
            }
        } else {
            html! {}
        }
    };

    // ── 样式 ──
    let style = html! {
        <style>
            {r##"
.audit-page { padding: 2rem; color: #c9d1d9; }
.audit-page h1 { font-size: 1.6rem; margin-bottom: 1.5rem; color: #e6edf3; }
.audit-summary {
  display: flex; gap: 1rem; margin-bottom: 1.2rem; flex-wrap: wrap;
}
.audit-summary-item {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 0.6rem 1.2rem; min-width: 100px;
}
.audit-summary-item .num { font-size: 1.4rem; font-weight: 700; color: #58a6ff; }
.audit-summary-item .label { font-size: 0.75rem; color: #8b949e; text-transform: capitalize; }
.audit-filters {
  display: flex; gap: 0.6rem; margin-bottom: 1rem; flex-wrap: wrap;
  align-items: center;
}
.audit-filters select, .audit-filters input {
  background: #0d1117; border: 1px solid #30363d; border-radius: 6px;
  color: #c9d1d9; padding: 0.4rem 0.7rem; font-size: 0.85rem;
}
.audit-btn {
  background: #21262d; border: 1px solid #30363d; border-radius: 6px;
  color: #c9d1d9; padding: 0.4rem 0.8rem; font-size: 0.85rem;
  cursor: pointer; text-decoration: none; display: inline-block;
}
.audit-btn:hover { background: #30363d; }
.audit-btn:disabled { opacity: 0.5; cursor: default; }
.audit-export { background: #1c2d3d; border-color: #1f6feb; }
.audit-table-wrap { overflow-x: auto; }
.audit-table {
  width: 100%; border-collapse: collapse; font-size: 0.85rem;
}
.audit-table th {
  background: #161b22; padding: 0.6rem 0.8rem; text-align: left;
  color: #8b949e; font-weight: 600; border-bottom: 2px solid #30363d;
  position: sticky; top: 0; z-index: 1; white-space: nowrap;
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
.audit-badge.user_login { background: #1c3d2d; color: #3fb950; }
.audit-badge.user_logout { background: #3d2d1c; color: #d29922; }
.audit-badge.agent_execution { background: #1c2d3d; color: #58a6ff; }
.audit-badge.config_change { background: #2d1c3d; color: #bc8cff; }
.audit-badge.permission_change { background: #3d1c2d; color: #f85149; }
.audit-ts { color: #8b949e; font-size: 0.8rem; white-space: nowrap; }
.audit-tt { font-family: monospace; font-size: 0.8rem; color: #8b949e; }
.audit-detail-preview { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 0.8rem; color: #8b949e; }
.audit-loading { text-align: center; padding: 2rem; color: #8b949e; }
.audit-error { color: #f85149; padding: 1rem; background: #3d1c1c; border-radius: 6px; margin-bottom: 1rem; }
.audit-pagination {
  display: flex; align-items: center; justify-content: center;
  gap: 1rem; margin-top: 1rem; padding: 0.5rem;
  color: #8b949e; font-size: 0.85rem;
}
.modal-overlay {
  position: fixed; top: 0; left: 0; width: 100%; height: 100%;
  background: rgba(0,0,0,0.6); z-index: 9999;
  display: flex; align-items: center; justify-content: center;
}
.audit-modal {
  background: #161b22; border: 1px solid #30363d; border-radius: 12px;
  max-width: 700px; width: 90%; max-height: 85vh; overflow-y: auto;
}
.audit-modal-header {
  display: flex; justify-content: space-between; align-items: center;
  padding: 1rem 1.2rem; border-bottom: 1px solid #30363d;
}
.audit-modal-header h2 { margin: 0; font-size: 1.2rem; color: #e6edf3; }
.audit-modal-body { padding: 1rem 1.2rem; }
.audit-detail-table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
.audit-detail-table td { padding: 0.5rem 0.6rem; border-bottom: 1px solid #21262d; }
.audit-detail-table .dl { color: #8b949e; font-weight: 600; width: 100px; vertical-align: top; }
.audit-detail-json {
  max-height: 300px; overflow: auto; background: #0d1117;
  padding: 0.6rem; border-radius: 6px; font-size: 0.8rem;
  white-space: pre-wrap; word-break: break-all; margin: 0;
}
            "##}
        </style>
    };

    html! {
        <div class="audit-page">
            { style }
            <h1>{ &format!("📋 {}", strings.audit_title) }</h1>
            { stats_html }
            { filter_html }
            { table_html }
            { pagination_html }
            { detail_html }
        </div>
    }
}
