use yew::prelude::*;

use crate::components::sidebar::Sidebar;
use crate::i18n::LanguageProvider;
use crate::pages::{
    AuditDashboard, Benchmark, Dashboard, EvalReports, Federation, Metrics, Page, Plugins,
    Security,
};

#[function_component(App)]
pub fn app() -> Html {
    html! {
        <LanguageProvider>
            <AppInner />
        </LanguageProvider>
    }
}

#[function_component(AppInner)]
fn app_inner() -> Html {
    let page = use_state(|| Page::Dashboard);

    let on_navigate = {
        let page = page.clone();
        Callback::from(move |p: Page| page.set(p))
    };

    let content = match (*page).clone() {
        Page::Dashboard => html! { <Dashboard /> },
        Page::Audit => html! { <AuditDashboard /> },
        Page::Federation => html! { <Federation /> },
        Page::EvalReports => html! { <EvalReports /> },
        Page::Metrics => html! { <Metrics /> },
        Page::Plugins => html! { <Plugins /> },
        Page::Security => html! { <Security /> },
        Page::Benchmark => html! { <Benchmark /> },
        Page::Tenant => html! { <TenantDashboard /> },
    };

    html! {
        <>
            <Sidebar active_page={(*page).clone()} on_navigate={on_navigate} />
            <main>
                { content }
            </main>
        </>
    }
}
