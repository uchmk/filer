//! Launching files and shell commands, following yazi's `[opener]` / `[open]`
//! rules.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::yazi::{Opener, YaziToml};
use crate::fs::Entry;
use crate::glob;

/// Openers that apply to `entry`, in the order the rules declare them.
pub fn openers_for<'a>(cfg: &'a YaziToml, entry: &Entry, mime: &str) -> Vec<&'a Opener> {
    let mut names: Vec<&str> = Vec::new();
    for rule in cfg.open.all_rules() {
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

/// [`substitute`] for a `:` / `;` line: with no placeholder and a shell
/// operator in the line, nothing is appended, as the path would land in the
/// last command or the redirect target (Q82).
pub fn substitute_line(template: &str, paths: &[PathBuf]) -> String {
    if line_skips_path(template) {
        return template.to_owned();
    }
    substitute(template, paths)
}

/// Whether [`substitute_line`] leaves the paths off `template` (Q82).
pub fn line_skips_path(template: &str) -> bool {
    let has_placeholder = ["$@", "%*", "%s"].iter().any(|p| template.contains(p))
        || template.as_bytes().windows(2).any(|w| matches!(w[0], b'$' | b'%') && w[1].is_ascii_digit());
    !has_placeholder && template.contains(['&', '|', '>', '<', ';'])
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
                // A config that quotes the placeholder itself -- `"$@"` -- would
                // otherwise end up with two quotes a side, because every path is
                // quoted on the way out. Drop the pair that is wrapping this one
                // placeholder, rather than collapsing every `""` in the line:
                // `start "" msedge %*` is the standard way to give `start` an
                // empty window title, and losing one of those quotes turns the
                // rest of the line into the title.
                if out.ends_with('"') && bytes.get(i + 2) == Some(&'"') {
                    out.pop();
                    out.push_str(&all);
                    i += 3;
                } else {
                    out.push_str(&all);
                    i += 2;
                }
                substituted = true;
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
    out
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
    /// A flag before the paths: `nvim +12 file`, `sakura -Y=12 file`.
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
        // `-Y=`, not `-L=`. Sakura takes `-L=` without complaining and does
        // nothing with it, so the file opened at line 1 and the only way to
        // notice was to look. Checked on sakura 2.4.2: `-L=6` lands on line 1,
        // `-Y=6` on line 6.
        "sakura" => LineArg::Flag("-Y="),
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
///
/// The returned [`Launch`] carries the failures that arrive after `spawn` has
/// already said yes; see its docs.
pub fn shell(cmdline: &str, cwd: &Path, block: bool, orphan: bool) -> std::io::Result<Launch> {
    // Windows gives a blocking opener a console of its own (`new_console`).
    // Elsewhere a GUI app has no terminal to lend, so one is opened for it.
    #[cfg(not(windows))]
    if block {
        return in_terminal(cmdline, cwd);
    }
    // Windows: `Command` always hands the child our standard handles, and a
    // release filer has none, so a console program would get a console with
    // nothing wired to it (Q12). Spawned the way `start` does it instead.
    #[cfg(windows)]
    if block {
        let pid = new_console(cmdline, cwd)?;
        return Ok(Launch { pid: Some(pid), ..Launch::none() });
    }
    let mut cmd = shell_command(cmdline);
    cmd.current_dir(cwd);
    configure(&mut cmd, block, orphan);
    // Only worth capturing for the launches whose console is hidden; see
    // `Launch`.
    let watched = !block;
    if watched {
        cmd.stderr(Stdio::piped());
    }
    let child = cmd.spawn()?;
    Ok(if watched { Launch::watch(child, cmdline) } else { Launch::none() })
}

/// A launch that can still fail after [`shell`] has returned `Ok`.
///
/// `spawn` succeeding means the *shell* started, nothing more. A non-blocking
/// opener runs with its console hidden, so a shell that cannot find the
/// program, or chokes on the command line, writes the reason to a console
/// nobody will ever see and exits — leaving a launch that is, from here,
/// indistinguishable from one that worked. That is why a mis-quoted opener
/// presents as "the key does nothing": there is no error to report because
/// nothing reported an error. `rx` carries that news across when it arrives.
pub struct Launch {
    pub rx: crossbeam_channel::Receiver<String>,
    /// The process started: the shell the line runs in (`cmd`, `sh`), or the
    /// terminal holding it. The program itself is its child, so a check can
    /// stop what this launch started and nothing of the same name (#162).
    pub pid: Option<u32>,
}

impl Launch {
    /// A launch with nothing to wait for — the caller can already see how it
    /// went, because it has a console of its own.
    fn none() -> Self {
        let (_tx, rx) = crossbeam_channel::bounded(0);
        Self { rx, pid: None }
    }

    /// Watch `child` for long enough to catch a shell that falls over at once.
    ///
    /// The window is short on purpose. A shell that cannot run the line is gone
    /// in milliseconds, whereas one that *did* start the program stays for as
    /// long as the program lives — which for an editor is the rest of the
    /// afternoon, and watching it costs a thread per file opened. Anything
    /// still alive at the deadline is treated as launched, and the handle is
    /// dropped: that closes our end of the pipe, so the program's later writes
    /// fail harmlessly instead of blocking on a buffer nobody drains.
    fn watch(mut child: Child, cmdline: &str) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let pid = Some(child.id());
        let cmdline = cmdline.to_owned();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match child.try_wait() {
                    Ok(Some(st)) if st.success() => return,
                    Ok(Some(st)) => {
                        let why = match stderr_text(&mut child) {
                            Some(msg) => msg,
                            None => match missing_program(&cmdline, &|p| crate::util::locate(p).is_some()) {
                                Some(p) => format!("`{p}` was not found"),
                                None => match st.code() {
                                    Some(c) => format!("exit code {c}"),
                                    None => "killed".into(),
                                },
                            },
                        };
                        let _ = tx.send(format!("Open failed: {why} — {cmdline}"));
                        return;
                    }
                    Ok(None) => {}
                    Err(_) => return,
                }
                if Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        });
        Self { rx, pid }
    }
}

/// The program at the front of a command line that failed, when it is not
/// there to run. `exit code 1` said the same for a misspelled program, a path
/// with a typo and a program that ran and opened nothing (#96); this one can
/// be told apart, and only off the UI thread, since it walks `PATH`. A
/// shell's own command (`start`, `echo`) is never "missing".
fn missing_program(cmdline: &str, found: &dyn Fn(&str) -> bool) -> Option<String> {
    const BUILTIN: [&str; 6] = ["start", "call", "echo", "cd", "set", "exec"];
    let exe = crate::envreport::program(cmdline)?;
    if BUILTIN.contains(&exe.to_ascii_lowercase().as_str()) || found(&exe) {
        return None;
    }
    Some(exe)
}

/// What the shell complained about, on one line.
///
/// `None` unless it is text we can read: a console on a non-English Windows
/// answers in its own code page, not UTF-8, and a toast of mojibake tells the
/// reader less than the exit code does.
fn stderr_text(child: &mut Child) -> Option<String> {
    use std::io::Read;
    let mut buf = Vec::new();
    // Bounded: this goes into a toast, and a program that failed while
    // producing megabytes is not going to be explained by all of them.
    child.stderr.take()?.take(4096).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8(buf).ok()?;
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!line.is_empty()).then_some(line)
}

/// The single argument `cmd /S /C` is handed.
///
/// `cmd` does not take the rest of its command line verbatim. Without `/S` it
/// keeps the quotes only when the line holds *exactly two* of them; otherwise
/// it strips the first quote and the last one and runs what is left. An opener
/// whose program is quoted because its path has a space —
/// `"C:\Program Files (x86)\sakura\sakura.exe" "C:\dev\x.toml"` — has four, so
/// it is mangled into `C:\Program Files (x86)\sakura\sakura.exe" "C:\dev\x.toml`
/// and dies on the space (or, with `(x86)` now outside the quotes, on the
/// parentheses, which `cmd` reads as grouping). `/S` replaces that guesswork
/// with one rule — strip the leading and trailing quote, take the rest as is —
/// so wrapping the line in a pair of our own delivers it intact.
///
/// Kept off `#[cfg(windows)]` so the rule stays under test on every platform:
/// it is the kind of quoting that is only ever noticed when it breaks.
#[cfg_attr(not(windows), allow(dead_code))]
fn cmd_s_c_arg(cmdline: &str) -> String {
    format!("\"{cmdline}\"")
}

/// `line`, made to wait for a key before its console closes (Q13): `git log -5`
/// is otherwise a window that flashes and is gone. A line that already says
/// `pause` is left alone rather than asking twice.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn held(line: &str) -> String {
    let says_pause = line.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w.eq_ignore_ascii_case("pause"));
    if says_pause { line.to_owned() } else { format!("{line} & pause") }
}

/// The command line the new console runs: `cmd` by its full path, so a
/// `cmd.exe` in the folder being browsed is never the one started.
#[cfg_attr(not(windows), allow(dead_code))]
fn console_command_line(system_root: &str, cmdline: &str) -> (String, String) {
    let cmd = format!("{}\\System32\\cmd.exe", system_root.trim_end_matches(['\\', '/']));
    let line = format!("\"{cmd}\" /S /C {}", cmd_s_c_arg(cmdline));
    (cmd, line)
}

/// Start `cmdline` in a console of its own, without `STARTF_USESTDHANDLES`.
///
/// That flag is what `Command` always sets, passing on our standard handles
/// even when they are null -- and in the release build, which has no console
/// (`windows_subsystem = "windows"`), they are. The child's new console then
/// has nothing attached: nvim draws nothing, `git log` prints nowhere (Q12).
/// Left unset, Windows gives the child the new console's own handles, which is
/// what `start` relies on. Debug builds have a console, which is why
/// `cargo run` never showed it.
#[cfg(windows)]
fn new_console(cmdline: &str, cwd: &Path) -> std::io::Result<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{CreateProcessW, CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW};
    use windows::core::{PCWSTR, PWSTR};
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let (app, line) = console_command_line(&root, cmdline);
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let app = wide(app.as_ref());
    let mut line = wide(line.as_ref());
    let dir = wide(cwd.as_os_str());
    let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: every buffer is NUL-terminated and outlives the call; `line` is
    // mutable as `CreateProcessW` requires.
    unsafe {
        CreateProcessW(
            PCWSTR(app.as_ptr()),
            Some(PWSTR(line.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT,
            None,
            PCWSTR(dir.as_ptr()),
            &si,
            &mut pi,
        )
    }
    .map_err(|e| std::io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
    // SAFETY: both handles were just returned to us and are closed once.
    unsafe {
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
    }
    Ok(pi.dwProcessId)
}

#[cfg(windows)]
fn shell_command(cmdline: &str) -> Command {
    let mut c = Command::new("cmd");
    // `/S` strips our outer pair, leaving the inner quotes for the program to
    // see. Written raw because Rust's own quoting would escape them again.
    c.arg("/S").arg("/C").raw_arg_compat(&cmd_s_c_arg(cmdline));
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
fn configure(cmd: &mut Command, _block: bool, _orphan: bool) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Only non-blocking launches come here (a blocking one goes to
    // `new_console`): a GUI program should not flash a console.
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure(_cmd: &mut Command, _block: bool, _orphan: bool) {}

/// The terminal emulators a `block = true` opener is tried in off Windows,
/// after `$TERMINAL`. Debian's alternatives name comes first: it is the one the
/// system was set up to prefer. xterm is last because it is everywhere and
/// nobody's favourite.
#[cfg_attr(windows, allow(dead_code))]
pub const TERMINALS: [&str; 10] = [
    "x-terminal-emulator",
    "gnome-terminal",
    "konsole",
    "xfce4-terminal",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "ghostty",
    "xterm",
];

/// macOS's own terminal, which is driven through `osascript` rather than run.
#[cfg_attr(windows, allow(dead_code))]
pub const MAC_TERMINAL: &str = "Terminal.app";

/// The terminals to try, in order: `$TERMINAL` when set, then Terminal.app on
/// macOS (always there), then [`TERMINALS`].
#[cfg_attr(windows, allow(dead_code))]
pub fn terminals(env: Option<&str>, macos: bool) -> Vec<String> {
    let mut out: Vec<String> = env.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned).into_iter().collect();
    if macos {
        out.push(MAC_TERMINAL.into());
    }
    out.extend(TERMINALS.iter().map(|t| t.to_string()));
    out
}

/// What runs inside the terminal: `cd` to `$1`, run `$2`, and if it failed,
/// keep the window open long enough to read why. Without the hold, an editor
/// that is not installed is a window that flashes and is gone -- the same
/// "the key does nothing" that [`Launch`] exists to prevent. The command line
/// is `eval`ed from an argument rather than pasted in, so no quoting of ours
/// can break it.
#[cfg_attr(windows, allow(dead_code))]
const TERMINAL_SCRIPT: &str = r#"cd -- "$1" || exit; eval "$2"; s=$?; if [ "$s" -ne 0 ]; then printf '\n[exit %s] Press Enter to close. ' "$s"; read -r _; fi"#;

/// The argv that opens `term` running `cmdline` in `cwd`.
///
/// `term` may carry its own arguments (`TERMINAL="kitty --single-instance"`).
/// Each emulator is told "run this" in its own way; the ones not listed take
/// xterm's `-e`, which is the convention Debian requires of
/// `x-terminal-emulator`.
#[cfg_attr(windows, allow(dead_code))]
pub fn terminal_argv(term: &str, cwd: &Path, cmdline: &str) -> Vec<String> {
    let cwd = cwd.to_string_lossy();
    if term == MAC_TERMINAL {
        // `do script` types into the user's login shell, whatever it is, so
        // all it is given is one `exec` of `sh`; from there it is the same
        // script as everywhere else.
        let line = login_shell_line(&cwd, cmdline);
        let tell = |what: &str| format!("tell application \"Terminal\" to {what}");
        return vec![
            "osascript".into(),
            "-e".into(),
            tell(&format!("do script {}", applescript_string(&line))),
            "-e".into(),
            tell("activate"),
        ];
    }
    let mut argv: Vec<String> = term.split_whitespace().map(str::to_owned).collect();
    let name = Path::new(&argv[0]).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let run: &[&str] = match name.as_str() {
        "gnome-terminal" => &["--"],
        "wezterm" => &["start", "--"],
        "xfce4-terminal" => &["-x"],
        "kitty" | "foot" => &[],
        _ => &["-e"],
    };
    argv.extend(run.iter().map(|s| s.to_string()));
    argv.extend(["sh", "-c", TERMINAL_SCRIPT, "sh", &cwd, cmdline].map(str::to_owned));
    argv
}

/// The one line Terminal.app types into the login shell.
#[cfg_attr(windows, allow(dead_code))]
fn login_shell_line(cwd: &str, cmdline: &str) -> String {
    ["exec sh -c", &sh_quote(TERMINAL_SCRIPT), "sh", &sh_quote(cwd), &sh_quote(cmdline)].join(" ")
}

/// `s` as one word to a POSIX shell.
#[cfg_attr(windows, allow(dead_code))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `s` as an AppleScript string literal.
#[cfg_attr(windows, allow(dead_code))]
fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', r"\\").replace('"', "\\\""))
}

/// Run `cmdline` in a new terminal window: the first of [`terminals`] that is
/// installed. Trying them by spawning, rather than looking along `PATH` first,
/// keeps the lookup to the one the OS does anyway.
#[cfg(not(windows))]
fn in_terminal(cmdline: &str, cwd: &Path) -> std::io::Result<Launch> {
    use std::io::{Error, ErrorKind};
    let env = std::env::var("TERMINAL").ok();
    for term in terminals(env.as_deref(), cfg!(target_os = "macos")) {
        let argv = terminal_argv(&term, cwd, cmdline);
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]).current_dir(cwd);
        // `osascript` is gone in a moment either way, and says why when
        // Terminal refuses; a terminal emulator lives as long as its window
        // and its failures show in it.
        let osa = term == MAC_TERMINAL;
        if osa {
            cmd.stderr(Stdio::piped());
        }
        match cmd.spawn() {
            Ok(child) => return Ok(if osa { Launch::watch(child, cmdline) } else { Launch { pid: Some(child.id()), ..Launch::none() } }),
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
    }
    Err(Error::new(ErrorKind::NotFound, format!("no terminal to run it in — set TERMINAL, or install one of {}", TERMINALS[1..].join(", "))))
}

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

/// Put `text` on the clipboard, with Windows line endings on Windows.
///
/// Every copy builds its text with `\n`, and the Windows clipboard is read as
/// CRLF: `Get-Clipboard` split an LF-only help panel into nothing and handed
/// back one string, so 34.15's own check found no line at all (#180, #182), and
/// older programs paste it as one long line (Q52). Elsewhere LF is the norm.
pub fn set_clipboard(text: &str) -> Result<(), String> {
    #[cfg(not(test))]
    {
        *LAST_SET.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_owned());
    }
    // A test never writes the machine's clipboard: on the owner's Windows
    // machine every `cargo test` used to replace what was on it (the QA
    // agent's finding on section 26). It goes to this thread's fake instead,
    // where `get_clipboard` reads it back.
    #[cfg(test)]
    {
        fake_clipboard(text);
        Ok(())
    }
    #[cfg(not(test))]
    set_real_clipboard(text)
}

/// What filer itself last put on the clipboard, for the state file (#197).
/// Only that: what is on the clipboard was often put there by something else,
/// and a file written beside a run is no place for it.
#[cfg(not(test))]
static LAST_SET: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The text of filer's last `set_clipboard`, if it has made one.
#[cfg(not(test))]
pub fn last_set_clipboard() -> Option<String> {
    LAST_SET.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Under test it is this thread's fake, which `set_clipboard` writes, so
/// tests running side by side do not see each other's.
#[cfg(test)]
pub fn last_set_clipboard() -> Option<String> {
    FAKE_CLIPBOARD.with(|c| c.borrow().clone())
}

#[cfg(not(test))]
fn set_real_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(line_endings(text, cfg!(windows)).into_owned()).map_err(|e| e.to_string())
}

/// `text` with every line ending as CRLF when `crlf`, and untouched otherwise.
/// A `\r\n` already there stays one, rather than growing a second `\r`.
fn line_endings(text: &str, crlf: bool) -> std::borrow::Cow<'_, str> {
    if !crlf || !text.contains('\n') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut prev = '\0';
    for c in text.chars() {
        if c == '\n' && prev != '\r' {
            out.push('\r');
        }
        out.push(c);
        prev = c;
    }
    std::borrow::Cow::Owned(out)
}

/// What is on the clipboard, as text.
///
/// egui hands over clipboard contents only when the platform sends a paste
/// event, which it does for `<C-v>` and for nothing else — a right-click is not
/// one, so a mouse paste has to read the clipboard itself.
///
/// An empty clipboard, or one holding an image rather than text, is `Ok("")`
/// rather than an error: there is nothing to paste, but nothing went wrong
/// either, and a caller that reported it would complain every time a paste was
/// tried on a fresh login. An `Err` is a clipboard that could not be opened at
/// all — another program holding it, mostly — which is worth saying out loud,
/// because the paste silently did nothing.
pub fn get_clipboard() -> Result<String, String> {
    #[cfg(test)]
    if let Some(text) = FAKE_CLIPBOARD.with(|c| c.borrow().clone()) {
        return Ok(text);
    }
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match cb.get_text() {
        Ok(text) => Ok(text),
        Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
thread_local! {
    /// What [`get_clipboard`] returns in a test that set it, so a paste can be
    /// driven without a desktop clipboard (CI has none on Linux). Per thread,
    /// so parallel tests do not see each other's.
    static FAKE_CLIPBOARD: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Have [`get_clipboard`] return `text` on this thread, for a test.
#[cfg(test)]
pub fn fake_clipboard(text: &str) {
    FAKE_CLIPBOARD.with(|c| *c.borrow_mut() = Some(text.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Q52: CRLF for the Windows clipboard, one `\r` per line however the
    /// text arrived, and nothing changed where LF is the norm.
    #[test]
    fn the_clipboard_gets_crlf_only_when_asked() {
        assert_eq!(line_endings("keys\nj\tdown\tarrow 1\n", true), "keys\r\nj\tdown\tarrow 1\r\n");
        assert_eq!(line_endings("a\r\nb\nc", true), "a\r\nb\r\nc", "an existing CRLF is not doubled");
        assert_eq!(line_endings("one line", true), "one line");
        assert_eq!(line_endings("a\nb\n", false), "a\nb\n");
        assert!(matches!(line_endings("a\nb", false), std::borrow::Cow::Borrowed(_)));
    }

    /// The `[opener]` example in the README is pasted as is, so it has to open
    /// each Office file in its own program: one shared list handed every Word
    /// and PowerPoint file to Excel (#96).
    #[test]
    fn the_readme_opener_example_opens_each_office_file_in_its_own_program() {
        let readme = include_str!("../README.md");
        let start = readme.find("```toml\n[opener]").expect("the README has the example") + "```toml\n".len();
        let block = &readme[start..start + readme[start..].find("```").unwrap()];
        let cfg: YaziToml = toml::from_str(block).expect("the example parses");
        for (name, want) in [("a.docx", "Word"), ("a.pptx", "PowerPoint"), ("a.xlsx", "Excel"), ("a.pdf", "Edge")] {
            let entry = Entry {
                path: PathBuf::from(name),
                name: name.into(),
                ext: name.rsplit_once('.').map(|(_, e)| e.into()),
                ..Default::default()
            };
            let first = openers_for(&cfg, &entry, "").first().and_then(|o| o.desc.clone());
            assert_eq!(first.as_deref(), Some(want), "{name}");
        }
    }

    /// yazi's `prepend_rules` / `append_rules`: tried before and after
    /// `rules`, so a prepended rule wins and an appended one only catches what
    /// nothing else did (#250: both were ignored without a word).
    #[test]
    fn prepended_rules_come_first_and_appended_last() {
        let cfg: YaziToml = toml::from_str(
            r#"
            [opener]
            edit = [{ run = "edit %s", desc = "Edit" }]
            view = [{ run = "view %s", desc = "View" }]
            last = [{ run = "last %s", desc = "Last" }]
            [open]
            rules = [{ name = "*.txt", use = "edit" }]
            prepend_rules = [{ name = "*.txt", use = "view" }]
            append_rules = [{ name = "*", use = "last" }]
            "#,
        )
        .unwrap();
        let entry = |name: &str| Entry {
            path: PathBuf::from(name),
            name: name.into(),
            ext: name.rsplit_once('.').map(|(_, e)| e.into()),
            ..Default::default()
        };
        let descs = |name: &str| openers_for(&cfg, &entry(name), "").iter().filter_map(|o| o.desc.clone()).collect::<Vec<_>>();
        assert_eq!(descs("a.txt"), ["View", "Edit", "Last"]);
        assert_eq!(descs("a.bin"), ["Last"], "only the catch-all");
    }

    /// #96: a program that is not there is named; one that is, or a shell's
    /// own command, leaves the exit code to speak.
    #[test]
    fn a_missing_program_is_named() {
        let only_code = |p: &str| p == "code";
        assert_eq!(missing_program("Hidemruu.exe /j10 a.txt", &only_code).as_deref(), Some("Hidemruu.exe"));
        assert_eq!(
            missing_program(r#""C:/Tools/Hide maru.exe" a.txt"#, &only_code).as_deref(),
            Some("C:/Tools/Hide maru.exe")
        );
        assert_eq!(missing_program("code -g a.txt:3", &only_code), None);
        assert_eq!(missing_program(r#"start "" a.txt"#, &only_code), None);
    }

    #[test]
    fn substitutes_placeholders() {
        let paths = vec![PathBuf::from(r"C:\a b\x.txt")];
        assert_eq!(substitute("nvim %s", &paths), "nvim \"C:\\a b\\x.txt\"");
        assert_eq!(substitute("code %*", &paths), "code \"C:\\a b\\x.txt\"");
        assert_eq!(substitute("mpv $0", &paths), "mpv \"C:\\a b\\x.txt\"");
        // No placeholder at all: append the paths.
        assert_eq!(substitute("explorer", &paths), "explorer \"C:\\a b\\x.txt\"");
    }

    /// A line with an operator and no placeholder gets no path appended (Q82).
    #[test]
    fn operators_stop_the_append() {
        let p = vec![PathBuf::from("a.txt")];
        assert_eq!(substitute_line("echo hi > out.txt", &p), "echo hi > out.txt");
        assert_eq!(substitute_line("dir | findstr x", &p), "dir | findstr x");
        assert_eq!(substitute_line("echo > out.txt $@", &p), "echo > out.txt \"a.txt\"");
        assert_eq!(substitute_line("explorer", &p), "explorer \"a.txt\"");
    }

    /// `start ""` keeps both its quotes.
    ///
    /// `""` after `start` is how a window title is left empty, and without it
    /// `start` reads the program as the title and opens a bare console instead.
    ///
    /// The quotes used to be collapsed by a blanket `"" -> "` over the whole
    /// line, which was meant for a config that quotes the placeholder itself and
    /// caught this idiom as well: `start "" msedge %*` came out as
    /// `start " msedge "C:\…"`, so `start` took `" msedge "C:\…"` for a title and
    /// opened a command prompt with the browser's name across the top.
    #[test]
    fn an_empty_start_title_survives() {
        let p = vec![PathBuf::from(r"C:\d\a b.pdf")];
        let q = "\"C:\\d\\a b.pdf\"";
        assert_eq!(substitute(r#"start "" msedge %*"#, &p), format!("start \"\" msedge {q}"));
        assert_eq!(substitute(r#"start "" %*"#, &p), format!("start \"\" {q}"));
        assert_eq!(substitute(r#"start "" excel %*"#, &p), format!("start \"\" excel {q}"));

        // The case the collapse existed for: the config quotes the placeholder,
        // and every path is quoted on the way out regardless.
        assert_eq!(substitute(r#"nvim "%s""#, &p), format!("nvim {q}"));
        assert_eq!(substitute(r#"nvim "$@""#, &p), format!("nvim {q}"));
        assert_eq!(substitute(r#"start "" msedge "%*""#, &p), format!("start \"\" msedge {q}"));

        // Two paths inside one quoted placeholder still come out as two
        // arguments, not one quoted blob.
        let two = vec![PathBuf::from(r"C:\x.pdf"), PathBuf::from(r"C:\y.pdf")];
        assert_eq!(substitute(r#"code "%*""#, &two), r#"code "C:\x.pdf" "C:\y.pdf""#);

        // A quote that merely sits next to the placeholder, not around it.
        assert_eq!(substitute(r#"say "hi" %*"#, &p), format!("say \"hi\" {q}"));
    }

    /// A path with a space in it, and the program path of an editor installed
    /// somewhere with one -- spelled for the platform the test is running on.
    ///
    /// The rules under test here are about where the line number goes and which
    /// flag carries it, not about which character separates directories. Writing
    /// the paths Windows-only made three tests fail on Linux, where a `C:\…`
    /// string is one path component and so matches no editor at all, and the
    /// answer to that was not to stop testing the rules off Windows -- it is the
    /// same reasoning `cmd_s_c_arg` above is kept unguarded for.
    struct Sample {
        paths: Vec<PathBuf>,
        /// The path as it appears between the quotes.
        raw: &'static str,
        /// A full path to an editor's executable, with a space in a directory.
        nvim: &'static str,
        sakura: &'static str,
        notepadpp: &'static str,
        mikan: &'static str,
    }

    fn sample(name: &str) -> Sample {
        #[cfg(windows)]
        {
            let raw: &'static str = match name {
                "txt" => r"C:\a b\x.txt",
                _ => r"C:\a b\x.rs",
            };
            Sample {
                paths: vec![PathBuf::from(raw)],
                raw,
                nvim: r"C:\Program Files\Neovim\bin\nvim.exe",
                sakura: r"C:\Program Files\sakura\sakura.exe",
                notepadpp: r"C:\Program Files\Notepad++\notepad++.exe",
                mikan: r"C:\Program Files\Mikan\Mikan.exe",
            }
        }
        #[cfg(not(windows))]
        {
            let raw: &'static str = match name {
                "txt" => "/a b/x.txt",
                _ => "/a b/x.rs",
            };
            Sample {
                paths: vec![PathBuf::from(raw)],
                raw,
                nvim: "/opt/n vim/bin/nvim",
                sakura: "/opt/s akura/sakura",
                notepadpp: "/opt/n pp/notepad++",
                mikan: "/opt/m ikan/Mikan",
            }
        }
    }

    #[test]
    fn opens_editors_at_a_line() {
        let s = sample("rs");
        let (p, pn) = (s.raw, format!("{}:12", s.raw));
        let at = |run: &str| at_line(run, &s.paths, 12, &LineArgs::new());
        assert_eq!(at("nvim %s").as_deref(), Some(format!(r#"nvim +12 "{p}""#).as_str()));
        assert_eq!(at("code %*").as_deref(), Some(format!(r#"code -g "{pn}""#).as_str()));
        assert_eq!(at(r#"hx "$@""#).as_deref(), Some(format!(r#"hx "{pn}""#).as_str()));
        assert_eq!(at("Code.CMD").as_deref(), Some(format!(r#"Code.CMD -g "{pn}""#).as_str()));
        // An editor named by its full path, with a space in a directory: the key
        // is matched on the file name without its extension.
        assert_eq!(
            at(&format!(r#""{}" -O %s"#, s.nvim)).as_deref(),
            Some(format!(r#""{}" +12 -O "{p}""#, s.nvim).as_str())
        );
        // A program that takes no line number is opened the plain way.
        assert_eq!(at("explorer %s"), None);
        assert_eq!(at(""), None);
    }

    /// The Windows editors' own flags. The programs only exist there, but the
    /// table that maps a name to a flag is plain data and is worth checking
    /// wherever the tests run.
    #[test]
    fn opens_windows_editors_at_a_line() {
        let s = sample("txt");
        let p = s.raw;
        let at = |run: &str| at_line(run, &s.paths, 123, &LineArgs::new());
        assert_eq!(at("hidemaru %s").as_deref(), Some(format!(r#"hidemaru /j123 "{p}""#).as_str()));
        assert_eq!(at("sakura %s").as_deref(), Some(format!(r#"sakura -Y=123 "{p}""#).as_str()));
        assert_eq!(at("emeditor %s").as_deref(), Some(format!(r#"emeditor /l 123 "{p}""#).as_str()));
        assert_eq!(at("notepad++ %s").as_deref(), Some(format!(r#"notepad++ -n123 "{p}""#).as_str()));
        // Notepad takes no line, so it is opened the plain way.
        assert_eq!(at("notepad %s"), None);
        assert_eq!(
            at(&format!(r#""{}" %s"#, s.sakura)).as_deref(),
            Some(format!(r#""{}" -Y=123 "{p}""#, s.sakura).as_str())
        );
        // `++` in the file name must not stop the key being found.
        assert_eq!(
            at(&format!(r#""{}" %s"#, s.notepadpp)).as_deref(),
            Some(format!(r#""{}" -n123 "{p}""#, s.notepadpp).as_str())
        );
    }

    /// The `[line_args]` config: an editor the built-in table does not know, the
    /// three template shapes, and a configured entry winning over a built-in one.
    #[test]
    fn line_args_from_the_config() {
        let s = sample("txt");
        let p = s.raw;
        let mut custom = LineArgs::new();
        // An editor the built-in table knows nothing about.
        custom.insert("mikan".into(), "-l {line} {path}".into());
        // The three shapes a template can take.
        custom.insert("myedit".into(), "{path}:{line}".into());
        custom.insert("goto".into(), "--goto {path}@{line}".into());
        // A configured editor overrides the built-in table.
        custom.insert("sakura".into(), "/LINE={line} {path}".into());
        let at = |run: &str| at_line(run, &s.paths, 123, &custom);

        assert_eq!(at("mikan %s").as_deref(), Some(format!(r#"mikan -l 123 "{p}""#).as_str()));
        assert_eq!(at("myedit %s").as_deref(), Some(format!(r#"myedit "{p}:123""#).as_str()));
        assert_eq!(at("goto %s").as_deref(), Some(format!(r#"goto --goto "{p}@123""#).as_str()));
        assert_eq!(at("sakura %s").as_deref(), Some(format!(r#"sakura /LINE=123 "{p}""#).as_str()));
        // Keys and programs are matched by file name, without the extension.
        assert_eq!(
            at(&format!(r#""{}" -w %s"#, s.mikan)).as_deref(),
            Some(format!(r#""{}" -l 123 -w "{p}""#, s.mikan).as_str())
        );
        // An opener with no placeholder still gets the path appended.
        assert_eq!(at("mikan").as_deref(), Some(format!(r#"mikan -l 123 "{p}""#).as_str()));
        // Editors outside both the table and the config are unchanged.
        assert_eq!(at("explorer %s"), None);
    }

    /// `$TERMINAL` wins, Terminal.app comes next on macOS, and a blank
    /// variable is no choice at all.
    #[test]
    fn terminals_are_tried_in_order() {
        assert_eq!(terminals(None, false), TERMINALS.map(String::from).to_vec());
        assert_eq!(terminals(Some("kitty -1"), false)[..2], ["kitty -1".to_string(), "x-terminal-emulator".into()]);
        assert_eq!(terminals(Some("  "), true)[0], MAC_TERMINAL);
        assert_eq!(terminals(Some("wezterm"), true)[..2], ["wezterm".to_string(), MAC_TERMINAL.into()]);
    }

    /// Each emulator is told to run the script its own way, and the script
    /// gets the folder and the command line as arguments, untouched.
    #[test]
    fn each_terminal_is_told_to_run_the_script_its_own_way() {
        let cwd = Path::new("/home/me/a b");
        let tail = |argv: &[String], at: usize| argv[at..].to_vec();
        let script = ["sh", "-c", TERMINAL_SCRIPT, "sh", "/home/me/a b", "nvim \"x y\""].map(String::from).to_vec();
        for (term, head) in [
            ("gnome-terminal", vec!["gnome-terminal", "--"]),
            ("wezterm", vec!["wezterm", "start", "--"]),
            ("xfce4-terminal", vec!["xfce4-terminal", "-x"]),
            ("kitty", vec!["kitty"]),
            ("/usr/bin/foot", vec!["/usr/bin/foot"]),
            ("xterm", vec!["xterm", "-e"]),
            ("x-terminal-emulator", vec!["x-terminal-emulator", "-e"]),
            ("kitty --single-instance", vec!["kitty", "--single-instance"]),
            ("Alacritty", vec!["Alacritty", "-e"]),
        ] {
            let argv = terminal_argv(term, cwd, "nvim \"x y\"");
            assert_eq!(argv[..head.len()], head.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "{term}");
            assert_eq!(tail(&argv, head.len()), script, "{term}");
        }
    }

    /// Terminal.app is driven by `osascript`, and what it types is one line
    /// the login shell turns into the same script.
    #[test]
    fn terminal_app_gets_one_escaped_line() {
        let argv = terminal_argv(MAC_TERMINAL, Path::new("/Users/me"), "nvim \"a\\b\"");
        assert_eq!(argv[0], "osascript");
        let line = login_shell_line("/Users/me", "nvim \"a\\b\"");
        assert_eq!(argv[2], format!("tell application \"Terminal\" to do script {}", applescript_string(&line)));
        assert_eq!(argv[4], "tell application \"Terminal\" to activate");
        assert_eq!(applescript_string(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
    }

    /// The script really does go to the folder, run the line, and come back
    /// with its status -- through `sh` itself, and through the login-shell
    /// line macOS uses, with a folder name that needs quoting twice over.
    #[cfg(unix)]
    #[test]
    fn the_terminal_script_runs_the_line_in_the_folder() {
        let dir = crate::util::test_dir("terminal-script");
        let cwd = dir.join("it's a dir");
        std::fs::create_dir(&cwd).unwrap();
        let cwd_s = cwd.to_string_lossy().into_owned();
        let run = |argv: &[&str]| Command::new(argv[0]).args(&argv[1..]).stdin(Stdio::null()).output().unwrap();

        let out = run(&["sh", "-c", TERMINAL_SCRIPT, "sh", &cwd_s, "pwd > here.txt # a comment"]);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(std::fs::read_to_string(cwd.join("here.txt")).unwrap().trim(), cwd_s);

        // A failure says so and waits for Enter; with no input, `read` returns at once.
        let out = run(&["sh", "-c", TERMINAL_SCRIPT, "sh", &cwd_s, "exit_with_7() { return 7; }; exit_with_7"]);
        assert!(String::from_utf8_lossy(&out.stdout).contains("[exit 7] Press Enter to close."), "{out:?}");

        let line = login_shell_line(&cwd_s, "echo 'from the login shell' > mac.txt");
        let out = run(&["sh", "-c", &line]);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(std::fs::read_to_string(cwd.join("mac.txt")).unwrap().trim(), "from the login shell");
    }

    /// The case that sent this looking: an opener whose program is quoted
    /// because its path has a space in it. Under a bare `/C` the line has four
    /// quotes, so `cmd` drops the outer two and runs
    /// `C:\Program Files (x86)\sakura\sakura.exe" "C:\dev\x.toml` — which is
    /// not a program, and whose `(x86)` is now bare parentheses `cmd` reads as
    /// grouping. Nothing opens, and with the console hidden nothing says so.
    #[test]
    fn a_quoted_program_survives_cmd() {
        let line = r#""C:\Program Files (x86)\sakura\sakura.exe" "C:\dev\x.toml""#;
        let arg = cmd_s_c_arg(line);
        // What `/S` does: drop the first character and the last, keep the rest.
        assert!(arg.starts_with('"') && arg.ends_with('"'));
        assert_eq!(&arg[1..arg.len() - 1], line, "the line must arrive untouched");
    }

    /// Q13: the console waits for a key, once.
    #[test]
    fn a_held_line_pauses_once() {
        assert_eq!(held("git log -5"), "git log -5 & pause");
        assert_eq!(held("dir & pause"), "dir & pause");
        assert_eq!(held("dir & PAUSE >nul"), "dir & PAUSE >nul");
        // A word that only contains it is not it.
        assert_eq!(held("echo pauses"), "echo pauses & pause");
    }

    /// Q12: `cmd` by its full path (never one in the browsed folder), and the
    /// line under the same `/S` rule as `shell_command`.
    #[test]
    fn the_new_console_runs_system_cmd() {
        let line = r#""C:\Program Files (x86)\sakura\sakura.exe" "C:\dev\x.toml""#;
        let (app, cmd) = console_command_line(r"C:\Windows\", line);
        assert_eq!(app, r"C:\Windows\System32\cmd.exe");
        assert_eq!(cmd, format!(r#""C:\Windows\System32\cmd.exe" /S /C "{line}""#));
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

