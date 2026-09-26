//! Background file operations with progress and conflict prompts.
//!
//! One worker thread runs jobs in order, the way yazi queues tasks: two
//! concurrent copies on the same disk are slower than one, and sequential
//! progress is easier to reason about.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crossbeam_channel::{Receiver, Sender};

use super::{archive, restore};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpKind {
    Copy,
    Move,
    Symlink { relative: bool },
    Hardlink,
    Trash,
    Delete,
    /// Put `srcs` back where they were before a [`OpKind::Trash`] job sent them
    /// away. This is what `u` runs to undo a delete.
    Restore,
    /// Unpack each source archive into a folder of its own under `dest_dir`.
    Extract,
    /// Pack the sources into the one archive `dest_file` names.
    Compress(archive::Format),
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
            Self::Restore => "Restore",
            Self::Extract => "Extract",
            Self::Compress(_) => "Compress",
        }
    }
}

#[derive(Debug)]
pub struct OpRequest {
    pub id: u64,
    pub kind: OpKind,
    pub srcs: Vec<PathBuf>,
    pub dest_dir: PathBuf,
    /// The archive a [`OpKind::Compress`] job writes. Unused by the rest.
    pub dest_file: Option<PathBuf>,
    /// Overwrite without asking.
    pub force: bool,
}

#[derive(Debug)]
pub enum OpEvent {
    Started { id: u64, files: u64, bytes: u64 },
    Progress { id: u64, files_done: u64, bytes_done: u64, current: String },
    /// The worker is blocked until the UI sends a [`Resolution`].
    Conflict { id: u64, src: PathBuf, dest: PathBuf, reply: Sender<Resolution> },
    /// The job has parked, or started moving again.
    Paused { id: u64, paused: bool },
    Finished {
        id: u64,
        kind: OpKind,
        errors: Vec<String>,
        cancelled: bool,
        /// For a move: what ended up where. The worker is the only one that
        /// knows, because a name already taken is resolved here — the file the
        /// caller asked to move to `x.pdf` can land as `x_1.pdf`, and an undo
        /// that went looking for `x.pdf` would find nothing.
        moved: Vec<(PathBuf, PathBuf)>,
    },
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

/// What the task panel can say to a job while it runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    Pause,
    Resume,
    Cancel,
}

/// The jobs waiting their turn. A queue rather than a channel, because the
/// task panel reorders it: a `VecDeque` can be looked into and rearranged,
/// and a channel cannot.
type Queue = Arc<(Mutex<VecDeque<OpRequest>>, Condvar)>;

pub struct Runner {
    queue: Queue,
    stop: Arc<AtomicBool>,
    ctl: Sender<(u64, Control)>,
    pub rx: Receiver<OpEvent>,
}

impl Runner {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (ev_tx, rx) = crossbeam_channel::unbounded::<OpEvent>();
        let (ctl, ctl_rx) = crossbeam_channel::unbounded::<(u64, Control)>();
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (q, s) = (queue.clone(), stop.clone());
        std::thread::Builder::new()
            .name("fs-ops".into())
            .spawn(move || {
                while let Some(req) = next_job(&q, &s) {
                    let mut ctx = Ctx {
                        id: req.id,
                        ev: &ev_tx,
                        ctl: &ctl_rx,
                        wake: &wake,
                        files_done: 0,
                        bytes_done: 0,
                        policy: if req.force { Policy::OverwriteAll } else { Policy::Ask },
                        errors: Vec::new(),
                        cancelled: false,
                        paused: false,
                        last_report: std::time::Instant::now(),
                        moved: Vec::new(),
                    };
                    ctx.run(&req);
                    let _ = ev_tx.send(OpEvent::Finished {
                        id: req.id,
                        kind: req.kind,
                        errors: std::mem::take(&mut ctx.errors),
                        cancelled: ctx.cancelled,
                        moved: std::mem::take(&mut ctx.moved),
                    });
                    wake();
                }
            })
            .expect("spawn fs-ops worker");
        Self { queue, stop, ctl, rx }
    }

    pub fn submit(&self, req: OpRequest) {
        let (lock, cv) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.push_back(req);
        }
        cv.notify_one();
    }

    /// Pause, resume or cancel the job that is running. A queued job is not
    /// running yet, so [`Runner::drop_queued`] is what cancels one of those.
    pub fn control(&self, id: u64, c: Control) {
        let _ = self.ctl.send((id, c));
    }

    /// Take a job out of the queue before it starts. `false` when it is not
    /// there, which means it is already running (or already done).
    pub fn drop_queued(&self, id: u64) -> bool {
        let (lock, _) = &*self.queue;
        let Ok(mut q) = lock.lock() else { return false };
        let Some(at) = q.iter().position(|r| r.id == id) else { return false };
        q.remove(at);
        true
    }

    /// Move a queued job to the front, so it is the next one to run.
    pub fn promote(&self, id: u64) -> bool {
        let (lock, _) = &*self.queue;
        let Ok(mut q) = lock.lock() else { return false };
        let Some(at) = q.iter().position(|r| r.id == id) else { return false };
        let Some(req) = q.remove(at) else { return false };
        q.push_front(req);
        true
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        // Let the worker out of its wait so the thread ends with the app.
        self.stop.store(true, Ordering::Relaxed);
        self.queue.1.notify_all();
    }
}

/// Block until there is a job to run, or until the app is going away.
fn next_job(queue: &Queue, stop: &AtomicBool) -> Option<OpRequest> {
    let (lock, cv) = &**queue;
    let mut q = lock.lock().ok()?;
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(req) = q.pop_front() {
            return Some(req);
        }
        q = cv.wait(q).ok()?;
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
    ctl: &'a Receiver<(u64, Control)>,
    wake: &'a (dyn Fn() + Send),
    files_done: u64,
    bytes_done: u64,
    policy: Policy,
    errors: Vec<String>,
    cancelled: bool,
    paused: bool,
    last_report: std::time::Instant,
    /// Where each moved file ended up. Empty for everything but a move.
    moved: Vec<(PathBuf, PathBuf)>,
}

impl Ctx<'_> {
    fn run(&mut self, req: &OpRequest) {
        let (files, bytes) = match req.kind {
            OpKind::Trash | OpKind::Delete | OpKind::Restore => (req.srcs.len() as u64, 0),
            OpKind::Symlink { .. } | OpKind::Hardlink => (req.srcs.len() as u64, 0),
            // Extract counts each archive as one unit: what is inside is only
            // known by reading it, and reading it twice to fill a progress bar
            // is not worth it. The name of each entry still goes past.
            OpKind::Copy | OpKind::Move | OpKind::Compress(_) | OpKind::Extract => {
                measure(&req.srcs)
            }
        };
        let _ = self.ev.send(OpEvent::Started { id: self.id, files, bytes });
        (self.wake)();

        match req.kind {
            // One call for the whole selection, because the shell batches it
            // and that is what puts a single step in the Recycle Bin's own
            // undo. The catch is the reporting: a batch that goes wrong comes
            // back as one error naming nothing -- "Some operations were
            // aborted" over a selection of thirty -- and it is silent about
            // which of them, or how many, actually went.
            //
            // So the failure is walked again, one path at a time. Whatever the
            // batch did manage is already gone and is only counted; what is
            // still there is tried on its own, and now the error can say which
            // file it is about. Costly, but only on the path that has already
            // failed.
            OpKind::Trash => match trash::delete_all(&req.srcs) {
                Ok(()) => self.files_done = req.srcs.len() as u64,
                Err(_) => {
                    for p in &req.srcs {
                        // `symlink_metadata`, so a link whose target has gone
                        // still counts as present -- it is, and it can be
                        // trashed.
                        if std::fs::symlink_metadata(p).is_err() {
                            self.files_done += 1;
                            continue;
                        }
                        self.report(&p.to_string_lossy());
                        match trash::delete(p) {
                            Ok(()) => self.files_done += 1,
                            Err(e) => self.errors.push(format!("{}: {e}", crate::util::file_name(p))),
                        }
                    }
                }
            },
            // Reading the trash walks all of it, so it is read once here and
            // then asked about each path in turn. Restoring one at a time keeps
            // a path that something else has taken over from stopping the rest.
            OpKind::Restore => match restore::found(&req.srcs) {
                Err(e) => self.errors.push(e),
                Ok(found) => {
                    for p in &req.srcs {
                        if self.cancelled {
                            break;
                        }
                        self.report(&p.to_string_lossy());
                        if let Err(e) = restore::put_back(&found, p) {
                            self.errors.push(e);
                        }
                        self.files_done += 1;
                    }
                }
            },
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
                        self.errors.push(format!("{}: {}", short(src), explain(&e)));
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
                        self.note_move(moving, src, &dest);
                        continue;
                    }
                    if is_inside(src, &dest) {
                        self.errors
                            .push(format!("{}: cannot copy into itself", short(src)));
                        continue;
                    }
                    let Some(dest) = self.resolve_dest(src, dest) else { continue };
                    self.transfer(src, &dest, moving);
                    self.note_move(moving, src, &dest);
                }
            }
            OpKind::Extract => self.extract(req),
            OpKind::Compress(format) => self.compress(req, format),
        }
        self.report("");
    }

    /// Each archive gets a folder of its own, named after it. A name already
    /// taken is stepped past rather than merged into: two unpacks of the same
    /// archive should not interleave their contents.
    fn extract(&mut self, req: &OpRequest) {
        for src in &req.srcs {
            if self.cancelled {
                break;
            }
            if archive::Format::from_path(src).is_none() {
                self.errors.push(format!("{}: not an archive", short(src)));
                continue;
            }
            let into = unique_name(&archive::extract_dir(src, &req.dest_dir));
            let mut seen = 0u64;
            let r = archive::extract(src, &into, &mut |name, _size| {
                seen += 1;
                self.report_entry(name);
                !self.cancelled
            });
            if let Err(e) = r {
                self.errors.push(format!("{}: {e}", short(src)));
                // A refused entry leaves the rest in place; an empty folder
                // from a job that got nowhere is just litter.
                if seen == 0 {
                    let _ = std::fs::remove_dir(&into);
                }
            }
            self.files_done += 1;
            self.bytes_done += std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
            self.report(&src.to_string_lossy());
        }
    }

    /// One archive from everything selected, named relative to the directory
    /// the job started in so a folder keeps its shape inside.
    fn compress(&mut self, req: &OpRequest, format: archive::Format) {
        let Some(dest) = req.dest_file.clone() else {
            self.errors.push("compress: no archive name".into());
            return;
        };
        // An archive that is already there goes through the same prompt a
        // paste would. The first source stands in as `src` so the dialog reads
        // as what is being packed into what.
        let first = req.srcs.first().cloned().unwrap_or_else(|| dest.clone());
        let dest = match req.force {
            true => dest,
            false => match self.resolve_dest(&first, dest) {
                Some(d) => d,
                None => return,
            },
        };
        let r = archive::compress(&req.srcs, &req.dest_dir, &dest, format, &mut |name, bytes| {
            self.files_done += 1;
            self.bytes_done += bytes;
            self.report_entry(name);
            !self.cancelled
        });
        if let Err(e) = r {
            self.errors.push(format!("{}: {e}", short(&dest)));
            // A half-written archive is worse than none: it looks openable.
            let _ = std::fs::remove_file(&dest);
        }
    }

    /// Progress from inside an archive, where the entry name is most of what
    /// there is to say. Rate-limited like [`Ctx::report`], but never silent:
    /// an entry is always worth naming once the interval has passed.
    fn report_entry(&mut self, name: &str) {
        self.pump();
        if self.last_report.elapsed().as_millis() < 50 {
            return;
        }
        self.last_report = std::time::Instant::now();
        let _ = self.ev.send(OpEvent::Progress {
            id: self.id,
            files_done: self.files_done,
            bytes_done: self.bytes_done,
            current: name.to_owned(),
        });
        (self.wake)();
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

    /// Remember a move that actually happened, for undo.
    ///
    /// Judged from the disk rather than from a return value: `transfer` falls
    /// back from a rename to copy-and-delete and can fail part way through
    /// either. A move is done when the destination is there and the source is
    /// not, and that is true however it got that way.
    fn note_move(&mut self, moving: bool, src: &Path, dest: &Path) {
        if moving && exists(dest) && !exists(src) {
            self.moved.push((src.to_path_buf(), dest.to_path_buf()));
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

    /// Take whatever the task panel has said, and park here while it wants the
    /// job paused. Every loop that moves bytes calls this, so a pause lands
    /// between files, or between chunks of one big file, and never mid-write.
    fn pump(&mut self) {
        let mut was = self.paused;
        while let Ok((id, c)) = self.ctl.try_recv() {
            if id == self.id {
                self.apply(c);
            }
        }
        // A paused job costs nothing but the thread it sits on, so it waits
        // here rather than spinning.
        while self.paused && !self.cancelled {
            if was != self.paused {
                self.announce_pause();
                was = self.paused;
            }
            match self.ctl.recv() {
                Ok((id, c)) if id == self.id => self.apply(c),
                Ok(_) => {}
                // The app is gone; there is nothing left to finish for.
                Err(_) => {
                    self.cancelled = true;
                    self.paused = false;
                }
            }
        }
        if was != self.paused {
            self.announce_pause();
        }
    }

    fn apply(&mut self, c: Control) {
        match c {
            Control::Pause => self.paused = true,
            Control::Resume => self.paused = false,
            Control::Cancel => {
                self.cancelled = true;
                self.paused = false;
            }
        }
    }

    fn announce_pause(&mut self) {
        let _ = self.ev.send(OpEvent::Paused { id: self.id, paused: self.paused });
        (self.wake)();
    }

    fn report(&mut self, current: &str) {
        self.pump();
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

/// The OS message, plus what to do about it where that is not obvious.
///
/// Windows refuses to create a symlink without a privilege most accounts do not
/// hold, and says so as "the client does not hold the required privilege"
/// (1314). That names the obstacle and not the remedy, and it is a wall every
/// first attempt on Windows runs into — the text is not even easy to search
/// for. Only 1314 gets the extra line: telling someone to turn on Developer
/// Mode when the real problem was a read-only folder would be worse than
/// saying nothing.
#[cfg(windows)]
fn explain(e: &std::io::Error) -> String {
    const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
    if e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) {
        return format!(
            "{e} — Windows needs Developer Mode for symlinks \
             (Settings > System > For developers), or run filer as administrator"
        );
    }
    e.to_string()
}

#[cfg(not(windows))]
fn explain(e: &std::io::Error) -> String {
    e.to_string()
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
