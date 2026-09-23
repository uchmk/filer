//! Directory watching. Only the handful of directories currently on screen are
//! watched, and changes are reported as "this directory is dirty" — the app
//! debounces and rescans.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossbeam_channel::Receiver;
use notify::{RecursiveMode, Watcher as _};

pub struct Watcher {
    inner: Option<notify::RecommendedWatcher>,
    watched: HashSet<PathBuf>,
    pub rx: Receiver<PathBuf>,
}

impl Watcher {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<PathBuf>();
        let inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if matches!(ev.kind, notify::EventKind::Access(_)) {
                return;
            }
            let mut sent = false;
            for p in ev.paths {
                let dir = if p.is_dir() { p.clone() } else { p.parent().map(Path::to_path_buf).unwrap_or(p) };
                if tx.send(dir).is_ok() {
                    sent = true;
                }
            }
            if sent {
                wake();
            }
        })
        .ok();
        Self { inner, watched: HashSet::new(), rx }
    }

    /// Make the watched set exactly `dirs`.
    pub fn sync<'a>(&mut self, dirs: impl IntoIterator<Item = &'a Path>) {
        let Some(w) = self.inner.as_mut() else { return };
        let wanted: HashSet<PathBuf> = dirs.into_iter().map(Path::to_path_buf).collect();
        for gone in self.watched.difference(&wanted).cloned().collect::<Vec<_>>() {
            let _ = w.unwatch(&gone);
            self.watched.remove(&gone);
        }
        for added in wanted.difference(&self.watched).cloned().collect::<Vec<_>>() {
            if w.watch(&added, RecursiveMode::NonRecursive).is_ok() {
                self.watched.insert(added);
            }
        }
    }
}
