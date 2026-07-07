use yew::prelude::*;
use crate::api::client::FederationNodeInfo;

#[derive(Properties, Clone, PartialEq)]
pub struct TopologyProps {
    pub nodes: Vec<FederationNodeInfo>,
}

#[function_component(TopologyGraph)]
pub fn topology_graph(props: &TopologyProps) -> Html {
    let svg_nodes: Vec<(usize, f64, f64)> = props
        .nodes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let angle = 2.0 * std::f64::consts::PI * i as f64 / props.nodes.len().max(1) as f64;
            let x = 200.0 + 130.0 * angle.cos();
            let y = 200.0 + 130.0 * angle.sin();
            (i, x, y)
        })
        .collect();

    html! {
        <div class="topology-container">
            <h3 class="topo-title">{ "🌐 Federation Topology" }</h3>
            <svg viewBox="0 0 400 400" class="topo-svg">
                {for svg_nodes.iter().map(|(_, x, y)| {
                    html! { <line x1="200" y1="200" x2={format!("{:.1}", x)} y2={format!("{:.1}", y)} stroke="#30363d" stroke-width="2" /> }
                })}
                <circle cx="200" cy="200" r="18" fill="#1f6feb" />
                <text x="200" y="200" text-anchor="middle" dominant-baseline="central" fill="#fff" font-size="10" font-weight="bold">{ "hub" }</text>
                {for svg_nodes.iter().map(|(i, x, y)| {
                    let node = &props.nodes[*i];
                    let fill = match node.status.as_str() {
                        "online"  => "#238636",
                        "offline" => "#f85149",
                        _         => "#d29922",
                    };
                    html! {
                        <g>
                            <circle cx={format!("{:.1}", x)} cy={format!("{:.1}", y)} r="16" fill={fill} />
                            <text x={format!("{:.1}", x)} y={format!("{:.1}", y)}
                                  text-anchor="middle" dominant-baseline="central"
                                  fill="#fff" font-size="8" font-weight="bold">
                                { &node.name.chars().take(4).collect::<String>() }
                            </text>
                            <title>{ format!("{} — {}", node.name, node.status) }</title>
                        </g>
                    }
                })}
            </svg>
            <div class="topo-legend">
                <span class="legend-item"><span class="dot" style="background:#238636"></span>{" Online"}</span>
                <span class="legend-item"><span class="dot" style="background:#d29922"></span>{" Degraded"}</span>
                <span class="legend-item"><span class="dot" style="background:#f85149"></span>{" Offline"}</span>
            </div>
            <style>
                {r##"
.topology-container {
  background: #161b22;
  border-radius: 8px;
  padding: 1rem;
}
.topo-title { margin-bottom: 0.8rem; font-size: 1rem; color: #c9d1d9; }
.topo-svg { width: 100%; max-width: 400px; display: block; margin: 0 auto; }
.topo-legend {
  display: flex; justify-content: center; gap: 1rem;
  margin-top: 0.8rem; font-size: 0.8rem; color: #8b949e;
}
.legend-item { display: flex; align-items: center; gap: 0.3rem; }
.dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
                "##}
            </style>
        </div>
    }
}
