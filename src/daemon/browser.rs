//! Bringing a review in front of the user: one browser tab per file.
//!
//! On macOS with a scriptable default browser (Chrome family, Safari) we look for the tab already
//! showing the session, focus it (raising its window) and close duplicates; only if there is none
//! do we open a new one. Elsewhere (or if scripting is denied) we fall back to opening the URL
//! when no tab is connected.

use std::process::{Command, Stdio};

/// Outcome of trying to focus an existing tab.
#[derive(Debug, PartialEq)]
enum Focus {
    Focused,
    NotFound,
    Unsupported,
}

/// Show the session to the user. `url` carries the token, `needle` identifies the session in tab
/// URLs (e.g. `:7717/s/abc123`), `connected` = a tab currently holds a WebSocket to it.
/// Blocking (may wait on an Automation permission prompt): call from a blocking thread.
pub fn present(url: &str, needle: &str, connected: bool) {
    match std::env::var("ANNOTATOR_BROWSER") {
        Ok(v) if v == "none" => return,
        Ok(v) if !v.is_empty() => {
            if !connected {
                launch(&v, url);
            }
            return;
        }
        _ => {}
    }
    if cfg!(target_os = "macos") {
        // A disconnected tab (e.g. after a daemon restart) holds a stale token cookie: reload it
        // through the token URL so it reconnects.
        match focus_tab(needle, url, !connected) {
            Focus::Focused => {}
            Focus::NotFound => launch("open", url),
            Focus::Unsupported if !connected => launch("open", url),
            Focus::Unsupported => {}
        }
    } else if !connected {
        launch("xdg-open", url);
    }
}

fn launch(cmd: &str, url: &str) {
    let r = Command::new(cmd)
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

/// JXA: find the default browser; if scriptable, focus the first tab whose URL contains the
/// needle (optionally reloading it to `url`), close other tabs for the same session.
const FOCUS_JXA: &str = r#"
function run(argv) {
  const needle = argv[0], url = argv[1], reload = argv[2] === '1';
  ObjC.import('AppKit');
  const appURL = $.NSWorkspace.sharedWorkspace.URLForApplicationToOpenURL($.NSURL.URLWithString('http://127.0.0.1/'));
  if (!appURL || appURL.isNil()) return 'unsupported';
  const id = ObjC.unwrap($.NSBundle.bundleWithURL(appURL).bundleIdentifier) || '';
  const chromium = ['com.google.chrome', 'com.google.chrome.beta', 'com.google.chrome.canary',
    'com.brave.browser', 'com.microsoft.edgemac', 'org.chromium.chromium', 'com.vivaldi.vivaldi'];
  const safari = id.toLowerCase() === 'com.apple.safari';
  if (!safari && !chromium.includes(id.toLowerCase())) return 'unsupported';
  const app = Application(id);
  if (!app.running()) return 'notfound';
  let found = null;
  const dupes = [];
  app.windows().forEach(w => {
    w.tabs().forEach((t, i) => {
      if ((t.url() || '').includes(needle)) {
        if (found) dupes.push(t); else found = { w, t, i };
      }
    });
  });
  if (!found) return 'notfound';
  dupes.forEach(t => t.close());
  if (safari) found.w.currentTab = found.t; else found.w.activeTabIndex = found.i + 1;
  if (reload) found.t.url = url;
  found.w.index = 1;
  app.activate();
  return 'focused';
}
"#;

fn focus_tab(needle: &str, url: &str, reload: bool) -> Focus {
    let out = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", FOCUS_JXA, needle, url])
        .arg(if reload { "1" } else { "0" })
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => match String::from_utf8_lossy(&o.stdout).trim() {
            "focused" => Focus::Focused,
            "notfound" => Focus::NotFound,
            _ => Focus::Unsupported,
        },
        Ok(o) => {
            // Typically: Automation permission denied for the browser.
            tracing::warn!(
                "tab focus failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            Focus::Unsupported
        }
        Err(e) => {
            tracing::warn!("osascript unavailable: {e}");
            Focus::Unsupported
        }
    }
}
