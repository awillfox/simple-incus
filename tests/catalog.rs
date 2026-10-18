//! Simplestreams catalog: parsing the real (trimmed) index and cache behaviour.
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use simple_incus::incus::catalog::{parse, Catalog};
use simple_incus::model::CatalogImage;
use tokio::net::TcpListener;

fn index() -> Value {
    let p = format!("{}/tests/fixtures/simplestreams_images.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn find<'a>(all: &'a [CatalogImage], alias: &str) -> &'a CatalogImage {
    all.iter().find(|i| i.alias == alias).unwrap_or_else(|| panic!("{alias} missing in {all:?}"))
}

#[test]
fn catalog_parse_filters_arch() {
    let json = index();
    let arm64_products = json["products"]
        .as_object()
        .unwrap()
        .values()
        .filter(|p| p["arch"] == "arm64")
        .count();
    assert!(arm64_products > 0, "fixture must contain arm64 products");
    let amd64 = parse(&json, "amd64");
    let mut aliases: Vec<&str> = amd64.iter().map(|i| i.alias.as_str()).collect();
    aliases.sort();
    assert_eq!(
        aliases,
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
    // Only one entry per product: ubuntu/24.04 exists for amd64 and arm64.
    assert_eq!(amd64.iter().filter(|i| i.alias == "ubuntu/24.04").count(), 1);
    // The other architecture is parsed on its own.
    let arm64 = parse(&json, "arm64");
    assert_eq!(arm64.len(), arm64_products);
    assert_eq!(arm64[0].alias, "ubuntu/24.04");
}

#[test]
fn catalog_flags_vm_and_container() {
    let all = parse(&index(), "amd64");
    let u = find(&all, "ubuntu/24.04");
    assert!(u.container && u.vm);
    assert_eq!((u.distro.as_str(), u.release.as_str(), u.variant.as_str()), ("ubuntu", "24.04", "default"));
    assert_eq!(u.description, "Ubuntu 24.04 (noble)");
    // Container only.
    let b = find(&all, "busybox/1.38.0");
    assert!(b.container && !b.vm, "{b:?}");
    // VM only (every product ships incus.tar.xz metadata, so it must not imply a container).
    for alias in ["ubuntu/24.04/desktop", "freebsd/15.0"] {
        let v = find(&all, alias);
        assert!(v.vm && !v.container, "{v:?}");
    }
    let c = find(&all, "ubuntu/24.04/cloud");
    assert_eq!(c.variant, "cloud");
    assert_eq!(c.description, "Ubuntu 24.04 (noble) cloud");
}

#[test]
fn catalog_uses_latest_version() {
    let item = |ftype: &str| json!({"ftype": ftype});
    let json = json!({"products": {"x:1:amd64:default": {
        "arch": "amd64", "os": "X", "release": "1", "release_title": "1", "variant": "default",
        "aliases": "x/1/default,x/1",
        "versions": {
            "20260101_00:00": {"items": {"a": item("squashfs"), "b": item("disk-kvm.img")}},
            "20260201_00:00": {"items": {"a": item("squashfs")}},
        }
    }}});
    let all = parse(&json, "amd64");
    assert_eq!(all.len(), 1);
    assert!(all[0].container && !all[0].vm, "{:?}", all[0]);
}

/// Serves `body` with 200 until `fail` is set, then 500; counts every request.
async fn spawn_index(body: String, fail: Arc<AtomicBool>, hits: Arc<AtomicUsize>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let (body, fail, hits) = (body.clone(), fail.clone(), hits.clone());
            tokio::spawn(async move {
                let svc = service_fn(move |_: Request<Incoming>| {
                    let (body, fail, hits) = (body.clone(), fail.clone(), hits.clone());
                    async move {
                        hits.fetch_add(1, Ordering::SeqCst);
                        let (status, body) =
                            if fail.load(Ordering::SeqCst) { (500, String::new()) } else { (200, body) };
                        Ok::<_, Infallible>(Response::builder().status(status).body(Full::new(Bytes::from(body))).unwrap())
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(sock), svc).await;
            });
        }
    });
    addr
}

#[tokio::test]
async fn catalog_serves_stale_on_failure() {
    let fail = Arc::new(AtomicBool::new(false));
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_index(index().to_string(), fail.clone(), hits.clone()).await;
    let catalog = Catalog::with_url(format!("http://{addr}/streams/v1/images.json"), Duration::from_millis(50));
    let first = catalog.get().await.unwrap();
    assert!(first.iter().any(|i| i.alias == "ubuntu/24.04"));
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    fail.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(120)).await; // TTL expired: a refresh is attempted and fails
    let stale = catalog.get().await.unwrap();
    assert_eq!(*stale, *first);
    // The refresh really was attempted (and failed), not skipped.
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn catalog_fails_without_a_good_copy() {
    let fail = Arc::new(AtomicBool::new(true));
    let addr = spawn_index(String::new(), fail, Arc::default()).await;
    let catalog = Catalog::with_url(format!("http://{addr}/images.json"), Duration::from_secs(3600));
    let err = catalog.get().await.unwrap_err();
    assert!(err.starts_with("Catalog unavailable: "), "{err}");
}
