//! What the last run actually used, written down so `filer env` can say it.
//!
//! Two of the questions a report needs answered are only knowable with a
//! window open. Which GPU adapter and backend egui ended up on is decided by
//! wgpu at startup, and which font files were loaded is decided by a search
//! whose result depends on what is installed. `filer env` exits before any of
//! that happens — it has no window and deliberately costs nothing — so asking
//! it to work them out would mean initialising wgpu in a diagnostic, which is
//! both slow and the thing most likely to be broken on the machine being
//! diagnosed.
//!
//! So the run that does know writes it down, and the diagnostic reads it back.
//! It reports the *last* run rather than this one, which is exactly right: the
//! run being reported on is the one that went wrong, not the one typing
//! `filer env` afterwards.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// `Eq` is gone from this derive because `ppp` is an `f32`. Nothing asks for it:
// the struct is written to a file and read back, never hashed or put in a set,
// and `assert_eq!` only ever wanted `PartialEq`.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct RunInfo {
    /// The version that wrote this, so a stale file cannot be read as current.
    pub version: String,
    /// The adapter's own name, e.g. `NVIDIA GeForce RTX 4070`.
    pub adapter: String,
    /// `Vulkan`, `Dx12`, `Gl`, …
    pub backend: String,
    /// `DiscreteGpu`, `IntegratedGpu`, `Cpu`, … — a software fallback is a
    /// finding all by itself when the complaint is that it is slow.
    pub device: String,
    /// The regular faces that were loaded, in the order they are consulted.
    pub fonts: Vec<PathBuf>,
    /// The bold faces, which are a separate search and separately absent.
    pub bold: Vec<PathBuf>,
    /// The window as egui laid it out, in points, and what one point came out
    /// as in pixels. `[0.0, 0.0]` and `0.0` mean no frame has been drawn yet.
    ///
    /// Written down for the same reason as the adapter: it cannot be known
    /// without a window. But unlike the adapter, it is a question people *do*
    /// try to answer from outside -- and that is where it goes wrong. What
    /// `GetClientRect` reports for a window depends on the DPI awareness of
    /// the process doing the asking, so a measurement taken from a script and
    /// the window's own view can disagree while both look authoritative. A
    /// screen capture is worse: `PrintWindow` can hand back a bitmap in
    /// logical coordinates, and then every length measured off it is wrong by
    /// the scale factor in a way that stays self-consistent.
    ///
    /// On 2026-09-29 that cost a whole section of TESTING.md: a run reported
    /// that filer was laying out a window half again too large for itself, and
    /// nothing available could tell whether the window or the ruler was at
    /// fault. These two fields are filer's own answer, which no amount of DPI
    /// virtualisation can distort.
    pub window_pt: [f32; 2],
    /// Pixels per point, as egui had it for the frame this was written on.
    pub ppp: f32,
    /// The terminal pane's grid as `[lines, columns]`, the last time the run
    /// drew it; `[0, 0]` if the pane was never opened. Section 1 of
    /// TESTING.md makes claims about this number that only the shell could
    /// answer (#107). Defaulted, so a record from before it still loads.
    #[serde(default)]
    pub pane: [usize; 2],
    /// The last command lines `<Enter>` or `:` / `!` ran, newest last, at
    /// most [`LAUNCHES_KEPT`]. A launch was a toast for a few seconds and then
    /// nothing; the opener bug in the README was found only because a
    /// screenshot happened to catch one (Q40).
    #[serde(default)]
    pub launched: Vec<String>,
    /// When the run that wrote this started, in seconds since 1970. With
    /// the same version a record from days ago looked current, and a run
    /// measuring four starts kept four state folders to tell them apart
    /// (#244). Defaulted, so a record from before it still loads.
    #[serde(default)]
    pub started: u64,
    /// The shell the pane started in that run, as its first toast named it
    /// (`pwsh`, `powershell (Windows PowerShell 5.1)`); empty when the pane
    /// was not opened. Runs read it from `Win32_Process` before (#184).
    #[serde(default)]
    pub pane_shell: String,
    /// Why the pane's shell did not start, when it did not; empty otherwise.
    /// `filer env` could only say `not opened in that run`, the same as a
    /// pane nobody asked for (#254).
    #[serde(default)]
    pub pane_failed: String,
    /// What asked for the backend in that run: `[ui] backend = "gl"`, or
    /// `WGPU_BACKEND=vulkan` when the variable won. `filer env` could not tell
    /// whether the adapter it lists came from the setting as it is now or as it
    /// was before an edit (#245). Defaulted, so a record from before it loads.
    #[serde(default)]
    pub backend_setting: String,
}

impl RunInfo {
    /// `2026-10-04 08:59 (12m00s ago)`, or nothing for a record from before
    /// [`RunInfo::started`] existed.
    pub fn started_line(&self, now: std::time::SystemTime) -> Option<String> {
        if self.started == 0 {
            return None;
        }
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(self.started);
        let ago = now.duration_since(at).unwrap_or_default();
        Some(format!("{} ({} ago)", crate::util::fmt_time(Some(at), "%Y-%m-%d %H:%M"), crate::util::fmt_duration(ago)))
    }
}

/// How many launches `filer env` shows.
pub const LAUNCHES_KEPT: usize = 5;

/// Add `line` to `list`, dropping the oldest past [`LAUNCHES_KEPT`].
fn push_launch(list: &mut Vec<String>, line: &str) {
    list.push(line.to_owned());
    let over = list.len().saturating_sub(LAUNCHES_KEPT);
    list.drain(..over);
}

/// Record a launch in `last-run.toml`, off the UI thread.
/// Record the shell the pane just started.
pub fn remember_shell(label: &str) {
    if cfg!(test) {
        return;
    }
    let label = label.to_owned();
    std::thread::spawn(move || {
        let mut info = load().unwrap_or_default();
        info.pane_shell = label;
        info.pane_failed.clear();
        save(&info);
    });
}

/// Record why the pane's shell did not start.
pub fn remember_shell_failed(why: &str) {
    if cfg!(test) {
        return;
    }
    let why = why.to_owned();
    std::thread::spawn(move || {
        let mut info = load().unwrap_or_default();
        info.pane_failed = why;
        save(&info);
    });
}

pub fn remember_launch(line: &str) {
    // A test launching something must not write into the real state folder.
    if cfg!(test) {
        return;
    }
    let line = line.to_owned();
    std::thread::spawn(move || {
        let mut info = load().unwrap_or_default();
        push_launch(&mut info.launched, &line);
        save(&info);
    });
}

impl RunInfo {
    /// The window as one line for a bug report: pixels first, because that is
    /// what a person measures, with the points and the scale that produced it.
    ///
    /// `None` before any frame has been drawn, which `filer env` prints as its
    /// own sentence rather than as a row of zeroes.
    pub fn window_line(&self) -> Option<String> {
        let [w, h] = self.window_pt;
        if self.ppp <= 0.0 || w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(format!(
            "{:.0} x {:.0} px ({:.0} x {:.0} pt @ {})",
            w * self.ppp,
            h * self.ppp,
            w,
            h,
            // `1.5`, not `1.5000001`: the scale is a setting, not a measurement.
            (self.ppp * 1000.0).round() / 1000.0,
        ))
    }
}

fn path() -> PathBuf {
    crate::config::Config::state_dir().join("last-run.toml")
}

/// Write it, best effort: a diagnostic that could stop a program from starting
/// would be worse than no diagnostic.
pub fn save(info: &RunInfo) {
    save_to(&path(), info);
}

/// What the last run wrote, or nothing when filer has not run on this machine
/// — which is itself worth printing rather than papering over.
pub fn load() -> Option<RunInfo> {
    load_from(&path())
}

// The pair below takes the path so the test can use one of its own. Going
// through `FILER_STATE_HOME` instead would move the state directory for every
// other test running beside it, several of which build an `App` and read it.
fn save_to(p: &std::path::Path, info: &RunInfo) {
    if let Some(dir) = p.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    if let Ok(text) = toml::to_string_pretty(info) {
        let _ = std::fs::write(p, text);
    }
}

fn load_from(p: &std::path::Path) -> Option<RunInfo> {
    toml::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window line reads as pixels first, because that is what a person
    /// with a ruler has, and carries the points and the scale that made them.
    ///
    /// The case that matters is the one the row exists for: 1360 x 860 points
    /// at 1.5 needs 2040 x 1290 pixels. Anyone reading the row can check the
    /// arithmetic against the window they are looking at, which is the whole
    /// point of printing all three numbers rather than the one filer used.
    /// #244: the record says when its run started, and how long ago; one
    /// from before the field reads as not recorded.
    #[test]
    fn the_record_says_when_its_run_started() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
        let info = RunInfo { started: 1_800_000_000, ..Default::default() };
        let line = info.started_line(at + std::time::Duration::from_secs(750)).unwrap();
        assert!(line.ends_with(" (12m30s ago)"), "{line}");
        assert_eq!(line.len(), "2027-01-15 08:00".len() + " (12m30s ago)".len(), "{line}");
        assert_eq!(RunInfo::default().started_line(at), None);
        let old: RunInfo = toml::from_str("version = \"0.74.0\"\nadapter = \"\"\nbackend = \"\"\ndevice = \"\"\nfonts = []\nbold = []\nwindow_pt = [0.0, 0.0]\nppp = 0.0\n").expect("an older record still loads");
        assert_eq!(old.started, 0);
    }

    #[test]
    fn the_window_line_gives_pixels_points_and_the_scale_between_them() {
        let at = |w: f32, h: f32, ppp: f32| {
            RunInfo { window_pt: [w, h], ppp, ..Default::default() }.window_line()
        };
        assert_eq!(at(1360.0, 860.0, 1.5).as_deref(), Some("2040 x 1290 px (1360 x 860 pt @ 1.5)"));
        assert_eq!(at(1360.0, 860.0, 1.0).as_deref(), Some("1360 x 860 px (1360 x 860 pt @ 1)"));
        // A scale that is not a round number still reads as one value, not as
        // whatever float arithmetic left behind.
        assert_eq!(at(1000.0, 500.0, 1.25).as_deref(), Some("1250 x 625 px (1000 x 500 pt @ 1.25)"));
    }

    /// Q40: the last five launches are kept, newest last.
    #[test]
    fn the_last_five_launches_are_kept() {
        let mut list = Vec::new();
        for i in 1..=7 {
            push_launch(&mut list, &format!("run {i}"));
        }
        assert_eq!(list, ["run 3", "run 4", "run 5", "run 6", "run 7"]);
    }

    /// A record written before `pane` existed still loads, with no pane.
    #[test]
    fn an_older_record_still_loads() {
        let old = "version = \"0.58.0\"\nadapter = \"\"\nbackend = \"\"\ndevice = \"\"\nfonts = []\nbold = []\nwindow_pt = [1.0, 1.0]\nppp = 1.0\n";
        let info: RunInfo = toml::from_str(old).expect("loads");
        assert_eq!(info.pane, [0, 0]);
    }

    /// Before the first frame there is no window, and the row says so rather
    /// than printing zeroes that read as a measurement.
    #[test]
    fn a_record_from_before_the_first_frame_has_no_window_line() {
        assert_eq!(RunInfo::default().window_line(), None, "nothing drawn yet");
        // A record written by a filer too old to know about the field also
        // arrives with zeroes, and must not be read as `0 x 0 px`.
        let old = RunInfo { adapter: "some GPU".into(), ..Default::default() };
        assert_eq!(old.window_line(), None, "an older filer left the field empty");
    }

    /// It survives the round trip, and an absent one is absent rather than
    /// empty: "filer has never opened a window here" and "it opened one and
    /// found no fonts" are different answers and must not look alike.
    #[test]
    fn it_is_written_and_read_back() {
        let dir = crate::util::test_dir("runinfo");
        let p = dir.join("last-run.toml");

        assert_eq!(load_from(&p), None, "nothing written yet");

        let info = RunInfo {
            version: "9.9.9".into(),
            adapter: "NVIDIA GeForce RTX 4070".into(),
            backend: "Vulkan".into(),
            device: "DiscreteGpu".into(),
            fonts: vec![PathBuf::from(r"C:\fonts\HackGen35ConsoleNF-Regular.ttf")],
            bold: Vec::new(),
            window_pt: [1360.0, 860.0],
            ppp: 1.5,
            pane: [12, 159],
            launched: vec!["code -g a.txt:3".into()],
            started: 1_800_000_000,
            pane_shell: "powershell (Windows PowerShell 5.1)".into(),
            pane_failed: "`nosuch` was not found on PATH".into(),
            backend_setting: "[ui] backend = \"gl\"".into(),
        };
        save_to(&p, &info);
        assert_eq!(load_from(&p).as_ref(), Some(&info));

        // A backslashed Windows path has to come back as it went in; TOML
        // would otherwise read `\f` and friends as escapes.
        let back = load_from(&p).unwrap();
        assert_eq!(back.fonts[0], info.fonts[0]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
