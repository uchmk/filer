//! Recursive search, streamed into the file list as results arrive.
//!
//! Uses the `ignore` walker so `.gitignore` is respected the way `fd` and
//! `rg` do it, and runs on its own threads so a search over a huge tree never
//! blocks the UI.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crossbeam_channel::Receiver;

use crate::config::cmd::SearchVia;
use crate::core::fuzzy;

pub enum Msg {
    Found(Vec<PathBuf>),
    Done { total: usize, truncated: bool },
}

pub struct Handle {
    pub rx: Receiver<Msg>,
    pub query: String,
    pub via: SearchVia,
    cancel: Arc<AtomicBool>,
}

impl Handle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.cancel();
    }
}

const MAX_CONTENT_BYTES: usize = 1024 * 1024;

pub fn spawn(
    root: &Path,
    query: &str,
    via: SearchVia,
    show_hidden: bool,
    limit: usize,
    wake: impl Fn() + Send + Sync + 'static,
) -> Handle {
    let (tx, rx) = crossbeam_channel::unbounded::<Msg>();
    let cancel = Arc::new(AtomicBool::new(false));

    let root = root.to_path_buf();
    let query_s = query.to_owned();
    let cancel_t = cancel.clone();
    std::thread::Builder::new()
        .name("search".into())
        .spawn(move || {
            let found = Arc::new(AtomicUsize::new(0));
            let case_sensitive = fuzzy::is_case_sensitive(&query_s, true, false);
            let needle = if case_sensitive {
                query_s.clone()
            } else {
                query_s.to_lowercase()
            };

            let walker = ignore::WalkBuilder::new(&root)
                .hidden(!show_hidden)
                .git_ignore(true)
                .git_global(false)
                .ignore(true)
                .follow_links(false)
                .threads(
                    std::thread::available_parallelism()
                        .map(|n| n.get().min(6))
                        .unwrap_or(4),
                )
                .build_parallel();

            let tx_w = tx.clone();
            let wake = Arc::new(wake);
            let root = Arc::new(root);
            walker.run(|| {
                let tx = tx_w.clone();
                let cancel = cancel_t.clone();
                let found = found.clone();
                let needle = needle.clone();
                let wake = wake.clone();
                let root = root.clone();
                Box::new(move |res| {
                    if cancel.load(Ordering::Relaxed) || found.load(Ordering::Relaxed) >= limit {
                        return ignore::WalkState::Quit;
                    }
                    let Ok(entry) = res else {
                        return ignore::WalkState::Continue;
                    };
                    let path = entry.path();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                    let hit = match via {
                        SearchVia::Name => {
                            let name = entry.file_name().to_string_lossy();
                            let hay = if case_sensitive {
                                name.to_string()
                            } else {
                                name.to_lowercase()
                            };
                            fuzzy::match_str(&needle, &hay, true).is_some()
                        }
                        SearchVia::Content => {
                            if is_dir {
                                false
                            } else {
                                contains(path, &needle, case_sensitive)
                            }
                        }
                    };
                    if hit && path != root.as_path() {
                        found.fetch_add(1, Ordering::Relaxed);
                        if tx.send(Msg::Found(vec![path.to_path_buf()])).is_err() {
                            return ignore::WalkState::Quit;
                        }
                        wake();
                    }
                    ignore::WalkState::Continue
                })
            });

            let total = found.load(Ordering::Relaxed);
            let _ = tx.send(Msg::Done {
                total,
                truncated: total >= limit,
            });
            wake();
        })
        .expect("spawn search worker");

    Handle {
        rx,
        query: query.to_owned(),
        via,
        cancel,
    }
}

fn contains(path: &Path, needle: &str, case_sensitive: bool) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = Vec::new();
    if f.by_ref()
        .take(MAX_CONTENT_BYTES as u64)
        .read_to_end(&mut buf)
        .is_err()
    {
        return false;
    }
    if buf.contains(&0) {
        return false; // binary
    }
    let text = String::from_utf8_lossy(&buf);
    if case_sensitive {
        text.contains(needle)
    } else {
        text.to_lowercase().contains(needle)
    }
}
