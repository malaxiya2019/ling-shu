use crate::api::client::{self, MarketPluginEntry, MarketSearchResponse, PluginListItem};
use yew::prelude::*;

#[function_component(Plugins)]
pub fn plugins() -> Html {
    // ── State ────────────────────────────────────────────
    let installed = use_state(|| Vec::<PluginListItem>::new());
    let market_results = use_state(|| MarketSearchResponse {
        query: String::new(),
        total: 0,
        plugins: Vec::new(),
    });
    let search_query = use_state(String::new);
    let error = use_state(String::new);
    let success = use_state(String::new);
    let hot_reload = use_state(|| false);
    let show_market = use_state(|| false);

    // ── Load installed plugins ───────────────────────────
    {
        let installed = installed.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match client::get_plugins().await {
                    Ok(resp) => installed.set(resp.plugins),
                    Err(e) => error.set(e),
                }
            });
            || ()
        });
    }

    // ── Handlers ─────────────────────────────────────────
    let on_search_input = {
        let search_query = search_query.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            search_query.set(input.value());
        })
    };

    let on_search = {
        let search_query = search_query.clone();
        let market_results = market_results.clone();
        let error = error.clone();
        Callback::from(move |_| {
            let q = search_query.clone();
            let mr = market_results.clone();
            let err = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client::market_search(&q).await {
                    Ok(resp) => mr.set(resp),
                    Err(e) => err.set(e),
                }
            });
        })
    };

    let on_install_from_market = {
        let market_results = market_results.clone();
        let installed = installed.clone();
        let success = success.clone();
        let error = error.clone();
        Callback::from(move |entry: MarketPluginEntry| {
            let _mr = market_results.clone();
            let inst = installed.clone();
            let s = success.clone();
            let e = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = client::MarketInstallRequest {
                    name: entry.name.clone(),
                    version: entry.version.clone(),
                    download_url: entry.download_url.clone(),
                    checksum: entry.checksum.clone(),
                };
                match client::market_install(&req).await {
                    Ok(resp) => {
                        s.set(format!(
                            "Installed {} v{} in {}",
                            resp.name, resp.version, resp.path
                        ));
                        // Refresh installed list
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                    }
                    Err(err_msg) => e.set(err_msg),
                }
            });
        })
    };

    let on_toggle_hot_reload = {
        let hot_reload = hot_reload.clone();
        let success = success.clone();
        let error = error.clone();
        Callback::from(move |_| {
            let hr = hot_reload.clone();
            let s = success.clone();
            let e = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if *hr {
                    match client::hot_reload_stop().await {
                        Ok(()) => {
                            hr.set(false);
                            s.set("Hot reload stopped".into());
                        }
                        Err(err) => e.set(err),
                    }
                } else {
                    match client::hot_reload_start().await {
                        Ok(()) => {
                            hr.set(true);
                            s.set("Hot reload started".into());
                        }
                        Err(err) => e.set(err),
                    }
                }
            });
        })
    };

    let on_start_plugin = {
        let installed = installed.clone();
        let success = success.clone();
        let error = error.clone();
        Callback::from(move |id: String| {
            let inst = installed.clone();
            let s = success.clone();
            let e = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client::start_plugin(&id).await {
                    Ok(resp) => {
                        s.set(format!("Plugin '{}' started", resp.name));
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                    }
                    Err(err) => e.set(err),
                }
            });
        })
    };

    let on_stop_plugin = {
        let installed = installed.clone();
        let success = success.clone();
        let error = error.clone();
        Callback::from(move |id: String| {
            let inst = installed.clone();
            let s = success.clone();
            let e = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client::stop_plugin(&id).await {
                    Ok(resp) => {
                        s.set(format!("Plugin '{}' stopped", resp.name));
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                    }
                    Err(err) => e.set(err),
                }
            });
        })
    };

    let on_uninstall_plugin = {
        let installed = installed.clone();
        let success = success.clone();
        let error = error.clone();
        Callback::from(move |id: String| {
            let inst = installed.clone();
            let s = success.clone();
            let e = error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match client::uninstall_plugin(&id).await {
                    Ok(()) => {
                        s.set("Plugin uninstalled".into());
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                    }
                    Err(err) => e.set(err),
                }
            });
        })
    };

    let on_toggle_market = {
        let show_market = show_market.clone();
        Callback::from(move |_| {
            let sm = show_market.clone();
            sm.set(!*sm);
        })
    };

    // ── Render ───────────────────────────────────────────
    let on_stop = on_stop_plugin.clone();
    let on_start = on_start_plugin.clone();
    let on_uninstall = on_uninstall_plugin.clone();
    let installed_plugins = installed.iter().map(|p| {
        let p_id = p.id.clone();
        let p_id2 = p.id.clone();
        let status_class = match p.status.as_str() {
            "Running" | "running" => "status-running",
            "Stopped" | "stopped" => "status-stopped",
            _ => "status-installed",
        };
        html! {
            <div class="plugin-card" key={p.id.clone()}>
                <div class="plugin-card-header">
                    <span class="plugin-name">{ &p.name }</span>
                    <span class="plugin-version">{ &p.version }</span>
                    <span class={classes!("plugin-status", status_class)}>{ &p.status }</span>
                </div>
                <div class="plugin-card-body">
                    <p class="plugin-desc">{ &p.description }</p>
                    { p.author.as_ref().map(|a| html!{ <span class="plugin-author">{ "by " }{ a }</span> }) }
                </div>
                <div class="plugin-card-actions">
                    if p.status == "Running" || p.status == "running" {
                        <button class="btn btn-sm btn-warning" onclick={let id = p_id; let cb = on_stop.clone(); move |_| cb.emit(id.clone())}>{ "⏹ Stop" }</button>
                    } else {
                        <button class="btn btn-sm btn-primary" onclick={let id = p_id.clone(); let cb = on_start.clone(); move |_| cb.emit(id.clone())}>{ "▶ Start" }</button>
                    }
                    <button class="btn btn-sm btn-danger" onclick={let id = p_id2; let cb = on_uninstall.clone(); move |_| cb.emit(id.clone())}>{ "🗑 Uninstall" }</button>
                </div>
            </div>
        }
    }).collect::<Html>();

    let on_install = on_install_from_market.clone();
    let market_cards = market_results.plugins.iter().map(|entry| {
        let entry_data = entry.clone();
        html! {
            <div class="market-card" key={entry.id.clone()}>
                <div class="market-card-header">
                    <span class="plugin-name">{ &entry.name }</span>
                    <span class="plugin-version">{ &entry.version }</span>
                </div>
                <div class="market-card-body">
                    <p class="plugin-desc">{ &entry.description }</p>
                    { entry.author.as_ref().map(|a| html!{ <span class="plugin-author">{ "by " }{ a }</span> }) }
                    if !entry.categories.is_empty() {
                        <div class="plugin-tags">
                            { entry.categories.iter().map(|c| html!{ <span class="tag">{ c }</span> }).collect::<Html>() }
                        </div>
                    }
                </div>
                <div class="market-card-footer">
                    { entry.size.map(|s| html!{ <span class="plugin-size">{ format_size(s) }</span> }) }
                    <button class="btn btn-sm btn-primary" onclick={let e = entry_data; let cb = on_install.clone(); move |_| cb.emit(e.clone())}>{ "📥 Install" }</button>
                </div>
            </div>
        }
    }).collect::<Html>();

    html! {
        <div class="page">
            <div class="page-header">
                <h1 class="page-title">{ "🧩 Plugins" }</h1>
                <div class="header-actions">
                    <button class={classes!("btn", "btn-sm", if *hot_reload { "btn-warning" } else { "btn-secondary" })}
                            onclick={on_toggle_hot_reload}>
                        { if *hot_reload { "⏹ Stop Hot Reload" } else { "▶ Start Hot Reload" } }
                    </button>
                    <button class="btn btn-sm btn-primary" onclick={on_toggle_market}>
                        { if *show_market { "📋 View Installed" } else { "🏪 Plugin Market" } }
                    </button>
                </div>
            </div>

            if !error.is_empty() {
                <div class="alert alert-error">{ error.as_str() }</div>
            }
            if !success.is_empty() {
                <div class="alert alert-success">{ success.as_str() }</div>
            }

            if *show_market {
                // ── Market Search ──────────────────────────
                <div class="market-section">
                    <div class="search-bar">
                        <input type="text"
                            class="search-input"
                            placeholder="Search plugins..."
                            value={(*search_query).clone()}
                            oninput={on_search_input}
                        />
                        <button class="btn btn-primary" onclick={on_search}>{ "🔍 Search" }</button>
                    </div>
                    <div class="market-grid">
                        { market_cards }
                    </div>
                    if market_results.total == 0 && !search_query.is_empty() {
                        <p class="empty-state">{ "No plugins found. Try a different search term." }</p>
                    }
                </div>
            } else {
                // ── Installed Plugins ──────────────────────
                <div class="section">
                    <h2>{ format!("Installed Plugins ({})", installed.len()) }</h2>
                    <div class="plugin-grid">
                        { installed_plugins }
                    </div>
                    if installed.is_empty() {
                        <p class="empty-state">{ "No plugins installed. Visit the Plugin Market to find and install plugins." }</p>
                    }
                </div>
            }

            <style>
                {r##"
.page { margin-left: 240px; padding: 2rem; }
.page-title { font-size: 1.5rem; margin-bottom: 1.5rem; color: #c9d1d9; }
.page-header { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 0.5rem; margin-bottom: 1.5rem; }
.header-actions { display: flex; gap: 0.5rem; }
.alert { padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1rem; }
.alert-error { background: #f8514933; color: #f85149; }
.alert-success { background: #3fb95033; color: #3fb950; }
.section { margin-top: 1rem; }
.section h2 { font-size: 1.1rem; color: #c9d1d9; margin-bottom: 1rem; }
.plugin-grid, .market-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 1rem; }
.plugin-card, .market-card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1rem; }
.plugin-card:hover, .market-card:hover { border-color: #58a6ff; }
.plugin-card-header, .market-card-header { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; flex-wrap: wrap; }
.plugin-name { font-weight: 600; color: #e6edf3; }
.plugin-version { font-size: 0.8rem; color: #6e7681; background: #21262d; padding: 0.15rem 0.4rem; border-radius: 4px; }
.plugin-status { font-size: 0.75rem; padding: 0.15rem 0.4rem; border-radius: 4px; margin-left: auto; }
.status-running { background: #3fb95033; color: #3fb950; }
.status-stopped { background: #f8514933; color: #f85149; }
.status-installed { background: #58a6ff33; color: #58a6ff; }
.plugin-card-body, .market-card-body { margin-bottom: 0.5rem; }
.plugin-desc { font-size: 0.85rem; color: #8b949e; margin-bottom: 0.3rem; }
.plugin-author { font-size: 0.8rem; color: #6e7681; }
.plugin-tags { display: flex; gap: 0.3rem; flex-wrap: wrap; margin-top: 0.3rem; }
.tag { font-size: 0.7rem; background: #21262d; color: #58a6ff; padding: 0.1rem 0.4rem; border-radius: 4px; }
.plugin-card-actions, .market-card-footer { display: flex; gap: 0.4rem; justify-content: flex-end; }
.plugin-size { font-size: 0.8rem; color: #6e7681; margin-right: auto; }
.btn { border: none; border-radius: 6px; cursor: pointer; font-size: 0.85rem; transition: opacity 0.15s; }
.btn:hover { opacity: 0.8; }
.btn-sm { padding: 0.3rem 0.7rem; }
.btn-primary { background: #238636; color: #fff; }
.btn-secondary { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; }
.btn-warning { background: #d29922; color: #fff; }
.btn-danger { background: #da3633; color: #fff; }
.search-bar { display: flex; gap: 0.5rem; margin-bottom: 1.5rem; }
.search-input { flex: 1; padding: 0.5rem 0.8rem; border: 1px solid #30363d; border-radius: 6px; background: #0d1117; color: #c9d1d9; font-size: 0.9rem; }
.search-input:focus { outline: none; border-color: #58a6ff; }
.empty-state { text-align: center; color: #6e7681; padding: 2rem; font-size: 0.95rem; }
.market-section { margin-top: 0.5rem; }
                "##}
            </style>
        </div>
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
