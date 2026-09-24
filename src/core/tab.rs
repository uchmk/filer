use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

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
}

impl Tab {
    pub fn new(cwd: PathBuf, sort: SortSpec, show_hidden: bool, linemode: String) -> Self {
        Self {
            current: Folder::loading(cwd.clone(), None),
            parent: cwd.parent().map(|p| Folder::loading(p.to_path_buf(), None)),
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
        }
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
        self.current.hovered().map(|e| vec![e.path.clone()]).unwrap_or_default()
    }

    pub fn toggle(&mut self, state: Option<bool>) {
        let Some(e) = self.current.hovered() else { return };
        let path = e.path.clone();
        let on = state.unwrap_or(!self.selected.contains(&path));
        if on {
            self.selected.insert(path);
        } else {
            self.selected.remove(&path);
        }
    }

    pub fn toggle_all(&mut self, state: Option<bool>) {
        let paths: Vec<PathBuf> =
            self.current.view.iter().map(|&i| self.current.entries[i as usize].path.clone()).collect();
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
        let (lo, hi) = if v.anchor <= to { (v.anchor, to) } else { (to, v.anchor) };
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
                    dir_size: None,
                }
            })
            .collect();
        let mut t = Tab::new(PathBuf::from("/t"), SortSpec::default(), true, String::new());
        t.current = Folder::from_entries(PathBuf::from("/t"), Arc::new(entries), true);
        t
    }

    /// The selection as the names it holds, in order.
    fn names(t: &Tab) -> Vec<String> {
        t.selected.iter().map(|p| crate::util::file_name(p)).collect()
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
