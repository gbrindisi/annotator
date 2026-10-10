//! In-process daemon tests: the router is driven over in-memory duplex streams, so these run
//! even where loopback TCP is unavailable.

use annotator::protocol::{OpenResponse, StatusResponse, WaitResult};
use axum::Router;
use axum::body::Body;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use tower::ServiceExt;

const PORT: u16 = 7799;
const TOKEN: &str = "tok123";

struct H {
    _home: tempfile::TempDir,
    home: PathBuf,
    router: Router,
}

fn setup() -> H {
    // SAFETY: every test in this binary sets the same value.
    unsafe { std::env::set_var("ANNOTATOR_BROWSER", "none") };
    let home = tempfile::tempdir().unwrap();
    let p = home.path().to_path_buf();
    H {
        router: annotator::daemon::test_router(p.clone(), PORT, TOKEN.into()).unwrap(),
        home: p,
        _home: home,
    }
}

fn req(method: &str, uri: &str) -> axum::http::request::Builder {
    axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("host", format!("127.0.0.1:{PORT}"))
}

impl H {
    fn file(&self, name: &str, content: &str) -> PathBuf {
        let dir = self.home.join("docs");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p.canonicalize().unwrap()
    }
    async fn call(&self, r: axum::http::Request<Body>) -> (u16, axum::http::HeaderMap, String) {
        let resp = self.router.clone().oneshot(r).await.unwrap();
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8_lossy(&body).into_owned())
    }
    async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> (u16, String) {
        let b = req(method, uri)
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json");
        let r = b
            .body(body.map_or(Body::empty(), |v| Body::from(v.to_string())))
            .unwrap();
        let (s, _, t) = self.call(r).await;
        (s, t)
    }
    async fn open(&self, p: &Path) -> OpenResponse {
        let (s, t) = self
            .api(
                "POST",
                "/api/open",
                Some(json!({"path": p, "browser": true})),
            )
            .await;
        assert_eq!(s, 200, "{t}");
        serde_json::from_str(&t).unwrap()
    }
    async fn wait(&self, id: &str, round: u32) -> WaitResult {
        let (s, t) = self
            .api(
                "GET",
                &format!("/api/sessions/{id}/wait?round={round}"),
                None,
            )
            .await;
        assert_eq!(s, 200, "{t}");
        serde_json::from_str(&t).unwrap()
    }
    async fn status(&self) -> StatusResponse {
        serde_json::from_str(&self.api("GET", "/api/status", None).await.1).unwrap()
    }
    async fn ws_with(&self, id: &str, cookie: bool, origin: &str) -> Result<Ws, String> {
        let (a, b) = tokio::io::duplex(1 << 20);
        let svc = hyper_util::service::TowerToHyperService::new(self.router.clone());
        tokio::spawn(async move {
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(b), svc)
                .with_upgrades()
                .await;
        });
        let mut r = format!("ws://127.0.0.1:{PORT}/ws/{id}")
            .into_client_request()
            .unwrap();
        if cookie {
            r.headers_mut().insert(
                "cookie",
                format!("annotator_{PORT}={TOKEN}").parse().unwrap(),
            );
        }
        r.headers_mut().insert("origin", origin.parse().unwrap());
        tokio_tungstenite::client_async(r, a)
            .await
            .map(|(s, _)| Ws(s))
            .map_err(|e| e.to_string())
    }
    async fn ws(&self, id: &str) -> Ws {
        self.ws_with(id, true, &format!("http://127.0.0.1:{PORT}"))
            .await
            .unwrap()
    }
}

struct Ws(tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>);

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

async fn wait_waiters(h: &H, n: usize) {
    for _ in 0..200 {
        if h.status()
            .await
            .sessions
            .iter()
            .map(|s| s.waiters)
            .sum::<usize>()
            >= n
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("waiters did not appear");
}

#[tokio::test]
async fn rounds_done_join_and_waiters() {
    let h = setup();
    let file = h.file("plan.md", "# Plan\n\none\ntwo\n");
    let o = h.open(&file).await;
    assert_eq!(o.round, 1);
    assert_eq!(o.session_id.len(), 12);
    assert_eq!(
        o.url,
        format!("http://127.0.0.1:{PORT}/s/{}?t={TOKEN}", o.session_id)
    );
    let sdir = h.home.join("sessions").join(&o.session_id);
    assert_eq!(
        std::fs::read_to_string(sdir.join("round-1.md")).unwrap(),
        "# Plan\n\none\ntwo\n"
    );

    // Join while active.
    std::fs::write(&file, "# Plan\n\none\nTWO\nthree\n").unwrap();
    let j = h.open(&file).await;
    assert_eq!((j.session_id, j.round), (o.session_id.clone(), 1));
    assert!(!sdir.join("round-2.md").exists());

    let mut ws = h.ws(&o.session_id).await;
    let doc = ws.recv().await;
    assert_eq!(doc["type"], "doc");
    assert_eq!(doc["round"], 1);
    let st = h.status().await;
    assert_eq!(st.sessions[0].clients, 1);
    assert_eq!(st.sessions[0].state, "active");

    // Two waiters, then Done.
    let id = o.session_id.clone();
    let (a, b, _) = tokio::join!(h.wait(&id, 1), h.wait(&id, 1), async {
        wait_waiters(&h, 2).await;
        ws.send(json!({"type":"done"})).await;
    });
    for r in [a, b] {
        match r {
            WaitResult::Done { round, summary } => {
                assert_eq!(round, 1);
                assert_eq!(
                    (
                        summary.edit_hunks,
                        summary.lines_added,
                        summary.lines_removed
                    ),
                    (1, 2, 1)
                );
                assert_eq!(summary.path, file.to_string_lossy());
            }
            r => panic!("{r:?}"),
        }
    }
    // The round_closed message may be preceded by a watcher Doc from the external write.
    loop {
        let m = ws.recv().await;
        if m["type"] == "round_closed" {
            assert_eq!(m["round"], 1);
            break;
        }
    }
    assert!(sdir.join("done-1.md").exists());
    // Late waiter: immediate.
    assert!(matches!(
        h.wait(&id, 1).await,
        WaitResult::Done { round: 1, .. }
    ));
    // Done twice is an error.
    ws.send(json!({"type":"done"})).await;
    assert_eq!(ws.recv().await["type"], "error");

    // Round 2 pushes a Doc to the connected tab.
    let r2 = h.open(&file).await;
    assert_eq!(r2.round, 2);
    assert!(sdir.join("round-2.md").exists());
    loop {
        let m = ws.recv().await;
        if m["type"] == "doc" && m["round"] == 2 {
            break;
        }
    }
    let meta: Value =
        serde_json::from_slice(&std::fs::read(sdir.join("meta.json")).unwrap()).unwrap();
    assert_eq!(
        (meta["round"].as_u64(), meta["round_state"].as_str()),
        (Some(2), Some("active"))
    );

    let (r, _) = tokio::join!(h.wait(&id, 2), async {
        wait_waiters(&h, 1).await;
        ws.send(json!({"type":"abandon"})).await;
    });
    assert!(matches!(r, WaitResult::Abandoned { round: 2 }));
    assert_eq!(h.status().await.sessions[0].state, "abandoned");

    // Waiting on a future round is a 404.
    let (s, _) = h
        .api("GET", &format!("/api/sessions/{id}/wait?round=9"), None)
        .await;
    assert_eq!(s, 404);
}

#[tokio::test]
async fn sessions_survive_restart() {
    let h = setup();
    let file = h.file("r.md", "x\n");
    let o = h.open(&file).await;
    // A fresh daemon over the same home continues round 1.
    let router2 = annotator::daemon::test_router(h.home.clone(), PORT, TOKEN.into()).unwrap();
    let h2 = H {
        _home: tempfile::tempdir().unwrap(),
        home: h.home.clone(),
        router: router2,
    };
    let mut ws = h2.ws(&o.session_id).await;
    assert_eq!(ws.recv().await["round"], 1);
    let o2 = h2.open(&file).await;
    assert_eq!(o2.round, 1);
}

#[tokio::test]
async fn replace_source_and_watcher() {
    let h = setup();
    let file = h.file("a.md", "hello\n");
    let o = h.open(&file).await;
    let mut ws = h.ws(&o.session_id).await;
    let doc = ws.recv().await;

    ws.send(json!({"type":"replace_source","text":"evil\n","base_hash":"0000"}))
        .await;
    let e = ws.recv().await;
    assert_eq!(
        (e["type"].as_str(), e["reloaded"].as_bool()),
        (Some("error"), Some(true))
    );
    assert_eq!(ws.recv().await["type"], "doc");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");

    ws.send(json!({"type":"reply","id":"x","body":"b"})).await;
    let e = ws.recv().await;
    assert_eq!(e["message"], "not implemented");
    assert_eq!(e["reloaded"], false);

    // Daemon write: exactly one Doc.
    ws.send(json!({"type":"replace_source","text":"mine\n","base_hash":doc["hash"]}))
        .await;
    assert_eq!(ws.recv().await["raw"], "mine\n");
    ws.no_msg(700).await;
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine\n");

    // External write: one Doc.
    std::fs::write(&file, "external\n").unwrap();
    let d = ws.recv().await;
    assert_eq!(
        (d["type"].as_str(), d["raw"].as_str()),
        (Some("doc"), Some("external\n"))
    );
    ws.no_msg(700).await;
}

#[tokio::test]
async fn security() {
    let h = setup();
    let file = h.file("s.md", "x\n");
    let o = h.open(&file).await;

    let (s, _, _) = h
        .call(req("GET", "/api/status").body(Body::empty()).unwrap())
        .await;
    assert_eq!(s, 401);
    let r = axum::http::Request::builder()
        .uri("/api/status")
        .header("host", "evil.example")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(h.call(r).await.0, 403);
    let r = axum::http::Request::builder()
        .uri("/api/status")
        .header("host", format!("localhost:{PORT}"))
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(h.call(r).await.0, 200);

    let page = format!("/s/{}", o.session_id);
    let (s, _, _) = h.call(req("GET", &page).body(Body::empty()).unwrap()).await;
    assert_eq!(s, 401);
    let (s, hd, _) = h
        .call(
            req("GET", &format!("{page}?t={TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(s, 302);
    assert_eq!(hd["location"], page.as_str());
    let c = hd["set-cookie"].to_str().unwrap();
    assert!(c.starts_with(&format!("annotator_{PORT}={TOKEN}")));
    assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict"));
    let (s, _, _) = h
        .call(
            req("GET", &format!("{page}?t=bad"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(s, 401);
    let cookie = format!("annotator_{PORT}={TOKEN}");
    let (s, _, body) = h
        .call(
            req("GET", &page)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(s, 200);
    assert!(body.contains("WebSocket"));
    let (s, _, body) = h
        .call(
            req("GET", "/")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(s, 200);
    assert!(body.contains(&o.session_id));
    // Unknown / malformed session ids.
    let (s, _, _) = h
        .call(
            req("GET", "/s/..%2f..")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(s, 404);

    let good = format!("http://127.0.0.1:{PORT}");
    assert!(
        h.ws_with(&o.session_id, true, "http://evil.example")
            .await
            .is_err()
    );
    assert!(h.ws_with(&o.session_id, false, &good).await.is_err());
    assert!(h.ws_with(&o.session_id, true, &good).await.is_ok());

    // Paths that do not exist are rejected.
    let (s, _) = h
        .api(
            "POST",
            "/api/open",
            Some(json!({"path":"/nonexistent/x.md","browser":false})),
        )
        .await;
    assert_eq!(s, 400);
}
