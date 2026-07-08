//! 🌐 国际化 / Internationalization (i18n)
//!
//! 支持中文 (zh) 和 English (en) 两种语言。
//! 自动检测浏览器语言，也可以在侧边栏或右下角悬浮按钮手动切换。

use yew::prelude::*;

/// 支持的语言
#[derive(Clone, Debug, PartialEq)]
pub enum Locale {
    Zh,
    En,
}

impl Locale {
    pub fn label(&self) -> &'static str {
        match self {
            Locale::Zh => "中文",
            Locale::En => "English",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            Locale::Zh => Locale::En,
            Locale::En => Locale::Zh,
        }
    }
}

/// 翻译字符串集合
pub struct Translations {
    pub app_title: &'static str,
    pub nav_dashboard: &'static str,
    pub nav_federation: &'static str,
    pub nav_eval_reports: &'static str,
    pub nav_metrics: &'static str,
    pub nav_plugins: &'static str,
    pub nav_security: &'static str,
    pub nav_benchmark: &'static str,
    pub dash_title: &'static str,
    pub dash_system_status: &'static str,
    pub dash_healthy: &'static str,
    pub dash_degraded: &'static str,
    pub dash_plugins: &'static str,
    pub dash_active_sessions: &'static str,
    pub dash_agents: &'static str,
    pub dash_installed_plugins: &'static str,
    pub dash_quick_links: &'static str,
    pub dash_ql_federation: &'static str,
    pub dash_ql_federation_desc: &'static str,
    pub dash_ql_eval: &'static str,
    pub dash_ql_eval_desc: &'static str,
    pub dash_ql_plugins: &'static str,
    pub dash_ql_plugins_desc: &'static str,
    pub dash_ql_api_docs: &'static str,
    pub dash_ql_api_docs_desc: &'static str,
    pub dash_active_count: &'static str,
    pub plugins_title: &'static str,
    pub plugins_installed: &'static str,
    pub plugins_market: &'static str,
    pub plugins_search_placeholder: &'static str,
    pub plugins_search_btn: &'static str,
    pub plugins_no_results: &'static str,
    pub plugins_no_plugins: &'static str,
    pub plugins_start: &'static str,
    pub plugins_stop: &'static str,
    pub plugins_uninstall: &'static str,
    pub plugins_install: &'static str,
    pub plugins_hot_reload_start: &'static str,
    pub plugins_hot_reload_stop: &'static str,
    pub lang_switch: &'static str,
    pub lang_zh: &'static str,
    pub lang_en: &'static str,
    pub loading: &'static str,
    pub error: &'static str,
    pub federation_title: &'static str,
    pub eval_title: &'static str,
    pub metrics_title: &'static str,
    pub security_title: &'static str,
    pub benchmark_title: &'static str,
    pub benchmark_suites: &'static str,
    pub benchmark_samples: &'static str,
    pub benchmark_pass_rate: &'static str,
    pub benchmark_categories: &'static str,
    pub benchmark_throughput: &'static str,
    pub benchmark_error_rate: &'static str,
}

const EN: Translations = Translations {
    app_title: "Lingshu Admin",
    nav_dashboard: "Dashboard",
    nav_federation: "Federation",
    nav_eval_reports: "Eval Reports",
    nav_metrics: "Metrics",
    nav_plugins: "Plugins",
    nav_security: "Security",
    nav_benchmark: "Benchmark",
    dash_title: "📊 Dashboard",
    dash_system_status: "System Status",
    dash_healthy: "Healthy",
    dash_degraded: "Degraded",
    dash_plugins: "Plugins",
    dash_active_sessions: "Active Sessions",
    dash_agents: "Agents",
    dash_installed_plugins: "🧩 Installed Plugins",
    dash_quick_links: "📈 Quick Links",
    dash_ql_federation: "Federation",
    dash_ql_federation_desc: "View cluster status and peers",
    dash_ql_eval: "Eval Reports",
    dash_ql_eval_desc: "Browse evaluation results",
    dash_ql_plugins: "Plugins",
    dash_ql_plugins_desc: "Manage plugins and hot reload",
    dash_ql_api_docs: "API Docs",
    dash_ql_api_docs_desc: "View API documentation",
    dash_active_count: "{} active",
    plugins_title: "🧩 Plugins",
    plugins_installed: "Installed Plugins ({})",
    plugins_market: "Plugin Market",
    plugins_search_placeholder: "Search plugins...",
    plugins_search_btn: "🔍 Search",
    plugins_no_results: "No plugins found. Try a different search term.",
    plugins_no_plugins:
        "No plugins installed. Visit the Plugin Market to find and install plugins.",
    plugins_start: "▶ Start",
    plugins_stop: "⏹ Stop",
    plugins_uninstall: "🗑 Uninstall",
    plugins_install: "📥 Install",
    plugins_hot_reload_start: "▶ Start Hot Reload",
    plugins_hot_reload_stop: "⏹ Stop Hot Reload",
    lang_switch: "🌐 Language",
    lang_zh: "中文",
    lang_en: "English",
    loading: "Loading...",
    error: "Error",
    federation_title: "🌐 Federation",
    eval_title: "📋 Eval Reports",
    metrics_title: "📈 Metrics",
    security_title: "🕷️ Security",
    benchmark_title: "⚡ Benchmark",
    benchmark_suites: "Benchmark Suites",
    benchmark_samples: "Total Samples",
    benchmark_pass_rate: "Pass Rate",
    benchmark_categories: "Categories",
    benchmark_throughput: "Throughput",
    benchmark_error_rate: "Error Rate",
};

const ZH: Translations = Translations {
    app_title: "灵枢管理面板",
    nav_dashboard: "仪表盘",
    nav_federation: "联邦网络",
    nav_eval_reports: "评测报告",
    nav_metrics: "监控指标",
    nav_plugins: "插件管理",
    nav_security: "安全中心",
    nav_benchmark: "基准测试",
    dash_title: "📊 仪表盘",
    dash_system_status: "系统状态",
    dash_healthy: "健康",
    dash_degraded: "降级",
    dash_plugins: "插件",
    dash_active_sessions: "活跃会话",
    dash_agents: "智能体",
    dash_installed_plugins: "🧩 已安装插件",
    dash_quick_links: "📈 快捷入口",
    dash_ql_federation: "联邦网络",
    dash_ql_federation_desc: "查看集群状态和对等节点",
    dash_ql_eval: "评测报告",
    dash_ql_eval_desc: "浏览评测结果",
    dash_ql_plugins: "插件管理",
    dash_ql_plugins_desc: "管理插件与热加载",
    dash_ql_api_docs: "API 文档",
    dash_ql_api_docs_desc: "查看接口文档",
    dash_active_count: "{} 个活跃",
    plugins_title: "🧩 插件管理",
    plugins_installed: "已安装插件 ({})",
    plugins_market: "插件市场",
    plugins_search_placeholder: "搜索插件...",
    plugins_search_btn: "🔍 搜索",
    plugins_no_results: "未找到插件，请尝试其他关键词",
    plugins_no_plugins: "尚未安装任何插件。前往插件市场查找并安装插件。",
    plugins_start: "▶ 启动",
    plugins_stop: "⏹ 停止",
    plugins_uninstall: "🗑 卸载",
    plugins_install: "📥 安装",
    plugins_hot_reload_start: "▶ 启动热加载",
    plugins_hot_reload_stop: "⏹ 停止热加载",
    lang_switch: "🌐 语言",
    lang_zh: "中文",
    lang_en: "English",
    loading: "加载中...",
    error: "错误",
    federation_title: "🌐 联邦网络",
    eval_title: "📋 评测报告",
    metrics_title: "📈 监控指标",
    security_title: "🕷️ 安全中心",
    benchmark_title: "⚡ 基准测试",
    benchmark_suites: "测试套件",
    benchmark_samples: "总采样数",
    benchmark_pass_rate: "通过率",
    benchmark_categories: "类别数",
    benchmark_throughput: "吞吐量",
    benchmark_error_rate: "错误率",
};

/// 获取语言对应的翻译
pub fn t(locale: &Locale) -> &'static Translations {
    match locale {
        Locale::Zh => &ZH,
        Locale::En => &EN,
    }
}

/// 自动检测浏览器语言
pub fn detect_locale() -> Locale {
    let lang = web_sys::window()
        .and_then(|w| w.navigator().language())
        .unwrap_or_default();
    if lang.starts_with("zh") {
        Locale::Zh
    } else {
        Locale::En
    }
}

// ── Yew Context ──────────────────────────────────────

/// 语言上下文 — 通过 Yew Context 传递给所有子组件
#[derive(Clone, Debug, PartialEq)]
pub struct LangContext {
    pub locale: Locale,
    pub on_toggle: Callback<()>,
}

impl LangContext {
    pub fn strings(&self) -> &'static Translations {
        t(&self.locale)
    }
}

/// Hook 获取当前语言上下文
#[hook]
pub fn use_lang() -> LangContext {
    use_context::<LangContext>().expect("LanguageProvider not found")
}

/// 语言 Provider — 包裹在 App 最外层
#[derive(Properties, Clone, PartialEq)]
pub struct LanguageProviderProps {
    pub children: Html,
}

#[function_component(LanguageProvider)]
pub fn language_provider(props: &LanguageProviderProps) -> Html {
    let detected = detect_locale();
    let locale_handle = use_state(|| detected);

    let on_toggle = {
        let h = locale_handle.clone();
        Callback::from(move |_| {
            let next = h.toggle();
            h.set(next);
        })
    };

    let ctx = LangContext {
        locale: (*locale_handle).clone(),
        on_toggle: on_toggle.clone(),
    };

    let locale = (*locale_handle).clone();

    html! {
        <ContextProvider<LangContext> context={ctx}>
            { props.children.clone() }
            <FloatingLangToggle locale={locale} on_toggle={on_toggle} />
        </ContextProvider<LangContext>>
    }
}

/// 右下角悬浮语言切换按钮
#[derive(Properties, Clone, PartialEq)]
struct FloatingLangToggleProps {
    pub on_toggle: Callback<()>,
    pub locale: Locale,
}

#[function_component(FloatingLangToggle)]
fn floating_lang_toggle(props: &FloatingLangToggleProps) -> Html {
    let onclick = {
        let cb = props.on_toggle.clone();
        Callback::from(move |_| cb.emit(()))
    };

    html! {
        <>
            <button class="lang-toggle" onclick={onclick}
                title={format!("Switch to {}", props.locale.toggle().label())}>
                <span class="lang-toggle-icon">{ "🌐" }</span>
                <span class="lang-toggle-label">{ props.locale.label() }</span>
            </button>
            <style>
                {r##"
.lang-toggle {
  position: fixed;
  bottom: 1.2rem;
  right: 1.2rem;
  z-index: 9999;
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 1rem;
  background: #161b22;
  border: 1px solid #30363d;
  border-radius: 20px;
  color: #c9d1d9;
  cursor: pointer;
  font-size: 0.85rem;
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
  transition: border-color 0.15s, transform 0.1s;
}
.lang-toggle:hover {
  border-color: #58a6ff;
  transform: translateY(-1px);
}
.lang-toggle-icon { font-size: 1rem; }
.lang-toggle-label { font-weight: 600; }
                "##}
            </style>
        </>
    }
}
