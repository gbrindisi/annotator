//! Browser WebSocket: push Docs, accept intents.

use super::state::{App, ReplaceError, Sess};
use crate::format::{self, NewCommentInput};
use crate::mapping::{MappingError, map_selection};
use crate::protocol::{ClientMsg, ServerMsg};
use crate::utf16;
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
        intent => match sess.apply_edit(|raw| apply_intent(raw, intent, &sess.home)) {
            Ok(()) => vec![],
            Err(ReplaceError::Conflict) => vec![
                error("file changed on disk; reloaded", true),
                sess.doc_msg(),
            ],
            Err(ReplaceError::Other(e)) => vec![error(e, false)],
        },
    }
}

/// Compute the new raw text for a comment intent.
fn apply_intent(raw: &str, msg: ClientMsg, home: &std::path::Path) -> Result<String, String> {
    let author = crate::config::author(home);
    let now = crate::time::now_iso();
    let input = |text: String| NewCommentInput {
        id: None,
        created_at: now.clone(),
        author: author.clone(),
        text,
        expected: None,
    };
    let e = |e: format::EditError| e.to_string();
    match msg {
        ClientMsg::AddComment { selection, body } => match map_selection(raw, &selection) {
            Ok(r) => format::add_comment(raw, r.start, r.end, &input(body))
                .map(|(doc, _)| doc)
                .map_err(e),
            Err(MappingError::InCodeBlock {
                block,
                from_line,
                to_line,
            }) => format::add_code_comment(raw, block, from_line, to_line, &input(body))
                .map(|(doc, _)| doc)
                .map_err(e),
            Err(MappingError::CrossBlock) => {
                Err("The selection spans several blocks; select text within a single block.".into())
            }
            Err(MappingError::NotFound) => Err(
                "Could not find the selected text in the source; it may have changed. Try again."
                    .into(),
            ),
            Err(MappingError::BadSourcepos(_)) => Err(
                "The selection refers to a block that no longer exists; the view may be stale."
                    .into(),
            ),
        },
        ClientMsg::AddCommentRaw { from, to, body } => {
            let (from, to) = (utf16::to_byte(raw, from), utf16::to_byte(raw, to));
            let r = format::normalize_selection(raw, from, to);
            if let Some(m) = format::validate_selection(raw, r.start, r.end) {
                return Err(m.to_string());
            }
            format::add_comment(raw, r.start, r.end, &input(body))
                .map(|(doc, _)| doc)
                .map_err(e)
        }
        ClientMsg::Reply { id, body } => {
            if body.trim().is_empty() {
                return Err("Write a reply first.".into());
            }
            format::append_reply(raw, &id, &now, &author, &body).map_err(e)
        }
        ClientMsg::EditEntry { id, entry, body } => {
            if body.trim().is_empty() {
                return Err("An entry cannot be empty; delete it instead.".into());
            }
            format::edit_entry(raw, &id, entry, &body).map_err(e)
        }
        ClientMsg::DeleteEntry { id, entry } => format::delete_entry(raw, &id, entry).map_err(e),
        ClientMsg::Resolve { id } => format::set_resolved(raw, &id, true).map_err(e),
        ClientMsg::Reopen { id } => format::set_resolved(raw, &id, false).map_err(e),
        ClientMsg::DeleteThread { id } => format::delete_comment(raw, &id).map_err(e),
        ClientMsg::ReplaceSource { .. } | ClientMsg::Done | ClientMsg::Abandon => {
            Err("unexpected message".into())
        }
    }
}
