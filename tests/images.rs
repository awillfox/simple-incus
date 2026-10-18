//! Local images, catalog search and host info against the fake Incus server.
mod common;

use std::time::Duration;

use common::fake_incus::spawn_recording;
use hyper::Method;
use serde_json::{json, Value};
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::catalog::Catalog;
use simple_incus::incus::{EventHub, IncusClient, IncusError};
use simple_incus::model::InstanceKind;
use simple_incus::server::host::host_info_impl;
use simple_incus::server::images::{catalog_impl, delete_image_impl, list_images_impl};
use simple_incus::state::AppState;

fn state(addr: std::net::SocketAddr) -> AppState {
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let hub = EventHub::start(IncusClient::new(IncusEndpoint::Tcp(dead)));
    AppState::with_hub(IncusClient::new(IncusEndpoint::Tcp(addr)), 4, hub)
}

fn sync(metadata: Value) -> Value {
    json!({"type":"sync","status":"Success","status_code":200,"error_code":0,"error":"","metadata":metadata})
}

const FP: &str = "f17d529e9e668c848daf4216142a60739a4ec2e842469577272e867f043fb7cc";
const OP: &str = "/1.0/operations/6c1b2f3e-0000-4a5b-8c9d-1234567890ab/wait";

fn images() -> Value {
    sync(json!([
        {"fingerprint": FP, "aliases": [{"name":"si-test-img","description":""}], "type": "container", "size": 3367796,
         "uploaded_at": "2026-10-18T13:13:29.520201007Z",
         "properties": {"description": "Alpine 3.21 amd64 (20261016_13:00)", "os": "Alpine"}},
        {"fingerprint": "aa11", "aliases": [], "type": "virtual-machine", "size": -1,
         "uploaded_at": "2026-10-19T00:00:00Z", "properties": {"os": "Ubuntu", "release": "24.04"}},
    ]))
}

#[tokio::test]
async fn list_images_maps_fields_newest_first() {
    let (addr, _) = spawn_recording(vec![(Method::GET, "/1.0/images", 200, images())]).await;
    let list = list_images_impl(&state(addr)).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].fingerprint, "aa11");
    assert_eq!(list[0].kind, InstanceKind::Vm);
    assert_eq!(list[0].description, "Ubuntu 24.04");
    assert_eq!(list[0].size, 0);
    assert_eq!(list[1].aliases, ["si-test-img"]);
    assert_eq!(list[1].description, "Alpine 3.21 amd64 (20261016_13:00)");
    assert_eq!(list[1].kind, InstanceKind::Container);
}

fn wait_op(code: i32, err: &str) -> Value {
    sync(json!({"id":"6c1b2f3e-0000-4a5b-8c9d-1234567890ab","status":"x","status_code":code,"err":err}))
}

fn async_op() -> Value {
    json!({"type":"async","status":"Operation created","status_code":100,"error_code":0,"error":"",
           "operation":"/1.0/operations/6c1b2f3e-0000-4a5b-8c9d-1234567890ab",
           "metadata":{"id":"6c1b2f3e-0000-4a5b-8c9d-1234567890ab","status":"Running","status_code":103}})
}

#[tokio::test]
async fn delete_image_waits_for_the_operation() {
    let (addr, calls) = spawn_recording(vec![
        (Method::DELETE, &format!("/1.0/images/{FP}"), 200, async_op()),
        (Method::GET, OP, 200, wait_op(200, "")),
    ])
    .await;
    delete_image_impl(&state(addr), FP).await.unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].0, Method::DELETE);
    assert_eq!(calls[1].1, OP);
}

#[tokio::test]
async fn delete_image_in_use_passes_the_message_through() {
    let (addr, _) = spawn_recording(vec![
        (Method::DELETE, &format!("/1.0/images/{FP}"), 200, async_op()),
        (Method::GET, OP, 200, wait_op(400, "Image is in use by instance \"web1\"")),
    ])
    .await;
    match delete_image_impl(&state(addr), FP).await.unwrap_err() {
        IncusError::Api { message, .. } => assert!(message.contains("in use"), "{message}"),
        other => panic!("{other:?}"),
    }
}

async fn catalog_state() -> AppState {
    use common::fake_incus::spawn;
    // The catalog index is served by the generic fake (path-routed JSON).
    let body: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{}/tests/fixtures/simplestreams_images.json", env!("CARGO_MANIFEST_DIR"))).unwrap(),
    )
    .unwrap();
    let addr = spawn(vec![(Method::GET, "/streams/v1/images.json", 200, body)]).await;
    state(addr).with_catalog(Catalog::with_url(format!("http://{addr}/streams/v1/images.json"), Duration::from_secs(60)))
}

fn aliases(v: &[simple_incus::model::CatalogImage]) -> Vec<&str> {
    v.iter().map(|i| i.alias.as_str()).collect()
}

#[tokio::test]
async fn catalog_search_matches_case_insensitively_and_sorts() {
    let st = catalog_state().await;
    let all = catalog_impl(&st, "").await.unwrap();
    // distro ascending, then release descending
    assert_eq!(
        aliases(&all),
        [
            "alpine/3.21",
            "archlinux/current",
            "busybox/1.38.0",
            "debian/12",
            "freebsd/15.0",
            "nixos/26.05",
            "ubuntu/24.04",
            "ubuntu/24.04/cloud",
            "ubuntu/24.04/desktop",
        ]
    );
    assert_eq!(aliases(&catalog_impl(&st, "  NOBLE ").await.unwrap()).len(), 3); // description has the codename
    assert_eq!(aliases(&catalog_impl(&st, "Alpine").await.unwrap()), ["alpine/3.21"]);
    assert_eq!(aliases(&catalog_impl(&st, "24.04/cl").await.unwrap()), ["ubuntu/24.04/cloud"]);
    assert!(catalog_impl(&st, "no-such-distro").await.unwrap().is_empty());
}

#[tokio::test]
async fn host_info_combines_resources_pools_and_version() {
    let (addr, _) = spawn_recording(vec![
        (Method::GET, "/1.0", 200, sync(json!({"environment": {"server_version": "7.5.1"}}))),
        (
            Method::GET,
            "/1.0/resources",
            200,
            sync(json!({"cpu": {"total": 6, "sockets": [{"name": "Intel(R) Core(TM) i5-9500T"}]},
                        "memory": {"total": 33822867456u64, "used": 10879041536u64}})),
        ),
        (
            Method::GET,
            "/1.0/storage-pools",
            200,
            sync(json!([{"name": "default", "driver": "dir"}, {"name": "broken", "driver": "zfs"}])),
        ),
        (
            Method::GET,
            "/1.0/storage-pools/default/resources",
            200,
            sync(json!({"space": {"total": 501809635328u64, "used": 30638829568u64}})),
        ),
    ])
    .await;
    let h = host_info_impl(&state(addr)).await.unwrap();
    assert_eq!(h.incus_version, "7.5.1");
    assert_eq!((h.cpu_model.as_str(), h.cpu_threads), ("Intel(R) Core(TM) i5-9500T", 6));
    assert_eq!((h.mem_total, h.mem_used), (33822867456, 10879041536));
    assert_eq!(h.pools.len(), 2);
    assert_eq!((h.pools[0].name.as_str(), h.pools[0].used, h.pools[0].total), ("default", 30638829568, 501809635328));
    // A pool whose resources call fails is still listed, with unknown (zero) usage.
    assert_eq!((h.pools[1].name.as_str(), h.pools[1].driver.as_str(), h.pools[1].total), ("broken", "zfs", 0));
}
