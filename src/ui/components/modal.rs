//! Confirmation dialog (replaces `confirm()`; never use the browser dialogs).
use leptos::html;
use leptos::prelude::*;

use super::icons::{Icon, IconName};

/// Modal confirm dialog. Escape or the backdrop cancels; focus starts on Cancel.
#[component]
pub fn ConfirmModal(
    open: RwSignal<bool>,
    #[prop(into)] title: String,
    #[prop(into)] body: String,
    #[prop(into)] confirm_label: String,
    #[prop(optional)] danger: bool,
    on_confirm: Callback<()>,
    /// Extra content between body and buttons (e.g. a "force" checkbox).
    #[prop(optional)]
    extra: Option<ChildrenFn>,
) -> impl IntoView {
    let cancel_ref = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if open.get() {
            if let Some(b) = cancel_ref.get() {
                let _ = b.focus();
            }
        }
    });
    let close = move || open.set(false);

    view! {
        <Show when=move || open.get()>
            <div
                class="fixed inset-0 z-50 flex items-center justify-center p-4"
                on:keydown=move |e| {
                    if e.key() == "Escape" {
                        close();
                    }
                }
            >
                <div class="anim-fade-in absolute inset-0 bg-overlay" on:click=move |_| close()></div>
                <div
                    role="alertdialog"
                    aria-modal="true"
                    aria-label=title.clone()
                    class="panel anim-pop-in relative w-full max-w-[440px] overflow-hidden shadow-pop"
                >
                    <div class="flex gap-3 px-5 pb-4 pt-5">
                        {danger
                            .then(|| {
                                view! {
                                    <div class="flex size-8 shrink-0 items-center justify-center rounded-full bg-danger/12 text-danger">
                                        <Icon name=IconName::AlertTriangle class="size-4"/>
                                    </div>
                                }
                            })}
                        <div class="min-w-0 flex-1">
                            <h2 class="text-[15px] font-semibold text-text">{title.clone()}</h2>
                            <p class="mt-1.5 text-[13px] leading-relaxed text-text-2">{body.clone()}</p>
                            {extra.as_ref().map(|x| view! { <div class="mt-3">{x()}</div> })}
                        </div>
                    </div>
                    <div class="flex justify-end gap-2 border-t border-border bg-surface-2 px-5 py-3">
                        <button type="button" class="btn" node_ref=cancel_ref on:click=move |_| close()>
                            "Cancel"
                        </button>
                        <button
                            type="button"
                            class=if danger { "btn btn-danger" } else { "btn btn-primary" }
                            on:click=move |_| {
                                on_confirm.run(());
                                close();
                            }
                        >
                            {confirm_label.clone()}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
