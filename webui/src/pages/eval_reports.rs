use crate::api::client::{self, EvalResultSummary};
use crate::components::status_card::StatusCard;
use yew::prelude::*;

#[function_component(EvalReports)]
pub fn eval_reports() -> Html {
    let lang = crate::i18n::use_lang();
    let strings = lang.strings;
    let result = use_state(|| None::<EvalResultSummary>);
    let error = use_state(String::new);

    {
        let result = result.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                match client::get_eval_result().await {
                    Ok(r) => result.set(Some(r)),
                    Err(e) => error.set(e),
                }
            });
            || ()
        });
    }

    let (score, eval_status, id, ts) = match result.as_ref() {
        Some(r) => (
            format!("{:.2}", r.score),
            r.status.clone(),
            r.id.clone(),
            r.timestamp.clone(),
        ),
        None => (
            "—".to_string(),
            "pending".to_string(),
            "—".to_string(),
            "—".to_string(),
        ),
    };

    let status_quality = match eval_status.as_str() {
        "passed" | "success" => "ok",
        "failed" | "error" => "err",
        _ => "warn",
    };

    html! {
        <div class="page">
            <h1 class="page-title">{ "📋 Evaluation Reports" }</h1>
            if !error.is_empty() {
                <div class="error-banner">{ error.as_str() }</div>
            }
            <div class="cards-row">
                <StatusCard title="Score"     value={score}       status={Some(status_quality.to_string())} icon={Some("🎯".to_string())} />
                <StatusCard title="Status"    value={eval_status} status={Some(status_quality.to_string())} icon={Some("📊".to_string())} />
                <StatusCard title="Eval ID"   value={id}          status={Some("ok".to_string())} icon={Some("🆔".to_string())} />
                <StatusCard title="Timestamp" value={ts}          status={Some("ok".to_string())} icon={Some("🕐".to_string())} />
            </div>

            {if let Some(r) = result.as_ref() {
                if !r.metrics.is_empty() {
                    html! {
                        <div class="section">
                            <h2>{ "📈 Metrics" }</h2>
                            <table class="metrics-table">
                                <thead><tr><th>{ "Metric" }</th><th>{ "Value" }</th></tr></thead>
                                <tbody>
                                    { for r.metrics.iter().map(|(k, v)| {
                                        html! {
                                            <tr>
                                                <td>{ k }</td>
                                                <td class="metric-value">{ format!("{:.4}", v) }</td>
                                            </tr>
                                        }
                                    })}
                                </tbody>
                            </table>
                        </div>
                    }
                } else {
                    html! {}
                }
            } else {
                html! {}
            }}

            <style>
                {r##"
.page { margin-left: 240px; padding: 2rem; }
.page-title { font-size: 1.5rem; margin-bottom: 1.5rem; color: #c9d1d9; }
.error-banner { background: #f8514933; color: #f85149; padding: 0.5rem 1rem; border-radius: 6px; margin-bottom: 1rem; }
.cards-row { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 2rem; }
.section { margin: 1.5rem 0; }
.section h2 { font-size: 1.1rem; color: #c9d1d9; margin-bottom: 0.8rem; }
.metrics-table { width: 100%; max-width: 500px; border-collapse: collapse; }
.metrics-table th, .metrics-table td { text-align: left; padding: 0.5rem; border-bottom: 1px solid #21262d; }
.metrics-table th { color: #58a6ff; font-size: 0.8rem; text-transform: uppercase; }
.metrics-table td { color: #c9d1d9; }
.metric-value { font-family: monospace; color: #3fb950; }
                "##}
            </style>
        </div>
    }
}
