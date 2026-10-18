//! SSR render of the app shell.
#![cfg(feature = "ssr")]

use leptos::prelude::*;
use leptos_meta::ServerMetaContext;
use leptos_router::location::RequestUrl;
use simple_incus::app::{shell, App};

fn render_at(path: &str, f: impl FnOnce() -> String) -> String {
    let owner = Owner::new();
    owner.set();
    provide_context(RequestUrl::new(path));
    let (meta, _output) = ServerMetaContext::new();
    provide_context(meta);
    let html = f();
    drop(owner);
    html
}

#[test]
fn shell_renders_nav() {
    let html = render_at("/", || view! { <App/> }.to_html());
    for label in ["Overview", "Instances", "Images"] {
        assert!(html.contains(label), "missing nav label {label}: {html}");
    }
    assert!(!html.contains("<script src=\"http"), "external script in: {html}");

    let options = LeptosOptions::builder().output_name("simple-incus").build();
    let doc = render_at("/", || shell(options).to_html());
    assert!(doc.contains("Instances"));
    for bad in ["src=\"http", "href=\"http", "url(http", "@import url"] {
        assert!(!doc.contains(bad), "external load `{bad}` in shell: {doc}");
    }
    // theme is applied before first paint
    assert!(doc.contains("si-theme"), "missing theme bootstrap script");
}
