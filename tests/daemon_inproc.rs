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
    // Skip Docs pushed as the waiters went away.
    let m = loop {
        let m = ws.recv().await;
        if m["type"] != "doc" {
            break m;
        }
    };
    assert_eq!(m["type"], "error");

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
    assert_eq!(e["message"], "Comment not found.");
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
    assert!(body.to_lowercase().contains("<html"));
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

const DOC: &str = "# Plan\n\nWe ship on **Friday** now.\n\n- first item\n- second item\n\n```py\na = 1\nb = 2\n```\n";

impl Ws {
    /// Next Doc message (skipping nothing else: errors fail the test).
    async fn doc(&mut self) -> Value {
        let m = self.recv().await;
        assert_eq!(m["type"], "doc", "{m}");
        m
    }
    async fn intent(&mut self, v: Value) -> Value {
        self.send(v).await;
        self.doc().await
    }
}

fn comment_ids(raw: &str) -> Vec<String> {
    annotator::format::parse_comments(raw)
        .into_iter()
        .map(|c| c.id)
        .collect()
}

#[tokio::test]
async fn comment_intents_end_to_end() {
    let h = setup();
    std::fs::write(h.home.join("config.toml"), "author = \"tester\"\n").unwrap();
    let file = h.file("c.md", DOC);
    let o = h.open(&file).await;
    let mut ws = h.ws(&o.session_id).await;
    let d = ws.doc().await;
    assert_eq!(d["author"], "tester");
    assert_eq!(d["round_state"], "active");
    let html = d["html"].as_str().unwrap();
    for sp in ["3:1-3:26", "3:12-3:21", "6:1-7:0", "8:1-11:3"] {
        assert!(
            html.contains(&format!("data-sourcepos=\"{sp}\"")),
            "{sp}: {html}"
        );
    }

    // Paragraph.
    let sel = |sp: &str, text: &str, offset: usize| json!({"sourcepos": sp, "text": text, "offset": offset});
    let d = ws
        .intent(json!({"type":"add_comment","selection":sel("3:1-3:26","ship",3),"body":"when?"}))
        .await;
    let raw = d["raw"].as_str().unwrap().to_string();
    let c = &d["comments"][0];
    let id1 = c["id"].as_str().unwrap().to_string();
    assert!(
        raw.contains(&format!("We <!--c:{id1}-->ship<!--/c:{id1}--> on")),
        "{raw}"
    );
    assert_eq!(c["quote"], "ship");
    assert_eq!(c["author"], "tester");
    assert_eq!(c["entries"][0]["text"], "when?");
    assert!(c["created_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), raw);

    // Bold span.
    let d = ws
        .intent(
            json!({"type":"add_comment","selection":sel("3:12-3:21","Friday",0),"body":"sure?"}),
        )
        .await;
    let raw = d["raw"].as_str().unwrap().to_string();
    let id2 = comment_ids(&raw)[1].clone();
    assert!(
        raw.contains(&format!("**<!--c:{id2}-->Friday<!--/c:{id2}-->**")),
        "{raw}"
    );

    // List item.
    let d = ws
        .intent(json!({"type":"add_comment","selection":sel("6:1-7:0","second",0),"body":"li"}))
        .await;
    let raw = d["raw"].as_str().unwrap().to_string();
    let id3 = comment_ids(&raw)[2].clone();
    assert!(
        raw.contains(&format!("- <!--c:{id3}-->second<!--/c:{id3}--> item")),
        "{raw}"
    );

    // Code block -> line comment.
    let d = ws
        .intent(json!({"type":"add_comment","selection":sel("8:1-11:3","b = 2",6),"body":"why 2"}))
        .await;
    let raw = d["raw"].as_str().unwrap().to_string();
    let views = d["comments"].as_array().unwrap();
    let code = views.iter().find(|c| !c["code_lines"].is_null()).unwrap();
    assert_eq!(code["code_lines"], json!([1, 1]));
    assert_eq!(code["quote"], "b = 2");
    let id4 = code["id"].as_str().unwrap().to_string();
    assert!(
        raw.contains(&format!(
            "<!--c:{id4}-->\n```py\na = 1\nb = 2\n```\n<!--/c:{id4}-->\n"
        )),
        "{raw}"
    );
    // marker_ranges hide all markers/bodies: what remains is the original document.
    let u: Vec<u16> = raw.encode_utf16().collect();
    let mut shown = Vec::new();
    let mut last = 0;
    for r in d["marker_ranges"].as_array().unwrap() {
        let (a, b) = (
            r[0].as_u64().unwrap() as usize,
            r[1].as_u64().unwrap() as usize,
        );
        shown.extend_from_slice(&u[last..a]);
        last = b;
    }
    shown.extend_from_slice(&u[last..]);
    assert_eq!(String::from_utf16(&shown).unwrap(), DOC);

    // Cross-block / not found: error, file untouched.
    ws.send(json!({"type":"add_comment","selection":sel("3:1-3:26","nonexistent",0),"body":"x"}))
        .await;
    let e = ws.recv().await;
    assert_eq!(e["type"], "error");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), raw);

    // Reply, edit, delete entry, resolve, reopen, delete thread.
    let d = ws
        .intent(json!({"type":"reply","id":id1,"body":"Friday"}))
        .await;
    let c = &d["comments"][0];
    assert_eq!(c["entries"].as_array().unwrap().len(), 2);
    assert_eq!(c["entries"][1]["author"], "tester");
    let d = ws
        .intent(json!({"type":"edit_entry","id":id1,"entry":1,"body":"Monday"}))
        .await;
    assert_eq!(d["comments"][0]["entries"][1]["text"], "Monday");
    let d = ws
        .intent(json!({"type":"delete_entry","id":id1,"entry":0}))
        .await;
    assert_eq!(d["comments"][0]["entries"].as_array().unwrap().len(), 1);
    assert_eq!(d["comments"][0]["entries"][0]["text"], "Monday");
    let d = ws.intent(json!({"type":"resolve","id":id1})).await;
    assert_eq!(d["comments"][0]["status"], "resolved");
    let d = ws.intent(json!({"type":"reopen","id":id1})).await;
    assert_eq!(d["comments"][0]["status"], "open");
    let d = ws.intent(json!({"type":"delete_thread","id":id1})).await;
    let raw = d["raw"].as_str().unwrap();
    assert!(!raw.contains(&id1));
    assert!(raw.contains("We ship on"));
    ws.send(json!({"type":"edit_entry","id":id2,"entry":7,"body":"x"}))
        .await;
    assert_eq!(ws.recv().await["message"], "That entry no longer exists.");

    // Closed round rejects intents.
    ws.send(json!({"type":"done"})).await;
    loop {
        if ws.recv().await["type"] == "round_closed" {
            break;
        }
    }
    let before = std::fs::read_to_string(&file).unwrap();
    ws.send(json!({"type":"reply","id":id2,"body":"late"}))
        .await;
    let e = ws.recv().await;
    assert_eq!(e["type"], "error");
    assert!(e["message"].as_str().unwrap().contains("closed"), "{e}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert_eq!(h.current_doc(&o.session_id).await["round_state"], "closed");
}

impl H {
    /// Current Doc via a fresh connection (sent immediately on connect).
    async fn current_doc(&self, id: &str) -> Value {
        self.ws(id).await.doc().await
    }
}

#[tokio::test]
async fn raw_add_with_utf16_offsets() {
    let h = setup();
    let file = h.file("u.md", "Ça 🎉 marche bien\n");
    let o = h.open(&file).await;
    let mut ws = h.ws(&o.session_id).await;
    ws.doc().await;
    // "Ça 🎉 " = 2 + 1 + 2 + 1 = 6 UTF-16 units; "marche" = 6 more.
    let d = ws
        .intent(json!({"type":"add_comment_raw","from":6,"to":12,"body":"ok"}))
        .await;
    let raw = d["raw"].as_str().unwrap();
    let id = d["comments"][0]["id"].as_str().unwrap();
    assert!(
        raw.starts_with(&format!("Ça 🎉 <!--c:{id}-->marche<!--/c:{id}--> bien\n")),
        "{raw}"
    );
    assert_eq!(d["marker_ranges"][0][0], 6);
    assert_eq!(d["comments"][0]["quote"], "marche");
    ws.send(json!({"type":"add_comment_raw","from":0,"to":0,"body":"x"}))
        .await;
    assert_eq!(ws.recv().await["type"], "error");
}

#[tokio::test]
async fn round_two_changes_new_entries_and_summary() {
    let h = setup();
    let file = h.file(
        "r2.md",
        "# T\n\nkeep <!--c:p7k2-->this<!--/c:p7k2-->\n<!--co:p7k2 by:agent status:open\nagent: ok?\n-->\n\nold para\n\ngone para\n",
    );
    let o = h.open(&file).await;
    let id = o.session_id.clone();
    let mut ws = h.ws(&id).await;
    let d = ws.doc().await;
    assert_eq!(d["comments"][0]["new_entries"], 0);
    assert_eq!(d["comments"][0]["new_thread"], false);
    assert!(d["changes"].as_array().unwrap().is_empty());

    // Reviewer: reply, new comment, text edit.
    ws.intent(json!({"type":"reply","id":"p7k2","body":"yes"}))
        .await;
    ws.intent(json!({"type":"add_comment","selection":{"sourcepos":"5:1-5:8","text":"old","offset":0},"body":"rewrite"}))
        .await;
    let d = h.current_doc(&id).await;
    let raw = d["raw"]
        .as_str()
        .unwrap()
        .replace("gone para\n", "gone para\nmore\n");
    ws.send(json!({"type":"replace_source","text":raw,"base_hash":d["hash"]}))
        .await;
    ws.doc().await;
    let new_id = comment_ids(&raw)[1].clone();

    let (r, _) = tokio::join!(h.wait(&id, 1), async {
        wait_waiters(&h, 1).await;
        ws.send(json!({"type":"done"})).await;
    });
    let WaitResult::Done { summary, .. } = r else {
        panic!("{r:?}")
    };
    assert_eq!(summary.new_threads, std::slice::from_ref(&new_id));
    assert_eq!(summary.replied_threads, ["p7k2"]);
    assert_eq!(
        (
            summary.edit_hunks,
            summary.lines_added,
            summary.lines_removed
        ),
        (1, 1, 0)
    );
    let text = annotator::summary::to_text(&summary);
    assert!(
        text.contains(&format!("- 1 new comment: {new_id}")),
        "{text}"
    );
    assert!(
        text.contains("- 1 reply on an existing comment: p7k2"),
        "{text}"
    );
    assert!(text.contains("- edits: 1 hunk, +1 -0 lines"), "{text}");

    // Agent: replies to the new thread, rewrites a paragraph, removes another.
    let cur = std::fs::read_to_string(&file).unwrap();
    let agent =
        annotator::format::append_reply(&cur, &new_id, "2026-01-01T00:00:00.000Z", "agent", "done")
            .unwrap()
            .replace("\ngone para\nmore\n", "\nmore\n")
            .replace("# T", "# Title");
    std::fs::write(&file, &agent).unwrap();
    assert_eq!(h.open(&file).await.round, 2);
    let d = loop {
        let m = ws.recv().await;
        if m["type"] == "doc" && m["round"] == 2 && m["raw"] == agent.as_str() {
            break m;
        }
    };
    assert_eq!(d["round_state"], "active");
    let views = d["comments"].as_array().unwrap();
    let v = views.iter().find(|c| c["id"] == new_id.as_str()).unwrap();
    assert_eq!(
        (v["new_entries"].as_u64(), v["new_thread"].as_bool()),
        (Some(1), Some(false))
    );
    let p = views.iter().find(|c| c["id"] == "p7k2").unwrap();
    assert_eq!(p["new_entries"], 0);
    let changes = d["changes"].as_array().unwrap();
    let kinds: Vec<&str> = changes
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["modified", "modified"], "{changes:?}");
    assert_eq!(changes[0]["old_text"], "# T");
    let html = d["html"].as_str().unwrap();
    assert!(html.contains(&format!(
        "data-sourcepos=\"{}\"",
        changes[0]["sourcepos"].as_str().unwrap()
    )));
    assert_eq!(changes[1]["old_text"], "gone para\nmore");
}

#[tokio::test]
async fn open_after_restart_waits_for_reconnecting_tab() {
    let h = setup();
    let file = h.file("t.md", "x\n");
    let o = h.open(&file).await;
    let mut ws = h.ws(&o.session_id).await;
    ws.recv().await;
    ws.send(json!({"type":"done"})).await;
    h.wait(&o.session_id, 1).await;
    drop(ws);

    // Fresh daemon over the same home: no tab connected yet.
    let h2 = H {
        _home: tempfile::tempdir().unwrap(),
        home: h.home.clone(),
        router: annotator::daemon::test_router(h.home.clone(), PORT, TOKEN.into()).unwrap(),
    };
    let id = o.session_id.clone();
    // A tab reconnects shortly after: open must notice it instead of launching a new tab.
    let reconnect = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        h2.ws(&id).await
    };
    let t = std::time::Instant::now();
    let (o2, _ws2) = tokio::join!(h2.open(&file), reconnect);
    assert_eq!(o2.round, 2);
    let waited = t.elapsed();
    assert!(waited >= Duration::from_millis(300), "{waited:?}");
    assert!(waited < Duration::from_millis(2000), "{waited:?}");
}

#[tokio::test]
async fn waiters_in_doc_and_no_wait_then_join() {
    let h = setup();
    let file = h.file("w.md", "hi\n");
    // Open without waiting (what `open --no-wait` does).
    let o = h.open(&file).await;
    let id = o.session_id.clone();
    let mut ws = h.ws(&id).await;
    let d = ws.doc().await;
    assert_eq!(
        (d["round_state"].as_str(), d["waiters"].as_u64()),
        (Some("active"), Some(0))
    );

    // An agent opens the same file later: joins round 1 and waits; the tab gets a fresh Doc.
    let j = h.open(&file).await;
    assert_eq!((j.session_id.as_str(), j.round), (id.as_str(), 1));
    let (r, _) = tokio::join!(h.wait(&id, 1), async {
        let d = ws.doc().await;
        assert_eq!(
            (d["round"].as_u64(), d["waiters"].as_u64()),
            (Some(1), Some(1))
        );
        ws.send(json!({"type":"done"})).await;
        assert_eq!(ws.recv().await["type"], "round_closed");
        let d = ws.doc().await;
        assert_eq!(
            (d["round_state"].as_str(), d["waiters"].as_u64()),
            (Some("closed"), Some(0))
        );
    });
    assert!(matches!(r, WaitResult::Done { round: 1, .. }));
    // A late wait on a closed round returns immediately and does not push a Doc.
    h.wait(&id, 1).await;
    ws.no_msg(300).await;
    assert_eq!(h.status().await.sessions[0].waiters, 0);
}
