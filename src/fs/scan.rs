//! Background directory scanning.
//!
//! Every read of the filesystem happens here, on a small pool of worker
//! threads, so the UI thread never blocks on IO. Results carry the request id
//! they answer; the caller drops anything it no longer cares about.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use crossbeam_channel::{Receiver, Sender};

use super::entry::Entry;
use super::sort::SortSpec;

#[derive(Debug)]
pub enum Task {
    /// List a directory.
    Scan { id: u64, path: PathBuf, sort: SortSpec },
    /// Count the direct children of each of these directories (for `linemode size`).
    Count { paths: Vec<PathBuf> },
}

#[derive(Debug)]
pub enum ScanResult {
    Listed { id: u64, path: PathBuf, entries: Vec<Entry> },
    Failed { id: u64, path: PathBuf, error: String },
    Counted { counts: Vec<(PathBuf, u64)> },
}

pub struct Scanner {
    hi: Sender<Task>,
    lo: Sender<Task>,
    pub rx: Receiver<ScanResult>,
    next_id: AtomicU64,
}

impl Scanner {
    pub fn new(threads: usize, wake: impl Fn() + Send + Clone + 'static) -> Self {
        let (hi_tx, hi_rx) = crossbeam_channel::unbounded::<Task>();
        let (lo_tx, lo_rx) = crossbeam_channel::unbounded::<Task>();
        let (res_tx, res_rx) = crossbeam_channel::unbounded::<ScanResult>();

        for i in 0..threads.max(1) {
            let hi_rx = hi_rx.clone();
            let lo_rx = lo_rx.clone();
            let res_tx = res_tx.clone();
            let wake = wake.clone();
            std::thread::Builder::new()
                .name(format!("scan-{i}"))
                .spawn(move || {
                    let mut sel = crossbeam_channel::Select::new();
                    let hi_i = sel.recv(&hi_rx);
                    let lo_i = sel.recv(&lo_rx);
                    loop {
                        // Always drain the high-priority queue first: the folder
                        // the user is looking at must never wait behind a preview.
                        let task = match hi_rx.try_recv() {
                            Ok(t) => Some(t),
                            Err(_) => {
                                let op = sel.select();
                                let idx = op.index();
                                if idx == hi_i {
                                    op.recv(&hi_rx).ok()
                                } else if idx == lo_i {
                                    op.recv(&lo_rx).ok()
                                } else {
                                    None
                                }
                            }
                        };
                        let Some(task) = task else { return };
                        if let Some(res) = run(task) {
                            if res_tx.send(res).is_err() {
                                return;
                            }
                            wake();
                        }
                    }
                })
                .expect("spawn scan worker");
        }

        Self { hi: hi_tx, lo: lo_tx, rx: res_rx, next_id: AtomicU64::new(1) }
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, AtomicOrdering::Relaxed)
    }

    /// Foreground scan: the folder being displayed.
    pub fn scan(&self, path: PathBuf, sort: SortSpec) -> u64 {
        let id = self.next_id();
        let _ = self.hi.send(Task::Scan { id, path, sort });
        id
    }

    /// Background scan: previews, prefetch, parents.
    pub fn scan_low(&self, path: PathBuf, sort: SortSpec) -> u64 {
        let id = self.next_id();
        let _ = self.lo.send(Task::Scan { id, path, sort });
        id
    }

    pub fn count(&self, paths: Vec<PathBuf>) -> u64 {
        let id = self.next_id();
        let _ = self.lo.send(Task::Count { paths });
        id
    }
}

/// The OS's words for a failed listing, and for a `\\host` that nobody
/// answered (1203 no network provider took the name, 53 path not found, 67
/// name not found) what that usually means, in English whatever the system's
/// language is (#233).
pub(crate) fn scan_error_text(path: &std::path::Path, e: &std::io::Error) -> String {
    let said = e.to_string();
    let unc = path.to_string_lossy().starts_with("\\\\");
    match (unc, e.raw_os_error()) {
        (true, Some(1203 | 53 | 67)) => format!("{said} — no host by that name answered (a typo, or the machine is off)"),
        _ => said,
    }
}

fn run(task: Task) -> Option<ScanResult> {
    match task {
        Task::Scan { id, path, sort } => {
            match list_dir(&path, sort) {
                Ok(entries) => Some(ScanResult::Listed { id, path, entries }),
                Err(e) => {
                    let error = scan_error_text(&path, &e);
                    Some(ScanResult::Failed { id, path, error })
                }
            }
        }
        Task::Count { paths } => {
            let counts = paths
                .into_iter()
                .filter_map(|p| count_children(&p).map(|n| (p, n)))
                .collect();
            Some(ScanResult::Counted { counts })
        }
    }
}

pub fn list_dir(path: &std::path::Path, sort: SortSpec) -> std::io::Result<Vec<Entry>> {
    // `\\host` is a server rather than a directory: the shares under it are
    // not on any disk here, and `read_dir` fails on one no matter how well the
    // server is answering. Ask the network for those instead.
    if crate::util::host_only_unc(path) {
        let mut out = super::shares::list(path)?;
        sort.apply(&mut out);
        return Ok(out);
    }
    let rd = std::fs::read_dir(path)?;
    // Most directories are small; the reserve keeps big ones from re-allocating much.
    let mut out: Vec<Entry> = Vec::with_capacity(64);
    for de in rd {
        match de {
            Ok(de) => out.push(Entry::from_dir_entry(&de)),
            Err(_) => continue, // a racing delete shouldn't abort the whole listing
        }
    }
    sort.apply(&mut out);
    Ok(out)
}

fn count_children(path: &std::path::Path) -> Option<u64> {
    let rd = std::fs::read_dir(path).ok()?;
    Some(rd.take(100_000).filter(|e| e.is_ok()).count() as u64)
}

#[cfg(test)]
mod error_text_tests {
    use super::scan_error_text;
    use std::io::Error;
    use std::path::Path;

    /// #233: a share's host that did not answer says so; nothing else changes.
    #[test]
    fn an_unanswered_host_is_named() {
        let host = Path::new(r"\\nohost\share");
        for code in [1203, 53, 67] {
            let said = scan_error_text(host, &Error::from_raw_os_error(code));
            assert!(said.ends_with("no host by that name answered (a typo, or the machine is off)"), "{code}: {said}");
        }
        let other = scan_error_text(host, &Error::from_raw_os_error(5));
        assert!(!other.contains("no host"), "{other}");
        let local = scan_error_text(Path::new("/tmp/x"), &Error::from_raw_os_error(53));
        assert!(!local.contains("no host"), "a local path is not a host: {local}");
    }
}
