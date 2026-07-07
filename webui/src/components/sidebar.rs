use yew::prelude::*;
use crate::pages::Page;

#[derive(Properties, Clone, PartialEq)]
pub struct SidebarProps {
    pub active_page: Page,
    pub on_navigate: Callback<Page>,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let on_click_dash = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Dashboard))
    };
    let on_click_fed = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Federation))
    };
    let on_click_eval = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::EvalReports))
    };
    let on_click_metrics = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Metrics))
    };
    let on_click_plugins = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Plugins))
    };
    let on_click_security = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Security))
    };

    let dash_class = if matches!(props.active_page, Page::Dashboard) { "active" } else { "" };
    let fed_class = if matches!(props.active_page, Page::Federation) { "active" } else { "" };
    let eval_class = if matches!(props.active_page, Page::EvalReports) { "active" } else { "" };
    let metrics_class = if matches!(props.active_page, Page::Metrics) { "active" } else { "" };
    let plugins_class = if matches!(props.active_page, Page::Plugins) { "active" } else { "" };
    let security_class = if matches!(props.active_page, Page::Security) { "active" } else { "" };

    html! {
        <nav class="sidebar">
            <div class="logo">
                <span class="logo-icon">{ "⚡" }</span>
                <span class="logo-text">{ "Lingshu" }</span>
            </div>
            <ul class="nav-list">
                <li>
                    <a class={dash_class} onclick={on_click_dash}>
                        <span class="nav-icon">{ "📊" }</span>
                        <span>{ " Dashboard" }</span>
                    </a>
                </li>
                <li>
                    <a class={fed_class} onclick={on_click_fed}>
                        <span class="nav-icon">{ "🌐" }</span>
                        <span>{ " Federation" }</span>
                    </a>
                </li>
                <li>
                    <a class={eval_class} onclick={on_click_eval}>
                        <span class="nav-icon">{ "📋" }</span>
                        <span>{ " Eval Reports" }</span>
                    </a>
                </li>
                <li>
                    <a class={metrics_class} onclick={on_click_metrics}>
                        <span class="nav-icon">{ "📈" }</span>
                        <span>{ " Metrics" }</span>
                    </a>
                </li>
                <li>
                    <a class={plugins_class} onclick={on_click_plugins}>
                        <span class="nav-icon">{ "🧩" }</span>
                        <span>{ " Plugins" }</span>
                    </a>
                </li>
                <li>
                    <a class={security_class} onclick={on_click_security}>
                        <span class="nav-icon">{ "🕷️" }</span>
                        <span>{ " Security" }</span>
                    </a>
                </li>
            </ul>
            <style>
                {r##"
.sidebar {
  width: 240px;
  background: #161b22;
  height: 100vh;
  position: fixed;
  left: 0;
  top: 0;
  border-right: 1px solid #30363d;
  display: flex;
  flex-direction: column;
}
.logo {
  padding: 1.2rem 1.5rem;
  border-bottom: 1px solid #30363d;
  display: flex;
  align-items: center;
  gap: 0.6rem;
}
.logo-icon { font-size: 1.4rem; }
.logo-text { font-size: 1.1rem; font-weight: 700; color: #58a6ff; }
.nav-list {
  list-style: none;
  padding: 0.8rem 0;
}
.nav-list li a {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.7rem 1.5rem;
  color: #8b949e;
  text-decoration: none;
  font-size: 0.95rem;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.nav-list li a:hover, .nav-list li a.active {
  background: #1f2937;
  color: #c9d1d9;
}
.nav-icon { font-size: 1.1rem; }
                "##}
            </style>
        </nav>
    }
}
