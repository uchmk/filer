//! `filer env`: everything a report would otherwise have to ask for.
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
    let mut out = String::new();
    section(&mut out, "Filer", &version());
    section(&mut out, "Config", &config());
    section(&mut out, "Last run", &last_run());
    section(&mut out, "Tools", &tools());
    section(&mut out, "Variables", &variables());
    out
}

fn section(out: &mut String, title: &str, rows: &[(String, String)]) {
    out.push_str(title);
    out.push('\n');
    let width = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    for (k, v) in rows {
        // A value that runs to several lines is indented under its own key, so
        // the column stays readable however long the answer is.
        let mut lines = v.split('\n');
        let first = lines.next().unwrap_or_default();
        out.push_str(&format!("    {k:width$} : {first}\n"));
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
fn config() -> Vec<(String, String)> {
    let dirs = crate::config::config_dirs();
    let mut rows = Vec::new();
    for dir in &dirs {
        // A row per directory rather than per file: four "No such file or
        // directory" lines per directory is the same fact eight times, and
        // buries the one file that is there.
        let (mut found, mut missing) = (Vec::new(), Vec::new());
        for name in ["yazi.toml", "keymap.toml", "theme.toml", "filer.toml"] {
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
    // Not a config file, but the other directory filer touches: bookmarks,
    // the jump history and the window size are written here, and "delete this
    // and try again" is a step a report is often asked to take.
    rows.push(("State".into(), crate::config::Config::state_dir().display().to_string()));
    let cfg = crate::config::Config::load();
    rows.push(("Warnings".into(), match cfg.warnings.len() {
        0 => "none".into(),
        _ => cfg.warnings.join("\n"),
    }));
    rows
}

/// The outside programs filer can use, and whether they are there.
///
/// None of these is required — filer previews and unpacks in-process — but
/// each one it can find widens what it does, and "it works on my machine" is
/// usually one of these lines differing.
fn tools() -> Vec<(String, String)> {
    [
        // `-v`, not `--version`: poppler's tools take the short one, and the
        // long one is read as a filename -- so the answer was an I/O error
        // about a file called `--version`, which reads as a broken install of
        // a tool that is in fact fine.
        ("pdftoppm", "-v", "PDF pages"),
        ("ffmpeg", "-version", "video frames"),
        ("ffprobe", "-version", "video duration"),
        ("pwsh", "--version", "terminal pane"),
        ("git", "--version", "the status column"),
    ]
    .iter()
    .map(|(exe, flag, what)| {
        let said = match probe(exe, flag) {
            Some(v) => format!("{v}   ({what})"),
            None => format!("not found   ({what})"),
        };
        (exe.to_string(), said)
    })
    .collect()
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
            "not recorded — filer has not opened a window on this machine yet".into(),
        )];
    };
    let list = |paths: &[std::path::PathBuf], none: &str| match paths.is_empty() {
        true => none.to_string(),
        false => paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"),
    };
    let mut rows = vec![
        ("Adapter".into(), match info.adapter.is_empty() {
            true => "not recorded".into(),
            false => format!("{}   ({}, {})", info.adapter, info.backend, info.device),
        }),
        ("Fonts".into(), list(&info.fonts, "none loaded — this is why icons are boxes")),
        ("Bold".into(), list(&info.bold, "none found; bold is faked by overstriking")),
    ];
    // A record left by an older filer describes an older filer. Saying so
    // costs a line and stops a stale answer being read as a current one.
    if info.version != env!("CARGO_PKG_VERSION") {
        rows.push(("Recorded by".into(), format!("filer {} — an earlier run", info.version)));
    }
    rows
}

fn variables() -> Vec<(String, String)> {
    ["EDITOR", "VISUAL", "SHELL", "TERM", "YAZI_CONFIG_HOME", "FILER_CONFIG_HOME", "FILER_STATE_HOME"]
        .iter()
        .map(|k| (k.to_string(), std::env::var(k).unwrap_or_else(|_| "unset".into())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Nothing in the report may be a guess.
    #[test]
    fn it_reports_what_is_actually_there() {
        let text = text();
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "the version is its own");
        // Every section is present even when a machine has none of the tools.
        for title in ["Filer", "Config", "Tools", "Variables"] {
            assert!(text.contains(title), "{title} is missing:\n{text}");
        }
        // A tool says which feature it is for, found or not, so the reader
        // learns what they are missing rather than only that it is absent.
        assert!(text.contains("(PDF pages)"), "{text}");
        assert!(text.contains("(video frames)"), "{text}");
    }
}
