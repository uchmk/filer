//! A picture drawn by somebody else.
//!
//! filer reads what it can in-process, which is most things, and nothing at
//! all of PDF or video: both want a renderer far larger than the rest of the
//! program. The shell's thumbnail handler gives one picture — page one, or the
//! poster frame — and has no way to ask for a second.
//!
//! So `[[preview]]` names a command instead. yazi solves it the same way and
//! for the same reason, and the tools it reaches for (`pdftoppm`, `ffmpeg`)
//! are the ones everybody already has. One mechanism covers both formats
//! because both are asking the same question: picture number `n` of this file.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::PreviewRule;
use crate::util::decode_stderr;

/// A picture from the command, or why there is none.
#[derive(Debug)]
pub struct Drawn {
    pub png: PathBuf,
    /// Kept alive: dropping it removes the directory the picture is in.
    pub _dir: TempDir,
}

/// Run `rule` for picture `n` of `path`.
///
/// The command writes into a directory of our own, which is then searched for
/// whatever it produced. Searching rather than predicting: `pdftoppm` is given
/// a *prefix* and appends `-01.png` or `.png` depending on its flags and
/// version, `ffmpeg` writes exactly what it is told, and pinning the mechanism
/// to either one's habits would break the other.
pub fn draw(rule: &PreviewRule, path: &Path, n: i64) -> Result<Drawn, String> {
    let dir = TempDir::new()?;
    let out = dir.0.join("page");
    let line = fill(&rule.run, path, &out, n);

    let output = shell(&line).map_err(|e| format!("{}: {e}", first_word(&rule.run)))?;
    let png = newest_file(&dir.0);
    match png {
        Some(png) => Ok(Drawn { png, _dir: dir }),
        // The command's own words, which are the useful ones: "Wrong page
        // range" from pdftoppm is what says the document ended.
        None => Err(match one_line(&decode_stderr(&output.stderr)) {
            said if said.is_empty() => format!("{} produced no picture", first_word(&rule.run)),
            said => said,
        }),
    }
}

/// All of what was said, on one line. `cmd` wraps its "not recognized" text over
/// two lines, and keeping only the first ended it at `internal or external
/// command,` (#274).
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Substitute into the command line.
///
/// A placeholder is expanded and then the **whole word around it** is quoted,
/// not the placeholder alone. `{out}.png` has to come out as `"…\page.png"`,
/// and quoting only the placeholder gives `"…\page".png` — which `sh` happens
/// to glue back together and `cmd` does not, so it passed every test here and
/// failed on Windows, where the quotes end up inside the filename and the file
/// cannot be created. `pdftoppm` was unaffected because its `{out}` stands
/// alone; `ffmpeg`, which is told `{out}.png`, never drew a single frame.
///
/// Quoting is filer's job either way: a rule that had to quote `{path}` itself
/// would be wrong on the first name with a space in it.
fn fill(run: &str, path: &Path, out: &Path, n: i64) -> String {
    run.split_whitespace()
        .map(|word| {
            let has_path = word.contains("{path}") || word.contains("{out}");
            let filled = word
                .replace("{path}", &path.display().to_string())
                .replace("{out}", &out.display().to_string())
                .replace("{n}", &n.to_string());
            // Left alone when the rule quoted it already, so a config written
            // the careful way is not broken by the careful thing being done
            // for it.
            match has_path && !filled.starts_with('"') {
                true => format!("\"{filled}\""),
                false => filled,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_word(run: &str) -> &str {
    run.split_whitespace().next().unwrap_or(run)
}

#[cfg(windows)]
fn shell(line: &str) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    // No console window: this runs on the preview thread, many times a second
    // while a key is held, and each one would flash.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("cmd").arg("/S").arg("/C").raw_arg(format!("\"{line}\"")).creation_flags(CREATE_NO_WINDOW).output()
}

#[cfg(not(windows))]
fn shell(line: &str) -> std::io::Result<std::process::Output> {
    Command::new("sh").arg("-c").arg(line).output()
}

/// Whatever the command left behind, preferring the newest when it left more
/// than one.
fn newest_file(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() || md.len() == 0 {
            continue;
        }
        let at = md.modified().unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(b, _)| at >= *b) {
            best = Some((at, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// A directory that removes itself.
///
/// Written out rather than pulled in: it is twenty lines against a new
/// dependency, and the only thing it has to get right is a name nothing else
/// will pick, which the process id and a counter give.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self, String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("filer-preview-{}-{n}", std::process::id()));
        // Fresh every time: a leftover picture from the last run would be
        // found by `newest_file` and shown as this one.
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(Self(p))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_error_stays_whole() {
        assert_eq!(one_line("'x' is not recognized as an internal or external command,\r\noperable program or batch file.\r\n"), "'x' is not recognized as an internal or external command, operable program or batch file.");
        assert_eq!(one_line(" \n"), "");
    }

    #[test]
    fn stderr_that_is_utf8_is_kept_as_it_is() {
        assert_eq!(decode_stderr("Wrong page range — 日本語".as_bytes()), "Wrong page range — 日本語");
        // Bytes that are no encoding at all never panic.
        assert!(!decode_stderr(&[0xff, 0xfe, b'x']).is_empty());
    }

    // Only the two tests that drive a real shell need this, and those are
    // written against . The Windows path is covered by section 30 of
    // TESTING.md instead, since  needs a different line entirely.
    #[cfg(not(windows))]
    fn rule(run: &str) -> PreviewRule {
        PreviewRule {
            pattern: "*".into(),
            run: run.into(),
            first: 1,
            step: 1,
            unit: "page".into(),
        }
    }

    /// The placeholders carry a path with a space in it.
    ///
    /// Quoting is filer's job, not the rule's: a config that had to quote
    /// `{path}` itself would be wrong on the first file with a space in its
    /// name, and on Windows that is most of them.
    #[test]
    fn a_path_with_a_space_survives_substitution() {
        let line = fill(
            "tool -f {n} -l {n} {path} {out}",
            Path::new("/my docs/a b.pdf"),
            Path::new("/tmp/x/page"),
            7,
        );
        assert_eq!(line, r#"tool -f 7 -l 7 "/my docs/a b.pdf" "/tmp/x/page""#);
    }

    /// A suffix on `{out}` goes **inside** the quotes.
    ///
    /// `ffmpeg` is told `{out}.png`, and quoting the placeholder alone gives
    /// `"…\page".png`. `sh` glues that back together, so it worked here and
    /// nowhere else: on Windows `cmd` hands the quotes to ffmpeg as part of
    /// the filename, which cannot contain them, and no frame was ever drawn.
    #[test]
    fn a_suffix_on_out_is_inside_the_quotes() {
        let line = fill("ff -i {path} -y {out}.png", Path::new("/v/a b.mp4"), Path::new("/t/page"), 0);
        assert_eq!(line, r#"ff -i "/v/a b.mp4" -y "/t/page.png""#);

        // And a rule that quoted it itself is left as it is.
        let line = fill(r#"ff -y "{out}.png""#, Path::new("/v/x.mp4"), Path::new("/t/page"), 0);
        assert_eq!(line, r#"ff -y "/t/page.png""#);
    }

    /// The whole way through, with a command that really runs.
    ///
    /// `{n}` has to reach the command, the picture it writes has to be found
    /// whatever it called it, and the directory has to be gone afterwards.
    #[cfg(not(windows))]
    #[test]
    fn it_runs_the_command_and_finds_what_it_wrote() {
        let src = crate::util::test_dir("ext").join("src.txt");
        std::fs::write(&src, "x").unwrap();

        // Writes a file whose *name* nobody could have predicted, which is the
        // point: the directory is searched, not guessed at.
        let r = rule("printf '%s' {n} > {out}-page-{n}.png");
        let drawn = draw(&r, &src, 3).expect("the command ran");
        assert_eq!(std::fs::read_to_string(&drawn.png).unwrap(), "3", "{{n}} reached it");

        let dir = drawn._dir.0.clone();
        assert!(dir.is_dir());
        drop(drawn);
        assert!(!dir.exists(), "the directory goes with it");

        let _ = std::fs::remove_file(&src);
    }

    /// A command that writes nothing is a failure, and says what it said.
    ///
    /// This is how the end of a document arrives: nothing asked how many pages
    /// there were, so page 99 of a two-page PDF is `pdftoppm` complaining.
    #[cfg(not(windows))]
    #[test]
    fn nothing_written_is_reported_in_the_commands_own_words() {
        let src = crate::util::test_dir("ext-none").join("src.txt");
        std::fs::write(&src, "x").unwrap();

        let r = rule("echo 'Wrong page range given' >&2");
        let e = draw(&r, &src, 99).unwrap_err();
        assert_eq!(e, "Wrong page range given");

        // And with nothing said either, it still names the tool.
        let r = rule("true");
        assert!(draw(&r, &src, 1).unwrap_err().contains("true"));

        let _ = std::fs::remove_file(&src);
    }
}
