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

    /// Recompute the selection from the visual anchor to the cursor.
    pub fn sync_visual(&mut self) {
        let Some(v) = self.visual.clone() else { return };
        let (lo, hi) = if v.anchor <= self.current.cursor {
            (v.anchor, self.current.cursor)
        } else {
            (self.current.cursor, v.anchor)
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
        self.selected = next;
    }

    pub fn clear_selection(&mut self) -> bool {
        let had = !self.selected.is_empty();
        self.selected.clear();
        had
    }
}
