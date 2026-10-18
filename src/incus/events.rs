//! Upstream Incus event stream, fanned out to browsers with reconnect/backoff.
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::tungstenite::Message;

use super::IncusClient;
use crate::model::{UiEvent, UiEventKind};

const EVENTS_PATH: &str = "/1.0/events?type=lifecycle,operation";
const CAPACITY: usize = 256;
/// Production backoff starts at 1 s, doubles, and is capped at 30x the base.
pub const BASE_DELAY: Duration = Duration::from_secs(1);
const MAX_FACTOR: u32 = 30;

#[derive(Clone)]
pub struct EventHub {
    tx: broadcast::Sender<UiEvent>,
    connected: watch::Sender<bool>,
}

impl EventHub {
    /// Spawns the upstream task (needs a tokio runtime). Backoff 1,2,4,8,16,30,30... s.
    pub fn start(client: IncusClient) -> EventHub {
        EventHub::start_with_backoff(client, BASE_DELAY)
    }

    /// Like [`start`](Self::start) with an injectable base delay (cap is 30x base).
    pub fn start_with_backoff(client: IncusClient, base: Duration) -> EventHub {
        let (tx, _) = broadcast::channel(CAPACITY);
        let (connected, _) = watch::channel(false);
        let hub = EventHub { tx, connected };
        tokio::spawn(run(client, hub.clone(), base));
        hub
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UiEvent> {
        self.tx.subscribe()
    }

    pub fn connected(&self) -> watch::Receiver<bool> {
        self.connected.subscribe()
    }
}

async fn run(client: IncusClient, hub: EventHub, base: Duration) {
    let mut delay = base;
    loop {
        if let Ok(mut ws) = client.ws_connect(EVENTS_PATH).await {
            delay = base;
            hub.connected.send_replace(true);
            while let Some(Ok(msg)) = ws.next().await {
                match msg {
                    Message::Text(text) => {
                        if let Some(ev) =
                            serde_json::from_str::<Value>(text.as_str()).ok().and_then(|v| map_event(&v))
                        {
                            // No receivers is fine.
                            let _ = hub.tx.send(ev);
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
        hub.connected.send_replace(false);
        tokio::time::sleep(delay).await;
        delay = next_delay(delay, base);
    }
}

/// Doubles `delay`, capped at 30x `base` (1,2,4,8,16,30,30... s for a 1 s base).
pub fn next_delay(delay: Duration, base: Duration) -> Duration {
    (delay * 2).min(base * MAX_FACTOR)
}

/// Maps one Incus event frame; other event types yield `None`.
pub fn map_event(v: &Value) -> Option<UiEvent> {
    let meta = &v["metadata"];
    match v["type"].as_str()? {
        "lifecycle" => Some(UiEvent {
            kind: UiEventKind::Lifecycle,
            instance: meta["source"].as_str().and_then(instance_from_path),
            op_id: None,
            progress: None,
            message: meta["action"].as_str().unwrap_or_default().to_string(),
        }),
        "operation" => {
            let mut message = meta["description"].as_str().unwrap_or_default().to_string();
            if let Some(err) = meta["err"].as_str().filter(|e| !e.is_empty()) {
                message.push_str(" failed: ");
                message.push_str(err);
            }
            let progress = meta["metadata"].as_object().and_then(|m| {
                m.iter()
                    .filter(|(k, _)| k.ends_with("_progress"))
                    .find_map(|(_, v)| v.as_str().map(str::to_string))
            });
            Some(UiEvent {
                kind: UiEventKind::Operation,
                instance: meta["resources"]["instances"]
                    .as_array()
                    .and_then(|a| a.iter().find_map(|p| p.as_str().and_then(instance_from_path))),
                op_id: meta["id"].as_str().map(str::to_string),
                progress,
                message,
            })
        }
        _ => None,
    }
}

/// `/1.0/instances/<name>[/...]` -> decoded name.
fn instance_from_path(path: &str) -> Option<String> {
    let rest = path.split('?').next()?.strip_prefix("/1.0/instances/")?;
    let seg = rest.split('/').next().filter(|s| !s.is_empty())?;
    Some(percent_decode(seg))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = if b[i] == b'%' { s.get(i + 1..i + 3) } else { None };
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(v) => {
                out.push(v);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
