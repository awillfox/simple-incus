//! Underlined tab strip of links (`<base>/<slug>`).
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Tab links. Each tab links to `<base>/<slug>`. `base` defaults to the
/// current path with a trailing tab slug removed; pass it explicitly when the
/// path could be ambiguous (for example an instance named like a tab).
#[component]
pub fn Tabs(
    /// `(slug, label)` pairs.
    tabs: Vec<(String, String)>,
    #[prop(into)] active: Signal<String>,
    #[prop(optional, into)] base: Option<Signal<String>>,
) -> impl IntoView {
    let location = use_location();
    let slugs: Vec<String> = tabs.iter().map(|(s, _)| s.clone()).collect();
    let base = Signal::derive(move || match base {
        Some(b) => b.get().trim_end_matches('/').to_string(),
        None => {
            let path = location.pathname.get();
            let path = path.trim_end_matches('/');
            match path.rsplit_once('/') {
                Some((head, last)) if slugs.iter().any(|s| s == last) => head.to_string(),
                _ => path.to_string(),
            }
        }
    });
    view! {
        <nav class="flex gap-1 overflow-x-auto border-b border-border" aria-label="Tabs">
            {tabs
                .into_iter()
                .map(|(slug, label)| {
                    let s = slug.clone();
                    let is_active = Memo::new(move |_| active.get() == s);
                    view! {
                        <a
                            href=move || format!("{}/{}", base.get(), slug)
                            aria-current=move || is_active.get().then_some("page")
                            class=move || {
                                if is_active.get() {
                                    "-mb-px whitespace-nowrap border-b-2 border-accent px-3 pb-2.5 pt-1 text-[13px] font-medium text-text"
                                } else {
                                    "-mb-px whitespace-nowrap border-b-2 border-transparent px-3 pb-2.5 pt-1 text-[13px] font-medium text-muted transition-colors hover:border-border-strong hover:text-text"
                                }
                            }
                        >
                            {label}
                        </a>
                    }
                })
                .collect_view()}
        </nav>
    }
}
