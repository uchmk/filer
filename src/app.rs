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
    /// `<C-S-t>` while a program runs under the shell.
    EndShell,
    /// Folder symlinks Windows refused, offered again as junctions (Q46).
    Junctions { links: Vec<ops::Link> },
    /// `<F12>`'s report, shown before anything leaves the machine (Q62).
    BugReport { url: String },
}

pub struct ConfirmOverlay {
    pub title: String,
    pub body: Vec<String>,
    pub options: Vec<(char, String)>,
    pub action: ConfirmAction,
    /// The colliding destination, when the dialog offers a rename.
    pub dest: Option<PathBuf>,
}

impl ConfirmOverlay {
    /// What button `i` says. The first one also names `<Enter>`, which picks
    /// it: on the Report a bug panel that opens a browser, the one key in the
    /// box that leaves the program, and nothing on screen said so (Q69, #230).
    pub fn button_label(&self, i: usize) -> String {
        let (k, l) = &self.options[i];
        match i {
            0 => format!("[{k}] / <Enter> {l}"),
            _ => format!("[{k}] {l}"),
        }
    }
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

/// Which keymap layer a panel reads, and which dispatcher runs what it names.
///
/// Exists so [`App::feed_overlay_key`] can be one function rather than one per
/// panel. Overlays absent from `of` have no layer of their own: `Input`,
/// `Confirm` and `Pick` are native widgets, and `None` is the file list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PanelLayer {
    Help,
    Tasks,
    Spot,
    Diff,
}

impl PanelLayer {
    fn of(ov: &Overlay) -> Option<Self> {
        match ov {
            Overlay::Help => Some(Self::Help),
            Overlay::Tasks(_) => Some(Self::Tasks),
            Overlay::Spot(_) => Some(Self::Spot),
            Overlay::Diff(_) => Some(Self::Diff),
            Overlay::None | Overlay::Input(_) | Overlay::Confirm(_) | Overlay::Pick(_) => None,
        }
    }

    fn bindings(self, km: &crate::config::keymap::Keymap) -> &[crate::config::keymap::Binding] {
        match self {
            Self::Help => &km.help,
            Self::Tasks => &km.tasks,
            Self::Spot => &km.spot,
            Self::Diff => &km.diff,
        }
    }

    fn act(self, app: &mut App, a: Act) {
        match self {
            Self::Help => app.help_act(a),
            Self::Tasks => app.tasks_act(a),
            Self::Spot => app.spot_act(a),
            Self::Diff => app.diff_act(a),
        }
    }
}

/// Two files side by side. `outcome` is `None` until the diff worker answers,
/// which is what the view shows as *Comparing…*.
pub struct DiffOverlay {
    pub left: PathBuf,
    pub right: PathBuf,
    pub outcome: Option<diff::Outcome>,
    /// The first row drawn. A scroll position, not a cursor: there is nothing
    /// selected here, so the last useful value is the one that puts the last
    /// row at the bottom of the pane, not the one that puts it at the top.
    pub offset: usize,
    /// Rows the pane can show; set by the renderer, as `page_rows` is for the
    /// file list. Without it the keys cannot tell where the scrolling stops.
    pub rows: usize,
    /// The selected row, for a tree comparison. A file comparison has nothing to
    /// select -- the rows are lines, not things you act on -- so it leaves this
    /// where it is and shows no cursor. An index into [`DiffOverlay::shown`],
    /// not into the rows themselves, so that it means the row on screen
    /// whether or not `=` rows are hidden.
    pub cursor: usize,
    /// A tree comparison with its `=` rows hidden (`z`, Q23), so a large pair
    /// can be walked difference by difference. The footer still counts them.
    pub hide_same: bool,
    /// The folder comparison this file comparison was opened from with
    /// `<Enter>`, where `q` goes back to -- on the row it was opened from.
    pub back: Option<Box<DiffOverlay>>,
}

impl DiffOverlay {
    /// The worker's answer, in place. A tree opens on its first difference,
    /// not its first row: 497 matching rows of 500 used to fill the screen
    /// with no difference on it, and nothing said there was one until the
    /// footer was read (Q23). `gg` is still the top.
    pub fn arrive(&mut self, outcome: diff::Outcome) {
        if let diff::Outcome::Tree { rows, .. } = &outcome {
            self.cursor =
                rows.iter().position(|r| !matches!(r.state, diff::TreeState::Same)).unwrap_or(0);
        }
        self.outcome = Some(outcome);
    }

    /// The tree rows on screen, as indices into the outcome's rows: all of
    /// them, or all but the matches when `hide_same` is on. Empty for a file
    /// comparison, which has lines rather than rows.
    pub fn shown(&self) -> Vec<usize> {
        let Some(diff::Outcome::Tree { rows, .. }) = &self.outcome else { return Vec::new() };
        (0..rows.len())
            .filter(|&i| !(self.hide_same && matches!(rows[i].state, diff::TreeState::Same)))
            .collect()
    }
}

impl Overlay {
    pub fn is_none(&self) -> bool {
        matches!(self, Overlay::None)
    }

    /// Whether this one is a panel drawn over the file list.
    ///
    /// It decides who gets the wheel. The list is still painted underneath, and
    /// still sees the pointer over its own rect, so without this a turn of the
    /// wheel over a panel scrolled both -- the panel visibly and the list
    /// invisibly, which only showed up as a jumped cursor once the panel was
    /// closed. `Input` is not one of these: the prompt is a single row at the
    /// bottom and the list above it is being read while the filter is typed.
    pub fn is_modal(&self) -> bool {
        use Overlay::*;
        matches!(self, Help | Tasks(_) | Spot(_) | Diff(_) | Pick(_) | Confirm(_))
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
    /// The row's first line. The label already starts with the verb, so it is
    /// not put in front a second time (`Trash  Trash 5 item(s)`).
    pub fn headline(&self) -> String {
        format!("{}  [{}]", self.label, self.state.label())
    }

    /// The row's second line. A job that counts no bytes -- the trash, a
    /// delete -- shows files only: `0 B / 0 B` read as a row of empty files.
    pub fn detail(&self) -> String {
        let mut out = format!("{}/{} files", self.files_done, self.files);
        if self.bytes > 0 {
            out.push_str(&format!(
                " · {} / {}",
                crate::util::human_size(self.bytes_done),
                crate::util::human_size(self.bytes)
            ));
        }
        if let Some(s) = self.speed() {
            out.push_str(&format!(" · {}/s", crate::util::human_size(s)));
        }
        if let Some(eta) = self.eta() {
            out.push_str(&format!(" · {} left", crate::util::fmt_duration(eta)));
        }
        out
    }

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

/// How many keys [`App::recent_keys`] keeps.
const RECENT_KEYS: usize = 20;

fn keymap_render(k: &Key) -> String {
    crate::config::keys::render_seq(std::slice::from_ref(k))
}

/// How many toasts [`App::toast_log`] keeps.
pub const TOAST_LOG: usize = 16;

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
    /// Which picture of the file a configured previewer is showing: a PDF's
    /// page, a video's second. Belongs to the file, so it resets on the way to
    /// the next one — landing on page nine of a two-page document would be a
    /// puzzle with no visible cause.
    pub n: i64,
    /// The last `n` that produced a picture, so that stepping past the end can
    /// step back to it. There is no page count to clamp against; the end is
    /// only ever found by reaching it.
    pub n_ok: Option<i64>,
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
    const CAP: u32 = crate::preview::MAX_DECODE;
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
            n: 0,
            n_ok: None,
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
    /// What `a` made: the folders it had to make on the way, outermost first,
    /// then the thing asked for -- a file when `file`, else a folder. Taken
    /// back only while that is still empty: a file someone has since written
    /// in is not a slip any more, and removing it would lose the writing.
    Create { paths: Vec<PathBuf>, file: bool },
    /// Links made by `-`, `_` or `=`. Taking one back removes the link
    /// and never what it points at.
    Link { links: Vec<ops::Link> },
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
            Self::Create { paths, .. } => format!("Removed {}", created_name(paths)),
            Self::Link { links } => match links.len() {
                1 => format!("Removed the {} {}", link_word(links), util::file_name(&links[0].at)),
                n => format!("Removed {n} {}(s)", link_word(links)),
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
            Self::Create { paths, .. } => format!("Created {}", created_name(paths)),
            Self::Link { links } if links.iter().all(|l| l.junction) => match links.len() {
                1 => format!("Made the junction {} again", util::file_name(&links[0].at)),
                n => format!("Made {n} junctions again"),
            },
            Self::Link { links } => match links.len() {
                1 => format!("Linked {}", util::file_name(&links[0].at)),
                n => format!("Made {n} link(s)"),
            },
        }
    }
}

/// The `cd` prompt's starting text: the folder with this platform's separator
/// after it, ready for a name. It used to end in `\\` everywhere, so Linux
/// and macOS showed `/home/me\\` (found on the virtual display), and a root
/// that already ends in one (`/`, `C:\\`) is not given a second.
fn dir_prefill(dir: &Path) -> String {
    let shown = dir.display().to_string();
    match shown.ends_with(['/', '\\']) {
        true => shown,
        false => format!("{shown}{}", std::path::MAIN_SEPARATOR),
    }
}

/// What a typed path that named nothing says when its parent is shown instead
/// (#98): one sentence, whichever of the two listings arrived first (#242).
fn nothing_there(name: &str, shown: &Path) -> String {
    format!("No such file or folder: {name} — showing {}", shown.display())
}

/// What `u` calls the links it took back. A junction stays a junction in the
/// toast: `y` said `Made a junction`, and `u` then `U` used to say `link`, the
/// word for the symlink Windows had refused (#185).
fn link_word(links: &[ops::Link]) -> &'static str {
    if links.iter().all(|l| l.junction) { "junction" } else { "link" }
}

/// The name a create is known by: the thing asked for, the last of its paths.
fn created_name(paths: &[PathBuf]) -> String {
    paths.last().map(|p| util::file_name(p)).unwrap_or_default()
}

/// The paths `a` will have to make for `target`: it and every parent that is
/// not there yet, outermost first. Empty when `target` is already there, so
/// that `a` on an existing folder -- which `create_dir_all` lets through --
/// leaves nothing for `u` to remove.
fn paths_to_make(target: &Path) -> Vec<PathBuf> {
    let mut made: Vec<PathBuf> =
        target.ancestors().take_while(|p| !p.as_os_str().is_empty() && !ops::exists(p)).map(Path::to_path_buf).collect();
    made.reverse();
    made
}

/// Take back a create: the thing asked for goes only while it is still empty,
/// then each folder made on the way, as far as each is empty too. A parent
/// something else has been put into since stays, without an error: the slip
/// being undone is the one name, and that is gone.
fn unmake(paths: &[PathBuf], file: bool) -> std::io::Result<usize> {
    let Some((leaf, parents)) = paths.split_last() else { return Ok(0) };
    if file {
        if std::fs::metadata(leaf)?.len() > 0 {
            return Err(std::io::Error::other(format!("{} has been written to since", util::file_name(leaf))));
        }
        std::fs::remove_file(leaf)?;
    } else {
        // `remove_dir` refuses a folder with anything in it, which is the
        // check itself.
        std::fs::remove_dir(leaf)?;
    }
    // A parent someone has since put something in stays, and so does every
    // folder above it; the count is of the folders that actually went.
    Ok(parents.iter().rev().take_while(|dir| std::fs::remove_dir(dir).is_ok()).count())
}

/// What a finished `E` or `e` says it made. An unpacked folder ends in a
/// separator, so it reads as a folder and not as one more file.
fn made_says(kind: OpKind, made: &[PathBuf]) -> Option<String> {
    let folder = |p: &PathBuf| format!("{}{}", util::file_name(p), std::path::MAIN_SEPARATOR);
    match (kind, made) {
        (OpKind::Compress(_), [archive]) => Some(format!("Packed into {}", util::file_name(archive))),
        (OpKind::TakeOut, [one]) => Some(format!("Took {} out of the archive", util::file_name(one))),
        (OpKind::TakeOut, many) if !many.is_empty() => Some(format!("Took {} out of the archive", util::items(many.len()))),
        (OpKind::Extract, [one]) => Some(format!("Unpacked into {}", folder(one))),
        (OpKind::Extract, [first, rest @ ..]) => {
            Some(format!("Unpacked {} archives into {} and {} more", rest.len() + 1, folder(first), rest.len()))
        }
        _ => None,
    }
}

/// The toast for a create taken back. The folders made on the way go with it,
/// and the toast counts them, so that `u` after `a new/deep/note.txt` does not
/// read as if only the file went.
fn removed_label(step: &UndoStep, folders: usize) -> String {
    match folders {
        0 => step.undone_label(),
        n => format!("{} and {n} folder(s)", step.undone_label()),
    }
}

/// Make again what [`unmake`] took back.
fn remake(paths: &[PathBuf], file: bool) -> std::io::Result<()> {
    let Some((leaf, parents)) = paths.split_last() else { return Ok(()) };
    for dir in parents {
        match std::fs::create_dir(dir) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(e),
            _ => {}
        }
    }
    match file {
        true => std::fs::OpenOptions::new().write(true).create_new(true).open(leaf).map(|_| ()),
        false => std::fs::create_dir(leaf),
    }
}

/// Run `f` over each link, splitting them into those it worked on and those
/// it did not, with the first error.
fn each_link(
    links: Vec<ops::Link>,
    f: fn(&ops::Link) -> std::io::Result<()>,
) -> (Vec<ops::Link>, Vec<ops::Link>, Option<std::io::Error>) {
    let (mut done, mut left, mut first) = (Vec::new(), Vec::new(), None);
    for l in links {
        match f(&l) {
            Ok(()) => done.push(l),
            Err(e) => {
                first.get_or_insert(e);
                left.push(l);
            }
        }
    }
    (done, left, first)
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
    /// Yanked inside an archive (`l`): the paths name members, which `p`
    /// takes out of the archive rather than copying.
    pub from_archive: bool,
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
    /// Wheel movement not yet worth a whole row, kept so that it becomes one.
    /// One per surface that scrolls by rows: sharing a single remainder would
    /// make the view jump when the pointer crossed between them mid-turn.
    /// See [`crate::ui::wheel_whole`] for why the remainder has to be kept.
    pub term_scroll_rows: f32,
    pub preview_scroll_rows: f32,
    /// One per place a file list is drawn: the left of the split, and the
    /// middle column (the right of the split). The two lists of a split are
    /// two surfaces, and sharing one remainder let a quarter turn over the
    /// left finish a quarter turn over the right (#202, 19.7).
    pub list_scroll_rows: [f32; 2],
    /// What the terminal was last searched for, so the key repeats it.
    term_needle: String,
    /// The shell the pane started, as its first toast named it.
    term_shell: String,
    /// A file to put the cursor on when its directory is next read, ahead of
    /// whatever was hovered: an archive that did not exist a moment ago, so
    /// the listing that is on screen cannot select it yet (Q25). Spent by the
    /// next read of the current directory, so leaving that directory drops it.
    land_on: Option<PathBuf>,
    /// The last jump that failed and was taken back. The parent columns it
    /// asked for fail too, for the same reason, and each one used to add its
    /// own toast naming a fragment of the path (#107).
    cd_refused: Option<PathBuf>,
    /// A drag in flight between the panes.
    pub drag: Option<Drag>,
    /// Where each pane was drawn this frame, so a drop can be placed.
    pub pane_rects: Vec<(usize, egui::Rect)>,
    /// A drag let go this frame, and whether Shift was held: placed once
    /// every pane has been drawn, not by the pane it started in (#208).
    pub drag_released: Option<bool>,
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
    /// The terminal pane has the window. A third of the height is right for a
    /// shell and too little for a full-screen program, so this is the way out.
    pub max_term: bool,
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
    /// Every toast raised, newest last, at most [`TOAST_LOG`] of them --
    /// including those that have already faded. `FILER_KEYS_DONE` writes it
    /// out, so a check whose expected result is a toast does not have to
    /// catch it on screen before it goes (#176, #190, #196).
    pub toast_log: std::collections::VecDeque<String>,
    /// The last keys pressed, rendered, newest last, for `<F12>`'s report
    /// (Q64). Keys only: text typed into a prompt is not kept.
    pub recent_keys: std::collections::VecDeque<String>,
    /// The newest error raised, for the same report.
    pub last_error: Option<String>,
    /// The report link `<F12>` last opened or copied, for the state file.
    pub last_report: Option<String>,
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
    /// What each running `D` is deleting, for the toast its end replaces.
    deleting: HashMap<u64, Vec<PathBuf>>,
    /// What each running cut paste emptied the register of, to put back when
    /// nothing moved (Q72: a declined overwrite used to lose the cut).
    cut_jobs: HashMap<u64, Vec<PathBuf>>,

    pub search: Option<crate::search::Handle>,
    /// The disk-usage walk, while one is running. Dropping it stops the walk, so
    /// leaving the view is all it takes to end one.
    pub usage: Option<crate::fs::usage::Handle>,
    /// The biggest total in the usage view, which the rows' bars are drawn
    /// against. Zero when the view is not up.
    pub usage_max: u64,
    /// Where `gu` was pressed. `h` climbs inside the view down to here, and
    /// leaves it from here.
    usage_root: Option<PathBuf>,
    /// The archive `l` went into (Q75), while its members are on screen.
    archive_view: Option<ArchiveView>,
    /// A listing or a member copy on its way from a worker; see
    /// [`App::drain_archive`].
    archive_rx: Option<crossbeam_channel::Receiver<ArchiveMsg>>,
    /// A member being unpacked for the preview, apart so that it never
    /// stands in the way of the listing or of `l` opening a copy.
    archive_preview_rx: Option<crossbeam_channel::Receiver<ArchiveMsg>>,
    /// The folder `h` came up out of, put under the cursor once the parent's
    /// walk reports it.
    usage_want: Option<String>,
    /// The tab's linemode from before `gu`, put back on the way out. The view
    /// shows sizes whatever the tab was showing (Q33): its bars alone could
    /// not be read as numbers, and `linemode = "usage"` in the config made every
    /// ordinary folder read `0 B` instead.
    usage_linemode: Option<crate::fs::entry::Linemode>,
    /// What `<A-t>` asked to type into a pane it had to open first, held until
    /// the shell has drawn something (Q35); see [`App::pump_terminal`].
    term_pending: Option<(Vec<u8>, Instant)>,
    pub ctx: egui::Context,
    /// How much bigger everything is drawn. Held here rather than read back
    /// from egui: `set_zoom_factor` only takes effect at the start of the next
    /// frame, so stepping from what egui currently reports would lose every
    /// press after the first in any one frame.
    pub scale: f32,
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
    /// The first line of the help panel drawn. A scroll position, not a cursor,
    /// so the last useful value is the one that puts the last line at the bottom
    /// of the panel rather than at the top.
    pub help_scroll: usize,
    /// Lines the help panel can show, and how many it has. Set by the renderer,
    /// the way `page_rows` is for the file list and `rows` for the comparison:
    /// without them the keys cannot tell where a page ends or where scrolling
    /// stops, and `help_scroll` ran off past the end of the list -- every press
    /// back up then moved a number nothing was drawing from, so the keys looked
    /// dead for as many presses as the reader had overshot.
    pub help_rows: usize,
    pub help_lines: usize,
    /// The wheel's leftover fraction of a row over the help panel.
    pub help_scroll_rows: f32,
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
        let tab = Tab::new(start, sort, cfg.yazi.mgr.show_hidden, cfg.yazi.mgr.linemode);
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
            term_scroll_rows: 0.0,
            preview_scroll_rows: 0.0,
            list_scroll_rows: [0.0; 2],
            term_needle: String::new(),
            term_shell: String::new(),
            land_on: None,
            cd_refused: None,
            drag: None,
            pane_rects: Vec::new(),
            drag_released: None,
            differ,
            spotter,
            spotted: None,
            pending: Vec::new(),
            which: Vec::new(),
            overlay: Overlay::None,
            pending_bookmark: None,
            yank: Yank { paths: Vec::new(), cut: false, from_archive: false },
            preview: PreviewSlot::default(),
            max_preview: false,
            max_term: false,
            quick: false,
            hide_parent: false,
            render_markdown,
            bold_font: false,
            refont: false,
            tasks: Vec::new(),
            toasts: Vec::new(),
            toast_log: std::collections::VecDeque::new(),
            recent_keys: std::collections::VecDeque::new(),
            last_error: None,
            last_report: None,
            launches: Vec::new(),
            bookmarks: Vec::new(),
            history: Vec::new(),
            undos: Undos::default(),
            op_undo: HashMap::new(),
            cut_jobs: HashMap::new(),
            deleting: HashMap::new(),
            search: None,
            usage: None,
            usage_max: 0,
            usage_linemode: None,
            usage_root: None,
            archive_view: None,
            archive_rx: None,
            archive_preview_rx: None,
            usage_want: None,
            term_pending: None,
            ctx,
            scale: 1.0,
            pending_conflict: None,
            dirty: HashMap::new(),
            counted: std::collections::HashSet::new(),
            pending_completion: None,
            inflight: HashMap::new(),
            quit: false,
            cwd_file: None,
            chooser_file: None,
            help_scroll: 0,
            help_rows: 0,
            help_lines: 0,
            help_scroll_rows: 0.0,
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
            Some(PendingCd { from: home, pushed: false, fallback: true, reveal: None });
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
        let text = text.into();
        self.last_error = Some(text.clone());
        self.raise(text, Level::Error);
    }

    /// Keep `k` among the last keys pressed.
    fn note_key(&mut self, k: &Key) {
        if self.recent_keys.len() == RECENT_KEYS {
            self.recent_keys.pop_front();
        }
        self.recent_keys.push_back(keymap_render(k));
    }

    /// A toast that supersedes the others of its family -- those starting
    /// with `family` -- rather than standing beside them. `At the last
    /// difference` and `At the first difference` side by side contradicted
    /// each other; the newer is the true one.
    fn toast_instead(&mut self, family: &[&str], text: String) {
        self.toasts.retain(|t| t.text == text || !family.iter().any(|f| t.text.starts_with(f)));
        self.toast(text);
    }

    /// Put a line up, or tick the one already saying it.
    ///
    /// Repeating rather than stacking: the same text arriving again is the same
    /// news, and stacking it spends the five slots the toast area has on one
    /// message. The timer restarts so a repeat stays up as long as a first.
    fn raise(&mut self, text: String, level: Level) {
        if self.toast_log.len() == TOAST_LOG {
            self.toast_log.pop_front();
        }
        self.toast_log.push_back(text.clone());
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
    ///
    /// Not in a host's listing (`\\server`): its rows are shares, and counting
    /// one is a `read_dir` across the network for a number nobody needs while
    /// picking a share. Its size column stays empty (Q32, TESTING.md 31.9).
    fn ensure_dir_sizes(&mut self) {
        if !self.tabs[self.active].linemode.wants_dir_size() {
            return;
        }
        if util::host_only_unc(&self.tabs[self.active].cwd) {
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
                    ov.arrive(res.outcome);
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
        self.drain_usage();
        self.drain_archive();
        self.sync_spot();
        self.pump_terminal();
        self.flush_dirty();
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(6));
        self.tasks.retain(|t| match t.finished {
            Some(at) => at.elapsed() < Duration::from_secs(20) || !t.errors.is_empty(),
            None => true,
        });
    }

    /// How long a flagged directory waits for the watcher to go quiet before it
    /// is read again — editors and installers touch a directory many times in a row.
    const RESCAN_QUIET: Duration = Duration::from_millis(150);

    /// Rescan directories the watcher flagged, once they have been quiet for
    /// [`Self::RESCAN_QUIET`].
    fn flush_dirty(&mut self) {
        if self.dirty.is_empty() {
            return;
        }
        let ready: Vec<PathBuf> = self
            .dirty
            .iter()
            .filter(|(_, t)| t.elapsed() > Self::RESCAN_QUIET)
            .map(|(p, _)| p.clone())
            .collect();
        for p in ready {
            self.dirty.remove(&p);
            self.cache.remove(&p);
            self.rescan(&p);
        }
    }

    /// When the next frame is owed to a flagged directory, if one is waiting.
    ///
    /// The watcher's own wake arrives the instant a change does, when the entry
    /// is not yet quiet enough to read, and an idle window asks for no frame
    /// after it -- so the change stayed off the screen until a key was pressed
    /// (#108). The frame loop asks for one at this moment instead; with nothing
    /// flagged it asks for none, and an idle window still costs nothing (47).
    pub fn rescan_due(&self) -> Option<Duration> {
        let oldest = self.dirty.values().map(|t| t.elapsed()).max()?;
        Some(Self::RESCAN_QUIET.saturating_sub(oldest) + Duration::from_millis(10))
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
                            hit = true;
                        }
                    }
                }
                // Only when nothing on screen carries it. A pane or the preview
                // that just took `LoadState::Error` is already showing this, in
                // place and in the same words; a toast on top says it twice, and
                // opening `C:\` says it three times at once -- one per system
                // folder Windows refuses -- with the toasts stacked over the
                // pane that already explained itself.
                // A folder above a jump still waiting, or above the one just
                // taken back, fails for the same reason; that jump says it.
                let explained = self.tabs.iter().any(|t| t.pending_cd.is_some() && t.cwd.starts_with(&path))
                    || self.cd_refused.as_ref().is_some_and(|p| p.starts_with(&path));
                if !hit && !explained {
                    self.error(format!("{}: {error}", util::file_name(&path)));
                }
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

        // A view standing in for the listing (search, usage) keeps the pane;
        // the jump still stands.
        if self.tabs[self.active].cwd == path && self.tabs[self.active].current.path != path {
            self.tabs[self.active].pending_cd = None;
        } else if self.tabs[self.active].cwd == path {
            // The directory answered, so the jump that led here stands.
            let reveal = self.tabs[self.active].pending_cd.take().and_then(|p| p.reveal);
            if let Some(name) = reveal.filter(|n| !entries.iter().any(|e| &e.name == n)) {
                // A typed path that named a file lands here with the file under
                // the cursor. One that named nothing used to land here too, in
                // silence, and looked like the place that was asked for.
                self.error(nothing_there(&name, path));
            }
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
            let land = self.land_on.take().filter(|p| p.parent() == Some(path));
            if let Some(p) = land {
                let name = util::file_name(&p);
                self.tabs[self.active].memo.insert(path.to_path_buf(), name.clone());
                self.tabs[self.active].current.select_name(&name);
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
        // The debounce's timer, taken rather than read: every way out of this
        // function below leaves it cleared except the one that is still
        // waiting, which puts it back. `main` repaints every 16 ms while it is
        // set, and each early return used to leave it set for good -- an idle
        // window drawing 60 frames a second, minimised or not (#86).
        let pending = self.preview.pending_since.take();
        let Some(mut entry) = self.tabs[self.active].current.hovered().cloned() else {
            self.preview.state = PreviewState::Empty;
            self.preview.key = None;
            self.preview.outline = None;
            self.preview.outline_wanted = None;
            return;
        };
        // Inside an archive a small member is unpacked on a worker and its
        // copy previewed like any file; until then, and for a folder or a
        // big one, the card says what it is.
        let ctx = self.ctx.clone();
        if let Some(view) = &mut self.archive_view {
            match view.copies.get(&entry.path) {
                Some(real) => entry = Entry { path: real.clone(), ..entry },
                None => {
                    let wanted = !entry.is_dir_like()
                        && entry.len <= ARCHIVE_PREVIEW_LIMIT
                        && view.copying.is_none()
                        && !view.refused.contains(&entry.path);
                    if wanted {
                        let member = match view.inner.is_empty() {
                            true => entry.name.clone(),
                            false => format!("{}/{}", view.inner, entry.name),
                        };
                        let dest = view.inner.split('/').filter(|p| !p.is_empty()).fold(view.scratch.clone(), |p, part| p.join(part));
                        let (archive, fake) = (view.archive.clone(), entry.path.clone());
                        let real = dest.join(&entry.name);
                        let (tx, rx) = crossbeam_channel::bounded(1);
                        std::thread::Builder::new()
                            .name("archive-preview".into())
                            .spawn(move || {
                                let got = archive::extract_one(&archive, &member, &dest, &mut |_, _| true).map(|()| real);
                                let _ = tx.send(ArchiveMsg::Previewed(fake, got));
                                ctx.request_repaint();
                            })
                            .expect("spawn archive thread");
                        view.copying = Some(entry.path.clone());
                        self.archive_preview_rx = Some(rx);
                    }
                    self.preview.state = PreviewState::Ready(view.card(&entry));
                    self.preview.key = None;
                    self.preview.texture = None;
                    return;
                }
            }
        }
        // The outline and the zoom belong to the file they were set on. Without
        // this, walking onto the next image shows a corner of it at 8x.
        let rule = crate::config::PreviewRule::for_path(&self.cfg.preview, &entry.path).cloned();
        if self.preview.key.as_ref().is_none_or(|k| k.path != entry.path) {
            self.preview.outline = None;
            self.preview.zoom = None;
            self.preview.pan = egui::Vec2::ZERO;
            self.preview.n = rule.as_ref().map_or(0, |r| r.first);
            self.preview.n_ok = None;
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
            // Markdown and CSV are laid out to the pane's width, so both have to
            // be re-read when the window is resized; nothing else does. Leaving
            // CSV out here is not a small bug: `cols == 0` reads as 80 further
            // down, so the table would sit at a fixed 80 columns for ever and
            // never reflow.
            cols: if matches!(mime, "text/markdown" | "text/csv") { self.preview.cols } else { 0 },
            n: self.preview.n,
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
        match pending {
            Some(t) if t.elapsed() >= debounce => {}
            Some(t) => {
                self.preview.pending_since = Some(t);
                return;
            }
            None if !debounce.is_zero() => {
                self.preview.pending_since = Some(Instant::now());
                return;
            }
            None => {}
        }

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
            preview: rule,
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
        // Running off the end of a document stops, rather than replacing the
        // page with an error. `<A-j>` past the last page is the same gesture
        // as `j` at the bottom of the list, and that one simply does not move.
        // Nothing knows how many pages there are -- the command refusing *is*
        // the end -- so it is found by arriving at it, and then stepped back.
        if let (Payload::Error(why), Some(n_ok)) = (&res.payload, self.preview.n_ok) {
            if res.key.n != n_ok && self.is_external_preview(&res.key.path) {
                let said = why.clone();
                self.preview.n = n_ok;
                self.toast(format!("No more: {said}"));
                return;
            }
        }
        if !matches!(res.payload, Payload::Error(_)) && self.is_external_preview(&res.key.path) {
            self.preview.n_ok = Some(res.key.n);
        }
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
    /// Whether what the last key started has landed: the listing is read, the
    /// preview is up, no file job is still running, and an open spot panel or
    /// comparison has its answer. `--keys` waits on this between presses, as a
    /// person would wait to see.
    pub fn settled(&self) -> bool {
        let tab = &self.tabs[self.active];
        // A usage walk adds rows as it goes, so a key pressed mid-walk lands on
        // whatever row happened to be there; #122 typed 600 `k`s to wait one out.
        // A job says how it went in a toast when it finishes, so `u<Shot:x>`
        // pictured `Restore 0%` and missed the toast it was taken for (#168).
        // A row that wants the job mid-run says `<Now>`, as 12.14 does. A job
        // asking about a name already taken waits on the next key, not the
        // other way round.
        let job_going = self.tasks.iter().any(|t| matches!(t.state, TaskState::Queued | TaskState::Running));
        if self.usage.is_some() || matches!(tab.current.state, LoadState::Loading)
            || (job_going && !matches!(self.overlay, Overlay::Confirm(_)))
            || self.preview.pending_since.is_some()
            || matches!(self.preview.state, PreviewState::Loading)
        {
            return false;
        }
        match &self.overlay {
            Overlay::Spot(_) => {
                let hovered = tab.current.hovered().map(|e| &e.path);
                hovered.is_none() || self.spotted.as_ref().map(|(p, _)| p) == hovered
            }
            Overlay::Diff(ov) => ov.outcome.is_some(),
            _ => true,
        }
    }

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
    pub(crate) fn outline_source_line(&self, k: usize) -> Option<usize> {
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
                // The same ceiling `seek` takes, and for the same reason: an
                // entry near the end of a file sits past the furthest the pane
                // can be scrolled, so jumping to it handed the draw an offset
                // beyond the content. The draw paints that frame and corrects
                // the number afterwards, which is one wrong frame per press --
                // invisible on a tap, and a flicker under a held key. This
                // path was missed when `seek` was clamped, which is why it
                // survived: it only shows on the last entry or two, and only
                // in a file whose last heading is near the end.
                self.tabs[self.active].preview_offset = line.min(self.preview.max_offset);
                true
            }
            Act::Seek(_) | Act::MaxPreview | Act::MaxTerm => false,
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
            Payload::Text { extent, outline, .. } => {
                row("Lines", extent.lines());
                if !outline.is_empty() {
                    row("Outline", format!("{} entries", outline.len()));
                }
                if let Some(r) = read(extent.truncated) {
                    row("Read", r);
                }
            }
            Payload::Markdown { doc, extent, .. } => {
                row("Lines", extent.lines());
                // A table stopped at its row cap says so here too (#205).
                match extent.rows {
                    Some(n) => row("Table", format!("first {n} rows only")),
                    None => row("Headings", doc.toc.len().to_string()),
                }
                if let Some(r) = read(extent.truncated) {
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

    /// The task panel's own commands: move, pause, cancel, reorder.
    /// Feed a key to the panel in front, through its own keymap layer.
    ///
    /// The four panels resolve keys identically -- push onto `pending`, resolve
    /// against the layer, run what a hit names, clear on a miss -- and differed
    /// only in which layer and which dispatcher. They were four copies of this,
    /// and adding a panel meant writing a fifth. The terminal and the file list
    /// are genuinely different (bytes for the shell, the `which` panel and a
    /// pending bookmark) and keep their own.
    pub fn feed_overlay_key(&mut self, k: Key) {
        let Some(layer) = PanelLayer::of(&self.overlay) else { return };
        self.note_key(&k);
        self.pending.push(k);
        let acts = match keymap::resolve(layer.bindings(&self.cfg.keymap), &self.pending) {
            keymap::Match::Exact(b) => b.run.clone(),
            // More keys may still follow, so what has been pressed stays.
            keymap::Match::Pending(_) => return,
            keymap::Match::None => {
                self.pending.clear();
                return;
            }
        };
        self.pending.clear();
        for a in acts {
            // The size of the window belongs to the window, not to whichever
            // panel happens to be over it. Handled here rather than in each
            // layer because all four `act`s end in a `_ => {}`: v0.46.0 bound
            // these chords in every panel and shipped them doing nothing, the
            // binding resolving and the action then being dropped in silence.
            // A fifth panel inherits this instead of repeating the mistake.
            if let Act::Scale(to) = a {
                self.scale(to);
                continue;
            }
            layer.act(self, a);
        }
    }

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

    /// The help panel's own keys, through the `[help]` layer like every other
    /// overlay.
    ///
    /// `j`, `k` and the arrows used to be read straight off the event in
    /// `main.rs`, which is why the panel that exists to show what the keys are
    /// was the one place they could not be changed -- and why it had no way to
    /// move by more than a line at a time through a list as long as the keymap.
    fn help_act(&mut self, a: Act) {
        match a {
            Act::Close | Act::Escape(_) | Act::Quit | Act::Help => self.overlay = Overlay::None,
            // A page is the panel's own height, not the file list's: "half a
            // page" has to mean half of what is on screen here. The stop is a
            // scroll position, so it is one past `lines - rows`, not `lines - 1`.
            Act::Arrow(step) => {
                let page = self.help_rows.max(1);
                let stop = self.help_lines.saturating_sub(page) + 1;
                self.help_scroll = step.apply(self.help_scroll, stop, page);
            }
            // What the panel's own config rows tell you to press (#163).
            Act::ConfigReload => self.act(Act::ConfigReload),
            // The whole list as text, as spot's `C` does (Q48).
            Act::Copy(CopyWhat::All) => {
                let (text, keys) = crate::ui::overlay::help_text(self);
                match exec::set_clipboard(&text) {
                    Ok(()) => self.toast(format!("Copied the help panel: {keys} keys")),
                    Err(err) => self.error(format!("Clipboard: {err}")),
                }
            }
            _ => {}
        }
    }

    /// yazi's spot commands: close, arrow (rows), swipe (files), copy (cell).
    fn spot_act(&mut self, a: Act) {
        let rows: usize = self.spot_sections().iter().map(|s| s.rows.len()).sum();
        let page = self.tabs[self.active].page_rows.max(1);
        let pr_url = if matches!(a, Act::Enter) { self.spot_pr_url() } else { None };
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
            // `<Enter>` on a pull request's rows opens its page (Q20), before
            // anything moves: the rows are what the cursor is on.
            Act::Enter if pr_url.is_some() => {
                let url = pr_url.unwrap_or_default();
                match exec::open_url(&url) {
                    Ok(()) => self.toast(format!("Opened {url}")),
                    Err(e) => self.error(format!("could not open the browser: {e}")),
                }
            }
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
            // Everything at once, labelled, so it can be pasted into a bug
            // report or a chat as text rather than a screenshot (Q18).
            Act::Copy(CopyWhat::All) => {
                let sections = self.spot_sections();
                let text = spot_text(&sections);
                let rows: usize = sections.iter().map(|s| s.rows.len()).sum();
                match exec::set_clipboard(&text) {
                    Ok(()) => self.toast(format!("Copied the spot panel: {rows} rows")),
                    Err(err) => self.error(format!("Clipboard: {err}")),
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

    /// The page the spot cursor is on, when it is on one: the pull request
    /// from its `Came in via` row or its own row, the branch from `From branch`.
    fn spot_pr_url(&self) -> Option<String> {
        let Overlay::Spot(ov) = &self.overlay else { return None };
        let sections = self.spot_sections();
        let mut at = ov.cursor;
        for s in &sections {
            if at < s.rows.len() {
                return page_for(&s.rows, &s.rows[at].0);
            }
            at -= s.rows.len();
        }
        None
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
        self.cd_refused = None;
        // A jump from inside an archive leaves it.
        self.archive_view = None;
        self.archive_rx = None;
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
        let pending = PendingCd { from, pushed: push_history, fallback, reveal: None };
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
        // A listing already here answers a jump's question now: the scan for
        // the parent column often lands before the jump's own fails, and the
        // pending jump -- with the name it was to reveal -- used to be dropped
        // unasked, so a typed path that named nothing landed in silence about
        // one run in two (#242).
        if listed {
            if let Some(name) = pending.as_ref().and_then(|p| p.reveal.as_deref()) {
                if !self.tabs[idx].current.entries.iter().any(|e| e.name == name) {
                    self.error(nothing_there(name, &target));
                }
            }
        }
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
            CdFallout::Revert { to, asked } => {
                self.arrive(idx, to, None);
                self.kick_scans();
                let path = asked.as_deref().unwrap_or(path);
                self.error(format!("{}: {error}", path.display()));
                self.cd_refused = Some(path.to_path_buf());
            }
        }
    }

    /// `<Esc>` while a jump is still waiting for its first listing: go back to
    /// where it came from. A mistyped address held the tab for twenty seconds
    /// with nothing to do but wait (#105). The scan itself cannot be called
    /// off, so its answer is let through quietly when it comes.
    fn abandon_cd(&mut self) -> bool {
        let tab = &mut self.tabs[self.active];
        if tab.current.state != LoadState::Loading {
            return false;
        }
        let Some(p) = tab.pending_cd.take() else { return false };
        if p.pushed {
            tab.back.pop();
        }
        let gone = tab.cwd.clone();
        self.arrive(self.active, p.from, None);
        self.kick_scans();
        self.cd_refused = Some(gone.clone());
        self.toast(format!("Stopped waiting for {}", gone.display()));
        true
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
        self.archive_view = None;
        self.archive_rx = None;
        self.archive_preview_rx = None;
        // Dropping the handle cancels the walk, so leaving is all it takes.
        self.usage = None;
        self.usage_max = 0;
        self.usage_root = None;
        self.usage_want = None;
        if let Some(mode) = self.usage_linemode.take() {
            self.tabs[self.active].linemode = mode;
        }
        // It said the walk was still going, which stopped being true just now
        // (#109).
        self.toasts.retain(|t| !t.text.starts_with("Measuring"));
        // And the total told how to leave a view that is no longer up.
        self.toasts.retain(|t| !t.text.ends_with("<Esc> to leave"));
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
        if self.archive_view.is_some() {
            match entry.is_dir_like() {
                true => self.archive_down(&entry.name),
                false => self.open_member(&entry),
            }
            return;
        }
        // An archive is gone into like a folder (Q75). Only from a real
        // listing: from a search or usage result `h` would not know where
        // to come back to.
        if !entry.is_dir_like() && !self.in_search_view() && archive::Format::from_path(&entry.path).is_some() {
            self.open_archive(entry.path);
            return;
        }
        // In the usage view a folder is measured in turn, the way `ncdu` goes
        // down: the view stays, and `h` comes back up.
        if entry.is_dir_like() && self.in_usage_view() {
            self.remeasure(entry.path, None);
        } else if entry.is_dir_like() {
            self.cd(entry.path, true);
        } else if !self.focus_outline() && !self.preview_ready() {
            // Still loading: move in once it arrives. A file without an
            // outline just stays where it is; opening is `open`'s job.
            self.preview.outline_wanted = Some(entry.path);
        }
    }

    fn leave(&mut self) {
        if let Some(view) = &self.archive_view {
            match view.inner.rsplit_once('/') {
                Some((up, name)) => {
                    let (up, name) = (up.to_owned(), name.to_owned());
                    self.archive_show(up, Some(name));
                }
                None if !view.inner.is_empty() => {
                    let name = view.inner.clone();
                    self.archive_show(String::new(), Some(name));
                }
                None => self.exit_search_view(),
            }
            return;
        }
        let cwd = self.tabs[self.active].cwd.clone();
        if self.in_usage_view() && self.usage_root.as_ref().is_some_and(|r| cwd.starts_with(r) && *r != cwd) {
            if let Some(p) = util::parent_dir(&cwd) {
                self.remeasure(p, Some(util::file_name(&cwd)));
                return;
            }
        }
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
        // Inside an archive the rows are not files yet, and the folder `p`
        // or `a` would write into is the one the archive sits in, not the one
        // on screen. These say so rather than act on paths that are not
        // there; `y` is let through, and `p` outside takes the members out.
        if self.archive_view.is_some() && changes_files(&a) {
            self.toast("Inside an archive: read only — y then p in a folder takes a copy out, Esc leaves");
            return;
        }
        match a {
            Act::Noop | Act::Unsupported(_) => {
                if let Act::Unsupported(what) = a {
                    self.error(format!("Not supported: {what}"));
                }
            }
            Act::Escape(what) => self.escape(what),
            // Every panel that has a keymap layer of its own -- `help`, `tasks`,
            // `spot` -- binds `q` to `close`. Quick look and a maximized preview
            // deliberately have no layer, so that `j` and `k` keep walking the
            // list underneath, which also meant `q` fell through to this arm and
            // quit the process with a panel still on screen. Peel the front layer
            // instead, through `escape` so the two keys cannot drift apart; a
            // second `q` quits, as it does in every panel.
            Act::Quit => {
                if self.quick || self.max_preview {
                    self.escape(EscapeWhat::default());
                } else {
                    self.quit = true;
                }
            }
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
                    self.open_input(InputKind::Cd, "Change directory", dir_prefill(&cwd));
                } else {
                    let base = self.tabs[self.active].cwd.clone();
                    self.cd(util::resolve_against(&base, &target), true);
                }
            }
            Act::Reveal(target) => self.reveal(target),
            Act::Follow => self.follow_link(),
            Act::Refresh => {
                let cwd = self.tabs[self.active].cwd.clone();
                self.cache.remove(&cwd);
                self.preview.cache.clear();
                self.counted.clear();
                self.rescan(&cwd);
                self.request_preview(true);
            }

            Act::Seek(step) => self.seek(step),

            Act::TabCreate { current, path } => self.create_tab(current, path),
            Act::TabClose(n) => {
                let idx = n.unwrap_or(self.active);
                self.close_tab(idx);
            }
            Act::TabSwitch { n, relative } => self.tab_switch(n, relative),
            Act::TabSwap(n) => self.tab_swap(n),

            Act::Split(state) => {
                if state.unwrap_or(self.split.is_none()) {
                    self.open_split();
                } else {
                    self.close_split();
                }
            }
            Act::PaneFocus(side) => self.pane_focus(side),

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

            Act::Hidden(state) => self.hidden(state),
            Act::Linemode(m) => self.tabs[self.active].linemode = m,
            Act::Sort { by, reverse, dir_first } => self.sort(by, reverse, dir_first),

            Act::Find { prev, smart, insensitive } => {
                let _ = (smart, insensitive);
                self.open_input(InputKind::Find { prev }, if prev { "Find previous" } else { "Find next" }, String::new());
            }
            Act::FindArrow { prev } => self.find_arrow(prev),
            // Only the comparison view has matching rows to hide; `diff_act` takes it there.
            Act::HideSame => {}
            Act::Filter { smart, insensitive } => self.filter(smart, insensitive),
            Act::Usage => self.start_usage(),
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
            // The same steps and bounds egui's own zoom used, so turning
            // that off and doing it here is not a change in feel.
            Act::Scale(to) => self.scale(to),
            Act::BugReport => self.bug_report(),
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
            Act::TermFind { prev, repeat } => self.term_search(prev, repeat),
            Act::TermScroll(step) => self.term_scroll(step),
            Act::Extract => self.do_extract(),
            Act::Compress => self.ask_compress(),
            Act::SendPane { cut } => self.send_to_pane(cut),
            Act::ToggleOutline => self.toggle_outline(),
            Act::ToggleRender => self.toggle_render(),

            // Nothing to maximise without a pane, and leaving the flag set
            // would surprise whoever opens one next.
            //
            // Maximising also hands the pane the keys. Pressed from the list,
            // it otherwise hides that list while leaving it holding them, so
            // the next keystroke goes somewhere off screen. Tying the two
            // together means maximised always implies the pane has the keys --
            // which is what makes leaving the pane the only way back.
            Act::MaxTerm => {
                self.max_term = self.term.is_some() && !self.max_term;
                if self.max_term {
                    self.term_focus = true;
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

    /// Move the keys to the other pane of a split.
    fn pane_focus(&mut self, side: Option<bool>) {
            // The first press splits the view; the next moves the keys.
            self.open_split();
            if let Some(sp) = self.split {
                if side.unwrap_or(!sp.right) != sp.right {
                    self.focus_pane(sp.other);
                }
            }
    }

    /// Narrow the listing to what matches as it is typed.
    fn filter(&mut self, smart: bool, insensitive: bool) {
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

    /// Go to a path and put the cursor on it, rather than merely into its folder.
    fn reveal(&mut self, target: String) {
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

    /// Search the terminal's scrollback, or step to the next match.
    fn term_search(&mut self, prev: bool, repeat: bool) {
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

    /// Show or hide the dotfiles, in every pane at once.
    fn hidden(&mut self, state: crate::config::cmd::Tri) {
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

    /// Zoom an image preview, which asks for a sharper decode as it grows.
    fn scale(&mut self, to: crate::config::cmd::ScaleTo) {
            let now = self.scale;
            let next = match to {
                crate::config::cmd::ScaleTo::In => now + 0.1,
                crate::config::cmd::ScaleTo::Out => now - 0.1,
                crate::config::cmd::ScaleTo::Reset => 1.0,
            };
            let next = (next.clamp(0.2, 5.0) * 10.0).round() / 10.0;
            self.scale = next;
            self.ctx.set_zoom_factor(next);
            // Held down, the toast's `×N` keeps counting after the scale has
            // stopped moving; saying so is what tells the two apart.
            let end = match to {
                crate::config::cmd::ScaleTo::In if next >= 5.0 => " (maximum)",
                crate::config::cmd::ScaleTo::Out if next <= 0.2 => " (minimum)",
                _ => "",
            };
            self.toast_instead(&["Scale "], format!("Scale {}%{end}", (next * 100.0).round() as i32));
    }

    /// Move the current tab along the bar.
    fn tab_swap(&mut self, n: i64) {
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

    /// Switch Markdown between the rendered view and its source, keeping the
    /// place: the two have different line numbers for the same content.
    fn toggle_render(&mut self) {
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

    /// Go to a tab by number, or step between them.
    fn tab_switch(&mut self, n: i64, relative: bool) {
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

    /// Walk the terminal's scrollback.
    fn term_scroll(&mut self, step: Step) {
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

    /// Scroll the preview, or step to the next picture where a configured
    /// previewer draws one at a time.
    fn seek(&mut self, step: Step) {
            let page = self.tabs[self.active].page_rows.max(1) as i64;
            let delta = match step {
                Step::Rel(n) => n,
                Step::Pct(p) => page * p / 100,
                Step::Top => i64::MIN / 2,
                Step::Bot => i64::MAX / 2,
            };
            // A configured previewer draws one picture at a time, so
            // there is nothing to scroll: the keys step to the next
            // picture instead. Same keys, because it is the same
            // intention -- further into this file.
            if let Some(rule) =
                crate::config::PreviewRule::for_path(&self.cfg.preview, &self.hovered_path())
            {
                let by = match step {
                    Step::Rel(n) => n.signum() * rule.step,
                    Step::Pct(p) => p.signum() * rule.step,
                    Step::Top => return self.go_to_picture(rule.first),
                    Step::Bot => return,
                };
                let want = self.preview.n + by;
                return self.go_to_picture(want.max(rule.first));
            }
            // Clamped here rather than only after the draw. The draw used
            // to be handed an offset past the end, paint a frame from
            // beyond the content, and fix the number afterwards — one bad
            // frame per keypress at the bottom of a file, which is why it
            // took a held-down key to see.
            let cur = self.tabs[self.active].preview_offset as i64;
            let want = cur.saturating_add(delta).max(0) as usize;
            self.tabs[self.active].preview_offset = want.min(self.preview.max_offset);
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
        // A maximized preview squeezes the list to nothing, so the screen reads
        // as modal even though only a column width changed. `Esc` doing nothing
        // there left `q` as the next thing to try, and `q` quits the app -- an
        // expensive way to find out that this one was not a panel. Restoring the
        // columns costs nothing if that is not what was wanted: `T` again.
        // Only for a bare `escape`; `escape --filter` and friends stay targeted.
        if all && self.max_preview {
            self.max_preview = false;
            return;
        }
        if all && self.abandon_cd() {
            return;
        }
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
            self.tabs[self.active].linemode,
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
            self.tabs[at].pending_cd = Some(PendingCd { from: base, pushed: false, fallback, reveal: None });
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
                Tab::new(src.cwd.clone(), src.sort, src.show_hidden, src.linemode);
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
        let from_archive = self.archive_view.is_some();
        self.yank = Yank { paths, cut, from_archive };
        let how = match (cut, from_archive) {
            (true, _) => " (cut)",
            (false, true) => " from the archive — p in a folder takes them out",
            (false, false) => "",
        };
        self.toast(format!("Yanked {}{how}", util::items(n)));
    }

    fn paste(&mut self, force: bool, _follow: bool) {
        if self.yank.paths.is_empty() {
            self.error("Nothing to paste");
            return;
        }
        let dest = self.tabs[self.active].cwd.clone();
        // Members of an archive are unpacked rather than copied; the register
        // stays, as a copy's does.
        if self.yank.from_archive {
            let srcs = self.yank.paths.clone();
            self.submit_op(OpKind::TakeOut, srcs, dest, force);
            return;
        }
        let kind = if self.yank.cut { OpKind::Move } else { OpKind::Copy };
        let mut srcs = self.yank.paths.clone();
        // A cut pasted where it already is would be renamed `same_1.txt`
        // without a word (#238); `<A-c>` and a drop refuse the same folder,
        // and so does this. What is cut elsewhere still moves.
        if self.yank.cut {
            srcs.retain(|p| p.parent() != Some(dest.as_path()));
            if srcs.is_empty() {
                self.toast("Already here — the cut is still there");
                return;
            }
        }
        let id = self.submit_op(kind, srcs, dest, force);
        if self.yank.cut {
            self.cut_jobs.insert(id, std::mem::take(&mut self.yank.paths));
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
        // Said, as `<A-c>` says it: let go over a list that showed the same
        // folder, the drop did nothing and nothing told you why (#208).
        if dest == self.tabs[drag.from].cwd {
            self.error("Both panes are in the same directory");
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
            self.error("Name it .zip, .7z, .tar or .tar.gz to say which format");
            return;
        };
        // Every format that can be read can be written, since v0.27.0. The
        // guard stays: `can_write` is what the writer asserts on, and a format
        // added for reading alone would otherwise reach it.
        if !format.can_write() {
            self.error(format!(
                "{} can be read here but not written — use .zip, .7z, .tar or .tar.gz",
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
        if kind == OpKind::Delete {
            self.deleting.insert(id, srcs.clone());
        }
        self.ops.submit(OpRequest { id, kind, srcs, dest_dir, dest_file, force });
        id
    }

    /// `D` went through: a red `Trash: <name>: …` still up about one of the
    /// files it took -- the `d` that failed and said to press `D` -- is no
    /// longer true, so it gives way to `Deleted <name>` (#207). Only that:
    /// a `D` with nothing to answer stays as quiet as it was.
    fn deleted(&mut self, paths: &[PathBuf]) {
        let names: Vec<String> = paths.iter().map(|p| util::file_name(p)).collect();
        let before = self.toasts.len();
        self.toasts.retain(|t| {
            t.level != Level::Error || !names.iter().any(|n| t.text.starts_with(&format!("Trash: {n}: ")))
        });
        if self.toasts.len() == before {
            return;
        }
        match names.as_slice() {
            [one] => self.toast(format!("Deleted {one}")),
            many => self.toast(format!("Deleted {} item(s)", many.len())),
        }
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
            ops::OpEvent::Finished { id, errors, cancelled, kind, moved, linked, junctions, made, trashed } => {
                // A move that actually moved something is a step `u` can take
                // back. A cancelled one is not: half a move is not a state
                // worth offering to reverse in one keystroke.
                // A cut whose paste moved nothing (every clash skipped, or the
                // whole thing cancelled) is still a cut: give it back, unless
                // something else has been yanked since.
                if let Some(cut) = self.cut_jobs.remove(&id) {
                    if moved.is_empty() && self.yank.paths.is_empty() {
                        self.yank = Yank { paths: cut, cut: true, from_archive: false };
                        if errors.is_empty() {
                            self.toast("Nothing moved — the cut is still there");
                        }
                    }
                }
                if kind == OpKind::Move && !cancelled && !moved.is_empty() {
                    self.undos.land(UndoStep::Move { pairs: moved }, Land::Fresh);
                }
                // Links are made one by one and each stands alone, so what was
                // made is a step even when a cancel or an error stopped the rest.
                if !linked.is_empty() {
                    let step = UndoStep::Link { links: linked };
                    // A new link barely changes the listing, so say it was
                    // made, as `d` does (#168). Not over an error: that toast
                    // comes below and is the one to read.
                    if errors.is_empty() && !cancelled {
                        self.toast(format!("{} — u to undo", step.redone_label()));
                    }
                    self.undos.land(step, Land::Fresh);
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
                if let Some(paths) = self.deleting.remove(&id) {
                    if errors.is_empty() && !cancelled {
                        self.deleted(&paths);
                    }
                }
                if !junctions.is_empty() {
                    self.offer_junctions(junctions);
                }
                if let Some((step, how)) = self.op_undo.remove(&id) {
                    if errors.is_empty() && !cancelled {
                        let said = match how {
                            Land::Undone => Some(step.undone_label()),
                            Land::Redone => Some(step.redone_label()),
                            // `d` looks the same as `D` on screen, so the trash
                            // says what went and how to get it back (#108).
                            Land::Fresh => matches!(step, UndoStep::Trash { .. })
                                .then(|| format!("{} — u to undo", step.redone_label())),
                        };
                        self.undos.land(step, how);
                        if let Some(said) = said {
                            self.toast(said);
                        }
                    } else if let (UndoStep::Trash { dir, .. }, Land::Fresh, false) = (&step, how, trashed.is_empty()) {
                        // Some went and some did not: what went is a step `u`
                        // can take back, and the rest is in the error above.
                        self.undos.land(UndoStep::Trash { paths: trashed, dir: dir.clone() }, Land::Fresh);
                    } else {
                        self.undos.keep(step, how);
                    }
                }
                let cwd = self.tabs[self.active].cwd.clone();
                // The archive just packed is what you want to look at next
                // (Q25) -- but only where you still are: a compress that ends
                // after you moved on does not pull you back.
                if let (OpKind::Compress(_) | OpKind::TakeOut | OpKind::Extract, Some(archive)) = (kind, made.first()) {
                    if archive.parent() == Some(cwd.as_path()) {
                        self.land_on = Some(archive.clone());
                    }
                }
                // Packing and unpacking change the listing by one name each,
                // easily missed, so the end says what was made (#174, #205).
                // Not over an error or a cancel: the toast above is the one
                // to read then.
                if errors.is_empty() && !cancelled {
                    if let Some(said) = made_says(kind, &made) {
                        self.toast(said);
                    }
                }
                self.cache.remove(&cwd);
                self.rescan(&cwd);
            }
        }
    }

    /// `<F12>`: what a bug report would carry, shown before anything leaves
    /// the machine (Q62). `o` / `<Enter>` opens the issue form with it filled
    /// in, `c` copies the link, `n` / `<Esc>` drops it. It used to open the
    /// browser on the press, and nobody saw what went with it -- which is
    /// fine for a version and an OS, and not for the keys and names Q64 adds.
    fn bug_report(&mut self) {
        // The `<F12>` itself is not part of what happened.
        let keys: Vec<String> = self.recent_keys.iter().rev().skip(1).rev().cloned().collect();
        let keys = keys.join(" ");
        let context = crate::bugreport::context(self.last_error.as_deref(), &self.cfg.loaded);
        let url = crate::bugreport::url(&[
            ("keys", if keys.is_empty() { String::new() } else { format!("Last keys, oldest first: {keys}") }),
            ("context", context.clone()),
        ]);
        let mut body = vec![crate::bugreport::version_line()];
        body.extend(crate::bugreport::os_line().lines().map(str::to_owned));
        body.push(format!("Last keys: {}", if keys.is_empty() { "(none)" } else { &keys }));
        body.extend(context.lines().map(str::to_owned));
        body.push(String::new());
        body.push("The form opens with these filled in; nothing is sent until you submit it there.".into());
        self.overlay = Overlay::Confirm(ConfirmOverlay {
            title: "Report a bug".into(),
            body,
            options: vec![
                ('o', "Open the form in your browser".into()),
                ('c', "Copy the link".into()),
                ('n', "Cancel".into()),
            ],
            action: ConfirmAction::BugReport { url },
            dest: None,
        });
    }

    /// The `o` to [`App::bug_report`]. When the browser cannot be opened,
    /// the URL goes on the clipboard so it can be pasted into one by hand --
    /// only then, so a report that worked never overwrites what was on the
    /// clipboard (Q22). The form's fields travel in the URL, so the pasted
    /// link is the whole report.
    fn open_report(&mut self, url: String) {
        match exec::open_url(&url) {
            Ok(()) => self.toast("Opened a bug report in your browser"),
            Err(e) => match exec::set_clipboard(&url) {
                Ok(()) => self.error(format!(
                    "could not open the browser: {e}. The report's link is on the clipboard -- paste it into one"
                )),
                Err(_) => self.error(format!("could not open the browser: {e}")),
            },
        }
        self.last_report = Some(url);
    }

    fn start_rename(&mut self, cursor: RenameCursor) {
        let Some(e) = self.tabs[self.active].current.hovered().cloned() else { return };
        let name = e.name.clone();
        let (stem, _ext) = util::stem_and_ext(&name);
        // A folder's name has no extension to keep, whatever dot is in it.
        let stem = if e.is_dir_like() { name.as_str() } else { stem };
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
        let Some(e) = self.tabs[self.active].current.hovered().cloned() else {
            // Quietly doing nothing left the last thing copied on the
            // clipboard, which looks like a valid path when pasted (#108).
            self.error("Nothing to copy — the list is empty");
            return;
        };
        let text = match what {
            // Outside the spot panel the hovered path is the only cell.
            CopyWhat::Path | CopyWhat::Cell | CopyWhat::All => e.path.display().to_string(),
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
        // An empty directory has nothing to say something about, so that one
        // stays quiet; a row that is simply not a link does not.
        let Some(e) = self.tabs[self.active].current.hovered().cloned() else { return };
        let Kind::Link { to_dir, broken } = e.kind else {
            // Naming the kind of thing the key is for, and how to spot one:
            // pressed on an ordinary file it did nothing at all, which is the
            // same as a key that is not bound to anything.
            self.error("Only a symlink can be followed — a link shows -> after its name");
            return;
        };
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
        // The usage view counted hidden files and lists them whatever the tab
        // shows; a re-sort that filtered them out left rows missing from the
        // total it still printed (#109).
        let in_usage = self.usage_linemode.is_some();
        let tab = &mut self.tabs[self.active];
        tab.current.resort(&sort, show || in_usage);
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
        // Inside an archive the members are not files yet: `<Enter>` opens a
        // copy, the way `l` does, and a folder is gone into.
        if self.archive_view.is_some() {
            self.enter();
            return;
        }
        // `<Enter>` on a folder is `l`, the usage view's going down included.
        if entry.is_dir_like() && !interactive {
            self.enter();
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
            // Said, as an opener's launch is: handed to the system, a file
            // whose app takes a while to appear looked as if `<Enter>` had
            // done nothing (#163).
            None => match exec::open_default(&entry.path) {
                Ok(()) => self.toast(format!("Opened {} with the system's default app", entry.name)),
                Err(e) => self.error(format!("Open failed: {e}")),
            },
        }
    }

    fn run_shell(&mut self, run: &str, block: bool, orphan: bool) {
        let paths = self.tabs[self.active].targets();
        let cwd = self.tabs[self.active].cwd.clone();
        let line = exec::substitute(run, &paths);
        // Off Windows the terminal already holds the window when the line
        // fails; there a console simply closes, so `:` waits for a key (Q13).
        let line = if block && cfg!(windows) { exec::held(&line) } else { line };
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
                crate::runinfo::remember_launch(line);
            }
            Err(e) => self.error(format!("{what}: {e}")),
        }
    }

    // ---------------------------------------------------------------- input

    /// A prompt that opens with text in it opens with that text selected, as
    /// Explorer's address bar and `F2` do: typing or pasting replaces it, and
    /// `<End>` keeps it to go on from (Q31). Two keep their own shape: a shell
    /// command is a template you add to, so the caret waits at its end, and an
    /// archive's name selects the part before `.zip`, which is what you change.
    pub fn open_input(&mut self, kind: InputKind, title: &str, text: String) {
        let len = text.chars().count();
        let selection = match kind {
            InputKind::Shell { .. } => (len, len),
            InputKind::Compress => (0, util::stem_and_ext(&text).0.chars().count()),
            _ => (0, len),
        };
        self.overlay = Overlay::Input(InputOverlay {
            kind,
            title: title.to_owned(),
            text,
            initial_selection: Some(selection),
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
        let made = paths_to_make(&target);
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
                // The cursor goes to what was made, as yazi's does (#250): to
                // `new` for `a new/deep/note.txt`, the part of it in this folder.
                // `land_on`, not the memo: the row the cursor was on when the
                // listing comes back would win over a memo.
                let here = target.strip_prefix(&base).ok().and_then(|r| r.components().next()).map(|c| base.join(c));
                self.land_on = here.or(Some(target.clone()));
                self.cache.remove(&base);
                self.rescan(&base);
                if !made.is_empty() {
                    self.undos.land(UndoStep::Create { paths: made, file: !as_dir }, Land::Fresh);
                }
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
            // Said like `d` and `-` say theirs: a rename was the one change
            // that went by in silence, though its undo is the one people use
            // most (#225).
            Ok(()) => {
                // Onto the new name, as after `a` (#250).
                self.land_on = Some(to.clone());
                let step = UndoStep::Rename { from: from.to_path_buf(), to };
                self.toast(format!("{} — u to undo", step.redone_label()));
                self.undos.land(step, Land::Fresh);
            }
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
        // Two folders are compared as trees; two files line by line. One of each
        // is neither, and there is nothing sensible to show for it.
        if a.1.is_dir_like() != b.1.is_dir_like() {
            return Err("compare two files, or two folders — not one of each".into());
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
        self.overlay =
            Overlay::Diff(DiffOverlay { left, right, outcome: None, offset: 0, rows: 1, cursor: 0, hide_same: false, back: None });
    }

    /// The compare view's own commands: scroll, jump between differences, close.
    fn diff_act(&mut self, a: Act) {
        let page = self.tabs[self.active].page_rows.max(1);
        let Overlay::Diff(ov) = &mut self.overlay else { return };
        // A tree comparison is a list with a cursor, so the same keys mean
        // something different there: `j` moves the selection rather than the
        // scroll, and `n`/`N` walk to the next path that is not a match.
        if matches!(&ov.outcome, Some(diff::Outcome::Tree { .. })) {
            let shown = ov.shown();
            let len = shown.len();
            let Some(diff::Outcome::Tree { rows, .. }) = &ov.outcome else { return };
            match a {
                Act::Close | Act::Escape(_) | Act::Quit | Act::Compare => {
                    self.overlay = Overlay::None
                }
                // The pair under the cursor, compared as files. Only a file on
                // both sides has a pair; anything else says why there is not.
                Act::Enter | Act::Open { .. } => {
                    let Some(row) = shown.get(ov.cursor).map(|&i| &rows[i]) else { return };
                    let why = match row.state {
                        diff::TreeState::LeftOnly | diff::TreeState::RightOnly => Some("it is on one side only"),
                        _ if row.dir => Some("that is a folder"),
                        _ => None,
                    };
                    if let Some(why) = why {
                        return self.error(format!("Compare: {why}"));
                    }
                    let (left, right) = (ov.left.join(&row.rel), ov.right.join(&row.rel));
                    let Overlay::Diff(tree) = std::mem::replace(&mut self.overlay, Overlay::None) else { return };
                    self.differ.request(diff::Request {
                        left: left.clone(),
                        right: right.clone(),
                        max_bytes: self.cfg.ui.max_text_bytes,
                    });
                    self.overlay = Overlay::Diff(DiffOverlay {
                        left,
                        right,
                        outcome: None,
                        offset: 0,
                        rows: 1,
                        cursor: 0,
                        hide_same: false,
                        back: Some(Box::new(tree)),
                    });
                }
                // Hidden or shown, the cursor stays on the row it was on; when
                // that row is a match being hidden, on the next one that is not.
                Act::HideSame => {
                    let at = shown.get(ov.cursor).copied().unwrap_or(0);
                    ov.hide_same = !ov.hide_same;
                    let now = ov.shown();
                    ov.cursor = now.iter().position(|&i| i >= at).unwrap_or(now.len().saturating_sub(1));
                    let what = if ov.hide_same { "Hiding matching rows" } else { "Showing matching rows" };
                    self.toast_instead(&["Hiding matching rows", "Showing matching rows"], what.into());
                }
                Act::Arrow(step) if len > 0 => ov.cursor = step.apply(ov.cursor, len, page),
                Act::FindArrow { prev } if len > 0 => {
                    let differs = |&i: &usize| !matches!(rows[i].state, diff::TreeState::Same);
                    let found = if prev {
                        shown[..ov.cursor].iter().rposition(differs)
                    } else {
                        shown[ov.cursor + 1..].iter().position(differs).map(|i| i + ov.cursor + 1)
                    };
                    match found {
                        Some(at) => ov.cursor = at,
                        None => {
                            let word = if prev { "first" } else { "last" };
                            self.toast_instead(&["At the "], format!("At the {word} difference"));
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        let rows: &[diff::Row] = match &ov.outcome {
            Some(diff::Outcome::Rows { rows, .. }) => rows,
            _ => &[],
        };
        match a {
            // Back to the folder comparison it was opened from, if any.
            Act::Close | Act::Escape(_) => {
                self.overlay = match ov.back.take() {
                    Some(tree) => Overlay::Diff(*tree),
                    None => Overlay::None,
                }
            }
            Act::Quit | Act::Compare => self.overlay = Overlay::None,
            // Against the last *scroll position*, not the last row. Clamping
            // to `len - 1` left `G` a screenful past where the pane can
            // actually sit, so the next `j` or `k` moved a number nothing was
            // drawing from and the keys looked dead -- for exactly as many
            // presses as the pane is tall, which is why a half-page `<C-u>`
            // appeared to wake them up.
            Act::Arrow(step) if !rows.is_empty() => {
                let stop = rows.len().saturating_sub(ov.rows.max(1)) + 1;
                ov.offset = step.apply(ov.offset, stop, page);
            }
            Act::FindArrow { prev } if !rows.is_empty() => {
                match diff::next_change(rows, ov.offset, prev) {
                    Some(at) => ov.offset = at,
                    None => {
                        let word = if prev { "first" } else { "last" };
                        self.toast_instead(&["At the "], format!("At the {word} difference"));
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
        let PreviewState::Ready(Payload::Image { own, vector, .. }) = self.preview.state else {
            return;
        };
        let from = self.preview.zoom.unwrap_or(self.preview.fit);
        self.preview.zoom = match to {
            ZoomTo::Fit => None,
            // An SVG at its own size in logical pixels, a raster one pixel to
            // one of the screen's (Q65).
            ZoomTo::Actual => Some(crate::preview::actual_zoom(own, vector, self.ctx.pixels_per_point())),
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
        let old_term = self.cfg.term.clone();
        let cfg = Config::reload(&mut self.cfg);
        let files = cfg.loaded.len();
        let warning = cfg.warnings.first().cloned();
        // A pane already running keeps the shell it started with; the new
        // `[term]` is for the next one. Two real-machine runs edited the shell,
        // reloaded, and read the old one back because nothing said so (#173).
        let shell_waits = self.term.is_some() && cfg.term != old_term;
        self.cfg = cfg;
        self.refont = true;
        // A theme change can turn every row a different color, and the preview
        // holds a highlighted copy of the old one.
        self.preview = PreviewSlot::default();
        match warning {
            Some(w) => self.warn(format!("Config: {w}")),
            None if shell_waits => self.toast(format!(
                "Reloaded {files} config file(s) — the pane keeps its shell until {}",
                self.term_close_key().map_or("it is closed".into(), |k| format!("{k} closes it"))
            )),
            None => self.toast(format!("Reloaded {files} config file(s)")),
        }
    }

    /// `-` on a folder without Developer Mode: Windows refused the symlink, and
    /// a junction would do without the privilege. Asked rather than done,
    /// because a junction is not the same thing -- it holds the full path where
    /// `-` would have written a relative one, and cannot reach a network
    /// location -- and swapping one kind for the other in silence would leave
    /// that to be found out later (Q46).
    fn offer_junctions(&mut self, links: Vec<ops::Link>) {
        const SHOWN: usize = 5;
        let mut body = vec![
            "Windows would not make the symlink: that needs Developer Mode or administrator.".to_owned(),
            "A junction needs neither. Unlike the symlink it holds the full path, not a relative one,".to_owned(),
            "and it cannot point at a network location.".to_owned(),
            String::new(),
        ];
        body.extend(links.iter().take(SHOWN).map(|l| format!("{}  →  {}", l.at.display(), l.target.display())));
        if links.len() > SHOWN {
            body.push(format!("… and {} more", links.len() - SHOWN));
        }
        let what = if links.len() == 1 { "Make the junction".to_owned() } else { format!("Make {} junctions", links.len()) };
        // `c` for someone who would rather make it themselves, or somewhere
        // else: the line is two absolute paths long, and a toast cannot be
        // copied from (#185, #191; Q56).
        let copy = if links.len() == 1 { "Copy the mklink command" } else { "Copy the mklink commands" };
        self.overlay = Overlay::Confirm(ConfirmOverlay {
            title: "Make a junction instead?".into(),
            body,
            options: vec![('y', what), ('c', copy.into()), ('n', "No".into())],
            action: ConfirmAction::Junctions { links },
            dest: None,
        });
    }

    /// The `y` to [`App::offer_junctions`]: made like any link, so `u` takes
    /// them back and `U` makes them again, as junctions.
    fn make_junctions(&mut self, links: Vec<ops::Link>) {
        let (done, left, err) = each_link(links, ops::Link::make);
        if let Some(l) = done.first().or(left.first()) {
            self.refresh_parent(&l.at.clone());
        }
        if !done.is_empty() {
            let said = match done.len() {
                1 => format!("Made a junction {} — u to undo", util::file_name(&done[0].at)),
                n => format!("Made {n} junctions — u to undo"),
            };
            self.undos.land(UndoStep::Link { links: done }, Land::Fresh);
            self.toast(said);
        }
        if let (Some(e), Some(l)) = (err, left.first()) {
            self.error(format!("Junction: {}: {e}", util::file_name(&l.at)));
        }
    }

    /// The junction question's `c`: the `mklink /J` lines, one per link, and
    /// nothing made. Each runs in any shell (see `ops::mklink_line`).
    fn copy_mklink(&mut self, links: &[ops::Link]) {
        let lines: Vec<String> = links.iter().map(|l| ops::mklink_line(&l.at, &l.target)).collect();
        match exec::set_clipboard(&lines.join("\n")) {
            Ok(()) => self.toast(match lines.len() {
                1 => "Copied the mklink command — it runs in cmd or PowerShell".to_owned(),
                n => format!("Copied {n} mklink commands — they run in cmd or PowerShell"),
            }),
            Err(e) => self.error(format!("Clipboard: {e}")),
        }
    }

    /// How the pane's keymap spells `terminal close`, for a message that tells
    /// someone to press it.
    fn term_close_key(&self) -> Option<String> {
        let b = self.cfg.keymap.term.iter().find(|b| b.raw == "terminal close")?;
        Some(crate::config::keys::render_seq(&b.on))
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
            UndoStep::Create { paths, file } => match unmake(&paths, file) {
                Ok(folders) => {
                    self.refresh_parent(&paths[0]);
                    let step = UndoStep::Create { paths, file };
                    self.toast(removed_label(&step, folders));
                    self.undos.land(step, Land::Undone);
                }
                Err(e) => {
                    self.error(format!("Undo: {e}"));
                    self.undos.keep(UndoStep::Create { paths, file }, Land::Undone);
                }
            },
            UndoStep::Link { links } => self.relink(links, ops::Link::remove, Land::Undone),
        }
    }

    /// Remove or make again a set of links. Each stands alone, so the ones
    /// that went through move to the other stack and the rest stay, named in
    /// the error -- `u` again once the way is clear finishes the job.
    fn relink(&mut self, links: Vec<ops::Link>, f: fn(&ops::Link) -> std::io::Result<()>, how: Land) {
        let (done, left, err) = each_link(links, f);
        if let Some(l) = done.first().or(left.first()) {
            self.refresh_parent(&l.at.clone());
        }
        let verb = if how == Land::Undone { "Undo" } else { "Redo" };
        if !done.is_empty() {
            let step = UndoStep::Link { links: done };
            self.toast(match how {
                Land::Undone => step.undone_label(),
                _ => step.redone_label(),
            });
            self.undos.land(step, how);
        }
        if let Some(e) = err {
            self.error(format!("{verb}: {e}"));
            self.undos.keep(UndoStep::Link { links: left }, how);
        }
    }

    /// List again the folder `p` is in, which `u` has just changed.
    fn refresh_parent(&mut self, p: &Path) {
        if let Some(dir) = p.parent() {
            let dir = dir.to_path_buf();
            self.cache.remove(&dir);
            self.rescan(&dir);
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
            UndoStep::Create { paths, file } => match remake(&paths, file) {
                Ok(()) => {
                    self.refresh_parent(&paths[0]);
                    let step = UndoStep::Create { paths, file };
                    self.toast(step.redone_label());
                    self.undos.land(step, Land::Redone);
                }
                Err(e) => {
                    self.error(format!("Redo: {e}"));
                    self.undos.keep(UndoStep::Create { paths, file }, Land::Redone);
                }
            },
            UndoStep::Link { links } => self.relink(links, ops::Link::make, Land::Redone),
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
            // Ending the shell ends whatever runs under it, and on screen that
            // looks just like `<C-t>` hiding the pane -- so a running program
            // is asked about first, and a prompt is not (Q21).
            if let Some(t) = self.term.as_ref().filter(|t| t.busy()) {
                let what = if t.title.is_empty() { "A program".to_owned() } else { format!("`{}`", t.title) };
                self.overlay = Overlay::Confirm(ConfirmOverlay {
                    title: "End the shell?".into(),
                    body: vec![format!("{what} is still running in the terminal, and ends with it.")],
                    options: vec![('y', "End it".into()), ('n', "Keep it".into())],
                    action: ConfirmAction::EndShell,
                    dest: None,
                });
                return;
            }
            let none = self.term.is_none();
            self.end_shell();
            if none {
                self.toast("No terminal to close");
            }
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
        // Empty means the default: `pwsh` on Windows when it is installed,
        // otherwise whatever the platform starts (Q29). A name here is the way
        // to ask for a particular one.
        let shell = match self.cfg.term.shell.is_empty() {
            false => Some((self.cfg.term.shell.clone(), self.cfg.term.args.clone())),
            true => crate::terminal::default_shell().map(|s| (s, Vec::new())),
        };
        let label = crate::terminal::shell_label(shell.as_ref().map(|(p, _)| p.as_str()));
        match crate::terminal::Terminal::spawn(&cwd, size, (8, 16), shell, move || {
            ctx.request_repaint()
        }) {
            Ok(t) => {
                self.term = Some(t);
                self.term_focus = true;
                self.toast(format!("Started {label} — <C-t> back to the list"));
                self.term_shell = label;
            }
            Err(e) => self.error(format!("Terminal failed: {e}")),
        }
    }

    /// Drop the terminal, which sends the shell its shutdown, and say so: the
    /// pane going away looks the same as `<C-t>` hiding it otherwise.
    fn end_shell(&mut self) {
        self.term_pending = None;
        let had = self.term.take().is_some();
        self.term_focus = false;
        self.max_term = false;
        if had {
            self.toast("Ended the shell");
        }
    }

    /// Type the selection into the shell, quoted so a path with a space in it
    /// arrives as one word. Nothing is run: the line is left for the user to
    /// put a command in front of.
    ///
    /// With the pane closed, it is opened first (Q35). A shell that is still
    /// reading its profile can drop what is typed at it, so the line waits
    /// until the shell has marked its prompt (OSC 133), or written something
    /// and then gone quiet for 800 ms (Q39, #250), or five seconds, whichever
    /// comes first.
    fn term_send_paths(&mut self) {
        let paths = self.tabs[self.active].targets();
        if paths.is_empty() {
            return;
        }
        let opened = self.term.is_none();
        if opened {
            self.terminal(Some(true));
        }
        let Some(term) = &self.term else { return };
        let how = term.quoting();
        let line: Vec<String> =
            paths.iter().map(|p| crate::terminal::quote(&p.to_string_lossy(), how)).collect();
        let bytes = format!(" {}", line.join(" ")).into_bytes();
        match !opened && term.has_drawn() {
            true => term.send(bytes),
            false => self.term_pending = Some((bytes, Instant::now())),
        }
        self.term_focus = true;
    }

    /// Whether a line is waiting for a pane that has just been opened, which
    /// keeps the frames coming until it has gone.
    pub fn term_waiting(&self) -> bool {
        self.term_pending.is_some()
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
        match term.search(needle, back) {
            Some(false) => return,
            Some(true) => return self.toast("Wrapped"),
            None => {}
        }
        // Nothing from here on; start again from the view.
        term.end_search();
        match term.search(needle, back) {
            Some(_) => self.toast("Wrapped"),
            None => self.error(format!("No match for {needle}")),
        }
    }

    /// Whether a configured command draws this file.
    fn is_external_preview(&self, path: &Path) -> bool {
        crate::config::PreviewRule::for_path(&self.cfg.preview, path).is_some()
    }

    /// The file under the cursor, or nothing-shaped when there is none.
    fn hovered_path(&self) -> PathBuf {
        self.tabs[self.active].current.hovered().map_or_else(PathBuf::new, |e| e.path.clone())
    }

    /// Show picture `n` of the hovered file.
    ///
    /// The old picture is left up while the new one is drawn. A command takes
    /// long enough to see, and blinking to "loading" and back on every press
    /// of `<A-j>` is worse than a moment of the previous page.
    fn go_to_picture(&mut self, n: i64) {
        if self.preview.n == n {
            return;
        }
        self.preview.n = n;
        self.preview.pending_since = None;
        self.request_preview(true);
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
            // actually run. The hook that does not is named here, and the
            // command that prints it (Q50): a toast does not wrap, and the
            // hook itself is wider than any window.
            let said = no_osc7(&self.term_shell, &filer_command());
            self.error(said);
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
            self.term_pending = None;
            self.term_focus = false;
            self.max_term = false;
            self.toast("The shell exited");
            return;
        }
        // A shell that marks its prompt (OSC 133) is ready when it says so.
        // Otherwise, quiet for 800 ms after its first output: 300 was enough
        // on ARM64 (#249), but the x64 machine's pwsh printed its banner and
        // then said nothing for over 300 ms while its profile loaded, so the
        // path went in before the prompt, 3 runs of 3 (#250).
        let ready = |at: &Instant| {
            term.prompt_seen() || term.quiet_for(Duration::from_millis(800)) || at.elapsed() > Duration::from_secs(5)
        };
        if self.term_pending.as_ref().is_some_and(|(_, at)| ready(at)) {
            if let Some((bytes, _)) = self.term_pending.take() {
                term.send(bytes);
            }
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
            // A full-screen program -- `nvim`, `less`, `htop` -- runs on the
            // alternate screen, which has no scrollback at all. Keeping a
            // scrolling key there would spend it on a scroll that cannot move
            // anything, and the program that owns the screen would never see
            // it, so hand it over instead. Only the scrolling ones: `<C-t>` has
            // to get you out of a full-screen program as much as out of a shell.
            let scrolls = !acts.is_empty() && acts.iter().all(|a| matches!(a, Act::TermScroll(_)));
            if scrolls && self.term_alt_screen() {
                if let (Some(term), Some(bytes)) = (&self.term, bytes) {
                    term.send(bytes);
                }
                return;
            }
            for a in acts {
                match a {
                    // Leaving the pane un-maximises it. A maximised pane is
                    // only of use while you are looking at it -- handing the
                    // keys back to a list that is not on screen leaves nothing
                    // to aim them at -- so "get out of the pane" and "give the
                    // window back" are in practice the same intent, and this
                    // spends one key on both rather than two.
                    Act::Close | Act::Escape(_) => {
                        self.term_focus = false;
                        self.max_term = false;
                    }
                    other => self.act(other),
                }
            }
            return;
        }
        if let (Some(term), Some(bytes)) = (&self.term, bytes) {
            term.send(bytes);
        }
    }

    /// True while a full-screen program is drawing in the terminal pane.
    pub fn term_alt_screen(&self) -> bool {
        self.term.as_ref().is_some_and(|t| t.with_grid(crate::terminal::alt_screen))
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

    /// Measure what is under each child of the current directory and show them
    /// largest first.
    ///
    /// The same trick the search view uses: a `Folder` whose path is not a real
    /// directory, filled in as the worker answers. That is the whole reason this
    /// needs no overlay, no keymap layer and no `is_modal` entry -- `j`/`k`, the
    /// wheel, selection, `y`, `d` and `<Esc>` all work because this is the file
    /// list, not a panel pretending to be one.
    /// `l` on an archive: list it on a worker, and show its top level once
    /// the list arrives. A tar has no index and is read end to end, so even
    /// listing it can take a while.
    fn open_archive(&mut self, archive: PathBuf) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let ctx = self.ctx.clone();
        let path = archive.clone();
        std::thread::Builder::new()
            .name("archive-list".into())
            .spawn(move || {
                let _ = tx.send(ArchiveMsg::Listed(path.clone(), archive::list(&path, ARCHIVE_LIMIT)));
                ctx.request_repaint();
            })
            .expect("spawn archive thread");
        self.archive_rx = Some(rx);
        let tab = &mut self.tabs[self.active];
        tab.remember_cursor();
        tab.current = Folder::loading(archive_path(&archive, ""), None);
        let scratch = preview_scratch(&archive);
        self.archive_view = Some(ArchiveView {
            archive,
            inner: String::new(),
            members: Arc::new(Vec::new()),
            more: false,
            copies: HashMap::new(),
            copying: None,
            refused: BTreeSet::new(),
            scratch,
        });
        self.preview.state = PreviewState::Empty;
        self.preview.key = None;
    }

    /// Put the level `inner` of the archive on screen, the cursor on `want`.
    fn archive_show(&mut self, inner: String, want: Option<String>) {
        let Some(view) = &mut self.archive_view else { return };
        view.inner = inner;
        let base = view.inner.split('/').filter(|p| !p.is_empty()).fold(view.archive.clone(), |p, part| p.join(part));
        let entries: Vec<Entry> = archive::level(&view.members, &view.inner)
            .into_iter()
            .map(|row| Entry {
                path: base.join(&row.name),
                ext: (!row.dir).then(|| util::stem_and_ext(&row.name).1.trim_start_matches('.').to_ascii_lowercase()).filter(|e| !e.is_empty()),
                hidden: row.name.starts_with('.'),
                kind: if row.dir { Kind::Dir } else { Kind::File },
                len: row.size,
                name: row.name,
                ..Default::default()
            })
            .collect();
        let path = archive_path(&view.archive, &view.inner);
        let tab = &mut self.tabs[self.active];
        let mut folder = Folder::from_entries(path, Arc::new(entries), tab.show_hidden);
        if let Some(name) = want {
            folder.select_name(&name);
        }
        tab.current = folder;
        tab.finder = None;
        self.preview.key = None;
        self.request_preview(true);
    }

    fn archive_down(&mut self, name: &str) {
        let Some(view) = &self.archive_view else { return };
        let inner = match view.inner.is_empty() {
            true => name.to_owned(),
            false => format!("{}/{name}", view.inner),
        };
        self.archive_show(inner, None);
    }

    /// `l` or `<Enter>` on a file inside an archive: unpack that one member
    /// into a folder of filer's own under the temporary folder, on a worker,
    /// and open the copy with the system's default app. A copy, and said to
    /// be one: changes to it do not go back into the archive.
    fn open_member(&mut self, entry: &Entry) {
        let Some(view) = &self.archive_view else { return };
        let member = match view.inner.is_empty() {
            true => entry.name.clone(),
            false => format!("{}/{}", view.inner, entry.name),
        };
        let archive = view.archive.clone();
        let dest = util::archive_scratch().join(util::file_name(&archive));
        let out = dest.join(&entry.name);
        let (tx, rx) = crossbeam_channel::bounded(1);
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("archive-member".into())
            .spawn(move || {
                let got = archive::extract_one(&archive, &member, &dest, &mut |_, _| true).map(|()| out);
                let _ = tx.send(ArchiveMsg::Copied(got));
                ctx.request_repaint();
            })
            .expect("spawn archive thread");
        self.archive_rx = Some(rx);
        self.toast(format!("Unpacking {}…", entry.name));
    }

    fn drain_archive(&mut self) {
        if let Some(Ok(ArchiveMsg::Previewed(fake, got))) = self.archive_preview_rx.as_ref().map(|rx| rx.try_recv()) {
            self.archive_preview_rx = None;
            if let Some(view) = &mut self.archive_view {
                view.copying = None;
                match got {
                    Ok(real) => {
                        view.copies.insert(fake, real);
                    }
                    Err(_) => {
                        view.refused.insert(fake);
                    }
                }
                // Whatever the cursor is on now: the one just unpacked, or
                // one it moved to while that was going.
                self.preview.key = None;
                self.request_preview(true);
            }
        }
        let Some(rx) = &self.archive_rx else { return };
        let Ok(msg) = rx.try_recv() else { return };
        self.archive_rx = None;
        match msg {
            ArchiveMsg::Listed(path, listed) => {
                // Left, or went into another, before the list came back.
                if self.archive_view.as_ref().is_none_or(|v| v.archive != path) {
                    return;
                }
                match listed {
                    Ok((members, more)) => {
                        if let Some(view) = &mut self.archive_view {
                            view.members = Arc::new(members);
                            view.more = more;
                        }
                        self.archive_show(String::new(), None);
                        if more {
                            self.toast(format!("Showing the first {ARCHIVE_LIMIT} entries of {}", util::file_name(&path)));
                        }
                    }
                    Err(e) => {
                        self.exit_search_view();
                        self.error(format!("{}: {e}", util::file_name(&path)));
                    }
                }
            }
            ArchiveMsg::Copied(Ok(path)) => {
                self.toasts.retain(|t| !t.text.starts_with("Unpacking "));
                match exec::open_default(&path) {
                    Ok(()) => self.toast(format!("Opened a copy of {} — changes stay out of the archive", util::file_name(&path))),
                    Err(e) => self.error(format!("Open failed: {e}")),
                }
            }
            ArchiveMsg::Previewed(..) => {}
            ArchiveMsg::Copied(Err(e)) => {
                self.toasts.retain(|t| !t.text.starts_with("Unpacking "));
                self.error(format!("Unpack failed: {e}"));
            }
        }
    }

    /// True while `l` has an archive's members on screen.
    pub fn in_archive_view(&self) -> bool {
        self.archive_view.is_some()
    }

    fn start_usage(&mut self) {
        if self.in_search_view() {
            self.error("Usage: leave this view first");
            return;
        }
        self.usage_root = Some(self.tabs[self.active].cwd.clone());
        self.usage_want = None;
        self.measure_here();
    }

    /// Move the usage view to `dir` and measure it, keeping where `gu` started.
    /// `want` is the name to put the cursor on when it turns up.
    fn remeasure(&mut self, dir: PathBuf, want: Option<String>) {
        let root = self.usage_root.clone();
        self.exit_search_view();
        self.cd(dir, true);
        self.usage_root = root;
        self.usage_want = want;
        // The level just left had its own total; it is not this one's.
        self.toasts.retain(|t| !t.text.contains(" in total"));
        self.measure_here();
    }

    fn measure_here(&mut self) {
        let root = self.tabs[self.active].cwd.clone();
        let ctx = self.ctx.clone();
        let handle = crate::fs::usage::spawn(&root, move || ctx.request_repaint());

        let tab = &mut self.tabs[self.active];
        tab.remember_cursor();
        let mut folder = Folder::loading(usage_path(&root), None);
        folder.state = LoadState::Ready;
        tab.current = folder;
        self.usage = Some(handle);
        self.usage_max = 0;
        self.usage_linemode = Some(tab.linemode);
        tab.linemode = crate::fs::entry::Linemode::Usage;
        self.preview.state = PreviewState::Empty;
        self.preview.key = None;
        self.toast("Measuring… <Esc> to leave");
    }

    fn drain_usage(&mut self) {
        let Some(handle) = &self.usage else { return };
        let mut batch: Vec<crate::fs::usage::Child> = Vec::new();
        let mut done: Option<(u64, bool)> = None;
        while let Ok(msg) = handle.rx.try_recv() {
            match msg {
                crate::fs::usage::Msg::Sized(mut v) => batch.append(&mut v),
                crate::fs::usage::Msg::Done { total, capped } => {
                    done = Some((total, capped));
                    break;
                }
            }
        }
        if !batch.is_empty() {
            let f = &mut self.tabs[self.active].current;
            let entries = Arc::make_mut(&mut f.entries);
            for (path, bytes, _files, whole) in batch {
                if let Ok(mut e) = Entry::from_path(path) {
                    e.usage = Some(bytes);
                    e.usage_cut = !whole;
                    entries.push(e);
                }
            }
            // Biggest first, which is the question being asked. Sorted here
            // rather than through `SortSpec` because the tab's own sort puts
            // every directory above every file, and a usage list that does that
            // cannot be read.
            entries.sort_by(|a, b| b.usage_bytes().cmp(&a.usage_bytes()).then(a.name.cmp(&b.name)));
            self.usage_max = entries.first().map_or(0, Entry::usage_bytes);
            f.rebuild(true);
            if let Some(name) = self.usage_want.take() {
                if !f.select_name(&name) {
                    self.usage_want = Some(name);
                }
            }
        }
        if let Some((total, capped)) = done {
            self.usage = None;
            self.toasts.retain(|t| !t.text.starts_with("Measuring"));
            self.toast(format!(
                "{} in total{} — <Esc> to leave",
                crate::util::human_size(total),
                if capped { " (walk cut short; totals are floors)" } else { "" }
            ));
        }
    }

    /// True while `gu`'s view is up, walk finished or not.
    pub fn in_usage_view(&self) -> bool {
        self.usage_linemode.is_some()
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
        // Filtered down to nothing: stay open and say so, rather than close
        // as if something had been chosen (#198).
        let Some(idx) = p.selected() else {
            let said = format!("Nothing matches `{}` — <Esc> closes", p.query);
            self.overlay = Overlay::Pick(p);
            self.toast(said);
            return;
        };
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
        // A key the box does not offer leaves it open (Q71): a stray `(`
        // used to close the `<F12>` box in silence, and the `<Enter>` meant
        // for it then went into a folder in the list. `n` always answers --
        // every box reads it as no -- and `<Esc>` closes before this.
        if let Overlay::Confirm(c) = &self.overlay {
            if ch != 'n' && !c.options.iter().any(|(k, _)| *k == ch) {
                return;
            }
        }
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
            ConfirmAction::EndShell => {
                if ch == 'y' {
                    self.end_shell();
                }
            }
            ConfirmAction::Junctions { links } => match ch {
                'y' => self.make_junctions(links),
                'c' => self.copy_mklink(&links),
                _ => {}
            },
            ConfirmAction::BugReport { url } => match ch {
                'o' => self.open_report(url),
                'c' => {
                    match exec::set_clipboard(&url) {
                        Ok(()) => self.toast("Copied the bug report's link -- paste it into a browser"),
                        Err(e) => self.error(format!("Clipboard: {e}")),
                    }
                    self.last_report = Some(url);
                }
                _ => {}
            },
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
        self.note_key(&k);
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

/// The commands that write to the disk, refused inside an archive.
fn changes_files(a: &Act) -> bool {
    matches!(
        a,
        // `y` is allowed: it is how a member is taken out (`p` in a folder).
        // `x` would take it out of the archive too, which this never writes.
        Act::Yank { cut: true }
            | Act::Paste { .. }
            | Act::Link { .. }
            | Act::Hardlink
            | Act::Remove { .. }
            | Act::Create { .. }
            | Act::Rename { .. }
            | Act::BulkRename
            | Act::Extract
            | Act::Compress
            | Act::SendPane { .. }
    )
}

/// How many entries of an archive are listed before stopping; the rest are
/// said to be there.
const ARCHIVE_LIMIT: usize = 100_000;

/// The archive `l` went into (Q75).
pub(crate) struct ArchiveView {
    archive: PathBuf,
    /// The folder inside it on screen, `/`-separated; empty at the top.
    inner: String,
    members: Arc<Vec<archive::Listed>>,
    /// The listing stopped at `ARCHIVE_LIMIT`.
    more: bool,
    /// Members unpacked for the preview, by the path the row shows.
    copies: HashMap<PathBuf, PathBuf>,
    /// The one being unpacked now, and ones that would not unpack.
    copying: Option<PathBuf>,
    refused: BTreeSet<PathBuf>,
    /// Where the preview's copies go: a folder of this view's own, so two
    /// archives of one name -- or two views in one process -- never share it.
    scratch: PathBuf,
}

/// Members up to this size are unpacked to be previewed; a bigger one keeps
/// its card. Reading a member means reading the archive up to it, and a tar
/// has to be read from the start.
const ARCHIVE_PREVIEW_LIMIT: u64 = 4 << 20;

impl Drop for ArchiveView {
    /// The preview's copies go with the view: nothing outside filer has
    /// them open. The copies `l` opened stay -- an editor may be holding one
    /// -- and are swept by a later start (`util::sweep_archive_scratch`).
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// A fresh folder name for one view's preview copies of `archive`.
fn preview_scratch(archive: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    util::archive_scratch().join("preview").join(format!("{n}-{}", util::file_name(archive)))
}

impl ArchiveView {
    /// What the preview says about a member, which is not a file on disk
    /// until it is unpacked.
    fn card(&self, entry: &Entry) -> crate::preview::Payload {
        let mut rows = vec![("Name".to_owned(), entry.name.clone())];
        match entry.is_dir_like() {
            true => {
                let inner = match self.inner.is_empty() {
                    true => entry.name.clone(),
                    false => format!("{}/{}", self.inner, entry.name),
                };
                rows.push(("Holds".into(), util::items(archive::level(&self.members, &inner).len())));
                rows.push(("Open".into(), "l to go in, h to come back".into()));
            }
            false => {
                rows.push(("Size".into(), util::human_size(entry.len)));
                rows.push(("Open".into(), "l or Enter opens a copy".into()));
            }
        }
        rows.push(("In".into(), self.archive.display().to_string()));
        crate::preview::Payload::Meta { rows }
    }
}

/// From a worker, for the archive view.
pub(crate) enum ArchiveMsg {
    Listed(PathBuf, std::io::Result<(Vec<archive::Listed>, bool)>),
    Copied(std::io::Result<PathBuf>),
    /// A member unpacked for the preview: the row's path and the copy's.
    Previewed(PathBuf, std::io::Result<PathBuf>),
}

/// The archive view's synthetic path, as the header shows it. Not a real
/// directory, so `in_search_view` holds and `<Esc>` leaves this view too.
fn archive_path(archive: &Path, inner: &str) -> PathBuf {
    match inner.is_empty() {
        true => PathBuf::from(format!("archive: {}", archive.display())),
        false => PathBuf::from(format!("archive: {} : {inner}", archive.display())),
    }
}

/// The usage view's synthetic path. Not a real directory, which is exactly what
/// `in_search_view` tests for, so `<Esc>` leaves this view too.
fn usage_path(root: &Path) -> PathBuf {
    PathBuf::from(format!("usage:  {}", root.display()))
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


/// The spot panel as text: each section's title on a line of its own, then
/// one `Label<TAB>value` line per row, and a blank line between sections. A tab
/// because values hold spaces and colons, and a spreadsheet splits on it.
fn spot_text(sections: &[Section]) -> String {
    sections
        .iter()
        .map(|s| {
            let rows: Vec<String> = s.rows.iter().map(|(k, v)| format!("{k}\t{v}")).collect();
            format!("{}\n{}", s.title, rows.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
            kind: if dir { Kind::Dir } else { Kind::File },
            ..Default::default()
        }
    }

    fn binding(on: &str, run: &str, desc: &str) -> keymap::Binding {
        keymap::Binding {
            on: on.chars().map(Key::char).collect(),
            run: vec![crate::config::cmd::parse(run)],
            desc: desc.into(),
            raw: run.into(),
            from: keymap::BUILT_IN.into(),
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
    /// #83: the verb once, and no byte counts for a job that has none.
    #[test]
    fn a_task_row_says_each_thing_once() {
        let mut t = task(0);
        t.kind = OpKind::Trash;
        t.label = "Trash 5 item(s)".into();
        t.files = 5;
        assert!(t.headline().starts_with("Trash 5 item(s)  ["), "{}", t.headline());
        assert_eq!(t.detail(), "0/5 files");
        let mut c = task(2048);
        c.bytes_done = 1024;
        assert!(c.detail().starts_with("0/1 files · 1.0 K / 2.0 K"), "{}", c.detail());
    }

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

    /// The debounce's timer ends with the look it was for, however that look
    /// ends -- not only by the worker being asked.
    ///
    /// `main` repaints every 16 ms while `pending_since` is set, which is what
    /// the timer needs to fire. `request_preview` took several early returns
    /// that left it set: onto a directory, onto a file already cached, back
    /// onto the file already shown. After any of those nothing ever cleared
    /// it, and an idle window -- minimised, even -- drew 60 frames a second
    /// for good: the "1 CPU-second per second" the Windows machine measured
    /// (#86). Each case here is the cursor leaving a file whose timer is
    /// running.
    #[test]
    fn the_debounce_timer_does_not_outlive_the_look() {
        let dir = crate::util::test_dir("debounce-timer");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        // An hour apart, on purpose. The cache key carries the file's mtime,
        // and this test once built b.txt's key from a.txt's: on Linux the two
        // were written in the same clock tick and it passed, on NTFS they were
        // not and it failed now and then. Apart, the mistake fails everywhere.
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(dir.join("b.txt")).unwrap().set_modified(hour_ago).unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let entries: Vec<crate::fs::Entry> = ["a.txt", "b.txt", "sub"]
            .iter()
            .map(|n| crate::fs::Entry::from_path(dir.join(n)).unwrap())
            .collect();

        let fresh = || {
            let mut a = App::new(Config::load(), std::env::temp_dir(), egui::Context::default());
            a.cfg.ui.preview_debounce_ms = 10_000;
            a.tabs[a.active].current = Folder::from_entries(dir.clone(), Arc::new(entries.clone()), true);
            a
        };
        let hover = |a: &mut App, name: &str| {
            assert!(a.tabs[a.active].current.select_name(name));
            a.request_preview(false);
        };

        // Onto a directory while the timer runs.
        let mut a = fresh();
        hover(&mut a, "a.txt");
        assert!(a.preview.pending_since.is_some(), "the timer starts on a file");
        hover(&mut a, "sub");
        assert!(a.preview.pending_since.is_none(), "a directory ends it");

        // Onto a file the cache already has.
        let mut a = fresh();
        let cached = preview::Key {
            path: dir.join("b.txt"),
            len: 1,
            mtime: entries[1].modified, // b.txt's own
            box_size: a.preview.box_size,
            cols: 0,
            n: 0,
        };
        a.preview.cache.put(cached, CachedPreview { payload: Payload::Error("x".into()), texture: None });
        hover(&mut a, "a.txt");
        assert!(a.preview.pending_since.is_some());
        hover(&mut a, "b.txt");
        assert!(matches!(a.preview.state, PreviewState::Ready(_)), "served from the cache");
        assert!(a.preview.pending_since.is_none(), "a cache hit ends it");

        // Back onto the file already on screen.
        hover(&mut a, "a.txt");
        assert!(a.preview.pending_since.is_some());
        hover(&mut a, "b.txt");
        assert!(a.preview.pending_since.is_none(), "the file already shown ends it");
    }

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
            n: 0,
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
            n: 0,
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
mod watcher_frames {
    use super::*;

    /// #108: a directory the watcher flagged asks for the frame that will read
    /// it, no sooner than the quiet period and not much later; with nothing
    /// flagged, nothing is asked for, so an idle window stays idle (47).
    #[test]
    fn a_flagged_directory_asks_for_its_frame() {
        let mut a = App::new(Config::load(), std::env::temp_dir(), egui::Context::default());
        a.dirty.clear();
        assert_eq!(a.rescan_due(), None, "nothing flagged, nothing owed");

        a.dirty.insert(std::env::temp_dir(), Instant::now());
        let due = a.rescan_due().expect("a frame is owed");
        assert!(due >= App::RESCAN_QUIET, "not before the quiet period: {due:?}");
        assert!(due < App::RESCAN_QUIET * 2, "and not long after it: {due:?}");

        std::thread::sleep(App::RESCAN_QUIET + Duration::from_millis(20));
        a.flush_dirty();
        assert_eq!(a.rescan_due(), None, "read, so nothing more is owed");
    }
}

#[cfg(test)]
mod host_sizes {
    use super::*;
    use crate::fs::entry::{Entry, Kind, Linemode};

    /// Q32 / 31.9: in `linemode size`, a folder's row asks for its children to
    /// be counted -- but a host's rows are shares, and each count would be a
    /// trip across the network. The same rows in an ordinary folder are counted.
    #[test]
    fn a_hosts_shares_are_not_counted() {
        let listing = |at: &Path| {
            let entries: Vec<Entry> = ["Backup", "cache"]
                .iter()
                .map(|n| Entry { path: at.join(n), name: n.to_string(), kind: Kind::Dir, ..Default::default() })
                .collect();
            crate::core::folder::Folder::from_entries(at.to_path_buf(), Arc::new(entries), true)
        };
        let ctx = egui::Context::default();
        let mut a = App::new(Config::load(), std::env::temp_dir(), ctx);
        a.tabs[a.active].linemode = Linemode::Size;
        a.tabs[a.active].page_rows = 10;

        // A `\\host` is a host only on Windows; elsewhere `//x` is a path.
        #[cfg(windows)]
        {
            let host = PathBuf::from(r"\\fileserver");
            a.tabs[a.active].cwd = host.clone();
            a.tabs[a.active].current = listing(&host);
            a.ensure_dir_sizes();
            assert!(a.counted.is_empty(), "nothing asked of the network: {:?}", a.counted);
        }

        let dir = crate::util::test_dir("host-sizes");
        a.tabs[a.active].cwd = dir.clone();
        a.tabs[a.active].current = listing(&dir);
        a.ensure_dir_sizes();
        assert_eq!(a.counted.len(), 2, "an ordinary folder's folders are still counted");
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
mod create_and_link_undo {
    use super::*;

    fn app(dir: &Path) -> App {
        App::new(Config::load(), dir.to_path_buf(), egui::Context::default())
    }

    /// `a` with folders on the way: `u` removes the file and the folders it
    /// had to make, and `U` makes them all again.
    #[test]
    fn a_create_goes_and_comes_back() {
        let dir = util::test_dir("create-undo");
        let mut a = app(&dir);
        a.do_create("new/deep/note.txt");
        let file = dir.join("new/deep/note.txt");
        assert!(file.is_file());

        a.undo_step();
        assert!(!dir.join("new").exists(), "the file and both folders made for it are gone");

        a.redo_step();
        assert!(file.is_file(), "U makes the file again, folders and all");
    }

    fn toasts(a: &App) -> Vec<&str> {
        a.toasts.iter().map(|t| t.text.as_str()).collect()
    }

    /// #168: the toast counts the folders `u` took with the file, and only
    /// those -- one someone has since put something in stays, and is not
    /// counted.
    #[test]
    fn undoing_a_create_counts_the_folders_that_went() {
        let dir = util::test_dir("create-undo-said");
        let mut a = app(&dir);
        a.do_create("new/deep/note.txt");
        a.undo_step();
        assert!(toasts(&a).contains(&"Removed note.txt and 2 folder(s)"), "{:?}", toasts(&a));

        a.redo_step();
        std::fs::write(dir.join("new/other.txt"), b"").unwrap();
        a.undo_step();
        assert!(dir.join("new").is_dir() && !dir.join("new/deep").exists());
        assert!(toasts(&a).contains(&"Removed note.txt and 1 folder(s)"), "{:?}", toasts(&a));

        a.do_create("plain.txt");
        a.undo_step();
        assert!(toasts(&a).contains(&"Removed plain.txt"), "{:?}", toasts(&a));
    }

    /// #168, proposal 4: `--keys` waits on `settled`, and a job only says how
    /// it went once it is over, so a job queued or running is not settled.
    /// Without this `u<Shot:x>` pictured the job at 0% and missed its toast.
    #[test]
    fn a_job_still_going_is_not_settled() {
        let dir = util::test_dir("settled-job");
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let mut s = crate::ui::harness::Screen::open(&dir);
        let settle = |s: &mut crate::ui::harness::Screen| {
            for _ in 0..1000 {
                s.turn();
                if s.app.settled() {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        assert!(settle(&mut s), "the listing and preview come up");

        s.app.submit_op(OpKind::Copy, vec![dir.join("a.txt")], dir.join("sub"), false);
        assert!(!s.app.settled(), "queued");
        assert!(settle(&mut s));
        assert!(dir.join("sub/a.txt").is_file(), "settled only once the copy was there");
        assert!(s.app.tasks.iter().all(|t| t.state == TaskState::Done));

        // Copied again, the name is taken and the job asks: that question is
        // waiting on a key, so it is settled.
        s.app.submit_op(OpKind::Copy, vec![dir.join("a.txt")], dir.join("sub"), false);
        for _ in 0..1000 {
            s.turn();
            if matches!(s.app.overlay, Overlay::Confirm(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(matches!(s.app.overlay, Overlay::Confirm(_)), "the conflict is asked");
        assert!(s.app.settled(), "the job waits on the answer");
    }

    fn finished(linked: Vec<ops::Link>, errors: Vec<String>) -> ops::OpEvent {
        ops::OpEvent::Finished {
            id: 3,
            kind: OpKind::Hardlink,
            errors,
            cancelled: false,
            moved: Vec::new(),
            linked,
            junctions: Vec::new(),
            made: Vec::new(),
            trashed: Vec::new(),
        }
    }

    /// #168: `=`, `-` and `_` say what they made, as `d` does, since a new
    /// link barely changes the listing.
    #[test]
    fn a_finished_link_says_what_it_made() {
        let dir = util::test_dir("link-said");
        let mut a = app(&dir);
        let link = |name: &str| ops::Link { at: dir.join(name), target: dir.join("src.txt"), dir: false, hard: true, junction: false };
        a.on_op_event(finished(vec![link("one.txt")], Vec::new()));
        assert!(toasts(&a).contains(&"Linked one.txt — u to undo"), "{:?}", toasts(&a));

        a.on_op_event(finished(vec![link("two.txt"), link("three.txt")], Vec::new()));
        assert!(toasts(&a).contains(&"Made 2 link(s) — u to undo"), "{:?}", toasts(&a));
    }

    /// #185: `u` and `U` on junctions say junction, as `y` did; on links,
    /// link, as before.
    #[test]
    fn undoing_a_junction_says_junction() {
        let made = |junction| ops::Link { at: PathBuf::from("w/alias"), target: PathBuf::from("w/real"), dir: true, hard: false, junction };
        let step = UndoStep::Link { links: vec![made(true)] };
        assert_eq!(step.undone_label(), "Removed the junction alias");
        assert_eq!(step.redone_label(), "Made the junction alias again");
        let step = UndoStep::Link { links: vec![made(true), made(true)] };
        assert_eq!((step.undone_label().as_str(), step.redone_label().as_str()), ("Removed 2 junction(s)", "Made 2 junctions again"));
        let step = UndoStep::Link { links: vec![made(false)] };
        assert_eq!((step.undone_label().as_str(), step.redone_label().as_str()), ("Removed the link alias", "Linked alias"));
    }

    /// Some made and some not: the error is what to read, so no success line
    /// goes up next to it. What was made is still a step `u` can take back.
    #[test]
    fn a_link_that_partly_failed_says_only_the_error() {
        let dir = util::test_dir("link-said-partial");
        let mut a = app(&dir);
        let link = ops::Link { at: dir.join("one.txt"), target: dir.join("src.txt"), dir: false, hard: true, junction: false };
        a.on_op_event(finished(vec![link], vec!["two.txt: denied".into()]));
        assert!(!toasts(&a).iter().any(|t| t.contains("u to undo")), "{:?}", toasts(&a));
        assert_eq!(a.undos.undo.len(), 1);
    }

    /// Q46: folder symlinks Windows refused come back as junctions to offer.
    /// The question names both paths and what a junction is not; `n` leaves
    /// everything as it was.
    #[test]
    fn a_refused_folder_link_is_offered_as_a_junction() {
        let dir = util::test_dir("junction-offer");
        let mut a = app(&dir);
        let offered = ops::Link { at: dir.join("alias"), target: dir.join("real"), dir: true, hard: false, junction: true };
        let mut ev = finished(Vec::new(), vec!["real: refused".into()]);
        if let ops::OpEvent::Finished { junctions, .. } = &mut ev {
            junctions.push(offered.clone());
        }
        a.on_op_event(ev);
        let Overlay::Confirm(c) = &a.overlay else { panic!("no question: {:?}", toasts(&a)) };
        assert!(matches!(&c.action, ConfirmAction::Junctions { links } if links == &vec![offered.clone()]));
        let body = c.body.join("\n");
        assert!(body.contains("not a relative one") && body.contains("network"), "{body}");
        assert!(body.contains(&format!("{}  →  {}", offered.at.display(), offered.target.display())), "{body}");
        assert_eq!(c.options[0], ('y', "Make the junction".into()));
        assert_eq!(c.options[1], ('c', "Copy the mklink command".into()));

        a.answer_confirm('n');
        assert!(a.overlay.is_none() && a.undos.undo.is_empty() && !offered.at.exists());
    }

    /// Q64, found by the QA agent's tests for section 26: an error naming a
    /// path under the home folder no longer carries the user's name into the
    /// report's link or its panel.
    #[test]
    fn the_report_names_no_home_folder_from_the_last_error() {
        let Some(home) = dirs::home_dir().filter(|h| h.as_os_str().len() >= 4) else { return };
        let dir = util::test_dir("report-home");
        let mut a = app(&dir);
        a.error(format!("{}: denied", home.join("secret").display()));
        a.bug_report();
        let Overlay::Confirm(c) = &a.overlay else { panic!("no panel") };
        let ConfirmAction::BugReport { url } = &c.action else { panic!("not the report") };
        let url = url.clone();
        let shown = home.display().to_string();
        assert!(!c.body.join("\n").contains(&shown), "{:?}", c.body);
        assert!(c.body.iter().any(|l| l.starts_with("Last error: ~")), "{:?}", c.body);
        let encoded = crate::bugreport::url(&[("x", shown.clone())]);
        let needle = encoded.rsplit("x=").next().unwrap_or_default();
        assert!(!url.contains(needle), "the link carries the home folder: {url}");
        // And `c` copies it to this thread's fake clipboard, never the machine's.
        a.answer_confirm('c');
        assert_eq!(crate::exec::get_clipboard(), Ok(url));
    }

    /// Q71: a key the box does not offer leaves it open, so a stray key no
    /// longer closes it in silence and sends the next `<Enter>` to the list.
    /// `n` still answers a box that has no `n` button (the overwrite one).
    #[test]
    fn a_key_the_box_does_not_offer_leaves_it_open() {
        let dir = util::test_dir("confirm-stray");
        let mut a = app(&dir);
        a.bug_report();
        for stray in ['(', 'x', 'j', 'Y'] {
            a.answer_confirm(stray);
            assert!(matches!(a.overlay, Overlay::Confirm(_)), "{stray:?} closed the box");
        }
        assert!(a.last_report.is_none(), "and did nothing else");
        a.answer_confirm('n');
        assert!(a.overlay.is_none(), "`n` closes it");
    }

    /// Q56: `c` copies the `mklink /J` line -- the one the refusal names --
    /// and makes nothing. Where there is no clipboard (a headless test run)
    /// it says so instead.
    #[test]
    fn c_copies_the_mklink_line_and_makes_nothing() {
        let dir = util::test_dir("junction-copy");
        let mut a = app(&dir);
        let offered = ops::Link { at: dir.join("alias"), target: dir.join("real"), dir: true, hard: false, junction: true };
        let mut ev = finished(Vec::new(), vec!["real: refused".into()]);
        if let ops::OpEvent::Finished { junctions, .. } = &mut ev {
            junctions.push(offered.clone());
        }
        a.on_op_event(ev);
        a.answer_confirm('c');
        assert!(a.overlay.is_none() && a.undos.undo.is_empty() && !offered.at.exists());
        let said = toasts(&a);
        assert!(
            said.contains(&"Copied the mklink command — it runs in cmd or PowerShell") || said.iter().any(|t| t.starts_with("Clipboard:")),
            "{said:?}"
        );
        assert_eq!(
            ops::mklink_line(&offered.at, &offered.target),
            format!("cmd /d /c mklink /J \"{}\" \"{}\"", offered.at.display(), offered.target.display())
        );
    }

    /// The `y`: a real junction on Windows, which `u` removes without touching
    /// the folder and `U` makes again. Only Windows has junctions.
    #[cfg(windows)]
    #[test]
    fn a_junction_is_made_undone_and_redone() {
        let dir = util::test_dir("junction-made");
        std::fs::create_dir(dir.join("real")).unwrap();
        std::fs::write(dir.join("real/inside.txt"), b"x").unwrap();
        let mut a = app(&dir);
        let link = ops::Link { at: dir.join("alias"), target: dir.join("real"), dir: true, hard: false, junction: true };
        a.make_junctions(vec![link.clone()]);
        assert!(toasts(&a).contains(&"Made a junction alias — u to undo"), "{:?}", toasts(&a));
        assert!(dir.join("alias/inside.txt").is_file(), "through the junction");

        a.undo_step();
        assert!(!ops::exists(&link.at), "the junction is gone");
        assert!(dir.join("real/inside.txt").is_file(), "the folder and its file are not");

        a.redo_step();
        assert!(dir.join("alias/inside.txt").is_file(), "U makes the junction again");
    }

    /// A file written in since is not a slip any more: `u` keeps it, says
    /// why, and leaves the step to try again.
    #[test]
    fn a_created_file_with_something_in_it_stays() {
        let dir = util::test_dir("create-undo-written");
        let mut a = app(&dir);
        a.do_create("kept.txt");
        std::fs::write(dir.join("kept.txt"), b"work").unwrap();

        a.undo_step();
        assert!(dir.join("kept.txt").is_file(), "not removed");
        assert_eq!(a.undos.undo.len(), 1, "still there for u once it is empty again");
    }

    /// `a` on a folder that is already there makes nothing, so `u` has
    /// nothing of its to remove.
    #[test]
    fn a_folder_that_was_there_is_not_recorded() {
        let dir = util::test_dir("create-undo-existing");
        std::fs::create_dir(dir.join("was")).unwrap();
        let mut a = app(&dir);
        a.do_create("was/");
        assert!(a.undos.undo.is_empty());
    }

    /// Undoing a hardlink removes the second name and leaves the file.
    #[test]
    fn a_hardlink_is_removed_and_the_file_stays() {
        let dir = util::test_dir("link-undo-hard");
        let src = dir.join("data.txt");
        std::fs::write(&src, b"data").unwrap();
        let link = ops::Link { at: dir.join("again.txt"), target: src.clone(), dir: false, hard: true, junction: false };
        link.make().unwrap();

        let mut a = app(&dir);
        a.undos.land(UndoStep::Link { links: vec![link.clone()] }, Land::Fresh);
        a.undo_step();
        assert!(!link.at.exists(), "the link is gone");
        assert_eq!(std::fs::read(&src).unwrap(), b"data", "the file it named is not");

        a.redo_step();
        assert_eq!(std::fs::read(&link.at).unwrap(), b"data", "U links it again");
    }

    /// Something that took the link's name since is not the link: `u` leaves
    /// it alone.
    #[test]
    fn a_file_that_took_the_name_is_not_removed() {
        let dir = util::test_dir("link-undo-replaced");
        let src = dir.join("data.txt");
        std::fs::write(&src, b"data").unwrap();
        let link = ops::Link { at: dir.join("again.txt"), target: src.clone(), dir: false, hard: true, junction: false };
        std::fs::write(&link.at, b"another file of a different size").unwrap();

        let mut a = app(&dir);
        a.undos.land(UndoStep::Link { links: vec![link.clone()] }, Land::Fresh);
        a.undo_step();
        assert!(link.at.exists(), "not ours to remove");
        assert_eq!(a.undos.undo.len(), 1, "the step stays");
    }

    /// A symlink to a folder: `u` removes the link, not the folder or what
    /// is in it.
    #[cfg(unix)]
    #[test]
    fn a_folder_symlink_is_removed_and_the_folder_stays() {
        let dir = util::test_dir("link-undo-sym");
        std::fs::create_dir(dir.join("real")).unwrap();
        std::fs::write(dir.join("real/inside.txt"), b"x").unwrap();
        let link = ops::Link { at: dir.join("alias"), target: PathBuf::from("real"), dir: true, hard: false, junction: false };
        link.make().unwrap();

        let mut a = app(&dir);
        a.undos.land(UndoStep::Link { links: vec![link.clone()] }, Land::Fresh);
        a.undo_step();
        assert!(!ops::exists(&link.at), "the link is gone");
        assert!(dir.join("real/inside.txt").is_file(), "the folder and its file are not");

        a.redo_step();
        assert!(dir.join("alias/inside.txt").is_file(), "U makes the same relative link again");
    }
}

#[cfg(test)]
mod move_undo {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// A file path inside a directory shared by this test.
    ///
    /// Deliberately **not** `util::test_dir`: this is called several times in one
    /// test for several file names, and `test_dir` wipes what it hands back, so
    /// the second call would delete the first file. The three tests here use
    /// distinct names, so the process id is uniqueness enough. Named the way
    /// `test_dir` names its own, so its sweep clears what a run leaves: as
    /// `filer-move-undo-<pid>` one was left per run, 448 in the cloud
    /// container by v0.77.0.
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("filer-test-move-undo-{}", std::process::id()));
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

/// The one thing a copy-paste could get wrong in `PanelLayer` and no other test
/// would notice: a layer wired to somebody else's keymap section. Pointer
/// equality, so it is the section itself and not a section that happens to look
/// like it.
#[cfg(test)]
mod panel_layers {
    use super::*;

    #[test]
    fn each_panel_reads_its_own_section() {
        let km = &Config::load().keymap;
        for (layer, want, name) in [
            (PanelLayer::Help, &km.help, "help"),
            (PanelLayer::Tasks, &km.tasks, "tasks"),
            (PanelLayer::Spot, &km.spot, "spot"),
            (PanelLayer::Diff, &km.diff, "diff"),
        ] {
            let got = layer.bindings(km);
            assert!(
                std::ptr::eq(got, want.as_slice()),
                "{layer:?} should read `[{name}]`"
            );
            assert!(!got.is_empty(), "`[{name}]` has default bindings, so this is wired up");
        }
    }

    /// The overlays that are native widgets, or no overlay at all, have no layer
    /// -- so a key pressed there must not be resolved against somebody else's.
    #[test]
    fn the_widget_overlays_have_no_layer() {
        assert_eq!(PanelLayer::of(&Overlay::None), None);
        assert_eq!(PanelLayer::of(&Overlay::Help), Some(PanelLayer::Help));
    }
}

/// A tree comparison reuses `Overlay::Diff` and the `[diff]` keymap, so the same
/// keys have to mean the right thing for a list with a cursor rather than for two
/// columns of lines.
#[cfg(test)]
mod diff_tree_keys {
    use super::*;
    use crate::diff::{Outcome, TreeRow, TreeState};

    fn row(rel: &str, state: TreeState) -> TreeRow {
        TreeRow { rel: PathBuf::from(rel), state, dir: false, left: 0, right: 0 }
    }

    fn app_with(rows: Vec<TreeRow>) -> App {
        let mut a = App::new(Config::load(), std::env::temp_dir(), egui::Context::default());
        a.overlay = Overlay::Diff(DiffOverlay {
            left: PathBuf::from("l"),
            right: PathBuf::from("r"),
            outcome: Some(Outcome::Tree {
                counts: crate::diff::TreeCounts::of(&rows),
                rows,
                truncated: false,
            }),
            offset: 0,
            rows: 10,
            cursor: 0,
            hide_same: false, back: None,
        });
        a
    }

    fn cursor(a: &App) -> usize {
        match &a.overlay {
            Overlay::Diff(ov) => ov.cursor,
            _ => panic!("the overlay closed"),
        }
    }

    /// A tree opens on its first difference, and on its top row when there is
    /// none (Q23).
    #[test]
    fn a_tree_opens_on_its_first_difference() {
        let rows = vec![row("a", TreeState::Same), row("b", TreeState::Same), row("c", TreeState::Differ)];
        let mut ov = DiffOverlay {
            left: PathBuf::from("l"),
            right: PathBuf::from("r"),
            outcome: None,
            offset: 0,
            rows: 10,
            cursor: 0,
            hide_same: false,
            back: None,
        };
        let counts = crate::diff::TreeCounts::of(&rows);
        ov.arrive(Outcome::Tree { rows, counts, truncated: false });
        assert_eq!(ov.cursor, 2, "on `c`, the one difference");

        let same = vec![row("a", TreeState::Same), row("b", TreeState::Same)];
        let counts = crate::diff::TreeCounts::of(&same);
        ov.arrive(Outcome::Tree { rows: same, counts, truncated: false });
        assert_eq!(ov.cursor, 0, "nothing differs, so the top");
    }

    /// `z` hides the matches and brings them back, keeping the cursor on the
    /// row it was on -- or, when that row is a match going out of sight, on the
    /// next one that stays. `j` and `n` then walk the rows that are shown (Q23).
    #[test]
    fn z_hides_the_matching_rows_and_the_cursor_stays_put() {
        let mut a = app_with(vec![
            row("a", TreeState::Same),
            row("b", TreeState::Differ),
            row("c", TreeState::Same),
            row("d", TreeState::LeftOnly),
            row("e", TreeState::Same),
        ]);
        let shown = |a: &App| match &a.overlay {
            Overlay::Diff(ov) => ov.shown(),
            _ => panic!("the overlay closed"),
        };
        // On `c`, a match.
        a.diff_act(Act::Arrow(crate::config::cmd::Step::Rel(2)));
        a.diff_act(Act::HideSame);
        assert_eq!(shown(&a), vec![1, 3], "only `b` and `d`");
        assert_eq!(shown(&a)[cursor(&a)], 3, "`c` went, so the cursor is on `d`, the next that stayed");

        a.diff_act(Act::Arrow(crate::config::cmd::Step::Rel(-1)));
        assert_eq!(shown(&a)[cursor(&a)], 1, "`k` steps over the hidden `c` onto `b`");

        a.diff_act(Act::HideSame);
        assert_eq!(shown(&a).len(), 5, "all back");
        assert_eq!(shown(&a)[cursor(&a)], 1, "still on `b`");
    }

    /// `j`/`k` move the selection, not the scroll -- the rows are things you pick,
    /// unlike the lines of a file comparison.
    #[test]
    fn the_arrows_move_the_cursor() {
        let mut a = app_with(vec![
            row("a", TreeState::Same),
            row("b", TreeState::Differ),
            row("c", TreeState::Same),
        ]);
        a.diff_act(Act::Arrow(crate::config::cmd::Step::Rel(1)));
        assert_eq!(cursor(&a), 1);
        a.diff_act(Act::Arrow(crate::config::cmd::Step::Bot));
        assert_eq!(cursor(&a), 2);
        a.diff_act(Act::Arrow(crate::config::cmd::Step::Rel(1)));
        assert_eq!(cursor(&a), 2, "the last row is the last row");
        a.diff_act(Act::Arrow(crate::config::cmd::Step::Top));
        assert_eq!(cursor(&a), 0);
    }

    /// `n`/`N` walk to the next path that is not a match, skipping the rows there
    /// is nothing to look at.
    #[test]
    fn n_walks_between_the_paths_that_are_not_matches() {
        let mut a = app_with(vec![
            row("a", TreeState::Same),
            row("b", TreeState::Same),
            row("c", TreeState::LeftOnly),
            row("d", TreeState::Same),
            row("e", TreeState::Differ),
        ]);
        a.diff_act(Act::FindArrow { prev: false });
        assert_eq!(cursor(&a), 2, "past the two matches");
        a.diff_act(Act::FindArrow { prev: false });
        assert_eq!(cursor(&a), 4);
        // Nothing further: the cursor stays and the view says so.
        a.diff_act(Act::FindArrow { prev: false });
        assert_eq!(cursor(&a), 4);
        a.diff_act(Act::FindArrow { prev: true });
        assert_eq!(cursor(&a), 2, "and back");
    }

    #[test]
    fn close_still_closes() {
        let mut a = app_with(vec![row("a", TreeState::Same)]);
        a.diff_act(Act::Close);
        assert!(matches!(a.overlay, Overlay::None));
    }

    /// An empty comparison must not index anything.
    #[test]
    fn no_rows_is_not_a_panic() {
        let mut a = app_with(Vec::new());
        a.diff_act(Act::Arrow(crate::config::cmd::Step::Rel(1)));
        a.diff_act(Act::FindArrow { prev: false });
        assert_eq!(cursor(&a), 0);
    }

    /// `<Enter>` on a row that differs compares that pair of files, and `q`
    /// goes back to the folders, on the same row.
    #[test]
    fn enter_opens_the_pair_and_q_comes_back() {
        let km = &Config::load().keymap;
        let enter = km.diff.iter().find(|b| crate::config::keys::render_seq(&b.on) == "<Enter>");
        assert_eq!(enter.map(|b| b.run.clone()), Some(vec![Act::Enter]), "<Enter> is bound in [diff]");

        let mut a = app_with(vec![row("a", TreeState::Same), row("sub/b.txt", TreeState::Differ)]);
        a.diff_act(Act::Arrow(Step::Rel(1)));
        a.diff_act(Act::Enter);
        match &a.overlay {
            Overlay::Diff(ov) => {
                assert_eq!((ov.left.as_path(), ov.right.as_path()), (Path::new("l/sub/b.txt"), Path::new("r/sub/b.txt")));
                assert!(ov.back.is_some(), "with the folders to go back to");
            }
            _ => panic!("the overlay closed"),
        }
        a.diff_act(Act::Close);
        assert_eq!(cursor(&a), 1, "back on the row it was opened from");
        assert!(matches!(&a.overlay, Overlay::Diff(ov) if matches!(ov.outcome, Some(Outcome::Tree { .. }))));
    }

    /// A row with nothing to pair says why and stays where it is.
    #[test]
    fn enter_on_a_one_sided_row_stays() {
        let mut a = app_with(vec![row("only-left.txt", TreeState::LeftOnly)]);
        a.diff_act(Act::Enter);
        assert!(matches!(&a.overlay, Overlay::Diff(ov) if ov.back.is_none() && ov.outcome.is_some()));
        assert!(a.toasts.iter().any(|t| t.text.contains("one side only")));
    }
}

/// What `<A-Up>` says when the shell has never reported its directory. The
/// shell is named, because the hook goes in *that* shell's profile: Windows
/// PowerShell 5.1 and PowerShell 7 read different ones, and someone running
/// 5.1 was sent to the same README line again and again (#101). Where there
/// is a hook for it, the command that adds it is the rest of the message
/// (Q50); 5.1 is told it cannot have one rather than handed one that fails.
fn no_osc7(shell: &str, filer: &str) -> String {
    let who = if shell.is_empty() { "The shell".to_owned() } else { format!("`{shell}`") };
    let name = shell.split(' ').next().unwrap_or("").to_ascii_lowercase();
    let what = match name.trim_end_matches(".exe") {
        "powershell" => {
            return format!(
                "{who} has not said where it is (no OSC 7), and cannot: the hook needs PowerShell 7 \
                 (winget install Microsoft.PowerShell)"
            )
        }
        "bash" => format!("{filer} shell-hook bash >> ~/.bashrc"),
        "zsh" => format!("{filer} shell-hook zsh >> ~/.zshrc"),
        "" | "pwsh" => format!("{filer} shell-hook | Add-Content $PROFILE"),
        _ => return format!("{who} has not said where it is (no OSC 7). `filer shell-hook` has hooks for {}", crate::shellhook::SHELLS),
    };
    format!("{who} has not said where it is (no OSC 7). In that shell: {what}, then <C-S-t> and <C-t>")
}

/// How the pane's shell can run this filer: by name when that is what the
/// `PATH` finds, by its full path when it is not (a zip unpacked anywhere).
fn filer_command() -> String {
    let Ok(me) = std::env::current_exe() else { return "filer".into() };
    let name = if cfg!(windows) { "filer.exe" } else { "filer" };
    let found = std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| same_file(&d.join(name), &me)));
    if found {
        "filer".into()
    } else if cfg!(windows) {
        format!("& '{}'", me.display())
    } else {
        format!("'{}'", me.display())
    }
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The web page behind one row of spot's Git section. `From branch` sat
/// between two rows that opened and was the one that did nothing (#119); its
/// page is built from the pull request's, which already carries the
/// repository, so `<Enter>` asks git nothing on the UI thread.
fn page_for(rows: &[(String, String)], key: &str) -> Option<String> {
    let pr = rows.iter().find(|(k, _)| k == crate::spot::PR_ROW).map(|(_, v)| v.as_str())?;
    match key {
        "Came in via" => Some(pr.to_owned()),
        k if k == crate::spot::PR_ROW => Some(pr.to_owned()),
        "From branch" => {
            let branch = rows.iter().find(|(k, _)| k == "From branch").map(|(_, v)| v.as_str())?;
            let repo = &pr[..pr.rfind("/pull/")?];
            Some(format!("{repo}/tree/{branch}"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod no_osc7_message {
    /// #101: the toast names the shell whose profile the hook belongs in.
    #[test]
    fn it_names_the_shell() {
        let said = super::no_osc7("powershell (Windows PowerShell 5.1)", "filer");
        assert!(said.starts_with("`powershell (Windows PowerShell 5.1)` has not said where it is"), "{said}");
        assert!(said.contains("needs PowerShell 7"), "{said}");
        assert!(super::no_osc7("", "filer").starts_with("The shell has not said"));
    }

    /// Q50: the command that adds the hook, for the shell the pane runs.
    #[test]
    fn it_gives_the_command_for_that_shell() {
        let said = super::no_osc7("pwsh", "& 'C:\\x\\filer.exe'");
        assert!(said.contains("In that shell: & 'C:\\x\\filer.exe' shell-hook | Add-Content $PROFILE, then"), "{said}");
        assert!(super::no_osc7("bash", "filer").contains("filer shell-hook bash >> ~/.bashrc"));
        assert!(super::no_osc7("zsh", "filer").contains("filer shell-hook zsh >> ~/.zshrc"));
        assert!(super::no_osc7("cmd.exe", "filer").contains("hooks for pwsh, bash, zsh"));
        assert!(super::no_osc7("", "filer").contains("filer shell-hook | Add-Content $PROFILE"));
    }
}

#[cfg(test)]
mod spot_pages {
    use super::page_for;

    /// The three rows that name a place on GitHub all open it; the rest do not.
    #[test]
    fn each_git_row_opens_its_own_page() {
        let rows: Vec<(String, String)> = [
            ("Came in via", "#71  48b6c9c"),
            ("From branch", "claude/task-09i0cs"),
            (crate::spot::PR_ROW, "https://github.com/uchmk/filer/pull/71"),
            ("Subject", "x"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(page_for(&rows, "Came in via").as_deref(), Some("https://github.com/uchmk/filer/pull/71"));
        assert_eq!(
            page_for(&rows, "From branch").as_deref(),
            Some("https://github.com/uchmk/filer/tree/claude/task-09i0cs")
        );
        assert_eq!(page_for(&rows, "Subject"), None);
        // No pull request, no repository to build a branch page from.
        assert_eq!(page_for(&rows[1..2], "From branch"), None);
    }
}

/// Going into an archive with `l` (Q75): its members on a `Folder` whose path
/// is not a real directory, the way the usage view is.
#[cfg(test)]
mod archive_view {
    use super::*;

    /// An archive of `docs/a/readme.md` and `top.txt`, and an app in its
    /// folder with the cursor on it.
    fn app_on_archive() -> (App, PathBuf) {
        let dir = crate::util::test_dir("archive-view");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("docs").join("a")).unwrap();
        std::fs::write(src.join("docs").join("a").join("readme.md"), "read me").unwrap();
        std::fs::write(src.join("top.txt"), "top").unwrap();
        let zip = dir.join("pack.zip");
        archive::compress(&[src.join("docs"), src.join("top.txt")], &src, &zip, archive::Format::Zip, &mut |_, _| true).unwrap();
        let mut a = App::new(Config::load(), dir.clone(), egui::Context::default());
        a.tabs[a.active].cwd = dir.clone();
        let entries = vec![Entry::from_path(zip.clone()).unwrap()];
        a.tabs[a.active].current = Folder::from_entries(dir, Arc::new(entries), true);
        (a, zip)
    }

    fn names(a: &App) -> Vec<String> {
        a.tabs[a.active].current.view.iter().filter_map(|&i| a.tabs[a.active].current.entries.get(i as usize)).map(|e| e.name.clone()).collect()
    }

    /// The listing arrives from a worker.
    fn wait(a: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while a.archive_rx.is_some() && Instant::now() < deadline {
            a.drain_archive();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn l_goes_in_and_h_comes_back_out() {
        let (mut a, zip) = app_on_archive();
        a.act(Act::Enter);
        assert!(a.in_archive_view() && a.in_search_view(), "a view, not a directory");
        wait(&mut a);
        assert_eq!(names(&a), ["docs", "top.txt"]);
        a.act(Act::Enter);
        a.act(Act::Enter);
        assert_eq!(names(&a), ["readme.md"], "two levels down");
        a.act(Act::Leave);
        assert_eq!(names(&a), ["a"]);
        assert_eq!(a.tabs[a.active].current.hovered().map(|e| e.name.as_str()), Some("a"), "the cursor on the folder come up out of");
        a.act(Act::Leave);
        a.act(Act::Leave);
        assert!(!a.in_archive_view(), "`h` at the top leaves");
        // Back on the real folder, listed afresh, with the cursor remembered
        // on the archive for when the listing lands.
        let dir = zip.parent().unwrap().to_path_buf();
        assert_eq!(a.tabs[a.active].current.path, dir);
        assert_eq!(a.tabs[a.active].memo.get(&dir).map(String::as_str), Some("pack.zip"));
    }

    /// Nothing that writes runs on rows that are not files yet.
    #[test]
    fn inside_an_archive_is_read_only() {
        let (mut a, _) = app_on_archive();
        a.act(Act::Enter);
        wait(&mut a);
        a.act(Act::Yank { cut: true });
        assert!(a.yank.paths.is_empty(), "nothing cut: it would take it out of the archive");
        assert!(a.toasts.iter().any(|t| t.text.starts_with("Inside an archive: read only")), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// `y` on a member, then `p` in a folder: the member comes out under its
    /// own name, a folder with what is under it, and the register stays.
    #[test]
    fn y_then_p_takes_a_member_out() {
        let (mut a, zip) = app_on_archive();
        a.act(Act::Enter);
        wait(&mut a);
        // `docs` is the first row: take the folder out.
        a.act(Act::Yank { cut: false });
        assert!(a.yank.from_archive && a.yank.paths.len() == 1, "{:?}", a.yank.paths);
        let out = zip.parent().unwrap().join("out");
        std::fs::create_dir_all(&out).unwrap();
        a.exit_search_view();
        a.cd(out.clone(), true);
        a.act(Act::Paste { force: false, follow: false });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !out.join("docs").join("a").join("readme.md").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(out.join("docs").join("a").join("readme.md")).unwrap(), "read me");
        assert!(!out.join("top.txt").exists(), "only what was yanked");
        assert!(a.yank.from_archive, "the register stays, as a copy's does");
        // Nothing of the job's own left behind.
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::fs::read_dir(&out).unwrap().count() > 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let names: Vec<String> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["docs"]);
    }

    /// A small member is unpacked for the preview and previewed as the copy;
    /// leaving the view takes the copies with it.
    #[test]
    fn a_member_is_previewed_from_a_copy() {
        let (mut a, zip) = app_on_archive();
        a.act(Act::Enter);
        wait(&mut a);
        a.act(Act::Arrow(Step::Rel(1)));
        assert_eq!(a.tabs[a.active].current.hovered().map(|e| e.name.as_str()), Some("top.txt"));
        a.request_preview(true);
        let deadline = Instant::now() + Duration::from_secs(10);
        while a.archive_preview_rx.is_some() && Instant::now() < deadline {
            a.drain_archive();
            std::thread::sleep(Duration::from_millis(5));
        }
        let real = a.archive_view.as_ref().and_then(|v| v.copies.get(&zip.join("top.txt")).cloned()).expect("a copy");
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "top");
        assert!(matches!(&a.preview.key, Some(k) if k.path == real) || a.preview.pending_since.is_some(), "the copy is what is previewed");
        a.act(Act::Escape(EscapeWhat::default()));
        assert!(!real.exists(), "the copy went with the view");
    }

    /// #198: `<Enter>` with nothing left after the filter keeps the picker
    /// open and says why, instead of closing as if something were chosen.
    #[test]
    fn a_picker_filtered_to_nothing_stays_open() {
        let (mut a, zip) = app_on_archive();
        let mut pick = PickOverlay {
            title: "Jump to".into(),
            items: vec!["one".into()],
            details: vec![String::new()],
            query: "zzz".into(),
            matches: Vec::new(),
            cursor: 0,
            action: PickAction::Jump { paths: vec![zip] },
            focused: false,
        };
        pick.refilter();
        a.overlay = Overlay::Pick(pick);
        a.submit_pick();
        assert!(matches!(a.overlay, Overlay::Pick(_)), "still open");
        assert!(a.toasts.iter().any(|t| t.text == "Nothing matches `zzz` — <Esc> closes"), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// A jump elsewhere leaves the view rather than carry it along.
    #[test]
    fn a_jump_leaves_the_archive() {
        let (mut a, zip) = app_on_archive();
        a.act(Act::Enter);
        wait(&mut a);
        let elsewhere = zip.parent().unwrap().join("src");
        a.cd(elsewhere, true);
        assert!(!a.in_archive_view());
    }
}

/// The usage view rides on the same machinery the search view does: a `Folder`
/// whose path is not a real directory. These pin the parts that make that work.
#[cfg(test)]
mod usage_view {
    use super::*;

    /// One directory per test. They run in parallel in one process, so a name
    /// built from the pid alone has them wiping each other's trees mid-walk.
    fn tree() -> PathBuf {
        let dir = crate::util::test_dir("usage-view");
        std::fs::create_dir_all(dir.join("fat").join("inner")).unwrap();
        std::fs::create_dir_all(dir.join("thin")).unwrap();
        std::fs::write(dir.join("fat").join("inner").join("a"), vec![b'x'; 900]).unwrap();
        std::fs::write(dir.join("thin").join("b"), vec![b'x'; 20]).unwrap();
        std::fs::write(dir.join("loose"), vec![b'x'; 100]).unwrap();
        dir
    }

    fn app_in(dir: &Path) -> App {
        let mut a = App::new(Config::load(), dir.to_path_buf(), egui::Context::default());
        a.tabs[a.active].cwd = dir.to_path_buf();
        a.tabs[a.active].current = Folder::loading(dir.to_path_buf(), None);
        a
    }

    /// Largest first, folders measured through their children, and the bars'
    /// scale taken from the biggest row. Also TESTING.md 44.15 — once the walk
    /// is done only the total's toast is left.
    #[test]
    fn the_biggest_thing_comes_first() {
        let dir = tree();
        let mut a = app_in(&dir);
        a.start_usage();
        assert!(a.usage.is_some(), "a walk is running");
        // Drain until the worker says it is done.
        for _ in 0..2000 {
            a.drain_usage();
            if a.usage.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(a.usage.is_none(), "the walk finished");
        // #114: the total replaces "Measuring…" rather than standing beside it.
        assert!(a.toasts.iter().any(|t| t.text.contains("in total")));
        assert!(!a.toasts.iter().any(|t| t.text.starts_with("Measuring")), "no longer measuring");
        let names: Vec<&str> =
            a.tabs[a.active].current.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["fat", "loose", "thin"], "largest first: {names:?}");
        assert_eq!(a.usage_max, 900, "the bars are drawn against the biggest row");
        let fat = a.tabs[a.active].current.entries.iter().find(|e| e.name == "fat").unwrap();
        assert_eq!(fat.usage_bytes(), 900, "a folder is worth what is under it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `l` on a folder measures that folder and stays in the view; `h` comes
    /// back up with the cursor on the folder it left, and from where `gu` was
    /// pressed, leaves.
    #[test]
    fn the_view_goes_down_and_comes_back_up() {
        let dir = tree();
        let mut a = app_in(&dir);
        let settle = |a: &mut App| {
            for _ in 0..2000 {
                a.drain_usage();
                if a.usage.is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        let names = |a: &App| -> Vec<String> {
            a.tabs[a.active].current.entries.iter().map(|e| e.name.clone()).collect()
        };
        a.start_usage();
        settle(&mut a);
        assert!(a.tabs[a.active].current.select_name("fat"));
        // `<Enter>` is bound to `open`, which reaches folders the same way.
        a.act(Act::Open { interactive: false, hovered: false });
        settle(&mut a);
        assert!(a.in_usage_view(), "still measuring, one level down");
        assert_eq!(a.tabs[a.active].cwd, dir.join("fat"));
        assert_eq!(names(&a), ["inner"]);
        assert_eq!(a.tabs[a.active].current.entries[0].usage, Some(900), "measured, not just listed");
        assert_eq!(a.usage_max, 900);
        assert_eq!(a.toasts.iter().filter(|t| t.text.contains(" in total")).count(), 1, "this level's total only");
        // The plain listing of `fat` arriving late must not replace the view.
        a.apply_listing(&dir.join("fat"), Arc::new(Vec::new()));
        assert_eq!(names(&a), ["inner"], "the view keeps its rows");

        a.act(Act::Leave);
        settle(&mut a);
        assert!(a.in_usage_view(), "back up, still in the view");
        assert_eq!(a.tabs[a.active].cwd, dir);
        assert_eq!(a.tabs[a.active].current.hovered_name(), Some("fat"), "on the folder it came out of");

        a.act(Act::Leave);
        assert!(!a.in_usage_view(), "from where `gu` was pressed, `h` leaves");
        assert!(!a.toasts.iter().any(|t| t.text.contains("to leave")), "nothing left saying how to leave");
        assert_eq!(a.tabs[a.active].cwd, dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #109: re-sorting the usage view keeps the hidden rows it counted, and
    /// `,s` sorts by the measured totals rather than by each entry's own length.
    #[test]
    fn re_sorting_keeps_every_row_and_sorts_by_total() {
        use crate::fs::SortBy;
        let dir = tree();
        std::fs::write(dir.join(".hidden"), vec![b'x'; 500]).unwrap();
        let mut a = app_in(&dir);
        a.tabs[a.active].show_hidden = false;
        a.start_usage();
        for _ in 0..2000 {
            a.drain_usage();
            if a.usage.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let rows = |a: &App| a.tabs[a.active].current.view.len();
        assert_eq!(rows(&a), 4, "fat, .hidden, loose, thin");

        a.act(Act::Sort { by: Some(SortBy::Alphabetical), reverse: Some(false), dir_first: Some(false) });
        assert_eq!(rows(&a), 4, "the hidden row is still there after a re-sort");

        a.act(Act::Sort { by: Some(SortBy::Size), reverse: Some(true), dir_first: Some(false) });
        let names: Vec<String> =
            (0..rows(&a)).filter_map(|i| a.tabs[a.active].current.at(i).map(|e| e.name.clone())).collect();
        assert_eq!(names, ["fat", ".hidden", "loose", "thin"], "largest total first: {names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Q33: the view shows sizes whatever the tab was showing, and gives the
    /// tab its own mode back on the way out -- along with dropping the
    /// "Measuring…" toast, which stopped being true when the walk did (#109).
    #[test]
    fn the_view_shows_sizes_and_puts_the_linemode_back() {
        use crate::fs::entry::Linemode;
        let dir = tree();
        let mut a = app_in(&dir);
        a.tabs[a.active].linemode = Linemode::Mtime;

        a.start_usage();
        assert_eq!(a.tabs[a.active].linemode, Linemode::Usage, "numbers without a config change");
        assert!(a.toasts.iter().any(|t| t.text.starts_with("Measuring")));

        a.exit_search_view();
        assert_eq!(a.tabs[a.active].linemode, Linemode::Mtime, "the tab's own mode again");
        assert!(!a.toasts.iter().any(|t| t.text.starts_with("Measuring")), "no longer measuring");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The view is a `Folder` with a path that is not a directory, which is what
    /// `in_search_view` tests -- so `<Esc>` and `leave` already handle it, and
    /// dropping the handle stops the walk.
    #[test]
    fn leaving_the_view_stops_the_walk() {
        let dir = tree();
        let mut a = app_in(&dir);
        a.start_usage();
        assert!(a.in_search_view(), "the usage view is not a real directory");
        a.exit_search_view();
        assert!(a.usage.is_none(), "the handle is dropped, so the walk is cancelled");
        assert_eq!(a.usage_max, 0);
        assert!(!a.in_search_view(), "and we are back in the directory");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Starting a usage walk from a view that is already synthetic would leave
    /// no directory to go back to.
    #[test]
    fn it_refuses_to_start_from_another_synthetic_view() {
        let dir = tree();
        let mut a = app_in(&dir);
        a.start_usage();
        let path = a.tabs[a.active].current.path.clone();
        a.start_usage();
        assert_eq!(a.tabs[a.active].current.path, path, "the view did not change");
        let _ = std::fs::remove_dir_all(&dir);
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
        let dir = crate::util::test_dir("spot-follow");
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

#[cfg(test)]
mod diff_scrolling {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// `G` then `k` has to move the view, and used not to.
    ///
    /// `offset` is where the pane starts drawing, so the furthest it can
    /// usefully go is the value that puts the last row at the *bottom*.
    /// Clamping it to the last row instead let `G` leave it a screenful past
    /// that, and the renderer drew from its own, lower, clamp — so the next
    /// `j` or `k` changed a number nothing read. The keys came back only once
    /// enough presses had walked `offset` down into the range being drawn,
    /// which is why a half-page `<C-u>` looked like it repaired them.
    #[test]
    fn the_keys_answer_at_the_bottom() {
        let mut a = app();
        let rows: Vec<diff::Row> = (0..100)
            .map(|n| diff::Row {
                left: Some(diff::Line { no: n, text: format!("line {n}"), changed: Vec::new() }),
                right: Some(diff::Line { no: n, text: format!("line {n}"), changed: Vec::new() }),
                same: true,
            })
            .collect();
        // 20 rows of pane, as the renderer would have reported after a frame.
        a.overlay = Overlay::Diff(DiffOverlay {
            left: PathBuf::from("a"),
            right: PathBuf::from("b"),
            outcome: Some(diff::Outcome::Rows { rows, truncated: false, rough: false }),
            offset: 0,
            rows: 20,
            cursor: 0,
            hide_same: false, back: None,
        });
        let at = |a: &App| match &a.overlay {
            Overlay::Diff(ov) => ov.offset,
            _ => panic!("the overlay closed"),
        };

        a.diff_act(Act::Arrow(Step::Bot));
        assert_eq!(at(&a), 80, "the last row sits at the bottom, not at the top");

        a.diff_act(Act::Arrow(Step::Rel(-1)));
        assert_eq!(at(&a), 79, "and one back is one row, not a dead press");

        a.diff_act(Act::Arrow(Step::Rel(1)));
        assert_eq!(at(&a), 80, "forward returns to the bottom");
        a.diff_act(Act::Arrow(Step::Rel(1)));
        assert_eq!(at(&a), 80, "and stops there");

        a.diff_act(Act::Arrow(Step::Top));
        assert_eq!(at(&a), 0);
    }

    /// A file shorter than the pane has nowhere to scroll.
    #[test]
    fn a_short_diff_does_not_move() {
        let mut a = app();
        let rows: Vec<diff::Row> =
            (0..3).map(|n| diff::Row { left: None, right: None, same: n % 2 == 0 }).collect();
        a.overlay = Overlay::Diff(DiffOverlay {
            left: PathBuf::from("a"),
            right: PathBuf::from("b"),
            outcome: Some(diff::Outcome::Rows { rows, truncated: false, rough: false }),
            offset: 0,
            rows: 20,
            cursor: 0,
            hide_same: false, back: None,
        });
        a.diff_act(Act::Arrow(Step::Bot));
        match &a.overlay {
            Overlay::Diff(ov) => assert_eq!(ov.offset, 0, "all three are already on screen"),
            _ => panic!("the overlay closed"),
        }
    }
}

#[cfg(test)]
mod outline_jump {
    use super::*;
    use crate::preview::TocEntry;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// An outline entry near the end of a file must not scroll past it.
    ///
    /// The draw paints from whatever `preview_offset` says and corrects the
    /// number after, so an offset beyond the content costs one wrong frame.
    /// A tap hides that; a held key turns it into a flicker. `seek` was
    /// clamped for this exact reason and the outline's own arrow was missed,
    /// which is why it survived: it shows only on the last entry or two, and
    /// only in a file whose last heading sits near the end — so it reproduces
    /// on one document and not on the next.
    #[test]
    fn it_stops_where_the_pane_does() {
        let mut a = app();
        let toc = |line: usize| TocEntry { level: 1, label: format!("h{line}"), line };
        a.preview.state = PreviewState::Ready(Payload::Text {
            lines: Vec::new(),
            map: Vec::new(),
            extent: crate::preview::Extent { truncated: false, total: 500, ..Default::default() },
            outline: vec![toc(0), toc(120), toc(480)],
        });
        // The furthest the pane can be scrolled, as the last draw worked out.
        a.preview.max_offset = 300;
        a.preview.outline = Some(0);

        // A middle entry is inside the range and lands on its own line.
        assert!(a.outline_act(&Act::Arrow(Step::Rel(1))));
        assert_eq!(a.tabs[a.active].preview_offset, 120);

        // The last one is past the end, and stops at the end instead.
        assert!(a.outline_act(&Act::Arrow(Step::Rel(1))));
        assert_eq!(a.preview.outline, Some(2), "the cursor still reaches it");
        assert_eq!(
            a.tabs[a.active].preview_offset, 300,
            "but the pane is not asked to draw from beyond the content",
        );

        // Held down at the end: every extra press has to be a no-op.
        for _ in 0..20 {
            a.outline_act(&Act::Arrow(Step::Rel(1)));
        }
        assert_eq!(a.preview.outline, Some(2));
        assert_eq!(a.tabs[a.active].preview_offset, 300, "no frame is drawn past the end");
    }
}

#[cfg(test)]
mod goto_and_history_keys {
    use crate::config::keymap;

    /// `g`+`c` goes to filer's own directory, not yazi's.
    ///
    /// Both are read, but they hold different things: `filer.toml` only ever
    /// belongs in filer's, and that is the one a reader cannot find, because
    /// it is often the directory that does not exist yet. yazi's keeps its own
    /// key rather than the shared one.
    #[test]
    fn the_two_config_directories_have_a_key_each() {
        let (km, warnings) = keymap::Keymap::load(&[]);
        assert!(warnings.is_empty(), "{warnings:?}");
        let run = |key: &str| {
            km.mgr
                .iter()
                .find(|b| crate::config::keys::render_seq(&b.on) == key)
                .unwrap_or_else(|| panic!("`{key}` is not bound"))
                .raw
                .clone()
        };
        // Written as the variables filer actually searches, so they resolve on
        // every platform. `%APPDATA%` named nothing outside Windows, and an unset
        // `%VAR%` expands to nothing, so `gc` used to walk to `/filer` there.
        assert_eq!(run("gc"), "cd %FILER_CONFIG_HOME%");
        assert_eq!(run("gy"), "cd %YAZI_CONFIG_HOME%");
        for (key, var) in [("gc", "FILER_CONFIG_HOME"), ("gy", "YAZI_CONFIG_HOME")] {
            let dir = crate::config::config_home(var).expect("a config directory");
            assert_eq!(crate::util::expand(&format!("%{var}%")), dir, "{key}");
            assert!(dir.is_absolute(), "{key} must not land on a relative path: {dir:?}");
        }
        // The pair is the same two directories the help panel and `filer env` list.
        assert_eq!(
            crate::config::config_dirs(),
            vec![
                crate::config::config_home("YAZI_CONFIG_HOME").unwrap(),
                crate::config::config_home("FILER_CONFIG_HOME").unwrap(),
            ]
        );

        // The pair the arrows reach, alongside the `H`/`L` that already did.
        assert_eq!(run("<A-Left>"), "back");
        assert_eq!(run("<A-Right>"), "forward");
        assert_eq!(run("H"), "back");
        assert_eq!(run("L"), "forward");
    }

    /// `T` maximizes the preview column, and is spelled as the shifted character.
    ///
    /// The command had a parser and an implementation but no key, so the feature
    /// was only reachable by writing a `prepend_keymap` line by hand. Pinned
    /// through the keymap rather than the parser, because the two spellings of a
    /// shifted letter parse equally well and only one of them ever matches.
    #[test]
    fn t_maximizes_the_preview_pane() {
        let (km, warnings) = keymap::Keymap::load(&[]);
        assert!(warnings.is_empty(), "{warnings:?}");
        let binding = km
            .mgr
            .iter()
            .find(|b| crate::config::keys::render_seq(&b.on) == "T")
            .expect("`T` is not bound");
        assert_eq!(binding.raw, "plugin toggle-pane max-preview");
        assert_eq!(crate::config::cmd::parse(&binding.raw), crate::config::cmd::Act::MaxPreview);

        // `t` is the new tab it sits next to; shifting it must not have moved it.
        let lower = km
            .mgr
            .iter()
            .find(|b| crate::config::keys::render_seq(&b.on) == "t")
            .expect("`t` is not bound");
        assert_eq!(lower.raw, "tab_create --current");
    }
}

#[cfg(test)]
mod send_pane_and_the_register {
    use super::*;

    /// `<A-c>` leaves the yank register alone, which the README now promises.
    ///
    /// It is the reason the key is not `<A-y>`: *yank* means "into the
    /// register", and this never goes near it — so `p` after an `<A-c>` still
    /// pastes whatever `y` last held. A name built on `y` would promise a `p`
    /// that is not wanted and an overwrite that does not happen, and if this
    /// ever started clearing or replacing the register, that promise would be
    /// the thing that broke.
    #[test]
    fn a_send_does_not_disturb_what_is_yanked() {
        let root = crate::util::test_dir("send-pane");
        let (left, right) = (root.join("left"), root.join("right"));
        let _ = std::fs::create_dir_all(&left);
        let _ = std::fs::create_dir_all(&right);
        let sent = left.join("sent.txt");
        std::fs::write(&sent, "x").unwrap();

        let ctx = egui::Context::default();
        let mut a = App::new(Config::load(), std::env::temp_dir(), ctx);
        a.tabs[a.active].cwd = left.clone();
        a.tabs[a.active].current = Folder::from_entries(
            left.clone(),
            Arc::new(vec![crate::fs::Entry::from_path(sent.clone()).unwrap()]),
            true,
        );
        // A second tab, shown in the other pane.
        let sort = a.tabs[a.active].sort;
        let mut other = crate::core::tab::Tab::new(right.clone(), sort, false, crate::fs::entry::Linemode::None);
        other.cwd = right.clone();
        a.tabs.push(other);
        a.split = Some(Split { other: 1, right: false });

        // Something else is held in the register.
        let held = PathBuf::from("held.txt");
        a.yank = Yank { paths: vec![held.clone()], cut: false, from_archive: false };

        a.act(Act::SendPane { cut: false });

        // Both of `send_to_pane`'s refusals leave the register alone too, so
        // without this the assertion below would pass on a send that never
        // happened.
        assert!(
            !a.toasts.iter().any(|t| t.level == Level::Error),
            "the send went through: {:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>(),
        );
        assert!(!a.tasks.is_empty(), "a copy was actually queued");

        assert_eq!(a.yank.paths, vec![held], "the register is untouched by a send");
        assert!(!a.yank.cut, "and so is what it is holding it for");

        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod follow_says_what_it_is_for {
    use super::*;

    /// `g`+`f` on an ordinary file used to be indistinguishable from an
    /// unbound key.
    ///
    /// The row does say so — a link is drawn with `->` after its name — but
    /// only if you already know to look, and nothing said that. The message
    /// names both the one kind of thing the key works on and the mark that
    /// identifies it.
    #[test]
    fn an_ordinary_file_is_told_that_it_is_not_a_link() {
        let dir = crate::util::test_dir("follow-msg");
        let plain = dir.join("plain.txt");
        std::fs::write(&plain, "x").unwrap();

        let ctx = egui::Context::default();
        let mut a = App::new(Config::load(), std::env::temp_dir(), ctx);
        a.tabs[a.active].cwd = dir.clone();
        a.tabs[a.active].current = Folder::from_entries(
            dir.clone(),
            Arc::new(vec![crate::fs::Entry::from_path(plain).unwrap()]),
            true,
        );

        a.act(Act::Follow);

        let said = a.toasts.last().expect("it says something now");
        assert_eq!(said.level, Level::Error, "the key could not do its job");
        assert!(said.text.contains("symlink"), "it names what the key is for: {}", said.text);
        assert!(said.text.contains("->"), "and how to spot one: {}", said.text);

        // Nothing under the cursor at all stays quiet: there is no row to
        // describe, and every other key is silent there too.
        a.tabs[a.active].current = Folder::from_entries(dir.clone(), Arc::new(Vec::new()), true);
        a.toasts.clear();
        a.act(Act::Follow);
        assert!(a.toasts.is_empty(), "an empty directory says nothing");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod window_scale {
    use super::*;
    use crate::config::cmd::ScaleTo;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// Scaling steps and stops where egui's own did.
    ///
    /// egui zooms on Ctrl +/-/0 at the end of every frame, and does not
    /// consume the key on the way: each of those three is a key filer binds,
    /// so both ran. `<C-->` made a hardlink *and* shrank the window, which is
    /// the sort of thing that reads as the program being possessed.
    #[test]
    fn it_steps_and_stops_where_egui_did() {
        let mut a = app();
        assert_eq!(a.scale, 1.0);

        a.act(Act::Scale(ScaleTo::In));
        assert!((a.scale - 1.1).abs() < 1e-6, "{}", a.scale);
        a.act(Act::Scale(ScaleTo::Out));
        a.act(Act::Scale(ScaleTo::Out));
        assert!((a.scale - 0.9).abs() < 1e-6, "{}", a.scale);

        a.act(Act::Scale(ScaleTo::Reset));
        assert_eq!(a.scale, 1.0);

        // The same bounds, so a held key cannot shrink it to nothing.
        for _ in 0..40 {
            a.act(Act::Scale(ScaleTo::Out));
        }
        assert_eq!(a.scale, 0.2);
        for _ in 0..80 {
            a.act(Act::Scale(ScaleTo::In));
        }
        assert_eq!(a.scale, 5.0);
    }

    /// A panel does not get to swallow the window's own size.
    ///
    /// v0.46.0 bound the scale chords in `[help]`, `[spot]`, `[tasks]` and
    /// `[diff]` and shipped them doing nothing: the binding resolved, and then
    /// every layer's `act` dropped the `Act` on its closing `_ => {}`. Nothing
    /// in the build caught it, because a binding that resolves to an ignored
    /// action is indistinguishable from a working one until a person presses
    /// the key. This goes through `feed_overlay_key`, the same door the window
    /// uses, so a fifth panel added later fails here rather than in someone's
    /// hands.
    #[test]
    fn every_panel_lets_the_window_be_resized() {
        use crate::app::{DiffOverlay, SpotOverlay, TasksOverlay};
        let mut a = app();
        let panels: Vec<(&str, Overlay)> = vec![
            ("help", Overlay::Help),
            ("tasks", Overlay::Tasks(TasksOverlay { cursor: 0 })),
            ("spot", Overlay::Spot(SpotOverlay { cursor: 0, scroll: 0 })),
            ("diff", Overlay::Diff(DiffOverlay {
                left: std::path::PathBuf::from("a"),
                right: std::path::PathBuf::from("b"),
                outcome: None,
                offset: 0,
                rows: 10,
                cursor: 0,
                hide_same: false, back: None,
            })),
        ];
        for (name, overlay) in panels {
            a.scale = 1.0;
            a.overlay = overlay;
            a.feed_overlay_key(Key::parse("<C-+>").unwrap());
            assert!((a.scale - 1.1).abs() < 1e-6, "`<C-+>` with {name} open: {}", a.scale);
            a.feed_overlay_key(Key::parse("<C-->").unwrap());
            a.feed_overlay_key(Key::parse("<C-->").unwrap());
            assert!((a.scale - 0.9).abs() < 1e-6, "`<C-->` with {name} open: {}", a.scale);
            a.feed_overlay_key(Key::parse("<C-0>").unwrap());
            assert_eq!(a.scale, 1.0, "`<C-0>` with {name} open");
            // The panel is still up: resizing is not a way out of it.
            assert!(!matches!(a.overlay, Overlay::None), "{name} closed itself");
        }
    }

    /// The two zooms are different commands, and stay that way.
    ///
    /// `zoom` was already taken by the image preview. Naming this one the same
    /// would have made the keymap ambiguous in a way only the parser could see.
    #[test]
    fn the_image_zoom_is_a_different_command() {
        use crate::config::cmd::parse;
        assert_eq!(parse("scale out"), Act::Scale(ScaleTo::Out));
        assert!(matches!(parse("zoom out"), Act::Zoom(_)));
    }
}

#[cfg(test)]
mod help_keys {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        let mut a = App::new(Config::load(), std::env::temp_dir(), ctx);
        a.overlay = Overlay::Help;
        // What the renderer leaves behind: a panel 20 rows tall showing a list
        // of 100.
        a.help_rows = 20;
        a.help_lines = 100;
        a
    }

    /// Each token is one key, spelled the way the keymap spells it.
    fn press(a: &mut App, tokens: &[&str]) {
        for t in tokens {
            a.feed_overlay_key(Key::parse(t).expect("notation the keymap can spell"));
        }
    }

    /// `<A-j>` / `<A-k>` move half the panel, the way they do in every other
    /// pane this app scrolls.
    #[test]
    fn alt_j_and_k_move_half_a_panel() {
        let mut a = app();
        press(&mut a, &["<A-j>"]);
        assert_eq!(a.help_scroll, 10, "half of the twenty rows on screen");
        press(&mut a, &["<A-j>"]);
        assert_eq!(a.help_scroll, 20);
        press(&mut a, &["<A-k>"]);
        assert_eq!(a.help_scroll, 10);

        // `<C-d>` / `<C-u>` are the same distance, for a hand coming from vim.
        press(&mut a, &["<C-d>"]);
        assert_eq!(a.help_scroll, 20);
        press(&mut a, &["<C-u>"]);
        assert_eq!(a.help_scroll, 10);
    }

    /// Scrolling stops with the last line at the bottom, not at the top.
    ///
    /// It used to stop nowhere at all: `help_scroll` was incremented raw, so
    /// holding `j` ran the number far past the end of the list while the panel
    /// sat still. Every press back up then moved a number nothing was drawing
    /// from, and the keys looked dead for exactly as many presses as had been
    /// wasted going down.
    #[test]
    fn it_stops_with_the_last_line_on_screen() {
        let mut a = app();
        for _ in 0..500 {
            press(&mut a, &["j"]);
        }
        assert_eq!(a.help_scroll, 80, "100 lines less the 20 on screen");
        press(&mut a, &["k"]);
        assert_eq!(a.help_scroll, 79, "and one press comes straight back");

        press(&mut a, &["G"]);
        assert_eq!(a.help_scroll, 80);
        press(&mut a, &["g", "g"]);
        assert_eq!(a.help_scroll, 0);
        press(&mut a, &["k"]);
        assert_eq!(a.help_scroll, 0, "nor does it go above the first line");
    }

    /// A panel with room to spare does not scroll at all.
    #[test]
    fn a_short_list_does_not_move() {
        let mut a = app();
        a.help_lines = 5;
        press(&mut a, &["<A-j>"]);
        press(&mut a, &["G"]);
        assert_eq!(a.help_scroll, 0);
    }

    /// The keys that opened the panel close it, and so do `q` and `<Esc>`.
    #[test]
    fn it_closes_on_its_own_keys() {
        for seq in ["<Esc>", "q", "~", "<F1>"] {
            let mut a = app();
            press(&mut a, &[seq]);
            assert!(a.overlay.is_none(), "{seq} should have closed the panel");
        }
    }

    /// The panel's keys come from the keymap, so they can be rebound.
    ///
    /// They were read straight off the egui event before, which left the one
    /// panel whose subject is the keymap unable to honor it.
    #[test]
    fn the_layer_is_the_keymap_not_the_event_loop() {
        let text = "[[help.keymap]]\non = \"n\"\nrun = \"arrow 1\"\n";
        let (km, _) = crate::config::Keymap::load(&[text]);
        let mut a = app();
        a.cfg.keymap = km;
        press(&mut a, &["n"]);
        assert_eq!(a.help_scroll, 1, "the added key scrolls");
    }

    /// A panel over the list owns the wheel; a one-row prompt does not.
    #[test]
    fn a_modal_panel_takes_the_wheel() {
        assert!(Overlay::Help.is_modal());
        assert!(!Overlay::None.is_modal());
        let prompt = InputOverlay {
            kind: InputKind::Filter,
            title: "filter".into(),
            text: String::new(),
            initial_selection: None,
            focused: true,
            completion: Vec::new(),
            completion_at: 0,
        };
        assert!(!Overlay::Input(prompt).is_modal(), "the list above it is still being read");
    }
}

/// `<Esc>` gets out of a maximized preview, and does it in the right order.
#[cfg(test)]
mod escape_and_max_preview {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// Reported from use: `<Esc>` did nothing under a maximized preview, so the
    /// next key tried was `q` -- which quits. The list is squeezed to nothing
    /// there, so the screen reads as a panel even though it is a column width.
    #[test]
    fn a_bare_escape_restores_the_columns() {
        let mut a = app();
        a.max_preview = true;
        a.escape(EscapeWhat::default());
        assert!(!a.max_preview, "`Esc` has to be a way out of this");
    }

    /// Maximising is not offered where it cannot be honoured, and it never
    /// leaves the keys somewhere off screen.
    ///
    /// `Esc` is deliberately *not* a way out of this one, unlike the maximised
    /// preview above. Inside the pane it belongs to the shell -- a TUI wants its
    /// own -- and outside the pane this state cannot arise, because maximising
    /// gives the pane the keys and every way out of the pane restores the size.
    #[test]
    fn maximising_the_pane_takes_the_keys_with_it() {
        // No pane: nothing happens, and nothing is remembered for the next one.
        let mut a = app();
        assert!(a.term.is_none(), "no pane in a fresh app");
        a.act(Act::MaxTerm);
        assert!(!a.max_term, "nothing to maximise, so nothing is armed");
        assert!(!a.term_focus, "and no keys are sent anywhere");
    }

    /// Leaving the pane hands the window back, by every route out of it.
    ///
    /// `<C-t>` is the one anybody presses; the other two are the pane going
    /// away underneath a maximised flag, which would otherwise be waiting for
    /// whoever opens the next one.
    #[test]
    fn leaving_the_pane_un_maximises_it() {
        // `<C-t>` from inside: `close` on the terminal keymap.
        let mut a = app();
        a.max_term = true;
        a.term_focus = true;
        a.feed_term_key(crate::config::keys::Key::ctrl('t'), None);
        assert!(!a.term_focus, "the keys go back to the list");
        assert!(!a.max_term, "and the window with them");

        // The shell was told to go: `terminal close`.
        let mut a = app();
        a.max_term = true;
        a.act(Act::Terminal(Some(false)));
        assert!(!a.max_term, "no pane left to be maximised");
    }

    /// Q49: `<C-F5>` that changes `[term]` while a pane runs says the pane keeps
    /// its shell until `<C-S-t>`. Without a pane, or with `[term]` unchanged,
    /// the toast is the plain one -- the next `<C-t>` picks the new shell up.
    #[test]
    fn a_reload_that_changes_the_shell_says_the_pane_keeps_its_own() {
        let mut a = app();
        let last = |a: &App| a.toasts.last().map(|t| t.text.clone()).unwrap_or_default();
        a.act(Act::Terminal(Some(true)));
        if a.term.is_none() {
            if cfg!(any(windows, target_os = "linux")) {
                panic!("the terminal did not start: {}", last(&a));
            }
            return;
        }
        // What the files on disk will not say, whatever they hold.
        a.cfg.term.shell = "not-the-configured-shell".into();
        a.reload_config();
        assert!(last(&a).ends_with("— the pane keeps its shell until <C-S-t> closes it"), "{}", last(&a));

        a.reload_config();
        assert!(last(&a).starts_with("Reloaded") && !last(&a).contains("shell"), "unchanged: {}", last(&a));

        a.term = None;
        a.cfg.term.shell = "not-the-configured-shell".into();
        a.reload_config();
        assert!(!last(&a).contains("shell"), "no pane to keep one: {}", last(&a));
    }

    /// Q53: `<C-S-t>` from the list ends the shell too, not only from inside
    /// the pane: `<C-t>` back to the list and then `<C-S-t>` did nothing, and a
    /// re-test took the old shell for a reload that had not worked (#182). With
    /// no pane it says so rather than nothing.
    #[test]
    fn ending_the_shell_from_the_list() {
        let mut a = app();
        let key = |a: &mut App| a.feed_key(crate::config::keys::Key::parse("<C-S-t>").unwrap());
        key(&mut a);
        assert_eq!(a.toasts.last().map(|t| t.text.as_str()), Some("No terminal to close"));

        a.act(Act::Terminal(Some(true)));
        if a.term.is_none() {
            if cfg!(any(windows, target_os = "linux")) {
                panic!("the terminal did not start");
            }
            return;
        }
        a.term_focus = false;
        key(&mut a);
        // A shell still reading its profile has children of its own, and then
        // the question comes first -- the same as from inside the pane.
        if matches!(&a.overlay, Overlay::Confirm(c) if matches!(c.action, ConfirmAction::EndShell)) {
            a.answer_confirm('y');
        }
        assert!(a.term.is_none(), "the list's `<C-S-t>` ended the shell");
        assert_eq!(a.toasts.last().map(|t| t.text.as_str()), Some("Ended the shell"));
    }

    /// `<C-S-t>` asks before ending a shell that is running something, and
    /// does what the answer says (Q21).
    ///
    /// A real shell on a real PTY, because the question is about a real child
    /// process: the shell is given a long-running command and the test waits,
    /// with a deadline, until that command shows up under it. Only the busy
    /// path is asserted -- whether an idle shell keeps a helper process of its
    /// own is the platform's business, and "busy" errs toward asking anyway.
    #[test]
    fn ending_a_busy_shell_asks_first() {
        let mut a = app();
        a.act(Act::Terminal(Some(true)));
        let Some(t) = a.term.as_ref() else {
            // Not a quiet pass: this used to `return` here, and a green run
            // could not say whether the shell had ever started (TODO, v0.52.1).
            // Windows (ConPTY) and Linux (a pty pair) always have one, so no
            // terminal there is a failure, with the reason the app gave.
            let why: Vec<&str> = a.toasts.iter().map(|t| t.text.as_str()).collect();
            if cfg!(any(windows, target_os = "linux")) {
                panic!("the terminal did not start: {why:?}");
            }
            eprintln!("skipped: no terminal on this platform: {why:?}");
            return;
        };
        // A shell starting up runs short-lived children of its own (profile
        // scripts), and the first version of this test caught one of those
        // instead of the command and then found it gone. So let it settle
        // first: idle for a stretch, or give up waiting after a few seconds
        // (a shell that keeps a helper for ever never goes idle, and that is
        // fine -- the command still has to show up below).
        let (settle, mut idle_since) = (std::time::Instant::now(), std::time::Instant::now());
        while settle.elapsed() < std::time::Duration::from_secs(5) {
            if t.busy() {
                idle_since = std::time::Instant::now();
            } else if idle_since.elapsed() > std::time::Duration::from_millis(500) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let cmd: &[u8] = if cfg!(windows) { b"ping -n 60 127.0.0.1\r" } else { b"sleep 60\r" };
        t.send(cmd.to_vec());
        // Busy for a stretch, not for one look: the settling above gives up
        // after five seconds, and on a loaded machine a profile script's child
        // was still about, so one glance saw it and went on before the command
        // had started (#122 saw this once in 553 on ARM64). The command runs
        // for a minute; a child that is gone within half a second is not it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut busy_since: Option<std::time::Instant> = None;
        loop {
            match a.term.as_ref().is_some_and(|t| t.busy()) {
                true => {
                    let since = *busy_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() > std::time::Duration::from_millis(500) {
                        break;
                    }
                }
                false => busy_since = None,
            }
            assert!(std::time::Instant::now() < deadline, "the command never showed up under the shell");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        a.act(Act::Terminal(Some(false)));
        assert!(
            matches!(&a.overlay, Overlay::Confirm(c) if matches!(c.action, ConfirmAction::EndShell)),
            "a running program is asked about",
        );
        a.answer_confirm('n');
        assert!(a.term.is_some(), "`n` keeps the shell and what it runs");

        a.act(Act::Terminal(Some(false)));
        a.answer_confirm('y');
        assert!(a.term.is_none(), "`y` ends it");
    }

    /// Q35: `<A-t>` with the pane closed opens it and types the name once the
    /// shell has drawn something, rather than saying the pane is not open.
    /// A real shell, because "ready" is a question only a real one answers:
    /// the name has to come back on the screen as the shell's own echo.
    #[test]
    fn sending_a_name_opens_a_closed_pane() {
        let dir = crate::util::test_dir("q35-send");
        let file = dir.join("q35-marker.txt");
        std::fs::write(&file, "x").unwrap();
        let mut a = app();
        a.tabs[a.active].cwd = dir.clone();
        let entries = vec![crate::fs::Entry::from_path(file.clone()).unwrap()];
        a.tabs[a.active].current = Folder::from_entries(dir, Arc::new(entries), true);
        assert!(a.term.is_none());

        a.act(Act::TermSend);
        if a.term.is_none() {
            let why: Vec<&str> = a.toasts.iter().map(|t| t.text.as_str()).collect();
            if cfg!(any(windows, target_os = "linux")) {
                panic!("the terminal did not start: {why:?}");
            }
            eprintln!("skipped: no terminal on this platform: {why:?}");
            return;
        }
        assert!(!a.toasts.iter().any(|t| t.text.contains("not open")), "no refusal");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        // The rows joined before searching: the pane is 80 columns here, and
        // with a long `TEMP` the echoed path wraps, putting the name across
        // two rows. Looked for row by row, it was never found -- always, on a
        // machine with no RAM disk, whose scratch path is long (#203).
        // The pane's text, joined, and how wide each row it came from is: on
        // a failure, the two say whether the name never arrived or arrived
        // wrapped. #203 and #206 each had to add a print to tell (#206).
        let pane = |a: &App| {
            a.term.as_ref().map_or((String::new(), Vec::new()), |t| {
                t.with_grid(|g| {
                    let rows = crate::terminal::snapshot(g);
                    let text = rows.iter().flat_map(|row| row.iter().map(|c| c.c)).collect::<String>();
                    let widths = rows.iter().map(|row| row.iter().filter(|c| c.c != ' ').count()).collect();
                    (text, widths)
                })
            })
        };
        while !(a.term_pending.is_none() && pane(&a).0.contains("q35-marker.txt")) {
            if std::time::Instant::now() >= deadline {
                let (text, widths) = pane(&a);
                panic!(
                    "the name never reached the shell; the pane held {:?}, rows {widths:?} wide",
                    text.split_whitespace().collect::<Vec<_>>().join(" "),
                );
            }
            a.pump_terminal();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // The whole path, not only its end: with a scratch longer than a row
        // this holds only because the rows were joined, so a re-test can read
        // that from the test passing instead of undoing the fix to see (#206).
        let (text, _) = pane(&a);
        let sent = file.display().to_string();
        assert!(text.contains(&sent), "the whole path arrived ({} characters): {sent}", sent.len());
        a.act(Act::Terminal(Some(false)));
        if a.term.is_some() {
            a.answer_confirm('y');
        }
    }

    /// `copy all` lays the panel out as text a spreadsheet or a bug report can
    /// take: titles alone, rows as `Label<TAB>value`, sections apart (Q18).
    #[test]
    fn the_whole_spot_panel_copies_as_labelled_lines() {
        let sections = vec![
            Section { title: "File".into(), rows: vec![("Name".into(), "a b.txt".into()), ("Size".into(), "3 B".into())] },
            Section { title: "Git".into(), rows: vec![("Came in via".into(), "#71  48b6c9c".into())] },
        ];
        assert_eq!(spot_text(&sections), "File\nName\ta b.txt\nSize\t3 B\n\nGit\nCame in via\t#71  48b6c9c");
    }

    /// The default spot keys reach the two new commands: `C` copies it all,
    /// and `<Enter>` is `enter`, which opens a pull request on its rows.
    #[test]
    fn the_spot_keys_for_copy_all_and_enter() {
        use crate::config::keys::Key;
        let km = &Config::load().keymap;
        let run = |k: Key| km.spot.iter().find(|b| b.on == vec![k]).map(|b| b.run.clone());
        assert_eq!(run(Key::parse("C").unwrap()), Some(vec![Act::Copy(CopyWhat::All)]));
        assert_eq!(run(Key::parse("<Enter>").unwrap()), Some(vec![Act::Enter]));
    }

    /// Q48: `C` in the help panel copies the list it shows, as text -- every
    /// list key as `keys<TAB>description<TAB>command` under its heading, so a
    /// check can ask "is my new key listed" without a screenshot (#171).
    #[test]
    fn the_help_panel_copies_as_text() {
        use crate::config::keys::{render_seq, Key};
        let a = App::new(Config::load(), std::env::temp_dir(), egui::Context::default());
        let km = &a.cfg.keymap;
        let run = km.help.iter().find(|b| b.on == vec![Key::parse("C").unwrap()]).map(|b| b.run.clone());
        assert_eq!(run, Some(vec![Act::Copy(CopyWhat::All)]));

        let (text, keys) = crate::ui::overlay::help_text(&a);
        assert_eq!(keys, km.mgr.len(), "one line per list key");
        assert!(text.lines().any(|l| l == "keys"), "the heading on its own line");
        let b = &km.mgr[0];
        let want = format!("{}\t{}\t{}", render_seq(&b.on), if b.desc.is_empty() { &b.raw } else { &b.desc }, b.raw);
        assert!(text.lines().any(|l| l == want), "{want:?} in the copy");
        assert!(text.lines().any(|l| l == "config"), "the config section comes too");
    }

    /// Q36: `m u` puts the usage numbers back inside `gu`'s view after another
    /// `m` key took them away; before it there was no key for them at all.
    #[test]
    fn m_u_is_the_usage_line_mode() {
        use crate::config::keys::Key;
        use crate::fs::entry::Linemode;
        let km = &Config::load().keymap;
        let on = vec![Key::parse("m").unwrap(), Key::parse("u").unwrap()];
        let run = km.mgr.iter().find(|b| b.on == on).map(|b| b.run.clone());
        assert_eq!(run, Some(vec![Act::Linemode(Linemode::Usage)]));
    }

    /// `escape --filter` is aimed at one thing and must stay aimed at it.
    #[test]
    fn a_targeted_escape_leaves_it_alone() {
        let mut a = app();
        a.max_preview = true;
        a.escape(EscapeWhat { filter: true, ..Default::default() });
        assert!(a.max_preview);
        // `--all` is explicitly everything, so it does clear it.
        a.escape(EscapeWhat { all: true, ..Default::default() });
        assert!(!a.max_preview);
    }

    /// With both up, the panel is what is in front, so it goes first and the
    /// maximized column survives that press.
    #[test]
    fn the_quick_panel_goes_first() {
        let mut a = app();
        a.max_preview = true;
        a.quick = true;
        a.escape(EscapeWhat::default());
        assert!(!a.quick && a.max_preview, "one press should not undo two states");
        a.escape(EscapeWhat::default());
        assert!(!a.max_preview);
    }

    /// Turning it on unhides the parent pane, which means `T` twice is not a
    /// round trip. Pinned because it is deliberate and looks like a bug.
    #[test]
    fn turning_it_on_unhides_the_parent() {
        let mut a = app();
        a.hide_parent = true;
        a.act(Act::MaxPreview);
        assert!(!a.hide_parent);
        a.act(Act::MaxPreview);
        assert!(!a.max_preview && !a.hide_parent, "the parent stays back");
    }
}

/// `q` means the same thing over every panel: close this, do not quit.
#[cfg(test)]
mod q_closes_the_panel_in_front {
    use super::*;

    fn app() -> App {
        let ctx = egui::Context::default();
        App::new(Config::load(), std::env::temp_dir(), ctx)
    }

    /// Reported from use: `q` quit the app with quick look still up. The panels
    /// that carry their own keymap layer all bind `q` to `close`; these two have
    /// no layer, by design, so the key reached `mgr`'s `quit`.
    #[test]
    fn quick_look_closes_before_the_process_does() {
        let mut a = app();
        a.quick = true;
        a.act(Act::Quit);
        assert!(!a.quick, "`q` closed the app instead of the panel");
        assert!(!a.quit, "and it must not have quit on the way");

        a.act(Act::Quit);
        assert!(a.quit, "a second `q` still quits");
    }

    #[test]
    fn a_maximized_preview_closes_first_too() {
        let mut a = app();
        a.max_preview = true;
        a.act(Act::Quit);
        assert!(!a.max_preview && !a.quit);
        a.act(Act::Quit);
        assert!(a.quit);
    }

    /// With nothing in front, `q` is still `quit` on the first press.
    #[test]
    fn q_quits_when_no_panel_is_up() {
        let mut a = app();
        a.act(Act::Quit);
        assert!(a.quit);
    }

    /// `q` and `<Esc>` peel in the same order, because `q` routes through the
    /// same code. Two panels up means two presses, either key.
    #[test]
    fn q_and_escape_agree_on_the_order() {
        for quit_key in [true, false] {
            let mut a = app();
            a.quick = true;
            a.max_preview = true;
            let press = |a: &mut App| {
                if quit_key {
                    a.act(Act::Quit);
                } else {
                    a.act(Act::Escape(EscapeWhat::default()));
                }
            };
            press(&mut a);
            assert!(!a.quick && a.max_preview, "the panel in front goes first");
            press(&mut a);
            assert!(!a.max_preview);
            assert!(!a.quit, "neither key quits while something is still up");
        }
    }
}

/// `<A-j>` / `<A-k>` scroll what is being read, in every pane that reads.
#[cfg(test)]
mod alt_jk_scrolls_every_pane {
    use super::*;

    fn run_of(layer: &[keymap::Binding], key: &str) -> Vec<Act> {
        layer
            .iter()
            .find(|b| crate::config::keys::render_seq(&b.on) == key)
            .unwrap_or_else(|| panic!("`{key}` is not bound"))
            .run
            .clone()
    }

    /// The terminal was the one pane where they were not bound, so they went
    /// through to the shell and the pane looked like the odd one out. Pinned
    /// against the list's own pair, because the two drifting apart is the
    /// failure: same direction, same distance, different pane.
    #[test]
    fn the_terminal_scrolls_the_way_the_list_does() {
        let (km, warnings) = keymap::Keymap::load(&[]);
        assert!(warnings.is_empty(), "{warnings:?}");

        assert_eq!(run_of(&km.mgr, "<A-j>"), vec![Act::Seek(Step::Rel(5))]);
        assert_eq!(run_of(&km.mgr, "<A-k>"), vec![Act::Seek(Step::Rel(-5))]);
        assert_eq!(run_of(&km.term, "<A-j>"), vec![Act::TermScroll(Step::Rel(5))]);
        assert_eq!(run_of(&km.term, "<A-k>"), vec![Act::TermScroll(Step::Rel(-5))]);

        // `feed_term_key` only ever consults single-key bindings, so a chord
        // added here would be read by nothing and reach the shell instead.
        assert!(km.term.iter().all(|b| b.on.len() == 1));
    }

    /// The help panel moves by half its own height rather than five lines, so
    /// this asserts the direction only -- `j` down, `k` up, as everywhere else.
    #[test]
    fn the_help_panel_agrees_on_which_way_is_down() {
        let (km, _) = keymap::Keymap::load(&[]);
        let down = run_of(&km.help, "<A-j>");
        let up = run_of(&km.help, "<A-k>");
        let step = |acts: &[Act]| match acts.first() {
            Some(Act::Arrow(s)) => *s,
            other => panic!("expected an arrow, got {other:?}"),
        };
        assert!(matches!(step(&down), Step::Pct(n) if n > 0), "{down:?}");
        assert!(matches!(step(&up), Step::Pct(n) if n < 0), "{up:?}");
    }
}

/// Keys that used to finish without a word, which on screen is the same as a
/// key bound to nothing (#108).
#[cfg(test)]
mod said_out_loud {
    use super::*;

    fn app_in(dir: &Path) -> App {
        let mut a = App::new(Config::load(), dir.to_path_buf(), egui::Context::default());
        a.tabs[a.active].cwd = dir.to_path_buf();
        a.tabs[a.active].current = Folder::loading(dir.to_path_buf(), None);
        a
    }

    /// TESTING.md 10.10 — `c c` in an empty folder. The clipboard still holds
    /// the last path, so staying quiet lets the old one be pasted as this one.
    #[test]
    fn copying_from_an_empty_list_says_there_is_nothing() {
        let dir = crate::util::test_dir("said-copy");
        let mut a = app_in(&dir);
        a.act(Act::Copy(CopyWhat::Path));
        assert!(a.toasts.iter().any(|t| t.text.starts_with("Nothing to copy")), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// `d` names what went to the trash and how to get it back; `D` asks first
    /// and so needs no such line.
    #[test]
    fn a_finished_trash_says_what_went() {
        let dir = crate::util::test_dir("said-trash");
        let mut a = app_in(&dir);
        let gone = dir.join("a.txt");
        a.record_job(7, UndoStep::Trash { paths: vec![gone], dir: dir.clone() }, Land::Fresh);
        a.on_op_event(ops::OpEvent::Finished {
            id: 7,
            kind: OpKind::Trash,
            errors: Vec::new(),
            cancelled: false,
            moved: Vec::new(),
            linked: Vec::new(),
            junctions: Vec::new(),
            made: Vec::new(),
            trashed: vec![dir.join("a.txt")],
        });
        assert!(a.toasts.iter().any(|t| t.text == "Trashed a.txt — u to undo"), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// #238: a cut pasted into the folder it came from moves nothing and
    /// keeps the register, rather than renaming the file `same_1.txt`.
    #[test]
    fn a_cut_pasted_where_it_is_stays_put() {
        let dir = crate::util::test_dir("cut-here");
        let mut a = app_in(&dir);
        let src = dir.join("same.txt");
        a.yank = Yank { paths: vec![src.clone()], cut: true, from_archive: false };
        let before = a.tasks.len();
        a.paste(false, false);
        assert_eq!(a.tasks.len(), before, "nothing was queued");
        assert_eq!(a.yank.paths, vec![src]);
        assert!(a.toasts.iter().any(|t| t.text == "Already here — the cut is still there"), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// Q72: a cut pasted where every clash was declined moved nothing, so
    /// the register keeps it and says so; a paste that moved something
    /// leaves it empty, as before.
    #[test]
    fn a_paste_that_moved_nothing_keeps_the_cut() {
        let dir = crate::util::test_dir("cut-kept");
        let mut a = app_in(&dir);
        // Cut from another folder: one cut where it already is never runs.
        let src = dir.join("from").join("a.txt");
        let finished = |id, moved| ops::OpEvent::Finished {
            id,
            kind: OpKind::Move,
            errors: Vec::new(),
            cancelled: false,
            moved,
            linked: Vec::new(),
            junctions: Vec::new(),
            made: Vec::new(),
            trashed: Vec::new(),
        };
        a.yank = Yank { paths: vec![src.clone()], cut: true, from_archive: false };
        a.paste(false, false);
        assert!(a.yank.paths.is_empty(), "the register empties while the job runs");
        let id = a.tasks.last().unwrap().id;
        a.on_op_event(finished(id, Vec::new()));
        assert_eq!(a.yank.paths, vec![src.clone()]);
        assert!(a.yank.cut);
        assert!(a.toasts.iter().any(|t| t.text == "Nothing moved — the cut is still there"), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());

        a.paste(false, false);
        let id = a.tasks.last().unwrap().id;
        a.on_op_event(finished(id, vec![(src.clone(), dir.join("b/a.txt"))]));
        assert!(a.yank.paths.is_empty(), "a paste that moved something used the cut up");

        // Yanked something else meanwhile: that is what the register holds.
        a.yank = Yank { paths: vec![src.clone()], cut: true, from_archive: false };
        a.paste(false, false);
        let id = a.tasks.last().unwrap().id;
        let other = dir.join("other.txt");
        a.yank = Yank { paths: vec![other.clone()], cut: false, from_archive: false };
        a.on_op_event(finished(id, Vec::new()));
        assert_eq!(a.yank.paths, vec![other]);
    }

    /// #225: a rename says what it did and that `u` takes it back, as `d`
    /// and `-` do. It used to say nothing at all.
    #[test]
    fn a_rename_says_what_it_did() {
        let dir = crate::util::test_dir("said-rename");
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let mut a = app_in(&dir);
        a.do_rename(&dir.join("a.txt"), "b.txt");
        assert!(dir.join("b.txt").exists() && !dir.join("a.txt").exists());
        assert!(a.toasts.iter().any(|t| t.text == "Renamed to b.txt — u to undo"), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// #207: `d` failed and said to use `D`; once `D` has gone through, the
    /// red line about that file gives way to `Deleted <name>`. A `D` with no
    /// such line to answer says nothing, as before.
    #[test]
    fn a_delete_replaces_the_trash_error_it_answers() {
        let dir = crate::util::test_dir("said-delete");
        let mut a = app_in(&dir);
        let finished = |id| ops::OpEvent::Finished {
            id,
            kind: OpKind::Delete,
            errors: Vec::new(),
            cancelled: false,
            moved: Vec::new(),
            linked: Vec::new(),
            junctions: Vec::new(),
            made: Vec::new(),
            trashed: Vec::new(),
        };
        a.error("Trash: a.txt: the Recycle Bin can't take files from R: (…). Use D to delete permanently");
        a.error("Trash: b.txt: it is open in another program");
        let id = a.submit_op_to(OpKind::Delete, vec![dir.join("a.txt")], dir.clone(), None, false);
        a.on_op_event(finished(id));
        let said: Vec<&str> = a.toasts.iter().map(|t| t.text.as_str()).collect();
        assert!(!said.iter().any(|t| t.starts_with("Trash: a.txt")), "the answered line is gone: {said:?}");
        assert!(said.contains(&"Trash: b.txt: it is open in another program"), "another file's stays: {said:?}");
        assert!(said.contains(&"Deleted a.txt"), "{said:?}");

        let quiet = a.toasts.len();
        let id = a.submit_op_to(OpKind::Delete, vec![dir.join("c.txt")], dir.clone(), None, false);
        a.on_op_event(finished(id));
        assert_eq!(a.toasts.len(), quiet, "nothing to answer, nothing said");
    }

    /// #205: a finished pack names its archive, an unpack the folder the
    /// contents are in -- several of them by the first and a count.
    #[test]
    fn a_pack_and_an_unpack_say_what_they_made() {
        let sep = std::path::MAIN_SEPARATOR;
        let zip = OpKind::Compress(crate::fs::archive::Format::Zip);
        assert_eq!(made_says(zip, &[PathBuf::from("x/out.zip")]).as_deref(), Some("Packed into out.zip"));
        assert_eq!(made_says(OpKind::Extract, &[PathBuf::from("x/out")]), Some(format!("Unpacked into out{sep}")));
        assert_eq!(
            made_says(OpKind::Extract, &[PathBuf::from("x/a"), PathBuf::from("x/b"), PathBuf::from("x/c")]),
            Some(format!("Unpacked 3 archives into a{sep} and 2 more")),
        );
        assert_eq!(made_says(OpKind::Extract, &[]), None, "nothing unpacked, nothing said");
        assert_eq!(made_says(OpKind::Copy, &[PathBuf::from("x/a")]), None);
    }

    /// #208: a drop onto a list showing the same folder says why it did
    /// nothing, in `<A-c>`'s words, and queues no job.
    #[test]
    fn a_drop_into_the_same_folder_says_so() {
        let dir = crate::util::test_dir("said-drop");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let mut a = app_in(&dir);
        let mut other = crate::core::tab::Tab::new(dir.clone(), a.tabs[0].sort, false, crate::fs::entry::Linemode::None);
        other.cwd = dir.clone();
        a.tabs.push(other);
        a.drag = Some(Drag { from: 0, paths: vec![dir.join("a.txt")], label: "a.txt".into() });
        a.drop_drag(Some(1), false);
        assert!(a.tasks.is_empty(), "no job");
        assert!(a.toasts.iter().any(|t| t.text == "Both panes are in the same directory"), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// #163: `<C-F5>` with the help panel open re-reads the config, as the
    /// panel's own rows say it does, and the panel stays open to show it.
    #[test]
    fn the_reload_key_works_with_help_open() {
        let dir = crate::util::test_dir("help-reload");
        let mut a = app_in(&dir);
        a.overlay = Overlay::Help;
        a.feed_overlay_key(Key::parse("<C-F5>").unwrap());
        assert!(matches!(a.overlay, Overlay::Help), "still open");
        assert!(a.toasts.iter().any(|t| t.text.starts_with("Reloaded")), "{:?}",
            a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
    }

    /// Q62 / Q64: `<F12>` shows what the report carries before anything is
    /// opened -- the keys before it, the last error, the config by name --
    /// with opening first, so `<Enter>` opens and `<Esc>` drops it.
    #[test]
    fn f12_shows_the_report_before_opening_it() {
        let dir = crate::util::test_dir("f12-panel");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let mut a = app_in(&dir);
        a.error("Copy: a.txt: denied");
        for k in ["j", "k", "<F12>"] {
            a.feed_key(Key::parse(k).unwrap());
        }
        let Overlay::Confirm(c) = &a.overlay else { panic!("no panel: {:?}", a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>()) };
        let ConfirmAction::BugReport { url } = &c.action else { panic!("not the report") };
        assert_eq!(c.options.first().map(|o| o.0), Some('o'), "<Enter> opens");
        assert!(c.body.iter().any(|l| l == "Last keys: j k"), "the keys before <F12>, not <F12>: {:?}", c.body);
        assert!(c.body.iter().any(|l| l == "Last error: Copy: a.txt: denied"), "{:?}", c.body);
        assert!(url.contains("&keys=Last%20keys%2C%20oldest%20first%3A%20j%20k"), "{url}");
        assert!(url.contains("&context=Last%20error%3A%20Copy%3A%20a.txt%3A%20denied"), "{url}");
        let url = url.clone();

        // `c` copies the link and leaves it where the state file can read it.
        a.answer_confirm('c');
        assert!(matches!(a.overlay, Overlay::None));
        assert_eq!(a.last_report.as_deref(), Some(url.as_str()));
    }

    /// #83: a trash where four of five went leaves a step for the four, so
    /// `u` brings them back rather than saying there is nothing to undo.
    #[test]
    fn a_partly_failed_trash_can_still_be_undone() {
        let dir = crate::util::test_dir("said-partial-trash");
        let mut a = app_in(&dir);
        let (gone, stuck) = (dir.join("a.txt"), dir.join("locked.txt"));
        a.record_job(9, UndoStep::Trash { paths: vec![gone.clone(), stuck], dir: dir.clone() }, Land::Fresh);
        a.on_op_event(ops::OpEvent::Finished {
            id: 9,
            kind: OpKind::Trash,
            errors: vec!["locked.txt: in use".into()],
            cancelled: false,
            moved: Vec::new(),
            linked: Vec::new(),
            junctions: Vec::new(),
            made: Vec::new(),
            trashed: vec![gone.clone()],
        });
        match a.undos.undo.last() {
            Some(UndoStep::Trash { paths, .. }) => assert_eq!(paths, &vec![gone], "only what went"),
            other => panic!("no trash step to undo: {other:?}"),
        }
    }

    /// TESTING.md 15.8 — a held zoom key stops at the ends, and the toast says
    /// it has stopped.
    #[test]
    fn the_scale_says_when_it_hits_an_end() {
        use crate::config::cmd::ScaleTo;
        let dir = crate::util::test_dir("said-scale");
        let mut a = app_in(&dir);
        a.act(Act::Scale(ScaleTo::In));
        assert!(a.toasts.iter().any(|t| t.text == "Scale 110%"));
        for _ in 0..60 {
            a.act(Act::Scale(ScaleTo::In));
        }
        assert!(a.toasts.iter().any(|t| t.text == "Scale 500% (maximum)"));
        for _ in 0..60 {
            a.act(Act::Scale(ScaleTo::Out));
        }
        assert!(a.toasts.iter().any(|t| t.text == "Scale 20% (minimum)"));
        a.act(Act::Scale(ScaleTo::Reset));
        assert!(a.toasts.iter().any(|t| t.text == "Scale 100%"));
        // One at a time: each step replaces the last rather than stacking.
        assert_eq!(a.toasts.iter().filter(|t| t.text.starts_with("Scale ")).count(), 1);
    }

    /// #107: one failed jump, one toast. The parent columns it asked for fail
    /// for the same reason, before and after the jump itself is taken back,
    /// and each used to add a toast naming only a fragment of the path.
    #[test]
    fn a_failed_jump_says_so_once() {
        let dir = crate::util::test_dir("said-cd");
        let mut a = app_in(&dir);
        let mid = dir.join("no").join("such");
        let bad = mid.join("place");
        a.cd(bad.clone(), true);
        let fail = |path: &Path| ScanResult::Failed { id: 0, path: path.to_path_buf(), error: "os error 123".into() };
        a.toasts.clear();
        a.on_scan(fail(&mid));
        a.on_scan(fail(&bad));
        a.on_scan(fail(&dir.join("no")));
        let said: Vec<&String> = a.toasts.iter().map(|t| &t.text).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("place"), "the whole path is named: {said:?}");
        assert_eq!(a.tabs[a.active].cwd, dir, "taken back");

        // A failure that has nothing to do with the jump is still heard.
        a.on_scan(fail(&std::env::temp_dir().join("elsewhere")));
        assert_eq!(a.toasts.len(), 2);
    }

    /// TESTING.md 23.6 — a typed path that names nothing falls back to its
    /// parent as before, and now says so; one naming a file stays quiet.
    #[test]
    fn a_path_that_names_nothing_says_where_it_landed() {
        let dir = crate::util::test_dir("said-reveal");
        std::fs::write(dir.join("here.txt"), "x").unwrap();
        let listing = || Arc::new(vec![Entry::from_path(dir.join("here.txt")).unwrap()]);

        let mut a = app_in(&dir.join("elsewhere"));
        a.cd_or_reveal(dir.join("here.txt"));
        a.on_scan(ScanResult::Failed { id: 0, path: dir.join("here.txt"), error: "not a dir".into() });
        a.apply_listing(&dir, listing());
        assert!(a.toasts.is_empty(), "the file is under the cursor: {:?}", a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
        assert_eq!(a.tabs[a.active].current.hovered_name(), Some("here.txt"));

        let mut a = app_in(&dir.join("elsewhere"));
        a.cd_or_reveal(dir.join("tpyo"));
        a.on_scan(ScanResult::Failed { id: 0, path: dir.join("tpyo"), error: "not found".into() });
        a.apply_listing(&dir, listing());
        let said: Vec<&String> = a.toasts.iter().map(|t| &t.text).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("No such file or folder: tpyo"), "{said:?}");
    }

    /// The `cd` prompt starts with the folder and this platform's separator,
    /// and a root gets no second one.
    #[test]
    fn the_cd_prompt_ends_in_this_platforms_separator() {
        let sep = std::path::MAIN_SEPARATOR;
        let dir = std::env::temp_dir().join("x");
        assert_eq!(dir_prefill(&dir), format!("{}{sep}", dir.display()));
        let root = PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" });
        assert_eq!(dir_prefill(&root), root.display().to_string());
        assert!(!dir_prefill(&dir).ends_with(if cfg!(windows) { '/' } else { '\\' }));
    }

    /// #242: the same, when the parent's listing has already arrived -- the
    /// scan that fills the parent column often answers before the jump's own
    /// fails. The name was then never checked, and the tab landed on the
    /// parent in silence about one run in two.
    #[test]
    fn a_path_that_names_nothing_says_so_whichever_listing_lands_first() {
        let dir = crate::util::test_dir("said-reveal-race");
        std::fs::write(dir.join("here.txt"), "x").unwrap();
        let listing = || vec![Entry::from_path(dir.join("here.txt")).unwrap()];

        let mut a = app_in(&dir.join("elsewhere"));
        a.cd_or_reveal(dir.join("tpyo"));
        a.on_scan(ScanResult::Listed { id: 0, path: dir.clone(), entries: listing() });
        a.on_scan(ScanResult::Failed { id: 0, path: dir.join("tpyo"), error: "not found".into() });
        let said: Vec<&String> = a.toasts.iter().map(|t| &t.text).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("No such file or folder: tpyo"), "{said:?}");
        assert_eq!(a.tabs[a.active].cwd, dir, "on the parent");

        // A file there is still revealed without a word, in this order too.
        let mut a = app_in(&dir.join("elsewhere"));
        a.cd_or_reveal(dir.join("here.txt"));
        a.on_scan(ScanResult::Listed { id: 0, path: dir.clone(), entries: listing() });
        a.on_scan(ScanResult::Failed { id: 0, path: dir.join("here.txt"), error: "not a dir".into() });
        assert!(a.toasts.is_empty(), "{:?}", a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());
        assert_eq!(a.tabs[a.active].current.hovered_name(), Some("here.txt"));
    }

    /// #105: `<Esc>` while a jump is still waiting takes the tab
    /// back, and the late failure of the abandoned scan says nothing.
    #[test]
    fn escape_gives_up_on_a_jump_still_waiting() {
        let dir = crate::util::test_dir("said-abandon");
        let mut a = app_in(&dir);
        let far = dir.join("slow");
        a.cd(far.clone(), true);
        assert!(a.tabs[a.active].pending_cd.is_some(), "waiting");
        a.act(Act::Escape(EscapeWhat::default()));
        assert_eq!(a.tabs[a.active].cwd, dir, "back where it came from");
        assert!(a.tabs[a.active].back.is_empty(), "and the jump is not in the history");
        assert!(a.toasts.iter().any(|t| t.text.starts_with("Stopped waiting for")));
        a.toasts.clear();
        a.on_scan(ScanResult::Failed { id: 0, path: far, error: "timed out".into() });
        assert!(a.toasts.is_empty(), "{:?}", a.toasts.iter().map(|t| &t.text).collect::<Vec<_>>());

        // With nothing waiting, `<Esc>` is what it was.
        a.act(Act::Escape(EscapeWhat::default()));
        assert_eq!(a.tabs[a.active].cwd, dir);
    }

    /// The same through a typed path, which first falls back to the parent in
    /// case it named a file: the parent then fails and is the one taken back.
    #[test]
    fn a_failed_typed_jump_says_so_once() {
        let dir = crate::util::test_dir("said-cd-typed");
        let mut a = app_in(&dir);
        let mid = dir.join("no").join("such");
        let bad = mid.join("place");
        a.cd_or_reveal(bad.clone());
        let fail = |path: &Path| ScanResult::Failed { id: 0, path: path.to_path_buf(), error: "os error 123".into() };
        a.toasts.clear();
        a.on_scan(fail(&mid));
        a.on_scan(fail(&bad));
        a.on_scan(fail(&mid));
        a.on_scan(fail(&dir.join("no")));
        let said: Vec<&String> = a.toasts.iter().map(|t| &t.text).collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert_eq!(a.tabs[a.active].cwd, dir, "taken back");
    }
}
