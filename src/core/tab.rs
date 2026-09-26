use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::fs::SortSpec;

use super::folder::Folder;

#[derive(Clone, Debug)]
pub struct VisualState {
    pub anchor: usize,
    /// `visual_mode --unset` removes from the selection instead of adding.
    pub unset: bool,
    /// Selection as it was when visual mode started.
    pub base: BTreeSet<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct Finder {
    pub query: String,
    pub case_sensitive: bool,
    pub prev: bool,
}

/// A jump whose directory has not been listed yet. Nothing on disk is touched
/// before a jump — a share that stopped answering can sit on `is_dir` for half
/// a minute — so the background scan is what confirms the tab may stay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCd {
    /// Where the tab sat before the jump, and where it returns if the scan fails.
    pub from: PathBuf,
    /// The jump pushed `from` onto `back`, so undoing it pops that entry.
    pub pushed: bool,
    /// Try the parent once before giving up. Set for a path someone typed,
    /// which may well name a file rather than a directory.
    pub fallback: bool,
}

/// What a failed listing does to the tab that asked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CdFallout {
    /// The tab had listed this directory before; leave it on screen with the error.
    Keep,
    /// Show `to` instead; the name that failed is already in `memo`, so the
    /// cursor lands on it whenever the listing arrives.
    Reveal { to: PathBuf, pending: PendingCd },
    /// Put the tab back where it started and report the error.
    Revert { to: PathBuf },
}

pub struct Tab {
    pub cwd: PathBuf,
    pub current: Folder,
    pub parent: Option<Folder>,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    /// Which file the cursor was on, per directory.
    pub memo: HashMap<PathBuf, String>,
    pub selected: BTreeSet<PathBuf>,
    pub visual: Option<VisualState>,
    /// Anchor a Shift+click range grows from, with the selection as it was
    /// before that range. Mouse ranges and visual mode share `selected`.
    pub mouse_range: Option<VisualState>,
    pub finder: Option<Finder>,
    pub sort: SortSpec,
    pub show_hidden: bool,
    pub linemode: String,
    pub preview_offset: usize,
    /// Rows that fit in the list, measured by the renderer each frame.
    pub page_rows: usize,
    /// The jump this tab is still waiting on, if any.
    pub pending_cd: Option<PendingCd>,
}

impl Tab {
    pub fn new(cwd: PathBuf, sort: SortSpec, show_hidden: bool, linemode: String) -> Self {
        Self {
            current: Folder::loading(cwd.clone(), None),
            parent: crate::util::parent_dir(&cwd).map(|p| Folder::loading(p, None)),
            cwd,
            back: Vec::new(),
            forward: Vec::new(),
            memo: HashMap::new(),
            selected: BTreeSet::new(),
            visual: None,
            mouse_range: None,
            finder: None,
            sort,
            show_hidden,
            linemode,
            preview_offset: 0,
            page_rows: 20,
            pending_cd: None,
        }
    }

    /// Decide what a failed listing of `cwd` means. The tab answers once: the
    /// pending jump is spent either way, so a second failure cannot loop.
    pub fn cd_failed(&mut self) -> CdFallout {
        let Some(p) = self.pending_cd.take() else {
            return CdFallout::Keep;
        };
        // The fallback reads a failure as "that was a file, not a directory",
        // and quietly shows the parent instead — which is right for
        // `C:\dir\file.txt`, and wrong for anything that failed for a reason
        // worth hearing, because reverting is the only branch that says one.
        // `\\host` is never a file: its parent is the bare root `\`, so the
        // fallback drops the reader somewhere they did not ask for and tells
        // them nothing about why.
        if p.fallback && !crate::util::host_only_unc(&self.cwd) {
            if let Some(to) = self.cwd.parent().map(Path::to_path_buf) {
                // `cd C:\dir\file.txt` means "show me that file".
                let name = crate::util::file_name(&self.cwd);
                self.memo.insert(to.clone(), name);
                return CdFallout::Reveal {
                    to,
                    pending: PendingCd {
                        fallback: false,
                        ..p
                    },
                };
            }
        }
        if p.pushed {
            self.back.pop();
        }
        CdFallout::Revert { to: p.from }
    }

    pub fn name(&self) -> String {
        crate::util::file_name(&self.cwd)
    }

    pub fn remember_cursor(&mut self) {
        if let Some(name) = self.current.hovered_name() {
            self.memo.insert(self.cwd.clone(), name.to_owned());
        }
    }

    pub fn recall_cursor(&mut self) {
        if let Some(name) = self.memo.get(&self.cwd).cloned() {
            self.current.select_name(&name);
        }
    }

    /// Files an action applies to: the selection if there is one, else the
    /// hovered file. This is yazi's rule.
    pub fn targets(&self) -> Vec<PathBuf> {
        if !self.selected.is_empty() {
            return self.selected.iter().cloned().collect();
        }
        self.current
            .hovered()
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    pub fn toggle(&mut self, state: Option<bool>) {
        let Some(e) = self.current.hovered() else {
            return;
        };
        let path = e.path.clone();
        let on = state.unwrap_or(!self.selected.contains(&path));
        if on {
            self.selected.insert(path);
        } else {
            self.selected.remove(&path);
        }
    }

    pub fn toggle_all(&mut self, state: Option<bool>) {
        let paths: Vec<PathBuf> = self
            .current
            .view
            .iter()
            .filter_map(|&i| self.current.entries.get(i as usize))
            .map(|e| e.path.clone())
            .collect();
        match state {
            Some(true) => self.selected.extend(paths),
            Some(false) => {
                for p in paths {
                    self.selected.remove(&p);
                }
            }
            None => {
                for p in paths {
                    if !self.selected.remove(&p) {
                        self.selected.insert(p);
                    }
                }
            }
        }
    }

    pub fn enter_visual(&mut self, unset: bool) {
        self.visual = Some(VisualState {
            anchor: self.current.cursor,
            unset,
            base: self.selected.clone(),
        });
        self.sync_visual();
    }

    pub fn leave_visual(&mut self) -> bool {
        self.visual.take().is_some()
    }

    /// Recompute the selection from the visual anchor to the cursor. Called
    /// after every keyboard cursor move, which also drops the mouse anchor so
    /// the next Shift+click grows from where the cursor ended up.
    pub fn sync_visual(&mut self) {
        self.mouse_range = None;
        let Some(v) = self.visual.clone() else { return };
        self.selected = self.range_from(&v, self.current.cursor);
    }

    /// The selection `v` describes once its range reaches `to`.
    fn range_from(&self, v: &VisualState, to: usize) -> BTreeSet<PathBuf> {
        let (lo, hi) = if v.anchor <= to {
            (v.anchor, to)
        } else {
            (to, v.anchor)
        };
        let mut next = v.base.clone();
        for row in lo..=hi {
            if let Some(e) = self.current.at(row) {
                if v.unset {
                    next.remove(&e.path);
                } else {
                    next.insert(e.path.clone());
                }
            }
        }
        next
    }

    /// A plain click: move the cursor and re-anchor the mouse range there.
    pub fn click(&mut self, row: usize) {
        self.current.cursor = row;
        self.sync_visual();
    }

    /// Right-click: point the menu at something unambiguous. A row inside the
    /// selection keeps it, so the menu acts on all of it; a row outside drops
    /// it and the menu acts on that row alone.
    pub fn right_click(&mut self, row: usize) {
        self.leave_visual();
        self.mouse_range = None;
        let inside = self
            .current
            .at(row)
            .map(|e| self.selected.contains(&e.path))
            .unwrap_or(false);
        if !inside {
            self.selected.clear();
        }
        self.current.cursor = row;
    }

    /// Ctrl+click (Cmd on macOS): flip one row, leaving the rest alone. The
    /// row becomes the anchor a following Shift+click grows from.
    pub fn ctrl_click(&mut self, row: usize) {
        self.leave_visual();
        self.current.cursor = row;
        self.mouse_range = None;
        self.toggle(None);
    }

    /// Shift+click: select from the anchor to `row`. Repeated Shift+clicks
    /// rewrite the range instead of piling up, so the anchor stays put.
    pub fn shift_click(&mut self, row: usize) {
        self.leave_visual();
        let v = match self.mouse_range.clone() {
            Some(v) => v,
            None => {
                let v = VisualState {
                    anchor: self.current.cursor,
                    unset: false,
                    base: self.selected.clone(),
                };
                self.mouse_range = Some(v.clone());
                v
            }
        };
        self.current.cursor = row;
        self.selected = self.range_from(&v, row);
    }

    pub fn clear_selection(&mut self) -> bool {
        let had = !self.selected.is_empty();
        self.selected.clear();
        self.mouse_range = None;
        had
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::cmd::Step;
    use crate::fs::{Entry, Kind};

    /// A tab over `n` files named `a`, `b`, ... with the cursor on row 0.
    fn tab(n: usize) -> Tab {
        let entries: Vec<Entry> = (0..n)
            .map(|i| {
                let name = ((b'a' + i as u8) as char).to_string();
                Entry {
                    path: PathBuf::from(format!("/t/{name}")),
                    name,
                    ext: None,
                    kind: Kind::File,
                    len: 0,
                    modified: None,
                    created: None,
                    accessed: None,
                    hidden: false,
                    readonly: false,
                    link_to: None,
                    dir_size: None,
                }
            })
            .collect();
        let mut t = Tab::new(
            PathBuf::from("/t"),
            SortSpec::default(),
            true,
            String::new(),
        );
        t.current = Folder::from_entries(PathBuf::from("/t"), Arc::new(entries), true);
        t
    }

    /// The selection as the names it holds, in order.
    fn names(t: &Tab) -> Vec<String> {
        t.selected
            .iter()
            .map(|p| crate::util::file_name(p))
            .collect()
    }

    /// The tab as `cd` leaves it: parked on `to`, waiting for its listing.
    fn jumped(from: &str, to: &str, fallback: bool) -> Tab {
        let mut t = Tab::new(PathBuf::from(to), SortSpec::default(), true, String::new());
        t.back.push(PathBuf::from(from));
        t.pending_cd = Some(PendingCd {
            from: PathBuf::from(from),
            pushed: true,
            fallback,
        });
        t
    }

    #[test]
    fn a_jump_that_never_listed_is_undone() {
        let mut t = jumped("/a", "//dead/share", false);
        assert_eq!(
            t.cd_failed(),
            CdFallout::Revert {
                to: PathBuf::from("/a")
            }
        );
        // The history entry the jump pushed goes with it.
        assert!(t.back.is_empty());
        assert!(t.pending_cd.is_none());
    }

    #[test]
    fn a_directory_already_listed_keeps_its_error() {
        let mut t = Tab::new(
            PathBuf::from("/a"),
            SortSpec::default(),
            true,
            String::new(),
        );
        assert_eq!(t.cd_failed(), CdFallout::Keep);
    }

    #[test]
    fn a_typed_path_that_names_a_file_falls_back_to_the_parent() {
        let mut t = jumped("/a", "/b/note.txt", true);
        let fallout = t.cd_failed();
        assert_eq!(
            fallout,
            CdFallout::Reveal {
                to: PathBuf::from("/b"),
                pending: PendingCd {
                    from: PathBuf::from("/a"),
                    pushed: true,
                    fallback: false
                },
            }
        );
        // The cursor lands on the file once `/b` answers.
        assert_eq!(
            t.memo.get(&PathBuf::from("/b")).map(String::as_str),
            Some("note.txt")
        );

        // `/b` failing too is the end of it: the tab goes home, no third try.
        let CdFallout::Reveal { to, pending } = fallout else {
            unreachable!()
        };
        t.cwd = to;
        t.pending_cd = Some(pending);
        assert_eq!(
            t.cd_failed(),
            CdFallout::Revert {
                to: PathBuf::from("/a")
            }
        );
        assert!(t.back.is_empty());
    }

    /// `filer C:\dir\file.txt`: nothing checked the path before the window
    /// went up, so the first failed listing reveals the file, and a second
    /// failure sends the tab home. No history entry was pushed to undo.
    #[test]
    fn a_start_path_that_names_a_file_reveals_it() {
        let cwd = PathBuf::from("/b/note.txt");
        let mut t = Tab::new(cwd, SortSpec::default(), true, String::new());
        t.pending_cd = Some(PendingCd {
            from: PathBuf::from("/home"),
            pushed: false,
            fallback: true,
        });
        let fallout = t.cd_failed();
        let CdFallout::Reveal { to, pending } = fallout else {
            panic!("want Reveal")
        };
        assert_eq!(to, PathBuf::from("/b"));
        assert_eq!(
            t.memo.get(&PathBuf::from("/b")).map(String::as_str),
            Some("note.txt")
        );

        t.cwd = to;
        t.pending_cd = Some(pending);
        assert_eq!(
            t.cd_failed(),
            CdFallout::Revert {
                to: PathBuf::from("/home")
            }
        );
    }

    #[test]
    fn shift_click_grows_from_the_cursor() {
        let mut t = tab(6);
        t.click(1);
        t.shift_click(3);
        assert_eq!(names(&t), ["b", "c", "d"]);
        assert_eq!(t.current.cursor, 3);
    }

    #[test]
    fn repeated_shift_clicks_rewrite_the_range() {
        let mut t = tab(6);
        t.click(1);
        t.shift_click(4);
        // Shrinking back keeps the anchor at row 1 instead of piling up.
        t.shift_click(2);
        assert_eq!(names(&t), ["b", "c"]);
        // And it reaches backwards past the anchor just as well.
        t.shift_click(0);
        assert_eq!(names(&t), ["a", "b"]);
    }

    #[test]
    fn ctrl_click_flips_one_row() {
        let mut t = tab(6);
        t.ctrl_click(2);
        t.ctrl_click(4);
        assert_eq!(names(&t), ["c", "e"]);
        t.ctrl_click(2);
        assert_eq!(names(&t), ["e"]);
    }

    #[test]
    fn shift_click_keeps_what_ctrl_click_picked() {
        let mut t = tab(6);
        t.ctrl_click(0);
        t.ctrl_click(2);
        t.shift_click(4);
        assert_eq!(names(&t), ["a", "c", "d", "e"]);
    }

    #[test]
    fn a_plain_click_drops_the_range_and_the_selection_stays() {
        let mut t = tab(6);
        t.click(1);
        t.shift_click(3);
        // A plain click only moves the cursor; the next Shift+click starts there.
        t.click(4);
        t.shift_click(5);
        assert_eq!(names(&t), ["b", "c", "d", "e", "f"]);
    }

    #[test]
    fn a_keyboard_move_re_anchors_the_range() {
        let mut t = tab(6);
        t.click(0);
        t.shift_click(1);
        t.current.arrow(Step::Rel(2), 10);
        t.sync_visual();
        t.shift_click(4);
        assert_eq!(names(&t), ["a", "b", "d", "e"]);
    }

    #[test]
    fn visual_mode_and_the_mouse_share_the_selection() {
        let mut t = tab(6);
        t.ctrl_click(0);
        t.current.cursor = 2;
        t.enter_visual(false);
        t.current.arrow(Step::Rel(1), 10);
        t.sync_visual();
        assert_eq!(names(&t), ["a", "c", "d"]);
        // Leaving visual mode keeps what it added, and Shift+click extends it.
        t.leave_visual();
        t.shift_click(4);
        assert_eq!(names(&t), ["a", "c", "d", "e"]);
    }
}
