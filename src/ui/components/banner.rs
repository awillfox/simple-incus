//! Full-width banner shown while the server cannot reach Incus.
use leptos::prelude::*;

use super::icons::{Icon, IconName};
use crate::ui::events::use_incus_connected;

#[component]
pub fn IncusBanner() -> impl IntoView {
    let connected = use_incus_connected();
    view! {
        <Show when=move || !connected.get()>
            <div
                role="alert"
                class="flex h-8 shrink-0 items-center justify-center gap-2 bg-danger-strong px-4 text-[13px] font-medium text-white"
            >
                <Icon name=IconName::AlertTriangle class="size-3.5"/>
                <span class="truncate">"Incus is unreachable — retrying…"</span>
            </div>
        </Show>
    }
}
