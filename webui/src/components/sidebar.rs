use crate::i18n::{use_lang, Locale};
use crate::pages::Page;
use yew::prelude::*;

#[derive(Properties, Clone, PartialEq)]
pub struct SidebarProps {
    pub active_page: Page,
    pub on_navigate: Callback<Page>,
}

#[function_component(Sidebar)]
pub fn sidebar(props: &SidebarProps) -> Html {
    let lang = use_lang();
    let strings = lang.strings;

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

    let dash_class = if matches!(props.active_page, Page::Dashboard) {
        "active"
    } else {
        ""
    };
    let fed_class = if matches!(props.active_page, Page::Federation) {
        "active"
    } else {
        ""
    };
    let eval_class = if matches!(props.active_page, Page::EvalReports) {
        "active"
    } else {
        ""
    };
    let metrics_class = if matches!(props.active_page, Page::Metrics) {
        "active"
    } else {
        ""
    };
    let plugins_class = if matches!(props.active_page, Page::Plugins) {
        "active"
    } else {
        ""
    };
    let security_class = if matches!(props.active_page, Page::Security) {
        "active"
    } else {
        ""
    };

    // ── Sidebar language switcher ────────────────────
    let is_zh = matches!(lang.locale, Locale::Zh);
    let toggle_zh = {
        let cb = lang.on_toggle.clone();
        Callback::from(move |_| {
            if !is_zh { cb() }
        })
    };
    let toggle_en = {
        let cb = lang.on_toggle.clone();
        Callback::from(move |_| {
            if is_zh { cb() }
        })
    };

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
                        <span>{ strings.nav_dashboard }</span>
                    </a>
                </li>
                <li>
                    <a class={fed_class} onclick={on_click_fed}>
                        <span class="nav-icon">{ "🌐" }</span>
                        <span>{ strings.nav_federation }</span>
                    </a>
                </li>
                <li>
                    <a class={eval_class} onclick={on_click_eval}>
                        <span class="nav-icon">{ "📋" }</span>
                        <span>{ strings.nav_eval_reports }</span>
                    </a>
                </li>
                <li>
                    <a class={metrics_class} onclick={on_click_metrics}>
                        <span class="nav-icon">{ "📈" }</span>
                        <span>{ strings.nav_metrics }</span>
                    </a>
                </li>
                <li>
                    <a class={plugins_class} onclick={on_click_plugins}>
                        <span class="nav-icon">{ "🧩" }</span>
                        <span>{ strings.nav_plugins }</span>
                    </a>
                </li>
                <li>
                    <a class={security_class} onclick={on_click_security}>
                        <span class="nav-icon">{ "🕷️" }</span>
                        <span>{ strings.nav_security }</span>
                    </a>
                </li>
            </ul>

            // ── Sidebar language switcher ──
            <div class="sidebar-lang">
                <div class="sidebar-lang-label">{ strings.lang_switch }</div>
                <div class="sidebar-lang-buttons">
                    <button class={format!("lang-btn{}",
                        if is_zh { " lang-btn-active" } else { "" }
                    )} onclick={toggle_zh}>
                        { strings.lang_zh }
                    </button>
                    <button class={format!("lang-btn{}",
                        if !is_zh { " lang-btn-active" } else { "" }
                    )} onclick={toggle_en}>
                        { strings.lang_en }
                    </button>
                </div>
            </div>

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
  flex: 1;
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
.sidebar-lang {
  padding: 1rem 1.5rem;
  border-top: 1px solid #30363d;
}
.sidebar-lang-label {
  font-size: 0.75rem;
  color: #6e7681;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin-bottom: 0.5rem;
}
.sidebar-lang-buttons {
  display: flex;
  gap: 0.4rem;
}
.lang-btn {
  flex: 1;
  padding: 0.35rem 0.5rem;
  border: 1px solid #30363d;
  border-radius: 6px;
  background: #0d1117;
  color: #8b949e;
  cursor: pointer;
  font-size: 0.8rem;
  text-align: center;
  transition: all 0.15s;
}
.lang-btn:hover { border-color: #58a6ff; color: #c9d1d9; }
.lang-btn-active {
  background: #1f6feb33;
  border-color: #58a6ff;
  color: #58a6ff;
  font-weight: 600;
}
                "##}
            </style>
        </nav>
    }
}
