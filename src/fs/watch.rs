//! Directory watching. Only the handful of directories currently on screen are
//! watched, and changes are reported as "this directory is dirty" — the app
//! debounces and rescans.
//!
//! Registering a directory opens a handle to it (`ReadDirectoryChangesW` on
//! Windows), which can hang on a dead network share, so the `notify` watcher
//! lives on its own thread. The UI thread only ever posts the set of
//! directories it wants; the thread applies the newest set it has been given.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};
use notify::{RecursiveMode, Watcher as _};

pub struct Watcher {
    /// Desired set of watched directories, handed to the watcher thread.
    tx: Option<Sender<HashSet<PathBuf>>>,
    /// What was posted last, so the per-frame call costs nothing.
    sent: HashSet<PathBuf>,
    pub rx: Receiver<PathBuf>,
}

impl Watcher {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded::<PathBuf>();
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<HashSet<PathBuf>>();
        let spawned = std::thread::Builder::new()
            .name("watch".into())
            .spawn(move || run(ev_tx, cmd_rx, wake))
            .is_ok();
        Self { tx: spawned.then_some(cmd_tx), sent: HashSet::new(), rx: ev_rx }
    }

    /// Ask for the watched set to become exactly `dirs`.
    pub fn sync<'a>(&mut self, dirs: impl IntoIterator<Item = &'a Path>) {
        let Some(tx) = self.tx.as_ref() else { return };
        let wanted: HashSet<PathBuf> = dirs.into_iter().map(Path::to_path_buf).collect();
        if wanted == self.sent {
            return;
        }
        if tx.send(wanted.clone()).is_ok() {
            self.sent = wanted;
        }
    }
}

/// The watcher thread: owns the `notify` watcher and every call that touches a
/// directory handle.
fn run(ev_tx: Sender<PathBuf>, cmd_rx: Receiver<HashSet<PathBuf>>, wake: impl Fn() + Send + 'static) {
    let Ok(mut inner) = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, notify::EventKind::Access(_)) {
            return;
        }
        let mut sent = false;
        for p in ev.paths {
            let dir = if p.is_dir() { p.clone() } else { p.parent().map(Path::to_path_buf).unwrap_or(p) };
            if ev_tx.send(dir).is_ok() {
                sent = true;
            }
        }
        if sent {
            wake();
        }
    }) else {
        return;
    };

    let mut watched: HashSet<PathBuf> = HashSet::new();
    while let Ok(mut wanted) = cmd_rx.recv() {
        // A slow `watch()` lets requests pile up; only the newest one matters.
        while let Ok(newer) = cmd_rx.try_recv() {
            wanted = newer;
        }
        let (drop, add) = plan(&watched, &wanted);
        for gone in drop {
            let _ = inner.unwatch(&gone);
            watched.remove(&gone);
        }
        for path in add {
            if inner.watch(&path, RecursiveMode::NonRecursive).is_ok() {
                watched.insert(path);
            }
        }
    }
}

/// Directories to stop watching, then ones to start watching.
fn plan(watched: &HashSet<PathBuf>, wanted: &HashSet<PathBuf>) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let drop = watched.difference(wanted).cloned().collect();
    let add = wanted.difference(watched).cloned().collect();
    (drop, add)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set<const N: usize>(paths: [&str; N]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn plan_only_moves_the_difference() {
        let (drop, add) = plan(&set(["a", "b"]), &set(["b", "c"]));
        assert_eq!(drop, vec![PathBuf::from("a")]);
        assert_eq!(add, vec![PathBuf::from("c")]);
    }

    #[test]
    fn plan_is_empty_when_nothing_changed() {
        let (drop, add) = plan(&set(["a", "b"]), &set(["b", "a"]));
        assert!(drop.is_empty() && add.is_empty());
    }

    #[test]
    fn sync_posts_only_when_the_set_changes() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<HashSet<PathBuf>>();
        let (_ev_tx, ev_rx) = crossbeam_channel::unbounded::<PathBuf>();
        let mut w = Watcher { tx: Some(cmd_tx), sent: HashSet::new(), rx: ev_rx };

        let (a, b) = (Path::new("a"), Path::new("b"));
        w.sync([a, b]);
        assert_eq!(cmd_rx.try_recv().unwrap(), set(["a", "b"]));

        // Same set, different order: the frame after a redraw stays silent.
        w.sync([b, a]);
        assert!(cmd_rx.try_recv().is_err());

        w.sync([a]);
        assert_eq!(cmd_rx.try_recv().unwrap(), set(["a"]));
    }

    #[test]
    fn sync_keeps_trying_after_the_thread_is_gone() {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<HashSet<PathBuf>>();
        let (_ev_tx, ev_rx) = crossbeam_channel::unbounded::<PathBuf>();
        let mut w = Watcher { tx: Some(cmd_tx), sent: HashSet::new(), rx: ev_rx };
        drop(cmd_rx);

        w.sync([Path::new("a")]);
        assert!(w.sent.is_empty());
    }
}
