//! Right-hand slide-over panel, plus the app-wide create-instance drawer state.
use leptos::html;
use leptos::prelude::*;

use super::icons::{Icon, IconName};
use crate::model::ImageSource;

/// Shared state for the create-instance drawer (its content arrives in a later
/// task). Open it from anywhere with `use_create_drawer().open_with(..)`.
#[derive(Clone, Copy)]
pub struct CreateDrawerState {
    pub open: RwSignal<bool>,
    pub prefill: RwSignal<Option<ImageSource>>,
}

impl CreateDrawerState {
    pub fn new() -> Self {
        CreateDrawerState { open: RwSignal::new(false), prefill: RwSignal::new(None) }
    }

    /// Opens the drawer, optionally preselecting an image.
    pub fn open_with(&self, prefill: Option<ImageSource>) {
        self.prefill.set(prefill);
        self.open.set(true);
    }
}

impl Default for CreateDrawerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Provides [`CreateDrawerState`] via context. Called once from `App`.
pub fn provide_create_drawer() -> CreateDrawerState {
    let s = CreateDrawerState::new();
    provide_context(s);
    s
}

pub fn use_create_drawer() -> CreateDrawerState {
    use_context::<CreateDrawerState>().expect("provide_create_drawer() must be called from App")
}

/// Slide-over panel from the right. Escape, the close button or the backdrop closes it.
#[component]
pub fn Drawer(open: RwSignal<bool>, #[prop(into)] title: String, children: ChildrenFn) -> impl IntoView {
    let close_ref = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if open.get() {
            if let Some(b) = close_ref.get() {
                let _ = b.focus();
            }
        }
    });
    view! {
        <Show when=move || open.get()>
            <div
                class="fixed inset-0 z-40"
                on:keydown=move |e| {
                    if e.key() == "Escape" {
                        open.set(false);
                    }
                }
            >
                <div class="anim-fade-in absolute inset-0 bg-overlay" on:click=move |_| open.set(false)></div>
                <aside
                    role="dialog"
                    aria-modal="true"
                    aria-label=title.clone()
                    class="anim-slide-in absolute inset-y-0 right-0 flex w-full flex-col border-l border-border bg-surface shadow-pop sm:w-[520px]"
                >
                    <header class="flex h-14 shrink-0 items-center justify-between gap-3 border-b border-border px-5">
                        <h2 class="truncate text-[15px] font-semibold text-text">{title.clone()}</h2>
                        <button
                            type="button"
                            class="btn btn-ghost btn-icon"
                            aria-label="Close"
                            node_ref=close_ref
                            on:click=move |_| open.set(false)
                        >
                            <Icon name=IconName::Close/>
                        </button>
                    </header>
                    <div class="min-h-0 flex-1 overflow-y-auto p-5">{children()}</div>
                </aside>
            </div>
        </Show>
    }
}
