//! `filer.com`: the console-subsystem front for `filer.exe` (Q44).
//!
//! `filer.exe` is a windowed program, so PowerShell does not wait for it at
//! the end of a pipeline: `$v = & filer env` came back empty, as did
//! `& filer --version`, and the text arrived after the prompt (#160, #176,
//! #183). The release zip ships this binary beside `filer.exe` as `filer.com`.
//! `PATHEXT` lists `.COM` before `.EXE`, so typing `filer` finds this one
//! first. It is the shape of Visual Studio's `devenv.com` and `devenv.exe`.
//!
//! For the commands that answer in text, it runs `filer.exe` with the same
//! arguments and the same standard handles, waits, and passes on the exit
//! code. Being a console program, it is what the shell waits for. Anything
//! else opens the window: it starts `filer.exe` and returns as soon as the
//! window is taking input, much as typing `filer.exe` always did. It waits
//! that long, not less, so that a command line `filer.exe` refuses before
//! opening a window (`--keys "<Tab"`, two paths) still prints its one line
//! here and passes on its exit code.
//!
//! It holds no logic of its own beyond that choice: the report, the help and
//! the hooks all come from `filer.exe`, so the two cannot disagree.

/// The arguments that make `filer.exe` print and exit rather than open a
/// window. Kept in step with `parse_cli` in `main.rs`.
const ANSWERS: &[&str] = &["env", "--env", "--version", "-V", "--help", "-h", "shell-hook"];

/// The options that take the next argument as their value, which is then
/// never a command: `--keys env` types three letters.
const TAKES_VALUE: &[&str] = &["--cwd-file", "--chooser-file", "--keys"];

/// Whether `filer.exe` will answer this command line in text and exit, read
/// the way `parse_cli` reads it: left to right, skipping option values.
fn answers_in_text(args: &[std::ffi::OsString]) -> bool {
    let mut args = args.iter().map(|a| a.to_str().unwrap_or(""));
    while let Some(a) = args.next() {
        if ANSWERS.contains(&a) {
            return true;
        }
        if TAKES_VALUE.contains(&a) {
            args.next();
        }
    }
    false
}

#[cfg(windows)]
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let exe = match std::env::current_exe() {
        Ok(me) => me.with_file_name("filer.exe"),
        Err(e) => {
            eprintln!("filer.com: cannot tell where it is: {e}");
            std::process::exit(2);
        }
    };
    if !exe.is_file() {
        eprintln!("filer.com: no filer.exe beside it ({})", exe.display());
        std::process::exit(2);
    }
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&args);
    if answers_in_text(&args) {
        match cmd.status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("filer.com: {}: {e}", exe.display());
                std::process::exit(2);
            }
        }
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("filer.com: {}: {e}", exe.display());
            std::process::exit(2);
        }
    };
    std::process::exit(until_the_window_is_up(&mut child));
}

/// Waits for the window to start taking input, then lets it be: the prompt
/// comes back with filer still open. If `filer.exe` exits first, it refused
/// the command line, and its exit code is this one's. A refusal comes before
/// any window, within milliseconds of starting; the cap is for a machine so
/// slow that neither has happened yet.
#[cfg(windows)]
fn until_the_window_is_up(child: &mut std::process::Child) -> i32 {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::{WaitForInputIdle, WaitForSingleObject};

    let process = HANDLE(child.as_raw_handle());
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code().unwrap_or(1),
            Ok(None) => {}
            Err(_) => return 0,
        }
        match unsafe { WaitForInputIdle(process, 50) } {
            // Idle: its message loop is running and waiting for input -- or
            // it has exited, which WaitForInputIdle also answers with 0. CI
            // caught that: `--keys "<Tab"` printed its refusal, filer.exe
            // exited 2, and this returned 0. So look again, giving an exit
            // that is already under way a moment to land.
            0 => {
                if unsafe { WaitForSingleObject(process, 250) } == WAIT_OBJECT_0 {
                    return match child.try_wait() {
                        Ok(Some(status)) => status.code().unwrap_or(1),
                        _ => 1,
                    };
                }
                return 0;
            }
            // WAIT_TIMEOUT: not yet.
            0x102 => {}
            // WAIT_FAILED: no message queue to wait on, which is a debug
            // build (a console program). Give a refusal its moment, then go.
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(300));
                return match child.try_wait() {
                    Ok(Some(status)) => status.code().unwrap_or(1),
                    _ => 0,
                };
            }
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return 0;
        }
    }
}

#[cfg(not(windows))]
fn main() {
    let _ = answers_in_text;
    eprintln!("filer-com is the console front for filer.exe, and only does anything on Windows");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(args: &[&str]) -> Vec<std::ffi::OsString> {
        args.iter().map(Into::into).collect()
    }

    /// The text commands are waited for; anything that opens the window is not.
    #[test]
    fn it_waits_only_for_what_answers_in_text() {
        for waits in [&["env"][..], &["env", "--out", "r.txt"], &["--version"], &["-V"], &["--help"], &["shell-hook", "bash"], &["C:\\dev", "env"], &["--keys", "j", "-V"]] {
            assert!(answers_in_text(&line(waits)), "{waits:?}");
        }
        for opens in [&[][..], &["C:\\dev"], &["--keys", "<Tab>"], &["--cwd-file", "x"], &["environment"], &["--keys", "env"], &["--cwd-file", "--help"]] {
            assert!(!answers_in_text(&line(opens)), "{opens:?}");
        }
    }
}
