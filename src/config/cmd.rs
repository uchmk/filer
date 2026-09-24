//! Parsing of yazi command strings such as `arrow -1`, `remove --permanently`,
//! `shell 'git log' --block` or `plugin toggle-pane max-preview`.

use crate::fs::SortBy;

pub type Tri = Option<bool>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Rel(i64),
    /// Percentage of a page; 100 % is one screenful.
    Pct(i64),
    Top,
    Bot,
}

impl Step {
    /// Where a cursor at `cursor` in a list of `len` ends up, never past
    /// either end. `page` is the number of rows that fit on screen.
    pub fn apply(self, cursor: usize, len: usize, page: usize) -> usize {
        let last = len.saturating_sub(1) as i64;
        let next = match self {
            Step::Top => 0,
            Step::Bot => last,
            Step::Rel(n) => (cursor as i64).saturating_add(n),
            Step::Pct(p) => (cursor as i64).saturating_add((page as i64).saturating_mul(p) / 100),
        };
        next.clamp(0, last) as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EscapeWhat {
    pub visual: bool,
    pub select: bool,
    pub filter: bool,
    pub find: bool,
    pub search: bool,
    pub all: bool,
}

impl EscapeWhat {
    pub fn everything(self) -> bool {
        self.all || !(self.visual || self.select || self.filter || self.find || self.search)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CopyWhat {
    Path,
    Dirname,
    Filename,
    NameWithoutExt,
    /// The spot panel's selected value (yazi's `copy cell`).
    Cell,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenameCursor {
    Start,
    End,
    BeforeExt,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchVia {
    Name,
    Content,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Act {
    Noop,
    Escape(EscapeWhat),
    Quit,
    Close,

    Arrow(Step),
    /// Spot the previous / next file (yazi's spot `swipe`).
    Swipe(i64),
    Leave,
    Enter,
    Back,
    Forward,
    Cd { target: String, interactive: bool },
    Reveal(String),
    Follow,
    Refresh,

    /// Scroll the preview pane.
    Seek(Step),

    TabCreate { current: bool, path: Option<String> },
    TabClose(Option<usize>),
    TabSwitch { n: i64, relative: bool },
    TabSwap(i64),

    /// Open / close the second pane (a filer extra; yazi has one pane).
    Split(Tri),
    /// Move the keys to the left / right pane, or to the other one when no
    /// side is named. Opens the split if it is closed.
    PaneFocus(Option<bool>),

    Toggle { state: Tri },
    ToggleAll { state: Tri },
    VisualMode { unset: bool },

    Open { interactive: bool, hovered: bool },
    Yank { cut: bool },
    Unyank,
    Paste { force: bool, follow: bool },
    Link { relative: bool },
    Hardlink,
    Remove { permanently: bool, force: bool, hovered: bool },
    Create { dir: bool, force: bool },
    Rename { force: bool, cursor: RenameCursor },
    Copy(CopyWhat),
    Shell { run: String, block: bool, confirm: bool, orphan: bool },

    /// Unpack the selected archives, each into a folder of its own.
    Extract,
    /// Pack the selection into one archive; the name typed at the prompt
    /// decides the format.
    Compress,

    Hidden(Tri),
    Linemode(String),
    Sort { by: Option<SortBy>, reverse: Tri, dir_first: Tri },

    Find { prev: bool, smart: bool, insensitive: bool },
    FindArrow { prev: bool },
    Filter { smart: bool, insensitive: bool },
    Search { via: SearchVia, insensitive: bool },
    /// Confirm the pending input (yazi's `*_do` commands).
    Submit,

    /// Complete a path in the input line.
    Complete,
    /// Fuzzy-jump to a bookmark or a recently visited directory.
    Jump,

    Help,
    TasksShow,
    /// Pause the selected job, or set it going again.
    TaskToggle,
    /// Stop the selected job, queued or running.
    TaskCancel,
    /// Move the selected job to the front of the queue.
    TaskTop,
    Spot,
    /// Copy (or move) the selection straight into the other pane, without
    /// yanking and pasting to get there.
    SendPane { cut: bool },

    /// Open the terminal pane and give it the keys, or take them back.
    /// `Some(false)` closes the pane and the shell with it.
    Terminal(Tri),
    /// Type the selected paths into the terminal.
    TermSend,

    /// Fuzzy-search every `mgr` binding and run the one picked.
    Palette,
    /// The context menu for the file under the cursor: its openers from
    /// `yazi.toml` and the bindings that act on it. Right-click runs this.
    Menu,
    /// Switch Markdown between the rendered view and its source.
    ToggleRender,
    /// Hand the keys to the preview's outline (functions, headings) and back.
    ToggleOutline,

    /// Built-in stand-ins for the plugins this config is likely to reference.
    MaxPreview,
    TogglePaneParent,
    BookmarkSave,
    BookmarkJump,
    BookmarkDelete,
    BookmarkDeleteAll,

    /// Recognized syntax, unimplemented behavior. Kept so `?` can list it and
    /// so a partially supported config still loads.
    Unsupported(String),
}

/// Split a command string into words, honoring single/double quotes.
pub fn lex(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if c == '\\' && q == '"' {
                    if let Some(n) = it.next() {
                        cur.push(n);
                    }
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                c if c.is_whitespace() => {
                    if started || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        started = false;
                    }
                }
                '\\' => {
                    if let Some(n) = it.next() {
                        cur.push(n);
                    }
                }
                c => cur.push(c),
            },
        }
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    out
}

struct Args {
    pos: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn new(words: &[String]) -> Self {
        let mut pos = Vec::new();
        let mut flags = Vec::new();
        for w in words {
            if let Some(rest) = w.strip_prefix("--") {
                match rest.split_once('=') {
                    Some((k, v)) => flags.push((k.to_string(), Some(v.to_string()))),
                    None => flags.push((rest.to_string(), None)),
                }
            } else {
                pos.push(w.clone());
            }
        }
        Self { pos, flags }
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }

    fn val(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(k, _)| k == name).and_then(|(_, v)| v.as_deref())
    }

    /// `--reverse`, `--reverse=true`, `--no-reverse` → Some(true/false); absent → None.
    fn tri(&self, name: &str) -> Tri {
        if let Some(v) = self.val(name) {
            return Some(!matches!(v, "false" | "no" | "0"));
        }
        if self.has(name) {
            return Some(true);
        }
        if self.has(&format!("no-{name}")) {
            return Some(false);
        }
        None
    }

    fn first(&self) -> Option<&str> {
        self.pos.first().map(String::as_str)
    }
}

pub fn parse(line: &str) -> Act {
    let words = lex(line);
    let Some((name, rest)) = words.split_first() else { return Act::Noop };
    let a = Args::new(rest);
    match name.as_str() {
        "noop" => Act::Noop,
        "escape" => Act::Escape(EscapeWhat {
            visual: a.has("visual"),
            select: a.has("select"),
            filter: a.has("filter"),
            find: a.has("find"),
            search: a.has("search"),
            all: a.has("all"),
        }),
        "quit" => Act::Quit,
        "close" if a.has("submit") => Act::Submit,
        "close" => Act::Close,
        "complete" => Act::Complete,
        "jump" => Act::Jump,

        "arrow" => Act::Arrow(parse_step(a.first().unwrap_or("1"))),
        "swipe" => Act::Swipe(a.first().and_then(|s| s.parse().ok()).unwrap_or(1)),
        "leave" => Act::Leave,
        "enter" => Act::Enter,
        "back" => Act::Back,
        "forward" => Act::Forward,
        "cd" => Act::Cd {
            target: a.pos.join(" "),
            interactive: a.has("interactive"),
        },
        "reveal" => Act::Reveal(a.pos.join(" ")),
        "follow" => Act::Follow,
        "refresh" => Act::Refresh,

        "seek" => Act::Seek(parse_step(a.first().unwrap_or("1"))),
        "peek" => Act::Seek(parse_step(a.first().unwrap_or("1"))),

        "tab_create" => Act::TabCreate {
            current: a.has("current"),
            path: a.first().map(str::to_owned),
        },
        "tab_close" => Act::TabClose(a.first().and_then(|s| s.parse().ok())),
        "tab_switch" => Act::TabSwitch {
            n: a.first().and_then(|s| s.parse().ok()).unwrap_or(0),
            relative: a.has("relative"),
        },
        "tab_swap" => Act::TabSwap(a.first().and_then(|s| s.parse().ok()).unwrap_or(0)),

        "split" => Act::Split(match a.first() {
            Some("open") => Some(true),
            Some("close") => Some(false),
            _ => None,
        }),
        "pane_focus" => Act::PaneFocus(match a.first() {
            Some("left") => Some(false),
            Some("right") => Some(true),
            _ => None,
        }),

        "toggle" => Act::Toggle { state: state_flag(&a) },
        "toggle_all" => Act::ToggleAll { state: state_flag(&a) },
        "visual_mode" => Act::VisualMode { unset: a.has("unset") },
        "select" => Act::Toggle { state: Some(true) },
        "select_all" => Act::ToggleAll { state: Some(true) },

        "open" | "open_do" => Act::Open {
            interactive: a.has("interactive"),
            hovered: a.has("hovered"),
        },
        "yank" => Act::Yank { cut: a.has("cut") },
        "unyank" => Act::Unyank,
        "paste" => Act::Paste { force: a.has("force"), follow: a.has("follow") },
        "link" => Act::Link { relative: a.has("relative") },
        "hardlink" => Act::Hardlink,
        "extract" => Act::Extract,
        "compress" => Act::Compress,
        "send_pane" => Act::SendPane { cut: a.has("cut") },
        "remove" => Act::Remove {
            permanently: a.has("permanently"),
            force: a.has("force"),
            hovered: a.has("hovered"),
        },
        "create" => Act::Create { dir: a.has("dir"), force: a.has("force") },
        "rename" => Act::Rename {
            force: a.has("force"),
            cursor: match a.val("cursor") {
                Some("start") => RenameCursor::Start,
                Some("end") => RenameCursor::End,
                _ => RenameCursor::BeforeExt,
            },
        },
        "copy" => Act::Copy(match a.first() {
            Some("dirname") => CopyWhat::Dirname,
            Some("filename") => CopyWhat::Filename,
            Some("name_without_ext") => CopyWhat::NameWithoutExt,
            Some("cell") => CopyWhat::Cell,
            _ => CopyWhat::Path,
        }),
        "shell" => Act::Shell {
            run: a.pos.join(" "),
            block: a.has("block"),
            confirm: a.has("confirm"),
            orphan: a.has("orphan"),
        },

        "hidden" => Act::Hidden(match a.first() {
            Some("show") => Some(true),
            Some("hide") => Some(false),
            _ => None,
        }),
        "linemode" => Act::Linemode(a.first().unwrap_or("none").to_owned()),
        "sort" => Act::Sort {
            by: a.first().and_then(SortBy::parse),
            reverse: a.tri("reverse"),
            dir_first: a.tri("dir-first"),
        },

        "find" => Act::Find {
            prev: a.has("previous"),
            smart: a.has("smart"),
            insensitive: a.has("insensitive"),
        },
        "find_arrow" => Act::FindArrow { prev: a.has("previous") },
        "filter" => Act::Filter { smart: a.has("smart"), insensitive: a.has("insensitive") },
        "search" => Act::Search {
            via: match a.val("via").or(a.first()) {
                Some("rg") | Some("content") => SearchVia::Content,
                _ => SearchVia::Name,
            },
            insensitive: a.has("insensitive"),
        },
        "search_stop" => Act::Escape(EscapeWhat { search: true, ..Default::default() }),
        "filter_do" | "find_do" | "search_do" | "cd_do" | "rename_do" | "create_do" => Act::Submit,

        "help" => Act::Help,
        "tasks_show" => Act::TasksShow,
        "task_toggle" => Act::TaskToggle,
        "task_cancel" => Act::TaskCancel,
        "task_top" => Act::TaskTop,
        "spot" => Act::Spot,
        "palette" => Act::Palette,
        "menu" => Act::Menu,
        "terminal" => Act::Terminal(match a.first() {
            Some("open") => Some(true),
            Some("close") => Some(false),
            _ => None,
        }),
        "term_send" => Act::TermSend,
        "toggle_render" => Act::ToggleRender,
        "toggle_outline" => Act::ToggleOutline,

        "plugin" => plugin(&a.pos),

        other => Act::Unsupported(other.to_owned()),
    }
}

fn state_flag(a: &Args) -> Tri {
    match a.val("state") {
        Some("on") | Some("true") => Some(true),
        Some("off") | Some("false") => Some(false),
        _ => None,
    }
}

fn parse_step(s: &str) -> Step {
    match s {
        "top" => Step::Top,
        "bot" | "bottom" => Step::Bot,
        "prev" => Step::Rel(-1),
        "next" => Step::Rel(1),
        s => {
            if let Some(p) = s.strip_suffix('%') {
                Step::Pct(p.parse().unwrap_or(100))
            } else {
                Step::Rel(s.parse().unwrap_or(1))
            }
        }
    }
}

/// Map the plugin invocations this project understands natively onto built-in
/// behavior, so a yazi config that uses them keeps working.
fn plugin(pos: &[String]) -> Act {
    let name = pos.first().map(String::as_str).unwrap_or("");
    let arg = pos.get(1).map(String::as_str).unwrap_or("");
    let arg2 = pos.get(2).map(String::as_str).unwrap_or("");
    match (name, arg) {
        ("toggle-pane", "max-preview") => Act::MaxPreview,
        ("toggle-pane", "min-preview") => Act::MaxPreview,
        ("toggle-pane", "max-parent") | ("toggle-pane", "min-parent") => Act::TogglePaneParent,
        ("toggle-pane", _) => Act::MaxPreview,
        ("bookmarks", "save") => Act::BookmarkSave,
        ("bookmarks", "jump") => Act::BookmarkJump,
        ("bookmarks", "delete") => Act::BookmarkDelete,
        ("bookmarks", "delete_all") => Act::BookmarkDeleteAll,
        ("max-preview", _) => Act::MaxPreview,
        // Directories are entered, files opened — which is what `open` does.
        ("smart-enter", _) => Act::Open { interactive: false, hovered: true },
        ("smart-filter", _) => Act::Filter { smart: true, insensitive: false },
        ("chmod", _) | ("mount", _) => Act::Unsupported(format!("plugin {name}")),
        _ => {
            let mut s = format!("plugin {name}");
            if !arg.is_empty() {
                s.push(' ');
                s.push_str(arg);
            }
            if !arg2.is_empty() {
                s.push(' ');
                s.push_str(arg2);
            }
            Act::Unsupported(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_quotes() {
        assert_eq!(lex("shell 'git log --oneline' --block"), vec![
            "shell".to_string(),
            "git log --oneline".to_string(),
            "--block".to_string()
        ]);
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse("arrow -1"), Act::Arrow(Step::Rel(-1)));
        assert_eq!(parse("arrow 50%"), Act::Arrow(Step::Pct(50)));
        assert_eq!(parse("arrow top"), Act::Arrow(Step::Top));
        assert_eq!(parse("remove --permanently"), Act::Remove {
            permanently: true,
            force: false,
            hovered: false
        });
        assert_eq!(parse("tab_switch 1 --relative"), Act::TabSwitch { n: 1, relative: true });
        assert_eq!(parse("plugin toggle-pane max-preview"), Act::MaxPreview);
        assert_eq!(parse("plugin bookmarks jump"), Act::BookmarkJump);
        assert_eq!(parse("palette"), Act::Palette);
        assert_eq!(parse("menu"), Act::Menu);
        assert_eq!(parse("split"), Act::Split(None));
        assert_eq!(parse("split close"), Act::Split(Some(false)));
        assert_eq!(parse("pane_focus"), Act::PaneFocus(None));
        assert_eq!(parse("pane_focus right"), Act::PaneFocus(Some(true)));
        assert_eq!(parse("toggle_render"), Act::ToggleRender);
        assert_eq!(parse("toggle_outline"), Act::ToggleOutline);
        assert_eq!(parse("plugin smart-enter"), Act::Open { interactive: false, hovered: true });
        assert!(matches!(parse("sort mtime --reverse"), Act::Sort {
            by: Some(SortBy::Mtime),
            reverse: Some(true),
            ..
        }));
    }

    #[test]
    fn steps_stay_inside_the_list() {
        assert_eq!(Step::Rel(1).apply(3, 5, 10), 4);
        assert_eq!(Step::Rel(1).apply(4, 5, 10), 4);
        assert_eq!(Step::Rel(-1).apply(0, 5, 10), 0);
        assert_eq!(Step::Rel(i64::MAX).apply(2, 5, 10), 4);
        assert_eq!(Step::Pct(50).apply(0, 100, 10), 5);
        assert_eq!(Step::Pct(-100).apply(15, 100, 10), 5);
        assert_eq!(Step::Top.apply(3, 5, 10), 0);
        assert_eq!(Step::Bot.apply(0, 5, 10), 4);
        assert_eq!(Step::Bot.apply(0, 0, 10), 0);
    }
}
