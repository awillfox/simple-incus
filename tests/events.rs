//! Event hub: mapping and reconnect against the fake Incus websocket.
mod common;

use std::time::Duration;

use common::fake_incus::spawn_ws_script;
use serde_json::{json, Value};
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::events::map_event;
use simple_incus::incus::{EventHub, IncusClient};
use simple_incus::model::{StatusKind, UiEvent, UiEventKind, WsMsg};

fn lifecycle(name: &str) -> Value {
    json!({"type":"lifecycle","timestamp":"2026-10-18T10:00:00Z","project":"default",
           "metadata":{"action":"instance-started","source":format!("/1.0/instances/{name}"),"context":{}}})
}

#[test]
fn maps_lifecycle_event() {
    let ev = map_event(&lifecycle("c1")).unwrap();
    assert_eq!(
        ev,
        UiEvent {
            kind: UiEventKind::Lifecycle,
            instance: Some("c1".into()),
            op_id: None,
            progress: None,
            message: "instance-started".into()
        }
    );
    assert_eq!(map_event(&lifecycle("my%20vm")).unwrap().instance.as_deref(), Some("my vm"));
    assert!(map_event(&json!({"type":"logging","metadata":{}})).is_none());
}

#[test]
fn maps_operation_progress() {
    let v = json!({"type":"operation","metadata":{"id":"abc-123","class":"task",
        "description":"Creating instance","status":"Running","status_code":103,
        "resources":{"instances":["/1.0/instances/c1"]},
        "metadata":{"download_progress":"rootfs: 45% (12.3MB/s)"},"err":""}});
    let ev = map_event(&v).unwrap();
    assert_eq!(ev.kind, UiEventKind::Operation);
    assert_eq!(ev.instance.as_deref(), Some("c1"));
    assert_eq!(ev.op_id.as_deref(), Some("abc-123"));
    assert_eq!(ev.progress.as_deref(), Some("rootfs: 45% (12.3MB/s)"));
    assert_eq!(ev.message, "Creating instance");

    let failed = json!({"type":"operation","metadata":{"id":"x","description":"Creating instance","err":"boom"}});
    assert_eq!(map_event(&failed).unwrap().message, "Creating instance failed: boom");
}

#[test]
fn ws_messages_round_trip() {
    assert_eq!(serde_json::to_string(&WsMsg::status(true)).unwrap(), r#"{"kind":"Status","connected":true}"#);
    let ev = map_event(&lifecycle("c1")).unwrap();
    let text = serde_json::to_string(&WsMsg::Event(ev.clone())).unwrap();
    assert_eq!(serde_json::from_str::<WsMsg>(&text).unwrap(), WsMsg::Event(ev));
    let st: WsMsg = serde_json::from_str(r#"{"kind":"Status","connected":false}"#).unwrap();
    assert_eq!(st, WsMsg::Status { kind: StatusKind::Status, connected: false });
}

#[tokio::test]
async fn reconnects_after_drop() {
    let addr = spawn_ws_script(
        vec![vec![lifecycle("first")], vec![lifecycle("second")]],
        Duration::from_millis(150),
    )
    .await;
    let hub = EventHub::start_with_backoff(IncusClient::new(IncusEndpoint::Tcp(addr)), Duration::from_millis(10));
    let mut events = hub.subscribe();
    let mut connected = hub.connected();
    let t = Duration::from_secs(5);

    tokio::time::timeout(t, connected.wait_for(|c| *c)).await.unwrap().unwrap();
    let first = tokio::time::timeout(t, events.recv()).await.unwrap().unwrap();
    assert_eq!(first.instance.as_deref(), Some("first"));
    tokio::time::timeout(t, connected.wait_for(|c| !*c)).await.unwrap().unwrap();
    tokio::time::timeout(t, connected.wait_for(|c| *c)).await.unwrap().unwrap();
    let second = tokio::time::timeout(t, events.recv()).await.unwrap().unwrap();
    assert_eq!(second.instance.as_deref(), Some("second"));
}

#[test]
fn backoff_sequence_is_1_2_4_8_16_30_30() {
    let base = Duration::from_secs(1);
    let mut d = base;
    let mut seq = vec![d.as_secs()];
    for _ in 0..6 {
        d = simple_incus::incus::events::next_delay(d, base);
        seq.push(d.as_secs());
    }
    assert_eq!(seq, [1, 2, 4, 8, 16, 30, 30]);
}

#[tokio::test]
async fn keeps_retrying_until_incus_appears() {
    // A listener that drops every connection: each attempt fails fast and is counted
    // (a closed port can take seconds to refuse on Windows).
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = attempts.clone();
    let dropper = tokio::spawn(async move {
        while let Ok((sock, _)) = l.accept().await {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            drop(sock);
        }
    });
    let hub = EventHub::start_with_backoff(IncusClient::new(IncusEndpoint::Tcp(addr)), Duration::from_millis(5));
    let mut events = hub.subscribe();
    let mut connected = hub.connected();
    // Several failed backoff cycles with nothing speaking websocket.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while attempts.load(std::sync::atomic::Ordering::SeqCst) < 4 {
        assert!(tokio::time::Instant::now() < deadline, "hub is not retrying");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!*connected.borrow());
    dropper.abort();
    let _ = dropper.await;

    // incusd comes up on the same port.
    common::fake_incus::spawn_ws_script_at(addr, vec![vec![lifecycle("late")]], Duration::from_millis(100)).await;
    let t = Duration::from_secs(5);
    tokio::time::timeout(t, connected.wait_for(|c| *c)).await.unwrap().unwrap();
    let ev = tokio::time::timeout(t, events.recv()).await.unwrap().unwrap();
    assert_eq!(ev.instance.as_deref(), Some("late"));
}

#[tokio::test]
async fn browser_socket_sends_status_then_events() {
    use futures_util::StreamExt;
    let upstream = spawn_ws_script(vec![vec![lifecycle("c9")]], Duration::from_millis(300)).await;
    let hub = EventHub::start_with_backoff(IncusClient::new(IncusEndpoint::Tcp(upstream)), Duration::from_millis(10));
    let app = axum::Router::new().route(
        "/ws/events",
        axum::routing::get(move |ws: axum::extract::ws::WebSocketUpgrade| {
            let hub = hub.clone();
            async move { simple_incus::ws::events::events_ws(ws, hub) }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await });

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/events")).await.unwrap();
    let first: WsMsg = serde_json::from_str(
        tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap().to_text().unwrap(),
    )
    .unwrap();
    assert!(matches!(first, WsMsg::Status { .. }), "first frame must be a status");
    // Read until the event arrives; status frames may come before it.
    let expected = map_event(&lifecycle("c9")).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let m = tokio::time::timeout_at(deadline, ws.next()).await.unwrap().unwrap().unwrap();
        if let WsMsg::Event(ev) = serde_json::from_str::<WsMsg>(m.to_text().unwrap()).unwrap() {
            assert_eq!(ev, expected);
            break;
        }
    }
}
