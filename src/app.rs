//! Application state and the action dispatcher.
//!
//! Everything the UI does goes through [`Act`], so keys, mouse clicks and
//! internal follow-ups all take the same path.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::config::cmd::{Act, CopyWhat, EscapeWhat, RenameCursor, SearchVia, Step, Tri, ZoomTo};
use crate::config::keys::{Code, Key};
use crate::config::{keymap, Config};
use crate::core::folder::{Filter, Folder, LoadState};
use crate::core::fuzzy;
use crate::core::tab::{CdFallout, Finder, PendingCd, Tab};
use crate::diff;
use crate::exec;
use crate::fs::archive;
use crate::fs::git;
use crate::fs::ops::{self, OpKind, OpRequest, Resolution};
use crate::fs::restore;
use crate::fs::scan::{ScanResult, Scanner};
use crate::fs::watch::Watcher;
use crate::fs::{Entry, Kind, SortSpec};
use crate::preview::{self, Payload, Previewer, TocEntry};
use crate::rename;
use crate::spot::{self, Section, Spotter};
use crate::util::{self, Lru};

pub const MAX_TABS: usize = 9;

// ----------------------------------------------------------------- overlays

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputKind {
    Create,
    Rename { from: PathBuf },
    /// A rule to rename all of these at once, previewed as it is typed.
    Bulk { paths: Vec<PathBuf> },
    Filter,
    Find { prev: bool },
    Cd,
    /// A string to find in the terminal's scrollback.
    TermFind,
    /// The name of the archive to pack the selection into.
    Compress,
    Shell { block: bool },
    Search { via: SearchVia },
    ConflictRename { job: u64 },
}

pub struct InputOverlay {
    pub kind: InputKind,
    pub title: String,
    pub text: String,
    /// Where to put the caret when the widget first gains focus.
    pub initial_selection: Option<(usize, usize)>,
    pub focused: bool,
    pub completion: Vec<String>,
    pub completion_at: usize,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Conflict { reply: Sender<Resolution>, job: u64 },
    DeleteForever { paths: Vec<PathBuf> },
    BookmarkDeleteAll,
}

pub struct ConfirmOverlay {
    pub title: String,
    pub body: Vec<String>,
    pub options: Vec<(char, String)>,
    pub action: ConfirmAction,
    /// The colliding destination, when the dialog offers a rename.
    pub dest: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub enum PickAction {
    /// `line` (1-based) is handed to editors that take one.
    OpenWith { paths: Vec<PathBuf>, runs: Vec<(String, bool, bool)>, line: Option<usize> },
    Jump { paths: Vec<PathBuf> },
    /// One keymap binding's command list per item.
    Command { runs: Vec<Vec<Act>> },
}

pub struct PickOverlay {
    pub title: String,
    pub items: Vec<String>,
    pub details: Vec<String>,
    pub query: String,
    pub matches: Vec<(usize, i32, Vec<usize>)>,
    pub cursor: usize,
    pub action: PickAction,
    pub focused: bool,
}

impl PickOverlay {
    pub fn refilter(&mut self) {
        let cs = fuzzy::is_case_sensitive(&self.query, true, false);
        let mut out: Vec<(usize, i32, Vec<usize>)> = Vec::new();
        for (i, label) in self.items.iter().enumerate() {
            if let Some(hit) = fuzzy::match_str(&self.query, label, cs) {
                out.push((i, hit.score, hit.positions));
            }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        self.matches = out;
        self.cursor = self.cursor.min(self.matches.len().saturating_sub(1));
    }

    pub fn selected(&self) -> Option<usize> {
        self.matches.get(self.cursor).map(|m| m.0)
    }
}

/// One opener from `yazi.toml`: `(run, block, orphan, label)`, as the pick
/// overlays want it.
pub type OpenerRow = (String, bool, bool, String);

/// Rows of a pick overlay: `(labels, details, runs)`. The three always have
/// the same length; the overlay indexes all of them by the row picked.
pub type PickRows = (Vec<String>, Vec<String>, Vec<Vec<Act>>);

/// Turn the `mgr` bindings into palette rows.
///
/// The label carries both the description and the command text so either one
/// can be typed at the filter. Commands bound to several keys appear once,
/// under the first key the keymap gives them. `openers` are what `yazi.toml`
/// offers for the file under the cursor; they come last, since the palette is
/// a list of commands first.
pub fn palette_items(bindings: &[keymap::Binding], openers: &[OpenerRow]) -> PickRows {
    let (mut items, mut details, mut runs) = (Vec::new(), Vec::new(), Vec::new());
    let mut seen: Vec<&str> = Vec::new();
    for b in bindings {
        let skip = b.run.is_empty()
            || b.run.iter().all(|a| *a == Act::Noop)
            || b.run.iter().any(|a| matches!(a, Act::Unsupported(_)));
        if skip || b.raw.is_empty() || seen.contains(&b.raw.as_str()) {
            continue;
        }
        seen.push(&b.raw);
        items.push(binding_label(b));
        details.push(crate::config::keys::render_seq(&b.on));
        runs.push(b.run.clone());
    }
    push_openers(&mut items, &mut details, &mut runs, openers, "Open with ");
    (items, details, runs)
}

/// The context menu for the file under the cursor: everything the config says
/// can be done with it, without a key having to be pressed for it.
///
/// `yazi.toml`'s openers come first, then the keymap's own `shell` actions —
/// the custom actions a yazi config would write as plugins — then the rest of
/// the bindings that act on the file rather than on the view.
pub fn menu_items(bindings: &[keymap::Binding], openers: &[OpenerRow]) -> PickRows {
    let (mut items, mut details, mut runs) = (Vec::new(), Vec::new(), Vec::new());
    push_openers(&mut items, &mut details, &mut runs, openers, "");

    let mut seen: Vec<&str> = Vec::new();
    // Two passes so the custom actions sit together at the top, above the
    // ordinary file commands.
    for shell_pass in [true, false] {
        for b in bindings {
            let usable = !b.run.is_empty()
                && !b.raw.is_empty()
                && b.run.iter().any(acts_on_file)
                && !b.run.iter().any(|a| matches!(a, Act::Unsupported(_)));
            let is_shell = b.run.iter().any(|a| matches!(a, Act::Shell { .. }));
            if !usable || is_shell != shell_pass || seen.contains(&b.raw.as_str()) {
                continue;
            }
            seen.push(&b.raw);
            items.push(binding_label(b));
            details.push(crate::config::keys::render_seq(&b.on));
            runs.push(b.run.clone());
        }
    }
    (items, details, runs)
}

/// An opener is a shell command with the file substituted in, so it runs as
/// one. `prefix` says what to call it where the file is not already named.
fn push_openers(
    items: &mut Vec<String>,
    details: &mut Vec<String>,
    runs: &mut Vec<Vec<Act>>,
    openers: &[OpenerRow],
    prefix: &str,
) {
    let mut seen: Vec<&str> = Vec::new();
    for (run, block, orphan, label) in openers {
        if seen.contains(&run.as_str()) {
            continue;
        }
        seen.push(run);
        items.push(format!("{prefix}{label}"));
        details.push(run.clone());
        runs.push(vec![Act::Shell {
            run: run.clone(),
            block: *block,
            confirm: false,
            orphan: *orphan,
        }]);
    }
}

fn binding_label(b: &keymap::Binding) -> String {
    match b.desc.is_empty() {
        true => b.raw.clone(),
        false => format!("{}  ·  {}", b.desc, b.raw),
    }
}

/// Whether a command does something to the file under the cursor (or to the
/// selection), as opposed to moving around or changing what the view shows.
/// It decides what the context menu is worth offering.
fn acts_on_file(a: &Act) -> bool {
    matches!(
        a,
        Act::Open { .. }
            | Act::Yank { .. }
            | Act::Unyank
            | Act::Paste { .. }
            | Act::Link { .. }
            | Act::Hardlink
            | Act::Remove { .. }
            | Act::Create { .. }
            | Act::Rename { .. }
            | Act::BulkRename
            | Act::Compare
            | Act::Copy(_)
            | Act::Shell { .. }
            | Act::Extract
            | Act::Compress
            | Act::SendPane { .. }
            | Act::TermSend
            | Act::Spot
            | Act::Quick(_)
            | Act::Follow
            | Act::Reveal(_)
            | Act::Toggle { .. }
    )
}

/// The task panel, which is a list now that its rows can be acted on.
pub struct TasksOverlay {
    pub cursor: usize,
}

/// The spot panel on the hovered file.
pub struct SpotOverlay {
    /// The selected value row, counted across all sections.
    pub cursor: usize,
    /// The first row on screen; the renderer keeps the cursor in view.
    pub scroll: usize,
}

pub enum Overlay {
    None,
    Input(InputOverlay),
    Confirm(ConfirmOverlay),
    Pick(PickOverlay),
    Help,
    Tasks(TasksOverlay),
    Spot(SpotOverlay),
    Diff(DiffOverlay),
}

/// Two files side by side. `outcome` is `None` until the diff worker answers,
/// which is what the view shows as *Comparing…*.
pub struct DiffOverlay {
    pub left: PathBuf,
    pub right: PathBuf,
    pub outcome: Option<diff::Outcome>,
    pub offset: usize,
}

impl Overlay {
    pub fn is_none(&self) -> bool {
        matches!(self, Overlay::None)
    }
}

// -------------------------------------------------------------------- tasks

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    /// Waiting its turn: one job runs at a time.
    Queued,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn label(&self) -> &'static str {
        match self {
            TaskState::Queued => "queued",
            TaskState::Running => "running",
            TaskState::Paused => "paused",
            TaskState::Done => "done",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
    }

    /// Whether the job still has somewhere to go, and so is worth a key.
    pub fn is_live(&self) -> bool {
        matches!(self, TaskState::Queued | TaskState::Running | TaskState::Paused)
    }
}

pub struct Task {
    pub id: u64,
    pub kind: OpKind,
    pub label: String,
    pub files: u64,
    pub bytes: u64,
    pub files_done: u64,
    pub bytes_done: u64,
    pub current: String,
    pub state: TaskState,
    pub errors: Vec<String>,
    pub finished: Option<Instant>,
    /// Bytes per second, smoothed. Zero until there is enough to say.
    speed: f64,
    /// The last point the speed was measured from.
    sampled_at: Instant,
    sampled_bytes: u64,
}

impl Task {
    pub fn fraction(&self) -> f32 {
        if self.bytes > 0 {
            (self.bytes_done as f32 / self.bytes as f32).clamp(0.0, 1.0)
        } else if self.files > 0 {
            (self.files_done as f32 / self.files as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Fold a new byte count into the running speed. Reports arrive about 20
    /// times a second, which is far too often to measure over: a quarter of a
    /// second of work is the shortest window that reads steadily.
    fn sample(&mut self, bytes_done: u64) {
        let dt = self.sampled_at.elapsed().as_secs_f64();
        if dt < 0.25 {
            return;
        }
        let now = bytes_done.saturating_sub(self.sampled_bytes) as f64 / dt;
        // Smoothed, so a run of small files does not make the number jump
        // about faster than it can be read.
        self.speed = match self.speed {
            0.0 => now,
            prev => prev * 0.7 + now * 0.3,
        };
        self.sampled_at = Instant::now();
        self.sampled_bytes = bytes_done;
    }

    /// Bytes per second, or nothing while the job is too young or too still
    /// to have a useful answer.
    pub fn speed(&self) -> Option<u64> {
        (self.state == TaskState::Running && self.speed >= 1.0).then_some(self.speed as u64)
    }

    /// How long the rest should take at the current speed. Only bytes can
    /// answer this: a file count says nothing about how big the files are.
    pub fn eta(&self) -> Option<Duration> {
        let speed = self.speed()? as f64;
        let left = self.bytes.checked_sub(self.bytes_done)?;
        if left == 0 || self.bytes == 0 {
            return None;
        }
        let secs = left as f64 / speed;
        // Past a day the number stops meaning anything.
        (secs.is_finite() && secs < 86_400.0).then(|| Duration::from_secs_f64(secs))
    }
}

/// How loud a toast is, which is to say what colour it takes.
///
/// The distinction that matters is [`Level::Warn`] against [`Level::Error`]:
/// one says the program could not do the thing, the other says it went ahead
/// and there is something you may want to know. Painting both red leaves the
/// reader no way to tell a broken startup from a stale line in their keymap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Info,
    Warn,
    Error,
}

pub struct Toast {
    pub text: String,
    pub level: Level,
    pub at: Instant,
    /// How many times this same line has been raised. A held-down key can
    /// produce five identical warnings, and five is the whole toast area, so
    /// one repeated message would hide every other for six seconds — including
    /// the header summary underneath it, which is where the reason usually is.
    pub count: u32,
}

// ------------------------------------------------------------------ preview

pub enum PreviewState {
    Empty,
    Loading,
    Dir(Folder),
    Ready(Payload),
}

#[derive(Clone)]
pub struct CachedPreview {
    pub payload: Payload,
    /// The uploaded image, so a cache hit can draw without re-uploading.
    pub texture: Option<egui::TextureHandle>,
}

pub struct PreviewSlot {
    pub key: Option<preview::Key>,
    pub state: PreviewState,
    pub texture: Option<egui::TextureHandle>,
    pub request_id: u64,
    pub pending_since: Option<Instant>,
    pub cache: Lru<preview::Key, CachedPreview>,
    /// Size of the preview pane in pixels, used when decoding images.
    pub box_size: (u32, u32),
    /// The largest `preview_offset` the pane can usefully show, as the last
    /// draw worked it out. `seek` stops here, so a scroll past the end never
    /// becomes a frame drawn from beyond the content.
    pub max_offset: usize,
    /// Text columns across the pane, which rendered Markdown wraps to.
    pub cols: u16,
    /// The outline entry under the cursor while the keys drive the preview's
    /// outline rather than the file list.
    pub outline: Option<usize>,
    /// A file whose outline should take the keys as soon as its preview lands
    /// (`enter` was pressed before it had loaded).
    pub outline_wanted: Option<PathBuf>,
    /// How an image is scaled. `None` is fit-to-pane, where every image starts.
    pub zoom: Option<f32>,
    /// How far a zoomed image has been dragged from centered, in points.
    pub pan: egui::Vec2,
    /// The scale *fit* comes to for the image on screen. Written by the pane
    /// each frame, since only it knows how big it is; read when zooming away
    /// from fit so the first step does not jump.
    pub fit: f32,
}

/// The scale that fits an image of `w` × `h` into `avail`, never magnifying: a
/// small image sits at its own size rather than being blown up unasked.
pub fn image_fit(avail: egui::Vec2, w: f32, h: f32) -> f32 {
    if w <= 0.0 || h <= 0.0 {
        return 1.0;
    }
    (avail.x / w).min(avail.y / h).min(1.0)
}

/// The smallest and largest an image may be scaled to.
const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 32.0;

/// Zoom about `pointer`, so whatever is under the cursor stays under it.
/// Without this the image slides out from under the eye as it grows.
pub fn zoom_at(zoom: f32, pan: egui::Vec2, center: egui::Pos2, pointer: egui::Pos2, factor: f32) -> (f32, egui::Vec2) {
    let next = (zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    if zoom <= 0.0 {
        return (next, pan);
    }
    // Where the pointer is relative to the image's own centre.
    let d = pointer - center - pan;
    (next, pan + d * (1.0 - next / zoom))
}

/// Keep an image from being dragged off the pane. Smaller than the pane, it has
/// nowhere to go and stays centred.
pub fn clamp_pan(pan: egui::Vec2, shown: egui::Vec2, avail: egui::Vec2) -> egui::Vec2 {
    let slack = ((shown - avail) * 0.5).max(egui::Vec2::ZERO);
    egui::Vec2::new(pan.x.clamp(-slack.x, slack.x), pan.y.clamp(-slack.y, slack.y))
}

/// The box the preview worker should decode an image into for the zoom in
/// force. The worker renders to the pane's size, so magnifying past that would
/// show a blurred copy; asking for a bigger decode is what the
/// `box_size` in [`preview::Key`] is for, and the cache keeps both.
///
/// Stepping in powers of two means dragging the zoom around costs a handful of
/// decodes rather than one per frame, and the cap keeps a huge photo from
/// asking for a gigabyte of texture.
///
/// What matters is the zoom against `fit`, not the zoom on its own: a photo that
/// fits at 5% is already asking for twice the pane's detail at 10%, while a
/// small icon at 2x is asking for nothing that exists.
pub fn zoom_box(pane: (u32, u32), zoom: Option<f32>, fit: f32) -> (u32, u32) {
    const CAP: u32 = 4096;
    let want = match zoom {
        Some(z) if fit > 0.0 => (z / fit).max(1.0),
        _ => 1.0,
    };
    let step = want.log2().ceil().exp2();
    (((pane.0 as f32 * step) as u32).min(CAP), ((pane.1 as f32 * step) as u32).min(CAP))
}

impl Default for PreviewSlot {
    fn default() -> Self {
        Self {
            key: None,
            state: PreviewState::Empty,
            texture: None,
            request_id: 0,
            pending_since: None,
            cache: Lru::new(24),
            box_size: (900, 900),
            max_offset: 0,
            cols: 80,
            outline: None,
            outline_wanted: None,
            zoom: None,
            pan: egui::Vec2::ZERO,
            fit: 1.0,
        }
    }
}

// --------------------------------------------------------------- split view

/// The second pane. The pane holding the keys always shows `App::active`, so
/// every action keeps working on one tab and never has to know about the split.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Split {
    /// The tab shown in the pane that does not have the keys.
    pub other: usize,
    /// The pane holding the keys is the right-hand one.
    pub right: bool,
}

/// Files being dragged from one pane towards the other.
///
/// It lives on `App` rather than in the list because the two panes are drawn
/// separately: the one the drag began in has no idea where it ends.
pub struct Drag {
    /// The tab the drag started in.
    pub from: usize,
    pub paths: Vec<PathBuf>,
    /// What to draw under the pointer while it is in flight.
    pub label: String,
}

/// Follow the other pane's tab index after the tab at `removed` is dropped.
/// `None` means that pane's own tab went away, so the split closes.
fn split_after_remove(other: usize, removed: usize) -> Option<usize> {
    match other.cmp(&removed) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Less => Some(other),
        std::cmp::Ordering::Greater => Some(other - 1),
    }
}

/// Where `tab_create` opens, and whether the target still has to prove itself
/// a directory. A path someone typed gets the same benefit of the doubt a
/// typed `cd` gets — it may name a file, and the parent is then what was
/// meant. `hovered` is the entry under the cursor, which came from a listing.
fn new_tab_target(
    base: &Path,
    current: bool,
    path: Option<&str>,
    hovered: Option<&Entry>,
    home: Option<PathBuf>,
) -> (PathBuf, bool) {
    match path {
        Some(p) if !p.is_empty() => (util::resolve_against(base, p), true),
        _ if current => match hovered {
            Some(e) if e.is_dir_like() => (e.path.clone(), false),
            _ => (base.to_path_buf(), false),
        },
        _ => (home.unwrap_or_else(|| base.to_path_buf()), false),
    }
}

/// Follow the other pane's tab index across `tabs.swap(a, b)`.
fn split_after_swap(other: usize, a: usize, b: usize) -> usize {
    if other == a {
        b
    } else if other == b {
        a
    } else {
        other
    }
}

// ---------------------------------------------------------------- bookmarks

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Bookmark {
    pub key: String,
    pub path: PathBuf,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BookmarkOp {
    Save,
    Jump,
    Delete,
}

#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
struct BookmarkFile {
    #[serde(default)]
    bookmark: Vec<Bookmark>,
}

// --------------------------------------------------------------- jump history

/// One directory `z` can jump to, with what it takes to rank it: how often it
/// has been visited and when it last was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    pub path: PathBuf,
    pub hits: u32,
    /// Seconds since the epoch, or 0 for a visit read from a history file
    /// written before this was recorded.
    pub at: i64,
}

/// How highly a directory ranks in `z`: how often it has been visited, weighted
/// by how long ago that last was. The shape is zoxide's — the point of it is
/// that a directory visited twenty times last month does not outrank the one
/// being worked in today.
fn frecency(hits: u32, age_secs: i64) -> f64 {
    let hits = hits as f64;
    // A clock that has gone backwards falls into the first arm, which is where
    // a just-visited directory belongs anyway.
    if age_secs < HOUR {
        hits * 4.0
    } else if age_secs < DAY {
        hits * 2.0
    } else if age_secs < 7 * DAY {
        hits * 0.5
    } else {
        hits * 0.25
    }
}

const HOUR: i64 = 3_600;
const DAY: i64 = 24 * HOUR;

/// How long ago, for the second column of the jump list. Deliberately coarse:
/// this is to tell today's directory from last month's, not to the minute.
fn ago(secs: i64) -> String {
    if secs < 0 {
        String::new()
    } else if secs < 60 {
        "just now".into()
    } else if secs < HOUR {
        format!("{}m ago", secs / 60)
    } else if secs < DAY {
        format!("{}h ago", secs / HOUR)
    } else if secs < 30 * DAY {
        format!("{}d ago", secs / DAY)
    } else {
        format!("{}mo ago", secs / (30 * DAY))
    }
}

/// Now, as the history file counts it.
fn epoch_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// One line of `history.txt`: `path`, a tab, the hit count, a tab, the time.
/// A path cannot hold a tab on any platform this runs on, so the path needs no
/// escaping and comes first.
fn write_visit(v: &Visit) -> String {
    format!("{}\t{}\t{}", v.path.display(), v.hits, v.at)
}

/// The other direction. A line with no counts is one a version before this
/// wrote: it is a real visit, so it keeps a hit, and its age is unknown, which
/// leaves it ranked below anything visited since.
fn parse_visit(line: &str) -> Visit {
    let mut fields = line.split('\t');
    let path = PathBuf::from(fields.next().unwrap_or_default());
    let hits = fields.next().and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
    let at = fields.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Visit { path, hits, at }
}

// --------------------------------------------------------------- undo / redo

/// A step that `u` can take back. Each variant holds enough to go either way,
/// so the same value moves between the two stacks as it is undone and redone.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UndoStep {
    /// `from` became `to`.
    Rename { from: PathBuf, to: PathBuf },
    /// These paths, which were in `dir`, went to the trash.
    Trash { paths: Vec<PathBuf>, dir: PathBuf },
    /// A bulk rename: every `from` became its `to`. One step, so a single `u`
    /// takes the whole batch back.
    Bulk { pairs: Vec<(PathBuf, PathBuf)> },
    /// Files moved by `x` then `p`. The pairs come back from the worker rather
    /// than from what was asked for: a name already taken is resolved down
    /// there, so the file sent to `x.pdf` may have landed as `x_1.pdf`.
    ///
    /// A move is here while a copy is not, and the difference is real. Undoing
    /// a copy would mean deleting the new files to tidy up — worse to get wrong
    /// than the thing being undone. Undoing a move puts a file back where it
    /// came from, which is a rename across directories and deletes nothing.
    Move { pairs: Vec<(PathBuf, PathBuf)> },
}

impl UndoStep {
    /// What a toast says about the step once it has been taken back.
    fn undone_label(&self) -> String {
        match self {
            Self::Rename { from, .. } => format!("Renamed back to {}", util::file_name(from)),
            Self::Trash { paths, .. } => match paths.len() {
                1 => format!("Restored {}", util::file_name(&paths[0])),
                n => format!("Restored {n} item(s)"),
            },
            Self::Bulk { pairs } => format!("Put {} name(s) back", pairs.len()),
            Self::Move { pairs } => match pairs.len() {
                1 => format!("Moved {} back", util::file_name(&pairs[0].0)),
                n => format!("Moved {n} item(s) back"),
            },
        }
    }

    /// And once it has been done again.
    fn redone_label(&self) -> String {
        match self {
            Self::Rename { to, .. } => format!("Renamed to {}", util::file_name(to)),
            Self::Trash { paths, .. } => match paths.len() {
                1 => format!("Trashed {}", util::file_name(&paths[0])),
                n => format!("Trashed {n} item(s)"),
            },
            Self::Bulk { pairs } => format!("Renamed {} file(s)", pairs.len()),
            Self::Move { pairs } => match pairs.len() {
                1 => format!("Moved {}", util::file_name(&pairs[0].0)),
                n => format!("Moved {n} item(s)"),
            },
        }
    }
}

/// Where a step belongs once the work behind it has succeeded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Land {
    /// Something the person just did: `u` can take it back, and whatever was
    /// undone before is no longer on the way forward.
    Fresh,
    /// `u` took it back, so `U` can do it again.
    Undone,
    /// `U` did it again, so `u` can take it back again.
    Redone,
}

/// The two stacks behind `u` and `U`. Apart from `App` so that the rules about
/// which stack a step lands on can be tested without a window.
#[derive(Default, Debug)]
pub struct Undos {
    /// What `u` takes back, newest last.
    pub undo: Vec<UndoStep>,
    /// What `U` does again, newest last.
    pub redo: Vec<UndoStep>,
}

impl Undos {
    /// Older steps than this fall off the bottom. Undo is for the slip that was
    /// just made, not a journal of the session.
    const MAX: usize = 50;

    fn land(&mut self, step: UndoStep, how: Land) {
        match how {
            Land::Fresh => {
                // A new action forks history; what was undone cannot be
                // reached from here any more.
                self.redo.clear();
                Self::push(&mut self.undo, step);
            }
            Land::Undone => Self::push(&mut self.redo, step),
            Land::Redone => Self::push(&mut self.undo, step),
        }
    }

    /// Put a step back where it came from, for work that did not go through.
    fn keep(&mut self, step: UndoStep, how: Land) {
        match how {
            // The action itself failed, so there is nothing to take back.
            Land::Fresh => {}
            Land::Undone => Self::push(&mut self.undo, step),
            Land::Redone => Self::push(&mut self.redo, step),
        }
    }

    fn push(stack: &mut Vec<UndoStep>, step: UndoStep) {
        stack.push(step);
        if stack.len() > Self::MAX {
            stack.remove(0);
        }
    }
}

// ---------------------------------------------------------------------- app

pub struct Yank {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

/// A `Tab` completion in the `cd` prompt that is waiting for its listing.
/// `dir` and `prefix` are what the input line said when the key was pressed,
/// so an answer that arrives over newer text can be told apart and dropped.
struct PendingCompletion {
    id: u64,
    dir: PathBuf,
    prefix: String,
}

pub struct App {
    pub cfg: Config,
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// The second pane, when the view is split.
    pub split: Option<Split>,
    pub cache: Lru<PathBuf, Arc<Vec<Entry>>>,

    pub scanner: Scanner,
    pub previewer: Previewer,
    pub ops: ops::Runner,
    pub watcher: Watcher,
    pub git: git::Git,
    /// The shell in the bottom pane, while there is one.
    pub term: Option<crate::terminal::Terminal>,
    /// The terminal has the keys, so they go to the shell rather than here.
    pub term_focus: bool,
    /// The file the spot worker was last asked about, so the panel can be
    /// pointed at a new one without asking again every frame.
    pub spot_asked: Option<PathBuf>,
    /// Wheel movement not yet worth a whole line, kept so that it becomes one.
    ///
    /// A frame's smoothed delta is usually a fraction of a row, and truncating
    /// each frame on its own threw all of it away: the view moved only on the
    /// frames that happened to clear a full line, which felt like a wheel that
    /// had to be spun hard for one or two lines. Carrying the remainder makes
    /// every notch arrive.
    pub term_scroll_px: f32,
    /// What the terminal was last searched for, so the key repeats it.
    term_needle: String,
    /// A drag in flight between the panes.
    pub drag: Option<Drag>,
    /// Where each pane was drawn this frame, so a drop can be placed.
    pub pane_rects: Vec<(usize, egui::Rect)>,
    /// What git says about each directory on screen, by directory.
    git_status: Lru<PathBuf, Arc<git::Status>>,
    pub differ: diff::Differ,
    pub spotter: Spotter,
    /// The spot worker's latest findings, for the file named.
    pub spotted: Option<(PathBuf, Vec<Section>)>,

    pub pending: Vec<Key>,
    pub which: Vec<(String, String, String)>,
    pub overlay: Overlay,
    pub pending_bookmark: Option<BookmarkOp>,

    pub yank: Yank,
    pub preview: PreviewSlot,
    pub max_preview: bool,
    /// The quick-look panel is up: the hovered file, big, over the panes.
    pub quick: bool,
    pub hide_parent: bool,
    /// Markdown is shown rendered rather than as source.
    pub render_markdown: bool,
    /// A bold face was loaded as the `bold` font family.
    pub bold_font: bool,
    /// The fonts named in the config have changed and must be installed again.
    /// Only `main` can do that, so it is left as a flag for the frame loop.
    pub refont: bool,

    pub tasks: Vec<Task>,
    pub toasts: Vec<Toast>,
    /// Openers still young enough to fail on us; drained in
    /// [`App::drain_channels`].
    launches: Vec<exec::Launch>,
    pub bookmarks: Vec<Bookmark>,
    /// Where `z` can jump, in visit order (newest last). Ranked by [`frecency`]
    /// when the picker opens.
    pub history: Vec<Visit>,
    pub undos: Undos,
    /// Jobs whose step joins a stack once they finish, and where it goes. A job
    /// that fails puts its step back rather than losing it.
    op_undo: HashMap<u64, (UndoStep, Land)>,

    pub search: Option<crate::search::Handle>,
    pub ctx: egui::Context,
    /// A copy job is blocked waiting for the new name being typed.
    pending_conflict: Option<Sender<Resolution>>,

    dirty: HashMap<PathBuf, Instant>,
    /// Directories whose child count has already been asked for.
    counted: std::collections::HashSet<PathBuf>,
    /// A path completion waiting on the scan pool. Only the newest one
    /// answers; anything the user typed over is dropped.
    pending_completion: Option<PendingCompletion>,
    /// Scans in flight, so results can be routed back to the right folder.
    inflight: HashMap<u64, PathBuf>,
    pub quit: bool,
    pub cwd_file: Option<PathBuf>,
    pub chooser_file: Option<PathBuf>,
    pub help_scroll: usize,
    pub last_action: Instant,
}

impl App {
    pub fn new(cfg: Config, start: PathBuf, ctx: egui::Context) -> Self {
        let wake = {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        // yazi's `micro_workers` is the nearest equivalent knob for "small IO jobs".
        let threads = (cfg.yazi.tasks.micro_workers as usize).clamp(2, 6);
        let scanner = Scanner::new(threads, wake.clone());
        let previewer = Previewer::new(wake.clone());
        let opsr = ops::Runner::new(wake.clone());
        let spotter = Spotter::new(wake.clone());
        let differ = diff::Differ::new(wake.clone());
        let git = git::Git::new(wake.clone());
        let watcher = Watcher::new(wake);

        let sort = SortSpec {
            by: cfg.yazi.mgr.sort_by,
            reverse: cfg.yazi.mgr.sort_reverse,
            dir_first: cfg.yazi.mgr.sort_dir_first,
            sensitive: cfg.yazi.mgr.sort_sensitive,
        };
        let tab = Tab::new(start, sort, cfg.yazi.mgr.show_hidden, cfg.yazi.mgr.linemode.clone());
        let render_markdown = cfg.ui.render_markdown;

        let mut app = Self {
            cfg,
            tabs: vec![tab],
            active: 0,
            split: None,
            cache: Lru::new(64),
            scanner,
            previewer,
            ops: opsr,
            watcher,
            git,
            // One per pane, plus the few a walk just left behind.
            git_status: Lru::new(8),
            term: None,
            term_focus: false,
            spot_asked: None,
            term_scroll_px: 0.0,
            term_needle: String::new(),
            drag: None,
            pane_rects: Vec::new(),
            differ,
            spotter,
            spotted: None,
            pending: Vec::new(),
            which: Vec::new(),
            overlay: Overlay::None,
            pending_bookmark: None,
            yank: Yank { paths: Vec::new(), cut: false },
            preview: PreviewSlot::default(),
            max_preview: false,
            quick: false,
            hide_parent: false,
            render_markdown,
            bold_font: false,
            refont: false,
            tasks: Vec::new(),
            toasts: Vec::new(),
            launches: Vec::new(),
            bookmarks: Vec::new(),
            history: Vec::new(),
            undos: Undos::default(),
            op_undo: HashMap::new(),
            search: None,
            ctx,
            pending_conflict: None,
            dirty: HashMap::new(),
            counted: std::collections::HashSet::new(),
            pending_completion: None,
            inflight: HashMap::new(),
            quit: false,
            cwd_file: None,
            chooser_file: None,
            help_scroll: 0,
            last_action: Instant::now(),
        };
        app.load_state();
        app.kick_scans();
        // A config problem that nobody is told about is one the reader spends
        // the evening blaming the program for. The `~` panel lists them all;
        // this is the line that says to go and look.
        if let Some(w) = app.cfg.warnings.first().cloned() {
            let more = app.cfg.warnings.len() - 1;
            let tail = if more > 0 { format!(" (+{more} more, see `~`)") } else { String::new() };
            app.warn(format!("Config: {w}{tail}"));
        }
        app
    }

    /// Treat the directory the app opened on as unproven, the way a typed `cd`
    /// is: nothing checked it before the window went up, since `is_dir` on a
    /// dead share can hold the first frame for half a minute. If the first
    /// listing fails, the tab falls back to `home` — by way of the parent, so
    /// `filer C:\dir\file.txt` reveals the file instead of giving up.
    pub fn start_unproven(&mut self, home: PathBuf) {
        if self.tabs[self.active].cwd == home {
            return;
        }
        self.tabs[self.active].pending_cd =
            Some(PendingCd { from: home, pushed: false, fallback: true });
    }

    // ------------------------------------------------------------- accessors

    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// The tab in the pane that does not have the keys.
    pub fn other_pane(&self) -> Option<usize> {
        self.split.map(|s| s.other)
    }

    /// Every tab on screen: the focused one, plus the other pane's.
    fn pane_tabs(&self) -> Vec<usize> {
        match self.other_pane() {
            Some(i) if i != self.active && i < self.tabs.len() => vec![self.active, i],
            _ => vec![self.active],
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.raise(text.into(), Level::Info);
    }

    /// Something the reader should see, about work that was not stopped.
    pub fn warn(&mut self, text: impl Into<String>) {
        self.raise(text.into(), Level::Warn);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.raise(text.into(), Level::Error);
    }

    /// Put a line up, or tick the one already saying it.
    ///
    /// Repeating rather than stacking: the same text arriving again is the same
    /// news, and stacking it spends the five slots the toast area has on one
    /// message. The timer restarts so a repeat stays up as long as a first.
    fn raise(&mut self, text: String, level: Level) {
        if let Some(t) = self.toasts.iter_mut().rev().find(|t| t.text == text && t.level == level)
        {
            t.count += 1;
            t.at = Instant::now();
            return;
        }
        self.toasts.push(Toast { text, level, at: Instant::now(), count: 1 });
    }

    // ------------------------------------------------------------- scanning

    /// Ask for any listing the current view needs and does not have.
    pub fn kick_scans(&mut self) {
        let sort = self.tabs[self.active].sort;
        let cwd = self.tabs[self.active].cwd.clone();
        let parent = util::parent_dir(&cwd);

        if self.tabs[self.active].current.scan_id.is_none()
            && self.tabs[self.active].current.state == LoadState::Loading
        {
            let id = self.scanner.scan(cwd.clone(), sort);
            self.inflight.insert(id, cwd.clone());
            self.tabs[self.active].current.scan_id = Some(id);
        }
        if let Some(p) = parent {
            let needs = match &self.tabs[self.active].parent {
                Some(f) => f.scan_id.is_none() && f.state == LoadState::Loading,
                None => false,
            };
            if needs {
                let id = self.scanner.scan_low(p.clone(), sort);
                self.inflight.insert(id, p.clone());
                if let Some(f) = self.tabs[self.active].parent.as_mut() {
                    f.scan_id = Some(id);
                }
            }
        }
        // The other pane is not what the keys drive, so it waits in the low
        // priority queue behind the directory the cursor is in.
        if let Some(idx) = self.other_pane().filter(|&i| i < self.tabs.len() && i != self.active) {
            let f = &self.tabs[idx].current;
            if f.scan_id.is_none() && f.state == LoadState::Loading {
                let (path, sort) = (self.tabs[idx].cwd.clone(), self.tabs[idx].sort);
                let id = self.scanner.scan_low(path.clone(), sort);
                self.inflight.insert(id, path);
                self.tabs[idx].current.scan_id = Some(id);
            }
        }
        self.ensure_dir_sizes();
        self.sync_watcher();
    }

    /// `linemode size` shows a child count for directories; compute it only for
    /// the rows actually on screen, and only once per directory.
    fn ensure_dir_sizes(&mut self) {
        if self.tabs[self.active].linemode != "size" {
            return;
        }
        let tab = &self.tabs[self.active];
        let start = tab.current.offset;
        let end = (start + tab.page_rows + 1).min(tab.current.view.len());
        let mut want = Vec::new();
        for row in start..end {
            let Some(e) = tab.current.at(row) else { continue };
            if e.is_dir_like() && e.dir_size.is_none() && !self.counted.contains(&e.path) {
                want.push(e.path.clone());
            }
        }
        if want.is_empty() {
            return;
        }
        for p in &want {
            self.counted.insert(p.clone());
        }
        self.scanner.count(want);
    }

    fn sync_watcher(&mut self) {
        let mut dirs: Vec<PathBuf> = vec![self.tabs[self.active].cwd.clone()];
        if let Some(p) = self.tabs[self.active].cwd.parent() {
            dirs.push(p.to_path_buf());
        }
        if let Some(idx) = self.other_pane().filter(|&i| i < self.tabs.len()) {
            dirs.push(self.tabs[idx].cwd.clone());
        }
        if let PreviewState::Dir(f) = &self.preview.state {
            dirs.push(f.path.clone());
        }
        let refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
        self.watcher.sync(refs);
    }

    pub fn drain_channels(&mut self, ctx: &egui::Context) {
        while let Ok(res) = self.scanner.rx.try_recv() {
            self.on_scan(res);
        }
        while let Ok(res) = self.previewer.rx.try_recv() {
            self.on_preview(res, ctx);
        }
        while let Ok(res) = self.spotter.rx.try_recv() {
            self.spotted = Some((res.path, res.sections));
        }
        while let Ok(res) = self.differ.rx.try_recv() {
            // An answer to a comparison that has since been closed, or replaced
            // by another pair, has nowhere to go.
            if let Overlay::Diff(ov) = &mut self.overlay {
                if ov.left == res.left && ov.right == res.right {
                    ov.outcome = Some(res.outcome);
                }
            }
        }
        while let Ok(ev) = self.ops.rx.try_recv() {
            self.on_op_event(ev);
        }
        while let Ok(dir) = self.watcher.rx.try_recv() {
            self.dirty.insert(dir, Instant::now());
        }
        while let Ok(rep) = self.git.rx.try_recv() {
            self.git_status.put(rep.dir, Arc::new(rep.status));
        }
        // A launch reports at most once, and its watcher lets go of the channel
        // when it stops caring, so a disconnected one is finished with.
        let mut failed = Vec::new();
        self.launches.retain(|l| match l.rx.try_recv() {
            Ok(msg) => {
                failed.push(msg);
                false
            }
            Err(crossbeam_channel::TryRecvError::Empty) => true,
            Err(crossbeam_channel::TryRecvError::Disconnected) => false,
        });
        for msg in failed {
            self.error(msg);
        }
        self.drain_search();
        self.sync_spot();
        self.pump_terminal();
        self.flush_dirty();
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(6));
        self.tasks.retain(|t| match t.finished {
            Some(at) => at.elapsed() < Duration::from_secs(20) || !t.errors.is_empty(),
            None => true,
        });
    }

    /// Rescan directories the watcher flagged, once they have been quiet for a
    /// moment — editors and installers touch a directory many times in a row.
    fn flush_dirty(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let ready: Vec<PathBuf> = self
            .dirty
            .iter()
            .filter(|(_, t)| t.elapsed() > Duration::from_millis(150))
            .map(|(p, _)| p.clone())
            .collect();
        for p in ready {
            self.dirty.remove(&p);
            self.cache.remove(&p);
            self.rescan(&p);
        }
    }

    fn rescan(&mut self, path: &Path) {
        let sort = self.tabs[self.active].sort;
        let mut wanted = false;
        for idx in self.pane_tabs() {
            if self.tabs[idx].cwd == path {
                wanted = true;
            }
        }
        if let Some(p) = self.tabs[self.active].parent.as_ref() {
            if p.path == path {
                wanted = true;
            }
        }
        if let PreviewState::Dir(f) = &self.preview.state {
            if f.path == path {
                wanted = true;
            }
        }
        if !wanted {
            return;
        }
        let id = self.scanner.scan(path.to_path_buf(), sort);
        self.inflight.insert(id, path.to_path_buf());
    }

    fn on_scan(&mut self, res: ScanResult) {
        match res {
            ScanResult::Listed { id, path, entries } => {
                self.inflight.remove(&id);
                let entries = Arc::new(entries);
                self.cache.put(path.clone(), entries.clone());
                self.apply_listing(&path, entries.clone());
                self.complete_from(id, &path, &entries);
            }
            ScanResult::Failed { id, path, error } => {
                self.inflight.remove(&id);
                // A completion that cannot be listed simply has no answer; the
                // toast below still says why.
                if self.pending_completion.as_ref().is_some_and(|p| p.id == id) {
                    self.pending_completion = None;
                }
                // A jump that was never listed is undone first: the tab goes
                // back where it was instead of showing an empty error column.
                let jumped = self
                    .pane_tabs()
                    .into_iter()
                    .find(|&i| self.tabs[i].cwd == path && self.tabs[i].pending_cd.is_some());
                if let Some(idx) = jumped {
                    self.undo_cd(idx, &path, &error);
                    return;
                }
                let mut hit = false;
                for idx in self.pane_tabs() {
                    if self.tabs[idx].cwd != path {
                        continue;
                    }
                    let show_hidden = self.tabs[idx].show_hidden;
                    let f = &mut self.tabs[idx].current;
                    f.state = LoadState::Error(error.clone());
                    f.entries = Arc::new(Vec::new());
                    f.scan_id = None;
                    f.rebuild(show_hidden);
                    hit = true;
                }
                if !hit {
                    if let PreviewState::Dir(f) = &mut self.preview.state {
                        if f.path == path {
                            f.state = LoadState::Error(error.clone());
                        }
                    }
                }
                self.error(format!("{}: {error}", util::file_name(&path)));
            }
            ScanResult::Counted { counts, .. } => {
                let map: HashMap<&PathBuf, u64> = counts.iter().map(|(p, n)| (p, *n)).collect();
                for tab in &mut self.tabs {
                    let entries = Arc::make_mut(&mut tab.current.entries);
                    for e in entries.iter_mut() {
                        if let Some(n) = map.get(&e.path) {
                            e.dir_size = Some(*n);
                        }
                    }
                }
            }
        }
    }

    fn apply_listing(&mut self, path: &Path, entries: Arc<Vec<Entry>>) {
        // A directory a pane is showing gets its git status asked for. Every
        // rescan comes through here, so a file operation or a change the
        // watcher caught refreshes the marks along with the listing.
        if self.pane_tabs().into_iter().any(|i| self.tabs[i].cwd == path) {
            self.git.request(path.to_path_buf());
        }
        let show_hidden = self.tabs[self.active].show_hidden;
        let memo = self.tabs[self.active].memo.get(path).cloned();

        if self.tabs[self.active].cwd == path {
            // The directory answered, so the jump that led here stands.
            self.tabs[self.active].pending_cd = None;
            let keep = self.tabs[self.active].current.hovered_name().map(str::to_owned);
            let filter = self.tabs[self.active].current.filter.clone();
            let cursor = self.tabs[self.active].current.cursor;
            let offset = self.tabs[self.active].current.offset;
            let f = &mut self.tabs[self.active].current;
            f.entries = entries.clone();
            f.state = LoadState::Ready;
            f.scan_id = None;
            f.filter = filter;
            f.cursor = cursor;
            f.offset = offset;
            f.rebuild(show_hidden);
            if let Some(name) = keep.or(memo) {
                f.select_name(&name);
            }
        }

        let parent_hit = self
            .tabs[self.active]
            .parent
            .as_ref()
            .map(|p| p.path == path)
            .unwrap_or(false);
        if parent_hit {
            let cwd = self.tabs[self.active].cwd.clone();
            if let Some(f) = self.tabs[self.active].parent.as_mut() {
                f.entries = entries.clone();
                f.state = LoadState::Ready;
                f.scan_id = None;
                f.rebuild(show_hidden);
                let name = util::file_name(&cwd);
                f.select_name(&name);
            }
        }

        if let PreviewState::Dir(f) = &mut self.preview.state {
            if f.path == path {
                f.entries = entries.clone();
                f.state = LoadState::Ready;
                f.scan_id = None;
                f.rebuild(show_hidden);
            }
        }

        // Other tabs share the listing but keep their own cursors. The other
        // pane is one of them, and it is on screen, so it needs the same care
        // over the cursor as the focused tab.
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i == self.active || tab.cwd != path {
                continue;
            }
            tab.pending_cd = None;
            let keep = tab.current.hovered_name().map(str::to_owned);
            tab.current.entries = entries.clone();
            tab.current.state = LoadState::Ready;
            tab.current.scan_id = None;
            tab.current.rebuild(tab.show_hidden);
            if let Some(name) = keep.or_else(|| tab.memo.get(path).cloned()) {
                tab.current.select_name(&name);
            }
        }
    }

    // -------------------------------------------------------------- preview

    pub fn request_preview(&mut self, force: bool) {
        let Some(entry) = self.tabs[self.active].current.hovered().cloned() else {
            self.preview.state = PreviewState::Empty;
            self.preview.key = None;
            self.preview.outline = None;
            self.preview.outline_wanted = None;
            return;
        };
        // The outline and the zoom belong to the file they were set on. Without
        // this, walking onto the next image shows a corner of it at 8x.
        if self.preview.key.as_ref().is_none_or(|k| k.path != entry.path) {
            self.preview.outline = None;
            self.preview.zoom = None;
            self.preview.pan = egui::Vec2::ZERO;
        }
        if self.preview.outline_wanted.as_ref().is_some_and(|p| *p != entry.path) {
            self.preview.outline_wanted = None;
        }

        if entry.is_dir_like() {
            let need = match &self.preview.state {
                PreviewState::Dir(f) => f.path != entry.path,
                _ => true,
            };
            if need {
                let show_hidden = self.tabs[self.active].show_hidden;
                let sort = self.tabs[self.active].sort;
                let folder = match self.cache.get(&entry.path) {
                    Some(entries) => Folder::from_entries(entry.path.clone(), entries.clone(), show_hidden),
                    None => {
                        let id = self.scanner.scan_low(entry.path.clone(), sort);
                        self.inflight.insert(id, entry.path.clone());
                        Folder::loading(entry.path.clone(), Some(id))
                    }
                };
                self.preview.state = PreviewState::Dir(folder);
                self.preview.key = None;
                self.preview.texture = None;
                self.tabs[self.active].preview_offset = 0;
                self.sync_watcher();
            }
            return;
        }

        let mime = crate::mime::guess(&entry);
        let key = preview::Key {
            path: entry.path.clone(),
            len: entry.len,
            mtime: entry.modified,
            box_size: self.preview.box_size,
            // Only Markdown is laid out to the pane's width; nothing else
            // needs re-reading when the window is resized.
            cols: if mime == "text/markdown" { self.preview.cols } else { 0 },
        };
        if self.preview.key.as_ref() == Some(&key) && !force {
            return;
        }
        if let Some(hit) = self.preview.cache.get(&key).cloned() {
            self.preview.key = Some(key);
            self.preview.texture = hit.texture;
            self.preview.state = PreviewState::Ready(hit.payload);
            self.preview.max_offset = 0;
            self.grant_outline_wish();
            return;
        }

        // Debounce: while the cursor is still moving, don't touch the disk.
        let debounce = Duration::from_millis(self.cfg.ui.preview_debounce_ms);
        match self.preview.pending_since {
            Some(t) if t.elapsed() >= debounce => {}
            Some(_) => return,
            None => {
                self.preview.pending_since = Some(Instant::now());
                if !debounce.is_zero() {
                    return;
                }
            }
        }
        self.preview.pending_since = None;

        // A re-layout of the file already shown keeps it up until the new
        // one arrives, rather than blinking through "loading" on every resize.
        let relayout = self
            .preview
            .key
            .as_ref()
            .is_some_and(|k| k.path == key.path && k.len == key.len && k.mtime == key.mtime)
            && matches!(
                self.preview.state,
                PreviewState::Ready(
                    // An image re-decoded for a zoom is the same picture at a
                    // different size: blinking through "loading" for it would
                    // be worse than a moment of the softer copy.
                    Payload::Text { .. } | Payload::Markdown { .. } | Payload::Image { .. }
                )
            );
        self.preview.key = Some(key.clone());
        if !relayout {
            self.preview.texture = None;
            self.preview.state = PreviewState::Loading;
        }
        self.preview.request_id = self.previewer.request(preview::Request {
            id: 0,
            key,
            mime,
            ext: entry.ext.clone(),
            max_bytes: self.cfg.ui.max_text_bytes,
            tab_size: self.cfg.yazi.preview.tab_size,
            syntect_theme: self.cfg.theme.syntect_theme.clone(),
        });
    }

    fn on_preview(&mut self, res: preview::Response, ctx: &egui::Context) {
        if self.preview.key.as_ref() != Some(&res.key) {
            return; // stale
        }
        self.preview.texture = match &res.payload {
            Payload::Image { width, height, rgba, .. } => {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [*width as usize, *height as usize],
                    rgba,
                );
                Some(ctx.load_texture("preview", img, egui::TextureOptions::LINEAR))
            }
            _ => None,
        };
        self.preview.cache.put(
            res.key,
            CachedPreview { payload: res.payload.clone(), texture: self.preview.texture.clone() },
        );
        // Restored: v0.5.0 replaced this line with the block below rather than
        // putting the block after it, and the answer has been going nowhere but
        // the cache ever since. The block reads `state` expecting it to be the
        // payload that just arrived, so it was dead too.
        self.preview.state = PreviewState::Ready(res.payload);
        self.preview.max_offset = 0;
        if let PreviewState::Ready(Payload::Image { source, .. }) = &self.preview.state {
            // A seed until the pane draws and says exactly: the box is twice the
            // pane in pixels, so half of it is roughly the points available.
            // Without this a zoom key pressed on this very frame would step from
            // whatever the last image's scale was.
            let (bw, bh) = self.preview.box_size;
            let avail = egui::vec2(bw as f32 / 2.0, bh as f32 / 2.0);
            self.preview.fit = image_fit(avail, source.0 as f32, source.1 as f32);
        }
        self.grant_outline_wish();
    }

    /// The outline of the preview as it is shown: declarations in source
    /// code, or the headings of rendered Markdown.
    pub fn outline_entries(&self) -> &[TocEntry] {
        match &self.preview.state {
            PreviewState::Ready(Payload::Text { outline, .. }) => outline,
            PreviewState::Ready(Payload::Markdown { doc, .. }) if self.render_markdown => &doc.toc,
            _ => &[],
        }
    }

    /// The preview on screen is the hovered file's, fully loaded.
    fn preview_ready(&self) -> bool {
        let hovered = self.tabs[self.active].current.hovered().map(|e| &e.path);
        matches!(self.preview.state, PreviewState::Ready(_))
            && self.preview.key.as_ref().map(|k| &k.path) == hovered
    }

    /// Hand the keys to the outline, starting on the entry being read.
    /// Returns whether there was an outline to hand them to.
    fn focus_outline(&mut self) -> bool {
        if !self.preview_ready() {
            return false;
        }
        let top = self.tabs[self.active].preview_offset;
        let entries = self.outline_entries();
        if entries.is_empty() {
            return false;
        }
        self.preview.outline = Some(entries.iter().rposition(|e| e.line <= top).unwrap_or(0));
        true
    }

    fn toggle_outline(&mut self) {
        if self.preview.outline.take().is_some() {
            return;
        }
        if !self.focus_outline() {
            self.toast("No outline for this file");
        }
    }

    /// `enter` on a file whose preview had not loaded yet: now that it has,
    /// move into its outline if it has one.
    fn grant_outline_wish(&mut self) {
        let wanted = self.preview.outline_wanted.take();
        if wanted.is_some() && wanted.as_ref() == self.preview.key.as_ref().map(|k| &k.path) {
            self.focus_outline();
        }
    }

    /// The 1-based source line outline entry `k` points at, for an editor.
    fn outline_source_line(&self, k: usize) -> Option<usize> {
        let e = self.outline_entries().get(k)?;
        let line = match &self.preview.state {
            PreviewState::Ready(Payload::Markdown { doc, .. }) => doc.src_for_line(e.line),
            _ => e.line,
        };
        Some(line + 1)
    }

    /// While the outline has the keys, moves go through it and scroll the
    /// preview along, and open starts the editor at the entry. Escape and
    /// leave hand the keys back; anything else not about the preview hands
    /// them back and then runs as usual. Returns whether the action was used
    /// up.
    fn outline_act(&mut self, a: &Act) -> bool {
        let Some(cursor) = self.preview.outline else { return false };
        let entries = self.outline_entries();
        if entries.is_empty() {
            self.preview.outline = None;
            return false;
        }
        match a {
            Act::Arrow(step) => {
                let page = self.tabs[self.active].page_rows.max(1);
                let k = step.apply(cursor.min(entries.len() - 1), entries.len(), page);
                let line = entries[k].line;
                self.preview.outline = Some(k);
                self.tabs[self.active].preview_offset = line;
                true
            }
            Act::Seek(_) | Act::MaxPreview => false,
            // Already in the outline.
            Act::Enter => true,
            Act::Open { interactive, .. } => {
                let line = self.outline_source_line(cursor.min(entries.len() - 1));
                self.preview.outline = None;
                self.open(*interactive, line);
                true
            }
            Act::Escape(_) | Act::ToggleOutline | Act::Leave => {
                self.preview.outline = None;
                true
            }
            _ => {
                self.preview.outline = None;
                false
            }
        }
    }

    // ------------------------------------------------------------------ spot

    fn open_spot(&mut self) {
        if self.tabs[self.active].current.hovered().is_none() {
            return;
        }
        self.overlay = Overlay::Spot(SpotOverlay { cursor: 0, scroll: 0 });
        self.sync_spot();
    }

    /// Keep the worker pointed at the file under the cursor.
    ///
    /// The panel is a page about one file, so it has to be about the hovered
    /// one -- and the cursor moves in ways the panel does not hear directly.
    /// `h` and `l` change directory, and what ends up hovered there is decided
    /// by a listing that arrives frames later on a worker thread, so asking at
    /// the moment the key is pressed would ask about nothing. Called once a
    /// frame as well, which is what makes that case work.
    fn sync_spot(&mut self) {
        if !matches!(self.overlay, Overlay::Spot(_)) {
            self.spot_asked = None;
            return;
        }
        let Some(path) = self.tabs[self.active].current.hovered().map(|e| e.path.clone()) else {
            return;
        };
        if self.spot_asked.as_ref() == Some(&path) {
            return;
        }
        self.spot_asked = Some(path.clone());
        self.spotter.request(path);
    }

    /// What the spot panel shows for the hovered file: the listing's facts,
    /// what its preview found, then the worker's findings once they are in.
    pub fn spot_sections(&self) -> Vec<Section> {
        let Some(entry) = self.tabs[self.active].current.hovered() else { return Vec::new() };
        let mut out = vec![spot::base(entry)];
        if self.preview_ready() {
            if let PreviewState::Ready(p) = &self.preview.state {
                out.extend(self.preview_section(p));
            }
        }
        if let Some((path, found)) = &self.spotted {
            if *path == entry.path {
                out.extend(found.iter().cloned());
            }
        }
        out
    }

    fn preview_section(&self, payload: &Payload) -> Option<Section> {
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut row = |k: &str, v: String| rows.push((k.into(), v));
        let read = |truncated: bool| {
            truncated.then(|| format!("first {} only", util::human_size(self.cfg.ui.max_text_bytes as u64)))
        };
        match payload {
            Payload::Text { total_lines, truncated, outline, .. } => {
                row("Lines", total_lines.to_string());
                if !outline.is_empty() {
                    row("Outline", format!("{} entries", outline.len()));
                }
                if let Some(r) = read(*truncated) {
                    row("Read", r);
                }
            }
            Payload::Markdown { doc, total_lines, truncated, .. } => {
                row("Lines", total_lines.to_string());
                row("Headings", doc.toc.len().to_string());
                if let Some(r) = read(*truncated) {
                    row("Read", r);
                }
            }
            Payload::Image { caption, .. } if !caption.is_empty() => row("Shows", caption.clone()),
            Payload::Meta { rows: meta } => rows.extend(meta.iter().cloned()),
            Payload::Error(e) => row("Error", e.clone()),
            _ => {}
        }
        (!rows.is_empty()).then(|| Section { title: "Preview".into(), rows })
    }

    pub fn feed_tasks_key(&mut self, k: Key) {
        self.pending.push(k);
        let bindings = &self.cfg.keymap.tasks;
        match keymap::resolve(bindings, &self.pending) {
            keymap::Match::Exact(b) => {
                let acts = b.run.clone();
                self.pending.clear();
                for a in acts {
                    self.tasks_act(a);
                }
            }
            keymap::Match::Pending(_) => {}
            keymap::Match::None => self.pending.clear(),
        }
    }

    /// The task panel's own commands: move, pause, cancel, reorder.
    fn tasks_act(&mut self, a: Act) {
        let len = self.tasks.len();
        let page = self.tabs[self.active].page_rows.max(1);
        let Overlay::Tasks(ov) = &mut self.overlay else { return };
        match a {
            Act::Close | Act::Escape(_) | Act::TasksShow | Act::Quit => {
                self.overlay = Overlay::None;
            }
            Act::Arrow(step) if len > 0 => ov.cursor = step.apply(ov.cursor, len, page),
            Act::TaskToggle => self.toggle_task(),
            Act::TaskCancel => self.cancel_task(),
            Act::TaskTop => self.promote_task(),
            _ => {}
        }
    }

    /// The job the panel's cursor is on, when there is one.
    fn selected_task(&self) -> Option<(u64, TaskState)> {
        let Overlay::Tasks(ov) = &self.overlay else { return None };
        self.tasks.get(ov.cursor).map(|t| (t.id, t.state.clone()))
    }

    fn toggle_task(&mut self) {
        let Some((id, state)) = self.selected_task() else { return };
        match state {
            // A queued job has not started, so there is nothing to park.
            TaskState::Queued => self.toast("That job has not started yet"),
            TaskState::Running => self.ops.control(id, ops::Control::Pause),
            TaskState::Paused => self.ops.control(id, ops::Control::Resume),
            _ => {}
        }
    }

    fn cancel_task(&mut self) {
        let Some((id, state)) = self.selected_task() else { return };
        if !state.is_live() {
            return;
        }
        // A job still in the queue never reaches the worker, so the panel
        // closes it out itself; a running one is told to stop and answers
        // with its own `Finished`.
        if self.ops.drop_queued(id) {
            if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                t.state = TaskState::Cancelled;
                t.finished = Some(Instant::now());
            }
            return;
        }
        self.ops.control(id, ops::Control::Cancel);
    }

    fn promote_task(&mut self) {
        let Some((id, state)) = self.selected_task() else { return };
        if state != TaskState::Queued {
            self.toast("Only a queued job can be moved up");
            return;
        }
        if !self.ops.promote(id) {
            return;
        }
        // The panel lists jobs in the order they were made, so say what
        // happened rather than leaving the row where it was.
        self.toast("Moved to the front of the queue");
    }

    pub fn feed_spot_key(&mut self, k: Key) {
        self.pending.push(k);
        let bindings = &self.cfg.keymap.spot;
        match keymap::resolve(bindings, &self.pending) {
            keymap::Match::Exact(b) => {
                let acts = b.run.clone();
                self.pending.clear();
                for a in acts {
                    self.spot_act(a);
                }
            }
            keymap::Match::Pending(_) => {}
            keymap::Match::None => self.pending.clear(),
        }
    }

    /// yazi's spot commands: close, arrow (rows), swipe (files), copy (cell).
    fn spot_act(&mut self, a: Act) {
        let rows: usize = self.spot_sections().iter().map(|s| s.rows.len()).sum();
        let page = self.tabs[self.active].page_rows.max(1);
        let Overlay::Spot(ov) = &mut self.overlay else { return };
        match a {
            Act::Close | Act::Escape(_) | Act::Spot | Act::Quit => self.overlay = Overlay::None,
            Act::Arrow(step) => ov.cursor = step.apply(ov.cursor, rows, page),
            Act::Swipe(n) => {
                self.act(Act::Arrow(Step::Rel(n)));
                self.sync_spot();
            }
            // `h` and `l` change directory, as they do in the list and so as
            // they do under `<F3>`, which keeps the list's own keys live.
            Act::Leave | Act::Enter => {
                // In the list, `enter` on a plain file focuses the preview's
                // outline -- another panel wanting these same keys. With the
                // spotter open only a directory is worth moving into.
                let into_dir =
                    self.tabs[self.active].current.hovered().is_some_and(|e| e.is_dir_like());
                if matches!(a, Act::Enter) && !into_dir {
                    return;
                }
                self.act(a);
                // The listing is still on its way, so the file this lands on
                // is not known yet; the once-a-frame call catches it.
                self.sync_spot();
            }
            Act::Copy(_) => {
                let cursor = ov.cursor;
                let sections = self.spot_sections();
                let Some((key, value)) = sections.iter().flat_map(|s| &s.rows).nth(cursor) else { return };
                match exec::set_clipboard(value) {
                    Ok(()) => self.toast(format!("Copied {key}: {value}")),
                    Err(err) => self.error(format!("Clipboard: {err}")),
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ navigation

    pub fn cd(&mut self, target: PathBuf, push_history: bool) {
        self.cd_inner(target, push_history, false);
    }

    /// `cd` for a path someone typed. If it turns out to name a file, the tab
    /// lands on the parent with that file under the cursor.
    fn cd_or_reveal(&mut self, target: PathBuf) {
        self.cd_inner(target, true, true);
    }

    /// Move the focused tab to `target`.
    ///
    /// No directory check happens here: on a share that stopped answering,
    /// `is_dir` can block for half a minute, and the UI thread cannot wait.
    /// The tab moves at once and the background scan has the last word —
    /// `Tab::cd_failed` decides what to do when the listing never arrives.
    fn cd_inner(&mut self, target: PathBuf, push_history: bool, fallback: bool) {
        let target = util::normalize(&target);
        if self.tabs[self.active].cwd == target {
            return;
        }
        let from = self.tabs[self.active].cwd.clone();
        {
            let tab = &mut self.tabs[self.active];
            tab.remember_cursor();
            if push_history {
                tab.back.push(from.clone());
                tab.forward.clear();
            }
            tab.visual = None;
            tab.mouse_range = None;
            tab.finder = None;
        }

        let active = self.active;
        let pending = PendingCd { from, pushed: push_history, fallback };
        self.arrive(active, target.clone(), Some(pending));

        self.remember_history(&target);
        self.kick_scans();
        // Even a cache hit gets a background refresh, so the view is never stale.
        if self.cache.peek(&target).is_some() {
            let sort = self.tabs[self.active].sort;
            let id = self.scanner.scan(target.clone(), sort);
            self.inflight.insert(id, target);
        }
    }

    /// Put a tab on `target` and show whatever the cache already holds.
    /// `pending` survives only while the listing is still missing: a cached
    /// directory was listed before and needs no second opinion.
    fn arrive(&mut self, idx: usize, target: PathBuf, pending: Option<PendingCd>) {
        let show_hidden = self.tabs[idx].show_hidden;
        let cur = match self.cache.get(&target) {
            Some(entries) => Folder::from_entries(target.clone(), entries.clone(), show_hidden),
            None => Folder::loading(target.clone(), None),
        };
        self.tabs[idx].cwd = target.clone();
        self.tabs[idx].current = cur;
        self.tabs[idx].preview_offset = 0;
        self.tabs[idx].recall_cursor();

        let parent = util::parent_dir(&target);
        self.tabs[idx].parent = parent.map(|p| match self.cache.get(&p) {
            Some(entries) => {
                let mut f = Folder::from_entries(p.clone(), entries.clone(), show_hidden);
                f.select_name(&util::file_name(&target));
                f
            }
            None => Folder::loading(p, None),
        });

        let listed = self.tabs[idx].current.state != LoadState::Loading;
        self.tabs[idx].pending_cd = pending.filter(|_| !listed);

        if idx == self.active {
            self.preview.state = PreviewState::Empty;
            self.preview.key = None;
            self.preview.texture = None;
            self.preview.pending_since = None;
        }
    }

    /// A jump whose listing never arrived: undo it rather than leave the tab
    /// parked on a path that did not answer.
    fn undo_cd(&mut self, idx: usize, path: &Path, error: &str) {
        match self.tabs[idx].cd_failed() {
            CdFallout::Keep => {}
            CdFallout::Reveal { to, pending } => {
                self.arrive(idx, to, Some(pending));
                self.kick_scans();
            }
            CdFallout::Revert { to } => {
                self.arrive(idx, to, None);
                self.kick_scans();
                self.error(format!("{}: {error}", path.display()));
            }
        }
    }

    /// Count a visit. The hit count survives the move to the end of the list,
    /// so a directory worked in every day climbs even though each visit looks
    /// like the last one.
    fn remember_history(&mut self, path: &Path) {
        let hits = match self.history.iter().position(|v| v.path == path) {
            Some(i) => self.history.remove(i).hits.saturating_add(1),
            None => 1,
        };
        self.history.push(Visit { path: path.to_path_buf(), hits, at: epoch_secs() });
        let max = self.cfg.ui.max_history;
        if self.history.len() > max {
            let cut = self.history.len() - max;
            self.history.drain(..cut);
        }
    }

    pub fn exit_search_view(&mut self) {
        self.search = None;
        let cwd = self.tabs[self.active].cwd.clone();
        let show_hidden = self.tabs[self.active].show_hidden;
        let folder = match self.cache.get(&cwd) {
            Some(entries) => Folder::from_entries(cwd.clone(), entries.clone(), show_hidden),
            None => Folder::loading(cwd.clone(), None),
        };
        self.tabs[self.active].current = folder;
        self.tabs[self.active].finder = None;
        self.tabs[self.active].recall_cursor();
        self.kick_scans();
    }

    fn enter(&mut self) {
        let Some(entry) = self.tabs[self.active].current.hovered().cloned() else { return };
        if entry.is_dir_like() {
            self.cd(entry.path, true);
        } else if !self.focus_outline() && !self.preview_ready() {
            // Still loading: move in once it arrives. A file without an
            // outline just stays where it is; opening is `open`'s job.
            self.preview.outline_wanted = Some(entry.path);
        }
    }

    fn leave(&mut self) {
        if self.in_search_view() {
            self.exit_search_view();
            return;
        }
        let cwd = self.tabs[self.active].cwd.clone();
        if let Some(p) = util::parent_dir(&cwd) {
            let name = util::file_name(&cwd);
            self.tabs[self.active].memo.insert(p.clone(), name);
            self.cd(p, true);
        }
    }

    // ------------------------------------------------------------- dispatch

    pub fn run(&mut self, acts: &[Act]) {
        for a in acts {
            self.act(a.clone());
        }
    }

    pub fn act(&mut self, a: Act) {
        self.last_action = Instant::now();
        if self.outline_act(&a) {
            return;
        }
        match a {
            Act::Noop | Act::Unsupported(_) => {
                if let Act::Unsupported(what) = a {
                    self.error(format!("Not supported: {what}"));
                }
            }
            Act::Escape(what) => self.escape(what),
            Act::Quit => self.quit = true,
            Act::Close => {
                if self.tabs.len() > 1 {
                    self.close_tab(self.active);
                } else {
                    self.quit = true;
                }
            }

            Act::Swipe(n) => self.act(Act::Arrow(Step::Rel(n))),
            Act::Arrow(step) => {
                let page = self.tabs[self.active].page_rows.max(1);
                self.tabs[self.active].current.arrow(step, page);
                self.tabs[self.active].sync_visual();
                self.preview.pending_since = None;
            }
            Act::Leave => self.leave(),
            Act::Enter => self.enter(),
            Act::Back => {
                if let Some(p) = self.tabs[self.active].back.pop() {
                    let cur = self.tabs[self.active].cwd.clone();
                    self.tabs[self.active].forward.push(cur);
                    self.cd(p, false);
                }
            }
            Act::Forward => {
                if let Some(p) = self.tabs[self.active].forward.pop() {
                    let cur = self.tabs[self.active].cwd.clone();
                    self.tabs[self.active].back.push(cur);
                    self.cd(p, false);
                }
            }
            Act::Cd { target, interactive } => {
                if interactive || target.is_empty() {
                    let cwd = self.tabs[self.active].cwd.clone();
                    self.open_input(InputKind::Cd, "Change directory", format!("{}\\", cwd.display()));
                } else {
                    let base = self.tabs[self.active].cwd.clone();
                    self.cd(util::resolve_against(&base, &target), true);
                }
            }
            Act::Reveal(target) => {
                let base = self.tabs[self.active].cwd.clone();
                let p = util::resolve_against(&base, &target);
                let name = util::file_name(&p);
                if let Some(dir) = p.parent() {
                    self.cd(dir.to_path_buf(), true);
                    let cwd = self.tabs[self.active].cwd.clone();
                    self.tabs[self.active].memo.insert(cwd, name.clone());
                    self.tabs[self.active].current.select_name(&name);
                }
            }
            Act::Follow => self.follow_link(),
            Act::Refresh => {
                let cwd = self.tabs[self.active].cwd.clone();
                self.cache.remove(&cwd);
                self.preview.cache.clear();
                self.counted.clear();
                self.rescan(&cwd);
                self.request_preview(true);
            }

            Act::Seek(step) => {
                let page = self.tabs[self.active].page_rows.max(1) as i64;
                let delta = match step {
                    Step::Rel(n) => n,
                    Step::Pct(p) => page * p / 100,
                    Step::Top => i64::MIN / 2,
                    Step::Bot => i64::MAX / 2,
                };
                // Clamped here rather than only after the draw. The draw used
                // to be handed an offset past the end, paint a frame from
                // beyond the content, and fix the number afterwards — one bad
                // frame per keypress at the bottom of a file, which is why it
                // took a held-down key to see.
                let cur = self.tabs[self.active].preview_offset as i64;
                let want = cur.saturating_add(delta).max(0) as usize;
                self.tabs[self.active].preview_offset = want.min(self.preview.max_offset);
            }

            Act::TabCreate { current, path } => self.create_tab(current, path),
            Act::TabClose(n) => {
                let idx = n.unwrap_or(self.active);
                self.close_tab(idx);
            }
            Act::TabSwitch { n, relative } => {
                let len = self.tabs.len() as i64;
                let idx = if relative {
                    (self.active as i64 + n).rem_euclid(len)
                } else {
                    n.clamp(0, len - 1)
                } as usize;
                if self.other_pane() == Some(idx) {
                    // It is already on screen: move the keys to its pane.
                    self.focus_pane(idx);
                } else {
                    self.switch_tab(idx);
                }
            }
            Act::TabSwap(n) => {
                let len = self.tabs.len() as i64;
                let from = self.active;
                let to = (self.active as i64 + n).rem_euclid(len) as usize;
                self.tabs.swap(from, to);
                self.active = to;
                if let Some(sp) = self.split {
                    self.split =
                        Some(Split { other: split_after_swap(sp.other, from, to), ..sp });
                }
                self.check_split();
            }

            Act::Split(state) => {
                if state.unwrap_or(self.split.is_none()) {
                    self.open_split();
                } else {
                    self.close_split();
                }
            }
            Act::PaneFocus(side) => {
                // The first press splits the view; the next moves the keys.
                self.open_split();
                if let Some(sp) = self.split {
                    if side.unwrap_or(!sp.right) != sp.right {
                        self.focus_pane(sp.other);
                    }
                }
            }

            Act::Toggle { state } => {
                self.tabs[self.active].toggle(state);
            }
            Act::ToggleAll { state } => self.tabs[self.active].toggle_all(state),
            Act::VisualMode { unset } => {
                if self.tabs[self.active].visual.is_some() {
                    self.tabs[self.active].leave_visual();
                } else {
                    self.tabs[self.active].enter_visual(unset);
                }
            }

            Act::Open { interactive, hovered } => {
                let _ = hovered;
                self.open(interactive, None);
            }
            Act::Yank { cut } => self.yank(cut),
            Act::Unyank => {
                self.yank.paths.clear();
                self.yank.cut = false;
            }
            Act::Paste { force, follow } => self.paste(force, follow),
            Act::Link { relative } => self.link(OpKind::Symlink { relative }),
            Act::Hardlink => self.link(OpKind::Hardlink),
            Act::Remove { permanently, force, hovered } => {
                self.remove(permanently, force, hovered)
            }
            Act::Create { dir, force } => {
                let _ = force;
                let title = if dir { "Create directory" } else { "Create (end with / for a directory)" };
                self.open_input(InputKind::Create, title, String::new());
            }
            Act::Rename { force, cursor } => {
                let _ = force;
                self.start_rename(cursor);
            }
            Act::Copy(what) => self.copy_text(what),
            Act::Shell { run, block, confirm, orphan } => {
                if run.is_empty() || confirm {
                    self.open_input(InputKind::Shell { block }, "Shell", run);
                } else {
                    self.run_shell(&run, block, orphan);
                }
            }

            Act::Hidden(state) => {
                let tab = &mut self.tabs[self.active];
                tab.show_hidden = state.unwrap_or(!tab.show_hidden);
                let show = tab.show_hidden;
                tab.current.rebuild(show);
                if let Some(p) = tab.parent.as_mut() {
                    p.rebuild(show);
                }
                if let PreviewState::Dir(f) = &mut self.preview.state {
                    f.rebuild(show);
                }
            }
            Act::Linemode(m) => self.tabs[self.active].linemode = m,
            Act::Sort { by, reverse, dir_first } => self.sort(by, reverse, dir_first),

            Act::Find { prev, smart, insensitive } => {
                let _ = (smart, insensitive);
                self.open_input(InputKind::Find { prev }, if prev { "Find previous" } else { "Find next" }, String::new());
            }
            Act::FindArrow { prev } => self.find_arrow(prev),
            Act::Filter { smart, insensitive } => {
                let _ = (smart, insensitive);
                let current = self
                    .tabs[self.active]
                    .current
                    .filter
                    .as_ref()
                    .map(|f| f.query.clone())
                    .unwrap_or_default();
                self.open_input(InputKind::Filter, "Filter", current);
            }
            Act::Search { via, .. } => {
                self.open_input(InputKind::Search { via }, match via {
                    SearchVia::Name => "Search by name",
                    SearchVia::Content => "Search by content",
                }, String::new());
            }
            Act::Submit => self.submit_input(),
            Act::Complete => self.complete_input(),
            Act::Jump => self.open_jump(),
            Act::Undo => self.undo_step(),
            Act::Redo => self.redo_step(),
            Act::ConfigReload => self.reload_config(),
            // Deliberately not an overlay: with the panel up, every key still
            // works, so `j` and `k` walk the list and the panel follows.
            Act::Quick(state) => self.quick = state.unwrap_or(!self.quick),
            Act::Zoom(to) => self.zoom_preview(to),
            Act::Minimap(state) => {
                self.cfg.ui.minimap = state.unwrap_or(!self.cfg.ui.minimap);
            }
            Act::BulkRename => self.start_bulk_rename(),
            Act::Compare => self.start_compare(),

            Act::Help => {
                self.help_scroll = 0;
                self.overlay = Overlay::Help;
            }
            // Opening a browser is a visible thing to do to someone's machine,
            // so it says what it did rather than leaving a window to appear
            // from nowhere.
            Act::BugReport => match exec::open_url(&crate::bugreport::url()) {
                Ok(()) => self.toast("Opened a bug report in your browser"),
                Err(e) => self.error(format!("could not open the browser: {e}")),
            },
            Act::TasksShow => self.overlay = Overlay::Tasks(TasksOverlay { cursor: 0 }),
            // These act on the row the task panel has under its cursor, so
            // they open it first when it is not the overlay in front.
            Act::TaskToggle | Act::TaskCancel | Act::TaskTop => {
                if !matches!(self.overlay, Overlay::Tasks(_)) {
                    self.overlay = Overlay::Tasks(TasksOverlay { cursor: 0 });
                }
                self.tasks_act(a);
            }
            Act::Spot => self.open_spot(),
            Act::Palette => self.open_palette(),
            Act::Menu => self.open_menu(),
            Act::Terminal(what) => self.terminal(what),
            Act::TermSend => self.term_send_paths(),
            Act::TermCd => self.term_pull_cwd(),
            Act::TermFind { prev, repeat } => {
                let needle = self.term_needle.clone();
                match repeat && !needle.is_empty() {
                    // `--prev` walks back towards the bottom, because the
                    // search itself runs the other way: what you are looking
                    // for in a terminal has scrolled off the top.
                    true => self.term_find(&needle, !prev),
                    // Nothing to repeat, so ask what to look for.
                    false => self.open_input(InputKind::TermFind, "Find in terminal", needle),
                }
            }
            Act::TermScroll(step) => {
                if let Some(t) = &self.term {
                    use alacritty_terminal::grid::Scroll;
                    let page = t.size().lines as i64;
                    // Negated, because the two conventions run opposite ways.
                    // `term_scroll -50%` reads like `arrow -50%` and means
                    // half a screen back, while alacritty counts a positive
                    // delta as older. Taking the number as it stood sent
                    // `<S-PageUp>` towards the bottom, where there is nothing
                    // to go to, so the one key most likely to be tried first
                    // did nothing at all.
                    t.scroll(match step {
                        Step::Top => Scroll::Top,
                        Step::Bot => Scroll::Bottom,
                        Step::Rel(n) => Scroll::Delta(-(n as i32)),
                        Step::Pct(p) => Scroll::Delta(-((page * p / 100) as i32)),
                    });
                }
            }
            Act::Extract => self.do_extract(),
            Act::Compress => self.ask_compress(),
            Act::SendPane { cut } => self.send_to_pane(cut),
            Act::ToggleOutline => self.toggle_outline(),
            Act::ToggleRender => {
                self.render_markdown = !self.render_markdown;
                // Stay on the same part of the document across the switch.
                if let PreviewState::Ready(Payload::Markdown { doc, .. }) = &self.preview.state {
                    let tab = &mut self.tabs[self.active];
                    tab.preview_offset = if self.render_markdown {
                        doc.line_for_src(tab.preview_offset)
                    } else {
                        doc.src_for_line(tab.preview_offset)
                    };
                }
            }

            Act::MaxPreview => {
                self.max_preview = !self.max_preview;
                if self.max_preview {
                    self.hide_parent = false;
                }
            }
            Act::TogglePaneParent => self.hide_parent = !self.hide_parent,
            Act::BookmarkSave => self.pending_bookmark = Some(BookmarkOp::Save),
            Act::BookmarkJump => self.pending_bookmark = Some(BookmarkOp::Jump),
            Act::BookmarkList => self.open_bookmark_list(),
            Act::BookmarkDelete => self.pending_bookmark = Some(BookmarkOp::Delete),
            Act::BookmarkDeleteAll => {
                self.overlay = Overlay::Confirm(ConfirmOverlay {
                    title: "Delete all bookmarks?".into(),
                    body: vec![format!("{} bookmarks will be removed.", self.bookmarks.len())],
                    options: vec![('y', "Yes".into()), ('n', "No".into())],
                    action: ConfirmAction::BookmarkDeleteAll,
                    dest: None,
                });
            }
        }
    }

    fn escape(&mut self, what: EscapeWhat) {
        if !self.overlay.is_none() {
            self.cancel_input();
            return;
        }
        if self.pending_bookmark.take().is_some() {
            return;
        }
        // The panel is the thing in front of everything else, so it goes first.
        if self.quick {
            self.quick = false;
            return;
        }
        let all = what.everything();
        if (all || what.search) && self.in_search_view() {
            self.exit_search_view();
            return;
        }
        let tab = &mut self.tabs[self.active];
        if (all || what.visual) && tab.leave_visual() {
            return;
        }
        if (all || what.filter) && tab.current.filter.is_some() {
            tab.current.filter = None;
            let show = tab.show_hidden;
            tab.current.rebuild(show);
            return;
        }
        if (all || what.find || what.search) && tab.finder.take().is_some() {
            return;
        }
        if all || what.select {
            tab.clear_selection();
        }
    }

    // ----------------------------------------------------------------- tabs

    /// Open a tab on `path`, on the hovered directory, or on home.
    ///
    /// Nothing checks the target first, for the reason `cd_inner` gives: the
    /// tab opens where it was asked to and the background scan has the last
    /// word. A listing that never arrives sends the new tab back to the
    /// directory it was opened from, rather than closing it.
    fn create_tab(&mut self, current: bool, path: Option<String>) {
        if self.tabs.len() >= MAX_TABS {
            self.error("Maximum number of tabs reached");
            return;
        }
        let base = self.tabs[self.active].cwd.clone();
        let (target, fallback) = new_tab_target(
            &base,
            current,
            path.as_deref(),
            self.tabs[self.active].current.hovered(),
            dirs::home_dir(),
        );
        let tab = Tab::new(
            target.clone(),
            self.tabs[self.active].sort,
            self.tabs[self.active].show_hidden,
            self.tabs[self.active].linemode.clone(),
        );
        let at = self.active + 1;
        self.tabs.insert(at, tab);
        if let Some(sp) = self.split {
            // The other pane keeps its tab, which the insert may have moved.
            if sp.other >= at {
                self.split = Some(Split { other: sp.other + 1, ..sp });
            }
        }
        self.switch_tab(at);
        // `switch_tab` fills the tab from the cache when the directory has been
        // listed before; one that is still loading has yet to prove it exists.
        if target != base && self.tabs[at].current.state == LoadState::Loading {
            self.tabs[at].pending_cd = Some(PendingCd { from: base, pushed: false, fallback });
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() <= 1 {
            self.quit = true;
            return;
        }
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if let Some(sp) = self.split {
            self.split = split_after_remove(sp.other, idx).map(|other| Split { other, ..sp });
        }
        self.switch_tab(self.active.min(self.tabs.len() - 1));
        self.check_split();
    }

    // ----------------------------------------------------------- split panes

    /// Open the second pane. The current tab keeps the keys on the left; the
    /// right pane takes the next tab, or a fresh view of the same directory
    /// when this is the only tab.
    fn open_split(&mut self) -> bool {
        if self.split.is_some() {
            return true;
        }
        let other = if self.tabs.len() > 1 {
            (self.active + 1) % self.tabs.len()
        } else if self.tabs.len() >= MAX_TABS {
            self.error("Maximum number of tabs reached");
            return false;
        } else {
            let src = &self.tabs[self.active];
            let name = src.current.hovered_name().map(str::to_owned);
            let mut tab =
                Tab::new(src.cwd.clone(), src.sort, src.show_hidden, src.linemode.clone());
            // The listing is already in hand, so the new pane starts filled.
            if let Some(entries) = self.cache.get(&tab.cwd).cloned() {
                let show = tab.show_hidden;
                tab.current = Folder::from_entries(tab.cwd.clone(), entries, show);
                if let Some(name) = name {
                    tab.current.select_name(&name);
                }
            }
            let at = self.active + 1;
            self.tabs.insert(at, tab);
            at
        };
        self.split = Some(Split { other, right: false });
        self.kick_scans();
        true
    }

    /// Close the second pane. Its tab stays open, just no longer on screen.
    fn close_split(&mut self) {
        self.split = None;
    }

    /// Give the keys to the pane showing `idx`, which must be the other pane.
    pub fn focus_pane(&mut self, idx: usize) {
        let Some(sp) = self.split else { return };
        if sp.other != idx || idx == self.active || idx >= self.tabs.len() {
            return;
        }
        self.split = Some(Split { other: self.active, right: !sp.right });
        self.switch_tab(idx);
    }

    /// Drop the split once its two tabs stop being a pair.
    fn check_split(&mut self) {
        if let Some(sp) = self.split {
            if sp.other >= self.tabs.len() || sp.other == self.active {
                self.split = None;
            }
        }
    }

    fn switch_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active {
            self.active = idx.min(self.tabs.len() - 1);
        } else {
            self.active = idx;
        }
        let show_hidden = self.tabs[self.active].show_hidden;
        let cwd = self.tabs[self.active].cwd.clone();
        if let Some(entries) = self.cache.get(&cwd).cloned() {
            if self.tabs[self.active].current.state == LoadState::Loading {
                let mut f = Folder::from_entries(cwd.clone(), entries, show_hidden);
                if let Some(name) = self.tabs[self.active].memo.get(&cwd) {
                    f.select_name(name);
                }
                self.tabs[self.active].current = f;
            }
        }
        if let Some(p) = cwd.parent().map(Path::to_path_buf) {
            let needs = self
                .tabs[self.active]
                .parent
                .as_ref()
                .map(|f| f.state == LoadState::Loading)
                .unwrap_or(true);
            if needs {
                if let Some(entries) = self.cache.get(&p).cloned() {
                    let mut f = Folder::from_entries(p, entries, show_hidden);
                    f.select_name(&util::file_name(&cwd));
                    self.tabs[self.active].parent = Some(f);
                }
            }
        }
        self.preview.state = PreviewState::Empty;
        self.preview.key = None;
        self.preview.texture = None;
        self.kick_scans();
    }

    // ----------------------------------------------------------- operations

    fn yank(&mut self, cut: bool) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        self.yank = Yank { paths, cut };
        self.toast(format!("Yanked {n} item(s){}", if cut { " (cut)" } else { "" }));
    }

    fn paste(&mut self, force: bool, _follow: bool) {
        if self.yank.paths.is_empty() {
            self.error("Nothing to paste");
            return;
        }
        let dest = self.tabs[self.active].cwd.clone();
        let kind = if self.yank.cut { OpKind::Move } else { OpKind::Copy };
        let srcs = self.yank.paths.clone();
        self.submit_op(kind, srcs, dest, force);
        if self.yank.cut {
            self.yank.paths.clear();
            self.yank.cut = false;
        }
    }

    /// Finish a drag. `onto` is the tab the pointer was over when it was let
    /// go, and `cut` is whether the move modifier was held.
    ///
    /// A drop needs somewhere to land, so it does nothing unless the pointer
    /// ended over a different pane: dropping a file back where it came from
    /// should be the no-op it looks like.
    pub fn drop_drag(&mut self, onto: Option<usize>, cut: bool) {
        let Some(drag) = self.drag.take() else { return };
        let Some(onto) = onto.filter(|&i| i != drag.from && i < self.tabs.len()) else { return };
        let dest = self.tabs[onto].cwd.clone();
        if dest == self.tabs[drag.from].cwd {
            return;
        }
        let kind = if cut { OpKind::Move } else { OpKind::Copy };
        self.submit_op(kind, drag.paths, dest, false);
        self.tabs[drag.from].clear_selection();
    }

    /// Start a drag on `row` in `tab`. The selection travels when the row is
    /// part of it; otherwise it is that one file, the way a drag usually works.
    pub fn start_drag(&mut self, tab: usize, row: usize) {
        let Some(entry) = self.tabs[tab].current.at(row).cloned() else { return };
        let selected = self.tabs[tab].selected.contains(&entry.path);
        let paths: Vec<PathBuf> = match selected {
            true => self.tabs[tab].selected.iter().cloned().collect(),
            false => vec![entry.path.clone()],
        };
        let label = match paths.len() {
            1 => entry.name.clone(),
            n => format!("{n} items"),
        };
        self.drag = Some(Drag { from: tab, paths, label });
    }

    /// Copy or move the selection into the other pane in one keypress.
    ///
    /// The same job a yank and a paste would raise, with the other pane's
    /// directory as the destination — the point being that the destination is
    /// already on screen, so naming it again is the step worth removing. The
    /// yank register is left alone: this is not a yank.
    fn send_to_pane(&mut self, cut: bool) {
        let Some(other) = self.other_pane().filter(|&i| i < self.tabs.len()) else {
            self.error("Open the second pane first (<C-w>)");
            return;
        };
        let srcs = self.tabs[self.active].targets();
        if srcs.is_empty() {
            return;
        }
        let dest = self.tabs[other].cwd.clone();
        // Into the directory it is already in: the copy would land beside
        // itself under another name, which is never what this key meant.
        if dest == self.tabs[self.active].cwd {
            self.error("Both panes are in the same directory");
            return;
        }
        let kind = if cut { OpKind::Move } else { OpKind::Copy };
        self.submit_op(kind, srcs, dest, false);
        self.tabs[self.active].clear_selection();
    }

    fn link(&mut self, kind: OpKind) {
        if self.yank.paths.is_empty() {
            self.error("Nothing to link; yank something first");
            return;
        }
        let dest = self.tabs[self.active].cwd.clone();
        let srcs = self.yank.paths.clone();
        self.submit_op(kind, srcs, dest, false);
    }

    fn remove(&mut self, permanently: bool, force: bool, hovered: bool) {
        let paths = if hovered {
            self.tabs[self.active].current.hovered().map(|e| vec![e.path.clone()]).unwrap_or_default()
        } else {
            self.tabs[self.active].targets()
        };
        if paths.is_empty() {
            return;
        }
        if permanently && !force {
            self.overlay = Overlay::Confirm(ConfirmOverlay {
                title: "Delete permanently?".into(),
                body: preview_paths(&paths),
                options: vec![('y', "Delete".into()), ('n', "Cancel".into())],
                action: ConfirmAction::DeleteForever { paths },
                dest: None,
            });
            return;
        }
        let kind = if permanently { OpKind::Delete } else { OpKind::Trash };
        let dest = self.tabs[self.active].cwd.clone();
        let id = self.submit_op(kind, paths.clone(), dest.clone(), true);
        // Only the trash can be undone. `D` is asked for twice and then means it.
        if kind == OpKind::Trash {
            self.record_job(id, UndoStep::Trash { paths, dir: dest }, Land::Fresh);
        }
        self.tabs[self.active].clear_selection();
    }

    /// Unpack every selected archive. Anything that is not one is named in an
    /// error rather than silently dropped, so a mixed selection says what it
    /// skipped.
    fn do_extract(&mut self) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        let (archives, rest): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.into_iter().partition(|p| archive::Format::from_path(p).is_some());
        if archives.is_empty() {
            // Name what was actually looked at. "Nothing here" reads as "this
            // directory has none", which is wrong often enough to matter: a
            // selection left over from an earlier command is what `e` acts on,
            // so the archive under the cursor can be ignored while the message
            // appears to deny it exists. Saying how many were examined is what
            // makes a stale selection visible — the count in the header is
            // behind the toast saying this.
            let n = self.tabs[self.active].selected.len();
            let what = if n == 0 {
                "The file under the cursor is not".to_owned()
            } else {
                format!("None of the {n} selected item(s) is")
            };
            self.error(format!("{what} an archive filer can read (zip, tar, tar.gz, tgz, 7z)"));
            return;
        }
        if !rest.is_empty() {
            self.toast(format!("Skipping {} non-archive item(s)", rest.len()));
        }
        let cwd = self.tabs[self.active].cwd.clone();
        self.submit_op(OpKind::Extract, archives, cwd, false);
        self.tabs[self.active].clear_selection();
    }

    /// Ask what the archive should be called. The extension picks the format,
    /// so one prompt covers zip, tar and tar.gz without a second menu.
    fn ask_compress(&mut self) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        // One item names the archive after itself; several after the folder
        // they are in, which is what the user would have typed anyway.
        let stem = match paths.len() {
            1 => util::stem_and_ext(&util::file_name(&paths[0])).0.to_owned(),
            _ => util::file_name(&self.tabs[self.active].cwd),
        };
        let stem = match stem.is_empty() {
            true => "archive".to_owned(),
            false => stem,
        };
        self.open_input(InputKind::Compress, "Compress to", format!("{stem}.zip"));
    }

    fn do_compress(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let cwd = self.tabs[self.active].cwd.clone();
        let dest = util::resolve_against(&cwd, name);
        let Some(format) = archive::Format::from_path(&dest) else {
            self.error("Name it .zip, .tar or .tar.gz to say which format");
            return;
        };
        if !format.can_write() {
            self.error(format!(
                "{} can be read here but not written — use .zip, .tar or .tar.gz",
                format.label(),
            ));
            return;
        }
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        self.submit_op_to(OpKind::Compress(format), paths, cwd, Some(dest), false);
        self.tabs[self.active].clear_selection();
    }

    fn submit_op(
        &mut self,
        kind: OpKind,
        srcs: Vec<PathBuf>,
        dest_dir: PathBuf,
        force: bool,
    ) -> u64 {
        self.submit_op_to(kind, srcs, dest_dir, None, force)
    }

    /// Queue a job and return its id, which [`App::record_job`] uses to hang an
    /// undo step on it.
    fn submit_op_to(
        &mut self,
        kind: OpKind,
        srcs: Vec<PathBuf>,
        dest_dir: PathBuf,
        dest_file: Option<PathBuf>,
        force: bool,
    ) -> u64 {
        let id = self.scanner.next_id();
        let label = match &dest_file {
            Some(f) => format!("{} {} item(s) into {}", kind.verb(), srcs.len(), util::file_name(f)),
            None => format!("{} {} item(s)", kind.verb(), srcs.len()),
        };
        self.tasks.push(Task {
            id,
            kind,
            label,
            files: 0,
            bytes: 0,
            files_done: 0,
            bytes_done: 0,
            current: String::new(),
            // One job runs at a time, so a new one is queued until the worker
            // picks it up and says `Started`.
            state: TaskState::Queued,
            errors: Vec::new(),
            finished: None,
            speed: 0.0,
            sampled_at: Instant::now(),
            sampled_bytes: 0,
        });
        self.ops.submit(OpRequest { id, kind, srcs, dest_dir, dest_file, force });
        id
    }

    /// Remember that finishing job `id` leaves `step` on one of the undo
    /// stacks. Nothing is pushed until the worker says the work went through.
    fn record_job(&mut self, id: u64, step: UndoStep, how: Land) {
        self.op_undo.insert(id, (step, how));
    }

    fn on_op_event(&mut self, ev: ops::OpEvent) {
        match ev {
            ops::OpEvent::Started { id, files, bytes } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                    t.files = files;
                    t.bytes = bytes;
                    t.state = TaskState::Running;
                    // The queue may have held it a while; the speed is about
                    // the work, not the wait.
                    t.sampled_at = Instant::now();
                    t.sampled_bytes = 0;
                }
            }
            ops::OpEvent::Progress { id, files_done, bytes_done, current } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                    t.files_done = files_done;
                    t.sample(bytes_done);
                    t.bytes_done = bytes_done;
                    if !current.is_empty() {
                        t.current = current;
                    }
                }
            }
            ops::OpEvent::Paused { id, paused } => {
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                    t.state = if paused { TaskState::Paused } else { TaskState::Running };
                    // Nothing moved while it was parked, so the old speed is
                    // not a measurement of anything.
                    t.speed = 0.0;
                    t.sampled_at = Instant::now();
                    t.sampled_bytes = t.bytes_done;
                }
            }
            ops::OpEvent::Conflict { id, src, dest, reply } => {
                self.overlay = Overlay::Confirm(ConfirmOverlay {
                    title: "File already exists".into(),
                    body: vec![
                        format!("src:  {}", src.display()),
                        format!("dest: {}", dest.display()),
                    ],
                    options: vec![
                        ('o', "Overwrite".into()),
                        ('a', "Overwrite all".into()),
                        ('s', "Skip".into()),
                        ('S', "Skip all".into()),
                        ('r', "Rename…".into()),
                        ('q', "Cancel".into()),
                    ],
                    action: ConfirmAction::Conflict { reply, job: id },
                    dest: Some(dest),
                });
            }
            ops::OpEvent::Finished { id, errors, cancelled, kind, moved } => {
                // A move that actually moved something is a step `u` can take
                // back. A cancelled one is not: half a move is not a state
                // worth offering to reverse in one keystroke.
                if kind == OpKind::Move && !cancelled && !moved.is_empty() {
                    self.undos.land(UndoStep::Move { pairs: moved }, Land::Fresh);
                }
                if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                    t.state = if cancelled {
                        TaskState::Cancelled
                    } else if errors.is_empty() {
                        TaskState::Done
                    } else {
                        TaskState::Failed
                    };
                    t.errors = errors.clone();
                    t.finished = Some(Instant::now());
                }
                if !errors.is_empty() {
                    self.error(format!("{}: {}", kind.verb(), errors[0]));
                }
                if let Some((step, how)) = self.op_undo.remove(&id) {
                    if errors.is_empty() && !cancelled {
                        let said = match how {
                            Land::Undone => Some(step.undone_label()),
                            Land::Redone => Some(step.redone_label()),
                            Land::Fresh => None,
                        };
                        self.undos.land(step, how);
                        if let Some(said) = said {
                            self.toast(said);
                        }
                    } else {
                        self.undos.keep(step, how);
                    }
                }
                let cwd = self.tabs[self.active].cwd.clone();
                self.cache.remove(&cwd);
                self.rescan(&cwd);
            }
        }
    }

    fn start_rename(&mut self, cursor: RenameCursor) {
        let Some(e) = self.tabs[self.active].current.hovered().cloned() else { return };
        let name = e.name.clone();
        let (stem, _ext) = util::stem_and_ext(&name);
        let sel = match cursor {
            RenameCursor::Start => (0, 0),
            RenameCursor::End => (name.chars().count(), name.chars().count()),
            RenameCursor::BeforeExt => (0, stem.chars().count()),
        };
        let mut ov = InputOverlay {
            kind: InputKind::Rename { from: e.path.clone() },
            title: "Rename".into(),
            text: name,
            initial_selection: Some(sel),
            focused: false,
            completion: Vec::new(),
            completion_at: 0,
        };
        ov.completion.clear();
        self.overlay = Overlay::Input(ov);
    }

    fn copy_text(&mut self, what: CopyWhat) {
        let Some(e) = self.tabs[self.active].current.hovered().cloned() else { return };
        let text = match what {
            // Outside the spot panel the hovered path is the only cell.
            CopyWhat::Path | CopyWhat::Cell => e.path.display().to_string(),
            CopyWhat::Dirname => e
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            CopyWhat::Filename => e.name.clone(),
            CopyWhat::NameWithoutExt => util::stem_and_ext(&e.name).0.to_owned(),
        };
        match exec::set_clipboard(&text) {
            Ok(()) => self.toast(format!("Copied: {text}")),
            Err(err) => self.error(format!("Clipboard: {err}")),
        }
    }

    /// Jump to where the hovered link points. The target came off the scan
    /// worker with the rest of the entry, so nothing here touches disk —
    /// `canonicalize` on a link into a share that stopped answering used to
    /// freeze the window until it gave up.
    fn follow_link(&mut self) {
        let Some(e) = self.tabs[self.active].current.hovered().cloned() else { return };
        let Kind::Link { to_dir, broken } = e.kind else { return };
        let Some(target) = e.link_to.filter(|_| !broken) else {
            self.error(format!("Broken link: {}", e.name));
            return;
        };
        // A link to a file lands on its directory with the file under the
        // cursor; a link to a directory simply opens it.
        let (dir, reveal) = match target.parent() {
            Some(p) if !to_dir => (p.to_path_buf(), Some(util::file_name(&target))),
            _ => (target, None),
        };
        self.cd(dir.clone(), true);
        if let Some(name) = reveal {
            // The listing may still be on its way, so leave the name in `memo`
            // as well — that is what the cursor is restored from on arrival.
            let tab = &mut self.tabs[self.active];
            tab.memo.insert(dir, name.clone());
            tab.current.select_name(&name);
        }
    }

    fn sort(&mut self, by: Option<crate::fs::SortBy>, reverse: Tri, dir_first: Tri) {
        let tab = &mut self.tabs[self.active];
        if let Some(b) = by {
            tab.sort.by = b;
        }
        if let Some(r) = reverse {
            tab.sort.reverse = r;
        }
        if let Some(d) = dir_first {
            tab.sort.dir_first = d;
        }
        let sort = tab.sort;
        let show = tab.show_hidden;
        tab.current.resort(&sort, show);
        if let Some(p) = tab.parent.as_mut() {
            p.resort(&sort, show);
        }
        if let PreviewState::Dir(f) = &mut self.preview.state {
            f.resort(&sort, show);
        }
        let label = sort.by.label().to_owned();
        self.toast(format!("Sort: {label}{}", if sort.reverse { " (reverse)" } else { "" }));
    }

    fn find_arrow(&mut self, prev: bool) {
        let Some(finder) = self.tabs[self.active].finder.clone() else { return };
        let tab = &mut self.tabs[self.active];
        let len = tab.current.view.len();
        if len == 0 {
            return;
        }
        let dir: i64 = if prev != finder.prev { -1 } else { 1 };
        for step in 1..=len as i64 {
            let idx = (tab.current.cursor as i64 + dir * step).rem_euclid(len as i64) as usize;
            let Some(e) = tab.current.at(idx) else { continue };
            if fuzzy::find_substring(&finder.query, &e.name, finder.case_sensitive).is_some() {
                tab.current.cursor = idx;
                tab.sync_visual();
                return;
            }
        }
    }

    /// Open the targets; `line` (1-based) asks an editor to start there, and is
    /// only honored when the hovered file is the one being opened.
    fn open(&mut self, interactive: bool, line: Option<usize>) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        let Some(entry) = self.tabs[self.active].current.hovered().cloned() else { return };
        if entry.is_dir_like() && !interactive {
            self.cd(entry.path, true);
            return;
        }
        let line = line.filter(|_| paths.len() == 1 && paths[0] == entry.path);
        let mime = crate::mime::guess(&entry);
        let openers: Vec<(String, bool, bool, String)> = exec::openers_for(&self.cfg.yazi, &entry, mime)
            .into_iter()
            .map(|o| (o.run.clone(), o.block, o.orphan, o.label()))
            .collect();

        if interactive {
            if openers.is_empty() {
                self.error("No opener configured for this file type");
                return;
            }
            let items: Vec<String> = openers.iter().map(|o| o.3.clone()).collect();
            let details: Vec<String> = openers.iter().map(|o| o.0.clone()).collect();
            let runs: Vec<(String, bool, bool)> =
                openers.iter().map(|o| (o.0.clone(), o.1, o.2)).collect();
            let mut pick = PickOverlay {
                title: "Open with".into(),
                items,
                details,
                query: String::new(),
                matches: Vec::new(),
                cursor: 0,
                action: PickAction::OpenWith { paths, runs, line },
                focused: false,
            };
            pick.refilter();
            self.overlay = Overlay::Pick(pick);
            return;
        }

        let cwd = self.tabs[self.active].cwd.clone();
        match openers.first() {
            Some((run, block, orphan, _)) => {
                let line = exec::command_line(run, &paths, line, &self.cfg.line_args);
                let (block, orphan) = (*block, *orphan);
                self.launch(&line, &cwd, block, orphan, "Open failed");
            }
            None => match exec::open_default(&entry.path) {
                Ok(()) => {}
                Err(e) => self.error(format!("Open failed: {e}")),
            },
        }
    }

    fn run_shell(&mut self, run: &str, block: bool, orphan: bool) {
        let paths = self.tabs[self.active].targets();
        let cwd = self.tabs[self.active].cwd.clone();
        let line = exec::substitute(run, &paths);
        self.launch(&line, &cwd, block, orphan, "Shell failed");
    }

    /// Run `line`, say so, and keep listening in case it falls over a moment
    /// later — which is the usual way an opener fails, the shell having
    /// started fine and then found nothing to run. See [`exec::Launch`].
    fn launch(&mut self, line: &str, cwd: &Path, block: bool, orphan: bool, what: &str) {
        match exec::shell(line, cwd, block, orphan) {
            Ok(l) => {
                self.toast(format!("$ {line}"));
                self.launches.push(l);
            }
            Err(e) => self.error(format!("{what}: {e}")),
        }
    }

    // ---------------------------------------------------------------- input

    pub fn open_input(&mut self, kind: InputKind, title: &str, text: String) {
        let len = text.chars().count();
        self.overlay = Overlay::Input(InputOverlay {
            kind,
            title: title.to_owned(),
            text,
            initial_selection: Some((len, len)),
            focused: false,
            completion: Vec::new(),
            completion_at: 0,
        });
    }

    /// Called on every keystroke for the live-updating inputs.
    pub fn input_changed(&mut self) {
        let Overlay::Input(ov) = &self.overlay else { return };
        match ov.kind.clone() {
            InputKind::Filter => {
                let query = ov.text.clone();
                let tab = &mut self.tabs[self.active];
                tab.current.filter = Some(Filter { query, smart: true, insensitive: false });
                let show = tab.show_hidden;
                tab.current.rebuild(show);
            }
            InputKind::Find { prev } => {
                let query = ov.text.clone();
                let cs = fuzzy::is_case_sensitive(&query, true, false);
                let tab = &mut self.tabs[self.active];
                tab.finder = Some(Finder { query: query.clone(), case_sensitive: cs, prev });
                if query.is_empty() {
                    return;
                }
                let len = tab.current.view.len();
                let start = tab.current.cursor;
                for step in 0..len {
                    let idx = if prev {
                        (start + len - step) % len
                    } else {
                        (start + step) % len
                    };
                    let Some(e) = tab.current.at(idx) else { continue };
                    if fuzzy::find_substring(&query, &e.name, cs).is_some() {
                        tab.current.cursor = idx;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    pub fn submit_input(&mut self) {
        let Overlay::Input(ov) = std::mem::replace(&mut self.overlay, Overlay::None) else {
            return;
        };
        self.pending_completion = None;
        let text = ov.text.trim().to_owned();
        match ov.kind {
            InputKind::Create => self.do_create(&text),
            InputKind::Compress => self.do_compress(&text),
            InputKind::TermFind => self.term_find(&text, true),
            InputKind::Rename { from } => self.do_rename(&from, &text),
            InputKind::Bulk { paths } => self.do_bulk_rename(&paths, &text),
            InputKind::Filter => { /* already applied live */ }
            InputKind::Find { .. } => { /* already applied live */ }
            InputKind::Cd => {
                if !text.is_empty() {
                    let base = self.tabs[self.active].cwd.clone();
                    self.cd_or_reveal(util::resolve_against(&base, &text));
                }
            }
            InputKind::Shell { block } => {
                if !text.is_empty() {
                    self.run_shell(&text, block, false);
                }
            }
            InputKind::Search { via } => self.start_search(&text, via),
            InputKind::ConflictRename { .. } => {
                if let Some(reply) = self.pending_conflict.take() {
                    let r = if text.is_empty() {
                        Resolution::Skip
                    } else {
                        Resolution::Rename(text)
                    };
                    let _ = reply.send(r);
                }
            }
        }
    }

    /// Dismiss the input line without acting on it. A job waiting on a rename
    /// must still be told something, or its worker stays parked forever.
    pub fn cancel_input(&mut self) {
        self.pending_completion = None;
        if let Some(reply) = self.pending_conflict.take() {
            let _ = reply.send(Resolution::Skip);
        }
        self.overlay = Overlay::None;
    }

    fn do_create(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let base = self.tabs[self.active].cwd.clone();
        let as_dir = text.ends_with('/') || text.ends_with('\\');
        let target = util::resolve_against(&base, text.trim_end_matches(['/', '\\']));
        let res = if as_dir {
            std::fs::create_dir_all(&target)
        } else {
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map(|_| ())
        };
        match res {
            Ok(()) => {
                let name = util::file_name(&target);
                self.cache.remove(&base);
                self.rescan(&base);
                self.tabs[self.active].memo.insert(base, name);
            }
            Err(e) => self.error(format!("Create failed: {e}")),
        }
    }

    fn do_rename(&mut self, from: &Path, text: &str) {
        if text.is_empty() {
            return;
        }
        let base = self.tabs[self.active].cwd.clone();
        let to = util::resolve_against(&base, text);
        if to == from {
            return;
        }
        match self.apply_rename(from, &to) {
            Ok(()) => self
                .undos
                .land(UndoStep::Rename { from: from.to_path_buf(), to }, Land::Fresh),
            Err(e) => self.error(format!("Rename failed: {e}")),
        }
    }

    /// The two files to compare. With the view split that is what each pane is
    /// standing on, which is the whole reason the split exists; otherwise it is
    /// the two that are selected.
    fn compare_pair(&self) -> Result<(PathBuf, PathBuf), String> {
        let hovered = |t: usize| self.tabs[t].current.hovered().map(|e| (e.path.clone(), e.kind));
        let (a, b) = match self.split {
            Some(sp) => {
                let (l, r) = match sp.right {
                    true => (sp.other, self.active),
                    false => (self.active, sp.other),
                };
                match (hovered(l), hovered(r)) {
                    (Some(a), Some(b)) => (a, b),
                    _ => return Err("both panes need a file".into()),
                }
            }
            None => {
                let sel = self.tabs[self.active].targets();
                if sel.len() != 2 {
                    return Err("split the view, or select exactly two files".into());
                }
                let of = |p: &PathBuf| {
                    let kind = self.tabs[self.active]
                        .current
                        .entries
                        .iter()
                        .find(|e| &e.path == p)
                        .map(|e| e.kind);
                    (p.clone(), kind.unwrap_or(Kind::File))
                };
                (of(&sel[0]), of(&sel[1]))
            }
        };
        if a.1.is_dir_like() || b.1.is_dir_like() {
            return Err("directories cannot be compared".into());
        }
        if a.0 == b.0 {
            return Err("that is the same file on both sides".into());
        }
        Ok((a.0, b.0))
    }

    fn start_compare(&mut self) {
        let (left, right) = match self.compare_pair() {
            Ok(pair) => pair,
            Err(why) => return self.error(format!("Compare: {why}")),
        };
        self.differ.request(diff::Request {
            left: left.clone(),
            right: right.clone(),
            max_bytes: self.cfg.ui.max_text_bytes,
        });
        self.overlay = Overlay::Diff(DiffOverlay { left, right, outcome: None, offset: 0 });
    }

    pub fn feed_diff_key(&mut self, k: Key) {
        self.pending.push(k);
        let bindings = &self.cfg.keymap.diff;
        match keymap::resolve(bindings, &self.pending) {
            keymap::Match::Exact(b) => {
                let acts = b.run.clone();
                self.pending.clear();
                for a in acts {
                    self.diff_act(a);
                }
            }
            keymap::Match::Pending(_) => {}
            keymap::Match::None => self.pending.clear(),
        }
    }

    /// The compare view's own commands: scroll, jump between differences, close.
    fn diff_act(&mut self, a: Act) {
        let page = self.tabs[self.active].page_rows.max(1);
        let Overlay::Diff(ov) = &mut self.overlay else { return };
        let rows: &[diff::Row] = match &ov.outcome {
            Some(diff::Outcome::Rows { rows, .. }) => rows,
            _ => &[],
        };
        match a {
            Act::Close | Act::Escape(_) | Act::Quit | Act::Compare => self.overlay = Overlay::None,
            Act::Arrow(step) if !rows.is_empty() => {
                ov.offset = step.apply(ov.offset, rows.len(), page);
            }
            Act::FindArrow { prev } if !rows.is_empty() => {
                match diff::next_change(rows, ov.offset, prev) {
                    Some(at) => ov.offset = at,
                    None => {
                        let word = if prev { "first" } else { "last" };
                        self.toast(format!("At the {word} difference"));
                    }
                }
            }
            _ => {}
        }
    }

    /// Ask for a rule to rename everything selected by. The prompt starts on
    /// the rule that changes nothing, so the preview below it opens showing the
    /// names as they are and the person edits from there.
    fn start_bulk_rename(&mut self) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        self.overlay = Overlay::Input(InputOverlay {
            kind: InputKind::Bulk { paths },
            title: "Bulk rename".into(),
            text: "{name}{ext}".into(),
            initial_selection: Some((0, "{name}{ext}".chars().count())),
            focused: false,
            completion: Vec::new(),
            completion_at: 0,
        });
    }

    /// What the preview panel under the prompt shows, and what the apply step
    /// works from. Both read the directory out of the listing already in
    /// memory, so typing a rule never touches the disk.
    pub fn bulk_preview(&self, paths: &[PathBuf], text: &str) -> Result<Vec<rename::Row>, String> {
        let taken: BTreeSet<String> =
            self.tabs[self.active].current.entries.iter().map(|e| e.name.clone()).collect();
        rename::plan(paths, text.trim(), &taken)
    }

    fn do_bulk_rename(&mut self, paths: &[PathBuf], text: &str) {
        let rows = match self.bulk_preview(paths, text) {
            Ok(rows) => rows,
            Err(e) => return self.error(format!("Bulk rename: {e}")),
        };
        // The preview already said which rows are wrong; renaming the rest and
        // leaving the batch half applied would be worse than doing nothing.
        if let Some(bad) = rows.iter().find(|r| r.problem.is_some()) {
            let why = bad.problem.as_deref().unwrap_or_default();
            return self.error(format!("{}: {why}", util::file_name(&bad.from)));
        }
        let pairs: Vec<(PathBuf, PathBuf)> = rows
            .iter()
            .filter(|r| r.to != util::file_name(&r.from))
            .map(|r| (r.from.clone(), r.from.with_file_name(&r.to)))
            .collect();
        if pairs.is_empty() {
            return self.toast("Bulk rename: nothing to change");
        }
        let done = pairs.len();
        match self.run_renames(&pairs) {
            Ok(()) => {
                self.tabs[self.active].clear_selection();
                self.undos.land(UndoStep::Bulk { pairs }, Land::Fresh);
                self.toast(format!("Renamed {done} file(s)"));
            }
            Err(e) => self.error(format!("Bulk rename: {e}")),
        }
    }

    /// Carry out a batch of renames, in an order that works even when two files
    /// swap names. Stops at the first failure and says so: what has already
    /// moved keeps its new name, which is at least a state the listing shows
    /// honestly.
    fn run_renames(&mut self, pairs: &[(PathBuf, PathBuf)]) -> Result<(), String> {
        let names: Vec<(String, String)> =
            pairs.iter().map(|(a, b)| (util::file_name(a), util::file_name(b))).collect();
        // Where each file is right now; parking moves one aside for a moment.
        let mut at: Vec<PathBuf> = pairs.iter().map(|(a, _)| a.clone()).collect();

        for step in rename::order(&names) {
            let (i, to) = match step {
                rename::Step::Park(i) => {
                    let dir = pairs[i].0.parent().unwrap_or(Path::new(""));
                    (i, rename::park_name(dir, i))
                }
                rename::Step::Rename(i) => (i, pairs[i].1.clone()),
            };
            self.apply_rename(&at[i].clone(), &to)?;
            at[i] = to;
        }
        Ok(())
    }

    /// Move `from` onto `to` and leave the cursor there. Undo and redo walk the
    /// same rename backwards and forwards, so this takes two paths rather than
    /// the name typed at the prompt.
    fn apply_rename(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        if ops::exists(to) {
            return Err(format!("Already exists: {}", util::file_name(to)));
        }
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::rename(from, to).map_err(|e| e.to_string())?;
        // `r` takes a path, so a rename can cross directories and leave a
        // listing stale at either end.
        for dir in [from.parent(), to.parent()].into_iter().flatten() {
            let dir = dir.to_path_buf();
            self.cache.remove(&dir);
            self.rescan(&dir);
        }
        if let Some(dir) = to.parent() {
            let name = util::file_name(to);
            self.tabs[self.active].memo.insert(dir.to_path_buf(), name);
        }
        Ok(())
    }

    /// Scale the image preview. Stepping in or out from *fit* starts from the
    /// scale on screen, so the first press does not jump.
    fn zoom_preview(&mut self, to: ZoomTo) {
        if !matches!(self.preview.state, PreviewState::Ready(Payload::Image { .. })) {
            return;
        }
        let from = self.preview.zoom.unwrap_or(self.preview.fit);
        self.preview.zoom = match to {
            ZoomTo::Fit => None,
            ZoomTo::Actual => Some(1.0),
            ZoomTo::In => Some((from * 1.25).clamp(ZOOM_MIN, ZOOM_MAX)),
            ZoomTo::Out => Some((from / 1.25).clamp(ZOOM_MIN, ZOOM_MAX)),
        };
        if self.preview.zoom.is_none() {
            self.preview.pan = egui::Vec2::ZERO;
        }
    }

    /// Read the config files again, so a theme, an icon set or a key can be
    /// changed without closing the window.
    ///
    /// What the person has changed by hand since the window opened is left
    /// alone: the sort a `,` key chose, and whether Markdown is rendered. Those
    /// have keys of their own, and having a reload undo them would be a
    /// surprise. The fonts are the one thing this cannot do itself — installing
    /// a face belongs to the frame loop — so it asks for it with a flag.
    fn reload_config(&mut self) {
        let cfg = Config::load();
        let files = cfg.loaded.len();
        let warning = cfg.warnings.first().cloned();
        self.cfg = cfg;
        self.refont = true;
        // A theme change can turn every row a different color, and the preview
        // holds a highlighted copy of the old one.
        self.preview = PreviewSlot::default();
        match warning {
            Some(w) => self.warn(format!("Config: {w}")),
            None => self.toast(format!("Reloaded {files} config file(s)")),
        }
    }

    /// Take back the newest step. A step that will not go back stays on the
    /// stack — the usual reason is something standing where it came from, which
    /// the person can clear before pressing `u` again.
    fn undo_step(&mut self) {
        let Some(step) = self.undos.undo.pop() else {
            self.toast("Nothing to undo");
            return;
        };
        match step {
            UndoStep::Rename { from, to } => match self.apply_rename(&to, &from) {
                Ok(()) => {
                    let step = UndoStep::Rename { from, to };
                    self.toast(step.undone_label());
                    self.undos.land(step, Land::Undone);
                }
                Err(e) => {
                    self.error(format!("Undo: {e}"));
                    self.undos.keep(UndoStep::Rename { from, to }, Land::Undone);
                }
            },
            UndoStep::Move { pairs } => {
                let back: Vec<(PathBuf, PathBuf)> =
                    pairs.iter().rev().map(|(a, b)| (b.clone(), a.clone())).collect();
                match self.run_renames(&back) {
                    Ok(()) => {
                        let step = UndoStep::Move { pairs };
                        self.toast(step.undone_label());
                        self.undos.land(step, Land::Undone);
                    }
                    Err(e) => {
                        self.error(format!("Undo: {e}"));
                        self.undos.keep(UndoStep::Move { pairs }, Land::Undone);
                    }
                }
            }
            UndoStep::Bulk { pairs } => {
                let back: Vec<(PathBuf, PathBuf)> =
                    pairs.iter().rev().map(|(a, b)| (b.clone(), a.clone())).collect();
                match self.run_renames(&back) {
                    Ok(()) => {
                        let step = UndoStep::Bulk { pairs };
                        self.toast(step.undone_label());
                        self.undos.land(step, Land::Undone);
                    }
                    Err(e) => {
                        self.error(format!("Undo: {e}"));
                        self.undos.keep(UndoStep::Bulk { pairs }, Land::Undone);
                    }
                }
            }
            UndoStep::Trash { paths, dir } => {
                if !restore::SUPPORTED {
                    self.error(format!("Undo: {}", restore::UNSUPPORTED));
                    self.undos.keep(UndoStep::Trash { paths, dir }, Land::Undone);
                    return;
                }
                let id = self.submit_op(OpKind::Restore, paths.clone(), dir.clone(), true);
                self.record_job(id, UndoStep::Trash { paths, dir }, Land::Undone);
            }
        }
    }

    /// Do again what `u` took back.
    fn redo_step(&mut self) {
        let Some(step) = self.undos.redo.pop() else {
            self.toast("Nothing to redo");
            return;
        };
        match step {
            UndoStep::Rename { from, to } => match self.apply_rename(&from, &to) {
                Ok(()) => {
                    let step = UndoStep::Rename { from, to };
                    self.toast(step.redone_label());
                    self.undos.land(step, Land::Redone);
                }
                Err(e) => {
                    self.error(format!("Redo: {e}"));
                    self.undos.keep(UndoStep::Rename { from, to }, Land::Redone);
                }
            },
            UndoStep::Move { pairs } => match self.run_renames(&pairs) {
                Ok(()) => {
                    let step = UndoStep::Move { pairs };
                    self.toast(step.redone_label());
                    self.undos.land(step, Land::Redone);
                }
                Err(e) => {
                    self.error(format!("Redo: {e}"));
                    self.undos.keep(UndoStep::Move { pairs }, Land::Redone);
                }
            },
            UndoStep::Bulk { pairs } => match self.run_renames(&pairs) {
                Ok(()) => {
                    let step = UndoStep::Bulk { pairs };
                    self.toast(step.redone_label());
                    self.undos.land(step, Land::Redone);
                }
                Err(e) => {
                    self.error(format!("Redo: {e}"));
                    self.undos.keep(UndoStep::Bulk { pairs }, Land::Redone);
                }
            },
            UndoStep::Trash { paths, dir } => {
                let id = self.submit_op(OpKind::Trash, paths.clone(), dir.clone(), true);
                self.record_job(id, UndoStep::Trash { paths, dir }, Land::Redone);
            }
        }
    }

    fn complete_input(&mut self) {
        let Overlay::Input(ov) = &self.overlay else { return };
        if !matches!(ov.kind, InputKind::Cd) {
            return;
        }
        let (dir, prefix) = completion_target(&ov.text, &self.tabs[self.active].cwd);
        // A directory that has been listed once answers on the spot, which
        // covers the cwd, its parent and everywhere the tab has been.
        if let Some(entries) = self.cache.get(&dir).cloned() {
            self.pending_completion = None;
            self.apply_completion(&dir, completion_hits(&entries, &prefix));
            return;
        }
        // Everything else goes to the scan pool. `read_dir` on a dead share
        // sits for half a minute, and Tab must not take the window with it.
        let sort = self.tabs[self.active].sort;
        let id = self.scanner.scan_low(dir.clone(), sort);
        self.inflight.insert(id, dir.clone());
        self.pending_completion = Some(PendingCompletion { id, dir, prefix });
    }

    /// Answer a completion whose listing has just arrived. What the user typed
    /// in the meantime wins: an answer to an older question is dropped.
    fn complete_from(&mut self, id: u64, path: &Path, entries: &[Entry]) {
        let Some(p) = self.pending_completion.take() else { return };
        if p.id != id || p.dir != path {
            self.pending_completion = Some(p);
            return;
        }
        let Overlay::Input(ov) = &self.overlay else { return };
        if !matches!(ov.kind, InputKind::Cd) {
            return;
        }
        if completion_target(&ov.text, &self.tabs[self.active].cwd)
            != (p.dir.clone(), p.prefix.clone())
        {
            return;
        }
        self.apply_completion(&p.dir, completion_hits(entries, &p.prefix));
    }

    /// Put the next match on the input line. Repeated presses walk the list,
    /// and `completion_at` remembers how far they got.
    fn apply_completion(&mut self, dir: &Path, hits: Vec<String>) {
        let Overlay::Input(ov) = &mut self.overlay else { return };
        if hits.is_empty() {
            return;
        }
        let idx = ov.completion_at % hits.len();
        ov.text = completed_text(dir, &hits[idx]);
        ov.completion_at = idx + 1;
        ov.completion = hits;
        ov.initial_selection = {
            let n = ov.text.chars().count();
            Some((n, n))
        };
        ov.focused = false;
    }

    /// Open the terminal pane, close it, or move the keys in and out of it.
    ///
    /// `None` is the toggle a key presses: open it and take the keys, or give
    /// them back when it already has them.
    fn terminal(&mut self, what: Tri) {
        if what == Some(false) {
            // Dropping it sends the shell its shutdown.
            self.term = None;
            self.term_focus = false;
            return;
        }
        if self.term.is_some() {
            self.term_focus = what.unwrap_or(!self.term_focus);
            return;
        }
        let cwd = self.tabs[self.active].cwd.clone();
        let ctx = self.ctx.clone();
        // The real shape arrives with the first frame that draws it; this is
        // only what the shell starts life believing.
        let size = crate::terminal::Size::new(80, 24);
        match crate::terminal::Terminal::spawn(&cwd, size, (8, 16), move || ctx.request_repaint()) {
            Ok(t) => {
                self.term = Some(t);
                self.term_focus = true;
            }
            Err(e) => self.error(format!("Terminal failed: {e}")),
        }
    }

    /// Type the selection into the shell, quoted so a path with a space in it
    /// arrives as one word. Nothing is run: the line is left for the user to
    /// put a command in front of.
    fn term_send_paths(&mut self) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        let Some(term) = &self.term else {
            self.error("The terminal is not open");
            return;
        };
        let line: Vec<String> =
            paths.iter().map(|p| crate::terminal::quote(&p.to_string_lossy())).collect();
        term.send(format!(" {}", line.join(" ")).into_bytes());
        self.term_focus = true;
    }

    /// Look for `needle` in the terminal's scrollback and put the match on
    /// screen. Repeating the command walks the matches; running out wraps.
    fn term_find(&mut self, needle: &str, back: bool) {
        let needle = needle.trim();
        if needle.is_empty() {
            return;
        }
        self.term_needle = needle.to_owned();
        let Some(term) = &mut self.term else { return };
        if term.search(needle, back) {
            return;
        }
        // Nothing from here on; start again from the view.
        term.end_search();
        match term.search(needle, back) {
            true => self.toast("Wrapped"),
            false => self.error(format!("No match for {needle}")),
        }
    }

    /// Follow the shell: put the pane where it says it is.
    ///
    /// The other direction, and the useful one when a command has moved the
    /// shell somewhere the pane knows nothing about. It needs the shell to
    /// report its directory (OSC 7), which most do out of the box and some
    /// have to be told to.
    fn term_pull_cwd(&mut self) {
        let Some(term) = &self.term else {
            self.error("The terminal is not open");
            return;
        };
        let Some(cwd) = term.shell_cwd.clone() else {
            // Naming the obstacle alone leaves nowhere to go: "OSC 7" is
            // hard to search for, and most of what comes back overrides
            // `prompt`, which breaks Starship and the other generators people
            // actually run. The hook that does not is named here; the line to
            // paste is in the README, because a toast does not wrap and a
            // PowerShell one-liner is wider than any window.
            self.error(
                "The shell has not said where it is (no OSC 7). PowerShell: set \
                 LocationChangedAction in $PROFILE — the line is in the README",
            );
            return;
        };
        if cwd == self.tabs[self.active].cwd {
            return;
        }
        self.cd(cwd, true);
    }

    /// Read what the shell has said, and keep it in the directory the pane is
    /// showing. Called once a frame, and cheap when there is nothing to do.
    fn pump_terminal(&mut self) {
        let cwd = self.tabs[self.active].cwd.clone();
        let Some(term) = &mut self.term else { return };
        for text in term.drain() {
            // A program asked for the clipboard; only this thread can oblige.
            let _ = exec::set_clipboard(&text);
        }
        if term.exited {
            self.term = None;
            self.term_focus = false;
            self.toast("The shell exited");
            return;
        }
        term.follow(&cwd);
    }

    /// A key while the terminal has the keys. The `[term]` keymap gets first
    /// refusal — that is where the way out is bound — and everything else is
    /// the shell's.
    pub fn feed_term_key(&mut self, k: Key, bytes: Option<Vec<u8>>) {
        // Only exact single-key bindings are consulted: a chord would have to
        // hold a key back, and the shell wants it now.
        let hit = self
            .cfg
            .keymap
            .term
            .iter()
            .find(|b| b.on.len() == 1 && b.on[0] == k)
            .map(|b| b.run.clone());
        if let Some(acts) = hit {
            for a in acts {
                match a {
                    Act::Close | Act::Escape(_) => self.term_focus = false,
                    other => self.act(other),
                }
            }
            return;
        }
        if let (Some(term), Some(bytes)) = (&self.term, bytes) {
            term.send(bytes);
        }
    }

    /// What git says about the rows of `dir`, or nothing while the answer is
    /// still on its way — or for ever, when there is no repository here.
    pub fn git_status(&self, dir: &Path) -> Option<Arc<git::Status>> {
        self.git_status.peek(&dir.to_path_buf()).cloned()
    }

    /// True while Tab is waiting on a listing, so the prompt can say so.
    pub fn completing(&self) -> bool {
        self.pending_completion.is_some()
    }

    fn start_search(&mut self, query: &str, via: SearchVia) {
        if query.is_empty() {
            return;
        }
        let root = self.tabs[self.active].cwd.clone();
        let show_hidden = self.tabs[self.active].show_hidden;
        let ctx = self.ctx.clone();
        let handle = crate::search::spawn(&root, query, via, show_hidden, 5000, move || {
            ctx.request_repaint()
        });

        let tab = &mut self.tabs[self.active];
        tab.remember_cursor();
        let mut folder = Folder::loading(search_path(query, &root), None);
        folder.state = LoadState::Ready;
        tab.current = folder;
        tab.finder = Some(Finder { query: query.to_owned(), case_sensitive: false, prev: false });
        self.search = Some(handle);
        self.preview.state = PreviewState::Empty;
        self.preview.key = None;
    }

    /// True while the current view is a search result list rather than a real
    /// directory; `leave`/`Esc` returns to the directory it started from.
    pub fn in_search_view(&self) -> bool {
        self.tabs[self.active].current.path != self.tabs[self.active].cwd
    }

    fn drain_search(&mut self) {
        let Some(handle) = &self.search else { return };
        let mut batch: Vec<PathBuf> = Vec::new();
        let mut done: Option<(usize, bool)> = None;
        while let Ok(msg) = handle.rx.try_recv() {
            match msg {
                crate::search::Msg::Found(mut v) => batch.append(&mut v),
                crate::search::Msg::Done { total, truncated } => {
                    done = Some((total, truncated));
                    break;
                }
            }
        }
        if !batch.is_empty() {
            let show_hidden = true;
            let f = &mut self.tabs[self.active].current;
            let entries = Arc::make_mut(&mut f.entries);
            for p in batch {
                if let Ok(e) = Entry::from_path(p) {
                    entries.push(e);
                }
            }
            f.rebuild(show_hidden);
        }
        if let Some((total, truncated)) = done {
            self.search = None;
            if total == 0 {
                self.error("No matches");
            } else {
                self.toast(format!(
                    "{total} match(es){} — <Esc> to leave the search view",
                    if truncated { " (truncated)" } else { "" }
                ));
            }
        }
    }

    // ------------------------------------------------------------ bookmarks

    pub fn bookmark_key(&mut self, ch: char) {
        let Some(op) = self.pending_bookmark.take() else { return };
        let key = ch.to_string();
        match op {
            BookmarkOp::Save => {
                let path = self.tabs[self.active].cwd.clone();
                let name = util::file_name(&path);
                self.bookmarks.retain(|b| b.key != key);
                self.bookmarks.push(Bookmark { key: key.clone(), path, name });
                self.bookmarks.sort_by(|a, b| a.key.cmp(&b.key));
                self.save_state();
                self.toast(format!("Bookmark `{key}` saved"));
            }
            BookmarkOp::Jump => match self.bookmarks.iter().find(|b| b.key == key).cloned() {
                Some(b) => self.cd(b.path, true),
                None => self.error(format!("No bookmark `{key}`")),
            },
            BookmarkOp::Delete => {
                let before = self.bookmarks.len();
                self.bookmarks.retain(|b| b.key != key);
                if self.bookmarks.len() == before {
                    self.error(format!("No bookmark `{key}`"));
                } else {
                    self.save_state();
                    self.toast(format!("Bookmark `{key}` deleted"));
                }
            }
        }
    }

    fn open_palette(&mut self) {
        let (items, details, runs) = palette_items(&self.cfg.keymap.mgr, &self.hovered_openers());
        if items.is_empty() {
            self.error("No commands are bound");
            return;
        }
        self.open_pick("Commands".into(), items, details, runs);
    }

    /// The context menu for the file under the cursor. Right-click opens it;
    /// so does the `menu` command.
    fn open_menu(&mut self) {
        let Some(name) = self.tabs[self.active].current.hovered_name().map(str::to_owned) else {
            self.error("Nothing under the cursor");
            return;
        };
        let (items, details, runs) = menu_items(&self.cfg.keymap.mgr, &self.hovered_openers());
        if items.is_empty() {
            self.error("Nothing is bound for this file");
            return;
        }
        // The selection is what the commands will act on, so say so when it is
        // more than the one file the pointer landed on.
        let n = self.tabs[self.active].targets().len();
        let title = match n > 1 {
            true => format!("Actions: {n} selected"),
            false => format!("Actions: {name}"),
        };
        self.open_pick(title, items, details, runs);
    }

    /// What `yazi.toml` offers to open the file under the cursor with.
    fn hovered_openers(&self) -> Vec<OpenerRow> {
        let Some(entry) = self.tabs[self.active].current.hovered() else { return Vec::new() };
        let mime = crate::mime::guess(entry);
        exec::openers_for(&self.cfg.yazi, entry, mime)
            .into_iter()
            .map(|o| (o.run.clone(), o.block, o.orphan, o.label()))
            .collect()
    }

    fn open_pick(
        &mut self,
        title: String,
        items: Vec<String>,
        details: Vec<String>,
        runs: Vec<Vec<Act>>,
    ) {
        let mut pick = PickOverlay {
            title,
            items,
            details,
            query: String::new(),
            matches: Vec::new(),
            cursor: 0,
            action: PickAction::Command { runs },
            focused: false,
        };
        pick.refilter();
        self.overlay = Overlay::Pick(pick);
    }

    /// The bookmarks on their own, as a picker.
    ///
    /// Separate from [`Self::open_jump`], which mixes in the visit history: a
    /// bookmark was named on purpose, and a list of them is the one place to
    /// see what is bound to what without holding a key down to read the hint.
    fn open_bookmark_list(&mut self) {
        if self.bookmarks.is_empty() {
            self.error("No bookmarks yet — `bs` saves this directory under a letter");
            return;
        }
        let mut items = Vec::new();
        let mut details = Vec::new();
        let mut paths = Vec::new();
        for b in &self.bookmarks {
            items.push(format!("[{}] {}", b.key, b.path.display()));
            details.push(b.name.clone());
            paths.push(b.path.clone());
        }
        let mut pick = PickOverlay {
            title: "Bookmarks".into(),
            items,
            details,
            query: String::new(),
            matches: Vec::new(),
            cursor: 0,
            action: PickAction::Jump { paths },
            focused: false,
        };
        pick.refilter();
        self.overlay = Overlay::Pick(pick);
    }

    fn open_jump(&mut self) {
        let mut items = Vec::new();
        let mut details = Vec::new();
        let mut paths = Vec::new();
        for b in &self.bookmarks {
            items.push(format!("[{}] {}", b.key, b.path.display()));
            details.push(b.name.clone());
            paths.push(b.path.clone());
        }
        // Bookmarks were named on purpose, so they stay on top in their own
        // order. The rest is ranked: most used, least stale first.
        let now = epoch_secs();
        let mut ranked: Vec<&Visit> = self.history.iter().collect();
        ranked.sort_by(|a, b| {
            let score = |v: &Visit| frecency(v.hits, now.saturating_sub(v.at));
            score(b).total_cmp(&score(a)).then(b.at.cmp(&a.at))
        });
        for v in ranked {
            if paths.contains(&v.path) {
                continue;
            }
            items.push(v.path.display().to_string());
            details.push(ago(now.saturating_sub(v.at)));
            paths.push(v.path.clone());
        }
        if items.is_empty() {
            self.error("No bookmarks or history yet");
            return;
        }
        let mut pick = PickOverlay {
            title: "Jump to".into(),
            items,
            details,
            query: String::new(),
            matches: Vec::new(),
            cursor: 0,
            action: PickAction::Jump { paths },
            focused: false,
        };
        pick.refilter();
        self.overlay = Overlay::Pick(pick);
    }

    pub fn submit_pick(&mut self) {
        let Overlay::Pick(p) = std::mem::replace(&mut self.overlay, Overlay::None) else {
            return;
        };
        let Some(idx) = p.selected() else { return };
        match p.action {
            PickAction::OpenWith { paths, runs, line } => {
                let Some((run, block, orphan)) = runs.get(idx).cloned() else { return };
                let cwd = self.tabs[self.active].cwd.clone();
                let line = exec::command_line(&run, &paths, line, &self.cfg.line_args);
                self.launch(&line, &cwd, block, orphan, "Open failed");
            }
            PickAction::Jump { paths } => {
                if let Some(p) = paths.get(idx).cloned() {
                    self.cd(p, true);
                }
            }
            PickAction::Command { runs } => {
                // The overlay is already closed, so a command that opens one of
                // its own (input, confirm, help) lands on a clean slate.
                let Some(acts) = runs.get(idx).cloned() else { return };
                for a in acts {
                    self.act(a);
                }
            }
        }
    }

    pub fn answer_confirm(&mut self, ch: char) {
        let Overlay::Confirm(c) = std::mem::replace(&mut self.overlay, Overlay::None) else {
            return;
        };
        match c.action {
            ConfirmAction::Conflict { reply, job } => {
                let r = match ch {
                    'o' => Resolution::Overwrite,
                    'a' => Resolution::OverwriteAll,
                    's' => Resolution::Skip,
                    'S' => Resolution::SkipAll,
                    'r' => {
                        // The worker stays blocked until the new name is
                        // submitted (or the prompt is cancelled).
                        self.pending_conflict = Some(reply);
                        let suggestion = match &c.dest {
                            Some(d) => crate::util::file_name(&ops::unique_name(d)),
                            None => String::new(),
                        };
                        self.open_input(
                            InputKind::ConflictRename { job },
                            "New name",
                            suggestion,
                        );
                        return;
                    }
                    _ => Resolution::Cancel,
                };
                let _ = reply.send(r);
            }
            ConfirmAction::DeleteForever { paths } => {
                if ch == 'y' {
                    let dest = self.tabs[self.active].cwd.clone();
                    self.submit_op(OpKind::Delete, paths, dest, true);
                    self.tabs[self.active].clear_selection();
                }
            }
            ConfirmAction::BookmarkDeleteAll => {
                if ch == 'y' {
                    self.bookmarks.clear();
                    self.save_state();
                    self.toast("All bookmarks deleted");
                }
            }
        }
    }

    // ------------------------------------------------------------ persistence

    fn state_file(name: &str) -> PathBuf {
        Config::state_dir().join(name)
    }

    fn load_state(&mut self) {
        if let Ok(text) = std::fs::read_to_string(Self::state_file("bookmarks.toml")) {
            if let Ok(f) = toml::from_str::<BookmarkFile>(&text) {
                self.bookmarks = f.bookmark;
            }
        }
        if let Ok(text) = std::fs::read_to_string(Self::state_file("history.txt")) {
            self.history = text.lines().map(parse_visit).collect();
        }
    }

    pub fn save_state(&self) {
        let dir = Config::state_dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let f = BookmarkFile { bookmark: self.bookmarks.clone() };
        if let Ok(text) = toml::to_string_pretty(&f) {
            let _ = std::fs::write(dir.join("bookmarks.toml"), text);
        }
        let hist: Vec<String> = self.history.iter().map(write_visit).collect();
        let _ = std::fs::write(dir.join("history.txt"), hist.join("\n"));
    }

    pub fn on_quit(&self) {
        self.save_state();
        if let Some(f) = &self.cwd_file {
            let _ = std::fs::write(f, self.tabs[self.active].cwd.display().to_string());
        }
        if let Some(f) = &self.chooser_file {
            let paths = self.tabs[self.active].targets();
            let text: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
            let _ = std::fs::write(f, text.join("\n"));
        }
    }

    // ------------------------------------------------------------ key input

    pub fn feed_key(&mut self, k: Key) {
        if let Some(_op) = self.pending_bookmark {
            match k.code {
                Code::Char(c) if k.is_bare_char() => {
                    self.bookmark_key(c);
                    return;
                }
                _ => {
                    self.pending_bookmark = None;
                    return;
                }
            }
        }

        self.pending.push(k);
        let bindings = &self.cfg.keymap.mgr;
        match keymap::resolve(bindings, &self.pending) {
            keymap::Match::Exact(b) => {
                let acts = b.run.clone();
                self.pending.clear();
                self.which.clear();
                self.run(&acts);
            }
            keymap::Match::Pending(cands) => {
                let depth = self.pending.len();
                self.which = cands
                    .iter()
                    .filter(|b| b.on.len() > depth)
                    .map(|b| {
                        (
                            crate::config::keys::render_seq(&b.on[depth..]),
                            b.desc.clone(),
                            b.raw.clone(),
                        )
                    })
                    .collect();
            }
            keymap::Match::None => {
                self.pending.clear();
                self.which.clear();
            }
        }
    }

    /// Rows currently visible in one pane's file list; set by the renderer.
    pub fn set_page_rows(&mut self, idx: usize, rows: usize) {
        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.page_rows = rows;
        }
    }
}

fn search_path(query: &str, root: &Path) -> PathBuf {
    PathBuf::from(format!("search: {query}  in  {}", root.display()))
}

/// Split what has been typed into the directory a completion has to list and
/// the prefix its names have to carry on from. A trailing separator means the
/// directory itself is the question, so every child answers it. The text is
/// resolved the way `cd` resolves it, so a relative path completes against the
/// tab it was typed in.
fn completion_target(text: &str, cwd: &Path) -> (PathBuf, String) {
    let p = util::resolve_against(cwd, text);
    if text.ends_with('/') || text.ends_with('\\') {
        return (p, String::new());
    }
    let prefix = util::file_name(&p);
    match p.parent() {
        Some(dir) => (dir.to_path_buf(), prefix),
        // A drive or share root has nothing above it: list the root itself.
        None => (p, String::new()),
    }
}

/// The directories in a listing that carry on from `prefix`, in the order the
/// prompt walks them. Links to directories count, the same as entering one.
fn completion_hits(entries: &[Entry], prefix: &str) -> Vec<String> {
    let prefix = prefix.to_lowercase();
    let mut hits: Vec<String> = entries
        .iter()
        .filter(|e| e.is_dir_like())
        .map(|e| e.name.clone())
        .filter(|n| prefix.is_empty() || n.to_lowercase().starts_with(&prefix))
        .collect();
    hits.sort_by(|a, b| util::natural_cmp(a, b, false));
    hits
}

/// What the input line reads once a name is chosen: the directory, ready for
/// the next component to be typed or completed.
fn completed_text(dir: &Path, name: &str) -> String {
    format!("{}{}", dir.join(name).display(), std::path::MAIN_SEPARATOR)
}

fn preview_paths(paths: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .take(8)
        .map(|p| util::file_name(p))
        .collect();
    if paths.len() > 8 {
        out.push(format!("… and {} more", paths.len() - 8));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::keys::Key;

    /// The bare bones of a listing row: a path and whether entering it means
    /// changing directory.
    fn entry(path: &str, dir: bool) -> Entry {
        let path = PathBuf::from(path);
        Entry {
            name: util::file_name(&path),
            path,
            ext: None,
            kind: if dir { Kind::Dir } else { Kind::File },
            len: 0,
            modified: None,
            created: None,
            accessed: None,
            hidden: false,
            readonly: false,
            link_to: None,
            dir_size: None,
        }
    }

    fn binding(on: &str, run: &str, desc: &str) -> keymap::Binding {
        keymap::Binding {
            on: on.chars().map(Key::char).collect(),
            run: vec![crate::config::cmd::parse(run)],
            desc: desc.into(),
            raw: run.into(),
        }
    }

    #[test]
    fn the_other_pane_follows_its_tab_when_a_tab_closes() {
        // Tabs before it keep their index; tabs after it shift down by one.
        assert_eq!(split_after_remove(0, 2), Some(0));
        assert_eq!(split_after_remove(3, 1), Some(2));
        // Closing the tab the other pane shows leaves nothing to split with.
        assert_eq!(split_after_remove(2, 2), None);
    }

    /// A new tab opens where it was asked to, with no `is_dir` on the way: the
    /// second half of the pair says whether the scan still has to confirm it.
    #[test]
    fn a_new_tab_opens_on_a_path_nobody_checked() {
        let base = Path::new("/a");
        let home = Some(PathBuf::from("/home"));

        // A typed path is unproven and may name a file, hence the fallback.
        let (to, fallback) = new_tab_target(base, false, Some("b/c"), None, home.clone());
        assert_eq!(to, PathBuf::from("/a/b/c"));
        assert!(fallback, "a typed path falls back to the parent");

        // No path and nothing to follow: home, and home is not typed.
        let (to, fallback) = new_tab_target(base, false, Some(""), None, home.clone());
        assert_eq!(to, PathBuf::from("/home"));
        assert!(!fallback);

        // Without a home directory the tab stays where it was opened from.
        let (to, _) = new_tab_target(base, false, None, None, None);
        assert_eq!(to, base.to_path_buf());
    }

    /// Tab asks the scan pool a question, and the answer may arrive over text
    /// that has since moved on. The question is the pair below, so an answer
    /// to an older one can be recognised and dropped.
    fn task(bytes: u64) -> Task {
        Task {
            id: 1,
            kind: OpKind::Copy,
            label: "Copy".into(),
            files: 1,
            bytes,
            files_done: 0,
            bytes_done: 0,
            current: String::new(),
            state: TaskState::Running,
            errors: Vec::new(),
            finished: None,
            speed: 0.0,
            sampled_at: Instant::now(),
            sampled_bytes: 0,
        }
    }

    /// The speed is measured over a window, not per report: reports arrive
    /// about twenty times a second and the gap between two of them says
    /// nothing useful.
    #[test]
    fn a_running_job_reports_a_speed_and_what_is_left() {
        let mut t = task(3_000_000);
        // Too soon to measure: the sample is ignored and there is no answer.
        t.sample(500_000);
        assert_eq!(t.speed(), None, "a fraction of a second is not a measurement");

        // A second's worth of work, a megabyte of it.
        t.sampled_at = Instant::now() - Duration::from_secs(1);
        t.sampled_bytes = 0;
        t.sample(1_000_000);
        t.bytes_done = 1_000_000;
        let speed = t.speed().expect("a second of copying is measurable");
        assert!((900_000..=1_100_000).contains(&speed), "got {speed} B/s");

        // Two of the three megabytes are left, at about a megabyte a second.
        let eta = t.eta().expect("bytes are known, so the rest can be timed");
        assert!((1..=3).contains(&eta.as_secs()), "got {eta:?}");
    }

    #[test]
    fn a_job_that_is_not_running_says_nothing_about_speed() {
        let mut t = task(3_000_000);
        t.sampled_at = Instant::now() - Duration::from_secs(1);
        t.sample(1_000_000);
        t.bytes_done = 1_000_000;

        // Parked: the number would be a memory, not a measurement.
        t.state = TaskState::Paused;
        assert_eq!(t.speed(), None);
        assert_eq!(t.eta(), None);

        // A job counted in files rather than bytes cannot say how long the
        // rest will take, however fast it is going.
        t.state = TaskState::Running;
        t.bytes = 0;
        assert_eq!(t.eta(), None);
        assert!(t.speed().is_some());
    }

    #[test]
    fn a_completion_asks_about_one_directory_and_one_prefix() {
        let cwd = Path::new("/here");
        assert_eq!(completion_target("/a/b/sr", cwd), (PathBuf::from("/a/b"), "sr".into()));
        // A trailing separator asks about the directory itself.
        assert_eq!(completion_target("/a/b/", cwd), (PathBuf::from("/a/b"), String::new()));
        // Another keystroke is another question, so an answer to the old one
        // can be told apart and dropped.
        assert_ne!(completion_target("/a/b/src", cwd), completion_target("/a/b/sr", cwd));
        // A relative name completes where it was typed, as `cd` would take it.
        assert_eq!(completion_target("sr", cwd), (PathBuf::from("/here"), "sr".into()));
    }

    #[test]
    fn a_completion_offers_the_directories_that_carry_on_from_the_prefix() {
        let entries = vec![
            entry("/a/src10", true),
            entry("/a/Src2", true),
            entry("/a/srcs.txt", false),
            entry("/a/target", true),
        ];
        // Case is ignored on the way in, and the order is the listing's own.
        assert_eq!(completion_hits(&entries, "sr"), vec!["Src2", "src10"]);
        // An empty prefix offers every directory, files still left out.
        assert_eq!(completion_hits(&entries, ""), vec!["Src2", "src10", "target"]);
        assert!(completion_hits(&entries, "zz").is_empty());
        // The chosen name comes back ready for the next component to be typed,
        // with the separator this platform spells paths with.
        let done = completed_text(Path::new("/a"), "Src2");
        assert!(done.ends_with(std::path::MAIN_SEPARATOR), "{done}");
        assert!(done.contains("Src2"), "{done}");
    }

    #[test]
    fn a_new_tab_follows_the_cursor_only_onto_a_directory() {
        let base = Path::new("/a");
        let dir = entry("/a/sub", true);
        let file = entry("/a/note.txt", false);

        let (to, fallback) = new_tab_target(base, true, None, Some(&dir), None);
        assert_eq!(to, PathBuf::from("/a/sub"));
        assert!(!fallback, "the entry came from a listing, so the parent is no help");

        // The cursor on a file opens a second view of the directory instead.
        let (to, _) = new_tab_target(base, true, None, Some(&file), None);
        assert_eq!(to, base.to_path_buf());
        let (to, _) = new_tab_target(base, true, None, None, None);
        assert_eq!(to, base.to_path_buf());
    }

    #[test]
    fn the_other_pane_follows_its_tab_when_two_tabs_swap() {
        assert_eq!(split_after_swap(1, 1, 3), 3);
        assert_eq!(split_after_swap(3, 1, 3), 1);
        // A swap between two tabs neither pane shows changes nothing.
        assert_eq!(split_after_swap(2, 0, 4), 2);
    }

    #[test]
    fn the_default_keymap_splits_the_view() {
        let (km, _) = keymap::Keymap::load(&[]);
        let runs: Vec<&Act> = km.mgr.iter().flat_map(|b| b.run.iter()).collect();
        assert!(runs.contains(&&Act::PaneFocus(None)), "<C-w> moves between panes");
        assert!(runs.contains(&&Act::Split(Some(false))), "a key closes the split");
    }

    #[test]
    fn palette_lists_every_command_once() {
        let bindings = vec![
            binding("k", "arrow -1", "Move cursor up"),
            // The same command on a second key: listed once, under the first.
            binding("K", "arrow -1", "Move cursor up"),
            binding("w", "tasks_show", ""),
            binding("z", "chmod", "Unimplemented"),
            binding("x", "noop", "Nothing"),
        ];
        let (items, details, runs) = palette_items(&bindings, &[]);
        assert_eq!(
            items,
            vec!["Move cursor up  ·  arrow -1".to_string(), "tasks_show".to_string()]
        );
        assert_eq!(details, vec!["k".to_string(), "w".to_string()]);
        assert_eq!(runs, vec![vec![Act::Arrow(Step::Rel(-1))], vec![Act::TasksShow]]);

        // The openers for the hovered file ride along at the end, named so it
        // is clear what picking one does.
        let openers = vec![(r"code %s".to_string(), false, true, "VS Code".to_string())];
        let (items, details, runs) = palette_items(&bindings, &openers);
        assert_eq!(items.last().unwrap(), "Open with VS Code");
        assert_eq!(details.last().unwrap(), "code %s");
        assert_eq!(
            runs.last().unwrap(),
            &vec![Act::Shell {
                run: "code %s".into(),
                block: false,
                confirm: false,
                orphan: true
            }]
        );
    }

    /// The context menu is the config read back: what `yazi.toml` opens this
    /// file with, the custom `shell` actions, then the file commands. Moving
    /// around and changing the view are not offered.
    #[test]
    fn the_menu_offers_the_openers_then_the_custom_actions_then_the_file_commands() {
        let bindings = vec![
            binding("k", "arrow -1", "Move cursor up"),
            binding("d", "remove", "Delete"),
            binding("E", "shell 'explorer %s' --orphan", "Reveal in Explorer"),
            binding("z", "chmod", "Unimplemented"),
            binding("gg", "arrow top", "Go to top"),
        ];
        let openers = vec![("notepad %s".to_string(), true, false, "Notepad".to_string())];
        let (items, details, runs) = menu_items(&bindings, &openers);

        assert_eq!(
            items,
            vec![
                "Notepad".to_string(),
                "Reveal in Explorer  ·  shell 'explorer %s' --orphan".to_string(),
                "Delete  ·  remove".to_string(),
            ],
            "moving the cursor is not a thing to do to a file"
        );
        assert_eq!(details[0], "notepad %s", "the opener shows the command it runs");
        assert_eq!(details[2], "d", "a binding shows the key that also runs it");
        assert_eq!(runs[2], vec![Act::Remove { permanently: false, force: false, hovered: false }]);
        // An opener is run as the shell command it is, `block` and all.
        assert!(matches!(runs[0][0], Act::Shell { block: true, orphan: false, .. }));
    }

    #[test]
    fn palette_filters_on_both_the_description_and_the_command() {
        let (km, warnings) = keymap::Keymap::load(&[]);
        assert!(warnings.is_empty(), "the built-in keymap must load clean: {warnings:?}");
        let (items, details, runs) = palette_items(&km.mgr, &[]);
        assert!(items.len() > 30, "got {} commands", items.len());
        // The context menu has a key of its own, so it is reachable without a
        // mouse and the palette lists it like any other command.
        assert!(runs.iter().any(|r| r == &[Act::Menu]), "the palette lists the context menu");

        // The terminal layer is loaded, and holds only the few keys the pane
        // keeps for itself — everything else has to reach the shell.
        assert!(!km.term.is_empty(), "the [term] section is read");
        assert!(
            km.term.iter().all(|b| b.on.len() == 1),
            "only single keys are consulted there, so only single keys belong"
        );
        assert!(
            km.term.iter().any(|b| b.run == vec![Act::Terminal(Some(false))]),
            "there is a way to close it"
        );
        assert!(
            km.term.iter().any(|b| matches!(b.run.first(), Some(Act::TermScroll(_)))),
            "and a way into the scrollback"
        );
        assert_eq!(items.len(), details.len());
        assert_eq!(items.len(), runs.len());
        assert!(runs.iter().any(|r| r == &[Act::Palette]), "the palette lists itself");

        let mut pick = PickOverlay {
            title: "Commands".into(),
            items,
            details,
            query: "tasks_show".into(),
            matches: Vec::new(),
            cursor: 0,
            action: PickAction::Command { runs },
            focused: false,
        };
        pick.refilter();
        let by_command = pick.selected().expect("the command text must match");

        pick.query = "task manager".into();
        pick.refilter();
        assert_eq!(pick.selected(), Some(by_command), "the description finds the same row");
    }

    /// The point of zooming about the pointer: the pixel under the cursor is
    /// the one being looked at, and it must not move.
    #[test]
    fn zooming_keeps_what_is_under_the_pointer_under_it() {
        let center = egui::pos2(100.0, 100.0);
        let pointer = egui::pos2(160.0, 80.0);
        let (zoom, pan) = (1.0, egui::vec2(10.0, -5.0));

        // Where in the image the pointer is, before and after.
        let at = |z: f32, p: egui::Vec2| (pointer - center - p) / z;
        let before = at(zoom, pan);
        let (z2, p2) = zoom_at(zoom, pan, center, pointer, 2.0);

        assert_eq!(z2, 2.0);
        let after = at(z2, p2);
        assert!((after - before).length() < 0.001, "{before:?} vs {after:?}");
    }

    #[test]
    fn the_zoom_stays_within_its_bounds() {
        let (c, p) = (egui::Pos2::ZERO, egui::Pos2::ZERO);
        assert_eq!(zoom_at(ZOOM_MAX, egui::Vec2::ZERO, c, p, 4.0).0, ZOOM_MAX);
        assert_eq!(zoom_at(ZOOM_MIN, egui::Vec2::ZERO, c, p, 0.25).0, ZOOM_MIN);
    }

    #[test]
    fn an_image_smaller_than_the_pane_cannot_be_dragged_off_center() {
        let avail = egui::vec2(400.0, 300.0);

        let stuck = clamp_pan(egui::vec2(50.0, 50.0), egui::vec2(100.0, 80.0), avail);
        assert_eq!(stuck, egui::Vec2::ZERO);

        // Twice the pane wide: it may travel half the overhang either way.
        let free = clamp_pan(egui::vec2(999.0, 0.0), egui::vec2(800.0, 300.0), avail);
        assert_eq!(free.x, 200.0);
    }

    #[test]
    fn fit_never_magnifies_a_small_image() {
        let avail = egui::vec2(400.0, 400.0);
        assert_eq!(image_fit(avail, 40.0, 40.0), 1.0, "a small image sits at its own size");
        assert_eq!(image_fit(avail, 800.0, 400.0), 0.5, "the wider side decides");
        assert_eq!(image_fit(avail, 0.0, 0.0), 1.0, "a zero-sized image cannot divide");
    }

    /// The decode box steps in powers of two, so dragging the zoom about costs
    /// a handful of decodes rather than one a frame, and is capped so a huge
    /// photo cannot ask for a huge texture.
    #[test]
    fn the_decode_box_grows_in_steps_and_stops() {
        assert_eq!(zoom_box((800, 600), None, 1.0), (800, 600));
        assert_eq!(zoom_box((800, 600), Some(0.4), 1.0), (800, 600), "fitting needs no more");
        assert_eq!(zoom_box((800, 600), Some(1.5), 1.0), (1600, 1200));
        assert_eq!(zoom_box((800, 600), Some(2.0), 1.0), (1600, 1200), "same step as 1.5");
        assert_eq!(zoom_box((800, 600), Some(3.0), 1.0), (3200, 2400));
        assert_eq!(zoom_box((800, 600), Some(32.0), 1.0), (4096, 4096), "capped");

        // A big photo fits at 5%, so a tenth of full size is already twice the
        // detail the pane holds.
        assert_eq!(zoom_box((800, 600), Some(0.1), 0.05), (1600, 1200));
        assert_eq!(zoom_box((800, 600), Some(0.05), 0.05), (800, 600), "still fitting");
    }

    /// The whole point of weighting by age: the directory being worked in today
    /// beats the one that was busy last month.
    #[test]
    fn frecency_puts_today_ahead_of_a_bigger_count_long_ago() {
        let today = frecency(3, 10 * 60);
        let last_month = frecency(40, 40 * DAY);

        assert!(today > last_month, "{today} vs {last_month}");
    }

    #[test]
    fn frecency_breaks_ties_within_an_age_by_the_count() {
        assert!(frecency(5, 30) > frecency(2, 30));
        // And a clock that has run backwards is treated as "just now".
        assert_eq!(frecency(2, -500), frecency(2, 0));
    }

    #[test]
    fn ago_is_coarse_but_never_wrong() {
        assert_eq!(ago(0), "just now");
        assert_eq!(ago(59), "just now");
        assert_eq!(ago(60), "1m ago");
        assert_eq!(ago(HOUR), "1h ago");
        assert_eq!(ago(DAY + 1), "1d ago");
        assert_eq!(ago(45 * DAY), "1mo ago");
        assert_eq!(ago(-1), "", "a time in the future says nothing at all");
    }

    /// A `history.txt` from before the counts were written still lists the
    /// directories; they simply rank below anything visited since.
    #[test]
    fn a_history_line_without_counts_still_reads_as_a_visit() {
        let old = parse_visit(r"C:\work\filer");
        assert_eq!(old, Visit { path: PathBuf::from(r"C:\work\filer"), hits: 1, at: 0 });

        let new = Visit { path: PathBuf::from(r"C:\work\filer"), hits: 7, at: 1_700_000_000 };
        assert_eq!(parse_visit(&write_visit(&new)), new, "a round trip keeps everything");
    }

    fn renamed(from: &str, to: &str) -> UndoStep {
        UndoStep::Rename { from: PathBuf::from(from), to: PathBuf::from(to) }
    }

    #[test]
    fn undoing_and_redoing_pass_the_step_between_the_stacks() {
        let mut u = Undos::default();
        u.land(renamed("a", "b"), Land::Fresh);
        assert_eq!(u.undo.len(), 1);

        // `u`: the step comes off the undo stack and lands on the redo stack.
        let step = u.undo.pop().unwrap();
        u.land(step, Land::Undone);
        assert!(u.undo.is_empty());
        assert_eq!(u.redo, vec![renamed("a", "b")]);

        // `U`: and back again, as many times as the person likes.
        let step = u.redo.pop().unwrap();
        u.land(step, Land::Redone);
        assert_eq!(u.undo, vec![renamed("a", "b")]);
        assert!(u.redo.is_empty());
    }

    #[test]
    fn a_new_action_after_an_undo_drops_what_could_have_been_redone() {
        let mut u = Undos::default();
        u.land(renamed("a", "b"), Land::Fresh);
        let step = u.undo.pop().unwrap();
        u.land(step, Land::Undone);
        assert_eq!(u.redo.len(), 1);

        u.land(renamed("c", "d"), Land::Fresh);

        assert_eq!(u.undo, vec![renamed("c", "d")]);
        assert!(u.redo.is_empty(), "history forked, so there is no way forward");
    }

    /// Work that did not go through leaves the stacks as they were, so the key
    /// can be pressed again once whatever was in the way is gone.
    #[test]
    fn a_step_that_fails_goes_back_where_it_came_from() {
        let mut u = Undos::default();
        u.keep(renamed("a", "b"), Land::Undone);
        assert_eq!(u.undo, vec![renamed("a", "b")]);

        u.keep(renamed("c", "d"), Land::Redone);
        assert_eq!(u.redo, vec![renamed("c", "d")]);

        // A fresh action that failed never happened; there is nothing to take back.
        let mut u = Undos::default();
        u.keep(renamed("a", "b"), Land::Fresh);
        assert!(u.undo.is_empty() && u.redo.is_empty());
    }

    #[test]
    fn the_oldest_steps_fall_off_the_bottom() {
        let mut u = Undos::default();
        for i in 0..Undos::MAX + 10 {
            u.land(renamed(&format!("a{i}"), &format!("b{i}")), Land::Fresh);
        }

        assert_eq!(u.undo.len(), Undos::MAX);
        assert_eq!(u.undo[0], renamed("a10", "b10"), "the first ten are gone");
    }
}

#[cfg(test)]
mod preview_delivery {
    use super::*;

    /// The answer from the preview worker has to reach the screen, not just the
    /// cache.
    ///
    /// This is here because it did not, for six versions. v0.5.0 added the
    /// image-fit seeding by replacing `state = Ready(payload)` rather than
    /// following it, so every preview of a file not already cached stayed on
    /// `…` for ever. The cache hid it: coming back to a file worked, because
    /// `request_preview` sets `Ready` on a cache hit, so only the *first* look
    /// at a file was broken and that reads like a slow load.
    ///
    /// Asserting on `PreviewState` rather than on the cache is the whole point.
    /// The old code passed every test there was.
    #[test]
    fn a_reply_puts_the_payload_on_screen() {
        let ctx = egui::Context::default();
        let mut app = App::new(Config::load(), std::env::temp_dir(), ctx.clone());

        let key = preview::Key {
            path: PathBuf::from("/nowhere/readme.md"),
            len: 7,
            mtime: None,
            box_size: (640, 480),
            cols: 80,
        };
        // What `request_preview` leaves behind once it has dispatched.
        app.preview.key = Some(key.clone());
        app.preview.state = PreviewState::Loading;

        app.on_preview(
            preview::Response { key, payload: Payload::Error("x".into()) },
            &ctx,
        );

        assert!(
            matches!(app.preview.state, PreviewState::Ready(_)),
            "a matching reply must leave the pane showing the payload, not `…`; \
             got {:?}",
            std::mem::discriminant(&app.preview.state),
        );
    }

    /// The other half: a reply for a file the cursor has already left is still
    /// dropped, and dropping it must not knock out the preview on screen.
    #[test]
    fn a_stale_reply_changes_nothing() {
        let ctx = egui::Context::default();
        let mut app = App::new(Config::load(), std::env::temp_dir(), ctx.clone());

        let wanted = preview::Key {
            path: PathBuf::from("/nowhere/wanted.md"),
            len: 1,
            mtime: None,
            box_size: (640, 480),
            cols: 80,
        };
        let old = preview::Key { path: PathBuf::from("/nowhere/old.md"), ..wanted.clone() };
        app.preview.key = Some(wanted);
        app.preview.state = PreviewState::Loading;

        app.on_preview(
            preview::Response { key: old, payload: Payload::Error("x".into()) },
            &ctx,
        );

        assert!(
            matches!(app.preview.state, PreviewState::Loading),
            "a reply for another file must not be shown",
        );
    }
}

#[cfg(test)]
mod preview_scroll {
    use super::*;

    fn app() -> (App, egui::Context) {
        let ctx = egui::Context::default();
        let app = App::new(Config::load(), std::env::temp_dir(), ctx.clone());
        (app, ctx)
    }

    /// `<A-j>` at the bottom of a file used to push `preview_offset` past the
    /// end, and the pane was drawn from there before anything corrected it —
    /// one wrong frame per press, which is why it took a held-down key to
    /// catch.
    #[test]
    fn scrolling_down_stops_at_the_end() {
        let (mut a, _c) = app();
        a.preview.max_offset = 40;
        a.tabs[a.active].preview_offset = 38;

        for _ in 0..20 {
            a.act(Act::Seek(Step::Rel(5)));
        }

        assert_eq!(
            a.tabs[a.active].preview_offset, 40,
            "twenty presses at the bottom must leave the offset on the last line",
        );
    }

    /// And the other end, which was already right: one press back from the
    /// bottom has to move, or the ceiling has turned into a trap.
    #[test]
    fn scrolling_back_up_is_immediate() {
        let (mut a, _c) = app();
        a.preview.max_offset = 40;
        a.tabs[a.active].preview_offset = 40;

        a.act(Act::Seek(Step::Rel(-5)));

        assert_eq!(
            a.tabs[a.active].preview_offset, 35,
            "leaving the bottom must take one press, not as many as were spent overshooting",
        );
    }

    #[test]
    fn scrolling_up_stops_at_the_top() {
        let (mut a, _c) = app();
        a.preview.max_offset = 40;
        a.tabs[a.active].preview_offset = 3;
        for _ in 0..10 {
            a.act(Act::Seek(Step::Rel(-5)));
        }
        assert_eq!(a.tabs[a.active].preview_offset, 0);
    }

    /// A file with nothing to scroll does not scroll.
    #[test]
    fn a_short_file_does_not_move() {
        let (mut a, _c) = app();
        a.preview.max_offset = 0;
        a.act(Act::Seek(Step::Rel(5)));
        assert_eq!(a.tabs[a.active].preview_offset, 0);
    }
}

#[cfg(test)]
mod extract_message {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// The message has to say what it looked at. "Nothing here" was read as
    /// "this directory holds no archives", which was wrong in the report that
    /// prompted this: the cursor was on a zip whose contents were on screen in
    /// the preview, and `e` was acting on a selection of PDFs left over from an
    /// earlier command. The count is what gives that away, because the header's
    /// own "N selected" is underneath the toast carrying this text.
    #[test]
    fn it_says_how_many_were_examined() {
        let mut a = app();
        let i = a.active;
        a.tabs[i].selected.insert(PathBuf::from("a.pdf"));
        a.tabs[i].selected.insert(PathBuf::from("b.pdf"));
        a.do_extract();

        let last = a.toasts.last().expect("an error was raised");
        assert_eq!(last.level, Level::Error, "it is an error, not a note");
        assert!(last.text.contains('2'), "the count is missing: {}", last.text);
        assert!(
            !last.text.contains("Nothing here"),
            "\"here\" reads as the directory: {}",
            last.text,
        );
    }

    /// And when nothing is selected it must not claim a selection.
    #[test]
    fn it_names_the_cursor_when_nothing_is_selected() {
        let mut a = app();
        // No entries, so `targets` is empty and `do_extract` returns early;
        // drive the message the way a hovered non-archive does.
        let i = a.active;
        assert!(a.tabs[i].selected.is_empty());
        a.tabs[i].selected.insert(PathBuf::from("x.pdf"));
        a.do_extract();
        let with_selection = a.toasts.last().unwrap().text.clone();
        assert!(with_selection.contains("selected"), "{with_selection}");
    }

    /// Whatever the wording, it has to name the formats — otherwise the reader
    /// still does not know whether their file was ever a candidate.
    #[test]
    fn it_lists_what_can_be_read() {
        let mut a = app();
        let i = a.active;
        a.tabs[i].selected.insert(PathBuf::from("a.pdf"));
        a.do_extract();
        let t = &a.toasts.last().unwrap().text;
        for f in ["zip", "tar", "7z"] {
            assert!(t.contains(f), "{f} missing from: {t}");
        }
    }
}

#[cfg(test)]
mod move_undo {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("filer-move-undo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d.join(name)
    }

    /// `x` then `p` is a move, and a move is undoable. It was not: only `d`,
    /// `r` and `R` were, and the README's reason for leaving the rest out —
    /// that undoing a copy would mean deleting files to tidy up — is about
    /// copies. Putting a moved file back deletes nothing.
    #[test]
    fn a_move_goes_back_where_it_came_from() {
        let from = tmp("origin.txt");
        let to = tmp("landed.txt");
        let _ = std::fs::remove_file(&from);
        std::fs::write(&to, b"x").unwrap();

        let mut a = app();
        a.undos.land(UndoStep::Move { pairs: vec![(from.clone(), to.clone())] }, Land::Fresh);
        a.undo_step();

        assert!(from.exists(), "the file must be back at its original path");
        assert!(!to.exists(), "and gone from where it was moved to");
        let _ = std::fs::remove_file(&from);
    }

    /// The pair that matters. A paste onto a taken name lands as `_1`, and the
    /// undo has to start from where the file actually is — an undo built from
    /// the name that was *asked* for would go looking for a file that was never
    /// created.
    #[test]
    fn it_starts_from_where_the_file_actually_landed() {
        let from = tmp("report.txt");
        let asked = tmp("dest.txt");
        let landed = tmp("dest_1.txt");
        let _ = std::fs::remove_file(&from);
        std::fs::write(&asked, b"in the way").unwrap();
        std::fs::write(&landed, b"the moved one").unwrap();

        let mut a = app();
        a.undos.land(UndoStep::Move { pairs: vec![(from.clone(), landed.clone())] }, Land::Fresh);
        a.undo_step();

        assert!(from.exists(), "back at the original name");
        assert!(!landed.exists(), "no longer at the conflict-resolved name");
        assert!(asked.exists(), "the file that was in the way is untouched");
        for p in [&from, &asked] {
            let _ = std::fs::remove_file(p);
        }
    }

    /// And `U` puts it back again.
    #[test]
    fn redo_moves_it_forward_again() {
        let from = tmp("there.txt");
        let to = tmp("here.txt");
        let _ = std::fs::remove_file(&to);
        std::fs::write(&from, b"x").unwrap();

        let mut a = app();
        a.undos.land(UndoStep::Move { pairs: vec![(from.clone(), to.clone())] }, Land::Undone);
        a.redo_step();

        assert!(to.exists() && !from.exists());
        let _ = std::fs::remove_file(&to);
    }
}

#[cfg(test)]
mod config_warnings {
    use super::*;

    /// A config warning is not an error, and has to stop looking like one.
    ///
    /// v0.20.0 put the startup line up through `error`, which paints it in the
    /// colour of a failed operation. The first person to see it asked what had
    /// gone wrong -- nothing had: a keymap of theirs bound `'` twice to the
    /// same command, which changes no behaviour at all. Asserting on the level
    /// rather than the text, because the wording is not what misled them.
    #[test]
    fn are_raised_as_warnings_not_errors() {
        let ctx = egui::Context::default();
        let cfg = Config { warnings: vec!["[mgr] `'` is bound twice".into()], ..Config::load() };
        let app = App::new(cfg, std::env::temp_dir(), ctx);

        let t = app.toasts.first().expect("the warning reaches the screen");
        assert_eq!(t.level, Level::Warn, "a config warning is advice, not a failure");
        assert!(t.text.starts_with("Config: "), "{}", t.text);
    }

    /// The colours have to differ, or the level above is a distinction the
    /// reader cannot see.
    #[test]
    fn warning_and_error_are_different_colours() {
        let t = crate::config::theme::Theme::default();
        assert_ne!(t.warning, t.progress_error);
        assert_ne!(t.warning, t.fg);
    }
}

#[cfg(test)]
mod term_scroll_direction {

    /// `term_scroll -50%` means half a screen *back*, the way `arrow -50%`
    /// means half a screen up. Alacritty counts the other way round: a
    /// positive delta is older. Passing the number through as it stood aimed
    /// `<S-PageUp>` at the bottom, which is where the view already is, so the
    /// first key anyone tries did nothing — while `<S-Home>` and `<S-End>`,
    /// having no sign to get wrong, worked and made it look like a key
    /// problem rather than an arithmetic one.
    #[test]
    fn a_negative_step_goes_back_into_the_history() {
        use crate::config::cmd::Step;
        let lines = 4usize;
        let mut t = crate::terminal::testing::term(20, lines);
        for i in 0..40 {
            crate::terminal::testing::feed(&mut t, &format!("line{i}\r\n"));
        }

        // The mapping under test, lifted out of `Act::TermScroll`.
        let scroll = |t: &mut alacritty_terminal::Term<crate::terminal::Proxy>, step: Step| {
            use alacritty_terminal::grid::Scroll;
            let page = lines as i64;
            let by = match step {
                Step::Top => Scroll::Top,
                Step::Bot => Scroll::Bottom,
                Step::Rel(n) => Scroll::Delta(-(n as i32)),
                Step::Pct(p) => Scroll::Delta(-((page * p / 100) as i32)),
            };
            t.scroll_display(by);
        };

        scroll(&mut t, Step::Pct(-50));
        assert_eq!(t.grid().display_offset(), 2, "back half of a four-line screen");
        scroll(&mut t, Step::Pct(50));
        assert_eq!(t.grid().display_offset(), 0, "and forward again");

        scroll(&mut t, Step::Rel(-3));
        assert_eq!(t.grid().display_offset(), 3, "three lines back");
        scroll(&mut t, Step::Top);
        assert!(t.grid().display_offset() > 3, "the top is as far as it goes");
        scroll(&mut t, Step::Bot);
        assert_eq!(t.grid().display_offset(), 0);
    }
}

#[cfg(test)]
mod spot_keys {
    use super::*;
    use crate::config::keymap;

    /// The spotter and quick look divide the keys the same way.
    ///
    /// `<F3>` leaves the `[mgr]` layer live, so there `j` moves the list and
    /// `<A-j>` scrolls what is on show. The spotter had the plain keys on its
    /// own rows and `h`/`l` on the files, so the same fingers did different
    /// things depending on which was open — and the panel is one screen of
    /// facts about one file, so walking the files is the common move.
    #[test]
    fn the_plain_keys_walk_the_files_and_the_alt_keys_the_panel() {
        let (km, warnings) = keymap::Keymap::load(&[]);
        assert!(warnings.is_empty(), "{warnings:?}");

        let named = |b: &crate::config::keymap::Binding| crate::config::keys::render_seq(&b.on);
        let run = |key: &str| {
            km.spot
                .iter()
                .find(|b| named(b) == key)
                .unwrap_or_else(|| panic!("`{key}` is not bound in [spot]"))
                .run
                .clone()
        };
        for key in ["j", "<Down>"] {
            assert_eq!(run(key), vec![Act::Swipe(1)], "`{key}` goes to the next file");
        }
        for key in ["k", "<Up>"] {
            assert_eq!(run(key), vec![Act::Swipe(-1)], "`{key}` goes to the previous file");
        }
        for key in ["l", "<Right>"] {
            assert_eq!(run(key), vec![Act::Enter], "`{key}` goes into the directory");
        }
        for key in ["h", "<Left>"] {
            assert_eq!(run(key), vec![Act::Leave], "`{key}` goes up to the parent");
        }
        for key in ["<A-j>", "<A-Down>"] {
            assert_eq!(run(key), vec![Act::Arrow(Step::Rel(1))], "`{key}` moves down the panel");
        }
        for key in ["<A-k>", "<A-Up>"] {
            assert_eq!(run(key), vec![Act::Arrow(Step::Rel(-1))], "`{key}` moves up the panel");
        }

        // The same split the mgr layer has, which is the point of the change.
        let mgr = |key: &str| km.mgr.iter().find(|b| named(b) == key).map(|b| b.run.clone());
        assert_eq!(mgr("j"), Some(vec![Act::Arrow(Step::Rel(1))]), "`j` moves the list under F3");
        assert!(
            matches!(mgr("<A-j>").as_deref(), Some([Act::Seek(_)])),
            "and <A-j> scrolls what is on show",
        );
        // The horizontal pair is the list's, unchanged, in both.
        assert_eq!(mgr("h"), Some(vec![Act::Leave]));
        assert_eq!(mgr("l"), Some(vec![Act::Enter]));
    }
}

#[cfg(test)]
mod spot_follows_the_cursor {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// The panel describes whatever is hovered, so it has to be asked about it.
    ///
    /// `h` and `l` move to another directory, and which file ends up hovered
    /// there is decided by a listing that arrives later on a worker thread —
    /// asking at the moment the key is pressed asks about nothing at all.
    /// `sync_spot` runs once a frame for exactly that, and the assertion is on
    /// what the worker was asked, because the panel is silently right either
    /// way: its first section is read from the listing live, so a stale worker
    /// answer just leaves the per-type rows missing.
    #[test]
    fn a_new_hover_is_asked_about_once_it_exists() {
        let mut a = app();
        let dir = std::env::temp_dir().join("filer-spot-follow");
        let _ = std::fs::create_dir_all(&dir);
        let one = dir.join("one.txt");
        let two = dir.join("two.txt");
        std::fs::write(&one, "1").unwrap();
        std::fs::write(&two, "2").unwrap();

        let entries = Arc::new(vec![
            crate::fs::Entry::from_path(one.clone()).unwrap(),
            crate::fs::Entry::from_path(two.clone()).unwrap(),
        ]);
        a.tabs[a.active].current = Folder::from_entries(dir.clone(), entries, true);
        a.overlay = Overlay::Spot(SpotOverlay { cursor: 0, scroll: 0 });

        a.sync_spot();
        assert_eq!(a.spot_asked.as_deref(), Some(one.as_path()), "the hovered one");

        // Asking again for the same file does not re-ask.
        a.spot_asked = None;
        a.sync_spot();
        assert_eq!(a.spot_asked.as_deref(), Some(one.as_path()));

        // Moving the cursor moves the panel with it.
        a.spot_act(Act::Swipe(1));
        assert_eq!(a.spot_asked.as_deref(), Some(two.as_path()), "it followed the cursor");

        // Closing the panel lets go, so reopening asks afresh.
        a.overlay = Overlay::None;
        a.sync_spot();
        assert_eq!(a.spot_asked, None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
