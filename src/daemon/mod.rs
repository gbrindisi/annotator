//! The daemon: axum HTTP + WebSocket server, session registry, file watching, idle shutdown.
//! Owned by the daemon/CLI Worker.
//!
//! Routes:
//! - `POST /api/open`, `GET /api/sessions/{id}/wait?round=N`, `GET /api/status`, `POST /api/stop`
//!   (Bearer token)
//! - `GET /`, `GET /s/{id}`, `GET /assets/{*path}`, `GET /ws/{id}` (cookie; `/s/{id}?t=` sets it)

mod doc;
mod state;
mod watch;
mod ws;

use crate::protocol::{DaemonInfo, OpenRequest, OpenResponse, StatusResponse, WaitResult};
use crate::session::{self, RoundState};
use axum::body::Body;
use axum::extract::{Path as AxPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use state::{App, Sess};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const FALLBACK_HTML: &str = include_str!("fallback.html");
const DEFAULT_PORT: u16 = 7717;

/// Spawn `current_exe() serve --foreground` detached from this process.
pub fn spawn_detached() -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let home = session::home();
    std::fs::create_dir_all(&home)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(home.join("daemon.log"))?;
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .args(["serve", "--foreground"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .process_group(0)
        .spawn()?;
    Ok(())
}

pub fn serve(foreground: bool) -> anyhow::Result<()> {
    if !foreground {
        return spawn_detached();
    }
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "annotator=info".into()),
        )
        .try_init();
    tokio::runtime::Runtime::new()?.block_on(run())
}

fn idle_secs() -> u64 {
    std::env::var("ANNOTATOR_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(900)
}

async fn bind() -> anyhow::Result<tokio::net::TcpListener> {
    let want = std::env::var("ANNOTATOR_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", want)).await {
        return Ok(l);
    }
    Ok(tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?)
}

fn random_token() -> anyhow::Result<String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("getrandom: {e}"))?;
    Ok(session::hex(&b))
}

/// Run the daemon until stopped or idle.
pub async fn run() -> anyhow::Result<()> {
    let home = session::home();
    std::fs::create_dir_all(home.join("sessions"))?;
    let listener = bind().await?;
    let port = listener.local_addr()?.port();
    let token = random_token()?;
    let app = App::new(home.clone(), port, token.clone())?;

    let info = DaemonInfo {
        pid: std::process::id(),
        port,
        token,
    };
    session::atomic_write(&home.join("daemon.json"), &serde_json::to_vec(&info)?)?;
    tracing::info!("annotator daemon listening on 127.0.0.1:{port}");

    tokio::spawn(idle_task(app.clone(), Duration::from_secs(idle_secs())));

    let router = router(app.clone());
    let a2 = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move { a2.shutdown_wait().await })
            .await
    });
    app.shutdown_wait().await;
    // Long-polls keep connections open; give them a moment, then exit anyway.
    let _ = tokio::time::timeout(Duration::from_secs(1), server).await;
    remove_daemon_json(&home);
    tracing::info!("annotator daemon stopped");
    Ok(())
}

fn remove_daemon_json(home: &std::path::Path) {
    let p = home.join("daemon.json");
    if let Ok(d) = std::fs::read(&p)
        && let Ok(info) = serde_json::from_slice::<DaemonInfo>(&d)
        && info.pid == std::process::id()
    {
        let _ = std::fs::remove_file(p);
    }
}

/// Build the full router without binding a socket (for in-process tests). The `Host` check
/// expects `127.0.0.1:<port>` / `localhost:<port>`.
#[doc(hidden)]
pub fn test_router(home: PathBuf, port: u16, token: String) -> anyhow::Result<Router> {
    std::fs::create_dir_all(home.join("sessions"))?;
    Ok(router(App::new(home, port, token)?))
}

async fn idle_task(app: Arc<App>, idle: Duration) {
    let mut idle_since = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if app.busy() {
            idle_since = Instant::now();
        } else if idle_since.elapsed() >= idle {
            tracing::info!("idle for {}s, shutting down", idle.as_secs());
            app.trigger_shutdown();
            return;
        }
    }
}

fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/api/open", post(api_open))
        .route("/api/sessions/{id}/wait", get(api_wait))
        .route("/api/status", get(api_status))
        .route("/api/stop", post(api_stop))
        .route_layer(middleware::from_fn_with_state(app.clone(), require_bearer));
    let browser = Router::new()
        .route("/", get(index))
        .route("/s/{id}", get(session_page))
        .route("/assets/{*path}", get(asset))
        .route("/ws/{id}", get(ws::handler))
        .route_layer(middleware::from_fn_with_state(app.clone(), require_cookie));
    Router::new()
        .merge(api)
        .merge(browser)
        .layer(middleware::from_fn_with_state(app.clone(), check_host))
        .with_state(app)
}

// ---------- security middleware ----------

fn err(code: StatusCode, msg: &str) -> Response {
    (code, msg.to_string()).into_response()
}

fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

async fn check_host(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let ok = host == format!("127.0.0.1:{}", app.port) || host == format!("localhost:{}", app.port);
    if !ok {
        return err(StatusCode::FORBIDDEN, "bad Host");
    }
    next.run(req).await
}

async fn require_bearer(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|t| ct_eq(t.trim(), &app.token));
    if !ok {
        return err(StatusCode::UNAUTHORIZED, "missing or bad token");
    }
    next.run(req).await
}

fn cookie_name(app: &App) -> String {
    format!("annotator_{}", app.port)
}

fn has_cookie(app: &App, headers: &HeaderMap) -> bool {
    let name = cookie_name(app);
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .any(|(k, v)| k == name && ct_eq(v, &app.token))
}

fn query_token(req: &Request) -> Option<String> {
    req.uri().query().and_then(|q| {
        q.split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "t")
            .map(|(_, v)| v.to_string())
    })
}

async fn require_cookie(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    // Token exchange: /s/{id}?t=TOKEN -> cookie + redirect without the token.
    if path.starts_with("/s/")
        && let Some(t) = query_token(&req)
    {
        if !ct_eq(&t, &app.token) {
            return err(StatusCode::UNAUTHORIZED, "bad token");
        }
        let mut resp = Redirect::to(&path).into_response();
        *resp.status_mut() = StatusCode::FOUND;
        let cookie = format!(
            "{}={}; HttpOnly; SameSite=Strict; Path=/",
            cookie_name(&app),
            app.token
        );
        resp.headers_mut()
            .insert(header::SET_COOKIE, cookie.parse().expect("valid cookie"));
        return resp;
    }
    if !has_cookie(&app, req.headers()) {
        return err(
            StatusCode::UNAUTHORIZED,
            "not authorized: open the link printed by `annotator open`",
        );
    }
    if path.starts_with("/ws/") {
        let origin = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let ok = origin == format!("http://127.0.0.1:{}", app.port)
            || origin == format!("http://localhost:{}", app.port);
        if !ok {
            return err(StatusCode::FORBIDDEN, "bad Origin");
        }
    }
    next.run(req).await
}

// ---------- API ----------

type ApiResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn api_open(
    State(app): State<Arc<App>>,
    Json(req): Json<OpenRequest>,
) -> ApiResult<OpenResponse> {
    let canonical = std::fs::canonicalize(PathBuf::from(&req.path))
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("{}: {e}", req.path)))?;
    if !canonical.is_file() {
        return Err((StatusCode::BAD_REQUEST, format!("{}: not a file", req.path)));
    }
    let sess = app.session_for_path(&canonical).map_err(internal)?;
    let (round, new_round) = sess.open_round(&app.home).map_err(internal)?;
    app.touch();
    if new_round {
        sess.broadcast_doc();
    }
    let url = format!(
        "http://127.0.0.1:{}/s/{}?t={}",
        app.port, sess.id, app.token
    );
    if req.browser && sess.clients() == 0 {
        launch_browser(&url);
    }
    Ok(Json(OpenResponse {
        session_id: sess.id.clone(),
        round,
        url,
    }))
}

fn launch_browser(url: &str) {
    use std::process::{Command, Stdio};
    let cmd = match std::env::var("ANNOTATOR_BROWSER") {
        Ok(v) if v == "none" => return,
        Ok(v) if !v.is_empty() => v,
        _ if cfg!(target_os = "macos") => "open".into(),
        _ => "xdg-open".into(),
    };
    let r = Command::new(&cmd)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match r {
        // Reap in the background so no zombie lingers.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => tracing::warn!("could not launch browser ({cmd}): {e}"),
    }
}

#[derive(serde::Deserialize)]
struct WaitQuery {
    round: u32,
}

async fn api_wait(
    State(app): State<Arc<App>>,
    AxPath(id): AxPath<String>,
    Query(q): Query<WaitQuery>,
) -> ApiResult<WaitResult> {
    let sess = app
        .get_session(&id)
        .ok_or((StatusCode::NOT_FOUND, "no such session".into()))?;
    sess.wait(&app, q.round).await.map(Json)
}

async fn api_status(State(app): State<Arc<App>>) -> Json<StatusResponse> {
    Json(app.status())
}

async fn api_stop(State(app): State<Arc<App>>) -> &'static str {
    app.trigger_shutdown();
    "stopping\n"
}

// ---------- browser ----------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn index(State(app): State<Arc<App>>) -> Html<String> {
    let mut rows = String::new();
    for s in app.status().sessions {
        rows.push_str(&format!(
            "<li><a href=\"/s/{id}\">{path}</a> &mdash; round {round}, {state}</li>\n",
            id = esc(&s.id),
            path = esc(&s.path),
            round = s.round,
            state = esc(&s.state)
        ));
    }
    if rows.is_empty() {
        rows = "<li>No sessions.</li>".into();
    }
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8><title>annotator</title></head>\
         <body style=\"font-family:sans-serif\"><h1>annotator sessions</h1><ul>{rows}</ul></body></html>"
    ))
}

async fn session_page(State(app): State<Arc<App>>, AxPath(id): AxPath<String>) -> Response {
    if app.get_session(&id).is_none() {
        return err(StatusCode::NOT_FOUND, "no such session");
    }
    let body = match crate::assets::Assets::get("index.html") {
        Some(f) => f.data.into_owned(),
        None => FALLBACK_HTML.as_bytes().to_vec(),
    };
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}

async fn asset(AxPath(path): AxPath<String>) -> Response {
    match crate::assets::Assets::get(&format!("assets/{path}"))
        .or_else(|| crate::assets::Assets::get(&path))
    {
        Some(f) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(f.data.into_owned()))
                .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "asset"))
        }
        None => err(StatusCode::NOT_FOUND, "not found"),
    }
}

impl Sess {
    /// Long-poll until `round` ends.
    async fn wait(&self, app: &App, round: u32) -> Result<WaitResult, (StatusCode, String)> {
        let mut rx = self.result_tx.subscribe();
        let _guard = state::WaiterGuard::new(self, app);
        loop {
            {
                let inner = self.inner.lock().unwrap();
                if round > inner.meta.round {
                    return Err((StatusCode::NOT_FOUND, format!("round {round} not started")));
                }
                if let Some(r) = &inner.meta.last_result
                    && result_round(r) == round
                {
                    return Ok(r.clone());
                }
                if round < inner.meta.round || inner.meta.round_state != RoundState::Active {
                    return Err((
                        StatusCode::GONE,
                        format!("round {round} result no longer available"),
                    ));
                }
            }
            if rx.changed().await.is_err() {
                return Err(internal("session dropped"));
            }
        }
    }
}

pub(crate) fn result_round(r: &WaitResult) -> u32 {
    match r {
        WaitResult::Done { round, .. } | WaitResult::Abandoned { round } => *round,
    }
}
