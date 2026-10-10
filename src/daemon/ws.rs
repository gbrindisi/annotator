//! Browser WebSocket: push Docs, accept intents.

use super::state::{App, ReplaceError, Sess};
use crate::protocol::{ClientMsg, ServerMsg};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

pub async fn handler(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(sess) = app.get_session(&id) else {
        return (StatusCode::NOT_FOUND, "no such session").into_response();
    };
    ws.on_upgrade(move |socket| client(app, sess, socket))
}

fn encode(m: &ServerMsg) -> Message {
    Message::Text(serde_json::to_string(m).unwrap_or_default().into())
}

fn error(message: impl Into<String>, reloaded: bool) -> ServerMsg {
    ServerMsg::Error {
        message: message.into(),
        reloaded,
    }
}

async fn client(app: Arc<App>, sess: Arc<Sess>, socket: WebSocket) {
    sess.add_client(1);
    app.touch();
    let mut rx = sess.tx.subscribe();
    let (mut sink, mut stream) = socket.split();
    let mut ok = sink.send(encode(&sess.doc_msg())).await.is_ok();
    while ok {
        tokio::select! {
            m = stream.next() => {
                let text = match m {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => continue,
                };
                for reply in handle(&app, &sess, &text) {
                    if sink.send(encode(&reply)).await.is_err() {
                        ok = false;
                        break;
                    }
                }
            }
            b = rx.recv() => match b {
                Ok(msg) => ok = sink.send(encode(&msg)).await.is_ok(),
                Err(RecvError::Lagged(_)) => ok = sink.send(encode(&sess.doc_msg())).await.is_ok(),
                Err(RecvError::Closed) => break,
            }
        }
    }
    sess.add_client(-1);
    app.touch();
}

/// Handle one client message; returns direct replies for this client only.
fn handle(app: &App, sess: &Sess, text: &str) -> Vec<ServerMsg> {
    let msg: ClientMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => return vec![error(format!("bad message: {e}"), false)],
    };
    app.touch();
    match msg {
        ClientMsg::ReplaceSource { text, base_hash } => {
            match sess.replace_source(&text, &base_hash) {
                Ok(()) => vec![],
                Err(ReplaceError::Conflict) => vec![
                    error("file changed on disk; reloaded", true),
                    sess.doc_msg(),
                ],
                Err(ReplaceError::Other(e)) => vec![error(e, false)],
            }
        }
        ClientMsg::Done | ClientMsg::Abandon => {
            let done = matches!(msg, ClientMsg::Done);
            match sess.finish(&app.home, done) {
                Ok(()) => vec![],
                Err(e) => vec![error(e, false)],
            }
        }
        _ => vec![error("not implemented", false)],
    }
}
