//! Instance read + action logic against the fake Incus server.
mod common;

use common::fake_incus::spawn_recording;
use hyper::Method;
use serde_json::{json, Value};
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::{IncusClient, IncusError};
use simple_incus::model::Action;
use simple_incus::server::instances::{delete_instance_impl, instance_action_impl, list_instances_impl};
use simple_incus::state::AppState;

fn fixture(name: &str) -> Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn state(addr: std::net::SocketAddr) -> AppState {
    AppState::new(IncusClient::new(IncusEndpoint::Tcp(addr)), 4)
}

fn instance(status: &str) -> Value {
    json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"",
           "metadata":{"name":"web1","status":status,"type":"container"}})
}

fn wait_ok() -> Value {
    json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"",
           "metadata":{"id":"6c1b2f3e-0000-4a5b-8c9d-1234567890ab","status":"Success","status_code":200,"err":""}})
}

const OP: &str = "/1.0/operations/6c1b2f3e-0000-4a5b-8c9d-1234567890ab/wait";

#[tokio::test]
async fn list_returns_summaries() {
    let (addr, _) = spawn_recording(vec![(
        Method::GET,
        "/1.0/instances",
        200,
        fixture("instances_recursion2.json"),
    )])
    .await;
    let st = state(addr);
    let list = list_instances_impl(&st).await.unwrap();
    assert!(list.iter().any(|i| i.name == "web1"));
    assert!(list.iter().all(|i| i.cpu_pct.is_none()), "first sample has no cpu_pct");
}

#[tokio::test]
async fn action_puts_state() {
    let (addr, calls) = spawn_recording(vec![
        (Method::PUT, "/1.0/instances/web1/state", 200, fixture("operation_running.json")),
    ])
    .await;
    let id = instance_action_impl(&state(addr), "web1", Action::Stop { force: true }).await.unwrap();
    assert_eq!(id, "6c1b2f3e-0000-4a5b-8c9d-1234567890ab");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, Method::PUT);
    let body: Value = serde_json::from_str(&calls[0].2).unwrap();
    assert_eq!(body, json!({"action":"stop","force":true,"timeout":-1}));
}

#[tokio::test]
async fn action_encodes_name() {
    let (addr, calls) = spawn_recording(vec![]).await;
    let _ = instance_action_impl(&state(addr), "a b", Action::Start).await;
    assert_eq!(calls.lock().unwrap()[0].1, "/1.0/instances/a%20b/state");
}

#[tokio::test]
async fn delete_running_without_force_errors() {
    let (addr, calls) = spawn_recording(vec![(Method::GET, "/1.0/instances/web1", 200, instance("Running"))]).await;
    let err = delete_instance_impl(&state(addr), "web1", false).await.unwrap_err();
    match err {
        IncusError::Api { message, .. } => {
            assert_eq!(message, "Instance is running; enable force to stop and delete")
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(calls.lock().unwrap().iter().all(|c| c.0 == Method::GET));
}

#[tokio::test]
async fn delete_running_with_force_stops_then_deletes() {
    let (addr, calls) = spawn_recording(vec![
        (Method::GET, "/1.0/instances/web1", 200, instance("Running")),
        (Method::PUT, "/1.0/instances/web1/state", 200, fixture("operation_running.json")),
        (Method::DELETE, "/1.0/instances/web1", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_ok()),
    ])
    .await;
    delete_instance_impl(&state(addr), "web1", true).await.unwrap();
    let seq: Vec<(String, String)> =
        calls.lock().unwrap().iter().map(|c| (c.0.to_string(), c.1.clone())).collect();
    let s = |m: &str, p: &str| (m.to_string(), p.to_string());
    assert_eq!(
        seq,
        vec![
            s("GET", "/1.0/instances/web1"),
            s("PUT", "/1.0/instances/web1/state"),
            s("GET", OP),
            s("DELETE", "/1.0/instances/web1"),
            s("GET", OP),
        ]
    );
    let body: Value = serde_json::from_str(&calls.lock().unwrap()[1].2).unwrap();
    assert_eq!(body, json!({"action":"stop","force":true,"timeout":-1}));
}

#[tokio::test]
async fn delete_stopped_just_deletes() {
    let (addr, calls) = spawn_recording(vec![
        (Method::GET, "/1.0/instances/web1", 200, instance("Stopped")),
        (Method::DELETE, "/1.0/instances/web1", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_ok()),
    ])
    .await;
    delete_instance_impl(&state(addr), "web1", false).await.unwrap();
    assert_eq!(calls.lock().unwrap().len(), 3);
}
