use yew::prelude::*;

#[derive(Properties, Clone, PartialEq)]
pub struct StatusCardProps {
    pub title: String,
    pub value: String,
    #[prop_or(None)]
    pub subtitle: Option<String>,
    #[prop_or(None)]
    pub status: Option<String>,
    #[prop_or(None)]
    pub icon: Option<String>,
}

#[function_component(StatusCard)]
pub fn status_card(props: &StatusCardProps) -> Html {
    let color = match props.status.as_deref() {
        Some("ok") => "#3fb950",
        Some("warn") => "#d29922",
        Some("err") => "#f85149",
        _ => "#58a6ff",
    };

    html! {
        <div class="status-card" style={format!("border-left: 3px solid {}", color)}>
            <div class="card-header">
                {props.icon.clone().map(|i| html!{ <span class="card-icon">{ i }</span> })}
                <span class="card-title">{ &props.title }</span>
            </div>
            <div class="card-value" style={format!("color: {}", color)}>
                { &props.value }
            </div>
            {props.subtitle.clone().map(|s| html!{ <div class="card-subtitle">{ s }</div> })}
            <style>
                {r##"
.status-card {
  background: #161b22;
  border-radius: 8px;
  padding: 1rem 1.2rem;
  min-width: 180px;
  transition: transform 0.1s;
}
.status-card:hover { transform: translateY(-1px); }
.card-header {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin-bottom: 0.5rem;
}
.card-icon { font-size: 1rem; }
.card-title { font-size: 0.8rem; color: #8b949e; text-transform: uppercase; letter-spacing: 0.04em; }
.card-value { font-size: 1.6rem; font-weight: 700; }
.card-subtitle { font-size: 0.8rem; color: #6e7681; margin-top: 0.3rem; }
                "##}
            </style>
        </div>
    }
}
