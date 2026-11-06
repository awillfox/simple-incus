//! Toast notifications.
//!
//! `provide_toasts()` once in `App`, render `<ToastHost/>` once, then call
//! `toast(kind, msg)` from any event handler or async task. Success/Info
//! dismiss after 5 s; errors stay until closed.
use leptos::prelude::*;

use super::icons::{Icon, IconName};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Shows a spinner instead of the kind's icon (work in progress).
    busy: bool,
    msg: RwSignal<String>,
}

#[derive(Clone, Copy)]
struct Toasts {
    list: RwSignal<Vec<ToastItem>>,
    next: StoredValue<u64>,
}

impl Toasts {
    fn dismiss(self, id: u64) {
        // Timers can fire after the app is gone.
        self.list.try_update(|l| l.retain(|t| t.id != id));
    }

    fn schedule_dismiss(self, id: u64) {
        #[cfg(feature = "hydrate")]
        set_timeout(move || self.dismiss(id), std::time::Duration::from_millis(AUTO_DISMISS_MS));
        #[cfg(not(feature = "hydrate"))]
        let _ = id;
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

fn push(kind: ToastKind, busy: bool, msg: String) -> Option<u64> {
    let t = store()?;
    let id = t.next.get_value();
    t.next.set_value(id + 1);
    t.list.update(|l| l.push(ToastItem { id, kind, busy, msg: RwSignal::new(msg) }));
    Some(id)
}

/// Shows a toast. No-op if `provide_toasts()` was never called.
pub fn toast(kind: ToastKind, msg: impl Into<String>) {
    if let (Some(t), Some(id)) = (store(), push(kind, false, msg.into())) {
        if kind != ToastKind::Error {
            t.schedule_dismiss(id);
        }
    }
}

/// Shows a spinner toast that stays until [`toast_finish`] or [`toast_dismiss`].
pub fn toast_busy(msg: impl Into<String>) -> Option<u64> {
    push(ToastKind::Info, true, msg.into())
}

/// Changes the text of a toast in place (live progress).
pub fn toast_set_text(id: u64, msg: impl Into<String>) {
    let Some(t) = store() else { return };
    if let Some(item) = t.list.try_with_untracked(|l| l.iter().find(|i| i.id == id).cloned()).flatten() {
        item.msg.try_set(msg.into());
    }
}

/// Turns a toast into a finished one of `kind`. Errors stay until closed,
/// others dismiss after a few seconds.
pub fn toast_finish(id: u64, kind: ToastKind, msg: impl Into<String>) {
    let Some(t) = store() else { return };
    let msg = msg.into();
    let live = t.list.try_with_untracked(|l| l.iter().any(|i| i.id == id)).unwrap_or(false);
    if !live {
        // The user closed the busy toast; the result must still be seen.
        toast(kind, msg);
        return;
    }
    let msg = RwSignal::new(msg);
    t.list.try_update(|l| {
        if let Some(item) = l.iter_mut().find(|i| i.id == id) {
            *item = ToastItem { id, kind, busy: false, msg };
        }
    });
    if kind != ToastKind::Error {
        t.schedule_dismiss(id);
    }
}

pub fn toast_dismiss(id: u64) {
    if let Some(t) = store() {
        t.dismiss(id);
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
            <For each=move || t.list.get() key=|item| (item.id, item.kind, item.busy) let:item>
                {
                    let (icon, tone) = match item.kind {
                        _ if item.busy => (IconName::Spinner, "text-info"),
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
                            <p class="min-w-0 flex-1 break-words text-[13px] leading-snug text-text">{move || item.msg.get()}</p>
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

#[cfg(test)]
mod tests {
    use super::*;

    fn with_toasts(f: impl FnOnce(Toasts)) {
        let owner = Owner::new();
        owner.set();
        provide_toasts();
        f(store().unwrap());
    }

    #[test]
    fn finishing_a_dismissed_toast_still_shows_the_result() {
        with_toasts(|t| {
            let id = toast_busy("Creating web1...").unwrap();
            toast_dismiss(id);
            toast_finish(id, ToastKind::Error, "Creating web1 failed: boom");
            let list = t.list.get_untracked();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].kind, ToastKind::Error);
            assert!(!list[0].busy);
            assert_eq!(list[0].msg.get_untracked(), "Creating web1 failed: boom");
        });
    }

    #[test]
    fn finishing_a_live_toast_replaces_it() {
        with_toasts(|t| {
            let id = toast_busy("Creating web1...").unwrap();
            toast_finish(id, ToastKind::Error, "failed");
            let list = t.list.get_untracked();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].id, id);
            assert!(!list[0].busy);
        });
    }
}
