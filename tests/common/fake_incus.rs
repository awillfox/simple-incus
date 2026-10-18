//! Fake Incus servers on 127.0.0.1:0 (hyper; runs on Windows).
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response as HyperResponse};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use tokio::net::TcpListener;

type Route = (Method, String, u16, Value);

/// HTTP server. Each route is (method, path-without-query, status, JSON body); unmatched -> 404 error envelope.
pub async fn spawn(routes: Vec<(Method, &str, u16, Value)>) -> SocketAddr {
    spawn_recording(routes).await.0
}

/// A request seen by the fake: (method, path-without-query, body text).
pub type Call = (Method, String, String);

/// Like [`spawn`], also returning every request in arrival order.
pub async fn spawn_recording(
    routes: Vec<(Method, &str, u16, Value)>,
) -> (SocketAddr, Arc<Mutex<Vec<Call>>>) {
    let calls: Arc<Mutex<Vec<Call>>> = Arc::default();
    let seen = calls.clone();
    let routes: Arc<Vec<Route>> =
        Arc::new(routes.into_iter().map(|(m, p, s, v)| (m, p.to_string(), s, v)).collect());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else { return };
            let routes = routes.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                let svc = service_fn(move |req: Request<Incoming>| {
                    let routes = routes.clone();
                    let seen = seen.clone();
                    async move {
                        let (parts, body) = req.into_parts();
                        let body = body.collect().await.map(|b| b.to_bytes()).unwrap_or_default();
                        seen.lock().unwrap().push((
                            parts.method.clone(),
                            parts.uri.path().to_string(),
                            String::from_utf8_lossy(&body).into_owned(),
                        ));
                        let hit = routes
                            .iter()
                            .find(|(m, p, _, _)| *m == parts.method && p == parts.uri.path());
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
    (addr, calls)
}

/// Accepts connections and never answers (a stalled incusd).
pub async fn spawn_silent() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((sock, _)) = listener.accept().await {
            held.push(sock);
        }
    });
    addr
}

/// Websocket server that upgrades any request and echoes every message back.
pub async fn spawn_ws_echo() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_async(sock).await else { return };
                while let Some(Ok(msg)) = ws.next().await {
                    if msg.is_close() || ws.send(msg).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

/// Event websocket server. Connection N (0-based) waits `delay`, sends
/// `scripts[N]` as text frames, then closes (except the last script, which stays open).
/// Connections past the script stay open and silent.
pub async fn spawn_ws_script(scripts: Vec<Vec<Value>>, delay: std::time::Duration) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut n = 0usize;
        while let Ok((sock, _)) = listener.accept().await {
            let frames = scripts.get(n).cloned();
            let last = n + 1 >= scripts.len();
            n += 1;
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_async(sock).await else { return };
                tokio::time::sleep(delay).await;
                for f in frames.iter().flatten() {
                    let _ = ws.send(tokio_tungstenite::tungstenite::Message::text(f.to_string())).await;
                }
                if frames.is_some() && !last {
                    let _ = ws.close(None).await;
                } else {
                    while let Some(Ok(m)) = ws.next().await {
                        if m.is_close() {
                            break;
                        }
                    }
                }
            });
        }
    });
    addr
}
