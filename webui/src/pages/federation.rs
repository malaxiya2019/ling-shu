use crate::api::client::{self, FederationNodeInfo, FederationStatus};
use crate::components::status_card::StatusCard;
use crate::components::topology_graph::TopologyGraph;
use yew::prelude::*;

#[function_component(Federation)]
pub fn federation() -> Html {
    let lang = crate::i18n::use_lang();
    let strings = lang.strings;
    let status = use_state(|| None::<FederationStatus>);
    let nodes = use_state(Vec::<FederationNodeInfo>::new);
    let error = use_state(String::new);

    {
        let status = status.clone();
        let nodes = nodes.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match client::get_federation_status().await {
                    Ok(s) => status.set(Some(s)),
                    Err(e) => error.set(format!("status: {e}")),
                }
                match client::get_federation_nodes().await {
                    Ok(n) => nodes.set(n),
                    Err(e) => error.set(format!("nodes: {e}")),
                }
            });
            || ()
        });
    }

    let (enabled_str, node_count, uptime_secs, status_quality) = match status.as_ref() {
        Some(s) => (
            if s.enabled { "Enabled" } else { "Disabled" }.to_string(),
            s.node_count,
            s.uptime_secs,
            if s.enabled { "ok" } else { "warn" },
        ),
        None => ("—".to_string(), 0usize, 0i64, "warn"),
    };

    html! {
        <div class="page">
            <h1 class="page-title">{ strings.federation_title }</h1>
            if !error.is_empty() {
                <div class="error-banner">{ error.as_str() }</div>
            }
            <div class="cards-row">
                <StatusCard title="Status"   value={enabled_str} status={Some(status_quality.to_string())} icon={Some("🔄".to_string())} />
                <StatusCard title="Nodes"    value={format!("{}", node_count)} status={Some("ok".to_string())} icon={Some("🖥️".to_string())} />
                <StatusCard title="Uptime"   value={format!("{}s", uptime_secs)} status={Some("ok".to_string())} icon={Some("⏱️".to_string())} />
            </div>

            <div class="section">
                <TopologyGraph nodes={(*nodes).clone()} />
            </div>

            if !nodes.is_empty() {
                <div class="section">
                    <h2>{ "📋 Peer Nodes" }</h2>
                    <table class="node-table">
                        <thead>
                            <tr><th>{ "Name" }</th><th>{ "Address" }</th><th>{ "Status" }</th><th>{ "Capabilities" }</th></tr>
                        </thead>
                        <tbody>
                            { for nodes.iter().map(|n| {
                                let status_class = match n.status.as_str() {
                                    "online"  => "status-online",
                                    "offline" => "status-offline",
                                    _         => "status-degraded",
                                };
                                html! {
                                    <tr>
                                        <td><strong>{ &n.name }</strong></td>
                                        <td><code>{ &n.addr }</code></td>
                                        <td><span class={status_class}>{ &n.status }</span></td>
                                        <td>{ n.capabilities.join(", ") }</td>
                                    </tr>
                                }
                            })}
                        </tbody>
                    </table>
                </div>
            }

            <style>
                {r##"
.page { margin-left: 240px; padding: 2rem; }
.page-title { font-size: 1.5rem; margin-bottom: 1.5rem; color: #c9d1d9; }
.error-banner { background: #f8514933; color: #f85149; padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1rem; }
.cards-row { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 2rem; }
.section { margin: 1.5rem 0; }
.section h2 { font-size: 1.1rem; color: #c9d1d9; margin-bottom: 0.8rem; }
.node-table { width: 100%; border-collapse: collapse; }
.node-table th, .node-table td { text-align: left; padding: 0.6rem; border-bottom: 1px solid #21262d; }
.node-table th { color: #58a6ff; font-size: 0.8rem; text-transform: uppercase; }
.node-table td { color: #c9d1d9; }
.node-table code { background: #161b22; padding: 0.15em 0.4em; border-radius: 3px; font-size: 0.85em; }
.status-online   { color: #3fb950; }
.status-offline  { color: #f85149; }
.status-degraded { color: #d29922; }
                "##}
            </style>
        </div>
    }
}
