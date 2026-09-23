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
    let quote = |p: &Path| quote(p, suffix);
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
pub fn command_line(run: &str, paths: &[PathBuf], line: Option<usize>) -> String {
    line.and_then(|n| at_line(run, paths, n)).unwrap_or_else(|| substitute(run, paths))
}

/// How an editor is told which line to open at.
enum LineArg {
    /// `nvim +12 file`
    Plus,
    /// `code -g file:12`
    Goto,
    /// `hx file:12`
    Colon,
}

/// The opener's command line, opening `paths` at `line` (1-based) when the
/// program is an editor known to take a line; `None` when it is not.
pub fn at_line(run: &str, paths: &[PathBuf], line: usize) -> Option<String> {
    let run = run.trim_start();
    // The program is the first token, quoted or not.
    let end = match run.strip_prefix('"') {
        Some(rest) => rest.find('"').map_or(run.len(), |i| i + 2),
        None => run.find(char::is_whitespace).unwrap_or(run.len()),
    };
    let program = run[..end].trim_matches('"');
    let name = Path::new(program).file_name()?.to_string_lossy().to_ascii_lowercase();
    let name = [".exe", ".cmd", ".bat"]
        .iter()
        .find_map(|x| name.strip_suffix(x))
        .unwrap_or(&name);
    let how = match name {
        "nvim" | "vim" | "vi" | "gvim" | "nano" | "emacs" | "emacsclient" | "micro" | "kak" => LineArg::Plus,
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" => LineArg::Goto,
        "hx" | "helix" | "subl" | "zed" => LineArg::Colon,
        _ => return None,
    };
    let (head, tail) = run.split_at(end);
    let colon = format!(":{line}");
    Some(match how {
        LineArg::Plus => substitute(&format!("{head} +{line}{tail}"), paths),
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
        let at = |run| at_line(run, &paths, 12);
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
}
