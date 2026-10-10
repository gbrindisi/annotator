//! File watching: watch each session file's parent directory (so atomic renames are seen),
//! debounce events per path (~200ms) and emit the changed paths.

use notify::{RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

const DEBOUNCE: Duration = Duration::from_millis(200);

pub struct FileWatcher {
    watcher: Mutex<notify::RecommendedWatcher>,
    dirs: Mutex<HashSet<PathBuf>>,
}

impl FileWatcher {
    pub fn new() -> anyhow::Result<(FileWatcher, mpsc::UnboundedReceiver<PathBuf>)> {
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<PathBuf>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && !ev.kind.is_access()
            {
                for p in ev.paths {
                    let _ = raw_tx.send(p);
                }
            }
        })?;
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
            loop {
                let next = pending.values().min().copied();
                tokio::select! {
                    p = raw_rx.recv() => match p {
                        Some(p) => { pending.insert(p, Instant::now() + DEBOUNCE); }
                        None => return,
                    },
                    _ = async { tokio::time::sleep_until(next.unwrap()).await }, if next.is_some() => {
                        let now = Instant::now();
                        let due: Vec<PathBuf> =
                            pending.iter().filter(|(_, t)| **t <= now).map(|(p, _)| p.clone()).collect();
                        for p in due {
                            pending.remove(&p);
                            if out_tx.send(p).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        Ok((
            FileWatcher {
                watcher: Mutex::new(watcher),
                dirs: Mutex::new(HashSet::new()),
            },
            out_rx,
        ))
    }

    pub fn watch(&self, file: &Path) -> notify::Result<()> {
        let dir = file.parent().unwrap_or(Path::new("/")).to_path_buf();
        let mut dirs = self.dirs.lock().unwrap();
        if dirs.contains(&dir) {
            return Ok(());
        }
        self.watcher
            .lock()
            .unwrap()
            .watch(&dir, RecursiveMode::NonRecursive)?;
        dirs.insert(dir);
        Ok(())
    }
}
