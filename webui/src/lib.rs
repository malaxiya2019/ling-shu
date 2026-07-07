pub mod api;
pub mod app;
pub mod components;
pub mod pages;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn run_app() {
    yew::Renderer::<app::App>::new().render();
}
