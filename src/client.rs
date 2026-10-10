//! CLI client: discover or spawn the daemon, register a session, long-poll until Done.
//! Owned by the daemon/CLI Worker.

use crate::protocol::{DaemonInfo, OpenRequest, OpenResponse, StatusResponse, WaitResult};
use crate::session;
use anyhow::{Context, bail};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, Instant};

struct Daemon {
    info: DaemonInfo,
    http: reqwest::Client,
}

impl Daemon {
    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.info.port)
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        rb.bearer_auth(&self.info.token)
    }

    async fn status(&self) -> anyhow::Result<StatusResponse> {
        let r = self
            .auth(self.http.get(self.url("/api/status")))
            .timeout(Duration::from_secs(2))
            .send()
            .await?
            .error_for_status()?;
        Ok(r.json().await?)
    }
}

fn pid_alive(pid: u32) -> bool {
    // SAFETY: kill with signal 0 only checks for existence/permission.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// A daemon recorded in daemon.json that answers `/api/status`.
async fn find_daemon() -> Option<Daemon> {
    let data = std::fs::read(session::home().join("daemon.json")).ok()?;
    let info: DaemonInfo = serde_json::from_slice(&data).ok()?;
    if !pid_alive(info.pid) {
        return None;
    }
    let d = Daemon {
        info,
        http: reqwest::Client::new(),
    };
    d.status().await.ok().map(|_| d)
}

/// Find a live daemon or spawn one (guarded by a spawn lock) and wait until it answers.
async fn ensure_daemon() -> anyhow::Result<Daemon> {
    if let Some(d) = find_daemon().await {
        return Ok(d);
    }
    let home = session::home();
    std::fs::create_dir_all(&home)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("spawn.lock"))?;
    let fd = lock.as_raw_fd();
    // Blocking flock on a worker thread; released when `lock` is dropped.
    tokio::task::spawn_blocking(move || {
        // SAFETY: fd stays valid: `lock` outlives this await.
        unsafe { libc::flock(fd, libc::LOCK_EX) }
    })
    .await?;
    if let Some(d) = find_daemon().await {
        return Ok(d);
    }
    let _ = std::fs::remove_file(home.join("daemon.json"));
    crate::daemon::spawn_detached().context("spawning daemon")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(d) = find_daemon().await {
            drop(lock);
            return Ok(d);
        }
    }
    bail!(
        "daemon did not start; see {}",
        home.join("daemon.log").display()
    )
}

async fn post_open(d: &Daemon, path: &str, browser: bool) -> anyhow::Result<OpenResponse> {
    let r = d
        .auth(d.http.post(d.url("/api/open")))
        .timeout(Duration::from_secs(10))
        .json(&OpenRequest {
            path: path.to_string(),
            browser,
        })
        .send()
        .await?;
    if !r.status().is_success() {
        bail!("open failed: {}", r.text().await.unwrap_or_default().trim());
    }
    Ok(r.json().await?)
}

enum WaitErr {
    /// Connection-level failure: daemon gone, retry.
    Transport(anyhow::Error),
    Fatal(anyhow::Error),
}

async fn wait(d: &Daemon, id: &str, round: u32) -> Result<WaitResult, WaitErr> {
    let r = d
        .auth(
            d.http
                .get(d.url(&format!("/api/sessions/{id}/wait?round={round}"))),
        )
        .send()
        .await
        .map_err(|e| WaitErr::Transport(e.into()))?;
    if !r.status().is_success() {
        let msg = r.text().await.unwrap_or_default();
        return Err(WaitErr::Fatal(anyhow::anyhow!(
            "wait failed: {}",
            msg.trim()
        )));
    }
    r.json().await.map_err(|e| WaitErr::Transport(e.into()))
}

async fn open_async(file: &Path, json: bool, no_browser: bool) -> anyhow::Result<i32> {
    let canonical = std::fs::canonicalize(file).with_context(|| format!("{}", file.display()))?;
    let path = canonical.to_string_lossy().into_owned();
    let mut d = ensure_daemon().await?;
    let resp = post_open(&d, &path, !no_browser).await?;
    eprintln!("annotator: review round {} at {}", resp.round, resp.url);
    let round = resp.round;
    let mut retries = 0;
    let result = loop {
        match wait(&d, &resp.session_id, round).await {
            Ok(r) => break r,
            Err(WaitErr::Fatal(e)) => return Err(e),
            Err(WaitErr::Transport(e)) => {
                retries += 1;
                if retries > 20 {
                    return Err(e.context("lost connection to daemon"));
                }
                tokio::time::sleep(Duration::from_millis(200 * retries.min(5))).await;
                let Ok(nd) = ensure_daemon().await else {
                    continue;
                };
                d = nd;
                // Re-register so the (possibly new) daemon knows the session.
                match post_open(&d, &path, false).await {
                    Ok(r) => {
                        if r.round != round {
                            eprintln!("annotator: daemon restarted; now at round {}", r.round);
                        }
                        eprintln!("annotator: reconnected, review at {}", r.url);
                        retries = 0;
                    }
                    Err(_) => continue,
                }
            }
        }
    };
    match result {
        WaitResult::Done { ref summary, .. } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!("{}", crate::summary::to_text(summary));
            }
            Ok(0)
        }
        WaitResult::Abandoned { round } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                eprintln!("annotator: review of {path} abandoned (round {round})");
            }
            Ok(2)
        }
    }
}

fn rt() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

/// Returns the process exit code: 0 Done, 2 Abandoned, 1 error.
pub fn open(file: &Path, json: bool, no_browser: bool) -> anyhow::Result<i32> {
    rt()?.block_on(open_async(file, json, no_browser))
}

pub fn status() -> anyhow::Result<()> {
    rt()?.block_on(async {
        let Some(d) = find_daemon().await else {
            println!("daemon not running");
            return Ok(());
        };
        let s = d.status().await?;
        println!(
            "daemon pid {} on http://127.0.0.1:{} (up {}s)",
            s.pid, s.port, s.uptime_secs
        );
        if s.sessions.is_empty() {
            println!("no sessions");
        }
        for x in s.sessions {
            println!(
                "  {}  round {} {:<9}  tabs {}  waiters {}  {}",
                x.id, x.round, x.state, x.clients, x.waiters, x.path
            );
        }
        Ok(())
    })
}

pub fn stop() -> anyhow::Result<()> {
    rt()?.block_on(async {
        let Some(d) = find_daemon().await else {
            println!("daemon not running");
            return Ok(());
        };
        d.auth(d.http.post(d.url("/api/stop")))
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .error_for_status()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while pid_alive(d.info.pid) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        println!("daemon stopped");
        Ok(())
    })
}
