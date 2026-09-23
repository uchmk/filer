//! Background file operations with progress and conflict prompts.
//!
//! One worker thread runs jobs in order, the way yazi queues tasks: two
//! concurrent copies on the same disk are slower than one, and sequential
//! progress is easier to reason about.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpKind {
    Copy,
    Move,
    Symlink { relative: bool },
    Hardlink,
    Trash,
    Delete,
}

impl OpKind {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Move => "Move",
            Self::Symlink { .. } => "Link",
            Self::Hardlink => "Hardlink",
            Self::Trash => "Trash",
            Self::Delete => "Delete",
        }
    }
}

#[derive(Debug)]
pub struct OpRequest {
    pub id: u64,
    pub kind: OpKind,
    pub srcs: Vec<PathBuf>,
    pub dest_dir: PathBuf,
    /// Overwrite without asking.
    pub force: bool,
}

#[derive(Debug)]
pub enum OpEvent {
    Started { id: u64, files: u64, bytes: u64 },
    Progress { id: u64, files_done: u64, bytes_done: u64, current: String },
    /// The worker is blocked until the UI sends a [`Resolution`].
    Conflict { id: u64, src: PathBuf, dest: PathBuf, reply: Sender<Resolution> },
    Finished { id: u64, kind: OpKind, errors: Vec<String>, cancelled: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Overwrite,
    OverwriteAll,
    Skip,
    SkipAll,
    Rename(String),
    Cancel,
}

pub struct Runner {
    tx: Sender<OpRequest>,
    pub rx: Receiver<OpEvent>,
}

impl Runner {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, job_rx) = crossbeam_channel::unbounded::<OpRequest>();
        let (ev_tx, rx) = crossbeam_channel::unbounded::<OpEvent>();
        std::thread::Builder::new()
            .name("fs-ops".into())
            .spawn(move || {
                while let Ok(req) = job_rx.recv() {
                    let mut ctx = Ctx {
                        id: req.id,
                        ev: &ev_tx,
                        wake: &wake,
                        files_done: 0,
                        bytes_done: 0,
                        policy: if req.force { Policy::OverwriteAll } else { Policy::Ask },
                        errors: Vec::new(),
                        cancelled: false,
                        last_report: std::time::Instant::now(),
                    };
                    ctx.run(&req);
                    let _ = ev_tx.send(OpEvent::Finished {
                        id: req.id,
                        kind: req.kind,
                        errors: std::mem::take(&mut ctx.errors),
                        cancelled: ctx.cancelled,
                    });
                    wake();
                }
            })
            .expect("spawn fs-ops worker");
        Self { tx, rx }
    }

    pub fn submit(&self, req: OpRequest) {
        let _ = self.tx.send(req);
    }
}

#[derive(PartialEq, Eq)]
enum Policy {
    Ask,
    OverwriteAll,
    SkipAll,
}

struct Ctx<'a> {
    id: u64,
    ev: &'a Sender<OpEvent>,
    wake: &'a (dyn Fn() + Send),
    files_done: u64,
    bytes_done: u64,
    policy: Policy,
    errors: Vec<String>,
    cancelled: bool,
    last_report: std::time::Instant,
}

impl Ctx<'_> {
    fn run(&mut self, req: &OpRequest) {
        let (files, bytes) = match req.kind {
            OpKind::Trash | OpKind::Delete => (req.srcs.len() as u64, 0),
            OpKind::Symlink { .. } | OpKind::Hardlink => (req.srcs.len() as u64, 0),
            OpKind::Copy | OpKind::Move => measure(&req.srcs),
        };
        let _ = self.ev.send(OpEvent::Started { id: self.id, files, bytes });
        (self.wake)();

        match req.kind {
            OpKind::Trash => {
                if let Err(e) = trash::delete_all(&req.srcs) {
                    self.errors.push(format!("trash: {e}"));
                }
                self.files_done = req.srcs.len() as u64;
            }
            OpKind::Delete => {
                for p in &req.srcs {
                    if self.cancelled {
                        break;
                    }
                    self.report(&p.to_string_lossy());
                    let r = if p.is_dir() && !is_symlink(p) {
                        std::fs::remove_dir_all(p)
                    } else {
                        std::fs::remove_file(p)
                    };
                    if let Err(e) = r {
                        self.errors.push(format!("{}: {e}", short(p)));
                    }
                    self.files_done += 1;
                }
            }
            OpKind::Symlink { relative } => {
                for src in &req.srcs {
                    let dest = req.dest_dir.join(file_name(src));
                    let Some(dest) = self.resolve_dest(src, dest) else { continue };
                    let target = if relative {
                        relative_to(&req.dest_dir, src).unwrap_or_else(|| src.clone())
                    } else {
                        src.clone()
                    };
                    if let Err(e) = symlink(&target, &dest, src.is_dir()) {
                        self.errors.push(format!("{}: {e}", short(src)));
                    }
                    self.files_done += 1;
                }
            }
            OpKind::Hardlink => {
                for src in &req.srcs {
                    let dest = req.dest_dir.join(file_name(src));
                    let Some(dest) = self.resolve_dest(src, dest) else { continue };
                    if let Err(e) = std::fs::hard_link(src, &dest) {
                        self.errors.push(format!("{}: {e}", short(src)));
                    }
                    self.files_done += 1;
                }
            }
            OpKind::Copy | OpKind::Move => {
                let moving = req.kind == OpKind::Move;
                for src in &req.srcs {
                    if self.cancelled {
                        break;
                    }
                    let dest = req.dest_dir.join(file_name(src));
                    if same_path(src, &dest) {
                        // Copying onto itself: make a "foo copy" style sibling instead.
                        let dest = unique_name(&dest);
                        self.transfer(src, &dest, moving);
                        continue;
                    }
                    if is_inside(src, &dest) {
                        self.errors
                            .push(format!("{}: cannot copy into itself", short(src)));
                        continue;
                    }
                    let Some(dest) = self.resolve_dest(src, dest) else { continue };
                    self.transfer(src, &dest, moving);
                }
            }
        }
        self.report("");
    }

    /// Apply the conflict policy, asking the UI when needed.
    /// Returns `None` when the entry should be skipped.
    fn resolve_dest(&mut self, src: &Path, dest: PathBuf) -> Option<PathBuf> {
        if !exists(&dest) {
            return Some(dest);
        }
        match self.policy {
            Policy::OverwriteAll => return Some(dest),
            Policy::SkipAll => return None,
            Policy::Ask => {}
        }
        let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
        let _ = self.ev.send(OpEvent::Conflict {
            id: self.id,
            src: src.to_path_buf(),
            dest: dest.clone(),
            reply: reply_tx,
        });
        (self.wake)();
        match reply_rx.recv() {
            Ok(Resolution::Overwrite) => Some(dest),
            Ok(Resolution::OverwriteAll) => {
                self.policy = Policy::OverwriteAll;
                Some(dest)
            }
            Ok(Resolution::Skip) => None,
            Ok(Resolution::SkipAll) => {
                self.policy = Policy::SkipAll;
                None
            }
            Ok(Resolution::Rename(name)) => {
                Some(dest.parent().unwrap_or(Path::new(".")).join(name))
            }
            Ok(Resolution::Cancel) | Err(_) => {
                self.cancelled = true;
                None
            }
        }
    }

    fn transfer(&mut self, src: &Path, dest: &Path, moving: bool) {
        if moving {
            // A same-volume rename is instant; only fall back when it isn't possible.
            match std::fs::rename(src, dest) {
                Ok(()) => {
                    self.files_done += 1;
                    self.bytes_done += std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
                    self.report(&dest.to_string_lossy());
                    return;
                }
                Err(_) => { /* fall through to copy + delete */ }
            }
        }
        let is_dir = src.is_dir() && !is_symlink(src);
        if is_dir {
            if let Err(e) = std::fs::create_dir_all(dest) {
                self.errors.push(format!("{}: {e}", short(dest)));
                return;
            }
            let children = match std::fs::read_dir(src) {
                Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect::<Vec<_>>(),
                Err(e) => {
                    self.errors.push(format!("{}: {e}", short(src)));
                    return;
                }
            };
            for child in children {
                if self.cancelled {
                    return;
                }
                let target = dest.join(file_name(&child));
                let Some(target) = (if exists(&target) {
                    self.resolve_dest(&child, target)
                } else {
                    Some(target)
                }) else {
                    continue;
                };
                self.transfer(&child, &target, moving);
            }
            if moving {
                let _ = std::fs::remove_dir(src);
            }
        } else {
            match self.copy_file(src, dest) {
                Ok(()) => {
                    if moving {
                        if let Err(e) = std::fs::remove_file(src) {
                            self.errors.push(format!("{}: {e}", short(src)));
                        }
                    }
                }
                Err(e) => self.errors.push(format!("{}: {e}", short(src))),
            }
            self.files_done += 1;
            self.report(&src.to_string_lossy());
        }
    }

    fn copy_file(&mut self, src: &Path, dest: &Path) -> std::io::Result<()> {
        let len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        // Small files: let the OS do it in one shot. Large ones: chunk so the
        // progress bar keeps moving.
        if len <= 32 * 1024 * 1024 {
            std::fs::copy(src, dest)?;
            self.bytes_done += len;
            return Ok(());
        }
        let mut r = std::fs::File::open(src)?;
        let mut w = std::fs::File::create(dest)?;
        let mut buf = vec![0u8; 4 * 1024 * 1024];
        loop {
            if self.cancelled {
                return Err(std::io::Error::other("cancelled"));
            }
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            self.bytes_done += n as u64;
            self.report(&src.to_string_lossy());
        }
        w.flush()?;
        Ok(())
    }

    fn report(&mut self, current: &str) {
        // ~20 Hz is plenty for a progress bar and keeps the channel quiet.
        if self.last_report.elapsed().as_millis() < 50 && !current.is_empty() {
            return;
        }
        self.last_report = std::time::Instant::now();
        let _ = self.ev.send(OpEvent::Progress {
            id: self.id,
            files_done: self.files_done,
            bytes_done: self.bytes_done,
            current: current.to_owned(),
        });
        (self.wake)();
    }
}

fn measure(srcs: &[PathBuf]) -> (u64, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let mut stack: Vec<PathBuf> = srcs.to_vec();
    // Bound the walk so a huge tree can't stall the start of the job.
    let mut budget = 200_000;
    while let Some(p) = stack.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        if md.is_dir() && !md.file_type().is_symlink() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.filter_map(|e| e.ok()).map(|e| e.path()));
            }
        } else {
            files += 1;
            bytes += md.len();
        }
    }
    (files, bytes)
}

pub fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn is_symlink(p: &Path) -> bool {
    std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}

fn file_name(p: &Path) -> PathBuf {
    p.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("unnamed"))
}

fn short(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn same_path(a: &Path, b: &Path) -> bool {
    crate::util::normalize(a) == crate::util::normalize(b)
}

fn is_inside(parent: &Path, child: &Path) -> bool {
    let parent = crate::util::normalize(parent);
    let child = crate::util::normalize(child);
    child.starts_with(&parent) && child != parent
}

/// `report.txt` -> `report_1.txt`, skipping names already taken.
pub fn unique_name(dest: &Path) -> PathBuf {
    if !exists(dest) {
        return dest.to_path_buf();
    }
    let dir = dest.parent().unwrap_or(Path::new("."));
    let name = dest.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = crate::util::stem_and_ext(&name);
    for i in 1..10_000 {
        let candidate = dir.join(format!("{stem}_{i}{ext}"));
        if !exists(&candidate) {
            return candidate;
        }
    }
    dest.to_path_buf()
}

fn relative_to(from_dir: &Path, target: &Path) -> Option<PathBuf> {
    let from = crate::util::normalize(from_dir);
    let to = crate::util::normalize(target);
    let mut f = from.components().peekable();
    let mut t = to.components().peekable();
    while f.peek().is_some() && f.peek() == t.peek() {
        f.next();
        t.next();
    }
    let mut out = PathBuf::new();
    for _ in f {
        out.push("..");
    }
    for c in t {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path, dir: bool) -> std::io::Result<()> {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    if dir {
        symlink_dir(target, link)
    } else {
        symlink_file(target, link)
    }
}

#[cfg(not(windows))]
fn symlink(target: &Path, link: &Path, _dir: bool) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
