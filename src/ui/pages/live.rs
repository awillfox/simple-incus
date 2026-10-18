//! A refetch trigger shared by the pages that show live data.
use leptos::prelude::*;

use super::actions::after_ms;
#[cfg(feature = "hydrate")]
use super::actions::page_visible;
use crate::model::UiEventKind;
use crate::ui::events::{use_events, use_incus_connected};

/// Lifecycle events arrive in bursts; refetch once after they settle.
const DEBOUNCE_MS: u64 = 300;

/// A counter that changes whenever a resource keyed on it should refetch:
/// debounced after Lifecycle events, right after Incus (or our socket) comes
/// back, and every `poll_ms` while the tab is visible. Timers can outlive the
/// page, so the counter is only touched through `try_*` accessors.
pub fn live_tick(poll_ms: u64) -> RwSignal<u32> {
    let tick = RwSignal::new(0u32);
    let bump = move || {
        tick.try_update(|t| *t = t.wrapping_add(1));
    };

    let events = use_events();
    let generation = StoredValue::new(0u32);
    Effect::new(move |prev: Option<()>| {
        // The signal keeps the last event of an earlier page; only react to new ones.
        let Some(ev) = events.get().filter(|_| prev.is_some()) else { return };
        if ev.kind != UiEventKind::Lifecycle {
            return;
        }
        let Some(g) = generation.try_get_value().map(|g| g.wrapping_add(1)) else { return };
        generation.try_set_value(g);
        after_ms(DEBOUNCE_MS, move || {
            if generation.try_get_value() == Some(g) {
                bump();
            }
        });
    });

    let connected = use_incus_connected();
    Effect::new(move |was: Option<bool>| {
        let now = connected.get();
        if was == Some(false) && now {
            bump();
        }
        now
    });

    #[cfg(feature = "hydrate")]
    if let Ok(handle) = set_interval_with_handle(
        move || {
            if page_visible() {
                bump();
            }
        },
        std::time::Duration::from_millis(poll_ms),
    ) {
        on_cleanup(move || handle.clear());
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = poll_ms;

    tick
}

/// Requests a refetch of everything keyed on `tick`.
pub fn bump(tick: RwSignal<u32>) {
    tick.try_update(|t| *t = t.wrapping_add(1));
}
