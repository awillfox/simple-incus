//! Client core tests against the fake Incus server.
mod common;

use std::time::{Duration, Instant};

use common::fake_incus::{spawn, spawn_silent, spawn_ws_echo};
use futures_util::{SinkExt, StreamExt};
use hyper::Method;
use serde_json::Value;
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::{IncusClient, IncusError, Response};
use tokio_tungstenite::tungstenite::Message;

fn fixture(name: &str) -> Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[tokio::test]
async fn get_unwraps_sync_metadata() {
    let addr = spawn(vec![(Method::GET, "/1.0", 200, fixture("server.json"))]).await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr));
    let v: Value = c.get("/1.0").await.unwrap();
    assert_eq!(v["api_version"], "1.0");
    assert!(c.ping().await);
}

#[tokio::test]
async fn api_error_maps() {
    let addr = spawn(vec![(Method::GET, "/1.0/instances/nope", 404, fixture("error_404.json"))]).await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr));
    let err = c.get::<Value>("/1.0/instances/nope").await.unwrap_err();
    match err {
        IncusError::Api { status, message } => {
            assert_eq!(status, 404);
            assert_eq!(message, "Instance not found");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn async_returns_operation() {
    let addr = spawn(vec![(
        Method::PUT,
        "/1.0/instances/web1/state",
        202,
        fixture("operation_running.json"),
    )])
    .await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr));
    let body = serde_json::json!({"action": "start"});
    let r: Response<Value> = c
        .send(Method::PUT, "/1.0/instances/web1/state", Some(&body))
        .await
        .unwrap();
    match r {
        Response::Async(op) => {
            assert_eq!(op.id, "6c1b2f3e-0000-4a5b-8c9d-1234567890ab");
            assert_eq!(op.status_code, 103);
        }
        Response::Sync(_) => panic!("expected async"),
    }
}

#[tokio::test]
async fn sync_list_decodes_recursion_fixture() {
    let addr = spawn(vec![(Method::GET, "/1.0/instances", 200, fixture("instances_recursion2.json"))]).await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr));
    let v: Vec<Value> = c.get("/1.0/instances").await.unwrap();
    assert_eq!(v[0]["name"], "web1");
}

#[tokio::test]
async fn unreachable_is_error_not_panic() {
    let c = IncusClient::new(IncusEndpoint::Tcp("127.0.0.1:1".parse().unwrap()));
    let err = c.get::<Value>("/1.0").await.unwrap_err();
    assert!(matches!(err, IncusError::Unreachable(_)), "{err:?}");
    assert!(!c.ping().await);
}

#[tokio::test]
async fn stalled_server_times_out_as_unreachable() {
    let addr = spawn_silent().await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr)).with_timeout(Duration::from_millis(300));
    let t = Instant::now();
    let err = c.get::<Value>("/1.0").await.unwrap_err();
    assert!(matches!(err, IncusError::Unreachable(_)), "{err:?}");
    assert!(t.elapsed() < Duration::from_secs(3));
    assert!(!c.ping().await);
}

#[tokio::test]
async fn ws_echo_roundtrip() {
    let addr = spawn_ws_echo().await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr));
    let mut ws = c.ws_connect("/1.0/events").await.unwrap();
    ws.send(Message::text("hi")).await.unwrap();
    let m = ws.next().await.unwrap().unwrap();
    assert_eq!(m.into_text().unwrap().as_str(), "hi");
}

#[tokio::test]
async fn ws_rejected_upgrade_maps_to_api_error() {
    let body = serde_json::json!({"type":"error","error":"forbidden secret","error_code":403});
    let addr = spawn(vec![(Method::GET, "/1.0/operations/x/websocket", 403, body)]).await;
    let c = IncusClient::new(IncusEndpoint::Tcp(addr));
    let err = c.ws_connect("/1.0/operations/x/websocket?secret=bad").await.unwrap_err();
    match err {
        IncusError::Api { status, message } => {
            assert_eq!(status, 403);
            assert_eq!(message, "forbidden secret");
        }
        other => panic!("unexpected {other:?}"),
    }
    let dead = IncusClient::new(IncusEndpoint::Tcp("127.0.0.1:1".parse().unwrap()));
    assert!(matches!(dead.ws_connect("/1.0/events").await, Err(IncusError::Unreachable(_))));
}
