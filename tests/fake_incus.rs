//! Fake Incus server for tests, plus the client core tests.
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response as HyperResponse};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use simple_incus::config::IncusEndpoint;
use simple_incus::incus::{IncusClient, IncusError, Response};
use tokio::net::TcpListener;

type Route = (Method, String, u16, Value);

/// Spawns a hyper server on 127.0.0.1:0. Each route is (method, path-without-query, status, JSON body).
pub async fn spawn(routes: Vec<(Method, &str, u16, Value)>) -> SocketAddr {
    let routes: Arc<Vec<Route>> =
        Arc::new(routes.into_iter().map(|(m, p, s, v)| (m, p.to_string(), s, v)).collect());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else { return };
            let routes = routes.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<Incoming>| {
                    let routes = routes.clone();
                    async move {
                        let hit = routes
                            .iter()
                            .find(|(m, p, _, _)| m == req.method() && p == req.uri().path());
                        let (status, body) = match hit {
                            Some((_, _, s, v)) => (*s, v.to_string()),
                            None => (404, r#"{"type":"error","error":"not found","error_code":404}"#.into()),
                        };
                        Ok::<_, Infallible>(
                            HyperResponse::builder()
                                .status(status)
                                .header("content-type", "application/json")
                                .body(Full::new(Bytes::from(body)))
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(sock), svc)
                    .await;
            });
        }
    });
    addr
}

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
