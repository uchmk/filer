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

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
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
