use yew::prelude::*;

use crate::components::sidebar::Sidebar;
use crate::pages::{Page, Dashboard, Federation, EvalReports, Metrics};

#[function_component(App)]
pub fn app() -> Html {
    let page = use_state(|| Page::Dashboard);

    let on_navigate = {
        let page = page.clone();
        Callback::from(move |p: Page| page.set(p))
    };

    let content = match (*page).clone() {
        Page::Dashboard => html! { <Dashboard /> },
        Page::Federation => html! { <Federation /> },
        Page::EvalReports => html! { <EvalReports /> },
        Page::Metrics => html! { <Metrics /> },
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
