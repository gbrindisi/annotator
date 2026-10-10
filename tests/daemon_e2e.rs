//! End-to-end tests against the built binary with a temp ANNOTATOR_HOME.

use annotator::protocol::{DaemonInfo, OpenResponse, StatusResponse, WaitResult};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

/// Some sandboxes block loopback TCP; these tests need it.
fn loopback_ok() -> bool {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let ok = std::net::TcpStream::connect(l.local_addr().unwrap()).is_ok();
    if !ok {
        eprintln!("loopback TCP unavailable; skipping");
    }
    ok
}

const BIN: &str = env!("CARGO_BIN_EXE_annotator");

struct Env {
    home: tempfile::TempDir,
    idle: u64,
}

impl Env {
    fn new() -> Env {
        Env::with_idle(60)
    }
    fn with_idle(idle: u64) -> Env {
        Env {
            home: tempfile::tempdir().unwrap(),
            idle,
        }
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env("ANNOTATOR_HOME", self.home.path())
            .env("ANNOTATOR_PORT", "0")
            .env("ANNOTATOR_BROWSER", "none")
            .env("ANNOTATOR_IDLE_SECS", self.idle.to_string());
        c
    }
    fn file(&self, name: &str, content: &str) -> PathBuf {
        let dir = self.home.path().join("docs");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p.canonicalize().unwrap()
    }
    fn info(&self) -> Option<DaemonInfo> {
        let d = std::fs::read(self.home.path().join("daemon.json")).ok()?;
        serde_json::from_slice(&d).ok()
    }
    async fn start(&self) -> Daemon {
        self.cmd(&["serve"]).status().unwrap();
        self.wait_daemon(None).await
    }
    async fn wait_daemon(&self, not_pid: Option<u32>) -> Daemon {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(info) = self.info()
                && Some(info.pid) != not_pid
            {
                let d = Daemon {
                    info,
                    http: reqwest::Client::new(),
                };
                let ok = d
                    .http
                    .get(d.url("/api/status"))
                    .bearer_auth(&d.info.token)
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success());
                if ok {
                    return d;
                }
            }
            assert!(Instant::now() < deadline, "daemon did not start");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(i) = self.info() {
            unsafe { libc::kill(i.pid as i32, libc::SIGKILL) };
        }
    }
}

struct Daemon {
    info: DaemonInfo,
    http: reqwest::Client,
}

impl Daemon {
    fn url(&self, p: &str) -> String {
        format!("http://127.0.0.1:{}{p}", self.info.port)
    }
    async fn get(&self, p: &str) -> reqwest::Response {
        self.http
            .get(self.url(p))
            .bearer_auth(&self.info.token)
            .send()
            .await
            .unwrap()
    }
    async fn open(&self, path: &Path) -> OpenResponse {
        let r = self
            .http
            .post(self.url("/api/open"))
            .bearer_auth(&self.info.token)
            .json(&json!({"path": path, "browser": true}))
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success(), "{:?}", r.text().await);
        r.json().await.unwrap()
    }
    async fn wait(&self, id: &str, round: u32) -> WaitResult {
        self.get(&format!("/api/sessions/{id}/wait?round={round}"))
            .await
            .json()
            .await
            .unwrap()
    }
    async fn status(&self) -> StatusResponse {
        self.get("/api/status").await.json().await.unwrap()
    }
    fn cookie(&self) -> String {
        format!("annotator_{}={}", self.info.port, self.info.token)
    }
    async fn ws(&self, id: &str) -> Ws {
        let mut req = format!("ws://127.0.0.1:{}/ws/{id}", self.info.port)
            .into_client_request()
            .unwrap();
        let h = req.headers_mut();
        h.insert("cookie", self.cookie().parse().unwrap());
        h.insert(
            "origin",
            format!("http://127.0.0.1:{}", self.info.port)
                .parse()
                .unwrap(),
        );
        let (s, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        Ws(s)
    }
}

struct Ws(
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
);

impl Ws {
    async fn recv(&mut self) -> Value {
        loop {
            let m = tokio::time::timeout(Duration::from_secs(5), self.0.next())
                .await
                .expect("ws timeout")
                .unwrap()
                .unwrap();
            if let Message::Text(t) = m {
                return serde_json::from_str(&t).unwrap();
            }
        }
    }
    async fn send(&mut self, v: Value) {
        self.0
            .send(Message::Text(v.to_string().into()))
            .await
            .unwrap();
    }
    async fn no_msg(&mut self, ms: u64) {
        let r = tokio::time::timeout(Duration::from_millis(ms), self.0.next()).await;
        assert!(r.is_err(), "unexpected message: {r:?}");
    }
}

fn spawn_open(env: &Env, file: &Path) -> Child {
    env.cmd(&["open", file.to_str().unwrap(), "--no-browser"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

async fn wait_child(mut c: Child) -> (i32, String) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(st) = c.try_wait().unwrap() {
            let mut out = String::new();
            std::io::Read::read_to_string(c.stdout.as_mut().unwrap(), &mut out).unwrap();
            return (st.code().unwrap_or(-1), out);
        }
        assert!(Instant::now() < deadline, "client did not exit");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_waiters(d: &Daemon, n: usize) -> StatusResponse {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let s = d.status().await;
        if s.sessions.iter().map(|s| s.waiters).sum::<usize>() >= n {
            return s;
        }
        assert!(Instant::now() < deadline, "no waiter appeared");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn client_done_rounds_and_join() {
    if !loopback_ok() {
        return;
    }
    let env = Env::new();
    let file = env.file("plan.md", "# Plan\n\none\ntwo\n");
    let client = spawn_open(&env, &file);
    let d = env.wait_daemon(None).await;
    let st = wait_waiters(&d, 1).await;
    let id = st.sessions[0].id.clone();
    assert_eq!(st.sessions[0].round, 1);
    assert_eq!(st.sessions[0].state, "active");

    // Joining an active round: same round, no new snapshot.
    let j = d.open(&file).await;
    assert_eq!((j.session_id.as_str(), j.round), (id.as_str(), 1));
    assert!(j.url.contains("?t="));
    let sdir = env.home.path().join("sessions").join(&id);
    assert!(sdir.join("round-1.md").exists());
    assert!(!sdir.join("round-2.md").exists());

    let mut ws = d.ws(&id).await;
    let doc = ws.recv().await;
    assert_eq!(doc["type"], "doc");
    assert_eq!(doc["round"], 1);
    assert!(doc["html"].as_str().unwrap().contains("Plan"));

    // Edit through the browser, then Done.
    let hash = doc["hash"].as_str().unwrap().to_string();
    ws.send(json!({"type":"replace_source","text":"# Plan\n\none\nTWO\nthree\n","base_hash":hash}))
        .await;
    assert_eq!(ws.recv().await["type"], "doc");
    ws.send(json!({"type":"done"})).await;
    let closed = ws.recv().await;
    assert_eq!(closed, json!({"type":"round_closed","round":1}));

    let (code, out) = wait_child(client).await;
    assert_eq!(code, 0);
    assert!(out.contains("finished (round 1)"), "{out}");
    assert!(out.contains("edits: 1 hunk, +2 -1 lines"), "{out}");
    assert!(sdir.join("done-1.md").exists());
    // A late waiter on round 1 gets the result immediately.
    match d.wait(&id, 1).await {
        WaitResult::Done { round, summary } => {
            assert_eq!(round, 1);
            assert_eq!(summary.lines_added, 2);
        }
        r => panic!("{r:?}"),
    }

    // Round 2: new snapshot, Doc pushed to the open tab.
    let r2 = d.open(&file).await;
    assert_eq!(r2.round, 2);
    assert!(sdir.join("round-2.md").exists());
    let doc = ws.recv().await;
    assert_eq!(
        (doc["type"].as_str(), doc["round"].as_u64()),
        (Some("doc"), Some(2))
    );
    let meta: Value =
        serde_json::from_slice(&std::fs::read(sdir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["round"], 2);
    assert_eq!(meta["round_state"], "active");

    // Two waiters both get Abandoned.
    let (a, b) = tokio::join!(d.wait(&id, 2), d.wait(&id, 2), async {
        wait_waiters(&d, 2).await;
        ws.send(json!({"type":"abandon"})).await;
    })
    .into_pair();
    assert!(matches!(a, WaitResult::Abandoned { round: 2 }));
    assert!(matches!(b, WaitResult::Abandoned { round: 2 }));
}

trait IntoPair<A, B> {
    fn into_pair(self) -> (A, B);
}
impl<A, B, C> IntoPair<A, B> for (A, B, C) {
    fn into_pair(self) -> (A, B) {
        (self.0, self.1)
    }
}

#[tokio::test]
async fn stale_replace_rejected_and_external_write_pushes() {
    if !loopback_ok() {
        return;
    }
    let env = Env::new();
    let d = env.start().await;
    let file = env.file("a.md", "hello\n");
    let o = d.open(&file).await;
    let mut ws = d.ws(&o.session_id).await;
    assert_eq!(ws.recv().await["type"], "doc");

    ws.send(json!({"type":"replace_source","text":"evil\n","base_hash":"0000"}))
        .await;
    let e = ws.recv().await;
    assert_eq!(e["type"], "error");
    assert_eq!(e["reloaded"], true);
    assert_eq!(ws.recv().await["type"], "doc");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    ws.send(json!({"type":"resolve","id":"x"})).await;
    let e = ws.recv().await;
    assert_eq!(
        (e["message"].as_str(), e["reloaded"].as_bool()),
        (Some("Comment not found."), Some(false))
    );

    std::fs::write(&file, "changed externally\n").unwrap();
    let doc = ws.recv().await;
    assert_eq!(doc["type"], "doc");
    assert_eq!(doc["raw"], "changed externally\n");
    // Debounced: only one push for one write.
    ws.no_msg(600).await;

    // A daemon write yields exactly one broadcast too.
    let hash = doc["hash"].as_str().unwrap().to_string();
    ws.send(json!({"type":"replace_source","text":"mine\n","base_hash":hash}))
        .await;
    assert_eq!(ws.recv().await["raw"], "mine\n");
    ws.no_msg(600).await;
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine\n");
}

#[tokio::test]
async fn security_checks() {
    if !loopback_ok() {
        return;
    }
    let env = Env::new();
    let d = env.start().await;
    let file = env.file("s.md", "x\n");
    let o = d.open(&file).await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    // API without / with bad token.
    let r = http.get(d.url("/api/status")).send().await.unwrap();
    assert_eq!(r.status(), 401);
    let r = http
        .get(d.url("/api/status"))
        .bearer_auth("nope")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    // Bad Host.
    let r = http
        .get(d.url("/api/status"))
        .bearer_auth(&d.info.token)
        .header("host", "evil.example:80")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    let r = http
        .get(d.url("/api/status"))
        .bearer_auth(&d.info.token)
        .header("host", format!("localhost:{}", d.info.port))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);

    // Browser routes need the cookie.
    let page = format!("/s/{}", o.session_id);
    assert_eq!(http.get(d.url(&page)).send().await.unwrap().status(), 401);
    assert_eq!(http.get(d.url("/")).send().await.unwrap().status(), 401);
    let r = http.get(&o.url).send().await.unwrap();
    assert_eq!(r.status(), 302);
    assert_eq!(r.headers()["location"], page.as_str());
    let c = r.headers()["set-cookie"].to_str().unwrap();
    assert!(
        c.contains("HttpOnly") && c.contains("SameSite=Strict"),
        "{c}"
    );
    let r = http
        .get(d.url(&page))
        .header("cookie", d.cookie())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.text().await.unwrap().contains("WebSocket"));
    let r = http
        .get(d.url("/"))
        .header("cookie", d.cookie())
        .send()
        .await
        .unwrap();
    assert!(r.text().await.unwrap().contains(&o.session_id));
    let r = http
        .get(d.url(&format!("/s/{}?t=wrong", o.session_id)))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    // WebSocket: bad Origin / no cookie rejected.
    let ws_url = format!("ws://127.0.0.1:{}/ws/{}", d.info.port, o.session_id);
    let mut req = ws_url.clone().into_client_request().unwrap();
    req.headers_mut()
        .insert("cookie", d.cookie().parse().unwrap());
    req.headers_mut()
        .insert("origin", "http://evil.example".parse().unwrap());
    assert!(tokio_tungstenite::connect_async(req).await.is_err());
    let mut req = ws_url.into_client_request().unwrap();
    req.headers_mut().insert(
        "origin",
        format!("http://127.0.0.1:{}", d.info.port).parse().unwrap(),
    );
    assert!(tokio_tungstenite::connect_async(req).await.is_err());
}

#[tokio::test]
async fn idle_shutdown_removes_daemon_json() {
    if !loopback_ok() {
        return;
    }
    let env = Env::with_idle(1);
    let d = env.start().await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while env.home.path().join("daemon.json").exists() {
        assert!(Instant::now() < deadline, "daemon did not exit when idle");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(d.http.get(d.url("/api/status")).send().await.is_err());
}

#[tokio::test]
async fn stop_and_status_commands() {
    if !loopback_ok() {
        return;
    }
    let env = Env::new();
    let out = env.cmd(&["status"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("daemon not running"));
    let _d = env.start().await;
    let out = env.cmd(&["status"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("daemon pid"));
    assert!(env.cmd(&["stop"]).status().unwrap().success());
    assert!(!env.home.path().join("daemon.json").exists());
}

#[tokio::test]
async fn client_respawns_killed_daemon() {
    if !loopback_ok() {
        return;
    }
    let env = Env::new();
    let file = env.file("k.md", "a\n");
    let client = spawn_open(&env, &file);
    let d = env.wait_daemon(None).await;
    let st = wait_waiters(&d, 1).await;
    let id = st.sessions[0].id.clone();
    unsafe { libc::kill(d.info.pid as i32, libc::SIGKILL) };

    let d2 = env.wait_daemon(Some(d.info.pid)).await;
    let st = wait_waiters(&d2, 1).await;
    assert_eq!(st.sessions[0].round, 1, "same round continues");
    let mut ws = d2.ws(&id).await;
    assert_eq!(ws.recv().await["round"], 1);
    ws.send(json!({"type":"done"})).await;
    let (code, out) = wait_child(client).await;
    assert_eq!(code, 0);
    assert!(out.contains("no changes"), "{out}");
}
