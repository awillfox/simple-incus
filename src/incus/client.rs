use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{header, Method, Request};
use hyper_util::rt::TokioIo;
use serde::{de::DeserializeOwned, Serialize};
use tokio_tungstenite::WebSocketStream;

use super::raw::{Envelope, Operation};
use super::transport::{connect, BoxedIo};
use crate::config::IncusEndpoint;

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
}

impl IncusClient {
    pub fn new(endpoint: IncusEndpoint) -> IncusClient {
        IncusClient { endpoint }
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
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(io))
            .await
            .map_err(|e| IncusError::Unreachable(e.to_string()))?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| IncusError::Unreachable(e.to_string()))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| IncusError::Unreachable(e.to_string()))?
            .to_bytes();

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
        self.get(&format!("/1.0/operations/{op_id}/wait?timeout={timeout_s}")).await
    }

    /// Opens a websocket (events, exec/console operation sockets) on a fresh connection.
    pub async fn ws_connect(&self, path: &str) -> Result<WebSocketStream<BoxedIo>, IncusError> {
        let io = connect(&self.endpoint).await?;
        let url = format!("ws://incus{path}");
        let (ws, _) = tokio_tungstenite::client_async(url, io)
            .await
            .map_err(|e| IncusError::Unreachable(e.to_string()))?;
        Ok(ws)
    }

    pub async fn ping(&self) -> bool {
        self.get::<serde_json::Value>("/1.0").await.is_ok()
    }
}
