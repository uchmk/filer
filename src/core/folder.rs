use std::path::PathBuf;
use std::sync::Arc;

use crate::config::cmd::Step;
use crate::fs::{Entry, SortSpec};

use ito_match::Matcher;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadState {
    Loading,
    Ready,
    Error(String),
}

/// What `f` keeps: the names a regular expression finds (smart case, as every
/// search is), with the query kept as typed so the prompt can show it again.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub query: String,
    pub matcher: Option<Matcher>,
}

impl Filter {
    /// `Err` is why `query` is not a regular expression.
    pub fn new(query: String) -> Result<Self, String> {
        let matcher = if query.is_empty() { None } else { Some(Matcher::new(&query)?) };
        Ok(Self { query, matcher })
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

    /// `entries.get`, not `entries[..]`. `view` holds indices into `entries`,
    /// and the two are only consistent between a swap of one and a rebuild of
    /// the other — a window `rebuild` itself opens, since it asks this which
    /// file to keep the cursor on before it has rebuilt anything. A file
    /// deleted from outside the program shrinks `entries` under a `view` that
    /// still points past the new end, and indexing took the window with it.
    pub fn hovered(&self) -> Option<&Entry> {
        self.view.get(self.cursor).and_then(|&i| self.entries.get(i as usize))
    }

    pub fn hovered_name(&self) -> Option<&str> {
        self.hovered().map(|e| e.name.as_str())
    }

    /// Total for the same reason as `hovered`, and it matters more here: this
    /// one is called from the drawing code, once per visible row.
    pub fn at(&self, row: usize) -> Option<&Entry> {
        self.view.get(row).and_then(|&i| self.entries.get(i as usize))
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
        let matcher = self.filter.as_ref().and_then(|f| f.matcher.clone());

        for (i, e) in self.entries.iter().enumerate() {
            if !show_hidden && e.hidden {
                continue;
            }
            match &matcher {
                None => {
                    self.view.push(i as u32);
                    self.hits.push(Vec::new());
                }
                Some(m) => {
                    if m.is_match(&e.name) {
                        self.view.push(i as u32);
                        self.hits.push(m.positions(&e.name));
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
            .position(|&i| self.entries.get(i as usize).is_some_and(|e| e.name == name))
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
        if !self.view.is_empty() {
            self.cursor = step.apply(self.cursor, self.view.len(), page);
        }
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
    ///
    /// The cursor is kept `scrolloff` rows inside the new window, not on its
    /// edge: [`Self::clamp_offset`] runs before every frame, and a cursor on
    /// the edge made it pull the window straight back to `cursor - scrolloff`.
    /// The two were a fixed point, so the wheel moved the cursor two rows and
    /// the view not at all (#110, 19.3). At either end of the list there is no
    /// margin to keep, as there is none in `clamp_offset` either.
    pub fn scroll(&mut self, delta: i64, rows: usize, scrolloff: usize) {
        let len = self.view.len();
        if len <= rows {
            return;
        }
        let max_offset = len - rows;
        self.offset = (self.offset as i64 + delta).clamp(0, max_offset as i64) as usize;
        let pad = scrolloff.min(rows.saturating_sub(1) / 2);
        let lo = if self.offset == 0 { 0 } else { self.offset + pad };
        let hi = match self.offset == max_offset {
            true => len - 1,
            false => (self.offset + rows).saturating_sub(1 + pad),
        };
        self.cursor = self.cursor.clamp(lo, hi.max(lo));
    }
}

#[cfg(test)]
mod stale_view {
    use super::*;
    use crate::fs::entry::{Entry, Kind};

    fn listing(n: usize) -> Arc<Vec<Entry>> {
        Arc::new(
            (0..n)
                .map(|i| Entry {
                    path: PathBuf::from(format!("f{i}")),
                    name: format!("f{i}"),
                    ext: None,
                    kind: Kind::File,
                    ..Default::default()
                })
                .collect(),
        )
    }

    /// A file deleted from outside shrinks the listing under the cursor.
    ///
    /// `entries` is replaced and `rebuild` is called, and `rebuild` opens by
    /// asking `hovered()` which file to keep the cursor on — a question it
    /// answers with the *old* `view`, whose indices now run past the end of the
    /// new `entries`. `view.get()` succeeds, hands back index 11, and the
    /// direct index into a ten-element `entries` took the window with it.
    #[test]
    fn a_listing_that_shrank_under_the_cursor_does_not_panic() {
        let mut f = Folder::from_entries(PathBuf::from("d"), listing(12), true);
        f.cursor = 11; // the last row

        f.entries = listing(10); // something else deleted two files
        f.rebuild(true); // panicked here: len is 10 but the index is 11

        assert!(f.cursor < f.view.len(), "the cursor must land inside the new listing");
    }

    /// The same thing one row at a time, since the crash needs the stale index
    /// to point past the new end and it is worth covering the boundary rather
    /// than one lucky number.
    #[test]
    fn every_amount_of_shrinkage_is_survivable() {
        for before in 1..=12usize {
            for after in 0..=before {
                let mut f = Folder::from_entries(PathBuf::from("d"), listing(before), true);
                f.cursor = before - 1;
                f.entries = listing(after);
                f.rebuild(true);
                assert!(
                    f.view.is_empty() || f.cursor < f.view.len(),
                    "{before} -> {after}: cursor {} is outside a view of {}",
                    f.cursor,
                    f.view.len(),
                );
            }
        }
    }

    /// `at()` reads the same way and is called from the drawing code, where a
    /// panic would be just as fatal.
    #[test]
    fn at_survives_a_stale_row() {
        let mut f = Folder::from_entries(PathBuf::from("d"), listing(12), true);
        f.entries = listing(3);
        assert!(f.at(11).is_none());
    }
}

/// #110 / 19.3: the wheel scrolls the list with `scrolloff` set.
#[cfg(test)]
mod wheel {
    use super::*;
    use crate::fs::entry::{Entry, Kind};

    fn folder(n: usize) -> Folder {
        let entries = (0..n)
            .map(|i| Entry { path: PathBuf::from(format!("f{i}")), name: format!("f{i}"), kind: Kind::File, ..Default::default() })
            .collect();
        Folder::from_entries(PathBuf::from("/"), Arc::new(entries), true)
    }

    /// One frame is `scroll` then the next frame's `clamp_offset`. Before, the
    /// second undid the first and the view never moved off the top.
    #[test]
    fn the_wheel_moves_the_view_and_the_next_frame_keeps_it() {
        let (rows, scrolloff) = (20, 5);
        let mut f = folder(500);
        f.clamp_offset(rows, scrolloff);
        for _ in 0..10 {
            f.scroll(3, rows, scrolloff);
            f.clamp_offset(rows, scrolloff);
        }
        assert_eq!(f.offset, 30, "ten notches of three rows: {}", f.offset);
        assert!(f.cursor >= f.offset + scrolloff, "the cursor is inside the margin: {}", f.cursor);

        for _ in 0..10 {
            f.scroll(-3, rows, scrolloff);
            f.clamp_offset(rows, scrolloff);
        }
        assert_eq!(f.offset, 0, "and back up to the top");
    }

    /// At the ends there is no margin: the cursor can reach the first and the
    /// last row, as it can with the keys.
    #[test]
    fn at_the_ends_the_cursor_reaches_the_edge() {
        let (rows, scrolloff) = (20, 5);
        let mut f = folder(100);
        f.cursor = 99;
        f.clamp_offset(rows, scrolloff);
        f.scroll(10, rows, scrolloff);
        f.clamp_offset(rows, scrolloff);
        assert_eq!((f.offset, f.cursor), (80, 99), "at the bottom, the last row");
        f.cursor = 0;
        f.clamp_offset(rows, scrolloff);
        f.scroll(-10, rows, scrolloff);
        assert_eq!((f.offset, f.cursor), (0, 0), "at the top, the first row");
    }
}
