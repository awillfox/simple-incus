//! Toast notifications.
//!
//! `provide_toasts()` once in `App`, render `<ToastHost/>` once, then call
//! `toast(kind, msg)` from any event handler or async task. Success/Info
//! dismiss after 5 s; errors stay until closed.
use leptos::prelude::*;

use super::icons::{Icon, IconName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

pub const AUTO_DISMISS_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToastItem {
    id: u64,
    kind: ToastKind,
    msg: String,
}

#[derive(Clone, Copy)]
struct Toasts {
    list: RwSignal<Vec<ToastItem>>,
    next: StoredValue<u64>,
}

impl Toasts {
    fn dismiss(self, id: u64) {
        self.list.update(|l| l.retain(|t| t.id != id));
    }
}

#[cfg(feature = "hydrate")]
thread_local! {
    // Event handlers / spawned tasks may run without a reactive owner, so
    // the browser keeps a fallback handle (the wasm app is single-threaded).
    static FALLBACK: std::cell::Cell<Option<Toasts>> = const { std::cell::Cell::new(None) };
}

/// Provides the toast store via context. Call once from `App`.
pub fn provide_toasts() {
    let t = Toasts { list: RwSignal::new(Vec::new()), next: StoredValue::new(0) };
    provide_context(t);
    #[cfg(feature = "hydrate")]
    FALLBACK.with(|f| f.set(Some(t)));
}

fn store() -> Option<Toasts> {
    let t = use_context::<Toasts>();
    #[cfg(feature = "hydrate")]
    let t = t.or_else(|| FALLBACK.with(|f| f.get()));
    t
}

/// Shows a toast. No-op if `provide_toasts()` was never called.
pub fn toast(kind: ToastKind, msg: impl Into<String>) {
    let Some(t) = store() else { return };
    let id = t.next.get_value();
    t.next.set_value(id + 1);
    t.list.update(|l| l.push(ToastItem { id, kind, msg: msg.into() }));
    if kind != ToastKind::Error {
        #[cfg(feature = "hydrate")]
        set_timeout(move || t.dismiss(id), std::time::Duration::from_millis(AUTO_DISMISS_MS));
    }
}

/// Renders the toast stack (bottom-right). Place once inside `App`.
#[component]
pub fn ToastHost() -> impl IntoView {
    let Some(t) = store() else { return ().into_any() };
    view! {
        <div
            class="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-[360px] max-w-[calc(100vw-32px)] flex-col gap-2"
            aria-live="polite"
        >
            <For each=move || t.list.get() key=|item| item.id let:item>
                {
                    let (icon, tone) = match item.kind {
                        ToastKind::Success => (IconName::CheckCircle, "text-ok"),
                        ToastKind::Error => (IconName::AlertCircle, "text-danger"),
                        ToastKind::Info => (IconName::Info, "text-info"),
                    };
                    let id = item.id;
                    view! {
                        <div
                            role=if item.kind == ToastKind::Error { "alert" } else { "status" }
                            class="anim-slide-in pointer-events-auto flex items-start gap-3 rounded-lg border border-border-strong bg-surface-2 py-3 pl-3.5 pr-2 shadow-pop"
                        >
                            <span class=format!("mt-px {tone}")>
                                <Icon name=icon/>
                            </span>
                            <p class="min-w-0 flex-1 break-words text-[13px] leading-snug text-text">{item.msg}</p>
                            <button
                                type="button"
                                class="-my-1 flex size-6 shrink-0 items-center justify-center rounded text-muted hover:bg-hover hover:text-text"
                                aria-label="Dismiss"
                                on:click=move |_| t.dismiss(id)
                            >
                                <Icon name=IconName::Close class="size-3.5"/>
                            </button>
                        </div>
                    }
                }
            </For>
        </div>
    }
    .into_any()
}
