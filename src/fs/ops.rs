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
    /// Unpack members of an archive, named by paths that run through it
    /// (`…\pack.zip\docs\a.txt`, as the archive view yanks them), into
    /// `dest_dir` under their own names.
    TakeOut,
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
            Self::TakeOut => "Take out",
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
        /// For a symlink or hardlink job: each link made, under the name it
        /// really got. What `u` removes.
        linked: Vec<Link>,
        /// Folder symlinks Windows refused for want of the privilege, as the
        /// junctions that could stand in for them (Q46). The app asks.
        junctions: Vec<Link>,
        /// For a compress: the archive written, under the name it really got
        /// (a name already taken can be resolved to `x_1.zip` here). For an
        /// unpack: each folder the contents are now in, after a lone folder
        /// inside was lifted out (Q43) -- the name to give back to the reader,
        /// since it is often not the archive's own.
        made: Vec<PathBuf>,
        /// For a trash: the paths that went. When some did not, these are what
        /// `u` can bring back -- the whole step was thrown away before, so four
        /// files of five in the bin and `u` said "Nothing to undo" (#83).
        trashed: Vec<PathBuf>,
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
                        linked: Vec::new(),
                        junctions: Vec::new(),
                        made: Vec::new(),
                        trashed: Vec::new(),
                    };
                    ctx.run(&req);
                    let _ = ev_tx.send(OpEvent::Finished {
                        id: req.id,
                        kind: req.kind,
                        errors: std::mem::take(&mut ctx.errors),
                        cancelled: ctx.cancelled,
                        moved: std::mem::take(&mut ctx.moved),
                        linked: std::mem::take(&mut ctx.linked),
                        junctions: std::mem::take(&mut ctx.junctions),
                        made: std::mem::take(&mut ctx.made),
                        trashed: std::mem::take(&mut ctx.trashed),
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
    /// The links a link job made. Empty for everything else.
    linked: Vec<Link>,
    /// See `OpEvent::Finished::junctions`.
    junctions: Vec<Link>,
    /// The archive a compress wrote, once it is whole; the folders an unpack
    /// filled.
    made: Vec<PathBuf>,
    /// What a trash sent to the bin. Empty for everything else.
    trashed: Vec<PathBuf>,
}

impl Ctx<'_> {
    fn run(&mut self, req: &OpRequest) {
        let (files, bytes) = match req.kind {
            OpKind::Trash | OpKind::Delete | OpKind::Restore => (req.srcs.len() as u64, 0),
            OpKind::Symlink { .. } | OpKind::Hardlink | OpKind::TakeOut => (req.srcs.len() as u64, 0),
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
                Ok(()) => {
                    self.files_done = req.srcs.len() as u64;
                    self.trashed = req.srcs.clone();
                }
                Err(_) => {
                    for p in &req.srcs {
                        // `symlink_metadata`, so a link whose target has gone
                        // still counts as present -- it is, and it can be
                        // trashed. One that is gone already went with the
                        // batch call before it gave up.
                        if std::fs::symlink_metadata(p).is_err() {
                            self.files_done += 1;
                            self.trashed.push(p.clone());
                            continue;
                        }
                        self.report(&p.to_string_lossy());
                        match trash::delete(p) {
                            Ok(()) => {
                                self.files_done += 1;
                                self.trashed.push(p.clone());
                            }
                            Err(e) => {
                                // The trash says "Some operations were aborted"
                                // and nothing about which or why; a file held
                                // open elsewhere is by far the usual reason (#83).
                                let why = match held_open(p) {
                                    true => "it is open in another program".to_owned(),
                                    false => trash_error(&e),
                                };
                                self.errors.push(format!("{}: {why}", crate::util::file_name(p)));
                            }
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
                    let link = Link { at: dest, target, dir: src.is_dir(), hard: false, junction: false };
                    match link.make() {
                        Ok(()) => self.linked.push(link),
                        Err(e) => {
                            if refused_privilege(&e) && link.dir {
                                let target = src.clone();
                                self.junctions.push(Link { target, junction: true, ..link.clone() });
                            }
                            self.errors.push(format!("{}: {}", short(src), symlink_error(&e, &link, src)));
                        }
                    }
                    self.files_done += 1;
                }
            }
            OpKind::Hardlink => {
                for src in &req.srcs {
                    let dest = req.dest_dir.join(file_name(src));
                    let Some(dest) = self.resolve_dest(src, dest) else { continue };
                    let link = Link { at: dest, target: src.clone(), dir: false, hard: true, junction: false };
                    match link.make() {
                        Ok(()) => self.linked.push(link),
                        Err(e) => self.errors.push(format!("{}: {}", short(src), hardlink_error(&e, src, &req.dest_dir))),
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
            OpKind::TakeOut => self.take_out(req),
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
            match r {
                // Whole and not cancelled: an archive that is one folder comes
                // out as that folder (Q43). A cancelled one stays wrapped, so
                // what was left half-done is all in one place.
                Ok(()) if !self.cancelled => match archive::lift_lone_folder(&into, unique_name) {
                    Ok(at) => self.made.push(at),
                    Err(e) => self.errors.push(format!("{}: {e}", short(src))),
                },
                Ok(()) => {}
                Err(e) => {
                    self.errors.push(format!("{}: {e}", short(src)));
                    // A refused entry leaves the rest in place; an empty folder
                    // from a job that got nowhere is just litter.
                    if seen == 0 {
                        let _ = std::fs::remove_dir(&into);
                    }
                }
            }
            self.files_done += 1;
            self.bytes_done += std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
            self.report(&src.to_string_lossy());
        }
    }

    /// Each member into `dest_dir`, through the same question about a name
    /// already there as a copy. It is unpacked into a hidden folder of the
    /// job's own first and then moved to its name, so a Skip leaves nothing
    /// behind and a Rename gets the name typed.
    fn take_out(&mut self, req: &OpRequest) {
        let staging = req.dest_dir.join(format!(".filer-take-out-{}", self.id));
        for src in &req.srcs {
            if self.cancelled {
                break;
            }
            let Some((archive, member)) = archive::split_member(src) else {
                self.errors.push(format!("{}: not inside an archive", short(src)));
                continue;
            };
            let name = crate::util::file_name(src);
            let r = archive::extract_one(&archive, &member, &staging, &mut |entry, _| {
                self.report_entry(entry);
                !self.cancelled
            });
            if let Err(e) = r {
                self.errors.push(format!("{name}: {e}"));
                continue;
            }
            let unpacked = staging.join(&name);
            if let Some(dest) = self.resolve_dest(src, req.dest_dir.join(&name)) {
                if exists(&dest) {
                    let _ = match std::fs::symlink_metadata(&dest).is_ok_and(|m| m.is_dir()) {
                        true => std::fs::remove_dir_all(&dest),
                        false => std::fs::remove_file(&dest),
                    };
                }
                match std::fs::rename(&unpacked, &dest) {
                    Ok(()) => self.made.push(dest),
                    Err(e) => self.errors.push(format!("{name}: {e}")),
                }
            }
            self.files_done += 1;
            self.report(&src.to_string_lossy());
        }
        let _ = std::fs::remove_dir_all(&staging);
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
        match r {
            Ok(()) if !self.cancelled => self.made.push(dest),
            Ok(()) => {}
            Err(e) => {
                self.errors.push(format!("{}: {e}", short(&dest)));
                // A half-written archive is worse than none: it looks openable.
                let _ = std::fs::remove_file(&dest);
            }
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
///
/// A folder gets one more line (#168): a junction needs neither, and most
/// links people want on Windows are to folders. It names the command with
/// both paths, absolute because a junction cannot be relative.
fn symlink_error(e: &std::io::Error, link: &Link, src: &Path) -> String {
    if !refused_privilege(e) {
        return e.to_string();
    }
    let mut said = format!(
        "{e} — Windows needs Developer Mode for symlinks \
         (Settings > System > For developers), or run filer as administrator"
    );
    if link.dir {
        said += &format!(". A junction needs neither: {}", mklink_line(&link.at, src));
    }
    said
}

/// The line that makes a junction at `at` to `target`: what the refusal above
/// tells people to type, and what the junction question's `c` copies (Q56),
/// spelled the same in both.
///
/// Through `cmd /d /c` because `mklink` is built into `cmd` and is nothing
/// anywhere else: pasted bare into filer's own pane, which runs PowerShell,
/// it answered "the term 'mklink' is not recognized" (#193). With the prefix
/// the same line works in `cmd`, `pwsh` and Windows PowerShell alike, and it
/// is what [`junction`] runs.
pub fn mklink_line(at: &Path, target: &Path) -> String {
    format!("cmd /d /c mklink /J \"{}\" \"{}\"", at.display(), target.display())
}

/// Windows' "the client does not hold the required privilege" (1314): a
/// symlink without Developer Mode or elevation.
fn refused_privilege(e: &std::io::Error) -> bool {
    const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
    cfg!(windows) && e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD)
}

/// A junction at `at` to the folder `target`, which must be absolute.
///
/// `mklink /J` rather than the reparse-point ioctl by hand: it is one line,
/// it is what the error message already tells people to type, and it needs no
/// new crate. `cmd` expands `%NAME%` even inside quotes, so a path with a `%`
/// in it is refused rather than handed over to be rewritten.
#[cfg(windows)]
fn junction(target: &Path, at: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    for p in [target, at] {
        if p.to_string_lossy().contains('%') {
            return Err(std::io::Error::other(format!("{} has a % in it; make the junction by hand", short(p))));
        }
    }
    let out = std::process::Command::new("cmd")
        .raw_arg(format!("/d /c mklink /J \"{}\" \"{}\"", at.display(), target.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    match out.status.success() && at.exists() {
        true => Ok(()),
        // `cmd` answers in the console's code page, which is not UTF-8 on a
        // Japanese Windows; the exit code is what can be read reliably.
        false => Err(std::io::Error::other(format!("mklink /J failed (exit {})", out.status.code().unwrap_or(-1)))),
    }
}

#[cfg(not(windows))]
fn junction(_target: &Path, _at: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "junctions are a Windows thing"))
}

/// Why the trash refused a path, in words. One case gets more than the
/// crate's own text: it normalizes every path before handing it to the
/// Recycle Bin, and a volume that cannot report its final paths -- an ImDisk
/// RAM disk, some virtual drives -- fails that with os error 1, so `d` there
/// always failed with `CanonicalizePath { original: "R:\\Temp\\…" }`. Handing
/// the raw path on instead is not done: on a volume with no Recycle Bin the
/// shell may delete for good without asking, and `d` is the key that can be
/// undone. So it says what happened and points at `D`.
fn trash_error(e: &trash::Error) -> String {
    match e {
        trash::Error::CanonicalizePath { original } => {
            // The drive (`R:`) where there is one, the folder otherwise.
            let drive = match original.components().next() {
                Some(std::path::Component::Prefix(p)) => p.as_os_str().to_string_lossy().into_owned(),
                _ => original.display().to_string(),
            };
            format!(
                "the Recycle Bin can't take files from {drive} (this drive can't report its own paths: \
                 a RAM disk or a virtual drive). Use D to delete permanently"
            )
        }
        e => e.to_string(),
    }
}

/// Whether another program holds `p` open so that it cannot be removed:
/// opening it with no sharing at all fails with a sharing violation (32).
#[cfg(windows)]
fn held_open(p: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    p.is_file()
        && std::fs::OpenOptions::new().read(true).share_mode(0).open(p).err().and_then(|e| e.raw_os_error())
            == Some(ERROR_SHARING_VIOLATION)
}

/// Elsewhere a file open in another program can be removed all the same.
#[cfg(not(windows))]
fn held_open(_: &Path) -> bool {
    false
}

/// Why a hardlink failed, in words that say what to do when the reason is
/// the one every hardlink across drives hits: "The system cannot move the
/// file to a different disk drive" (os error 17) named neither drive nor the
/// way round it (#83).
fn hardlink_error(e: &std::io::Error, src: &Path, dest_dir: &Path) -> String {
    // ERROR_NOT_SAME_DEVICE on Windows, EXDEV elsewhere.
    let cross = if cfg!(windows) { 17 } else { 18 };
    if e.raw_os_error() != Some(cross) {
        return e.to_string();
    }
    format!(
        "hardlinks can't cross drives ({} → {}). Use p to copy instead",
        volume(src),
        volume(dest_dir)
    )
}

/// The drive a path is on, as its prefix (`R:`, `\\host\share`), or the
/// root where paths have none.
fn volume(p: &Path) -> String {
    match p.components().next() {
        Some(std::path::Component::Prefix(pre)) => pre.as_os_str().to_string_lossy().into_owned(),
        _ => p.components().take(2).collect::<std::path::PathBuf>().display().to_string(),
    }
}

/// A link a job made: enough to take it away again and to make it again,
/// which is what `u` and `U` do with it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Link {
    /// The link itself.
    pub at: PathBuf,
    /// What it points at, as written into it: relative for `-` with
    /// `relative`, and the source path for a hardlink.
    pub target: PathBuf,
    /// A symlink to a folder, which Windows makes and removes differently.
    pub dir: bool,
    pub hard: bool,
    /// A Windows junction rather than a symlink: what a folder link can be
    /// without Developer Mode (Q46). `target` is absolute, as a junction's
    /// has to be.
    pub junction: bool,
}

impl Link {
    pub fn make(&self) -> std::io::Result<()> {
        match (self.hard, self.junction) {
            (true, _) => std::fs::hard_link(&self.target, &self.at),
            (false, true) => junction(&self.target, &self.at),
            (false, false) => symlink(&self.target, &self.at, self.dir),
        }
    }

    /// Remove the link and nothing it points at. Whatever stands at `at` must
    /// still be a link of this kind: a symlink for a symlink, and for a
    /// hardlink a file that is still the same file as its source -- not
    /// something that has taken the name since.
    pub fn remove(&self) -> std::io::Result<()> {
        let meta = std::fs::symlink_metadata(&self.at)?;
        let still = match self.hard {
            true => meta.is_file() && same_file(&self.at, &self.target),
            false => meta.file_type().is_symlink(),
        };
        if !still {
            return Err(std::io::Error::other(format!("{} is no longer the link that was made", short(&self.at))));
        }
        // A folder symlink on Windows is a directory entry and goes with
        // `remove_dir`; on Unix every symlink is removed as a file.
        match cfg!(windows) && self.dir && !self.hard {
            true => std::fs::remove_dir(&self.at),
            false => std::fs::remove_file(&self.at),
        }
    }
}

/// Whether two paths name one file on disk.
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
        _ => false,
    }
}

/// Windows has no stable file id in `std` yet; the size and the time last
/// written agree for two names of one file, and a file that took the name
/// since would have to match both.
#[cfg(not(unix))]
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.len() == b.len() && a.modified().ok() == b.modified().ok(),
        _ => false,
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

#[cfg(test)]
mod symlink_message {
    use super::*;

    /// #168: refused for want of the privilege, a folder is told about the
    /// junction it can make instead, a file is not (a junction is folders
    /// only), and any other failure keeps the OS's own words.
    #[test]
    fn a_refused_folder_link_names_the_junction() {
        let refused = std::io::Error::from_raw_os_error(1314);
        let (src, at) = (Path::new("C:/work/src"), Path::new("C:/work/dst/src"));
        let folder = Link { at: at.to_path_buf(), target: PathBuf::from("../src"), dir: true, hard: false, junction: false };
        let file = Link { dir: false, ..folder.clone() };
        let said = symlink_error(&refused, &folder, src);
        if cfg!(windows) {
            assert!(said.contains("needs Developer Mode"), "{said}");
            assert!(said.ends_with(r#"A junction needs neither: cmd /d /c mklink /J "C:/work/dst/src" "C:/work/src""#), "{said}");
            let said = symlink_error(&refused, &file, src);
            assert!(said.contains("needs Developer Mode") && !said.contains("junction"), "{said}");
        } else {
            assert_eq!(said, refused.to_string(), "only Windows asks for the privilege");
        }
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert_eq!(symlink_error(&other, &folder, src), other.to_string());
    }
}

#[cfg(test)]
mod hardlink_message {
    use super::*;

    /// #83: a hardlink across drives names both and the way round; any other
    /// failure keeps the OS's own words.
    #[test]
    fn a_hardlink_across_drives_says_so() {
        let cross = std::io::Error::from_raw_os_error(if cfg!(windows) { 17 } else { 18 });
        let (src, dest) = if cfg!(windows) {
            (Path::new(r"R:\tmp\a.txt"), Path::new(r"C:\work"))
        } else {
            (Path::new("/mnt/a.txt"), Path::new("/home/me"))
        };
        let said = hardlink_error(&cross, src, dest);
        assert!(said.starts_with("hardlinks can't cross drives ("), "{said}");
        assert!(said.ends_with("Use p to copy instead"), "{said}");
        let drives = if cfg!(windows) { "(R: → C:)" } else { "(/mnt → /home)" };
        assert!(said.contains(drives), "{said}");
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert_eq!(hardlink_error(&other, src, dest), other.to_string());
    }
}

#[cfg(test)]
mod trash_message {
    use super::*;

    /// A drive whose paths cannot be normalized (an ImDisk RAM disk) gets a
    /// sentence naming it and `D`, not the crate's `CanonicalizePath { … }`;
    /// any other refusal keeps the crate's own words.
    #[test]
    fn a_drive_the_trash_cannot_use_is_named() {
        let (at, drive) = if cfg!(windows) { (r"R:\Temp\run-1", "R:") } else { ("/mnt/ram/run-1", "/mnt/ram/run-1") };
        let said = trash_error(&trash::Error::CanonicalizePath { original: PathBuf::from(at) });
        assert!(said.starts_with(&format!("the Recycle Bin can't take files from {drive} (")), "{said}");
        assert!(said.ends_with("Use D to delete permanently"), "{said}");
        let other = trash::Error::Unknown { description: "Some operations were aborted".into() };
        assert_eq!(trash_error(&other), other.to_string());
    }
}
