use crate::api::client::{self, MarketPluginEntry, MarketSearchResponse, PluginListItem};
use crate::i18n::use_lang;
use yew::prelude::*;

#[function_component(Plugins)]
pub fn plugins() -> Html {
    let lang = use_lang();
    let strings = lang.strings();

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
                    Ok(_) => {
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                        s.set("Plugin started".into());
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
                    Ok(_) => {
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                        s.set("Plugin stopped".into());
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
                    Ok(_) => {
                        if let Ok(list) = client::get_plugins().await {
                            inst.set(list.plugins);
                        }
                        s.set("Plugin uninstalled".into());
                    }
                    Err(err) => e.set(err),
                }
            });
        })
    };

    // ── Render: Installed Plugin Cards ──────────────────
    let installed_plugins = installed.iter().map(|pl| {
        let status_class = match pl.status.as_str() {
            "running" | "active" => "status-running",
            "stopped" => "status-stopped",
            _ => "status-installed",
        };
        let pid = pl.id.clone();
        let pid2 = pl.id.clone();
        let pid3 = pl.id.clone();
        let on_start = {
            let cb = on_start_plugin.clone();
            Callback::from(move |_| cb.emit(pid.clone()))
        };
        let on_stop = {
            let cb = on_stop_plugin.clone();
            Callback::from(move |_| cb.emit(pid2.clone()))
        };
        let on_uninstall = {
            let cb = on_uninstall_plugin.clone();
            Callback::from(move |_| cb.emit(pid3.clone()))
        };

        html! {
            <div class="plugin-card" key={pl.id.clone()}>
                <div class="plugin-card-header">
                    <span class="plugin-name">{ &pl.name }</span>
                    <span class="plugin-version">{ &pl.version }</span>
                    <span class={format!("plugin-status {}", status_class)}>{ &pl.status }</span>
                </div>
                <div class="plugin-card-body">
                    <div class="plugin-desc">{ &pl.description }</div>
                </div>
                <div class="plugin-card-actions">
                    if pl.status == "stopped" || pl.status == "installed" || pl.status == "loaded" {
                        <button class="btn btn-sm btn-primary" onclick={on_start}>{ strings.plugins_start }</button>
                    } else {
                        <button class="btn btn-sm btn-warning" onclick={on_stop}>{ strings.plugins_stop }</button>
                    }
                    <button class="btn btn-sm btn-danger" onclick={on_uninstall}>{ strings.plugins_uninstall }</button>
                </div>
            </div>
        }
    }).collect::<Html>();

    // ── Render: Market Cards ────────────────────────────
    let market_cards = market_results.plugins.iter().map(|entry| {
        let entry_data = entry.clone();
        let on_install = {
            let cb = on_install_from_market.clone();
            Callback::from(move |_| cb.emit(entry_data.clone()))
        };

        let size_str = entry.size.map(format_size).unwrap_or_default();

        html! {
            <div class="market-card" key={entry.id.clone()}>
                <div class="market-card-header">
                    <span class="plugin-name">{ &entry.name }</span>
                    <span class="plugin-version">{ &entry.version }</span>
                    if let Some(author) = &entry.author {
                        <span class="plugin-author">{ author }</span>
                    }
                </div>
                <div class="market-card-body">
                    <div class="plugin-desc">{ &entry.description }</div>
                    <div class="plugin-tags">
                        { for entry.tags.iter().map(|t| html!{ <span class="tag">{ t }</span> }) }
                    </div>
                </div>
                <div class="market-card-footer">
                    if !size_str.is_empty() {
                        <span class="plugin-size">{ size_str }</span>
                    }
                    <button class="btn btn-sm btn-primary" onclick={on_install}>{ strings.plugins_install }</button>
                </div>
            </div>
        }
    }).collect::<Html>();

    // ── Page Layout ─────────────────────────────────────
    html! {
        <div class="page">
            <div class="page-header">
                <h1 class="page-title">{ strings.plugins_title }</h1>
                <div class="header-actions">
                    <button class="btn btn-primary" onclick={let sm = show_market.clone(); Callback::from(move |_| sm.set(!*sm))}>
                        { if *show_market { "← " } else { "" } }{ strings.plugins_market }
                    </button>
                    <button class={format!("btn {}",
                        if *hot_reload { "btn-warning" } else { "btn-secondary" }
                    )} onclick={on_toggle_hot_reload}>
                        { if *hot_reload { strings.plugins_hot_reload_stop } else { strings.plugins_hot_reload_start } }
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
                <div class="market-section">
                    <div class="search-bar">
                        <input type="text"
                            class="search-input"
                            placeholder={strings.plugins_search_placeholder}
                            value={(*search_query).clone()}
                            oninput={on_search_input}
                        />
                        <button class="btn btn-primary" onclick={on_search}>{ strings.plugins_search_btn }</button>
                    </div>
                    <div class="market-grid">{ market_cards }</div>
                    if market_results.total == 0 && !search_query.is_empty() {
                        <p class="empty-state">{ strings.plugins_no_results }</p>
                    }
                </div>
            } else {
                <div class="section">
                    <h2>{ format!("{} ({})", strings.plugins_installed, installed.len()) }</h2>
                    <div class="plugin-grid">{ installed_plugins }</div>
                    if installed.is_empty() {
                        <p class="empty-state">{ strings.plugins_no_plugins }</p>
                    }
                </div>
            }
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
