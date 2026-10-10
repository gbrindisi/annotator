//! In-memory daemon state: session registry, rounds, broadcast channels.

use super::watch::FileWatcher;
use crate::protocol::{DocState, ServerMsg, SessionStatus, StatusResponse, WaitResult};
use crate::session::{self, Meta, RoundState};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch};

pub struct App {
    pub home: PathBuf,
    pub port: u16,
    pub token: String,
    pub started: Instant,
    sessions: Mutex<HashMap<String, Arc<Sess>>>,
    shutdown: watch::Sender<bool>,
    last_touch: Mutex<Instant>,
    watcher: FileWatcher,
}

impl App {
    pub fn new(home: PathBuf, port: u16, token: String) -> anyhow::Result<Arc<App>> {
        let (watcher, mut changes) = FileWatcher::new()?;
        let app = Arc::new(App {
            home,
            port,
            token,
            started: Instant::now(),
            sessions: Mutex::new(HashMap::new()),
            shutdown: watch::channel(false).0,
            last_touch: Mutex::new(Instant::now()),
            watcher,
        });
        let a = app.clone();
        tokio::spawn(async move {
            while let Some(path) = changes.recv().await {
                let sess = a
                    .sessions
                    .lock()
                    .unwrap()
                    .values()
                    .find(|s| s.path == path)
                    .cloned();
                if let Some(s) = sess {
                    s.on_file_changed();
                }
            }
        });
        Ok(app)
    }

    pub fn trigger_shutdown(&self) {
        self.shutdown.send_replace(true);
    }

    pub async fn shutdown_wait(&self) {
        let mut rx = self.shutdown.subscribe();
        let _ = rx.wait_for(|v| *v).await;
    }

    pub fn touch(&self) {
        *self.last_touch.lock().unwrap() = Instant::now();
    }

    /// Clients or waiters present, or recent API activity.
    pub fn busy(&self) -> bool {
        if self.last_touch.lock().unwrap().elapsed() < Duration::from_secs(1) {
            return true;
        }
        self.sessions.lock().unwrap().values().any(|s| {
            let i = s.inner.lock().unwrap();
            i.clients > 0 || i.waiters > 0
        })
    }

    fn insert(&self, sess: Sess) -> Arc<Sess> {
        let mut map = self.sessions.lock().unwrap();
        if let Some(s) = map.get(&sess.id) {
            return s.clone();
        }
        if let Err(e) = self.watcher.watch(&sess.path) {
            tracing::warn!("cannot watch {}: {e}", sess.path.display());
        }
        let s = Arc::new(sess);
        map.insert(s.id.clone(), s.clone());
        s
    }

    pub fn session_for_path(&self, canonical: &Path) -> anyhow::Result<Arc<Sess>> {
        let id = session::session_id(canonical);
        if let Some(s) = self.sessions.lock().unwrap().get(&id) {
            return Ok(s.clone());
        }
        let meta = session::load_meta(&self.home, &id)
            .filter(|m| Path::new(&m.path) == canonical)
            .unwrap_or_else(|| Meta {
                path: canonical.to_string_lossy().into_owned(),
                round: 0,
                round_state: RoundState::Done,
                last_result: None,
            });
        Ok(self.insert(Sess::new(
            self.home.clone(),
            id,
            canonical.to_path_buf(),
            meta,
        )))
    }

    /// In-memory session, or one persisted on disk by an earlier daemon run.
    pub fn get_session(&self, id: &str) -> Option<Arc<Sess>> {
        if !session::valid_id(id) {
            return None;
        }
        if let Some(s) = self.sessions.lock().unwrap().get(id) {
            return Some(s.clone());
        }
        let meta = session::load_meta(&self.home, id)?;
        if meta.round == 0 {
            return None;
        }
        let path = PathBuf::from(&meta.path);
        Some(self.insert(Sess::new(self.home.clone(), id.to_string(), path, meta)))
    }

    pub fn status(&self) -> StatusResponse {
        let mut sessions: Vec<SessionStatus> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| {
                let i = s.inner.lock().unwrap();
                SessionStatus {
                    id: s.id.clone(),
                    path: i.meta.path.clone(),
                    round: i.meta.round,
                    state: i.meta.round_state.as_str().into(),
                    clients: i.clients,
                    waiters: i.waiters,
                }
            })
            .collect();
        sessions.sort_by(|a, b| a.path.cmp(&b.path));
        StatusResponse {
            pid: std::process::id(),
            port: self.port,
            uptime_secs: self.started.elapsed().as_secs(),
            sessions,
        }
    }
}

pub struct Inner {
    pub meta: Meta,
    pub clients: usize,
    pub waiters: usize,
    /// Hash of the last Doc broadcast, to suppress duplicate pushes.
    last_hash: Option<String>,
}

pub struct Sess {
    pub home: PathBuf,
    pub id: String,
    pub path: PathBuf,
    pub inner: Mutex<Inner>,
    pub tx: broadcast::Sender<ServerMsg>,
    /// Bumped whenever a round closes.
    pub result_tx: watch::Sender<u64>,
}

pub enum ReplaceError {
    Conflict,
    Other(String),
}

impl Sess {
    fn new(home: PathBuf, id: String, path: PathBuf, meta: Meta) -> Sess {
        Sess {
            home,
            id,
            path,
            inner: Mutex::new(Inner {
                meta,
                clients: 0,
                waiters: 0,
                last_hash: None,
            }),
            tx: broadcast::channel(64).0,
            result_tx: watch::channel(0).0,
        }
    }

    pub fn clients(&self) -> usize {
        self.inner.lock().unwrap().clients
    }

    pub fn add_client(&self, delta: isize) {
        let mut i = self.inner.lock().unwrap();
        i.clients = i.clients.saturating_add_signed(delta);
    }

    /// Start a new round if none is active; returns (round, started_new).
    pub fn open_round(&self, home: &Path) -> anyhow::Result<(u32, bool)> {
        let mut i = self.inner.lock().unwrap();
        if i.meta.round > 0 && i.meta.round_state == RoundState::Active {
            return Ok((i.meta.round, false));
        }
        let n = i.meta.round + 1;
        std::fs::create_dir_all(session::session_dir(home, &self.id))?;
        std::fs::copy(&self.path, session::round_snapshot(home, &self.id, n))?;
        i.meta.round = n;
        i.meta.round_state = RoundState::Active;
        session::save_meta(home, &self.id, &i.meta)?;
        Ok((n, true))
    }

    fn doc_locked(&self, i: &Inner) -> Result<DocState, String> {
        let raw = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        let round = i.meta.round;
        let previous = (round > 1)
            .then(|| {
                std::fs::read_to_string(session::done_snapshot(&self.home, &self.id, round - 1))
                    .ok()
            })
            .flatten();
        Ok(DocState {
            path: i.meta.path.clone(),
            round,
            hash: session::sha256_hex(raw.as_bytes()),
            html: crate::render::render(&raw).html,
            comments: super::doc::comment_views(&raw, previous.as_deref()),
            marker_ranges: super::doc::marker_ranges(&raw),
            changes: previous
                .as_deref()
                .map(|p| crate::diff::changes(p, &raw))
                .unwrap_or_default(),
            round_state: if i.meta.round_state == RoundState::Active {
                "active"
            } else {
                "closed"
            }
            .into(),
            author: crate::config::author(&self.home),
            raw,
        })
    }

    /// Current Doc message (or an error message if the file is unreadable).
    pub fn doc_msg(&self) -> ServerMsg {
        let i = self.inner.lock().unwrap();
        match self.doc_locked(&i) {
            Ok(d) => ServerMsg::Doc(d),
            Err(message) => ServerMsg::Error {
                message,
                reloaded: false,
            },
        }
    }

    pub fn broadcast_doc(&self) {
        let mut i = self.inner.lock().unwrap();
        self.broadcast_doc_locked(&mut i);
    }

    fn broadcast_doc_locked(&self, i: &mut Inner) {
        if let Ok(d) = self.doc_locked(i) {
            i.last_hash = Some(d.hash.clone());
            let _ = self.tx.send(ServerMsg::Doc(d));
        }
    }

    /// Debounced watcher callback: push a Doc if the content actually changed.
    pub fn on_file_changed(&self) {
        let mut i = self.inner.lock().unwrap();
        let Ok(data) = std::fs::read(&self.path) else {
            return;
        };
        if i.last_hash.as_deref() != Some(&session::sha256_hex(&data)) {
            self.broadcast_doc_locked(&mut i);
        }
    }

    pub fn replace_source(&self, text: &str, base_hash: &str) -> Result<(), ReplaceError> {
        let mut i = self.inner.lock().unwrap();
        if i.meta.round_state != RoundState::Active {
            return Err(ReplaceError::Other("round is not active".into()));
        }
        let cur = std::fs::read(&self.path).map_err(|e| ReplaceError::Other(e.to_string()))?;
        if session::sha256_hex(&cur) != base_hash {
            return Err(ReplaceError::Conflict);
        }
        session::atomic_write(&self.path, text.as_bytes())
            .map_err(|e| ReplaceError::Other(e.to_string()))?;
        self.broadcast_doc_locked(&mut i);
        Ok(())
    }

    /// Apply an edit to the current file content and write it back atomically, then broadcast
    /// one Doc. `edit` gets the current raw text and returns the new text or a user message.
    pub fn apply_edit(
        &self,
        edit: impl FnOnce(&str) -> Result<String, String>,
    ) -> Result<(), ReplaceError> {
        let mut i = self.inner.lock().unwrap();
        if i.meta.round_state != RoundState::Active {
            return Err(ReplaceError::Other(
                "this round is closed (already sent back); wait for the next round".into(),
            ));
        }
        let cur =
            std::fs::read_to_string(&self.path).map_err(|e| ReplaceError::Other(e.to_string()))?;
        let base = session::sha256_hex(cur.as_bytes());
        let next = edit(&cur).map_err(ReplaceError::Other)?;
        // Hash-checked: refuse if the file changed while we computed the edit.
        let now = std::fs::read(&self.path).map_err(|e| ReplaceError::Other(e.to_string()))?;
        if session::sha256_hex(&now) != base {
            return Err(ReplaceError::Conflict);
        }
        session::atomic_write(&self.path, next.as_bytes())
            .map_err(|e| ReplaceError::Other(e.to_string()))?;
        self.broadcast_doc_locked(&mut i);
        Ok(())
    }

    /// Close the active round with Done (`done = true`) or Abandon.
    pub fn finish(&self, home: &Path, done: bool) -> Result<(), String> {
        let mut i = self.inner.lock().unwrap();
        if i.meta.round_state != RoundState::Active {
            return Err("round is not active".into());
        }
        let n = i.meta.round;
        let result = if done {
            let after = std::fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
            std::fs::write(session::done_snapshot(home, &self.id, n), &after)
                .map_err(|e| e.to_string())?;
            let before = std::fs::read_to_string(session::round_snapshot(home, &self.id, n))
                .unwrap_or_default();
            WaitResult::Done {
                round: n,
                summary: crate::summary::compute(&i.meta.path, n, &before, &after),
            }
        } else {
            WaitResult::Abandoned { round: n }
        };
        i.meta.round_state = if done {
            RoundState::Done
        } else {
            RoundState::Abandoned
        };
        i.meta.last_result = Some(result);
        session::save_meta(home, &self.id, &i.meta).map_err(|e| e.to_string())?;
        drop(i);
        self.result_tx.send_modify(|v| *v += 1);
        let _ = self.tx.send(ServerMsg::RoundClosed { round: n });
        Ok(())
    }
}

/// Counts a pending long-poll for status/idle purposes.
pub struct WaiterGuard<'a> {
    sess: &'a Sess,
    app: &'a App,
}

impl<'a> WaiterGuard<'a> {
    pub fn new(sess: &'a Sess, app: &'a App) -> Self {
        sess.inner.lock().unwrap().waiters += 1;
        WaiterGuard { sess, app }
    }
}

impl Drop for WaiterGuard<'_> {
    fn drop(&mut self) {
        let mut i = self.sess.inner.lock().unwrap();
        i.waiters = i.waiters.saturating_sub(1);
        self.app.touch();
    }
}
