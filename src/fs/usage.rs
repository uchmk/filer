//! What is eating the space: the children of one directory, each with the total
//! of everything underneath it.
//!
//! The list already answers "how big is this file"; what it cannot answer is
//! "how big is this folder", because a directory's own length is the size of its
//! inode and `Entry::dir_size` is a count of children one level down. Sorting by
//! size therefore cannot find the folder that is full, which is the one question
//! worth asking about a disk.
//!
//! One child at a time, largest last to arrive is fine -- the view re-sorts as
//! results come in. Nothing here uses the `ignore` walker that search uses: its
//! defaults respect `.gitignore` and skip hidden files, and a folder does not
//! stop taking up room because git was told to overlook it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

/// Entries visited before the walk gives up, shared across all the children.
/// The same budget `ops::measure` uses, and for the same reason: a pathological
/// tree must not hold the worker for ever.
const BUDGET: usize = 200_000;
/// How often partial results are sent. A size walk produces far more events than
/// a name search, so this is the throttle `ops` reports progress through.
const TICK: Duration = Duration::from_millis(50);

/// A child and what is under it: bytes, the number of files counted, and
/// whether the walk got to the end of it. A child the budget ran out in, or
/// never reached, is a floor and not an answer (44.7, #109).
pub type Child = (PathBuf, u64, u64, bool);

pub enum Msg {
    Sized(Vec<Child>),
    Done {
        total: u64,
        /// Whether the walk ran out of budget, so the totals are floors rather
        /// than answers.
        capped: bool,
    },
}

pub struct Handle {
    pub rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
}

impl Handle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Dropping the handle stops the walk, so leaving the view is enough to end it.
impl Drop for Handle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn spawn(root: &Path, wake: impl Fn() + Send + 'static) -> Handle {
    let (tx, rx) = crossbeam_channel::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let walk_root = root.to_path_buf();
    std::thread::Builder::new()
        .name("usage".into())
        .spawn(move || {
            let mut budget = BUDGET;
            let mut seen = HashSet::new();
            let mut total = 0u64;
            let mut batch: Vec<Child> = Vec::new();
            let mut last = Instant::now();
            let children: Vec<PathBuf> = match std::fs::read_dir(&walk_root) {
                Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
                Err(_) => Vec::new(),
            };
            for child in children {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let (bytes, files, whole) = measure(&child, &mut budget, &mut seen, &stop);
                total += bytes;
                batch.push((child, bytes, files, whole));
                if last.elapsed() >= TICK {
                    let _ = tx.send(Msg::Sized(std::mem::take(&mut batch)));
                    wake();
                    last = Instant::now();
                }
            }
            if !batch.is_empty() {
                let _ = tx.send(Msg::Sized(batch));
            }
            let _ = tx.send(Msg::Done { total, capped: budget == 0 });
            wake();
        })
        .expect("spawn usage thread");
    Handle { rx, cancel }
}

/// Everything under `path`, as bytes, a file count, and whether all of it was
/// reached before the budget ran out.
///
/// An explicit stack rather than recursion, and `symlink_metadata` so a link to
/// a directory is one entry rather than a second copy of a tree -- or a loop.
/// A file with more than one hard link is counted once, under the first name
/// the walk meets, as `du` does: `seen` holds the ones met so far across the
/// whole walk, so two folders sharing a file do not both claim it. Unix only;
/// see [`link_key`].
fn measure(path: &Path, budget: &mut usize, seen: &mut HashSet<(u64, u64)>, stop: &AtomicBool) -> (u64, u64, bool) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        if *budget == 0 || stop.load(Ordering::Relaxed) {
            return (bytes, files, false);
        }
        *budget -= 1;
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        if md.is_dir() && !md.file_type().is_symlink() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.filter_map(|e| e.ok()).map(|e| e.path()));
            }
        } else {
            if link_key(&md).is_some_and(|k| !seen.insert(k)) {
                continue;
            }
            files += 1;
            bytes += md.len();
        }
    }
    (bytes, files, true)
}

/// The device and inode of a file other names also point at, or `None` for a
/// file with only the one name -- the common case, kept out of `seen`.
///
/// Windows has the same idea (a volume serial and a file index) but std does
/// not expose it on stable, and reading it means opening every file, which on
/// a walk of 200,000 entries is not a price worth paying for links NTFS users
/// rarely make. There a tree that uses them still reads high (README).
#[cfg(unix)]
fn link_key(md: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (md.nlink() > 1).then(|| (md.dev(), md.ino()))
}

#[cfg(not(unix))]
fn link_key(_md: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> PathBuf {
        let dir = crate::util::test_dir("usage");
        std::fs::create_dir_all(dir.join("big").join("deep")).unwrap();
        std::fs::create_dir_all(dir.join("small")).unwrap();
        std::fs::write(dir.join("big").join("a"), vec![b'x'; 300]).unwrap();
        std::fs::write(dir.join("big").join("deep").join("b"), vec![b'x'; 700]).unwrap();
        std::fs::write(dir.join("small").join("c"), vec![b'x'; 50]).unwrap();
        std::fs::write(dir.join("loose"), vec![b'x'; 10]).unwrap();
        dir
    }

    fn collect(root: &Path) -> (Vec<Child>, u64, bool) {
        let h = spawn(root, || {});
        let mut all = Vec::new();
        let (mut total, mut capped) = (0, false);
        while let Ok(msg) = h.rx.recv() {
            match msg {
                Msg::Sized(v) => all.extend(v),
                Msg::Done { total: t, capped: c } => {
                    total = t;
                    capped = c;
                    break;
                }
            }
        }
        (all, total, capped)
    }

    /// The whole point: a folder's total is everything underneath it, not the
    /// size of its own directory entry.
    #[test]
    fn a_folder_is_worth_what_is_under_it() {
        let dir = tree();
        let (all, total, capped) = collect(&dir);
        assert!(!capped);
        let by = |name: &str| all.iter().find(|(p, ..)| p.ends_with(name)).map_or(0, |t| t.1);
        assert_eq!(by("big"), 1000, "300 and 700, counted through the subfolder: {all:?}");
        assert_eq!(by("small"), 50);
        assert_eq!(by("loose"), 10, "a plain file is worth its own length");
        assert_eq!(total, 1060);
        // Three children of the root, and no grandchildren among them.
        assert_eq!(all.len(), 3, "{all:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_counts_come_back_too() {
        let dir = tree();
        let (all, ..) = collect(&dir);
        let files = |name: &str| all.iter().find(|(p, ..)| p.ends_with(name)).map(|t| t.2);
        assert_eq!(files("big"), Some(2));
        assert_eq!(files("small"), Some(1));
        assert_eq!(files("loose"), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 44.7: a child the budget ran out in says so, and so does one it never
    /// reached -- the second used to come back as a plain 0 B, which is what
    /// `C:\Windows` read at the root of a drive.
    #[test]
    fn a_walk_cut_short_says_so() {
        let dir = tree();
        let stop = AtomicBool::new(false);
        let mut budget = 2;
        let (_, _, whole) = measure(&dir.join("big"), &mut budget, &mut HashSet::new(), &stop);
        assert!(!whole, "four entries under `big`, two allowed");
        let (bytes, files, whole) = measure(&dir.join("small"), &mut budget, &mut HashSet::new(), &stop);
        assert_eq!((bytes, files, whole), (0, 0, false), "nothing left for `small`: unknown, not empty");

        let mut budget = 1000;
        assert!(measure(&dir.join("small"), &mut budget, &mut HashSet::new(), &stop).2, "with room, the walk ends");
        let (all, ..) = collect(&dir);
        assert!(all.iter().all(|t| t.3), "and a whole walk marks every child whole: {all:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file hard-linked into two folders is counted once, under the first
    /// folder the walk meets, and the total is the bytes on disk.
    #[cfg(unix)]
    #[test]
    fn a_hard_link_is_counted_once() {
        let dir = crate::util::test_dir("usage-links");
        std::fs::create_dir_all(dir.join("one")).unwrap();
        std::fs::create_dir_all(dir.join("two")).unwrap();
        std::fs::write(dir.join("one").join("a"), vec![b'x'; 400]).unwrap();
        std::fs::hard_link(dir.join("one").join("a"), dir.join("two").join("a")).unwrap();
        std::fs::write(dir.join("two").join("b"), vec![b'x'; 5]).unwrap();
        let (all, total, _) = collect(&dir);
        assert_eq!(total, 405, "400 once, and 5: {all:?}");
        let by = |name: &str| all.iter().find(|(p, ..)| p.ends_with(name)).map_or(0, |t| t.1);
        assert!(matches!((by("one"), by("two")), (400, 5) | (0, 405)), "{all:?}");
    }

    /// An empty or unreadable directory answers rather than hanging.
    #[test]
    fn an_empty_directory_finishes_at_zero() {
        let dir = crate::util::test_dir("usage-empty");
        let (all, total, capped) = collect(&dir);
        assert!(all.is_empty());
        assert_eq!(total, 0);
        assert!(!capped);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
