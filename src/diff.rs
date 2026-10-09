//! Comparing two files, line by line, for the side-by-side view.
//!
//! [`compare`] is pure and is the whole of the interesting part; [`Differ`] is
//! the usual worker around it, since reading two files and lining them up is
//! not something the UI thread may do.

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};

/// One line of one side, with the number it has in its own file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line {
    pub no: usize,
    pub text: String,
    /// The parts of `text` that changed, as char ranges, when this line is
    /// one of an edited pair: the view paints them stronger than the rest of
    /// the row, so a one-word edit in a long line can be found at a glance.
    /// Empty when the line has nothing opposite it, or nothing in common with
    /// it -- then the whole row is the change, and its tint already says so.
    pub changed: Vec<std::ops::Range<usize>>,
}

/// One row of the view: what is on the left, what is on the right, and whether
/// they are the same line. A row with only one side is a line that exists in
/// one file and not the other.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub left: Option<Line>,
    pub right: Option<Line>,
    pub same: bool,
}

/// What a comparison found about one path present in one tree or both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeState {
    LeftOnly,
    RightOnly,
    /// The contents differ, or one side is a file where the other is a folder.
    Differ,
    /// Byte for byte the same.
    Same,
    /// The same size, but too big to read inside the budget, so this is as much
    /// as was established. Not reported as `Same`: saying two files match is a
    /// claim, and this is the absence of one.
    Unread,
}

impl TreeState {
    /// The state in words, as the state file and `C`'s copy write it.
    pub fn word(self) -> &'static str {
        match self {
            TreeState::LeftOnly => "left only",
            TreeState::RightOnly => "right only",
            TreeState::Differ => "differ",
            TreeState::Same => "same",
            TreeState::Unread => "unread",
        }
    }
}

/// One path, relative to both roots.
/// How many rows are in each state. Summed when the comparison is built.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct TreeCounts {
    pub left_only: usize,
    pub right_only: usize,
    pub differ: usize,
    pub same: usize,
    pub unread: usize,
}

impl TreeCounts {
    /// Public so anything building an [`Outcome::Tree`] counts the rows it
    /// actually has, rather than writing numbers that can drift from them.
    pub fn of(rows: &[TreeRow]) -> Self {
        let mut c = Self::default();
        for row in rows {
            let n = match row.state {
                TreeState::LeftOnly => &mut c.left_only,
                TreeState::RightOnly => &mut c.right_only,
                TreeState::Differ => &mut c.differ,
                TreeState::Same => &mut c.same,
                TreeState::Unread => &mut c.unread,
            };
            *n += 1;
        }
        c
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TreeRow {
    pub rel: PathBuf,
    pub state: TreeState,
    /// Whether it is a directory on the side it exists on.
    pub dir: bool,
    /// Sizes, where the side has the file. A folder's is zero.
    pub left: u64,
    pub right: u64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Byte for byte the same file.
    Identical,
    /// At least one side is not text. Lining up bytes is nobody's idea of a
    /// diff, so this says only whether they match.
    Binary { same: bool },
    Rows {
        rows: Vec<Row>,
        /// The view stops here: there were more rows than [`MAX_ROWS`].
        truncated: bool,
        /// The files were too big to line up properly, so the part that differs
        /// is shown side by side without being matched up.
        rough: bool,
    },
    /// Two directory trees, paired by the path each entry has inside them.
    Tree {
        rows: Vec<TreeRow>,
        /// How many of each state there are, counted once here rather than in
        /// the renderer: the footer wants totals over every row, and the view
        /// draws only the rows on screen, so counting there would walk a
        /// hundred thousand paths every frame to print four numbers.
        counts: TreeCounts,
        /// The walk stopped at [`MAX_TREE_ENTRIES`].
        truncated: bool,
    },
    Error(String),
}

/// Paths compared before a tree walk gives up.
pub const MAX_TREE_ENTRIES: usize = 100_000;

/// The largest pair of same-sized files that gets read to settle whether they
/// match. Past this the row says `Unread` rather than guessing: a comparison
/// that reads two 4 GB files to answer one row is not a comparison anyone asked
/// for.
pub const MAX_COMPARE_BYTES: u64 = 64 << 20;

/// Rows past this are dropped. Long past the point where anyone is reading.
pub const MAX_ROWS: usize = 5_000;

/// The largest table [`compare`] will build: lines on the left times lines on
/// the right, once the matching top and bottom are peeled off. At four bytes a
/// cell this is 16 MB on a worker thread, held for as long as one comparison
/// takes.
pub const MAX_CELLS: usize = 4_000_000;

/// Read both files and line them up. Runs on the worker.
pub fn compare_files(left: &Path, right: &Path, max_bytes: usize) -> Outcome {
    let a = match std::fs::read(left) {
        Ok(b) => b,
        Err(e) => return Outcome::Error(format!("{}: {e}", crate::util::file_name(left))),
    };
    let b = match std::fs::read(right) {
        Ok(b) => b,
        Err(e) => return Outcome::Error(format!("{}: {e}", crate::util::file_name(right))),
    };
    if a == b {
        return Outcome::Identical;
    }
    let (Some(a), Some(b)) = (as_text(&a, max_bytes), as_text(&b, max_bytes)) else {
        return Outcome::Binary { same: false };
    };
    let (rows, rough) = compare(&a, &b, MAX_CELLS);
    let truncated = rows.len() > MAX_ROWS;
    let rows = rows.into_iter().take(MAX_ROWS).collect();
    Outcome::Rows { rows, truncated, rough }
}

/// Split into lines, or decline: a NUL byte or invalid UTF-8 means this is not
/// something to read side by side. Oversized files are cut short rather than
/// refused, the way the text preview cuts them.
fn as_text(bytes: &[u8], max_bytes: usize) -> Option<Vec<String>> {
    let head = &bytes[..bytes.len().min(max_bytes)];
    if head.contains(&0) {
        return None;
    }
    let text = std::str::from_utf8(head).ok()?;
    Some(text.lines().map(|l| l.replace('\t', "    ")).collect())
}

/// Line up `a` and `b`, longest common subsequence.
///
/// The matching top and bottom are peeled off first, which is what makes this
/// affordable: a one-line change in a thousand-line file leaves a table of
/// almost nothing. Returns the rows and whether the middle had to be shown
/// roughly, unmatched, for being too large to line up.
pub fn compare(a: &[String], b: &[String], max_cells: usize) -> (Vec<Row>, bool) {
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();

    let mut rows: Vec<Row> = Vec::new();
    for i in 0..head {
        rows.push(both(i, &a[i], i, &b[i]));
    }

    let am = &a[head..a.len() - tail];
    let bm = &b[head..b.len() - tail];
    let rough = am.len().saturating_mul(bm.len()) > max_cells;
    if rough {
        // Nothing is claimed about which line answers which: they are simply
        // laid alongside each other so the shape of the difference shows.
        for i in 0..am.len().max(bm.len()) {
            rows.push(Row {
                left: am.get(i).map(|t| line(head + i, t)),
                right: bm.get(i).map(|t| line(head + i, t)),
                same: false,
            });
        }
    } else {
        rows.extend(align(am, bm, head));
    }

    for k in 0..tail {
        let i = a.len() - tail + k;
        let j = b.len() - tail + k;
        rows.push(both(i, &a[i], j, &b[j]));
    }
    (rows, rough)
}

fn line(i: usize, text: &str) -> Line {
    Line { no: i + 1, text: text.to_owned(), changed: Vec::new() }
}

fn both(i: usize, x: &str, j: usize, y: &str) -> Row {
    Row { left: Some(line(i, x)), right: Some(line(j, y)), same: true }
}

/// The table walk. `off` is how many lines were peeled off the top, so the line
/// numbers come out right.
fn align(a: &[String], b: &[String], off: usize) -> Vec<Row> {
    let (n, m) = (a.len(), b.len());
    let stride = m + 1;
    // lcs[i][j] is the length of the longest common subsequence of a[i..] and
    // b[j..], filled from the bottom right so the walk below can go forwards.
    let mut lcs = vec![0u32; (n + 1) * stride];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * stride + j] = if a[i] == b[j] {
                lcs[(i + 1) * stride + j + 1] + 1
            } else {
                lcs[(i + 1) * stride + j].max(lcs[i * stride + j + 1])
            };
        }
    }

    let mut rows = Vec::new();
    let (mut dels, mut ins): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            flush(&mut rows, &mut dels, &mut ins, a, b, off);
            rows.push(both(off + i, &a[i], off + j, &b[j]));
            i += 1;
            j += 1;
        } else if j == m || (i < n && lcs[(i + 1) * stride + j] >= lcs[i * stride + j + 1]) {
            dels.push(i);
            i += 1;
        } else {
            ins.push(j);
            j += 1;
        }
    }
    flush(&mut rows, &mut dels, &mut ins, a, b, off);
    rows
}

/// Turn a run of removals and a run of additions into rows. Pairing them up
/// puts an edited line opposite the line it was edited from, which is the whole
/// reason for looking at this side by side.
fn flush(
    rows: &mut Vec<Row>,
    dels: &mut Vec<usize>,
    ins: &mut Vec<usize>,
    a: &[String],
    b: &[String],
    off: usize,
) {
    for k in 0..dels.len().max(ins.len()) {
        let mut left = dels.get(k).map(|&i| line(off + i, &a[i]));
        let mut right = ins.get(k).map(|&j| line(off + j, &b[j]));
        if let (Some(l), Some(r)) = (&mut left, &mut right) {
            (l.changed, r.changed) = changed_words(&l.text, &r.text);
        }
        rows.push(Row { left, right, same: false });
    }
    dels.clear();
    ins.clear();
}

/// Past this many words on a side, a pair of lines is not split into words:
/// the table is the square of it, and a line that long is not read word by word.
const MAX_WORDS: usize = 400;

/// Split a line into words: a run of letters, digits and `_`, a run of
/// whitespace, or any other single character. As char ranges.
fn words(s: &str) -> Vec<std::ops::Range<usize>> {
    let class = |c: char| if c.is_alphanumeric() || c == '_' { 0 } else if c.is_whitespace() { 1 } else { 2 };
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let k = class(chars[i]);
        let mut j = i + 1;
        if k != 2 {
            while j < chars.len() && class(chars[j]) == k {
                j += 1;
            }
        }
        out.push(i..j);
        i = j;
    }
    out
}

/// What changed between two lines that face each other, word by word: the
/// words of each that are not in their longest common run, merged where they
/// touch. Both empty when the lines share nothing (or are too long to split),
/// which leaves the whole row as the change.
pub fn changed_words(a: &str, b: &str) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let (wa, wb) = (words(a), words(b));
    if wa.len() > MAX_WORDS || wb.len() > MAX_WORDS {
        return (Vec::new(), Vec::new());
    }
    let (ca, cb): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let ta: Vec<&[char]> = wa.iter().map(|r| &ca[r.clone()]).collect();
    let tb: Vec<&[char]> = wb.iter().map(|r| &cb[r.clone()]).collect();
    let (n, m) = (ta.len(), tb.len());
    let stride = m + 1;
    let mut lcs = vec![0u32; (n + 1) * stride];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * stride + j] = if ta[i] == tb[j] {
                lcs[(i + 1) * stride + j + 1] + 1
            } else {
                lcs[(i + 1) * stride + j].max(lcs[i * stride + j + 1])
            };
        }
    }
    // Whitespace alone in common is not a resemblance worth showing.
    let shared = (0..n).any(|i| (0..m).any(|j| ta[i] == tb[j] && !ta[i].iter().all(|c| c.is_whitespace())));
    if !shared {
        return (Vec::new(), Vec::new());
    }
    let (mut keep_a, mut keep_b) = (vec![false; n], vec![false; m]);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if ta[i] == tb[j] {
            keep_a[i] = true;
            keep_b[j] = true;
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * stride + j] >= lcs[i * stride + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let spans = |ws: &[std::ops::Range<usize>], keep: &[bool]| {
        let mut out: Vec<std::ops::Range<usize>> = Vec::new();
        for (r, &k) in ws.iter().zip(keep) {
            if k {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.end == r.start => last.end = r.end,
                _ => out.push(r.clone()),
            }
        }
        out
    };
    (spans(&wa, &keep_a), spans(&wb, &keep_b))
}

/// Where the next change is, from `row`, in the direction given. Used by `n`
/// and `N` in the view.
pub fn next_change(rows: &[Row], from: usize, back: bool) -> Option<usize> {
    // Only the first row of a block counts, so holding `n` walks from one
    // difference to the next rather than down the lines of one of them.
    let starts_a_block =
        |i: usize| !rows[i].same && (i == 0 || rows[i - 1].same);
    match back {
        true => (0..from.min(rows.len())).rev().find(|&i| starts_a_block(i)),
        false => (from + 1..rows.len()).find(|&i| starts_a_block(i)),
    }
}

/// How many differences `n` walks through: blocks of changed rows, not rows.
pub fn count_changes(rows: &[Row]) -> usize {
    (0..rows.len()).filter(|&i| !rows[i].same && (i == 0 || rows[i - 1].same)).count()
}

// ------------------------------------------------------------------- worker

/// Walk both trees and pair their entries by the path each has inside its root.
///
/// The cheap test first: two files of different lengths differ, and nothing has
/// to be read to know it -- which settles almost every row in practice. Only
/// same-sized pairs are read, and only up to [`MAX_COMPARE_BYTES`].
pub fn compare_trees(left: &Path, right: &Path) -> Outcome {
    compare_trees_within(left, right, MAX_TREE_ENTRIES)
}

/// [`compare_trees`] with the path limit given, so a test can reach it with a
/// tree of twenty files rather than a hundred thousand.
fn compare_trees_within(left: &Path, right: &Path, limit: usize) -> Outcome {
    // Half the limit for each side, not one limit shared. Shared, the left
    // walk could spend all of it, and every path the right walk never reached
    // then read as "only left": two identical `node_modules` came out as
    // `5000 only left · 0 only right` (#86).
    let (mut budget_l, mut budget_r) = (limit / 2, limit / 2);
    let a = match walk(left, &mut budget_l) {
        Ok(m) => m,
        Err(e) => return Outcome::Error(format!("{}: {e}", crate::util::file_name(left))),
    };
    let b = match walk(right, &mut budget_r) {
        Ok(m) => m,
        Err(e) => return Outcome::Error(format!("{}: {e}", crate::util::file_name(right))),
    };
    // A side that was cut short cannot say a path is missing from it, so a
    // path found only on the *other* side is not a difference anyone can
    // vouch for, and is left out rather than shown as `<` or `>`.
    let (cut_l, cut_r) = (budget_l == 0, budget_r == 0);
    let mut rels: Vec<PathBuf> = a.keys().chain(b.keys()).cloned().collect();
    rels.sort();
    rels.dedup();

    let mut rows = Vec::with_capacity(rels.len());
    for rel in rels {
        let (l, r) = (a.get(&rel), b.get(&rel));
        let (state, dir, ls, rs) = match (l, r) {
            (Some(_), None) if cut_r => continue,
            (None, Some(_)) if cut_l => continue,
            (Some(e), None) => (TreeState::LeftOnly, e.dir, e.len, 0),
            (None, Some(e)) => (TreeState::RightOnly, e.dir, 0, e.len),
            // A link is compared as a link: by where it points. Reading
            // through it compared the targets' contents, so two links to
            // identical files read as matching wherever they pointed (45.11),
            // and a link to a directory could not be opened as a file and read
            // as "too big to read" (#86).
            (Some(le), Some(re)) if le.link.is_some() || re.link.is_some() => {
                let state = if le.link == re.link { TreeState::Same } else { TreeState::Differ };
                (state, false, 0, 0)
            }
            (Some(le), Some(re)) => {
                let (ld, ll, rd, rl) = (le.dir, le.len, re.dir, re.len);
                if ld != rd {
                    // A folder on one side and a file on the other is a
                    // difference, not something to read.
                    (TreeState::Differ, ld, ll, rl)
                } else if ld {
                    (TreeState::Same, true, 0, 0)
                } else if ll != rl {
                    (TreeState::Differ, false, ll, rl)
                } else {
                    let state = match same_bytes(&left.join(&rel), &right.join(&rel), ll) {
                        Some(true) => TreeState::Same,
                        Some(false) => TreeState::Differ,
                        None => TreeState::Unread,
                    };
                    (state, false, ll, rl)
                }
            }
            (None, None) => continue,
        };
        rows.push(TreeRow { rel, state, dir, left: ls, right: rs });
        if rows.len() >= MAX_ROWS {
            let counts = TreeCounts::of(&rows);
            return Outcome::Tree { rows, counts, truncated: true };
        }
    }
    let counts = TreeCounts::of(&rows);
    Outcome::Tree { rows, counts, truncated: cut_l || cut_r }
}

/// One path found by [`walk`].
#[derive(Debug, PartialEq)]
struct Found {
    dir: bool,
    len: u64,
    /// Where a symlink (or a junction) points. `None` for anything else.
    link: Option<Target>,
}

/// Where a link points, in the form two trees can be compared by.
#[derive(Debug, PartialEq)]
enum Target {
    /// Somewhere inside the tree being compared, relative to its root. Two
    /// copies of one tree hold links that point into *their own* copy, and a
    /// junction's target is always absolute, so as written they could never
    /// match (Q26, #98). Relative to the root they do.
    Inside(PathBuf),
    /// Anywhere else, as written: two links out to one shared place match,
    /// and a relative link out of the tree is left as the text it is.
    Written(PathBuf),
}

impl Target {
    /// `link` is the link's own path under `root`, `to` what `read_link`
    /// returned; `roots` is `root` spelled every way a target might spell it.
    fn of(roots: &[PathBuf], link: &Path, to: PathBuf) -> Self {
        let absolute = match to.is_absolute() {
            true => crate::util::unverbatim(&to),
            false => link.parent().map(|p| p.join(&to)).unwrap_or_else(|| to.clone()),
        };
        let absolute = crate::util::normalize(&absolute);
        for root in roots {
            if let Ok(rel) = absolute.strip_prefix(root) {
                return Self::Inside(rel.to_path_buf());
            }
        }
        Self::Written(to)
    }
}

/// Every path under `root`, relative to it, as (is a directory, length).
///
/// `symlink_metadata`, so a link is itself rather than what it points at: two
/// trees that differ only in where a link goes should read as differing, and a
/// link into a parent must not turn the walk into a loop.
fn walk(root: &Path, budget: &mut usize) -> std::io::Result<std::collections::HashMap<PathBuf, Found>> {
    let mut out = std::collections::HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    // Fail on the root only: a directory further down that cannot be read is one
    // unreadable row, not a reason to abandon the comparison.
    std::fs::metadata(root)?;
    // The spellings a link's target might use for the root: as it was given,
    // and as the file system resolves it (a junction's target is the real
    // path, with any link above the root already followed).
    let mut roots = vec![crate::util::normalize(&crate::util::unverbatim(root))];
    if let Ok(real) = std::fs::canonicalize(root) {
        let real = crate::util::normalize(&crate::util::unverbatim(&real));
        if !roots.contains(&real) {
            roots.push(real);
        }
    }
    while let Some(p) = stack.pop() {
        if *budget == 0 {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&p) else { continue };
        for e in rd.filter_map(|e| e.ok()) {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            let path = e.path();
            let Ok(md) = std::fs::symlink_metadata(&path) else { continue };
            let is_link = md.file_type().is_symlink();
            let dir = md.is_dir() && !is_link;
            // An unreadable target still marks it a link: it is compared, and
            // a link that cannot be read on either side reads as the same.
            let link = is_link.then(|| Target::of(&roots, &path, std::fs::read_link(&path).unwrap_or_default()));
            if let Ok(rel) = path.strip_prefix(root) {
                let len = if dir || is_link { 0 } else { md.len() };
                out.insert(rel.to_path_buf(), Found { dir, len, link });
            }
            if dir {
                stack.push(path);
            }
        }
    }
    Ok(out)
}

/// Whether two files of the same length hold the same bytes. `None` when they
/// are longer than [`MAX_COMPARE_BYTES`], so nothing was established.
///
/// Read in blocks and stopped at the first difference, so two files that differ
/// early cost almost nothing however large they are.
fn same_bytes(a: &Path, b: &Path, len: u64) -> Option<bool> {
    use std::io::Read;
    if len > MAX_COMPARE_BYTES {
        return None;
    }
    let mut fa = std::io::BufReader::new(std::fs::File::open(a).ok()?);
    let mut fb = std::io::BufReader::new(std::fs::File::open(b).ok()?);
    let mut ba = [0u8; 64 << 10];
    let mut bb = [0u8; 64 << 10];
    loop {
        let na = fa.read(&mut ba).ok()?;
        let nb = fb.read(&mut bb).ok()?;
        if na != nb {
            return Some(false);
        }
        if na == 0 {
            return Some(true);
        }
        if ba[..na] != bb[..nb] {
            return Some(false);
        }
    }
}

pub struct Request {
    pub left: PathBuf,
    pub right: PathBuf,
    pub max_bytes: usize,
}

pub struct Response {
    pub left: PathBuf,
    pub right: PathBuf,
    pub outcome: Outcome,
}

/// Runs [`compare_files`] off the UI thread, newest request only: asking for a
/// second comparison while the first is still reading a slow share drops the
/// answer nobody is waiting for any more.
pub struct Differ {
    tx: Sender<Request>,
    pub rx: Receiver<Response>,
}

impl Differ {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, req_rx) = crossbeam_channel::unbounded::<Request>();
        let (res_tx, rx) = crossbeam_channel::unbounded::<Response>();
        std::thread::Builder::new()
            .name("diff".into())
            .spawn(move || {
                while let Ok(mut req) = req_rx.recv() {
                    while let Ok(newer) = req_rx.try_recv() {
                        req = newer;
                    }
                    let outcome = if req.left.is_dir() && req.right.is_dir() {
                        compare_trees(&req.left, &req.right)
                    } else {
                        compare_files(&req.left, &req.right, req.max_bytes)
                    };
                    if res_tx.send(Response { left: req.left, right: req.right, outcome }).is_err()
                    {
                        return;
                    }
                    wake();
                }
            })
            .expect("spawn diff worker");
        Self { tx, rx }
    }

    pub fn request(&self, req: Request) {
        let _ = self.tx.send(req);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picked(s: &str, spans: &[std::ops::Range<usize>]) -> Vec<String> {
        let c: Vec<char> = s.chars().collect();
        spans.iter().map(|r| c[r.clone()].iter().collect()).collect()
    }

    /// One word changed in a line: that word is the change on each side, and
    /// the rest of the line is not.
    #[test]
    fn one_changed_word_is_marked_alone() {
        let (a, b) = ("let total = price * count;", "let total = cost * count;");
        let (l, r) = changed_words(a, b);
        assert_eq!(picked(a, &l), ["price"]);
        assert_eq!(picked(b, &r), ["cost"]);
    }

    /// Something added on one side only marks that side; neighbouring changed
    /// words merge into one span. Char ranges, so a Japanese word is whole.
    #[test]
    fn additions_and_wide_characters() {
        let (a, b) = ("f(x)", "f(x, y)");
        let (l, r) = changed_words(a, b);
        assert!(l.is_empty(), "nothing on the left went away");
        assert_eq!(picked(b, &r), [", y"]);
        let (a, b) = ("名前 は 太郎", "名前 は 花子");
        let (_, r) = changed_words(a, b);
        assert_eq!(picked(b, &r), ["花子"]);
    }

    /// Lines with nothing in common but spaces are a whole-line change, which
    /// the row's tint already shows.
    #[test]
    fn nothing_in_common_marks_nothing() {
        assert_eq!(changed_words("alpha beta", "gamma delta"), (Vec::new(), Vec::new()));
    }

    /// The pairing in the view carries the words.
    #[test]
    fn an_edited_pair_comes_with_its_words() {
        let a = vec!["keep".to_string(), "one two three".to_string()];
        let b = vec!["keep".to_string(), "one 2 three".to_string()];
        let (rows, _) = compare(&a, &b, 1_000_000);
        let edited = rows.iter().find(|r| !r.same).unwrap();
        assert_eq!(picked(&edited.left.as_ref().unwrap().text, &edited.left.as_ref().unwrap().changed), ["two"]);
        assert_eq!(picked(&edited.right.as_ref().unwrap().text, &edited.right.as_ref().unwrap().changed), ["2"]);
    }

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::to_owned).collect()
    }

    /// What the rows look like written down: `=` both sides, `-` left only,
    /// `+` right only, `~` a line opposite the line it replaced.
    fn shape(rows: &[Row]) -> String {
        rows.iter()
            .map(|r| match (r.same, &r.left, &r.right) {
                (true, ..) => '=',
                (_, Some(_), Some(_)) => '~',
                (_, Some(_), None) => '-',
                (_, None, Some(_)) => '+',
                _ => '?',
            })
            .collect()
    }

    #[test]
    fn identical_text_is_all_matching_rows() {
        let a = lines("one\ntwo\nthree");
        let (rows, rough) = compare(&a, &a, MAX_CELLS);

        assert_eq!(shape(&rows), "===");
        assert!(!rough);
    }

    #[test]
    fn an_edited_line_sits_opposite_the_line_it_replaced() {
        let a = lines("keep\nold\nkeep2");
        let b = lines("keep\nnew\nkeep2");

        let (rows, _) = compare(&a, &b, MAX_CELLS);

        assert_eq!(shape(&rows), "=~=");
        assert_eq!(rows[1].left.as_ref().unwrap().text, "old");
        assert_eq!(rows[1].right.as_ref().unwrap().text, "new");
    }

    #[test]
    fn line_numbers_follow_each_side_of_the_file() {
        let a = lines("a\nb\nc");
        let b = lines("a\nc");

        let (rows, _) = compare(&a, &b, MAX_CELLS);

        assert_eq!(shape(&rows), "=-=");
        assert_eq!(rows[1].left.as_ref().unwrap().no, 2, "`b` is line 2 on the left");
        // `c` is line 3 on the left and line 2 on the right.
        assert_eq!(rows[2].left.as_ref().unwrap().no, 3);
        assert_eq!(rows[2].right.as_ref().unwrap().no, 2);
    }

    #[test]
    fn an_insertion_and_a_removal_each_take_one_side() {
        let (rows, _) = compare(&lines("a\nb"), &lines("a\nx\nb"), MAX_CELLS);
        assert_eq!(shape(&rows), "=+=");

        let (rows, _) = compare(&lines("a\nx\nb"), &lines("a\nb"), MAX_CELLS);
        assert_eq!(shape(&rows), "=-=");
    }

    /// The peeling is what keeps a big file affordable, so it has to survive a
    /// table budget far too small for the whole thing.
    #[test]
    fn a_small_change_in_a_big_file_is_lined_up_exactly() {
        let mut a: Vec<String> = (0..4_000).map(|i| format!("line {i}")).collect();
        let mut b = a.clone();
        b[2_000] = "changed".into();

        let (rows, rough) = compare(&a, &b, 16);

        assert!(!rough, "the middle is one line against one line");
        assert_eq!(rows.len(), 4_000);
        assert_eq!(rows[2_000].right.as_ref().unwrap().text, "changed");
        assert!(rows.iter().filter(|r| !r.same).count() == 1);

        // Two files with nothing in common cannot be peeled, and are shown
        // roughly rather than filling memory with a table.
        a.iter_mut().enumerate().for_each(|(i, l)| *l = format!("a{i}"));
        b = (0..4_000).map(|i| format!("b{i}")).collect();
        let (rows, rough) = compare(&a, &b, 16);
        assert!(rough);
        assert_eq!(rows.len(), 4_000, "still laid out side by side");
    }

    #[test]
    fn an_empty_file_against_a_full_one_is_all_additions() {
        let (rows, _) = compare(&[], &lines("a\nb"), MAX_CELLS);
        assert_eq!(shape(&rows), "++");

        let (rows, _) = compare(&[], &[], MAX_CELLS);
        assert!(rows.is_empty());
    }

    #[test]
    fn n_and_shift_n_walk_between_the_blocks_that_differ() {
        //            0    1    2    3    4    5
        let a = lines("k\nold\nk2\nk3\ngone\nk4");
        let b = lines("k\nnew\nk2\nk3\nk4");
        let (rows, _) = compare(&a, &b, MAX_CELLS);
        assert_eq!(shape(&rows), "=~==-=");

        assert_eq!(next_change(&rows, 0, false), Some(1));
        assert_eq!(next_change(&rows, 1, false), Some(4));
        assert_eq!(next_change(&rows, 4, false), None, "nothing after the last one");
        assert_eq!(next_change(&rows, 5, true), Some(4));
        assert_eq!(next_change(&rows, 4, true), Some(1));
        assert_eq!(next_change(&rows, 1, true), None);

        // Standing inside a block, `n` leaves it rather than finding its own
        // next line.
        let (rows, _) = compare(&lines("k\na\nb"), &lines("k\nx\ny"), MAX_CELLS);
        assert_eq!(shape(&rows), "=~~");
        assert_eq!(next_change(&rows, 1, false), None, "rows 1 and 2 are one difference");
        assert_eq!(next_change(&rows, 2, true), Some(1), "and its first row is where `N` lands");
        assert_eq!(count_changes(&rows), 1);

        let (rows, _) = compare(&lines("k\nold\nk2\nk3\ngone\nk4"), &lines("k\nnew\nk2\nk3\nk4"), MAX_CELLS);
        assert_eq!(count_changes(&rows), 2, "two blocks, three rows");
        assert_eq!(count_changes(&[]), 0);
    }

    #[test]
    fn a_file_with_a_nul_byte_is_not_read_as_text() {
        assert!(as_text(b"ok\ntext", 1024).is_some());
        assert!(as_text(b"bin\0ary", 1024).is_none());
        assert!(as_text(&[0xff, 0xfe, 0xfd], 1024).is_none(), "not UTF-8 either");
    }

    #[test]
    fn two_files_are_read_off_disk_and_compared() {
        let dir = crate::util::test_dir("diff");
        let (l, r) = (dir.join("l.txt"), dir.join("r.txt"));

        std::fs::write(&l, "a\nb\n").unwrap();
        std::fs::write(&r, "a\nb\n").unwrap();
        assert_eq!(compare_files(&l, &r, 1 << 20), Outcome::Identical);

        std::fs::write(&r, "a\nc\n").unwrap();
        let Outcome::Rows { rows, .. } = compare_files(&l, &r, 1 << 20) else {
            panic!("two text files line up");
        };
        assert_eq!(shape(&rows), "=~");

        std::fs::write(&r, [0u8, 1, 2]).unwrap();
        assert_eq!(compare_files(&l, &r, 1 << 20), Outcome::Binary { same: false });

        assert!(matches!(compare_files(&l, &dir.join("gone"), 1 << 20), Outcome::Error(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Comparing two directory trees rather than two files.
#[cfg(test)]
mod tree_tests {
    use super::*;

    fn dirs() -> (PathBuf, PathBuf) {
        let root = crate::util::test_dir("tree");
        let (l, r) = (root.join("l"), root.join("r"));
        std::fs::create_dir_all(l.join("sub")).unwrap();
        std::fs::create_dir_all(r.join("sub")).unwrap();
        (l, r)
    }

    fn rows(l: &Path, r: &Path) -> Vec<TreeRow> {
        match compare_trees(l, r) {
            Outcome::Tree { rows, .. } => rows,
            other => panic!("two folders compare as a tree, got {other:?}"),
        }
    }

    /// The same trick `shape` plays for lines: fold each row to one character so
    /// a whole comparison is one string to read.
    fn shape(rows: &[TreeRow]) -> String {
        rows.iter()
            .map(|r| match r.state {
                TreeState::LeftOnly => '<',
                TreeState::RightOnly => '>',
                TreeState::Differ => '~',
                TreeState::Same => '=',
                TreeState::Unread => '?',
            })
            .collect()
    }

    fn named(rows: &[TreeRow], rel: &str) -> TreeState {
        rows.iter()
            .find(|r| r.rel == Path::new(rel))
            .unwrap_or_else(|| panic!("no row for {rel}: {rows:?}"))
            .state
    }

    /// Each side has its own half of the limit, and a side that ran out cannot
    /// make paths it never reached look like the other side's alone (#86).
    #[test]
    fn a_cut_short_walk_does_not_invent_one_sided_rows() {
        let (l, r) = dirs();
        for i in 0..20 {
            std::fs::write(l.join(format!("f{i:02}")), b"x").unwrap();
            std::fs::write(r.join(format!("f{i:02}")), b"x").unwrap();
        }
        let Outcome::Tree { rows, truncated, .. } = compare_trees_within(&l, &r, 10) else {
            panic!("two folders compare as a tree")
        };
        assert!(truncated, "ten paths is not enough for 21 a side");
        assert!(
            rows.iter().all(|r| matches!(r.state, TreeState::Same)),
            "identical trees show only matches, never `<` or `>`: {}",
            shape(&rows)
        );
        assert!(!rows.is_empty(), "what both sides reached is still compared");

        // With room for everything, nothing is held back.
        let Outcome::Tree { truncated, .. } = compare_trees_within(&l, &r, 1000) else { panic!() };
        assert!(!truncated);
    }

    /// Links are compared by where they point, not by what is there: two
    /// links to identical files in different places differ, two to the same
    /// place match, and neither is read (45.11, #86).
    #[cfg(unix)]
    #[test]
    fn links_compare_by_where_they_point() {
        use std::os::unix::fs::symlink;
        let (l, r) = dirs();
        for side in [&l, &r] {
            std::fs::write(side.join("t1"), b"same bytes").unwrap();
            std::fs::write(side.join("t2"), b"same bytes").unwrap();
        }
        symlink("t1", l.join("ln")).unwrap();
        symlink("t2", r.join("ln")).unwrap();
        symlink("t1", l.join("same")).unwrap();
        symlink("t1", r.join("same")).unwrap();
        symlink("sub", l.join("dl")).unwrap();
        symlink("sub", r.join("dl")).unwrap();
        let rows = rows(&l, &r);
        assert_eq!(named(&rows, "ln"), TreeState::Differ, "same bytes behind, different places");
        assert_eq!(named(&rows, "same"), TreeState::Same);
        assert_eq!(named(&rows, "dl"), TreeState::Same, "a link to a directory, not `?`");
    }

    /// The Unix half of Q26, which also covers a relative link against an
    /// absolute one: both copies' links go to their own `t1`, one written as
    /// `t1` and one as the full path, and they match. A link out of the tree
    /// is still compared as written, so two links to one shared place match
    /// and two to different places do not.
    #[cfg(unix)]
    #[test]
    fn links_into_their_own_copy_match_and_links_out_do_not_move() {
        use std::os::unix::fs::symlink;
        let (l, r) = dirs();
        let out = crate::util::test_dir("diff-links-out");
        std::fs::write(out.join("x"), b"x").unwrap();
        std::fs::write(out.join("y"), b"y").unwrap();
        for side in [&l, &r] {
            std::fs::write(side.join("t1"), b"t").unwrap();
            std::fs::create_dir_all(side.join("sub")).unwrap();
        }
        symlink(l.join("t1"), l.join("abs")).unwrap();
        symlink(r.join("t1"), r.join("abs")).unwrap();
        symlink("t1", l.join("mixed")).unwrap();
        symlink(r.join("t1"), r.join("mixed")).unwrap();
        symlink("../t1", l.join("sub").join("up")).unwrap();
        symlink(r.join("t1"), r.join("sub").join("up")).unwrap();
        symlink(out.join("x"), l.join("shared")).unwrap();
        symlink(out.join("x"), r.join("shared")).unwrap();
        symlink(out.join("x"), l.join("apart")).unwrap();
        symlink(out.join("y"), r.join("apart")).unwrap();
        let rows = rows(&l, &r);
        assert_eq!(named(&rows, "abs"), TreeState::Same, "absolute, each into its own copy");
        assert_eq!(named(&rows, "mixed"), TreeState::Same, "relative against absolute, one place");
        assert_eq!(named(&rows, "sub/up"), TreeState::Same, "`..` resolved from the link's folder");
        assert_eq!(named(&rows, "shared"), TreeState::Same, "one place outside both");
        assert_eq!(named(&rows, "apart"), TreeState::Differ, "two places outside");
    }

    /// The Windows half, with junctions, which need no privilege to make: a
    /// link to a directory used to be opened as a file, fail, and read as
    /// "too big to read" (#86).
    #[cfg(windows)]
    #[test]
    fn junctions_compare_by_where_they_point() {
        let (l, r) = dirs();
        let junction = |at: &Path, to: &Path| {
            let ok = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(at)
                .arg(to)
                .output()
                .is_ok_and(|o| o.status.success());
            assert!(ok, "mklink /J {} {}", at.display(), to.display());
        };
        std::fs::create_dir_all(l.join("a")).unwrap();
        std::fs::create_dir_all(r.join("a")).unwrap();
        std::fs::create_dir_all(r.join("b")).unwrap();
        junction(&l.join("j"), &l.join("a"));
        junction(&r.join("j"), &r.join("b"));
        let rows = rows(&l, &r);
        let j = named(&rows, "j");
        assert_ne!(j, TreeState::Unread, "a junction is a link, not an unreadable file");
        assert_eq!(j, TreeState::Differ, "the two point to different places");
    }

    /// Two copies of one tree whose junctions point into their own copy: the
    /// targets are absolute and so differ as text, but they are the same
    /// place in each tree, and 45.5 says two copies read as `=` (Q26, #98).
    #[cfg(windows)]
    #[test]
    fn junctions_into_their_own_copy_match() {
        let (l, r) = dirs();
        for side in [&l, &r] {
            std::fs::create_dir_all(side.join("t1")).unwrap();
            let ok = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(side.join("ln"))
                .arg(side.join("t1"))
                .output()
                .is_ok_and(|o| o.status.success());
            assert!(ok, "mklink /J under {}", side.display());
        }
        assert_eq!(named(&rows(&l, &r), "ln"), TreeState::Same);
    }

    /// The footer's numbers are counted once, when the comparison is built, so
    /// they have to agree with the rows they came from -- the renderer no longer
    /// walks the rows to check, which is the point.
    #[test]
    fn the_counts_agree_with_the_rows() {
        let (l, r) = dirs();
        std::fs::write(l.join("only-left"), b"x").unwrap();
        std::fs::write(r.join("only-right"), b"x").unwrap();
        std::fs::write(l.join("same"), b"ab").unwrap();
        std::fs::write(r.join("same"), b"ab").unwrap();
        std::fs::write(l.join("differ"), b"ab").unwrap();
        std::fs::write(r.join("differ"), b"cd").unwrap();
        let Outcome::Tree { rows, counts, .. } = compare_trees(&l, &r) else {
            panic!("two folders compare as a tree")
        };
        assert_eq!(counts, TreeCounts::of(&rows), "the carried counts are the rows' own");
        assert_eq!(counts.left_only, 1);
        assert_eq!(counts.right_only, 1);
        assert_eq!(counts.differ, 1);
        assert_eq!(counts.same, 2, "`same`, plus the `sub` both sides have");
        assert_eq!(counts.unread, 0);
        // And they add up to every row, so the footer accounts for all of them.
        let total = counts.left_only + counts.right_only + counts.differ + counts.same + counts.unread;
        assert_eq!(total, rows.len());
    }

    #[test]
    fn each_side_reports_what_only_it_has() {
        let (l, r) = dirs();
        std::fs::write(l.join("only-left"), b"x").unwrap();
        std::fs::write(r.join("only-right"), b"x").unwrap();
        std::fs::write(l.join("both"), b"same").unwrap();
        std::fs::write(r.join("both"), b"same").unwrap();
        let rows = rows(&l, &r);
        assert_eq!(named(&rows, "only-left"), TreeState::LeftOnly);
        assert_eq!(named(&rows, "only-right"), TreeState::RightOnly);
        assert_eq!(named(&rows, "both"), TreeState::Same);
        assert_eq!(named(&rows, "sub"), TreeState::Same, "a folder both sides have matches");
        // Sorted by path, so the shape is stable: both, only-left, only-right, sub.
        assert_eq!(shape(&rows), "=<>=");
    }

    /// The cheap test carries almost every row: two files of different lengths
    /// differ, and nothing is read to know it.
    #[test]
    fn a_different_length_is_a_difference_without_reading() {
        let (l, r) = dirs();
        std::fs::write(l.join("f"), b"short").unwrap();
        std::fs::write(r.join("f"), b"much longer").unwrap();
        assert_eq!(named(&rows(&l, &r), "f"), TreeState::Differ);
    }

    /// Same length is the case that has to be read, and the answer must not be
    /// "same" just because the sizes match.
    #[test]
    fn the_same_length_is_settled_by_reading() {
        let (l, r) = dirs();
        std::fs::write(l.join("same"), b"abcdef").unwrap();
        std::fs::write(r.join("same"), b"abcdef").unwrap();
        std::fs::write(l.join("diff"), b"abcdef").unwrap();
        std::fs::write(r.join("diff"), b"abcdeX").unwrap();
        let rows = rows(&l, &r);
        assert_eq!(named(&rows, "same"), TreeState::Same);
        assert_eq!(named(&rows, "diff"), TreeState::Differ, "same size, different bytes");
    }

    #[test]
    fn nested_paths_are_paired_by_what_they_are_inside_the_roots() {
        let (l, r) = dirs();
        std::fs::write(l.join("sub").join("deep"), b"one").unwrap();
        std::fs::write(r.join("sub").join("deep"), b"two").unwrap();
        let rows = rows(&l, &r);
        let rel = std::path::Path::new("sub").join("deep");
        let got = rows.iter().find(|x| x.rel == rel).expect("the nested file is paired");
        assert_eq!(got.state, TreeState::Differ);
        assert!(!got.dir);
    }

    /// A folder on one side where the other has a file is a difference, and must
    /// not be read as one.
    #[test]
    fn a_folder_against_a_file_is_a_difference() {
        let (l, r) = dirs();
        std::fs::create_dir_all(l.join("x")).unwrap();
        std::fs::write(r.join("x"), b"file").unwrap();
        assert_eq!(named(&rows(&l, &r), "x"), TreeState::Differ);
    }

    #[test]
    fn two_copies_of_one_tree_are_all_matches() {
        let (l, r) = dirs();
        for d in [&l, &r] {
            std::fs::write(d.join("a"), b"aa").unwrap();
            std::fs::write(d.join("sub").join("b"), b"bb").unwrap();
        }
        let rows = rows(&l, &r);
        assert_eq!(shape(&rows), "===", "a, sub, sub/b");
        assert!(rows.iter().all(|r| matches!(r.state, TreeState::Same)));
    }

    #[test]
    fn a_missing_root_is_an_error_rather_than_an_empty_answer() {
        let (l, _) = dirs();
        let gone = l.parent().unwrap().join("never-made");
        assert!(matches!(compare_trees(&l, &gone), Outcome::Error(_)));
    }

    /// `same_bytes` stops at the first block that differs, so this is the test
    /// that it compares content rather than just lengths.
    #[test]
    fn the_byte_comparison_finds_a_difference_anywhere() {
        let (l, r) = dirs();
        let mut a = vec![b'x'; 200_000];
        let mut b = a.clone();
        b[199_999] = b'y';
        std::fs::write(l.join("big"), &a).unwrap();
        std::fs::write(r.join("big"), &b).unwrap();
        assert_eq!(named(&rows(&l, &r), "big"), TreeState::Differ, "differs in the last byte");
        a[0] = b'z';
        std::fs::write(l.join("big"), &a).unwrap();
        assert_eq!(named(&rows(&l, &r), "big"), TreeState::Differ, "and in the first");
    }
}
