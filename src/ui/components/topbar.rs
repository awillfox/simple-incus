//! Top bar: navigation toggle, breadcrumb, live connection state, theme toggle.
use leptos::prelude::*;
use leptos_router::hooks::use_location;

use super::icons::{Icon, IconName};
use super::sidebar::{NavState, NAV_ITEMS};
use crate::ui::events::use_incus_connected;
use crate::ui::theme::{self, Theme};

/// Breadcrumb for a path: section `(href, label)` plus an optional detail item.
pub fn breadcrumb(path: &str) -> (&'static str, &'static str, Option<String>) {
    let mut parts = path.trim_matches('/').split('/').filter(|s| !s.is_empty());
    let section = parts.next().unwrap_or("");
    let detail = parts.next().map(str::to_string);
    for (href, label, _) in NAV_ITEMS {
        if href.trim_start_matches('/') == section {
            return (href, label, detail);
        }
    }
    ("/", "Overview", None)
}

#[component]
pub fn Topbar() -> impl IntoView {
    let nav = use_context::<NavState>().expect("provide_nav_state() must be called from App");
    let location = use_location();
    let connected = use_incus_connected();
    let theme_sig = theme::use_theme();

    let crumbs = move || {
        let (href, label, detail) = breadcrumb(&location.pathname.get());
        match detail {
            Some(d) => view! {
                <a href=href class="truncate text-muted transition-colors hover:text-text">{label}</a>
                <span class="text-border-strong" aria-hidden="true">"/"</span>
                <span class="mono truncate text-[13px] text-text" aria-current="page">{d}</span>
            }
            .into_any(),
            None => view! { <span class="truncate text-text" aria-current="page">{label}</span> }.into_any(),
        }
    };

    view! {
        <header class="flex h-12 shrink-0 items-center gap-3 border-b border-border bg-surface px-3 md:px-5">
            <button
                type="button"
                class="btn btn-ghost btn-icon -ml-1 md:hidden"
                aria-label="Open navigation"
                on:click=move |_| nav.mobile_open.set(true)
            >
                <Icon name=IconName::Menu/>
            </button>
            <nav class="flex min-w-0 flex-1 items-center gap-2 text-[13px] font-medium" aria-label="Breadcrumb">
                {crumbs}
            </nav>
            <div
                class="flex h-7 items-center gap-2 rounded-full border border-border px-2 text-[12px] text-text-2 sm:px-2.5"
                title=move || if connected.get() { "Connected to Incus" } else { "Incus unreachable" }
            >
                <span class="relative flex size-2">
                    <span class=move || {
                        if connected.get() {
                            "absolute inset-0 rounded-full bg-ok opacity-60 motion-safe:animate-ping [animation-duration:2.4s]"
                        } else {
                            "hidden"
                        }
                    }></span>
                    <span class=move || {
                        if connected.get() { "relative size-2 rounded-full bg-ok" } else { "relative size-2 rounded-full bg-danger" }
                    }></span>
                </span>
                <span class="hidden sm:inline">{move || if connected.get() { "Connected" } else { "Unreachable" }}</span>
            </div>
            <button
                type="button"
                class="btn btn-ghost btn-icon"
                aria-label=move || {
                    if theme_sig.get() == Theme::Dark { "Switch to light theme" } else { "Switch to dark theme" }
                }
                title=move || if theme_sig.get() == Theme::Dark { "Light theme" } else { "Dark theme" }
                on:click=move |_| {
                    let next = theme_sig.get_untracked().toggled();
                    theme::apply(next);
                    theme_sig.set(next);
                }
            >
                {move || {
                    if theme_sig.get() == Theme::Dark {
                        view! { <Icon name=IconName::Sun/> }.into_any()
                    } else {
                        view! { <Icon name=IconName::Moon/> }.into_any()
                    }
                }}
            </button>
        </header>
    }
}

#[cfg(test)]
mod tests {
    use super::breadcrumb;

    #[test]
    fn crumbs() {
        assert_eq!(breadcrumb("/"), ("/", "Overview", None));
        assert_eq!(breadcrumb("/instances"), ("/instances", "Instances", None));
        assert_eq!(breadcrumb("/instances/web/console"), ("/instances", "Instances", Some("web".into())));
        assert_eq!(breadcrumb("/images"), ("/images", "Images", None));
    }
}
