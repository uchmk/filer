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
use tsumugi_match::Matcher;

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
    matcher: Matcher,
    query: &str,
    via: SearchVia,
    show_hidden: bool,
    limit: usize,
    wake: impl Fn() + Send + Sync + 'static,
) -> Handle {
    let (tx, rx) = crossbeam_channel::unbounded::<Msg>();
    let cancel = Arc::new(AtomicBool::new(false));

    let root = root.to_path_buf();
    let cancel_t = cancel.clone();
    std::thread::Builder::new()
        .name("search".into())
        .spawn(move || {
            let found = Arc::new(AtomicUsize::new(0));

            let walker = ignore::WalkBuilder::new(&root)
                .hidden(!show_hidden)
                .git_ignore(true)
                .git_global(false)
                .ignore(true)
                .follow_links(false)
                .threads(std::thread::available_parallelism().map(|n| n.get().min(6)).unwrap_or(4))
                .build_parallel();

            let tx_w = tx.clone();
            let wake = Arc::new(wake);
            let root = Arc::new(root);
            walker.run(|| {
                let tx = tx_w.clone();
                let cancel = cancel_t.clone();
                let found = found.clone();
                let matcher = matcher.clone();
                let wake = wake.clone();
                let root = root.clone();
                Box::new(move |res| {
                    if cancel.load(Ordering::Relaxed) || found.load(Ordering::Relaxed) >= limit {
                        return ignore::WalkState::Quit;
                    }
                    let Ok(entry) = res else { return ignore::WalkState::Continue };
                    let path = entry.path();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                    let hit = match via {
                        SearchVia::Name => {
                            let name = entry.file_name().to_string_lossy();
                            matcher.is_match(&name)
                        }
                        SearchVia::Content => {
                            if is_dir {
                                false
                            } else {
                                contains(path, &matcher)
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
            let _ = tx.send(Msg::Done { total, truncated: total >= limit });
            wake();
        })
        .expect("spawn search worker");

    Handle { rx, query: query.to_owned(), via, cancel }
}

fn contains(path: &Path, matcher: &Matcher) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut buf = Vec::new();
    if f.by_ref().take(MAX_CONTENT_BYTES as u64).read_to_end(&mut buf).is_err() {
        return false;
    }
    if buf.contains(&0) {
        return false; // binary
    }
    matcher.is_match(&String::from_utf8_lossy(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree made fresh under the temp dir, gone when the test is.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str, files: &[(&str, &[u8])]) -> Self {
            let root = std::env::temp_dir().join(format!("filer-search-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (rel, body) in files {
                let at = root.join(rel);
                std::fs::create_dir_all(at.parent().unwrap()).unwrap();
                std::fs::write(at, body).unwrap();
            }
            Self(root)
        }

        /// The names found, sorted, once the search says it is done.
        fn find(&self, query: &str, via: SearchVia) -> Vec<String> {
            let handle = spawn(&self.0, Matcher::new(query).unwrap(), query, via, true, 100, || {});
            let mut names = Vec::new();
            while let Ok(msg) = handle.rx.recv_timeout(std::time::Duration::from_secs(10)) {
                match msg {
                    Msg::Found(paths) => names.extend(paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned())),
                    Msg::Done { .. } => break,
                }
            }
            names.sort();
            names
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_name_search_takes_a_regular_expression() {
        let t = Tree::new("name", &[("a.log", b""), ("a.log.1", b""), ("b.txt", b""), ("sub/c.log", b"")]);
        assert_eq!(t.find(r"\.log$", SearchVia::Name), ["a.log", "c.log"]);
        assert_eq!(t.find("^b", SearchVia::Name), ["b.txt"]);
    }

    #[test]
    fn a_name_search_is_a_substring_not_letters_in_order() {
        let t = Tree::new("substr", &[("awake.txt", b""), ("a-w-a.txt", b"")]);
        assert_eq!(t.find("awa", SearchVia::Name), ["awake.txt"]);
    }

    #[test]
    fn a_content_search_takes_a_regular_expression() {
        let t = Tree::new("content", &[("one.txt", b"alpha 123\n"), ("two.txt", b"alpha beta\n"), ("sub/three.txt", b"nothing\n")]);
        assert_eq!(t.find(r"alpha \d+", SearchVia::Content), ["one.txt"]);
        assert_eq!(t.find("ALPHA", SearchVia::Content), Vec::<String>::new(), "a capital makes it case-sensitive");
        assert_eq!(t.find("alpha", SearchVia::Content), ["one.txt", "two.txt"]);
    }
}
