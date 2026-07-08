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
            {props.icon.clone().map(|i| html!{ <div class="status-card-icon">{ i }</div> })}
            <div class="status-card-value" style={format!("color: {}", color)}>
                { &props.value }
            </div>
            <div class="status-card-label">
                { &props.title }
            </div>
            {props.subtitle.clone().map(|s| html!{ <div class="status-card-subtitle">{ s }</div> })}
        </div>
    }
}
