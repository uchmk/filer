//! Launching files and shell commands, following yazi's `[opener]` / `[open]`
//! rules.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::yazi::{Opener, YaziToml};
use crate::fs::Entry;
use crate::glob;

/// Openers that apply to `entry`, in the order the rules declare them.
pub fn openers_for<'a>(cfg: &'a YaziToml, entry: &Entry, mime: &str) -> Vec<&'a Opener> {
    let mut names: Vec<&str> = Vec::new();
    for rule in &cfg.open.rules {
        if !rule_matches(rule, entry, mime) {
            continue;
        }
        for n in &rule.use_.0 {
            if !names.contains(&n.as_str()) {
                names.push(n);
            }
        }
    }
    if names.is_empty() {
        // No rule matched: fall back to the conventional bucket names.
        for n in ["open", "edit"] {
            if cfg.opener.contains_key(n) {
                names.push(n);
            }
        }
    }
    let mut out = Vec::new();
    for n in names {
        if let Some(list) = cfg.opener.get(n) {
            out.extend(list.iter().filter(|o| o.matches_platform()));
        }
    }
    out
}

fn rule_matches(rule: &crate::config::yazi::OpenRule, entry: &Entry, mime: &str) -> bool {
    let mut matched = false;
    if let Some(pat) = &rule.name {
        let (pat, dir_only) = match pat.strip_suffix('/') {
            Some(p) => (p, true),
            None => (pat.as_str(), false),
        };
        if dir_only && !entry.is_dir_like() {
            return false;
        }
        let pat = if pat.is_empty() { "*" } else { pat };
        if !glob::matches(pat, &entry.name, true) {
            return false;
        }
        matched = true;
    }
    if let Some(pat) = &rule.mime {
        if !glob::matches(pat, mime, true) {
            return false;
        }
        matched = true;
    }
    matched
}

/// Substitute yazi's opener placeholders. Both the POSIX (`$@`, `$0`) and the
/// Windows (`%*`, `%0`) spellings are accepted, plus the `%s` that shows up in
/// configs copied from elsewhere.
pub fn substitute(template: &str, paths: &[PathBuf]) -> String {
    substitute_with(template, paths, "")
}

/// [`substitute`], with `suffix` added to every path inside its quotes.
fn substitute_with(template: &str, paths: &[PathBuf], suffix: &str) -> String {
    substitute_render(template, paths, &|p| quote(p, suffix))
}

/// [`substitute`], with `render` deciding how one path is written out.
fn substitute_render(template: &str, paths: &[PathBuf], render: &dyn Fn(&Path) -> String) -> String {
    let quote = render;
    let all = paths.iter().map(|p| quote(p)).collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(template.len() + all.len());
    let bytes: Vec<char> = template.chars().collect();
    let mut i = 0;
    let mut substituted = false;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        match (c, next) {
            ('$', Some('@')) | ('%', Some('*')) | ('%', Some('s')) => {
                out.push_str(&all);
                substituted = true;
                i += 2;
            }
            ('$', Some(d)) | ('%', Some(d)) if d.is_ascii_digit() => {
                let idx: usize = d.to_digit(10).unwrap() as usize;
                let p = if c == '%' && idx > 0 { idx - 1 } else { idx };
                out.push_str(&paths.get(p).map(|p| quote(p)).unwrap_or_default());
                substituted = true;
                i += 2;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    if !substituted && !paths.is_empty() {
        out.push(' ');
        out.push_str(&all);
    }
    // `"$@"` in a config leaves stray quotes behind once we quote ourselves.
    out.replace("\"\"", "\"")
}

fn quote(p: &Path, suffix: &str) -> String {
    let s = p.to_string_lossy();
    if s.is_empty() {
        return "\"\"".into();
    }
    format!("\"{}{suffix}\"", s.replace('"', "\\\""))
}

/// The opener's command line, at `line` when there is one and the editor
/// takes it.
pub fn command_line(run: &str, paths: &[PathBuf], line: Option<usize>, custom: &LineArgs) -> String {
    line.and_then(|n| at_line(run, paths, n, custom)).unwrap_or_else(|| substitute(run, paths))
}

/// Line-jump argument templates from `filer.toml`'s `[line_args]`, keyed by
/// [`editor_key`]. A template is written as the arguments themselves, e.g.
/// `"/j{line} {path}"` or `"-g {path}:{line}"`; it wins over the built-in table.
pub type LineArgs = std::collections::HashMap<String, String>;

/// The key an editor is looked up by: its file name, lowercased, without the
/// executable extension. `C:\bin\Sakura.exe` and `sakura` both give `sakura`.
pub fn editor_key(program: &str) -> String {
    let name = match Path::new(program).file_name() {
        Some(n) => n.to_string_lossy().to_ascii_lowercase(),
        None => return String::new(),
    };
    let stem = [".exe", ".cmd", ".bat"]
        .iter()
        .find_map(|x| name.strip_suffix(x))
        .unwrap_or(&name);
    stem.to_owned()
}

/// Split a `[line_args]` template into the arguments that come before the path
/// and the single token carrying it, with `{line}` already filled in. `None`
/// when the template names no path, or more than one.
fn split_template(template: &str, line: usize) -> Option<(String, String)> {
    let mut before: Vec<&str> = Vec::new();
    let mut path = None;
    for tok in template.split_whitespace() {
        match (tok.contains("{path}"), path.is_some()) {
            (true, true) => return None,
            (true, false) => path = Some(tok),
            (false, _) => before.push(tok),
        }
    }
    let n = line.to_string();
    Some((before.join(" ").replace("{line}", &n), path?.replace("{line}", &n)))
}

/// Whether a `[line_args]` template can be used at all, for the config loader
/// to warn about the ones that cannot.
pub fn template_is_valid(template: &str) -> bool {
    split_template(template, 1).is_some()
}

/// How an editor is told which line to open at.
enum LineArg {
    /// A flag before the paths: `nvim +12 file`, `sakura -L=12 file`.
    Flag(&'static str),
    /// `code -g file:12`
    Goto,
    /// `hx file:12`
    Colon,
}

/// The opener's command line, opening `paths` at `line` (1-based) when the
/// program is an editor known to take a line; `None` when it is not. A
/// `[line_args]` template in `custom` is preferred over the built-in table.
pub fn at_line(run: &str, paths: &[PathBuf], line: usize, custom: &LineArgs) -> Option<String> {
    let run = run.trim_start();
    // The program is the first token, quoted or not.
    let end = match run.strip_prefix('"') {
        Some(rest) => rest.find('"').map_or(run.len(), |i| i + 2),
        None => run.find(char::is_whitespace).unwrap_or(run.len()),
    };
    let program = run[..end].trim_matches('"');
    Path::new(program).file_name()?;
    let name = editor_key(program);
    let name = name.as_str();
    if let Some(template) = custom.get(name) {
        if let Some((before, token)) = split_template(template, line) {
            let (head, tail) = run.split_at(end);
            let head = if before.is_empty() { head.to_owned() } else { format!("{head} {before}") };
            return Some(substitute_render(&format!("{head}{tail}"), paths, &|p| {
                let path = p.to_string_lossy().replace('"', "\\\"");
                format!("\"{}\"", token.replace("{path}", &path))
            }));
        }
    }
    let how = match name {
        "nvim" | "vim" | "vi" | "gvim" | "nano" | "emacs" | "emacsclient" | "micro" | "kak" => LineArg::Flag("+"),
        // Editors common on Windows. Notepad has no line switch, so it stays out.
        "hidemaru" => LineArg::Flag("/j"),
        "sakura" => LineArg::Flag("-L="),
        "emeditor" => LineArg::Flag("/l "),
        "notepad++" => LineArg::Flag("-n"),
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" => LineArg::Goto,
        "hx" | "helix" | "subl" | "zed" => LineArg::Colon,
        _ => return None,
    };
    let (head, tail) = run.split_at(end);
    let colon = format!(":{line}");
    Some(match how {
        LineArg::Flag(flag) => substitute(&format!("{head} {flag}{line}{tail}"), paths),
        LineArg::Goto => substitute_with(&format!("{head} -g{tail}"), paths, &colon),
        LineArg::Colon => substitute_with(run, paths, &colon),
    })
}

/// Run a command line through the platform shell.
pub fn shell(cmdline: &str, cwd: &Path, block: bool, orphan: bool) -> std::io::Result<()> {
    let mut cmd = shell_command(cmdline);
    cmd.current_dir(cwd);
    configure(&mut cmd, block, orphan);
    cmd.spawn()?;
    Ok(())
}

#[cfg(windows)]
fn shell_command(cmdline: &str) -> Command {
    let mut c = Command::new("cmd");
    // /C with the whole line keeps quoting semantics the user expects.
    c.arg("/C").raw_arg_compat(cmdline);
    c
}

#[cfg(not(windows))]
fn shell_command(cmdline: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmdline);
    c
}

/// `Command::raw_arg` is Windows-only; this keeps the call site tidy.
#[cfg(windows)]
trait RawArgCompat {
    fn raw_arg_compat(&mut self, s: &str) -> &mut Self;
}

#[cfg(windows)]
impl RawArgCompat for Command {
    fn raw_arg_compat(&mut self, s: &str) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.raw_arg(s)
    }
}

#[cfg(windows)]
fn configure(cmd: &mut Command, block: bool, _orphan: bool) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // A blocking (terminal) program needs a console of its own; a GUI program
    // should not flash one.
    cmd.creation_flags(if block { CREATE_NEW_CONSOLE } else { CREATE_NO_WINDOW });
}

#[cfg(not(windows))]
fn configure(_cmd: &mut Command, _block: bool, _orphan: bool) {}

/// Open with whatever the OS considers the default handler.
pub fn open_default(path: &Path) -> std::io::Result<()> {
    open::that_detached(path)
}

/// Hand a URL to the default browser.
///
/// Detached for the same reason as `open_default`: the browser outlives us, and
/// waiting on it would freeze the window until it closed.
pub fn open_url(url: &str) -> std::io::Result<()> {
    open::that_detached(url)
}

pub fn set_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_owned()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_placeholders() {
        let paths = vec![PathBuf::from(r"C:\a b\x.txt")];
        assert_eq!(substitute("nvim %s", &paths), "nvim \"C:\\a b\\x.txt\"");
        assert_eq!(substitute("code %*", &paths), "code \"C:\\a b\\x.txt\"");
        assert_eq!(substitute("mpv $0", &paths), "mpv \"C:\\a b\\x.txt\"");
        // No placeholder at all: append the paths.
        assert_eq!(substitute("explorer", &paths), "explorer \"C:\\a b\\x.txt\"");
    }

    #[test]
    fn opens_editors_at_a_line() {
        let paths = vec![PathBuf::from(r"C:\a b\x.rs")];
        let at = |run| at_line(run, &paths, 12, &LineArgs::new());
        assert_eq!(at("nvim %s").as_deref(), Some(r#"nvim +12 "C:\a b\x.rs""#));
        assert_eq!(at("code %*").as_deref(), Some(r#"code -g "C:\a b\x.rs:12""#));
        assert_eq!(at(r#"hx "$@""#).as_deref(), Some(r#"hx "C:\a b\x.rs:12""#));
        assert_eq!(at("Code.CMD").as_deref(), Some(r#"Code.CMD -g "C:\a b\x.rs:12""#));
        assert_eq!(
            at(r#""C:\Program Files\Neovim\bin\nvim.exe" -O %s"#).as_deref(),
            Some(r#""C:\Program Files\Neovim\bin\nvim.exe" +12 -O "C:\a b\x.rs""#)
        );
        assert_eq!(at("explorer %s"), None);
        assert_eq!(at(""), None);
    }

    #[test]
    fn opens_windows_editors_at_a_line() {
        let paths = vec![PathBuf::from(r"C:\a b\x.txt")];
        let at = |run| at_line(run, &paths, 123, &LineArgs::new());
        assert_eq!(at("hidemaru %s").as_deref(), Some(r#"hidemaru /j123 "C:\a b\x.txt""#));
        assert_eq!(at("sakura %s").as_deref(), Some(r#"sakura -L=123 "C:\a b\x.txt""#));
        assert_eq!(at("emeditor %s").as_deref(), Some(r#"emeditor /l 123 "C:\a b\x.txt""#));
        assert_eq!(at("notepad++ %s").as_deref(), Some(r#"notepad++ -n123 "C:\a b\x.txt""#));
        // Notepad takes no line, so it is opened the plain way.
        assert_eq!(at("notepad %s"), None);
        assert_eq!(
            at(r#""C:\Program Files\sakura\sakura.exe" %s"#).as_deref(),
            Some(r#""C:\Program Files\sakura\sakura.exe" -L=123 "C:\a b\x.txt""#)
        );
        assert_eq!(
            at(r#""C:\Program Files\Notepad++\notepad++.exe" %s"#).as_deref(),
            Some(r#""C:\Program Files\Notepad++\notepad++.exe" -n123 "C:\a b\x.txt""#)
        );
    }

    #[test]
    fn line_args_from_the_config_win() {
        let paths = vec![PathBuf::from(r"C:\a b\x.txt")];
        let mut custom = LineArgs::new();
        // An editor the built-in table knows nothing about.
        custom.insert("mikan".into(), "-l {line} {path}".into());
        // The three shapes a template can take.
        custom.insert("myedit".into(), "{path}:{line}".into());
        custom.insert("goto".into(), "--goto {path}@{line}".into());
        // A configured editor overrides the built-in table.
        custom.insert("sakura".into(), "/LINE={line} {path}".into());
        let at = |run| at_line(run, &paths, 123, &custom);

        assert_eq!(at("mikan %s").as_deref(), Some(r#"mikan -l 123 "C:\a b\x.txt""#));
        assert_eq!(at("myedit %s").as_deref(), Some(r#"myedit "C:\a b\x.txt:123""#));
        assert_eq!(at("goto %s").as_deref(), Some(r#"goto --goto "C:\a b\x.txt@123""#));
        assert_eq!(at("sakura %s").as_deref(), Some(r#"sakura /LINE=123 "C:\a b\x.txt""#));
        // Keys and programs are matched by file name, without the extension.
        assert_eq!(
            at(r#""C:\Program Files\Mikan\Mikan.exe" -w %s"#).as_deref(),
            Some(r#""C:\Program Files\Mikan\Mikan.exe" -l 123 -w "C:\a b\x.txt""#)
        );
        // An opener with no placeholder still gets the path appended.
        assert_eq!(at("mikan").as_deref(), Some(r#"mikan -l 123 "C:\a b\x.txt""#));
        // Editors outside both the table and the config are unchanged.
        assert_eq!(at("explorer %s"), None);
    }

    #[test]
    fn rejects_templates_without_one_path() {
        assert!(template_is_valid("-l {line} {path}"));
        assert!(template_is_valid("{path}"));
        assert!(!template_is_valid("-l {line}"));
        assert!(!template_is_valid("{path} {path}"));
        assert!(!template_is_valid(""));
    }
}

#[cfg(test)]
mod hint_is_true {
    use super::substitute;
    use std::path::PathBuf;

    /// The shell prompt's hint claims four things. Each is checked here rather
    /// than trusted, because a hint that describes syntax the substituter has
    /// stopped honouring is worse than no hint: it is believed, and the command
    /// runs on the wrong thing.
    #[test]
    fn the_prompt_honours_everything_it_advertises() {
        let paths = vec![PathBuf::from("/one.txt"), PathBuf::from("/two.txt")];

        // `$@` — all of them.
        let all = substitute("cmd $@", &paths);
        assert!(all.contains("one.txt") && all.contains("two.txt"), "{all}");

        // `$0` — the first, and only the first.
        let first = substitute("cmd $0", &paths);
        assert!(first.contains("one.txt"), "{first}");
        assert!(!first.contains("two.txt"), "$0 took more than the first: {first}");

        // `$1` — the second.
        let second = substitute("cmd $1", &paths);
        assert!(second.contains("two.txt"), "{second}");
        assert!(!second.contains("one.txt"), "$1 took more than the second: {second}");

        // No placeholder — appended.
        let bare = substitute("cmd", &paths);
        assert!(
            bare.contains("one.txt") && bare.contains("two.txt"),
            "a command with no placeholder must still get the files: {bare}",
        );
    }

    /// The hint shows no quoting, because the quoting is done for you. A name
    /// with a space in it is the case that would otherwise split into two
    /// arguments and act on something else entirely.
    #[test]
    fn a_name_with_a_space_stays_one_argument() {
        let paths = vec![PathBuf::from("/a file.txt")];
        let out = substitute("cmd $@", &paths);
        assert!(out.contains("\"/a file.txt\""), "not quoted: {out}");
    }
}
