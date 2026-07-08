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
    let strings = lang.strings();

    let on_click_dash = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Dashboard))
    };
    let on_click_audit = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Audit))
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
    let on_click_bench = {
        let cb = props.on_navigate.clone();
        Callback::from(move |_| cb.emit(Page::Benchmark))
    };

    let dash_class = if matches!(props.active_page, Page::Dashboard) { "active" } else { "" };
    let audit_class = if matches!(props.active_page, Page::Audit) { "active" } else { "" };
    let fed_class = if matches!(props.active_page, Page::Federation) { "active" } else { "" };
    let eval_class = if matches!(props.active_page, Page::EvalReports) { "active" } else { "" };
    let metrics_class = if matches!(props.active_page, Page::Metrics) { "active" } else { "" };
    let plugins_class = if matches!(props.active_page, Page::Plugins) { "active" } else { "" };
    let security_class = if matches!(props.active_page, Page::Security) { "active" } else { "" };
    let bench_class = if matches!(props.active_page, Page::Benchmark) { "active" } else { "" };

    // Sidebar language switcher
    let is_zh = matches!(lang.locale, Locale::Zh);
    let toggle_zh = {
        let cb = lang.on_toggle.clone();
        Callback::from(move |_| { if !is_zh { cb.emit(()) } })
    };
    let toggle_en = {
        let cb = lang.on_toggle.clone();
        Callback::from(move |_| { if is_zh { cb.emit(()) } })
    };

    html! {
        <nav class="sidebar">
            <div class="sidebar-header">
                <span>{ "⚡" }</span>
                <span>{ "Lingshu" }</span>
            </div>
            <ul class="sidebar-nav">
                <li>
                    <a class={dash_class} onclick={on_click_dash}>
                        <span class="nav-icon">{ "📊" }</span><span>{ strings.nav_dashboard }</span>
                    </a>
                </li>
                <li>
                    <a class={audit_class} onclick={on_click_audit}>
                        <span class="nav-icon">{ "📋" }</span><span>{ strings.nav_audit }</span>
                    </a>
                </li>
                <li>
                    <a class={fed_class} onclick={on_click_fed}>
                        <span class="nav-icon">{ "🌐" }</span><span>{ strings.nav_federation }</span>
                    </a>
                </li>
                <li>
                    <a class={eval_class} onclick={on_click_eval}>
                        <span class="nav-icon">{ "📋" }</span><span>{ strings.nav_eval_reports }</span>
                    </a>
                </li>
                <li>
                    <a class={metrics_class} onclick={on_click_metrics}>
                        <span class="nav-icon">{ "📈" }</span><span>{ strings.nav_metrics }</span>
                    </a>
                </li>
                <li>
                    <a class={bench_class} onclick={on_click_bench}>
                        <span class="nav-icon">{ "⚡" }</span><span>{ strings.nav_benchmark }</span>
                    </a>
                </li>
                <li>
                    <a class={plugins_class} onclick={on_click_plugins}>
                        <span class="nav-icon">{ "🧩" }</span><span>{ strings.nav_plugins }</span>
                    </a>
                </li>
                <li>
                    <a class={security_class} onclick={on_click_security}>
                        <span class="nav-icon">{ "🕷️" }</span><span>{ strings.nav_security }</span>
                    </a>
                </li>
            </ul>

            <div class="sidebar-footer">
                <div>{ strings.lang_switch }</div>
                <div>
                    <button class={format!("lang-btn{}", if is_zh { " lang-btn-active" } else { "" })}
                        onclick={toggle_zh}>{ strings.lang_zh }</button>
                    <button class={format!("lang-btn{}", if !is_zh { " lang-btn-active" } else { "" })}
                        onclick={toggle_en}>{ strings.lang_en }</button>
                </div>
            </div>
        </nav>
    }
}
