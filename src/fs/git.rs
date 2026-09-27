//! Git status for the directory on screen.
//!
//! `git` itself does the reading: one `git status --porcelain=v1 -z` names
//! everything it has to say about the tree, and parsing that is the whole job.
//! No library is linked in, so nothing here needs a C toolchain, the build
//! cross-compiles as before, and whatever version of git the user has is the
//! one that answers.
//!
//! The worker is the usual shape: newest-wins, so a fast walk through
//! directories leaves at most one status in flight and the answers that no
//! longer matter are dropped on arrival.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

/// How long a "not a repository" answer is trusted. Long enough that a rescan
/// storm asks once, short enough that `git init` is noticed.
const NOT_REPO_TTL: Duration = Duration::from_secs(10);

/// What git says about one row of the listing.
///
/// The order they are written in is the order of interest, and the derived
/// `Ord` is what rolls a directory up: it carries the strongest state of
/// anything inside it, so a conflict outranks a change and a change outranks
/// an addition.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum State {
    /// Tracked and unchanged: nothing is drawn for these.
    #[default]
    Clean,
    /// Not in the index and not ignored.
    Untracked,
    /// Changed in the index, matching the worktree.
    Staged,
    /// Changed in the worktree since the index.
    Modified,
    /// Gone from the worktree, or a staged delete.
    Deleted,
    /// Both sides changed; the merge is unfinished.
    Conflict,
}

impl State {
    /// The one character the list draws. `Clean` has none.
    pub fn mark(self) -> Option<char> {
        match self {
            State::Clean => None,
            State::Untracked => Some('?'),
            State::Staged => Some('+'),
            State::Modified => Some('M'),
            State::Deleted => Some('D'),
            State::Conflict => Some('!'),
        }
    }
}

/// The states of one directory's rows, by the name each row carries.
#[derive(Debug, Default)]
pub struct Status {
    pub states: HashMap<String, State>,
    /// What every row is, before `states` says otherwise. Git reports an
    /// untracked directory as one record rather than walking it, so a listing
    /// *inside* one is told about the directory and nothing else — and
    /// everything in it is untracked too.
    pub all: State,
    /// The branch HEAD is on, for the status bar. Empty outside a repository,
    /// and `HEAD` when it is detached, which is what git itself answers.
    pub branch: String,
}

impl Status {
    /// What to draw for one row of the listing.
    pub fn get(&self, name: &str) -> State {
        self.states.get(name).copied().unwrap_or(self.all)
    }
}

#[derive(Debug)]
pub struct Report {
    pub dir: PathBuf,
    /// Empty when the directory is not in a repository, or git could not run.
    pub status: Status,
}

pub struct Git {
    tx: Sender<PathBuf>,
    pub rx: Receiver<Report>,
}

impl Git {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, req_rx) = crossbeam_channel::unbounded::<PathBuf>();
        let (res_tx, rx) = crossbeam_channel::unbounded::<Report>();
        std::thread::Builder::new()
            .name("git".into())
            .spawn(move || {
                // Most of the disk is not a repository, and a rescan asks
                // again every time. Remembering the noes keeps `git` from
                // being started to hear the same one; the note is short-lived
                // so a `git init` still shows up.
                let mut not_repo: HashMap<PathBuf, Instant> = HashMap::new();
                while let Ok(mut dir) = req_rx.recv() {
                    // Newest wins: walking through directories leaves a trail
                    // of requests nobody is waiting for any more.
                    while let Ok(newer) = req_rx.try_recv() {
                        dir = newer;
                    }
                    let fresh_no = not_repo
                        .get(&dir)
                        .is_some_and(|at| at.elapsed() < NOT_REPO_TTL);
                    let status = match fresh_no {
                        true => Status::default(),
                        false => match status(&dir) {
                            Some(st) => {
                                not_repo.remove(&dir);
                                st
                            }
                            None => {
                                not_repo.insert(dir.clone(), Instant::now());
                                // A walk through a big tree would otherwise
                                // remember every directory in it.
                                if not_repo.len() > 64 {
                                    not_repo.retain(|_, at| at.elapsed() < NOT_REPO_TTL);
                                }
                                Status::default()
                            }
                        },
                    };
                    if res_tx.send(Report { dir, status }).is_err() {
                        return;
                    }
                    wake();
                }
            })
            .expect("spawn git worker");
        Self { tx, rx }
    }

    pub fn request(&self, dir: PathBuf) {
        let _ = self.tx.send(dir);
    }
}

/// Ask git about `dir`. `None` when it is not a repository, git is not
/// installed, or it failed — all of which mean the same thing to the list:
/// draw no marks.
fn status(dir: &Path) -> Option<Status> {
    // Where `dir` sits inside the repository, and what branch it is on. One
    // call answers both, and answers "is this a repository at all" by failing.
    let head = run(dir, &["rev-parse", "--show-prefix", "--abbrev-ref", "HEAD"])?;
    let mut lines = head.lines();
    let prefix = lines.next().unwrap_or_default().to_owned();
    // A repository with no commits yet has no HEAD to name.
    let branch = lines.next().unwrap_or_default().to_owned();

    // `-z` because a path may hold anything, including a newline.
    // `--no-renames` keeps every record to one path, so a rename reads as the
    // delete and the add it is on screen.
    // `-unormal` collapses an untracked directory into one entry rather than
    // walking all of it, which is both faster and what the row wants.
    let out = run(
        dir,
        &["status", "--porcelain=v1", "-z", "--no-renames", "-unormal", "--", "."],
    )?;
    let mut st = parse(&out, &prefix);
    st.branch = branch;
    Some(st)
}

fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    // A repository owned by someone else makes git refuse with a long note
    // about `safe.directory`; the pager and the terminal are no use either.
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Without this the child flashes a console window on every listing.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Turn porcelain v1 into one state per row of the listing.
///
/// Records are `XY <path>` joined by NUL, with paths relative to the top of
/// the repository. `prefix` is where the listed directory sits under that top,
/// so stripping it leaves a path relative to the listing; everything below the
/// first component of that path rolls up into the directory holding it.
fn parse(out: &str, prefix: &str) -> Status {
    let mut st = Status::default();
    for record in out.split('\0') {
        // `XY ` then the path: four characters at the very least.
        if record.len() < 4 {
            continue;
        }
        let (code, path) = record.split_at(3);
        let mut chars = code.chars();
        let (x, y) = (chars.next().unwrap_or(' '), chars.next().unwrap_or(' '));
        let Some(rest) = path.strip_prefix(prefix) else { continue };
        let state = classify(x, y);
        if state == State::Clean {
            continue;
        }
        // Nothing left after the prefix: git is naming the listed directory
        // itself, so it is speaking about every row at once.
        let Some(name) = rest.split('/').next().filter(|n| !n.is_empty()) else {
            st.all = st.all.max(state);
            continue;
        };
        let entry = st.states.entry(name.to_owned()).or_insert(State::Clean);
        *entry = (*entry).max(state);
    }
    st
}

/// The two status letters, as one state. `X` is the index against HEAD and `Y`
/// is the worktree against the index, so a file that is both staged and
/// changed since reads as changed: that is the part still to be committed.
fn classify(x: char, y: char) -> State {
    match (x, y) {
        ('?', '?') => State::Untracked,
        // Ignored, which is only reported when asked for.
        ('!', '!') => State::Clean,
        // Any `U`, or the two same-letter pairs, is an unfinished merge.
        ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => State::Conflict,
        (_, 'D') => State::Deleted,
        (_, 'M') | (_, 'T') | (_, 'R') | (_, 'C') | (_, 'A') => State::Modified,
        ('D', _) => State::Deleted,
        (' ', _) => State::Clean,
        _ => State::Staged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Porcelain records are NUL-joined, and git leaves a trailing NUL.
    fn porcelain(records: &[&str]) -> String {
        let mut s = records.join("\0");
        s.push('\0');
        s
    }

    #[test]
    fn the_two_letters_say_what_happened() {
        assert_eq!(classify('?', '?'), State::Untracked);
        assert_eq!(classify('!', '!'), State::Clean);
        // Staged only.
        assert_eq!(classify('A', ' '), State::Staged);
        assert_eq!(classify('M', ' '), State::Staged);
        // Changed in the worktree, whether or not the index has it too.
        assert_eq!(classify(' ', 'M'), State::Modified);
        assert_eq!(classify('M', 'M'), State::Modified);
        assert_eq!(classify(' ', 'D'), State::Deleted);
        assert_eq!(classify('D', ' '), State::Deleted);
        // An unfinished merge outranks everything.
        assert_eq!(classify('U', 'U'), State::Conflict);
        assert_eq!(classify('A', 'A'), State::Conflict);
        assert_eq!(classify('A', 'U'), State::Conflict);
    }

    #[test]
    fn a_listing_gets_one_state_per_row() {
        let out = porcelain(&[" M src/app.rs", "?? notes.txt", "A  Cargo.toml"]);
        let st = parse(&out, "");
        assert_eq!(st.get("notes.txt"), State::Untracked);
        assert_eq!(st.get("Cargo.toml"), State::Staged);
        // The file is inside `src`, so the row that shows is the directory.
        assert_eq!(st.get("src"), State::Modified);
        assert_eq!(st.states.get("src/app.rs"), None, "rows are named, not pathed");
        // Nothing is said about what git did not mention.
        assert_eq!(st.get("README.md"), State::Clean);
    }

    /// A directory shows the strongest thing under it: one conflict is what
    /// you need to see, however many tidy files sit beside it.
    #[test]
    fn a_directory_carries_the_strongest_state_inside_it() {
        let out = porcelain(&["A  src/new.rs", " M src/app.rs", "UU src/merge.rs"]);
        assert_eq!(parse(&out, "").get("src"), State::Conflict);

        let out = porcelain(&["A  src/new.rs", " M src/app.rs"]);
        assert_eq!(parse(&out, "").get("src"), State::Modified);

        let out = porcelain(&["A  src/new.rs", "A  src/other.rs"]);
        assert_eq!(parse(&out, "").get("src"), State::Staged);
    }

    /// Paths come relative to the top of the repository, but the listing is
    /// somewhere below it.
    #[test]
    fn the_prefix_puts_the_paths_back_in_the_listed_directory() {
        let out = porcelain(&[" M src/fs/git.rs", "?? src/fs/new.rs", " M README.md"]);
        let st = parse(&out, "src/fs/");
        assert_eq!(st.get("git.rs"), State::Modified);
        assert_eq!(st.get("new.rs"), State::Untracked);
        // Outside the listed directory: not this listing's business.
        assert_eq!(st.get("README.md"), State::Clean);
        assert_eq!(st.states.len(), 2);
    }

    /// Git reports an untracked directory as one record instead of walking it,
    /// so a listing inside one hears only about itself — and everything in it
    /// is untracked too.
    #[test]
    fn inside_an_untracked_directory_every_row_is_untracked() {
        let st = parse(&porcelain(&["?? sub/"]), "sub/");
        assert_eq!(st.all, State::Untracked);
        assert_eq!(st.get("anything.txt"), State::Untracked);
        assert!(st.states.is_empty(), "nothing was named row by row");

        // Seen from above, the same record is one row and says nothing about
        // the rest of the listing.
        let st = parse(&porcelain(&["?? sub/"]), "");
        assert_eq!(st.get("sub"), State::Untracked);
        assert_eq!(st.get("beside.txt"), State::Clean);
    }

    #[test]
    fn a_path_holding_a_newline_survives_being_split() {
        let out = porcelain(&["?? odd\nname.txt", " M plain.txt"]);
        let st = parse(&out, "");
        assert_eq!(st.get("odd\nname.txt"), State::Untracked);
        assert_eq!(st.get("plain.txt"), State::Modified);
    }

    /// The end of the run is real git output, which is the only way to know
    /// the flags and the parsing agree with the version installed.
    #[test]
    fn a_real_repository_reports_what_was_done_to_it() {
        // Plain characters only: `"` and `:` are legal in a Unix path and not
        // in a Windows one, and CI is where that difference shows up.
        let root = crate::util::test_dir("git-status");
        std::fs::create_dir_all(root.join("sub")).unwrap();

        let git = |args: &[&str]| {
            Command::new("git").arg("-C").arg(&root).args(args).output()
        };
        let Ok(out) = git(&["init", "-q"]) else {
            eprintln!("git is not installed; skipping");
            return;
        };
        if !out.status.success() {
            eprintln!("git init failed; skipping");
            return;
        }
        let _ = git(&["config", "user.email", "t@example.com"]);
        let _ = git(&["config", "user.name", "t"]);

        std::fs::write(root.join("committed.txt"), b"one").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "first"]);

        std::fs::write(root.join("committed.txt"), b"two").unwrap();
        std::fs::write(root.join("fresh.txt"), b"new").unwrap();
        std::fs::write(root.join("staged.txt"), b"staged").unwrap();
        std::fs::write(root.join("sub").join("deep.txt"), b"deep").unwrap();
        let _ = git(&["add", "staged.txt"]);

        let st = status(&root).expect("the directory is a repository");
        assert_eq!(st.get("committed.txt"), State::Modified);
        assert_eq!(st.get("fresh.txt"), State::Untracked);
        assert_eq!(st.get("staged.txt"), State::Staged);
        // An untracked directory arrives as the one row that shows it.
        assert_eq!(st.get("sub"), State::Untracked);
        assert_eq!(st.all, State::Clean, "the repository root is tracked");

        // A directory below the top reports its own rows, not the top's. This
        // one is untracked, so git names it whole and every row follows.
        let st = status(&root.join("sub")).expect("still the same repository");
        assert_eq!(st.get("deep.txt"), State::Untracked);
        assert_eq!(st.get("committed.txt"), State::Untracked, "the whole directory is");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn somewhere_that_is_not_a_repository_says_nothing() {
        let dir = crate::util::test_dir("git-bare");
        std::fs::create_dir_all(&dir).unwrap();
        // `status` gives `None` whether git refused or is not installed, and
        // the caller draws nothing either way.
        assert!(status(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
