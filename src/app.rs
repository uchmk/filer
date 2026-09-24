//! Application state and the action dispatcher.
//!
//! Everything the UI does goes through [`Act`], so keys, mouse clicks and
//! internal follow-ups all take the same path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::config::cmd::{Act, CopyWhat, EscapeWhat, RenameCursor, SearchVia, Step, Tri};
use crate::config::keys::{Code, Key};
use crate::config::{keymap, Config};
use crate::core::folder::{Filter, Folder, LoadState};
use crate::core::fuzzy;
use crate::core::tab::{CdFallout, Finder, PendingCd, Tab};
use crate::exec;
use crate::fs::archive;
use crate::fs::git;
use crate::fs::ops::{self, OpKind, OpRequest, Resolution};
use crate::fs::scan::{ScanResult, Scanner};
use crate::fs::watch::Watcher;
use crate::fs::{Entry, Kind, SortSpec};
use crate::preview::{self, Payload, Previewer, TocEntry};
use crate::spot::{self, Section, Spotter};
use crate::util::{self, Lru};

pub const MAX_TABS: usize = 9;

// ----------------------------------------------------------------- overlays

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputKind {
    Create,
    Rename { from: PathBuf },
    Filter,
    Find { prev: bool },
    Cd,
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
            | Act::Copy(_)
            | Act::Shell { .. }
            | Act::Extract
            | Act::Compress
            | Act::SendPane { .. }
            | Act::TermSend
            | Act::Spot
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

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub at: Instant,
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
    /// Text columns across the pane, which rendered Markdown wraps to.
    pub cols: u16,
    /// The outline entry under the cursor while the keys drive the preview's
    /// outline rather than the file list.
    pub outline: Option<usize>,
    /// A file whose outline should take the keys as soon as its preview lands
    /// (`enter` was pressed before it had loaded).
    pub outline_wanted: Option<PathBuf>,
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
            cols: 80,
            outline: None,
            outline_wanted: None,
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
    /// A drag in flight between the panes.
    pub drag: Option<Drag>,
    /// Where each pane was drawn this frame, so a drop can be placed.
    pub pane_rects: Vec<(usize, egui::Rect)>,
    /// What git says about each directory on screen, by directory.
    git_status: Lru<PathBuf, Arc<git::Status>>,
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
    pub hide_parent: bool,
    /// Markdown is shown rendered rather than as source.
    pub render_markdown: bool,
    /// A bold face was loaded as the `bold` font family.
    pub bold_font: bool,

    pub tasks: Vec<Task>,
    pub toasts: Vec<Toast>,
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<PathBuf>,

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
            drag: None,
            pane_rects: Vec::new(),
            spotter,
            spotted: None,
            pending: Vec::new(),
            which: Vec::new(),
            overlay: Overlay::None,
            pending_bookmark: None,
            yank: Yank { paths: Vec::new(), cut: false },
            preview: PreviewSlot::default(),
            max_preview: false,
            hide_parent: false,
            render_markdown,
            bold_font: false,
            tasks: Vec::new(),
            toasts: Vec::new(),
            bookmarks: Vec::new(),
            history: Vec::new(),
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
        self.toasts.push(Toast { text: text.into(), error: false, at: Instant::now() });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast { text: text.into(), error: true, at: Instant::now() });
    }

    // ------------------------------------------------------------- scanning

    /// Ask for any listing the current view needs and does not have.
    pub fn kick_scans(&mut self) {
        let sort = self.tabs[self.active].sort;
        let cwd = self.tabs[self.active].cwd.clone();
        let parent = cwd.parent().map(Path::to_path_buf);

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
        while let Ok(ev) = self.ops.rx.try_recv() {
            self.on_op_event(ev);
        }
        while let Ok(dir) = self.watcher.rx.try_recv() {
            self.dirty.insert(dir, Instant::now());
        }
        while let Ok(rep) = self.git.rx.try_recv() {
            self.git_status.put(rep.dir, Arc::new(rep.status));
        }
        self.drain_search();
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
        // The outline belongs to the file it was opened on.
        if self.preview.key.as_ref().is_none_or(|k| k.path != entry.path) {
            self.preview.outline = None;
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
            && matches!(self.preview.state, PreviewState::Ready(Payload::Text { .. } | Payload::Markdown { .. }));
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
        self.preview.state = PreviewState::Ready(res.payload);
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
        let Some(entry) = self.tabs[self.active].current.hovered() else { return };
        self.spotter.request(entry.path.clone());
        self.overlay = Overlay::Spot(SpotOverlay { cursor: 0, scroll: 0 });
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
                let before = self.tabs[self.active].current.hovered().map(|e| e.path.clone());
                self.act(Act::Arrow(Step::Rel(n)));
                let after = self.tabs[self.active].current.hovered().map(|e| e.path.clone());
                if let Some(path) = after.filter(|p| Some(p) != before.as_ref()) {
                    self.spotter.request(path);
                }
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

        let parent = target.parent().map(Path::to_path_buf);
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

    fn remember_history(&mut self, path: &Path) {
        self.history.retain(|p| p != path);
        self.history.push(path.to_path_buf());
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
        if let Some(p) = cwd.parent() {
            let p = p.to_path_buf();
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
                let cur = self.tabs[self.active].preview_offset as i64;
                self.tabs[self.active].preview_offset = cur.saturating_add(delta).max(0) as usize;
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

            Act::Help => {
                self.help_scroll = 0;
                self.overlay = Overlay::Help;
            }
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
        self.submit_op(kind, paths, dest, true);
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
            self.error("Nothing here is an archive filer can read");
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
            self.error(format!("{} can be read here but not written", format.label()));
            return;
        }
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        self.submit_op_to(OpKind::Compress(format), paths, cwd, Some(dest), false);
        self.tabs[self.active].clear_selection();
    }

    fn submit_op(&mut self, kind: OpKind, srcs: Vec<PathBuf>, dest_dir: PathBuf, force: bool) {
        self.submit_op_to(kind, srcs, dest_dir, None, force);
    }

    fn submit_op_to(
        &mut self,
        kind: OpKind,
        srcs: Vec<PathBuf>,
        dest_dir: PathBuf,
        dest_file: Option<PathBuf>,
        force: bool,
    ) {
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
            ops::OpEvent::Finished { id, errors, cancelled, kind } => {
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
                match exec::shell(&line, &cwd, *block, *orphan) {
                    Ok(_) => self.toast(format!("Opened with: {line}")),
                    Err(e) => self.error(format!("Open failed: {e}")),
                }
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
        match exec::shell(&line, &cwd, block, orphan) {
            Ok(_) => self.toast(format!("$ {line}")),
            Err(e) => self.error(format!("Shell failed: {e}")),
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
            InputKind::Rename { from } => self.do_rename(&from, &text),
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
        if ops::exists(&to) {
            self.error(format!("Already exists: {}", util::file_name(&to)));
            return;
        }
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(from, &to) {
            Ok(()) => {
                let name = util::file_name(&to);
                self.cache.remove(&base);
                self.rescan(&base);
                self.tabs[self.active].memo.insert(base, name);
            }
            Err(e) => self.error(format!("Rename failed: {e}")),
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

    fn open_jump(&mut self) {
        let mut items = Vec::new();
        let mut details = Vec::new();
        let mut paths = Vec::new();
        for b in &self.bookmarks {
            items.push(format!("[{}] {}", b.key, b.path.display()));
            details.push(b.name.clone());
            paths.push(b.path.clone());
        }
        for p in self.history.iter().rev() {
            if paths.contains(p) {
                continue;
            }
            items.push(p.display().to_string());
            details.push(String::new());
            paths.push(p.clone());
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
                match exec::shell(&line, &cwd, block, orphan) {
                    Ok(_) => self.toast(format!("$ {line}")),
                    Err(e) => self.error(format!("Open failed: {e}")),
                }
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
            self.history = text.lines().map(PathBuf::from).collect();
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
        let hist: Vec<String> = self.history.iter().map(|p| p.display().to_string()).collect();
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
}
