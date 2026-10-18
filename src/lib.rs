// Deeply nested view! types in the page tests need a higher query depth.
#![recursion_limit = "256"]
pub mod app;
pub mod model;
pub mod server;
pub mod ui;
#[cfg(feature = "ssr")]
pub mod ws;
#[cfg(feature = "ssr")]
pub mod state;
#[cfg(feature = "ssr")]
pub mod config;
#[cfg(feature = "ssr")]
pub mod incus;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
