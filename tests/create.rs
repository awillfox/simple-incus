//! Advanced create: name rules, request body and the no-wait guarantee.
mod common;

use std::time::{Duration, Instant};

use common::fake_incus::spawn_recording;
use hyper::Method;
use serde_json::{json, Value};
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::{EventHub, IncusClient, IncusError};
use simple_incus::model::{CreateRequest, CreateStatus, ImageSource, InstanceKind};
use simple_incus::server::create::{create_body, create_instance_impl, create_options_impl, create_status_impl};
use simple_incus::state::AppState;

fn state(addr: std::net::SocketAddr) -> AppState {
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let hub = EventHub::start(IncusClient::new(IncusEndpoint::Tcp(dead)));
    AppState::with_hub(IncusClient::new(IncusEndpoint::Tcp(addr)), 4, hub)
}

fn fixture(name: &str) -> Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn sync(metadata: Value) -> Value {
    json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"","metadata":metadata})
}

fn req() -> CreateRequest {
    CreateRequest {
        name: "si-test-vm".into(),
        image: ImageSource::Catalog { alias: "ubuntu/24.04".into() },
        kind: InstanceKind::Vm,
        cpu: None,
        memory_mib: None,
        disk_gib: None,
        profiles: vec![],
        network: None,
        pool: None,
        cloud_init: None,
        start: true,
    }
}

#[test]
fn validate_name_cases() {
    use simple_incus::model::validate_name;
    for ok in ["web1", "si-test-a", "a", &"a".repeat(63)] {
        assert!(validate_name(ok).is_ok(), "{ok}");
    }
    for bad in ["", "1abc", "abc-", "a_b", "123", &"a".repeat(64), "a b", "-a"] {
        assert!(validate_name(bad).is_err(), "{bad}");
    }
}

#[test]
fn create_body_catalog_vm() {
    let mut r = req();
    r.cpu = Some(2);
    r.memory_mib = Some(2048);
    r.disk_gib = Some(20);
    r.cloud_init = Some("#cloud-config\npackages: [htop]\n".into());
    r.profiles = vec!["default".into(), "extra".into()];
    assert_eq!(
        create_body(&r, Some("default")),
        json!({
            "name": "si-test-vm",
            "type": "virtual-machine",
            "source": {"type": "image", "alias": "ubuntu/24.04",
                       "server": "https://images.linuxcontainers.org", "protocol": "simplestreams"},
            "profiles": ["default", "extra"],
            "config": {"limits.cpu": "2", "limits.memory": "2048MiB",
                       "cloud-init.user-data": "#cloud-config\npackages: [htop]\n"},
            "devices": {"root": {"type": "disk", "path": "/", "pool": "default", "size": "20GiB"}},
            "start": true
        })
    );
}

#[test]
fn create_body_minimal_local_container_with_network() {
    let mut r = req();
    r.kind = InstanceKind::Container;
    r.image = ImageSource::Local { fingerprint: "f17d529e".into() };
    r.network = Some("incusbr0".into());
    r.start = false;
    assert_eq!(
        create_body(&r, None),
        json!({
            "name": "si-test-vm",
            "type": "container",
            "source": {"type": "image", "fingerprint": "f17d529e"},
            "profiles": ["default"],
            "config": {},
            "devices": {"eth0": {"type": "nic", "name": "eth0", "network": "incusbr0"}},
            "start": false
        })
    );
}

#[test]
fn pool_override_alone_creates_root_device() {
    let mut r = req();
    r.pool = Some("fast".into());
    let b = create_body(&r, None);
    assert_eq!(b["devices"], json!({"root": {"type": "disk", "path": "/", "pool": "fast"}}));
}

#[tokio::test]
async fn create_returns_without_waiting() {
    // 202 + an operation that is never completed; no wait route exists.
    let (addr, calls) =
        spawn_recording(vec![(Method::POST, "/1.0/instances", 202, fixture("operation_running.json"))]).await;
    let t = Instant::now();
    let id = create_instance_impl(&state(addr), &req()).await.unwrap();
    assert!(t.elapsed() < Duration::from_secs(1));
    assert_eq!(id, "6c1b2f3e-0000-4a5b-8c9d-1234567890ab");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "only the POST; no operation wait");
    assert_eq!(calls[0].1, "/1.0/instances");
}

#[tokio::test]
async fn invalid_name_is_rejected_without_calling_incus() {
    let (addr, calls) = spawn_recording(vec![]).await;
    let mut r = req();
    r.name = "1bad".into();
    let e = create_instance_impl(&state(addr), &r).await.unwrap_err();
    assert!(matches!(e, IncusError::Api { status: 400, .. }));
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn disk_override_without_pool_uses_profile_root_pool() {
    let (addr, calls) = spawn_recording(vec![
        (
            Method::GET,
            "/1.0/profiles/default",
            200,
            sync(json!({"name":"default","devices":{"root":{"type":"disk","path":"/","pool":"tank"}}})),
        ),
        (Method::POST, "/1.0/instances", 202, fixture("operation_running.json")),
    ])
    .await;
    let mut r = req();
    r.disk_gib = Some(10);
    create_instance_impl(&state(addr), &r).await.unwrap();
    let calls = calls.lock().unwrap();
    let post = calls.iter().find(|c| c.0 == Method::POST).unwrap();
    let body: Value = serde_json::from_str(&post.2).unwrap();
    assert_eq!(body["devices"]["root"]["pool"], json!("tank"));
    assert_eq!(body["devices"]["root"]["size"], json!("10GiB"));
}

#[tokio::test]
async fn options_list_managed_networks_only() {
    let (addr, _) = spawn_recording(vec![
        (Method::GET, "/1.0/profiles", 200, sync(json!([{"name":"default"},{"name":"web"}]))),
        (
            Method::GET,
            "/1.0/networks",
            200,
            sync(json!([{"name":"incusbr0","managed":true},{"name":"eth0","managed":false},{"name":"docker0","managed":false}])),
        ),
        (Method::GET, "/1.0/storage-pools", 200, sync(json!([{"name":"default","driver":"dir"}]))),
    ])
    .await;
    let o = create_options_impl(&state(addr)).await.unwrap();
    assert_eq!(o.profiles, ["default", "web"]);
    assert_eq!(o.networks, ["incusbr0"]);
    assert_eq!(o.pools, ["default"]);
}

#[tokio::test]
async fn disk_override_with_no_root_pool_in_any_profile_is_rejected_before_create() {
    let (addr, calls) = spawn_recording(vec![
        (Method::GET, "/1.0/profiles/bare", 200, sync(json!({"name":"bare","devices":{}}))),
        (Method::POST, "/1.0/instances", 202, fixture("operation_running.json")),
    ])
    .await;
    let mut r = req();
    r.profiles = vec!["bare".into()];
    r.disk_gib = Some(10);
    let e = create_instance_impl(&state(addr), &r).await.unwrap_err();
    assert!(matches!(e, IncusError::Api { status: 400, .. }), "{e:?}");
    assert!(calls.lock().unwrap().iter().all(|c| c.0 != Method::POST), "must not reach Incus create");
}

fn op(status: &str, err: &str) -> Value {
    sync(json!({"id":"op1","status":status,"status_code":0,"err":err}))
}

#[tokio::test]
async fn status_of_live_operation_follows_incus() {
    for (status, err, want) in [
        ("Running", "", CreateStatus::Running),
        ("Success", "", CreateStatus::Done),
        ("Failure", "Image not found", CreateStatus::Failed("Image not found".into())),
    ] {
        let addr = common::fake_incus::spawn(vec![(Method::GET, "/1.0/operations/op1", 200, op(status, err))]).await;
        assert_eq!(create_status_impl(&state(addr), "op1", "si-test-c", true).await.unwrap(), want, "{status}");
    }
}

async fn expired_op_status(instance_status: &str, start: bool) -> CreateStatus {
    let addr = common::fake_incus::spawn(vec![(
        Method::GET,
        "/1.0/instances/si-test-c",
        200,
        sync(json!({"name":"si-test-c","status":instance_status})),
    )])
    .await;
    create_status_impl(&state(addr), "op1", "si-test-c", start).await.unwrap()
}

#[tokio::test]
async fn expired_operation_with_running_instance_counts_as_done() {
    assert_eq!(expired_op_status("Running", true).await, CreateStatus::Done);
}

#[tokio::test]
async fn expired_operation_with_stopped_instance_is_done_when_start_was_not_requested() {
    assert_eq!(expired_op_status("Stopped", false).await, CreateStatus::Done);
}

#[tokio::test]
async fn expired_operation_with_stopped_instance_failed_when_start_was_requested() {
    let got = expired_op_status("Stopped", true).await;
    assert!(matches!(got, CreateStatus::Failed(_)), "{got:?}");
}

#[tokio::test]
async fn expired_operation_with_no_instance_counts_as_failed() {
    let addr = common::fake_incus::spawn(vec![]).await;
    let got = create_status_impl(&state(addr), "op1", "si-test-c", true).await.unwrap();
    assert!(matches!(got, CreateStatus::Failed(_)), "{got:?}");
}
