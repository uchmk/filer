//! Recursive search, streamed into the file list as results arrive.
//!
//! Uses the `ignore` walker so `.gitignore` is respected the way `fd` and
//! `rg` do it, and runs on its own threads so a search over a huge tree never
//! blocks the UI.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;

use crate::config::cmd::SearchVia;
use ito_match::Matcher;

pub enum Msg {
    Found(Vec<PathBuf>),
    /// `order` is the whole result, best match first, for the one search that
    /// ranks (`F`); the others leave it empty and keep the order they arrived in.
    /// `binary` counts the files a content search left unread because they are not text.
    Done { total: usize, truncated: bool, order: Vec<PathBuf>, binary: usize },
}

pub struct Handle {
    pub rx: Receiver<Msg>,
    pub query: String,
    pub via: SearchVia,
    /// Where it looks and whether it goes into hidden files (see [`scope`]).
    pub root: PathBuf,
    pub reach: Reach,
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

/// How much of a file is looked at before deciding it is not text. Text has no
/// NUL; a binary almost always shows one in its header.
const SNIFF_BYTES: usize = 8 * 1024;

/// Extensions that are never text, skipped without opening the file.
const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "o", "obj", "a", "lib", "pdb", "class", "jar", "pyc", "wasm", "bin", "dat", "iso", "img", "dmg", "msi", "png",
    "jpg", "jpeg", "gif", "bmp", "ico", "webp", "tif", "tiff", "psd", "mp3", "wav", "flac", "ogg", "m4a", "mp4", "mkv", "avi", "mov", "webm",
    "zip", "7z", "rar", "gz", "bz2", "xz", "zst", "tar", "tgz", "pdf", "doc", "xls", "ppt", "docx", "xlsx", "pptx", "ttf", "otf", "woff",
    "woff2", "sqlite", "db",
];

fn binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| BINARY_EXTENSIONS.iter().any(|b| b.eq_ignore_ascii_case(e)))
}

enum Look {
    Hit,
    Miss,
    Binary,
}

pub fn spawn(
    root: &Path,
    matcher: Matcher,
    query: &str,
    via: SearchVia,
    reach: Reach,
    limit: usize,
    wake: impl Fn() + Send + Sync + 'static,
) -> Handle {
    let (tx, rx) = crossbeam_channel::unbounded::<Msg>();
    let cancel = Arc::new(AtomicBool::new(false));

    let root = root.to_path_buf();
    let root_for_handle = root.clone();
    let reach_for_handle = reach.clone();
    let reach = Arc::new(reach);
    let cancel_t = cancel.clone();
    std::thread::Builder::new()
        .name("search".into())
        .spawn(move || {
            let found = Arc::new(AtomicUsize::new(0));
            let ranked: Arc<Mutex<Vec<(i32, PathBuf)>>> = Arc::default();
            let binary = Arc::new(AtomicUsize::new(0));

            let walker = ignore::WalkBuilder::new(&root)
                .hidden(!reach.hidden)
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
                let ranked = ranked.clone();
                let binary = binary.clone();
                let matcher = matcher.clone();
                let wake = wake.clone();
                let root = root.clone();
                let reach = reach.clone();
                Box::new(move |res| {
                    if cancel.load(Ordering::Relaxed) || found.load(Ordering::Relaxed) >= limit {
                        return ignore::WalkState::Quit;
                    }
                    let Ok(entry) = res else { return ignore::WalkState::Continue };
                    let path = entry.path();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                    let mut score = None;
                    let hit = reach.admits(path, is_dir) && match via {
                        SearchVia::Name => {
                            let name = entry.file_name().to_string_lossy();
                            matcher.is_match(&name)
                        }
                        SearchVia::Fuzzy => {
                            // The path below the root, so a directory name counts
                            // as it does when the letters are typed from `src/`.
                            // Always `/`, so a query typed with one matches on Windows.
                            let rel = path.strip_prefix(root.as_path()).unwrap_or(path).to_string_lossy().replace('\\', "/");
                            score = matcher.score(&rel);
                            score.is_some()
                        }
                        SearchVia::Content => {
                            if is_dir {
                                false
                            } else {
                                match look(path, &matcher) {
                                    Look::Hit => true,
                                    Look::Miss => false,
                                    Look::Binary => {
                                        binary.fetch_add(1, Ordering::Relaxed);
                                        false
                                    }
                                }
                            }
                        }
                    };
                    if hit && path != root.as_path() {
                        found.fetch_add(1, Ordering::Relaxed);
                        if let Some(score) = score {
                            ranked.lock().unwrap().push((score, path.to_path_buf()));
                        }
                        if tx.send(Msg::Found(vec![path.to_path_buf()])).is_err() {
                            return ignore::WalkState::Quit;
                        }
                        wake();
                    }
                    ignore::WalkState::Continue
                })
            });

            let total = found.load(Ordering::Relaxed);
            let mut ranked = std::mem::take(&mut *ranked.lock().unwrap());
            // Best first; the path breaks a tie so the order does not depend on
            // which thread got there first.
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            let order = ranked.into_iter().map(|(_, p)| p).collect();
            let binary = binary.load(Ordering::Relaxed);
            let _ = tx.send(Msg::Done { total, truncated: total >= limit, order, binary });
            wake();
        })
        .expect("spawn search worker");

    Handle { rx, query: query.to_owned(), via, root: root_for_handle, reach: reach_for_handle, cancel }
}

fn look(path: &Path, matcher: &Matcher) -> Look {
    if binary_extension(path) {
        return Look::Binary;
    }
    let Ok(mut f) = std::fs::File::open(path) else { return Look::Miss };
    let mut buf = Vec::new();
    // The head first: a binary is turned away after 8 KB, not after 1 MB.
    if f.by_ref().take(SNIFF_BYTES as u64).read_to_end(&mut buf).is_err() {
        return Look::Miss;
    }
    if buf.contains(&0) {
        return Look::Binary;
    }
    if f.by_ref().take((MAX_CONTENT_BYTES - SNIFF_BYTES) as u64).read_to_end(&mut buf).is_err() {
        return Look::Miss;
    }
    if matcher.is_match(&String::from_utf8_lossy(&buf)) {
        Look::Hit
    } else {
        Look::Miss
    }
}

/// What a search reaches into besides the folder: hidden files, and, when the
/// query carried `ext:`, only the files with one of those extensions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reach {
    pub hidden: bool,
    /// Lower case, without the dot. Empty means every kind of file.
    pub exts: Vec<String>,
}

impl Reach {
    #[cfg(test)]
    fn all() -> Self {
        Self { hidden: true, exts: Vec::new() }
    }

    /// Whether `path` is the kind of thing the search is asked for: with an
    /// extension filter on, only files that have one of them.
    fn admits(&self, path: &Path, is_dir: bool) -> bool {
        if self.exts.is_empty() {
            return true;
        }
        !is_dir && path.extension().and_then(|e| e.to_str()).is_some_and(|e| self.exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
    }
}

/// Takes the `ext:log` / `ext:rs,toml` words out of a query: what is left is
/// what to look for, and the extensions are what to look in. A leading dot
/// (`ext:.log`) is allowed; a bare `ext:` stays part of the query.
pub fn split_ext(query: &str) -> (String, Vec<String>) {
    let (mut rest, mut exts) = (Vec::new(), Vec::new());
    for word in query.split_whitespace() {
        match word.strip_prefix("ext:") {
            Some(list) if !list.trim_matches([',', '.']).is_empty() => {
                exts.extend(list.split(',').map(|e| e.trim_start_matches('.').to_lowercase()).filter(|e| !e.is_empty()));
            }
            _ => rest.push(word),
        }
    }
    (rest.join(" "), exts)
}

/// What a search covers, so a miss (or a short list) is not a mystery: the
/// folder it started from and what it leaves out. Hidden files and the kinds
/// of file are a choice; `.gitignore` and `.ignore` are always honoured.
pub fn scope(root: &Path, reach: &Reach) -> String {
    let hidden = if reach.hidden { "hidden files included" } else { "hidden files skipped" };
    let only = if reach.exts.is_empty() {
        String::new()
    } else {
        format!(", only {}", reach.exts.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(" "))
    };
    format!("in {} ({hidden}, .gitignore honoured{only})", root.display())
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
            let mut names = self.run(query, via).0;
            names.sort();
            names
        }

        /// The names as they arrived, and the ranking the search ended with.
        fn run(&self, query: &str, via: SearchVia) -> (Vec<String>, Vec<String>) {
            let matcher = match via {
                SearchVia::Fuzzy => Matcher::fuzzy(query),
                _ => Matcher::new(query).unwrap(),
            };
            let handle = spawn(&self.0, matcher, query, via, Reach::all(), 100, || {});
            let name = |p: &PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
            let (mut names, mut order) = (Vec::new(), Vec::new());
            while let Ok(msg) = handle.rx.recv_timeout(std::time::Duration::from_secs(10)) {
                match msg {
                    Msg::Found(paths) => names.extend(paths.iter().map(name)),
                    Msg::Done { order: o, .. } => {
                        order = o.iter().map(name).collect();
                        break;
                    }
                }
            }
            (names, order)
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

    #[test]
    fn a_fuzzy_search_takes_letters_in_order_across_the_path() {
        let t = Tree::new(
            "fuzzy",
            &[("src/main.rs", b""), ("src/lib.rs", b""), ("docs/domain-notes.md", b""), ("a-w-a.txt", b""), ("zzz.txt", b"")],
        );
        let (found, order) = t.run("srcmain", SearchVia::Fuzzy);
        assert_eq!(found, ["main.rs"], "the letters run across the directory and the name");
        assert_eq!(order, ["main.rs"]);

        let (_, order) = t.run("awa", SearchVia::Fuzzy);
        assert_eq!(order, ["a-w-a.txt"], "letters in order, not a substring");

        let (_, order) = t.run("rs", SearchVia::Fuzzy);
        assert_eq!(order.len(), 2, "both .rs files, and nothing else: {order:?}");
    }

    /// How many files the finished search says it did not read as text.
    fn binary_count(t: &Tree, query: &str) -> usize {
        let handle = spawn(&t.0, Matcher::new(query).unwrap(), query, SearchVia::Content, Reach::all(), 100, || {});
        while let Ok(msg) = handle.rx.recv_timeout(std::time::Duration::from_secs(10)) {
            if let Msg::Done { binary, .. } = msg {
                return binary;
            }
        }
        panic!("the search never finished");
    }

    #[test]
    fn a_content_search_leaves_binaries_out_and_counts_them() {
        let mut nul = b"needle ".to_vec();
        nul.push(0);
        let t = Tree::new(
            "binary",
            &[("a.txt", b"needle\n"), ("tool.exe", b"needle"), ("data.bin", &nul), ("tail.txt", b"needle"), ("Pic.PNG", b"needle")],
        );
        assert_eq!(t.find("needle", SearchVia::Content), ["a.txt", "tail.txt"]);
        assert_eq!(binary_count(&t, "needle"), 3, "the .exe and .PNG by extension, data.bin by its NUL");
        assert_eq!(t.find("exe", SearchVia::Name), ["tool.exe"], "a name search still finds binaries");
    }

    #[test]
    fn a_text_file_longer_than_the_sniffed_head_is_still_searched() {
        let mut body = vec![b'x'; SNIFF_BYTES + 10];
        body.extend_from_slice(b" needle");
        let t = Tree::new("longtext", &[("long.txt", &body)]);
        assert_eq!(t.find("needle", SearchVia::Content), ["long.txt"]);
        assert_eq!(binary_count(&t, "needle"), 0);
    }

    #[test]
    fn ext_words_come_out_of_the_query() {
        let split = |q: &str| split_ext(q);
        assert_eq!(split("error ext:log"), ("error".into(), vec!["log".into()]));
        assert_eq!(split("ext:Rs,.toml main"), ("main".into(), vec!["rs".into(), "toml".into()]));
        assert_eq!(split("ext:log"), (String::new(), vec!["log".into()]));
        assert_eq!(split("ext: a"), ("ext: a".into(), vec![]), "a bare ext: is just text");
        assert_eq!(split("a b"), ("a b".into(), vec![]));
    }

    #[test]
    fn an_extension_narrows_a_name_and_a_content_search() {
        let t = Tree::new("ext", &[("a.log", b"needle"), ("b.txt", b"needle"), ("sub.log/c.txt", b"needle"), ("d.LOG", b"x")]);
        let run = |via: SearchVia, query: &str| {
            let reach = Reach { hidden: true, exts: vec!["log".into()] };
            let handle = spawn(&t.0, Matcher::new(query).unwrap(), query, via, reach, 100, || {});
            let mut names = Vec::new();
            while let Ok(msg) = handle.rx.recv_timeout(std::time::Duration::from_secs(10)) {
                match msg {
                    Msg::Found(paths) => names.extend(paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned())),
                    Msg::Done { .. } => break,
                }
            }
            names.sort();
            names
        };
        assert_eq!(run(SearchVia::Name, ""), ["a.log", "d.LOG"], "every .log file, and not the folder called sub.log");
        assert_eq!(run(SearchVia::Content, "needle"), ["a.log"], "and a content search reads only those");
    }

    #[test]
    fn the_scope_names_the_extensions() {
        let s = scope(Path::new("/w"), &Reach { hidden: false, exts: vec!["log".into(), "txt".into()] });
        assert!(s.contains("only .log .txt"), "{s}");
    }

    #[test]
    fn only_a_fuzzy_search_ranks() {
        let t = Tree::new("rank", &[("a.txt", b"x"), ("b.txt", b"x")]);
        assert!(t.run("txt", SearchVia::Name).1.is_empty());
        assert_eq!(t.run("txt", SearchVia::Fuzzy).1.len(), 2);
    }

    #[test]
    fn the_scope_names_the_folder_and_what_is_left_out() {
        let s = scope(Path::new("/work/proj"), &Reach::default());
        assert!(s.contains("/work/proj") && s.contains("hidden files skipped") && s.contains(".gitignore"), "{s}");
        assert!(scope(Path::new("/w"), &Reach::all()).contains("hidden files included"));
    }
}
