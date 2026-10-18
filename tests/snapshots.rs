//! Snapshot and instance-detail logic against the fake Incus server.
mod common;

use common::fake_incus::spawn_recording;
use hyper::Method;
use serde_json::{json, Value};
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::{EventHub, IncusClient, IncusError};
use simple_incus::server::instances::get_instance_impl;
use simple_incus::server::snapshots::{
    create_snapshot_impl, default_snapshot_name, delete_snapshot_impl, list_snapshots_impl, restore_snapshot_impl,
    validate_snapshot_name,
};
use simple_incus::state::AppState;

fn fixture(name: &str) -> Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn state(addr: std::net::SocketAddr) -> AppState {
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let hub = EventHub::start(IncusClient::new(IncusEndpoint::Tcp(dead)));
    AppState::with_hub(IncusClient::new(IncusEndpoint::Tcp(addr)), 4, hub)
}

const OP: &str = "/1.0/operations/6c1b2f3e-0000-4a5b-8c9d-1234567890ab/wait";

fn wait_op(status: &str, code: i32, err: &str) -> Value {
    json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"",
           "metadata":{"id":"6c1b2f3e-0000-4a5b-8c9d-1234567890ab","status":status,"status_code":code,"err":err}})
}

fn msg(e: IncusError) -> String {
    match e {
        IncusError::Api { message, .. } => message,
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn restore_sends_restore_body() {
    let (addr, calls) = spawn_recording(vec![
        (Method::PUT, "/1.0/instances/web1", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Success", 200, "")),
    ])
    .await;
    restore_snapshot_impl(&state(addr), "web1", "snap0").await.unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].0, Method::PUT);
    assert_eq!(calls[0].1, "/1.0/instances/web1");
    let body: Value = serde_json::from_str(&calls[0].2).unwrap();
    assert_eq!(body, json!({"restore": "snap0"}));
    assert_eq!(calls[1].1, OP);
}

#[test]
fn snapshot_name_default() {
    // 2026-10-18T10:05:09Z
    assert_eq!(default_snapshot_name(1_792_317_909), "snap-20261018-100509");
    assert_eq!(default_snapshot_name(0), "snap-19700101-000000");
}

#[tokio::test]
async fn empty_name_is_generated_server_side() {
    let (addr, calls) = spawn_recording(vec![
        (Method::POST, "/1.0/instances/web1/snapshots", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Success", 200, "")),
    ])
    .await;
    create_snapshot_impl(&state(addr), "web1", "   ").await.unwrap();
    let body: Value = serde_json::from_str(&calls.lock().unwrap()[0].2).unwrap();
    let name = body["name"].as_str().unwrap();
    assert!(name.starts_with("snap-") && name.len() == "snap-20261018-100509".len(), "{name}");
    assert!(name[5..13].bytes().all(|b| b.is_ascii_digit()) && name.as_bytes()[13] == b'-', "{name}");
    assert_eq!(body["stateful"], json!(false));
}

#[tokio::test]
async fn named_snapshot_is_trimmed_and_sent() {
    let (addr, calls) = spawn_recording(vec![
        (Method::POST, "/1.0/instances/web1/snapshots", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Success", 200, "")),
    ])
    .await;
    create_snapshot_impl(&state(addr), "web1", " before-upgrade ").await.unwrap();
    let body: Value = serde_json::from_str(&calls.lock().unwrap()[0].2).unwrap();
    assert_eq!(body["name"], json!("before-upgrade"));
}

#[test]
fn snapshot_name_rules() {
    for ok in ["a", "snap0", "before-upgrade", "A1-b2", &"a".repeat(63)] {
        assert!(validate_snapshot_name(ok).is_ok(), "{ok}");
    }
    for bad in ["", "0snap", "-snap", "has space", "sl/ash", "under_score", "dot.s", "\u{e9}", &"a".repeat(64)] {
        assert!(validate_snapshot_name(bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn invalid_name_is_rejected_without_calling_incus() {
    let (addr, calls) = spawn_recording(vec![]).await;
    let e = create_snapshot_impl(&state(addr), "web1", "bad name").await.unwrap_err();
    assert!(msg(e).contains("Snapshot name"), "clear message");
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn delete_encodes_and_waits() {
    let (addr, calls) = spawn_recording(vec![
        (Method::DELETE, "/1.0/instances/web1/snapshots/a%20b", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Success", 200, "")),
    ])
    .await;
    delete_snapshot_impl(&state(addr), "web1", "a b").await.unwrap();
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn running_op_after_wait_is_a_timeout() {
    let (addr, _) = spawn_recording(vec![
        (Method::POST, "/1.0/instances/web1/snapshots", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Running", 103, "")),
    ])
    .await;
    let e = create_snapshot_impl(&state(addr), "web1", "s1").await.unwrap_err();
    assert_eq!(msg(e), "Timed out waiting for snapshot creation");

    let (addr, _) = spawn_recording(vec![
        (Method::PUT, "/1.0/instances/web1", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Running", 103, "")),
    ])
    .await;
    let e = restore_snapshot_impl(&state(addr), "web1", "s1").await.unwrap_err();
    assert_eq!(msg(e), "Timed out waiting for snapshot restore");
}

#[tokio::test]
async fn failed_op_reports_incus_error() {
    let (addr, _) = spawn_recording(vec![
        (Method::DELETE, "/1.0/instances/web1/snapshots/s1", 200, fixture("operation_running.json")),
        (Method::GET, OP, 200, wait_op("Failure", 400, "Snapshot is busy")),
    ])
    .await;
    let e = delete_snapshot_impl(&state(addr), "web1", "s1").await.unwrap_err();
    assert_eq!(msg(e), "Snapshot is busy");
}

#[tokio::test]
async fn list_strips_instance_prefix_and_sorts_newest_first() {
    let body = json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"","metadata":[
        {"name":"web1/old","created_at":"2026-10-01T08:00:00Z","stateful":false},
        {"name":"new","created_at":"2026-10-02T08:00:00.5Z","stateful":true}]});
    let (addr, calls) = spawn_recording(vec![(Method::GET, "/1.0/instances/web1/snapshots", 200, body)]).await;
    let list = list_snapshots_impl(&state(addr), "web1").await.unwrap();
    assert_eq!(list.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["new", "old"]);
    assert!(list[0].stateful && !list[1].stateful);
    assert_eq!(calls.lock().unwrap().len(), 1);
}

fn detail_routes() -> Vec<(Method, &'static str, u16, Value)> {
    let inst = json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"","metadata":{
        "name":"web1","status":"Running","type":"container","created_at":"2026-10-01T08:00:00Z","profiles":["default"],
        "config":{"image.os":"Alpine","image.release":"3.21","volatile.eth0.hwaddr":"00:16:3e:aa","user.note":"x",
                  "cloud-init.user-data":"#cloud-config","limits.cpu":"2"},
        "expanded_config":{"image.os":"Alpine","image.release":"3.21","volatile.eth0.hwaddr":"00:16:3e:aa",
                  "limits.cpu":"2","limits.memory":"1GiB","cloud-init.user-data":"#cloud-config","security.nesting":"true"},
        "devices":{},
        "expanded_devices":{"root":{"path":"/","pool":"default","type":"disk"},
                            "eth0":{"name":"eth0","network":"incusbr0","type":"nic"}}}});
    let state = json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"","metadata":{
        "status":"Running","cpu":{"usage":1000},"memory":{"usage":52428800},
        "disk":{"root":{"usage":104857600}},
        "network":{"eth0":{"addresses":[{"family":"inet","address":"10.0.0.7","scope":"global"}],
                            "counters":{"bytes_received":10,"bytes_sent":20}}}}});
    vec![
        (Method::GET, "/1.0/instances/web1", 200, inst),
        (Method::GET, "/1.0/instances/web1/state", 200, state),
    ]
}

#[tokio::test]
async fn detail_has_live_state_filtered_config_and_devices() {
    let (addr, _) = spawn_recording(detail_routes()).await;
    let d = get_instance_impl(&state(addr), "web1").await.unwrap();
    assert_eq!(d.summary.name, "web1");
    assert_eq!(d.summary.ipv4, vec!["10.0.0.7"]);
    assert_eq!(d.summary.mem_used, 52428800);
    assert_eq!(d.summary.mem_limit, Some(1 << 30));
    assert_eq!((d.summary.net_rx, d.summary.net_tx), (10, 20));
    let keys: Vec<&str> = d.config.keys().map(String::as_str).collect();
    assert_eq!(keys, ["cloud-init.user-data", "image.os", "image.release", "limits.cpu", "limits.memory"]);
    let devs: Vec<(&str, &str)> = d.devices.iter().map(|x| (x.name.as_str(), x.kind.as_str())).collect();
    assert_eq!(devs, [("eth0", "nic"), ("root", "disk")]);
    assert_eq!(d.devices[0].detail, "incusbr0");
    assert_eq!(d.devices[1].detail, "default:/");
}

#[tokio::test]
async fn detail_of_unknown_instance_is_404() {
    let (addr, _) = spawn_recording(vec![]).await;
    match get_instance_impl(&state(addr), "nope").await.unwrap_err() {
        IncusError::Api { status, .. } => assert_eq!(status, 404),
        other => panic!("unexpected {other:?}"),
    }
}
