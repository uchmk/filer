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
    Error(String),
}

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
    Line { no: i + 1, text: text.to_owned() }
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
        rows.push(Row {
            left: dels.get(k).map(|&i| line(off + i, &a[i])),
            right: ins.get(k).map(|&j| line(off + j, &b[j])),
            same: false,
        });
    }
    dels.clear();
    ins.clear();
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

// ------------------------------------------------------------------- worker

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
                    let outcome = compare_files(&req.left, &req.right, req.max_bytes);
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
    }

    #[test]
    fn a_file_with_a_nul_byte_is_not_read_as_text() {
        assert!(as_text(b"ok\ntext", 1024).is_some());
        assert!(as_text(b"bin\0ary", 1024).is_none());
        assert!(as_text(&[0xff, 0xfe, 0xfd], 1024).is_none(), "not UTF-8 either");
    }

    #[test]
    fn two_files_are_read_off_disk_and_compared() {
        let dir = std::env::temp_dir().join(format!("filer-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
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
