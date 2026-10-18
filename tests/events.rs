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

#[tokio::test]
async fn unreachable_incus_stays_disconnected_without_panic() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    let hub = EventHub::start_with_backoff(IncusClient::new(IncusEndpoint::Tcp(addr)), Duration::from_millis(5));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!*hub.connected().borrow());
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
    let mut got = Vec::new();
    while got.len() < 3 {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        got.push(serde_json::from_str::<WsMsg>(m.to_text().unwrap()).unwrap());
    }
    assert!(matches!(got[0], WsMsg::Status { .. }));
    assert!(got.contains(&WsMsg::status(true)));
    assert!(got.iter().any(|m| matches!(m, WsMsg::Event(e) if e.instance.as_deref() == Some("c9"))));
}
