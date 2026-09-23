use std::path::PathBuf;
use std::sync::Arc;

use crate::config::cmd::Step;
use crate::fs::{Entry, SortSpec};

use super::fuzzy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadState {
    Loading,
    Ready,
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub query: String,
    pub smart: bool,
    pub insensitive: bool,
}

impl Filter {
    fn case_sensitive(&self) -> bool {
        fuzzy::is_case_sensitive(&self.query, self.smart, self.insensitive)
    }
}

/// One directory listing plus the cursor state that belongs to it.
pub struct Folder {
    pub path: PathBuf,
    pub entries: Arc<Vec<Entry>>,
    /// Indices into `entries` that survive the hidden flag and the filter.
    pub view: Vec<u32>,
    /// Highlight positions per visible row, when a filter is active.
    pub hits: Vec<Vec<usize>>,
    pub cursor: usize,
    pub offset: usize,
    pub state: LoadState,
    pub scan_id: Option<u64>,
    pub filter: Option<Filter>,
}

impl Folder {
    pub fn loading(path: PathBuf, scan_id: Option<u64>) -> Self {
        Self {
            path,
            entries: Arc::new(Vec::new()),
            view: Vec::new(),
            hits: Vec::new(),
            cursor: 0,
            offset: 0,
            state: LoadState::Loading,
            scan_id,
            filter: None,
        }
    }

    pub fn from_entries(path: PathBuf, entries: Arc<Vec<Entry>>, show_hidden: bool) -> Self {
        let mut f = Self {
            path,
            entries,
            view: Vec::new(),
            hits: Vec::new(),
            cursor: 0,
            offset: 0,
            state: LoadState::Ready,
            scan_id: None,
            filter: None,
        };
        f.rebuild(show_hidden);
        f
    }

    pub fn hovered(&self) -> Option<&Entry> {
        self.view.get(self.cursor).map(|&i| &self.entries[i as usize])
    }

    pub fn hovered_name(&self) -> Option<&str> {
        self.hovered().map(|e| e.name.as_str())
    }

    pub fn at(&self, row: usize) -> Option<&Entry> {
        self.view.get(row).map(|&i| &self.entries[i as usize])
    }

    pub fn hit_at(&self, row: usize) -> &[usize] {
        self.hits.get(row).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Recompute the visible rows, keeping the cursor on the same file if it
    /// is still there.
    pub fn rebuild(&mut self, show_hidden: bool) {
        let keep = self.hovered().map(|e| e.name.clone());
        self.view.clear();
        self.hits.clear();
        let filter = self.filter.clone().filter(|f| !f.query.is_empty());
        let case_sensitive = filter.as_ref().map(Filter::case_sensitive).unwrap_or(false);

        for (i, e) in self.entries.iter().enumerate() {
            if !show_hidden && e.hidden {
                continue;
            }
            match &filter {
                None => {
                    self.view.push(i as u32);
                    self.hits.push(Vec::new());
                }
                Some(f) => {
                    if let Some(hit) = fuzzy::match_str(&f.query, &e.name, case_sensitive) {
                        self.view.push(i as u32);
                        self.hits.push(hit.positions);
                    }
                }
            }
        }

        self.cursor = match keep.and_then(|n| self.index_of(&n)) {
            Some(i) => i,
            None => self.cursor.min(self.view.len().saturating_sub(1)),
        };
        if self.view.is_empty() {
            self.cursor = 0;
            self.offset = 0;
        }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.view
            .iter()
            .position(|&i| self.entries[i as usize].name == name)
    }

    pub fn select_name(&mut self, name: &str) -> bool {
        match self.index_of(name) {
            Some(i) => {
                self.cursor = i;
                true
            }
            None => false,
        }
    }

    pub fn resort(&mut self, sort: &SortSpec, show_hidden: bool) {
        let entries = Arc::make_mut(&mut self.entries);
        sort.apply(entries);
        self.rebuild(show_hidden);
    }

    /// Move the cursor. `page` is the number of rows that fit on screen.
    pub fn arrow(&mut self, step: Step, page: usize) {
        if self.view.is_empty() {
            return;
        }
        let last = self.view.len() - 1;
        let delta = match step {
            Step::Top => {
                self.cursor = 0;
                return;
            }
            Step::Bot => {
                self.cursor = last;
                return;
            }
            Step::Rel(n) => n,
            Step::Pct(p) => (page as i64 * p) / 100,
        };
        let next = self.cursor as i64 + delta;
        self.cursor = next.clamp(0, last as i64) as usize;
    }

    /// Keep `scrolloff` rows of context around the cursor.
    pub fn clamp_offset(&mut self, rows: usize, scrolloff: usize) {
        if rows == 0 {
            self.offset = 0;
            return;
        }
        let len = self.view.len();
        if len <= rows {
            self.offset = 0;
            return;
        }
        let pad = scrolloff.min((rows.saturating_sub(1)) / 2);
        let max_offset = len - rows;
        if self.cursor < self.offset + pad {
            self.offset = self.cursor.saturating_sub(pad);
        } else if self.cursor + pad >= self.offset + rows {
            self.offset = (self.cursor + pad + 1).saturating_sub(rows);
        }
        self.offset = self.offset.min(max_offset);
    }

    /// Scroll without moving the cursor off screen (mouse wheel).
    pub fn scroll(&mut self, delta: i64, rows: usize) {
        let len = self.view.len();
        if len <= rows {
            return;
        }
        let max_offset = (len - rows) as i64;
        self.offset = (self.offset as i64 + delta).clamp(0, max_offset) as usize;
        let lo = self.offset;
        let hi = (self.offset + rows).saturating_sub(1);
        self.cursor = self.cursor.clamp(lo, hi);
    }
}
