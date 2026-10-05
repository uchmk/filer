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

/// The last commit that touched `path`, and how many did.
///
/// One `git log` for both: the first line is the newest commit, and the number
/// of lines is the count. It is capped at [`LOG_CAP`] so that asking about a
/// directory near the root of a long history reads a page rather than all of
/// it -- a count past the cap is reported as "and more" rather than a lie.
///
/// `None` when git is absent, the path is outside a repository, or nothing in
/// the history touches it (a file that has never been committed).
pub(crate) fn last_commit(path: &Path) -> Option<Commit> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let mut args = vec![
        "log".to_string(),
        format!("-n{LOG_CAP}"),
        // NUL cannot appear in any of the four, so nothing here needs quoting.
        "--format=%h%x00%aI%x00%an%x00%s".to_string(),
    ];
    // A directory is asked about as `.`, because `run` has already put git
    // inside it with `-C`. Until v0.47.32 the pathspec was dropped entirely
    // here, so `git -C sub log` answered for the whole repository: a directory
    // reported whatever commit was newest anywhere, and counted every commit
    // in the history. `status` a few lines up has always passed `-- .`; this is
    // the same thing.
    let spec = if path.is_dir() { Some(".".to_string()) } else { name };
    if let Some(spec) = spec {
        args.push("--".to_string());
        args.push(spec);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run(dir, &args)?;
    let mut lines = out.lines().filter(|l| !l.is_empty());
    let first = lines.next()?;
    let count = 1 + lines.count();
    let mut f = first.split('\0');
    Some(Commit {
        hash: f.next()?.to_string(),
        when: f.next()?.to_string(),
        author: f.next()?.to_string(),
        subject: f.next().unwrap_or_default().to_string(),
        count,
        capped: count >= LOG_CAP,
    })
}

/// How far back [`last_commit`] counts before it stops and says "and more".
const LOG_CAP: usize = 50;

/// How the commit that last touched a path arrived on the current branch.
///
/// This closes the question [`last_commit`] opens but cannot answer: the hash
/// says *when* the file changed, not *why it was taken*. Where a repository is
/// merged with merge commits -- which this one is, deliberately, so that the
/// joins stay visible -- the merge that brought a commit in **carries the pull
/// request number in its own subject**, so the answer is already on disk.
///
/// Nothing here reaches the network. No token to keep out of `filer env`, no
/// request that might never return, and the two rules the Git section is held
/// to keep applying word for word: 46.8 (no pause where there is no
/// repository) and 46.9 (silence where there is no `git`), because this is one
/// more `git log` and nothing else.
pub(crate) struct Origin {
    /// The merge commit, short.
    pub merge: String,
    /// The pull request its subject names, when it names one.
    pub pr: Option<u32>,
    /// The branch it took in, when its subject names one.
    pub branch: Option<String>,
}

/// The merge that first took `commit` into `HEAD`, if one did.
///
/// `--ancestry-path` keeps only commits descended from `commit` *and* ancestral
/// to `HEAD`; among those, the oldest merge is the one that brought it in.
/// `rev-list` writes newest first, so that is the last line.
///
/// `None` is the honest answer in two ordinary cases, and neither is an error:
/// a commit pushed straight to the branch never went through a merge, and a
/// commit on a branch not merged yet has not arrived anywhere to be asked
/// about.
pub(crate) fn origin(path: &Path, commit: &str) -> Option<Origin> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let range = format!("{commit}..HEAD");
    let out = run(dir, &["rev-list", "--merges", "--ancestry-path", &range])?;
    let sha = out.lines().rev().find(|l| !l.is_empty())?;
    // Being *after* a commit is not the same as having *brought it in*. A merge
    // took the commit in only if the commit was not already on the branch the
    // merge targeted -- that is, not reachable from the merge's first parent.
    // Without this, a commit pushed straight onto the mainline is credited to
    // whichever merge happened next, and the row fills in with a real merge and
    // a real number that have nothing to do with the file. That is the same
    // silent shape as the missing pathspec in v0.47.32, and the test here
    // caught it the same way: by building a history where the two disagree.
    let first_parent = format!("{sha}^1");
    if run(dir, &["merge-base", "--is-ancestor", commit, &first_parent]).is_some() {
        return None;
    }
    let line = run(dir, &["log", "-n1", "--format=%h%x00%s", sha])?;
    let (merge, subject) = line.trim_end().split_once('\0')?;
    let (pr, branch) = merge_subject(subject);
    Some(Origin { merge: merge.to_string(), pr, branch })
}

/// The web page of pull request `pr`, when `origin` is on GitHub.
///
/// Built from `remote.origin.url` alone: nothing is fetched, so the Git
/// section's promise not to touch the network (46.16) holds until the page is
/// actually opened, which is the user's own `<Enter>` (Q20). Other forges are
/// not guessed at -- a wrong URL is worse than none.
pub(crate) fn pull_request_url(path: &Path, pr: u32) -> Option<String> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let remote = run(dir, &["config", "--get", "remote.origin.url"])?;
    github_pr_url(remote.trim(), pr)
}

/// `owner/repo` out of the three ways a GitHub remote is written, and the pull
/// request's page from it.
fn github_pr_url(remote: &str, pr: u32) -> Option<String> {
    let path = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("http://github.com/"))
        .or_else(|| remote.strip_prefix("git@github.com:"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}/pull/{pr}"))
}

/// The default branch `commit` has not reached yet, as `origin/main`.
///
/// Answers the question "Came in via" leaves open when it is empty: a commit
/// pushed straight onto the mainline and one on a branch nobody has merged
/// look the same without it (Q19). Only asked where the clone knows its
/// default branch (`origin/HEAD`); guessing `main` would mark every commit of a
/// `master` repository as unmerged. And only from local refs -- as current as
/// the last fetch, and no newer.
pub(crate) fn not_merged_into(path: &Path, commit: &str) -> Option<NotMerged> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let default = run(dir, &["rev-parse", "--abbrev-ref", "origin/HEAD"]).map(|d| d.trim().to_owned());
    let default = match default.filter(|d| !d.is_empty() && d != "origin/HEAD") {
        Some(d) => d,
        // A clone that has an `origin` but never learned its default branch
        // (`git remote set-head origin -a` fixes it): the absence of a
        // `Not merged` row there did not mean "merged" (#212). A repository
        // with no `origin` at all has nothing to be merged into.
        None => return run(dir, &["remote", "get-url", "origin"]).map(|_| NotMerged::Unknown),
    };
    // `--is-ancestor` exits 0 when merged, 1 when not; `run` reads anything
    // but 0 as "no answer", so a failure here also reads as "not merged". The
    // commit came from `git log` a moment ago, so it exists.
    match run(dir, &["merge-base", "--is-ancestor", commit, &default]) {
        Some(_) => None,
        None => Some(NotMerged::In(default)),
    }
}

/// What [`not_merged_into`] found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NotMerged {
    /// Not in this branch yet.
    In(String),
    /// The default branch is not known here, so nothing can be said.
    Unknown,
}

/// The pull request number and branch a merge commit's subject names.
///
/// Three shapes turn up: GitHub's `Merge pull request #61 from owner/branch`,
/// GitLab's `Merge branch 'x' into 'y'`, and git's own `Merge branch 'x'`.
/// Anything else yields neither and the caller still has the merge's hash,
/// which is the point -- a forge this does not know about degrades to showing
/// the merge rather than to showing nothing.
fn merge_subject(subject: &str) -> (Option<u32>, Option<String>) {
    if let Some(rest) = subject.strip_prefix("Merge pull request #") {
        let (num, rest) = rest.split_once(' ').unwrap_or((rest, ""));
        // `from owner/branch`. The owner is noise next to the branch, but a
        // branch may hold slashes of its own, so only the first one is cut.
        let branch = rest
            .strip_prefix("from ")
            .and_then(|o| o.split_once('/'))
            .map(|(_, b)| b.to_string())
            .filter(|b| !b.is_empty());
        return (num.parse().ok(), branch);
    }
    let branch = subject
        .strip_prefix("Merge branch '")
        .and_then(|r| r.split_once('\''))
        .map(|(b, _)| b.to_string())
        .filter(|b| !b.is_empty());
    (None, branch)
}

/// What `git log` had to say about one path.
pub(crate) struct Commit {
    pub hash: String,
    /// ISO-8601 with the author's own offset, as `%aI` writes it.
    pub when: String,
    pub author: String,
    pub subject: String,
    pub count: usize,
    /// The count hit [`LOG_CAP`] and is a floor, not a total.
    pub capped: bool,
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

    /// Every way a GitHub remote is written leads to the same page, and a
    /// remote that is not GitHub leads nowhere rather than somewhere wrong.
    #[test]
    fn a_github_remote_gives_the_pull_request_page() {
        let page = Some("https://github.com/uchmk/filer/pull/71".to_string());
        for remote in [
            "https://github.com/uchmk/filer",
            "https://github.com/uchmk/filer.git",
            "https://github.com/uchmk/filer/",
            "git@github.com:uchmk/filer.git",
            "ssh://git@github.com/uchmk/filer.git",
        ] {
            assert_eq!(github_pr_url(remote, 71), page, "{remote}");
        }
        for remote in ["https://gitlab.com/uchmk/filer.git", "git@github.com:uchmk", "https://github.com/uchmk/filer/tree/main", ""] {
            assert_eq!(github_pr_url(remote, 71), None, "{remote}");
        }
    }

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

    /// The four fields survive the round trip, and the count is the count.
    ///
    /// The format string is the part that breaks silently: a `%h%x00%aI...`
    /// that loses a field still parses into a `Commit` with the wrong things
    /// in the wrong rows, so the assertions name what each one should hold
    /// rather than only that four arrived.
    #[test]
    fn the_last_commit_on_a_path_is_read_back_whole() {
        let root = crate::util::test_dir("git-log");
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(&root).args(args).output();
        let Ok(out) = git(&["init", "-q"]) else {
            eprintln!("git is not installed; skipping");
            return;
        };
        if !out.status.success() {
            eprintln!("git init failed; skipping");
            return;
        }
        let _ = git(&["config", "user.email", "t@example.com"]);
        let _ = git(&["config", "user.name", "Ada"]);

        let file = root.join("a.txt");
        std::fs::write(&file, b"one").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "the first one"]);
        std::fs::write(&file, b"two").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "the second one"]);

        let c = last_commit(&file).expect("a committed file has a last commit");
        assert_eq!(c.subject, "the second one", "the newest is first, not the oldest");
        assert_eq!(c.author, "Ada");
        assert_eq!(c.count, 2, "both commits touched it");
        assert!(!c.capped, "two is not the cap");
        assert!(!c.hash.is_empty() && c.hash.len() <= 40, "an abbreviated hash: {:?}", c.hash);
        assert!(c.when.starts_with("20") && c.when.contains('T'), "ISO-8601: {:?}", c.when);

        // A file git has never seen is not a commit with empty fields.
        std::fs::write(root.join("b.txt"), b"never committed").unwrap();
        assert!(last_commit(&root.join("b.txt")).is_none(), "nothing in the history touches it");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A directory answers for itself, not for the repository around it.
    ///
    /// The bug this covers was silent in the worst way: the row was filled in,
    /// with a real commit and a real count, just the wrong ones. Section 46 on
    /// the Windows machine only caught it because the fixtures repository had
    /// been given commits that touch nothing in the directory being asked
    /// about -- with a history where the newest commit happens to touch
    /// everything, a missing pathspec and a correct one agree.
    ///
    /// So the repository here is built to disagree: `top.txt` is committed
    /// last and never touches `sub`, and `sub` has two commits of its own.
    /// Without the pathspec this reads `sub` as `top.txt`'s commit, and counts
    /// three.
    #[test]
    fn a_directory_reports_the_last_commit_inside_it() {
        let root = crate::util::test_dir("git-log-dir");
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(&root).args(args).output();
        let Ok(out) = git(&["init", "-q"]) else {
            eprintln!("git is not installed; skipping");
            return;
        };
        if !out.status.success() {
            eprintln!("git init failed; skipping");
            return;
        }
        let _ = git(&["config", "user.email", "t@example.com"]);
        let _ = git(&["config", "user.name", "Grace"]);

        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("inner.txt"), b"one").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "sub: the first"]);
        std::fs::write(sub.join("inner.txt"), b"two").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "sub: the second"]);

        // Newest in the repository, and nothing to do with `sub`.
        std::fs::write(root.join("top.txt"), b"elsewhere").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "top: unrelated to sub"]);

        let c = last_commit(&sub).expect("a directory with history has a last commit");
        assert_eq!(c.subject, "sub: the second", "the newest that touched `sub`, not the repo's");
        assert_eq!(c.count, 2, "the two commits inside `sub`, not all three");

        // And the file inside it still answers for itself.
        let inner = last_commit(&sub.join("inner.txt")).expect("the file has history too");
        assert_eq!(inner.subject, "sub: the second");
        assert_eq!(inner.count, 2);

        // The root does see everything, because everything is inside it.
        let top = last_commit(&root).expect("the root has history");
        assert_eq!(top.subject, "top: unrelated to sub", "the root's newest is the repo's");
        assert_eq!(top.count, 3);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The four subject shapes, and the one that is none of them.
    ///
    /// This runs on both platforms on purpose: what a forge writes in a merge
    /// subject has nothing to do with the operating system, so a
    /// `#[cfg(windows)]` here would only mean Linux stopped checking it.
    #[test]
    fn a_merge_subject_names_its_pull_request() {
        // GitHub, which is where the number comes from.
        let (pr, branch) = merge_subject("Merge pull request #61 from uchmk/claude/task-09i0cs");
        assert_eq!(pr, Some(61));
        assert_eq!(
            branch.as_deref(),
            Some("claude/task-09i0cs"),
            "the owner is cut, the branch's own slashes are kept"
        );

        // The number is all that is there when a fork is not named.
        let (pr, branch) = merge_subject("Merge pull request #7");
        assert_eq!(pr, Some(7));
        assert_eq!(branch, None);

        // GitLab, and git's own default: a branch but no number.
        let (pr, branch) = merge_subject("Merge branch 'feature/x' into 'main'");
        assert_eq!(pr, None);
        assert_eq!(branch.as_deref(), Some("feature/x"));
        let (pr, branch) = merge_subject("Merge branch 'topic'");
        assert_eq!(pr, None);
        assert_eq!(branch.as_deref(), Some("topic"));

        // An ordinary commit subject yields neither, rather than a stray parse.
        // `origin` still shows the merge's hash, so the row is not empty.
        assert_eq!(merge_subject("v0.48.0: add something"), (None, None));
        assert_eq!(merge_subject("Merge pull request #x from a/b").0, None);
    }

    /// #212: a clone with an `origin` but no `origin/HEAD` says it does not
    /// know, and one with no `origin` at all says nothing.
    #[test]
    fn a_clone_without_origin_head_says_it_does_not_know() {
        let root = crate::util::test_dir("git-no-origin-head");
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(&root).args(args).output();
        let Ok(out) = git(&["init", "-q", "-b", "main"]) else {
            eprintln!("git is not installed; skipping");
            return;
        };
        if !out.status.success() {
            return;
        }
        let _ = git(&["config", "user.email", "t@example.com"]);
        let _ = git(&["config", "user.name", "Ada"]);
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "first"]);
        assert_eq!(not_merged_into(&root.join("a.txt"), "HEAD"), None, "no origin, nothing to be merged into");
        let _ = git(&["remote", "add", "origin", "https://example.invalid/x.git"]);
        assert_eq!(not_merged_into(&root.join("a.txt"), "HEAD"), Some(NotMerged::Unknown));
    }

    /// A real merge, read back out of a real repository.
    ///
    /// The point being checked is that **the pull request number survives on
    /// disk**: nothing here talks to a forge, and the answer still names #7.
    /// That is what makes the spot row possible without a token or a request,
    /// and it holds because the merge is a merge commit -- squash or rebase
    /// would have flattened the subject away, which is why this repository's
    /// own rule forbids them.
    #[test]
    fn the_merge_that_took_a_commit_in_is_found() {
        let root = crate::util::test_dir("git-origin");
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| Command::new("git").arg("-C").arg(&root).args(args).output();
        let Ok(out) = git(&["init", "-q", "-b", "main"]) else {
            eprintln!("git is not installed; skipping");
            return;
        };
        if !out.status.success() {
            eprintln!("git init failed; skipping");
            return;
        }
        let _ = git(&["config", "user.email", "t@example.com"]);
        let _ = git(&["config", "user.name", "Ada"]);

        std::fs::write(root.join("base.txt"), b"base").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "the base"]);

        // A file that exists only on the branch, so its last commit is the one
        // the merge brought in.
        let _ = git(&["checkout", "-q", "-b", "feature/x"]);
        let file = root.join("added.txt");
        std::fs::write(&file, b"added on the branch").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "add the file"]);

        // Meanwhile a commit goes straight onto main. It is *older* than the
        // merge and not the root, which is the case that has to come back
        // empty: the merge is on its ancestry path, so a walk alone credits it.
        let _ = git(&["checkout", "-q", "main"]);
        let straight = root.join("mainline.txt");
        std::fs::write(&straight, b"pushed straight to main").unwrap();
        let _ = git(&["add", "."]);
        let _ = git(&["commit", "-qm", "straight onto main"]);

        let subject = "Merge pull request #7 from acme/feature/x";
        let _ = git(&["merge", "-q", "--no-ff", "-m", subject, "feature/x"]);

        let c = last_commit(&file).expect("the file is committed");
        assert_eq!(c.subject, "add the file", "the branch's commit, not the merge");
        let o = origin(&file, &c.hash).expect("a merge took it in");
        assert_eq!(o.pr, Some(7), "read off the merge subject, with nothing fetched");
        assert_eq!(o.branch.as_deref(), Some("feature/x"));
        assert!(!o.merge.is_empty(), "the merge's own hash is shown too");
        assert_ne!(o.merge, c.hash, "the merge is not the commit it took in");

        // Neither of the mainline commits was brought in by that merge, so
        // neither gets a row. This is the half the first draft got wrong: the
        // merge *is* on their ancestry path, so a walk that stops there reports
        // #7 for a file the pull request never touched.
        for name in ["mainline.txt", "base.txt"] {
            let p = root.join(name);
            let m = last_commit(&p).expect("committed");
            assert!(
                origin(&p, &m.hash).is_none(),
                "{name} went straight onto main; #7 did not bring it in"
            );
        }

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
