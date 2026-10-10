//! `kura env`: everything a report would otherwise have to ask for.
//!
//! Modelled on yazi's `ya env`, and for the same reason. A bug report that
//! needs "which config files were read", "is `ffmpeg` on the PATH", "what does
//! `EDITOR` say" turns into a conversation of one question per round trip,
//! and each round trip is a day. Printing the lot at once ends that, and the
//! answers are the ones the program already has: it read those files, it looks
//! for those tools, it is running in that environment.
//!
//! It prints rather than opens a browser, unlike `<F12>`, because this is text
//! to paste into a report that already exists.

use std::process::Command;

/// The whole report.
pub fn text() -> String {
    let cfg = crate::config::Config::load();
    let mut out = String::new();
    section(&mut out, "Kura", &version());
    section(&mut out, "Config", &config(&cfg));
    section(&mut out, "Last run", &last_run());
    section(&mut out, "Tools", &tools(&cfg));
    section(&mut out, "Variables", &variables());
    // One newline after the last row, not the blank line every section ends
    // with: the report's line count came out 47, 48 or 49 depending on how it
    // was counted (#189).
    out.truncate(out.trim_end_matches('\n').len());
    out.push('\n');
    out
}

fn section(out: &mut String, title: &str, rows: &[(String, String)]) {
    out.push_str(title);
    out.push('\n');
    // A key longer than this (a config directory, as long as the path) goes on
    // a line of its own with the value under it, so the column does not push
    // every other row past the width of a console (#248).
    const MAX_KEY: usize = 24;
    let width = rows.iter().map(|(k, _)| k.chars().count()).filter(|&n| n <= MAX_KEY).max().unwrap_or(0);
    for (k, v) in rows {
        // A value that runs to several lines is indented under its own key, so
        // the column stays readable however long the answer is.
        let mut lines = v.split('\n');
        let first = lines.next().unwrap_or_default();
        if k.chars().count() > MAX_KEY {
            out.push_str(&format!("    {k}\n    {:width$}   {first}\n", ""));
        } else {
            out.push_str(&format!("    {k:width$} : {first}\n"));
        }
        for rest in lines {
            out.push_str(&format!("    {:width$}   {rest}\n", ""));
        }
    }
    out.push('\n');
}

fn version() -> Vec<(String, String)> {
    // `os_line` is the bug report's own, already carrying both architectures,
    // so this does not repeat them: on Windows on ARM the two disagree and the
    // disagreement is the finding, which a second copy would only muddle.
    let mut rows = vec![("Version".into(), env!("CARGO_PKG_VERSION").to_string())];
    // Which `.exe` this is: with two builds side by side, nothing else in the
    // output said which one had answered (#84).
    let exe = std::env::current_exe().map_or_else(|e| format!("(unknown: {e})"), |p| p.display().to_string());
    rows.push(("Executable".into(), exe));
    for line in crate::bugreport::os_line().split('\n') {
        match line.split_once(": ") {
            Some((k, v)) => rows.push((k.to_string(), v.to_string())),
            None => rows.push((line.to_string(), String::new())),
        }
    }
    rows.push(("Debug".into(), cfg!(debug_assertions).to_string()));
    rows
}

/// Which files were read, and which were looked for and were not there.
///
/// The second half is the useful one: "it is not picking up my theme" is
/// almost always a file in the other directory, or a name spelled differently,
/// and a list of what was found cannot show that. Saying where it looked, and
/// that nothing was there, can.
fn config(cfg: &crate::config::Config) -> Vec<(String, String)> {
    let dirs = crate::config::config_dirs();
    let mut rows = Vec::new();
    for dir in &dirs {
        // A row per directory rather than per file: four "No such file or
        // directory" lines per directory is the same fact eight times, and
        // buries the one file that is there.
        let (mut found, mut missing) = (Vec::new(), Vec::new());
        for name in crate::config::FILES {
            match std::fs::metadata(dir.join(name)) {
                Ok(m) => found.push(format!("{name} {}", crate::util::human_size(m.len()))),
                Err(_) => missing.push(name),
            }
        }
        let said = match found.is_empty() {
            true => "nothing here".to_string(),
            false => match missing.is_empty() {
                true => found.join("   "),
                false => format!("{}\nnot here: {}", found.join("   "), missing.join(", ")),
            },
        };
        rows.push((format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR), said));
    }
    // Not a config file, but the other directory kura touches: bookmarks,
    // the jump history and the window size are written here, and "delete this
    // and try again" is a step a report is often asked to take.
    rows.push(("State".into(), crate::config::Config::state_dir().display().to_string()));
    // What draws the window next time, and what decides it: the env var wins
    // for a run, then `[ui] backend` (Q70). The adapter it got is the `Adapter`
    // row of the last run, below.
    let wgpu_env = std::env::var("WGPU_BACKEND").ok().filter(|v| !v.is_empty());
    rows.push(("Backend".into(), backend_row(cfg, wgpu_env, crate::predicted_fallback)));
    // Said as the window says it: what the setting fell back to, not "the default".
    let mut warnings = cfg.warnings.clone();
    if warnings.iter().any(|w| w.contains("[ui] backend")) {
        crate::name_the_fallback(&mut warnings, &crate::predicted_fallback());
    }
    rows.push(("Warnings".into(), match warnings.len() {
        0 => "none".into(),
        _ => warnings.join("\n"),
    }));
    // Not warnings: a default key a file of yours rebinds, listed so what it
    // displaced can be looked up (Q60).
    if !cfg.keymap.overrides.is_empty() {
        rows.push(("Overrides".into(), cfg.keymap.overrides.join("\n")));
    }
    rows
}

/// The outside programs kura actually runs, and whether they are there.
///
/// Only the ones it really does run. Listing a tool kura has no code for
/// would be the worst kind of wrong in a diagnostic: it reads as a dependency,
/// and "not found" next to it sends the reader off installing something that
/// changes nothing. Previews and archives are handled in-process and need
/// none of this.
///
/// The shell is the one that will actually be launched -- `[term] shell` when
/// it is set, and the platform's own default when it is not. Probing `pwsh`
/// regardless would report "not found" on a machine whose terminal pane works
/// perfectly well on Windows PowerShell.
fn tools(cfg: &crate::config::Config) -> Vec<(String, String)> {
    let mut rows = vec![row("git", "--version", "the status column")];

    let shell = match cfg.term.shell.is_empty() {
        false => cfg.term.shell.clone(),
        true => crate::terminal::default_program(),
    };
    rows.push((shell.clone(), found_with_version(&shell, &shell_source(&cfg.term))));
    #[cfg(windows)]
    rows.push(("ConPTY".into(), conpty_source(std::env::current_exe().ok().as_deref())));
    // Where a `block = true` opener runs. Windows gives it a console of its
    // own; anywhere else a terminal emulator has to be there to open.
    #[cfg(not(windows))]
    rows.push(terminal_row());

    // What `<Enter>` will try to run. An opener naming something that is not
    // installed fails at the moment it is pressed and not before, which is
    // exactly the report that arrives with no other evidence.
    let mut seen: Vec<String> = Vec::new();

    // What `[[preview]]` names. These are run on the preview thread every time
    // the cursor lands on a matching file, so one that is not installed is a
    // pane full of the same error over and over.
    for r in &cfg.preview {
        let Some(exe) = program(&r.run) else { continue };
        if seen.contains(&exe) {
            continue;
        }
        seen.push(exe.clone());
        rows.push((exe.clone(), found(&exe, &format!("preview {}", r.pattern))));
    }

    for (kind, openers) in &cfg.yazi.opener {
        for o in openers {
            let Some(exe) = program(&o.run) else { continue };
            if seen.contains(&exe) {
                continue;
            }
            seen.push(exe.clone());
            rows.push((exe.clone(), found(&exe, &format!("opener [{kind}]"))));
        }
    }
    rows
}

/// Which ConPTY the pane runs on. The DLL search is limited to the exe's
/// folder and System32 (`restrict_dll_search`), so the answer is whether a
/// `conpty.dll` sits beside the exe; a pane bug is often an old ConPTY (#184).
#[cfg(any(windows, test))]
fn conpty_source(exe: Option<&std::path::Path>) -> String {
    match exe.and_then(|e| e.parent()).map(|d| d.join("conpty.dll")) {
        Some(dll) if dll.is_file() => format!("{}   (beside kura.exe)", dll.display()),
        _ => "built into Windows   (no conpty.dll beside kura.exe)".into(),
    }
}

/// The terminal a `block = true` opener will open, found the way
/// `exec::shell` finds it: the first of the list that is there.
#[cfg(not(windows))]
fn terminal_row() -> (String, String) {
    let what = "block = true openers";
    let env = std::env::var("TERMINAL").ok();
    for term in crate::exec::terminals(env.as_deref(), cfg!(target_os = "macos")) {
        if term == crate::exec::MAC_TERMINAL {
            return (term, format!("via osascript   ({what})"));
        }
        let exe = term.split_whitespace().next().unwrap_or_default();
        if let Some(p) = crate::util::locate(exe) {
            return (term, format!("{}   ({what})", p.display()));
        }
    }
    ("terminal".into(), format!("none found, set TERMINAL   ({what})"))
}

fn row(exe: &str, flag: &str, what: &str) -> (String, String) {
    let said = match probe(exe, flag) {
        Some(v) => format!("{v}   ({what})"),
        None => format!("not found   ({what})"),
    };
    (exe.to_string(), said)
}

/// Whether a program is there, **without running it**.
///
/// Shells and openers are looked up rather than asked. Two reasons, and the
/// second is the serious one. Shells disagree about how to be asked: `pwsh`
/// takes `--version`, Windows PowerShell does not, and `sh` answers `Illegal
/// option --`, so a version column would be wrong more often than right. And
/// an opener is a command line out of the user's own config -- running it to
/// see if it exists would launch their editor, or their image viewer, or
/// whatever else they have put there, every time they asked what was wrong.
/// Where the pane's shell came from, and the `[term] args` that
/// `KURA_TERM_SHELL` left out, when there were any (#190).
fn shell_source(term: &crate::config::TermCfg) -> String {
    let said = shell_source_base(term);
    match (term.args_from_env, term.args_unused) {
        (true, _) => format!("{said}; args from KURA_TERM_ARGS: {}", term.args.join(" ")),
        (_, true) => format!("{said}; KURA_TERM_ARGS not used: it needs KURA_TERM_SHELL"),
        _ => said,
    }
}

fn shell_source_base(term: &crate::config::TermCfg) -> String {
    match (term.shell.is_empty(), term.from_env) {
        (false, true) if term.dropped_args.is_empty() => "terminal pane, from KURA_TERM_SHELL".into(),
        (false, true) => format!("terminal pane, from KURA_TERM_SHELL; [term] args not used: {}", term.dropped_args.join(" ")),
        (false, false) => "terminal pane, from [term] shell".into(),
        (true, _) => "terminal pane, the platform default".into(),
    }
}

fn found(exe: &str, what: &str) -> String {
    if shell_builtin(exe) {
        return format!("built into {SHELL_NAME}   ({what})");
    }
    match crate::util::locate(exe) {
        Some(p) => format!("{}   ({what})", p.display()),
        None => format!("not found   ({what})"),
    }
}

/// `found`, plus the file's version on Windows, read from the version resource
/// so that nothing is run (25.4d). `pwsh` 7.4 and Windows PowerShell 5.1 differ
/// in ways a pane bug report needs to know (#258).
fn found_with_version(exe: &str, what: &str) -> String {
    let said = found(exe, what);
    #[cfg(windows)]
    if let Some(v) = crate::util::locate(exe).and_then(|p| file_version(&p)) {
        return said.replacen("   (", &format!("   v{v}   ("), 1);
    }
    said
}

/// `major.minor.build.revision` from a file's version resource.
#[cfg(windows)]
fn file_version(path: &std::path::Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO};
    use windows::core::{w, PCWSTR};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the calls; `buf` is sized by
    // the call that reports the size, and the pointer `VerQueryValueW` hands back
    // points into `buf`, which is read before `buf` is dropped.
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, buf.as_mut_ptr().cast()).ok()?;
        let mut info: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(buf.as_ptr().cast(), w!("\\"), &mut info, &mut len).as_bool() || info.is_null() || (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }
        // The string table first: under the compatibility layer that makes an app
        // see Windows 6.2 (no `supportedOS` manifest), the fixed block of a system
        // file reads `6.2.x` where the string says `10.0.x` (25.4e, ARM64).
        if let Some(v) = string_file_version(&buf) {
            return Some(v);
        }
        let f = &*(info as *const VS_FIXEDFILEINFO);
        Some(format!("{}.{}.{}.{}", f.dwFileVersionMS >> 16, f.dwFileVersionMS & 0xffff, f.dwFileVersionLS >> 16, f.dwFileVersionLS & 0xffff))
    }
}

/// `FileVersion` from the first language of the string table, cut at the first
/// space (`10.0.28000.2113 (WinBuild...)` -> `10.0.28000.2113`).
///
/// # Safety
/// `buf` must be a block filled by `GetFileVersionInfoW`.
#[cfg(windows)]
unsafe fn string_file_version(buf: &[u8]) -> Option<String> {
    use windows::Win32::Storage::FileSystem::VerQueryValueW;
    use windows::core::{w, PCWSTR};
    let mut p: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut len = 0u32;
    if !VerQueryValueW(buf.as_ptr().cast(), w!("\\VarFileInfo\\Translation"), &mut p, &mut len).as_bool() || p.is_null() || len < 4 {
        return None;
    }
    let (lang, page) = (*(p as *const u16), *(p as *const u16).add(1));
    let key: Vec<u16> = format!("\\StringFileInfo\\{lang:04x}{page:04x}\\FileVersion").encode_utf16().chain(Some(0)).collect();
    if !VerQueryValueW(buf.as_ptr().cast(), PCWSTR(key.as_ptr()), &mut p, &mut len).as_bool() || p.is_null() || len == 0 {
        return None;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, len as usize));
    let v = text.trim_end_matches('\0').split_whitespace().next()?;
    (v.split('.').count() == 4 && v.split('.').all(|n| n.parse::<u32>().is_ok())).then(|| v.to_string())
}

#[cfg(windows)]
const SHELL_NAME: &str = "cmd";
#[cfg(not(windows))]
const SHELL_NAME: &str = "sh";

/// Commands the shell implements itself, which are not files and so can never
/// be found on `PATH`.
///
/// `start` is the one that matters. It is how an opener reaches a GUI program
/// that is not on `PATH` -- `excel`, `msedge`, the default handler -- which
/// makes it the commonest first word in an opener list, and every one of them
/// was being reported as missing. That is v0.28.1's mistake from the other
/// side: there a working tool was asked the wrong question, here it is looked
/// for in the wrong place. Either way a diagnostic said a working thing was
/// broken, which is the worst thing this file can do.
fn shell_builtin(exe: &str) -> bool {
    #[cfg(windows)]
    // `cmd` is case-insensitive about its own names, so `START` is `start`.
    let (name, known) = (
        exe.to_ascii_lowercase(),
        ["start", "call", "echo", "type", "cd", "set", "copy", "del", "dir", "move", "rem"],
    );
    #[cfg(not(windows))]
    let (name, known) =
        (exe.to_string(), ["echo", "cd", "export", "eval", "exec", "set", "test", "printf"]);

    known.contains(&name.as_str())
}

/// The program an opener's command line starts with.
///
/// Quoted when it holds a space, which a full path usually does -- and a path
/// is the common case here, since that is how an editor outside the `PATH` is
/// named.
pub(crate) fn program(run: &str) -> Option<String> {
    let run = run.trim();
    let exe = match run.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => run.split_whitespace().next()?,
    };
    match exe.is_empty() {
        true => None,
        false => Some(exe.to_owned()),
    }
}

/// A program's version, or nothing when it is not on the PATH.
///
/// The first line only, and trimmed: these tools answer with anything from one
/// word to a paragraph of build flags, and the paragraph is not what a report
/// needs. The whole thing is run with a clean stdin so nothing can sit waiting
/// for input.
fn probe(exe: &str, flag: &str) -> Option<String> {
    let out = Command::new(exe)
        .arg(flag)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = match out.stdout.is_empty() {
        true => String::from_utf8_lossy(&out.stderr).into_owned(),
        false => String::from_utf8_lossy(&out.stdout).into_owned(),
    };
    let line = text.lines().next()?.trim();
    // ffmpeg and friends put a paragraph of copyright on the same line as the
    // version. The version is the part a report needs.
    let line = line.split(" Copyright").next().unwrap_or(line).trim();
    match line.is_empty() {
        true => None,
        false => Some(line.to_owned()),
    }
}


/// The `Terminal pane` row: what the last run's pane did.
fn pane_row(info: &crate::runinfo::RunInfo) -> String {
    match (info.pane, info.pane_shell.as_str()) {
        ([0, _] | [_, 0], "") if !info.pane_failed.is_empty() => format!("did not start: {}", info.pane_failed),
        ([0, _] | [_, 0], "") => "not opened in that run".into(),
        ([0, _] | [_, 0], shell) => format!("{shell}, not drawn"),
        ([lines, cols], "") => format!("{lines} x {cols} (lines x columns)"),
        ([lines, cols], shell) => format!("{shell}, {lines} x {cols} (lines x columns)"),
    }
}

/// What the window used, read back from what the last run wrote down.
///
/// Neither of these can be worked out from here: the adapter is wgpu's choice
/// at startup, and the fonts are a search whose result depends on what is
/// installed. Both are the answer to a complaint that has no other answer --
/// a blank or slow window is the adapter, and boxes instead of icons is the
/// font — and both are invisible to every other kind of investigation.
fn last_run() -> Vec<(String, String)> {
    let Some(info) = crate::runinfo::load() else {
        return vec![(
            "Rendering".into(),
            "not recorded — kura has not opened a window on this machine yet".into(),
        )];
    };
    let list = |paths: &[std::path::PathBuf], none: &str| match paths.is_empty() {
        true => none.to_string(),
        false => paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"),
    };
    let mut rows = vec![
        ("Adapter".into(), match info.adapter.is_empty() {
            true => "not recorded".into(),
            false if info.backend_setting.is_empty() => format!("{}   ({}, {})", info.adapter, info.backend, info.device),
            false => format!("{}   ({}, {}; set by {})", info.adapter, info.backend, info.device, info.backend_setting),
        }),
        // Which start this record is: with the same version, one from days
        // ago read as the run just made (#244).
        ("Started".into(), info.started_line(std::time::SystemTime::now()).unwrap_or_else(|| "not recorded".into())),
        // Above the fonts because it is the one row that settles an argument:
        // what kura thinks its own window is, as against what a capture or a
        // script measured from outside.
        ("Window".into(), info.window_line().unwrap_or_else(|| {
            "not recorded — no frame was drawn before the record was written".into()
        })),
        ("Terminal pane".into(), pane_row(&info)),
        // What was launched, as the command lines kura built (Q40): an opener
        // that ran the wrong thing is visible here after the toast has gone.
        ("Launched".into(), match info.launched.is_empty() {
            true => "nothing in that run".into(),
            false => info.launched.join("\n"),
        }),
        ("Fonts".into(), list(&info.fonts, "none loaded — this is why icons are boxes")),
        ("Bold".into(), list(&info.bold, "none found; bold is faked by overstriking")),
    ];
    // A record left by an older kura describes an older kura. Saying so
    // costs a line and stops a stale answer being read as a current one.
    if info.version != env!("CARGO_PKG_VERSION") {
        rows.push(("Recorded by".into(), format!("kura {} — an earlier run", info.version)));
    }
    rows
}

fn variables() -> Vec<(String, String)> {
    ["EDITOR", "VISUAL", "SHELL", "TERM", "YAZI_CONFIG_HOME", "KURA_CONFIG_HOME", "KURA_STATE_HOME", "KURA_TERM_SHELL", "KURA_TERM_ARGS", "KURA_SCALE"]
        .iter()
        .map(|k| (k.to_string(), std::env::var(k).unwrap_or_else(|_| "unset".into())))
        .collect()
}

/// The `Backend` row: the env var, else the setting, with `auto` completed by
/// `fallback` (only called for `auto`, which is when it is wanted).
fn backend_row(cfg: &crate::config::Config, wgpu_env: Option<String>, fallback: impl FnOnce() -> String) -> String {
    match wgpu_env {
        Some(v) => format!("{v} (from WGPU_BACKEND; [ui] backend = \"{}\" not used)", cfg.ui.backend),
        None => {
            let mut said = format!("[ui] backend = \"{}\"", cfg.ui.backend);
            // What `auto` comes to on this machine: GL where it has one
            // (Windows), wgpu's own pick elsewhere (#245).
            if matches!(cfg.ui.backend_name(), Ok(None)) {
                said += &auto_note(&fallback());
            }
            said
        }
    }
}

/// The tail of the `Backend` row for `auto`, from `predicted_fallback`.
fn auto_note(fallback: &str) -> String {
    match fallback {
        "" => " (wgpu's own pick here)".to_string(),
        b => format!(" (this machine: {b})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::locate;

    /// What `[ui] backend = "auto"` is said to come to: GL where the machine
    /// has it, wgpu's own pick where `predicted_fallback` says nothing.
    #[test]
    fn auto_backend_note() {
        assert_eq!(auto_note("Gl"), " (this machine: Gl)");
        assert_eq!(auto_note(""), " (wgpu's own pick here)");
    }

    /// Only `auto` gets a tail; a named backend or the env var says what it is.
    #[test]
    fn only_auto_gets_a_note() {
        let mut cfg = crate::config::Config::load();
        let boom = || -> String { panic!("fallback asked for a non-auto backend") };
        cfg.ui.backend = "gl".into();
        assert_eq!(backend_row(&cfg, None, boom), "[ui] backend = \"gl\"");
        assert!(backend_row(&cfg, Some("vulkan".into()), boom).starts_with("vulkan (from WGPU_BACKEND"));
        cfg.ui.backend = "auto".into();
        assert_eq!(backend_row(&cfg, None, || "Gl".into()), "[ui] backend = \"auto\" (this machine: Gl)");
    }

    /// #248: a long key does not widen the column for every other row.
    #[test]
    fn a_long_key_does_not_widen_the_column() {
        let long = "x".repeat(67);
        let rows = vec![(long.clone(), "nothing here".to_string()), ("State".to_string(), "/s".to_string())];
        let mut out = String::new();
        section(&mut out, "Config", &rows);
        assert!(out.contains(&format!("    {long}\n")), "{out}");
        assert!(out.lines().any(|l| l == "    State : /s"), "{out}");
        assert!(out.lines().all(|l| l.chars().count() < 40 || l.trim() == long), "{out}");
    }

    /// #254: a pane that would not start is not "not opened".
    #[test]
    fn a_pane_that_failed_to_start_says_so() {
        let mut info = crate::runinfo::RunInfo::default();
        assert_eq!(pane_row(&info), "not opened in that run");
        info.pane_failed = "`nosuch` was not found on PATH".into();
        assert_eq!(pane_row(&info), "did not start: `nosuch` was not found on PATH");
        info.pane_shell = "pwsh".into();
        info.pane = [12, 159];
        assert_eq!(pane_row(&info), "pwsh, 12 x 159 (lines x columns)");
    }

    /// `start` is not a file and never will be, so the `PATH` lookup that
    /// serves every other entry can only say "not found" about the commonest
    /// opener there is. Reported from a real machine: every `start ""`-based
    /// opener in the list looked broken while all of them worked.
    #[test]
    #[cfg(windows)]
    fn a_cmd_builtin_is_not_missing() {
        let said = found("start", "opener [browser]");
        assert!(said.starts_with("built into cmd"), "{said}");
        assert!(!said.contains("not found"), "{said}");
        // Case follows `cmd`, which does not care.
        assert!(found("START", "x").starts_with("built into cmd"));
    }

    #[test]
    #[cfg(not(windows))]
    fn a_sh_builtin_is_not_missing() {
        assert!(found("echo", "opener [x]").starts_with("built into sh"));
    }

    /// The exemption is for the shell's own names only; anything else is still
    /// looked for, so a genuinely absent program still reads as absent.
    #[test]
    fn a_real_program_is_still_looked_for() {
        assert!(!shell_builtin("pdftoppm"));
        assert!(!shell_builtin("i_view64.exe"));
        assert!(!shell_builtin(r"C:\Program Files\IrfanView\i_view64.exe"));
        assert!(found("definitely-not-a-program-93f2", "opener [edit]").starts_with("not found"));
    }

    /// The column lines up, and an answer of several lines stays under its own
    /// key rather than starting a new column.
    ///
    /// Config warnings are the multi-line case and the one that matters: a
    /// TOML parse error arrives with its own little diagram, and it has to
    /// still read as one answer to one question.
    #[test]
    fn a_long_answer_stays_in_its_column() {
        let mut out = String::new();
        section(&mut out, "Bits", &[
            ("short".into(), "yes".into()),
            ("a longer key".into(), "one\ntwo\nthree".into()),
        ]);
        assert_eq!(
            out,
            "Bits\n\
             \x20   short        : yes\n\
             \x20   a longer key : one\n\
             \x20                  two\n\
             \x20                  three\n\n",
        );
    }

    /// A version line is the version, not the copyright that follows it.
    ///
    /// ffmpeg answers `ffmpeg version 8.1.2-full_build-… Copyright (c) 2000-…
    /// the FFmpeg developers`, all on one line, and the half after `Copyright`
    /// is the same for everyone.
    #[test]
    fn the_copyright_is_not_part_of_the_version() {
        let cut = |s: &str| s.split(" Copyright").next().unwrap_or(s).trim().to_string();
        assert_eq!(
            cut("ffmpeg version 8.1.2-full_build Copyright (c) 2000-2026 the FFmpeg developers"),
            "ffmpeg version 8.1.2-full_build",
        );
        // Lines without one are left exactly as they are.
        assert_eq!(cut("git version 2.52.0.windows.1"), "git version 2.52.0.windows.1");
        assert_eq!(cut("PowerShell 7.6.6"), "PowerShell 7.6.6");
    }

    /// An opener's command line starts with the program, quoted or not.
    ///
    /// Quoted is the case that matters: an editor outside the `PATH` is named
    /// by its full path, and a full path on Windows nearly always has a space
    /// in it, so splitting on whitespace would report `C:/Program` as missing.
    #[test]
    fn the_program_is_taken_off_the_front_of_an_opener() {
        assert_eq!(program("code %s").as_deref(), Some("code"));
        assert_eq!(program("  notepad  %s  ").as_deref(), Some("notepad"));
        assert_eq!(
            program(r#""C:/Program Files/Hidemaru/Hidemaru.exe" /j%l %s"#).as_deref(),
            Some("C:/Program Files/Hidemaru/Hidemaru.exe"),
        );
        assert_eq!(program("").as_deref(), None);
        assert_eq!(program("   ").as_deref(), None);
    }

    /// Looking a program up must not run it.
    ///
    /// An opener is a command line out of the user's own config. Asking it for
    /// a version to see whether it is installed would launch their editor --
    /// or whatever else is in there -- every time they asked what was wrong.
    #[test]
    fn a_program_is_located_not_executed() {
        // Something that certainly exists, found by an absolute path.
        let me = std::env::current_exe().unwrap();
        assert_eq!(locate(me.to_str().unwrap()), Some(me));
        // And something that certainly does not.
        assert_eq!(locate("kura-no-such-program-anywhere"), None);
        assert_eq!(locate("/no/such/path/at/all"), None);
    }

    /// #190: the args `KURA_TERM_SHELL` set aside are named, and only when
    /// there were some.
    #[test]
    fn the_shell_row_names_the_args_the_variable_dropped() {
        use crate::config::TermCfg;
        let env = |dropped: &[&str]| TermCfg {
            shell: "bash".into(),
            from_env: true,
            dropped_args: dropped.iter().map(|a| a.to_string()).collect(),
            ..TermCfg::default()
        };
        assert_eq!(shell_source(&env(&[])), "terminal pane, from KURA_TERM_SHELL");
        assert_eq!(
            shell_source(&env(&["-NoLogo", "-NoProfile"])),
            "terminal pane, from KURA_TERM_SHELL; [term] args not used: -NoLogo -NoProfile"
        );
        let file = TermCfg { shell: "pwsh".into(), args: vec!["-NoLogo".into()], ..TermCfg::default() };
        assert_eq!(shell_source(&file), "terminal pane, from [term] shell");
        assert_eq!(shell_source(&TermCfg::default()), "terminal pane, the platform default");
    }

    #[test]
    fn conpty_is_the_one_beside_the_exe_or_the_built_in() {
        let dir = crate::util::test_dir("conpty-row");
        let exe = dir.join("kura.exe");
        assert!(conpty_source(Some(&exe)).starts_with("built into Windows"));
        std::fs::write(dir.join("conpty.dll"), b"").unwrap();
        let row = conpty_source(Some(&exe));
        assert!(row.contains("conpty.dll") && row.ends_with("(beside kura.exe)"), "{row}");
        assert!(conpty_source(None).starts_with("built into Windows"));
    }

    /// Nothing in the report may be a guess.
    #[test]
    fn it_reports_what_is_actually_there() {
        let text = text();
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "the version is its own");
        // Every section is present even when a machine has none of the tools.
        for title in ["Kura", "Config", "Tools", "Variables"] {
            assert!(text.contains(title), "{title} is missing:\n{text}");
        }
        // A tool says what it is for, found or not, so the reader learns what
        // they are missing rather than only that it is absent.
        assert!(text.contains("(the status column)"), "{text}");
        // Which binary answered, by its own path (#84).
        let exe = std::env::current_exe().unwrap();
        let row = text.lines().find(|l| l.trim_start().starts_with("Executable")).unwrap_or_default();
        assert!(row.ends_with(&format!(": {}", exe.display())), "{text}");
        // And only tools kura really runs: naming one it has no code for
        // reads as a dependency and sends the reader off installing something
        // that changes nothing.
        for never_run in ["pdftoppm", "ffmpeg", "ffprobe"] {
            assert!(!text.contains(never_run), "{never_run} is not used yet:\n{text}");
        }
        // It ends at its last row: no blank lines to count or not (#189).
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"), "{:?}", &text[text.len().saturating_sub(40)..]);
    }
}
