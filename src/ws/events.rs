//! `GET /ws/events`: relays hub events and Incus connectivity to a browser.
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::sync::broadcast::error::RecvError;

use crate::incus::EventHub;
use crate::model::WsMsg;

pub fn events_ws(ws: WebSocketUpgrade, hub: EventHub) -> Response {
    ws.on_upgrade(move |socket| relay(socket, hub))
}

async fn send(socket: &mut WebSocket, msg: &WsMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(text) => socket.send(Message::Text(text.into())).await.is_ok(),
        Err(_) => true,
    }
}

async fn relay(mut socket: WebSocket, hub: EventHub) {
    let mut events = hub.subscribe();
    let mut connected = hub.connected();
    let initial = *connected.borrow_and_update();
    if !send(&mut socket, &WsMsg::status(initial)).await {
        return;
    }
    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Ok(ev) => if !send(&mut socket, &WsMsg::Event(ev)).await { return },
                // A slow browser misses events; keep the socket.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            },
            changed = connected.changed() => {
                if changed.is_err() { return }
                let now = *connected.borrow_and_update();
                if !send(&mut socket, &WsMsg::status(now)).await { return }
            }
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}
