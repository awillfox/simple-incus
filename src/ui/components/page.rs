//! Page scaffolding: header, empty state, skeleton.
use leptos::prelude::*;

use super::icons::{Icon, IconName};

/// Page title row with optional description and right-aligned actions.
#[component]
pub fn PageHeader(
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] actions: Option<ChildrenFn>,
) -> impl IntoView {
    view! {
        <div class="mb-5 flex flex-wrap items-end justify-between gap-3">
            <div class="min-w-0">
                <h1 class="truncate text-[20px] font-semibold tracking-[-0.01em] text-text">{title}</h1>
                {description.map(|d| view! { <p class="mt-1 text-[13px] text-muted">{d}</p> })}
            </div>
            {actions.map(|a| view! { <div class="flex items-center gap-2">{a()}</div> })}
        </div>
    }
}

/// Centered empty state inside a bordered panel.
#[component]
pub fn EmptyState(
    #[prop(optional)] icon: Option<IconName>,
    #[prop(into)] title: String,
    #[prop(optional, into)] body: Option<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let icon = icon.unwrap_or(IconName::Inbox);
    view! {
        <div class="panel flex flex-col items-center justify-center px-6 py-14 text-center">
            <div class="mb-4 flex size-10 items-center justify-center rounded-lg border border-border bg-surface-2 text-muted">
                <Icon name=icon class="size-5"/>
            </div>
            <h2 class="text-[14px] font-semibold text-text">{title}</h2>
            {body.map(|b| view! { <p class="mt-1 max-w-[46ch] text-[13px] text-muted">{b}</p> })}
            {children.map(|c| view! { <div class="mt-5 flex items-center gap-2">{c()}</div> })}
        </div>
    }
}

/// A grey placeholder block; size it with `class` (e.g. "h-3 w-24").
#[component]
pub fn Skeleton(#[prop(into)] class: String) -> impl IntoView {
    view! { <div class=format!("skeleton {class}") aria-hidden="true"></div> }
}
