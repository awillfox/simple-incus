use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{header, Method, Request};
use hyper_util::rt::TokioIo;
use serde::{de::DeserializeOwned, Serialize};
use tokio_tungstenite::WebSocketStream;

use std::time::Duration;

use super::raw::{self, Envelope, Operation};
use super::transport::{connect, BoxedIo};
use crate::config::IncusEndpoint;
use crate::model::Action;

#[derive(Debug, thiserror::Error)]
pub enum IncusError {
    #[error("incus unreachable: {0}")]
    Unreachable(String),
    #[error("incus api error {status}: {message}")]
    Api { status: u16, message: String },
    #[error("incus decode error: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response<T> {
    Sync(T),
    Async(Operation),
}

/// Cheap-to-clone client; one short-lived HTTP/1.1 connection per request.
#[derive(Clone)]
pub struct IncusClient {
    endpoint: IncusEndpoint,
    timeout: Duration,
}

/// Default deadline for a whole request (connect excluded; it has its own 10 s).
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

fn expired<T>(r: Result<T, tokio::time::error::Elapsed>) -> Result<T, IncusError> {
    r.map_err(|_| IncusError::Unreachable("request timed out".into()))
}

impl IncusClient {
    pub fn new(endpoint: IncusEndpoint) -> IncusClient {
        IncusClient { endpoint, timeout: DEFAULT_REQUEST_TIMEOUT }
    }

    /// Overrides the per-request deadline (default 30 s).
    pub fn with_timeout(mut self, timeout: Duration) -> IncusClient {
        self.timeout = timeout;
        self
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, IncusError> {
        match self.send::<(), T>(Method::GET, path, None).await? {
            Response::Sync(v) => Ok(v),
            Response::Async(_) => Err(IncusError::Decode("unexpected async response".into())),
        }
    }

    pub async fn send<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Response<T>, IncusError> {
        self.send_with_timeout(method, path, body, self.timeout).await
    }

    async fn send_with_timeout<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        deadline: Duration,
    ) -> Result<Response<T>, IncusError> {
        let payload = match body {
            Some(b) => Bytes::from(serde_json::to_vec(b).map_err(|e| IncusError::Decode(e.to_string()))?),
            None => Bytes::new(),
        };
        let mut req = Request::builder().method(method).uri(path).header(header::HOST, "incus");
        if body.is_some() {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
        let req = req.body(Full::new(payload)).map_err(|e| IncusError::Decode(e.to_string()))?;

        let io = connect(&self.endpoint).await?;
        let exchange = async {
            let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(io))
                .await
                .map_err(|e| IncusError::Unreachable(e.to_string()))?;
            let conn = tokio::spawn(async move {
                let _ = conn.await;
            });
            let resp = sender
                .send_request(req)
                .await
                .map_err(|e| IncusError::Unreachable(e.to_string()));
            let out = match resp {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    resp.into_body()
                        .collect()
                        .await
                        .map(|b| (status, b.to_bytes()))
                        .map_err(|e| IncusError::Unreachable(e.to_string()))
                }
                Err(e) => Err(e),
            };
            conn.abort();
            out
        };
        let (status, bytes) = expired(tokio::time::timeout(deadline, exchange).await)??;

        let env: Envelope = serde_json::from_slice(&bytes).map_err(|e| {
            if status >= 400 {
                IncusError::Api { status, message: String::from_utf8_lossy(&bytes).into_owned() }
            } else {
                IncusError::Decode(e.to_string())
            }
        })?;
        if env.kind == "error" || status >= 400 {
            let code = if env.error_code != 0 { env.error_code } else { status };
            return Err(IncusError::Api { status: code, message: env.error });
        }
        if env.kind == "async" {
            let op = serde_json::from_value(env.metadata).map_err(|e| IncusError::Decode(e.to_string()))?;
            return Ok(Response::Async(op));
        }
        serde_json::from_value(env.metadata)
            .map(Response::Sync)
            .map_err(|e| IncusError::Decode(e.to_string()))
    }

    pub async fn wait(&self, op_id: &str, timeout_s: u32) -> Result<Operation, IncusError> {
        let deadline = Duration::from_secs(u64::from(timeout_s) + 10).max(self.timeout);
        match self
            .send_with_timeout::<(), Operation>(
                Method::GET,
                &format!("/1.0/operations/{op_id}/wait?timeout={timeout_s}"),
                None,
                deadline,
            )
            .await?
        {
            Response::Sync(op) => Ok(op),
            Response::Async(_) => Err(IncusError::Decode("unexpected async response".into())),
        }
    }

    /// Opens a websocket (events, exec/console operation sockets) on a fresh connection.
    pub async fn ws_connect(&self, path: &str) -> Result<WebSocketStream<BoxedIo>, IncusError> {
        let io = connect(&self.endpoint).await?;
        let url = format!("ws://incus{path}");
        let (ws, _) = expired(tokio::time::timeout(self.timeout, tokio_tungstenite::client_async(url, io)).await)?
            .map_err(ws_error)?;
        Ok(ws)
    }

    /// All instances with live state (`recursion=2`).
    pub async fn list_instances(&self) -> Result<Vec<raw::Instance>, IncusError> {
        self.get("/1.0/instances?recursion=2").await
    }

    /// One instance without live state.
    pub async fn get_instance(&self, name: &str) -> Result<raw::Instance, IncusError> {
        self.get(&format!("/1.0/instances/{}", encode_segment(name))).await
    }

    /// Live state of one instance (`/state`).
    pub async fn instance_state(&self, name: &str) -> Result<raw::InstanceState, IncusError> {
        self.get(&format!("/1.0/instances/{}/state", encode_segment(name))).await
    }

    pub async fn list_snapshots(&self, name: &str) -> Result<Vec<raw::InstanceSnapshot>, IncusError> {
        self.get(&format!("/1.0/instances/{}/snapshots?recursion=1", encode_segment(name))).await
    }

    /// Creates a (stateless) snapshot; returns the background operation.
    pub async fn create_snapshot(&self, name: &str, snap: &str) -> Result<Operation, IncusError> {
        let body = serde_json::json!({"name": snap, "stateful": false});
        let path = format!("/1.0/instances/{}/snapshots", encode_segment(name));
        expect_async(self.send::<_, serde_json::Value>(Method::POST, &path, Some(&body)).await?)
    }

    pub async fn delete_snapshot(&self, name: &str, snap: &str) -> Result<Operation, IncusError> {
        let path = format!("/1.0/instances/{}/snapshots/{}", encode_segment(name), encode_segment(snap));
        expect_async(self.send::<(), serde_json::Value>(Method::DELETE, &path, None).await?)
    }

    /// Restores the instance from a snapshot; returns the background operation.
    pub async fn restore_snapshot(&self, name: &str, snap: &str) -> Result<Operation, IncusError> {
        let body = serde_json::json!({"restore": snap});
        let path = format!("/1.0/instances/{}", encode_segment(name));
        expect_async(self.send::<_, serde_json::Value>(Method::PUT, &path, Some(&body)).await?)
    }

    /// Host CPU thread count from `/1.0/resources`.
    pub async fn host_threads(&self) -> Result<u32, IncusError> {
        #[derive(serde::Deserialize)]
        struct Cpu {
            total: u32,
        }
        #[derive(serde::Deserialize)]
        struct Resources {
            cpu: Cpu,
        }
        Ok(self.get::<Resources>("/1.0/resources").await?.cpu.total)
    }

    /// Local images with full details.
    pub async fn list_images(&self) -> Result<Vec<raw::Image>, IncusError> {
        self.get("/1.0/images?recursion=1").await
    }

    /// Deletes a local image; returns the background operation.
    pub async fn delete_image(&self, fingerprint: &str) -> Result<Operation, IncusError> {
        let path = format!("/1.0/images/{}", encode_segment(fingerprint));
        expect_async(self.send::<(), serde_json::Value>(Method::DELETE, &path, None).await?)
    }

    pub async fn resources(&self) -> Result<raw::Resources, IncusError> {
        self.get("/1.0/resources").await
    }

    pub async fn storage_pools(&self) -> Result<Vec<raw::StoragePool>, IncusError> {
        self.get("/1.0/storage-pools?recursion=1").await
    }

    pub async fn pool_resources(&self, pool: &str) -> Result<raw::PoolResources, IncusError> {
        self.get(&format!("/1.0/storage-pools/{}/resources", encode_segment(pool))).await
    }

    /// Incus server version from `GET /1.0` (`environment.server_version`).
    pub async fn server_version(&self) -> Result<String, IncusError> {
        let v: serde_json::Value = self.get("/1.0").await?;
        Ok(v["environment"]["server_version"].as_str().unwrap_or_default().to_string())
    }

    /// Requests a lifecycle change; returns the background operation.
    pub async fn instance_state_put(&self, name: &str, action: &Action) -> Result<Operation, IncusError> {
        let (verb, force) = match *action {
            Action::Start => ("start", false),
            Action::Stop { force } => ("stop", force),
            Action::Restart { force } => ("restart", force),
            Action::Freeze => ("freeze", false),
            Action::Unfreeze => ("unfreeze", false),
        };
        let body = serde_json::json!({"action": verb, "force": force, "timeout": -1});
        let path = format!("/1.0/instances/{}/state", encode_segment(name));
        expect_async(self.send::<_, serde_json::Value>(Method::PUT, &path, Some(&body)).await?)
    }

    /// Deletes a (stopped) instance; returns the background operation.
    pub async fn delete_instance(&self, name: &str) -> Result<Operation, IncusError> {
        let path = format!("/1.0/instances/{}", encode_segment(name));
        expect_async(self.send::<(), serde_json::Value>(Method::DELETE, &path, None).await?)
    }

    pub async fn ping(&self) -> bool {
        self.get::<serde_json::Value>("/1.0").await.is_ok()
    }
}

/// A rejected upgrade (HTTP response) is an API error; anything else is transport.
fn ws_error(e: tokio_tungstenite::tungstenite::Error) -> IncusError {
    use tokio_tungstenite::tungstenite::Error;
    match e {
        Error::Http(resp) => {
            let status = resp.status();
            let message = resp
                .body()
                .as_deref()
                .and_then(|b| serde_json::from_slice::<Envelope>(b).ok())
                .map(|env| env.error)
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| status.canonical_reason().unwrap_or("websocket upgrade rejected").to_string());
            IncusError::Api { status: status.as_u16(), message }
        }
        other => IncusError::Unreachable(other.to_string()),
    }
}

fn expect_async<T>(r: Response<T>) -> Result<Operation, IncusError> {
    match r {
        Response::Async(op) => Ok(op),
        Response::Sync(_) => Err(IncusError::Decode("expected an async operation".into())),
    }
}

/// Percent-encodes a value for use as one URL path segment.
fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
