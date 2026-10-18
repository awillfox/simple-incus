//! Browser-side live event stream.
//!
//! `provide_event_stream()` must be called exactly once, from `App`; it owns the
//! single shared `/ws/events` connection (hydrate only) and exposes it through
//! context. Components then call [`use_events`] / [`use_incus_connected`].
//! On ssr the signals are inert (`connected` defaults to true so the server
//! render never flashes the "Incus unreachable" banner).
use leptos::prelude::*;

use crate::model::{UiEvent, UiEventKind};

#[derive(Clone, Copy)]
struct EventStream {
    event: ReadSignal<Option<UiEvent>>,
    connected: ReadSignal<bool>,
    activity: ReadSignal<Vec<Activity>>,
}

/// One entry of the activity feed.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    /// Unix seconds when the browser received the event.
    pub at: i64,
    pub event: UiEvent,
}

/// The feed keeps at most this many entries.
pub const ACTIVITY_MAX: usize = 20;

/// Adds `event` at the front (newest first). Repeated updates of one operation
/// replace its earlier entry so progress moves in place; the feed is capped.
pub fn record_activity(log: &mut Vec<Activity>, event: UiEvent, at: i64) {
    if event.kind == UiEventKind::Operation {
        if let Some(id) = event.op_id.as_deref() {
            log.retain(|a| a.event.op_id.as_deref() != Some(id));
        }
    }
    log.insert(0, Activity { at, event });
    log.truncate(ACTIVITY_MAX);
}

/// Creates the shared connection and provides it via context. Call once from `App`.
pub fn provide_event_stream() {
    let (event, set_event) = signal(None::<UiEvent>);
    let (connected, set_connected) = signal(true);
    let (activity, set_activity) = signal(Vec::<Activity>::new());
    provide_context(EventStream { event, connected, activity });

    // Every event since the page loaded feeds the overview's activity list
    // (kept here so it survives client-side navigation).
    Effect::new(move |_| {
        if let Some(ev) = event.get() {
            set_activity.update(|log| record_activity(log, ev, crate::ui::format::now_secs()));
        }
    });

    #[cfg(feature = "hydrate")]
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            ws::connect(set_event, set_connected);
        }
    });
    #[cfg(not(feature = "hydrate"))]
    let _ = (set_event, set_connected);
}

fn stream() -> EventStream {
    use_context::<EventStream>().expect("provide_event_stream() must be called from App")
}

/// Latest event received from the server (`None` until the first one).
pub fn use_events() -> ReadSignal<Option<UiEvent>> {
    stream().event
}

/// Recent events since the page loaded, newest first (at most [`ACTIVITY_MAX`]).
pub fn use_activity() -> ReadSignal<Vec<Activity>> {
    stream().activity
}

/// Whether the server can currently reach Incus (false also while our own socket is down).
pub fn use_incus_connected() -> ReadSignal<bool> {
    stream().connected
}

#[cfg(feature = "hydrate")]
mod ws {
    use leptos::prelude::*;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{CloseEvent, MessageEvent, WebSocket};

    use crate::model::{UiEvent, WsMsg};

    const RECONNECT_MS: i32 = 2000;

    fn url() -> Option<String> {
        let loc = web_sys::window()?.location();
        let scheme = if loc.protocol().ok()? == "https:" { "wss" } else { "ws" };
        Some(format!("{scheme}://{}/ws/events", loc.host().ok()?))
    }

    fn retry(set_event: WriteSignal<Option<UiEvent>>, set_connected: WriteSignal<bool>) {
        let Some(window) = web_sys::window() else { return };
        let cb = Closure::once_into_js(move || connect(set_event, set_connected));
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), RECONNECT_MS);
    }

    pub fn connect(set_event: WriteSignal<Option<UiEvent>>, set_connected: WriteSignal<bool>) {
        let Some(sock) = url().and_then(|u| WebSocket::new(&u).ok()) else {
            retry(set_event, set_connected);
            return;
        };
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let Some(text) = e.data().as_string() else { return };
            match serde_json::from_str::<WsMsg>(&text) {
                Ok(WsMsg::Event(ev)) => set_event.set(Some(ev)),
                Ok(WsMsg::Status { connected, .. }) => set_connected.set(connected),
                Err(_) => {}
            }
        });
        sock.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        let on_close = Closure::<dyn FnMut(CloseEvent)>::new(move |_: CloseEvent| {
            set_connected.set(false);
            retry(set_event, set_connected);
        });
        sock.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: UiEventKind, op: Option<&str>, progress: Option<&str>) -> UiEvent {
        UiEvent {
            kind,
            instance: Some("web1".into()),
            op_id: op.map(str::to_string),
            progress: progress.map(str::to_string),
            message: "m".into(),
        }
    }

    #[test]
    fn activity_is_newest_first_and_capped() {
        let mut log = Vec::new();
        for i in 0..25 {
            record_activity(&mut log, ev(UiEventKind::Lifecycle, None, None), i);
        }
        assert_eq!(log.len(), ACTIVITY_MAX);
        assert_eq!(log[0].at, 24);
        assert_eq!(log[19].at, 5);
    }

    #[test]
    fn operation_updates_replace_in_place() {
        let mut log = Vec::new();
        record_activity(&mut log, ev(UiEventKind::Operation, Some("a"), Some("10%")), 1);
        record_activity(&mut log, ev(UiEventKind::Lifecycle, None, None), 2);
        record_activity(&mut log, ev(UiEventKind::Operation, Some("a"), Some("60%")), 3);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].event.progress.as_deref(), Some("60%"));
        assert_eq!(log[0].at, 3);
    }
}
