use crate::api::client::{self, FederationNodeInfo, FederationStatus};
use crate::components::status_card::StatusCard;
use crate::components::topology_graph::TopologyGraph;
use yew::prelude::*;

#[function_component(Federation)]
pub fn federation() -> Html {
    let lang = crate::i18n::use_lang();
    let strings = lang.strings();
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
                    <table class="data-table">
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
        </div>
    }
}
