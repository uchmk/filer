//! An embedded terminal.
//!
//! `alacritty_terminal` does the parts that are a terminal emulator and not a
//! file manager: it owns the PTY — a real one on Unix, ConPTY on Windows —
//! reads it on a thread of its own, and parses the escape sequences into a
//! grid. It brings its own PTY, so there is no second implementation here.
//!
//! What is left is the three things that belong to this app: starting the
//! shell in the directory the pane is showing, turning key presses into the
//! bytes a shell expects, and drawing the grid with the list's own font and
//! theme. Only the middle one is logic rather than plumbing, so that is the
//! part with tests: [`encode`] is a pure function over a key and its
//! modifiers.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event as PtyEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, EventedReadWrite};

use crossbeam_channel::{Receiver, Sender};

/// The grid's shape, which is all `Term` needs to know about the window.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub cols: usize,
    pub lines: usize,
}

impl Size {
    /// Columns and lines never usefully reach zero, and `Grid` divides by
    /// them, so the floor is one of each.
    pub fn new(cols: usize, lines: usize) -> Self {
        Self { cols: cols.max(1), lines: lines.max(1) }
    }
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Carries what the terminal wants to say back to the UI thread. The parser
/// runs on the PTY reader thread, so everything arrives over a channel and is
/// read where the rest of the app's channels are read.
#[derive(Clone)]
pub struct Proxy {
    tx: Sender<PtyEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl EventListener for Proxy {
    fn send_event(&self, event: PtyEvent) {
        let _ = self.tx.send(event);
        (self.wake)();
    }
}

/// The PTY with a tap on the bytes coming out of it.
///
/// A shell says where it is with OSC 7, and `alacritty_terminal`'s parser does
/// not carry that one — vte handles the title, the clipboard and the colors,
/// and lets the rest fall on the floor. Rather than replace the event loop to
/// get at it, the PTY is wrapped: the loop reads through here, so the bytes
/// are seen on the way past and the terminal still gets every one of them.
///
/// `Reader = Self` because [`tty::EventedReadWrite::reader`] hands back a
/// reference into `self`, which leaves nowhere to put a wrapper that borrows
/// it. Being its own reader is how the tap gets to keep its state.
struct Tapped {
    inner: tty::Pty,
    cwd: Sender<PathBuf>,
    /// Bytes of an OSC 7 that has begun but not ended, since a read can stop
    /// anywhere — including in the middle of one.
    partial: Vec<u8>,
    /// `FILER_PTY_LOG`, when it is set; see [`PtyLog`].
    log: PtyLog,
    /// Whether the other end has asked for win32-input-mode; see
    /// [`Terminal::win32_input`]. Written here, where the request goes past.
    win32: Arc<AtomicBool>,
    /// The end of the last read, in case the request was cut in two.
    mode_tail: Vec<u8>,
}

/// Longest OSC 7 worth waiting for. A path cannot sensibly be longer, and a
/// stream that never terminates one must not grow this for ever.
const MAX_OSC: usize = 4096;

impl io::Read for Tapped {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.reader().read(buf)?;
        log_pty(&self.log, "out", &buf[..n]);
        for path in scan_osc7(&mut self.partial, &buf[..n]) {
            let _ = self.cwd.send(path);
        }
        if let Some(on) = scan_win32_mode(&mut self.mode_tail, &buf[..n]) {
            self.win32.store(on, Ordering::Relaxed);
        }
        Ok(n)
    }
}

impl tty::EventedReadWrite for Tapped {
    type Reader = Self;
    type Writer = <tty::Pty as tty::EventedReadWrite>::Writer;

    unsafe fn register(
        &mut self,
        poller: &Arc<polling::Poller>,
        interest: polling::Event,
        mode: polling::PollMode,
    ) -> io::Result<()> {
        unsafe { self.inner.register(poller, interest, mode) }
    }

    fn reregister(
        &mut self,
        poller: &Arc<polling::Poller>,
        interest: polling::Event,
        mode: polling::PollMode,
    ) -> io::Result<()> {
        self.inner.reregister(poller, interest, mode)
    }

    fn deregister(&mut self, poller: &Arc<polling::Poller>) -> io::Result<()> {
        self.inner.deregister(poller)
    }

    fn reader(&mut self) -> &mut Self::Reader {
        self
    }

    fn writer(&mut self) -> &mut Self::Writer {
        self.inner.writer()
    }
}

impl tty::EventedPty for Tapped {
    fn next_child_event(&mut self) -> Option<tty::ChildEvent> {
        self.inner.next_child_event()
    }
}

impl alacritty_terminal::event::OnResize for Tapped {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.inner.on_resize(window_size)
    }
}

/// The shell the pane starts when `[term] shell` names none.
///
/// On Windows that is `pwsh` when PowerShell 7 is installed, and Windows
/// PowerShell 5.1 when it is not (Q29), which is how Windows Terminal picks
/// too. 5.1 lacks `LocationChangedAction`, the hook the README gives for
/// bringing the directory back, and ships a PSReadLine without prediction, so
/// starting it on a machine that has 7 was starting the worse of two shells.
/// `None` is the platform's own default: `powershell` on Windows, the login
/// shell elsewhere.
pub fn default_shell() -> Option<String> {
    pick_default_shell(cfg!(windows), crate::util::locate("pwsh").is_some())
}

fn pick_default_shell(windows: bool, have_pwsh: bool) -> Option<String> {
    (windows && have_pwsh).then(|| "pwsh".to_owned())
}

/// A key as a single win32-input-mode record, **pressed only**:
/// `CSI Vk ; Sc ; Uc ; 1 ; Cs ; 1 _`.
///
/// This is how `Esc` reaches a tcell program (lazygit, gh-dash) intact, and
/// the "pressed only" is the whole fix. A program in virtual-terminal input
/// mode gets the press as the character `0x1b` and waits 50ms to be sure it is
/// a lone Escape. If a release record follows -- which it did, both when
/// ConPTY made one up from a plain `ESC` and when this sent one -- tcell turns
/// it into `ESC [ 27 ; 1 ; 27 ; 0 ; 0 ; 1 _` in the same stream. A second ESC
/// inside the wait makes the first an Alt prefix, the release is then ignored,
/// and the key is gone. Windows Terminal delivers no release to such a
/// program, so the wait runs out and Escape arrives; this matches it.
///
/// ConPTY takes the record without having asked for win32-input-mode, which
/// `scripts/keyprobe.ps1` showed on the real machine: what this sent came out
/// as a key record carrying the virtual key.
pub fn win32_key(vk: u16, scan: u16, ch: u16, mods: Mods) -> Vec<u8> {
    record(vk, scan, ch, mods, false)
}

fn record(vk: u16, scan: u16, ch: u16, mods: Mods, enhanced: bool) -> Vec<u8> {
    // dwControlKeyState: the left-hand bits, since a synthesised key has no
    // side and the left is what every keyboard has. ENHANCED_KEY (0x100) is
    // what the arrows and the block above them carry on a real keyboard.
    let state = (if mods.alt { 0x02 } else { 0 })
        | (if mods.ctrl { 0x08 } else { 0 })
        | (if mods.shift { 0x10 } else { 0 })
        | (if enhanced { 0x100 } else { 0 });
    format!("\x1b[{vk};{scan};{ch};1;{state};1_").into_bytes()
}

/// A special key as a win32-input-mode record, for when the other end has
/// asked for them (Q27).
///
/// Every sequence a special key sends as VT starts with ESC, and a tcell
/// program cannot tell a lone `<Esc>` from the start of the next one if it
/// arrives inside its 50ms wait: `<Esc>` followed within 31ms by a forwarded
/// `<S-End>` (`\e[1;2F`) lost the Escape for good (#99). A record is one key
/// whatever follows it, which is how Windows Terminal sends them all.
pub fn special_record(key: Special, mods: Mods) -> Vec<u8> {
    use Special::*;
    let (vk, scan, ch, enhanced, mods) = match key {
        Enter => (0x0d, 0x1c, 0x0d, false, mods),
        Backspace => (0x08, 0x0e, if mods.ctrl { 0x7f } else { 0x08 }, false, mods),
        Tab => (0x09, 0x0f, 0x09, false, mods),
        BackTab => (0x09, 0x0f, 0x09, false, Mods { shift: true, ..mods }),
        Escape => (0x1b, 0x01, 0x1b, false, mods),
        Up => (0x26, 0x48, 0, true, mods),
        Down => (0x28, 0x50, 0, true, mods),
        Left => (0x25, 0x4b, 0, true, mods),
        Right => (0x27, 0x4d, 0, true, mods),
        Home => (0x24, 0x47, 0, true, mods),
        End => (0x23, 0x4f, 0, true, mods),
        PageUp => (0x21, 0x49, 0, true, mods),
        PageDown => (0x22, 0x51, 0, true, mods),
        Insert => (0x2d, 0x52, 0, true, mods),
        Delete => (0x2e, 0x53, 0, true, mods),
        F(n @ 1..=10) => (0x70 + u16::from(n) - 1, 0x3b + u16::from(n) - 1, 0, false, mods),
        F(11) => (0x7a, 0x57, 0, false, mods),
        F(12) => (0x7b, 0x58, 0, false, mods),
        F(n) => (0x70 + u16::from(n.min(24)) - 1, 0, 0, false, mods),
    };
    record(vk, scan, ch, mods, enhanced)
}

/// A letter or digit held with Ctrl or Alt as a win32-input-mode record.
/// `None` for anything else, which then goes as the bytes it always did.
///
/// `ch` is what the key would type: the control code under Ctrl (`Ctrl+C` is
/// 3), the character itself under Alt alone. The scan codes are a US
/// keyboard's, which is what ConPTY expects of a synthesised key.
pub fn char_record(c: char, mods: Mods) -> Option<Vec<u8>> {
    const LETTERS: [u16; 26] = [
        0x1e, 0x30, 0x2e, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18,
        0x19, 0x10, 0x13, 0x1f, 0x14, 0x16, 0x2f, 0x11, 0x2d, 0x15, 0x2c,
    ];
    if !(mods.ctrl || mods.alt) {
        return None;
    }
    let (vk, scan) = match c.to_ascii_uppercase() {
        u @ 'A'..='Z' => (u as u16, LETTERS[(u as u8 - b'A') as usize]),
        '0' => (0x30, 0x0b),
        d @ '1'..='9' => (d as u16, 0x02 + (d as u16 - '1' as u16)),
        _ => return None,
    };
    let typed = match (mods.ctrl, c.is_ascii_alphabetic()) {
        (true, true) => (c.to_ascii_lowercase() as u16) - u16::from(b'a') + 1,
        (true, false) => 0,
        (false, _) if mods.shift => c.to_ascii_uppercase() as u16,
        (false, _) => c as u16,
    };
    Some(record(vk, scan, typed, mods, false))
}

/// Whether this read turned win32-input-mode on or off, going by the last
/// `\e[?9001h` or `\e[?9001l` in it. ConPTY asks for it as it starts, when
/// the terminal on the other side can send key records rather than VT.
///
/// `tail` keeps the end of the previous read, since a request can be cut in
/// two by where one read stopped.
fn scan_win32_mode(tail: &mut Vec<u8>, chunk: &[u8]) -> Option<bool> {
    const SET: &[u8] = b"\x1b[?9001h";
    const RESET: &[u8] = b"\x1b[?9001l";
    tail.extend_from_slice(chunk);
    let mut last = None;
    let mut i = 0;
    while i + SET.len() <= tail.len() {
        if tail[i..].starts_with(SET) {
            last = Some(true);
            i += SET.len();
        } else if tail[i..].starts_with(RESET) {
            last = Some(false);
            i += RESET.len();
        } else {
            i += 1;
        }
    }
    let keep = tail.len().saturating_sub(SET.len() - 1);
    tail.drain(..keep);
    last
}

/// The terminal's answer to a program asking whether win32-input-mode is on
/// (`\e[?9001$p`). alacritty does not know the mode and says so (`0`, not
/// recognised), which told lazygit the mode did not exist while `<Esc>` was
/// already arriving in it. Said as it is instead: set (1) or reset (2).
fn answer_win32_query(reply: String, on: bool) -> String {
    match reply.as_str() {
        "\x1b[?9001;0$y" => format!("\x1b[?9001;{}$y", if on { 1 } else { 2 }),
        _ => reply,
    }
}

/// A record of every byte that crosses the PTY, for when the pane and a
/// program in it disagree about what was said.
///
/// Set `FILER_PTY_LOG` to a file path before starting filer and each chunk is
/// appended as one line: milliseconds since the pane opened, the direction,
/// and the bytes with control characters spelled out. `out` is what the shell
/// side (on Windows, ConPTY) wrote to filer; `in` is what filer wrote to it,
/// split by origin into `in key`, `in paste` and `in reply` -- the last being
/// the terminal's own answers to a program's queries, which are the bytes
/// under suspicion when lazygit opens a menu at startup that nobody asked for.
///
/// The first line gives the moment the pane opened in Unix milliseconds, so a
/// line's time plus that is the wall clock `scripts/keyprobe.ps1` prints.
///
/// Off unless the variable is set, and nothing is read or written for it then.
/// A chunk is whatever one read or write happened to carry, so a sequence can
/// be split across two lines, and a multi-byte character cut at a chunk's edge
/// shows as U+FFFD.
type PtyLog = Option<Arc<Mutex<PtyLogFile>>>;

struct PtyLogFile {
    file: std::fs::File,
    start: Instant,
}

fn open_pty_log() -> PtyLog {
    let path = std::env::var_os("FILER_PTY_LOG")?;
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
    let log = PtyLogFile { file, start: Instant::now() };
    let log = Arc::new(Mutex::new(log));
    if let Ok(mut l) = log.lock() {
        // The wall clock once, so the lines' milliseconds can be matched
        // against another program's (`scripts/keyprobe.ps1` prints this clock).
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let _ = writeln!(l.file, "== pane opened at {unix} (Unix ms; the times below count from here)");
    }
    Some(log)
}

fn log_pty(log: &PtyLog, dir: &str, bytes: &[u8]) {
    let Some(log) = log else { return };
    let Ok(mut l) = log.lock() else { return };
    let ms = l.start.elapsed().as_millis();
    let _ = writeln!(l.file, "{ms:>8} {dir:<9} {}", escape_bytes(bytes));
}

/// Bytes as one readable line: `\e` for ESC, `\r` `\n` `\t`, `\xNN` for any
/// other control character, and `\\` for a backslash so that none of those
/// spellings can be mistaken for the text they stand in for.
pub fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for c in String::from_utf8_lossy(bytes).chars() {
        match c {
            '\x1b' => out.push_str("\\e"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\x7f' => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Pull the directories out of any OSC 7 sequences in `chunk`.
///
/// The shape is `ESC ] 7 ; file://host/path` closed by BEL or ST (`ESC \`).
///
/// A read stops wherever the pipe happened to fill, which can be anywhere —
/// including between the `ESC` and the `]`. So `carry` holds whatever of the
/// previous read could still matter, and each call works over the join: an
/// unfinished sequence, or the first bytes of a start marker. Everything
/// older is dropped, so the buffer does not grow with the output.
fn scan_osc7(carry: &mut Vec<u8>, chunk: &[u8]) -> Vec<PathBuf> {
    const START: &[u8] = b"\x1b]7;";
    let mut out = Vec::new();
    carry.extend_from_slice(chunk);

    let mut from = 0usize;
    // Where the kept tail begins once the scan runs out of complete sequences.
    let keep;
    loop {
        let Some(rel) = find(&carry[from..], START) else {
            // Nothing begun: only a split start marker could still matter.
            keep = from.max(carry.len().saturating_sub(START.len() - 1));
            break;
        };
        let at = from + rel;
        let body = at + START.len();
        match end_of_osc(&carry[body..]) {
            Some((end, skip)) => {
                if let Some(p) = from_file_url(&carry[body..body + end]) {
                    out.push(p);
                }
                from = body + end + skip;
            }
            None => {
                // Still waiting for the end. A sequence this long is not a
                // path, so give up rather than hold it for ever.
                keep = match carry.len() - at > MAX_OSC {
                    true => carry.len(),
                    false => at,
                };
                break;
            }
        }
    }
    carry.drain(..keep);
    out
}

/// Where an OSC body ends, and how many bytes the terminator takes.
fn end_of_osc(bytes: &[u8]) -> Option<(usize, usize)> {
    let bel = bytes.iter().position(|&b| b == 0x07);
    let st = find(bytes, b"\x1b\\");
    match (bel, st) {
        (Some(a), Some(b)) if a < b => Some((a, 1)),
        (Some(_), Some(b)) => Some((b, 2)),
        (Some(a), None) => Some((a, 1)),
        (None, Some(b)) => Some((b, 2)),
        (None, None) => None,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The path out of a `file://host/path` URL, with percent-escapes undone.
///
/// The host is whatever machine the shell is on; a remote one names a path
/// this side cannot open, but there is no way to tell from here, so it is
/// taken at face value and simply fails to list if it is not there.
fn from_file_url(bytes: &[u8]) -> Option<PathBuf> {
    let s = std::str::from_utf8(bytes).ok()?.trim();
    let rest = s.strip_prefix("file://")?;
    // Past the host, which may be empty (`file:///home/…`).
    let path = &rest[rest.find('/')?..];
    let decoded = percent_decode(path);
    let decoded = decoded.trim_end_matches('/');
    if decoded.is_empty() {
        return Some(PathBuf::from("/"));
    }
    // A share: PowerShell writes `\\host\share` as `file://///host/share`, and
    // a UNC prefix needs exactly two slashes -- with three it read as
    // `\host\share`, a path on the current drive (#101, 29.4).
    if decoded.starts_with("//") {
        let unc = format!("//{}", decoded.trim_start_matches('/'));
        return Some(crate::util::normalize(Path::new(&unc)));
    }
    // Windows spells it `file:///C:/dir`, which is a path once the slash goes.
    let trimmed = match decoded.as_bytes() {
        [b'/', c, b':', ..] if c.is_ascii_alphabetic() => &decoded[1..],
        _ => decoded,
    };
    Some(crate::util::normalize(Path::new(trimmed)))
}

/// The bytes to send for a paste of `text`.
///
/// `\x1b[200~` and `\x1b[201~` are the markers xterm defined and everything
/// since has followed. They go on only when the program asked for them: a shell
/// that did not ask would show them as `[200~` and then run the paste anyway,
/// which is worse than not bracketing at all.
fn bracket(text: &str, wanted: bool) -> Vec<u8> {
    // Nothing to paste needs no markers; a bare pair would reach a shell that
    // does not strip them as `[200~[201~` on the command line.
    if !wanted || text.is_empty() {
        return text.as_bytes().to_vec();
    }
    let mut out = Vec::with_capacity(text.len() + 12);
    out.extend_from_slice(b"\x1b[200~");
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\x1b[201~");
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct Terminal {
    term: Arc<FairMutex<Term<Proxy>>>,
    sender: EventLoopSender,
    rx: Receiver<PtyEvent>,
    /// What the shell has set the title to, when it has.
    pub title: String,
    /// The shell is gone; the pane says so rather than pretending.
    pub exited: bool,
    size: Size,
    /// Where the shell was last told to go, so it is not told twice.
    followed: Option<PathBuf>,
    /// How to quote a word for the shell that was actually started, decided
    /// once at `spawn` because that is where the program's name is known.
    quoting: Quoting,
    /// Whether the other end asked for keys as win32-input-mode records.
    win32: Arc<AtomicBool>,
    cwd_rx: Receiver<PathBuf>,
    /// The last match, start and end, so the next search carries on past it
    /// in whichever direction it goes.
    found: Option<(Point, Point)>,
    /// Where the shell says it is, when it says so at all. A shell that does
    /// not send OSC 7 leaves this `None` for ever, which is why it only ever
    /// suppresses work rather than driving any.
    pub shell_cwd: Option<PathBuf>,
    /// `FILER_PTY_LOG`, when it is set; see [`PtyLog`].
    log: PtyLog,
    /// The shell's process, to ask whether it has started anything. `None`
    /// where the PTY did not say, which reads as "nothing running".
    shell_pid: Option<u32>,
}

impl Terminal {
    /// Start a shell in `cwd`. The cell size is what the PTY is told, so a
    /// program asking for pixels (an image protocol, say) gets the truth.
    pub fn spawn(
        cwd: &Path,
        size: Size,
        cell: (u16, u16),
        shell: Option<(String, Vec<String>)>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let quoting = Quoting::for_shell(shell.as_ref().map(|(p, _)| p.as_str()));
        let options = tty::Options {
            // `None` is the platform default, which on Windows is
            // `powershell` -- Windows PowerShell 5.1. `default_shell` asks for
            // `pwsh` before it comes to that when 7 is installed. They read
            // different profiles, so a shell hook set up for one is simply not
            // there in the other; `[term] shell` is how you say which.
            shell: shell.map(|(program, args)| tty::Shell::new(program, args)),
            working_directory: Some(cwd.to_path_buf()),
            drain_on_exit: false,
            env: Default::default(),
            #[cfg(target_os = "windows")]
            escape_args: true,
        };
        let window = window_size(size, cell);
        let pty = tty::new(&options, window, 0)?;
        #[cfg(windows)]
        let shell_pid = pty.child_watcher().pid().map(|p| p.get());
        #[cfg(unix)]
        let shell_pid = Some(pty.child().id());
        let (cwd_tx, cwd_rx) = crossbeam_channel::unbounded();
        let log = open_pty_log();
        let win32 = Arc::new(AtomicBool::new(false));
        let pty = Tapped {
            inner: pty,
            cwd: cwd_tx,
            partial: Vec::new(),
            log: log.clone(),
            win32: win32.clone(),
            mode_tail: Vec::new(),
        };

        let (tx, rx) = crossbeam_channel::unbounded();
        let proxy = Proxy { tx, wake: Arc::new(wake) };
        let term = Term::new(Config::default(), &size, proxy.clone());
        let term = Arc::new(FairMutex::new(term));

        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
        let sender = event_loop.channel();
        // The reader thread owns the PTY from here; it parses into `term` and
        // ends when the shell does.
        let _ = event_loop.spawn();

        Ok(Self {
            term,
            sender,
            rx,
            title: String::new(),
            exited: false,
            size,
            followed: Some(cwd.to_path_buf()),
            quoting,
            cwd_rx,
            found: None,
            shell_cwd: None,
            log,
            shell_pid,
            win32,
        })
    }

    /// Whether keys should go as win32-input-mode records: the other end
    /// (ConPTY, on Windows) asked for them. Never true anywhere else.
    pub fn win32_input(&self) -> bool {
        self.win32.load(Ordering::Relaxed)
    }

    /// Whether the shell is running something -- lazygit, an editor, a build
    /// -- rather than sitting at its prompt. Ending the shell ends that too,
    /// so `<C-S-t>` asks first when this is true (Q21).
    ///
    /// "Something" is any child process of the shell. A shell that keeps a
    /// helper of its own alive would read as busy and be asked about when it
    /// need not be; that errs on the side of the question, which costs a key.
    pub fn busy(&self) -> bool {
        !self.exited && self.shell_pid.is_some_and(|pid| !children(pid).is_empty())
    }

    /// Take everything the shell has said since the last frame. Returns the
    /// text it asked to put on the clipboard, which only the UI thread can do.
    pub fn drain(&mut self) -> Vec<String> {
        let mut clipboard = Vec::new();
        // Where the shell says it is. Only the last one matters.
        while let Ok(p) = self.cwd_rx.try_recv() {
            self.shell_cwd = Some(p);
        }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                PtyEvent::Title(t) => self.title = t,
                PtyEvent::ResetTitle => self.title.clear(),
                PtyEvent::ClipboardStore(_, text) => clipboard.push(text),
                // A program answering a query writes back through the same
                // pipe it would if the user had typed it.
                PtyEvent::PtyWrite(text) => {
                    let text = answer_win32_query(text, self.win32_input());
                    self.send_as(text.into_bytes(), "in reply")
                }
                PtyEvent::Exit | PtyEvent::ChildExit(_) => self.exited = true,
                _ => {}
            }
        }
        clipboard
    }

    pub fn resize(&mut self, size: Size, cell: (u16, u16)) {
        if size.cols == self.size.cols && size.lines == self.size.lines {
            return;
        }
        self.size = size;
        // Both halves have to hear about it: the grid reflows, and the shell
        // is signalled so a full-screen program repaints itself.
        self.term.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(window_size(size, cell)));
    }

    pub fn send(&self, bytes: Vec<u8>) {
        self.send_as(bytes, "in key");
    }

    /// `send`, labelled for the PTY log by where the bytes came from.
    fn send_as(&self, bytes: Vec<u8>, origin: &str) {
        log_pty(&self.log, origin, &bytes);
        // Typing is an answer to what is on screen, so the view comes back to
        // the bottom — every terminal does this, and a key that seemed to do
        // nothing because the view was in the scrollback is a bad surprise.
        self.term.lock().scroll_display(Scroll::Bottom);
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    pub fn size(&self) -> Size {
        self.size
    }

    /// Move the view through the scrollback. Lines are positive for older.
    pub fn scroll(&self, by: Scroll) {
        self.term.lock().scroll_display(by);
    }

    /// Whether the view is somewhere above the bottom, which the pane says so
    /// the scrollback is never a silent place to be lost in.
    pub fn scrolled_back(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    /// Begin a selection at a cell, or carry one on to it. `start` is the
    /// press; everything after is the drag.
    pub fn select(&self, cell: (usize, usize), right_half: bool, start: bool) {
        select_at(&mut self.term.lock(), cell, right_half, start);
    }

    /// Select the word under a cell — what a double-click means everywhere.
    pub fn select_word(&self, cell: (usize, usize)) {
        let point = self.point(cell);
        let mut term = self.term.lock();
        term.selection = Some(Selection::new(SelectionType::Semantic, point, Side::Left));
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// The selected text, if any of it is.
    pub fn selection(&self) -> Option<String> {
        self.term.lock().selection_to_string().filter(|s| !s.is_empty())
    }

    /// Find `needle` from the top of the view, and put the match on screen.
    /// Returns whether anything matched.
    pub fn search(&mut self, needle: &str, back: bool) -> bool {
        let mut term = self.term.lock();
        search_in(&mut term, &mut self.found, needle, back)
    }

    /// Forget where a search got to, so the next one starts from the view.
    pub fn end_search(&mut self) {
        self.found = None;
    }

    /// A cell of the visible grid as a point in the whole buffer, which is
    /// where the scrollback lives above line zero.
    fn point(&self, cell: (usize, usize)) -> Point {
        point_at(&self.term.lock(), cell)
    }

    /// Type text in, as a paste rather than as keys.
    ///
    /// Wrapped in the bracketed-paste markers when the program on the other end
    /// asked for them, which bash, zsh, fish, PSReadLine and vim all do. The
    /// difference is not cosmetic: inside the brackets a line editor puts the
    /// text in the buffer and leaves it there, so a clipboard holding three
    /// lines ends up as three lines waiting to be read rather than two commands
    /// already run. Without the brackets a newline *is* Enter and there is no
    /// way to send one that is not.
    pub fn paste(&self, text: &str) {
        // Carriage returns are what a terminal calls Enter; a pasted `\n`
        // that stays a newline confuses a line editor.
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        self.send_as(bracket(&text, self.bracketed_paste()), "in paste");
    }

    /// Whether the program on the other end asked for bracketed paste.
    fn bracketed_paste(&self) -> bool {
        use alacritty_terminal::term::TermMode;
        self.term.lock().mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Follow the pane into `cwd`, by typing the `cd` a person would.
    ///
    /// Typing is the only way in: a shell takes no other instruction. That
    /// makes it a line of input like any other — harmless at a prompt, a
    /// nuisance in the middle of a command — so it is sent as rarely as it
    /// can be. Twice over: not when the pane has not moved, and not when the
    /// shell has already said (through OSC 7) that it is there. A shell that
    /// reports its directory therefore never hears a `cd` it does not need,
    /// including the one that would otherwise follow its own.
    pub fn follow(&mut self, cwd: &Path) {
        if self.followed.as_deref() == Some(cwd) {
            return;
        }
        self.followed = Some(cwd.to_path_buf());
        if self.shell_cwd.as_deref() == Some(cwd) {
            return;
        }
        let quoted = quote(&cwd.to_string_lossy(), self.quoting);
        self.send(format!("cd {quoted}\r").into_bytes());
    }

    /// How a word has to be quoted for the shell in this pane.
    pub fn quoting(&self) -> Quoting {
        self.quoting
    }

    /// Run the terminal's own locked grid through `f`. Locking is the caller's
    /// business to keep short: the reader thread wants it back.
    pub fn with_grid<R>(&self, f: impl FnOnce(&Term<Proxy>) -> R) -> R {
        f(&self.term.lock())
    }

    /// Whether the shell has put anything you could read on the screen yet --
    /// a banner or a prompt. ConPTY writes its own setup sequences the moment
    /// the pane opens, so "some bytes arrived" says nothing about whether the
    /// shell is ready to be typed at; a character on screen does.
    pub fn has_drawn(&self) -> bool {
        self.with_grid(|t| snapshot(t).iter().flatten().any(|c| !matches!(c.c, ' ' | '\0')))
    }

}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Ask the reader thread to stop; without this it sits on a PTY whose
        // pane is gone.
        let _ = self.sender.send(Msg::Shutdown);
    }
}

fn window_size(size: Size, cell: (u16, u16)) -> WindowSize {
    WindowSize {
        num_lines: size.lines as u16,
        num_cols: size.cols as u16,
        cell_width: cell.0.max(1),
        cell_height: cell.1.max(1),
    }
}

/// How the shell in the pane wants a word with something awkward in it.
///
/// They do not agree, and the one filer opens by default is the one that
/// agrees least: this used to emit the POSIX form for everything, on a
/// Windows-first program whose pane runs PowerShell unless told otherwise.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quoting {
    /// `powershell`, `pwsh`. A single quote inside is **doubled**.
    #[default]
    PowerShell,
    /// `bash`, `sh`, `zsh`, … A single quote inside is closed, escaped with a
    /// backslash, and reopened.
    Posix,
    /// `cmd.exe`, where single quotes mean nothing at all -- they would be
    /// handed to the program as part of the name. Double quotes are the
    /// grouping, and a `"` inside a path is not legal on Windows anyway.
    Cmd,
}

impl Quoting {
    /// Work out the convention from what `[term] shell` names.
    ///
    /// An unknown shell on Windows is far likelier to be PowerShell-shaped
    /// than POSIX-shaped, and elsewhere the reverse, so the platform decides
    /// what the fallback is rather than one convention being assumed for all.
    pub fn for_shell(program: Option<&str>) -> Self {
        let Some(program) = program else {
            // What `tty::Options { shell: None }` starts.
            return if cfg!(windows) { Self::PowerShell } else { Self::Posix };
        };
        // Split on both separators by hand rather than through `Path`, which
        // only knows the host's: `C:\\WINDOWS\\System32\\cmd.exe` is one long
        // component on Linux, and this string comes out of a config file that
        // may name either shape.
        let name = program.rsplit(['/', '\\']).next().unwrap_or(program).to_ascii_lowercase();
        let name = [".exe", ".cmd", ".bat"]
            .iter()
            .find_map(|x| name.strip_suffix(x))
            .unwrap_or(&name)
            .to_owned();
        match name.as_str() {
            "powershell" | "pwsh" => Self::PowerShell,
            "cmd" => Self::Cmd,
            "bash" | "sh" | "zsh" | "dash" | "ksh" | "fish" => Self::Posix,
            _ => {
                if cfg!(windows) {
                    Self::PowerShell
                } else {
                    Self::Posix
                }
            }
        }
    }
}

/// Wrap a path for a shell that is about to read it as one word.
///
/// Until v0.47.34 this wrote the POSIX form whatever the shell was, and said
/// in its own doc comment that PowerShell read it the same way. It does not:
/// PowerShell doubles a single quote inside single quotes, and reads the
/// POSIX `'\''` as a closed string followed by a stray backslash and an
/// unterminated one. Section 1 on the Windows machine found the pane sitting
/// at the `>>` continuation prompt after `<A-t>` on a file with a quote in its
/// name -- and getting out of that with `<C-c>` used to quit filer.
pub fn quote(s: &str, how: Quoting) -> String {
    // `'` is deliberately not in the safe set: cmd leaves it alone, but the
    // other two do not, and a name that needs no quotes in one shell still has
    // to come out right in the others.
    //
    // `\` is safe only where it is *nothing but* a path separator. A POSIX shell
    // reads it as an escape, so an unquoted `R:\Temp\x` arrives as `R:Tempx`,
    // and with `[term] shell` set to Git Bash **every ordinary Windows path
    // failed** -- `follow()` typed a `cd` that could not land, so walking the
    // list left the pane behind. Section 1 on the Windows machine found it, and
    // the same run showed the cure already works: 1.21's quoted
    // `cd 'R:\Temp\filer-fixtures\it'\''s here'` arrived as
    // `/r/Temp/filer-fixtures/it's here`, so Git Bash takes `\` intact inside
    // quotes. The test below used to pin the broken form for all three shells.
    let safe: &str = match how {
        Quoting::Posix => "_-./:",
        Quoting::PowerShell | Quoting::Cmd => "_-./:\\",
    };
    if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || safe.contains(c)) {
        return s.to_owned();
    }
    match how {
        Quoting::PowerShell => format!("'{}'", s.replace('\'', "''")),
        Quoting::Posix => format!("'{}'", s.replace('\'', r"'\''")),
        // Nothing to escape: a `"` cannot be in a Windows path, and `'` is an
        // ordinary character here.
        Quoting::Cmd => format!("\"{s}\""),
    }
}

/// The keys a terminal wants that are not text.
///
/// egui hands printable characters over as text events, which go through
/// untouched; these are the ones that have to become escape sequences.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    Enter,
    Backspace,
    Tab,
    BackTab,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
}

/// What is held down while a key is pressed. `ctrl` doubles for `Cmd` on
/// macOS the way the rest of this app treats it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    /// The number xterm uses for a modifier combination, or `None` for none of
    /// them — which is the difference between `\x1b[1;5A` and `\x1b[A`.
    fn code(self) -> Option<u8> {
        let n = 1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl);
        (n > 1).then_some(n)
    }
}

/// The bytes a special key sends.
///
/// `app_cursor` is the terminal's application-cursor mode, which swaps the
/// arrows between CSI and SS3 — vim and readline both rely on it, so it is
/// read from the terminal rather than assumed.
pub fn encode(key: Special, mods: Mods, app_cursor: bool) -> Vec<u8> {
    use Special::*;
    // A modifier turns the short forms into the parameterised ones, so the
    // two cases are kept apart rather than patched together.
    if let Some(m) = mods.code() {
        let out = match key {
            Up => Some(format!("\x1b[1;{m}A")),
            Down => Some(format!("\x1b[1;{m}B")),
            Right => Some(format!("\x1b[1;{m}C")),
            Left => Some(format!("\x1b[1;{m}D")),
            Home => Some(format!("\x1b[1;{m}H")),
            End => Some(format!("\x1b[1;{m}F")),
            Insert => Some(format!("\x1b[2;{m}~")),
            Delete => Some(format!("\x1b[3;{m}~")),
            PageUp => Some(format!("\x1b[5;{m}~")),
            PageDown => Some(format!("\x1b[6;{m}~")),
            F(n @ 1..=4) => Some(format!("\x1b[1;{m}{}", (b'P' + n - 1) as char)),
            F(n) => f_key_number(n).map(|num| format!("\x1b[{num};{m}~")),
            _ => None,
        };
        if let Some(s) = out {
            return s.into_bytes();
        }
    }

    let plain: &[u8] = match key {
        Enter => b"\r",
        // A terminal's Backspace is DEL, and Ctrl+Backspace is the BS that
        // shells read as "delete the word".
        Backspace if mods.ctrl => b"\x08",
        Backspace => b"\x7f",
        Tab => b"\t",
        BackTab => b"\x1b[Z",
        Escape => b"\x1b",
        Up if app_cursor => b"\x1bOA",
        Down if app_cursor => b"\x1bOB",
        Right if app_cursor => b"\x1bOC",
        Left if app_cursor => b"\x1bOD",
        Up => b"\x1b[A",
        Down => b"\x1b[B",
        Right => b"\x1b[C",
        Left => b"\x1b[D",
        Home => b"\x1b[H",
        End => b"\x1b[F",
        PageUp => b"\x1b[5~",
        PageDown => b"\x1b[6~",
        Insert => b"\x1b[2~",
        Delete => b"\x1b[3~",
        F(1) => b"\x1bOP",
        F(2) => b"\x1bOQ",
        F(3) => b"\x1bOR",
        F(4) => b"\x1bOS",
        F(n) => {
            return match f_key_number(n) {
                Some(num) => format!("\x1b[{num}~").into_bytes(),
                None => Vec::new(),
            };
        }
    };
    let mut out = Vec::with_capacity(plain.len() + 1);
    // Alt is the escape prefix, which is how a terminal has always spelled it.
    if mods.alt && !matches!(key, Escape) {
        out.push(0x1b);
    }
    out.extend_from_slice(plain);
    out
}

/// F5 and up are numbered, with gaps where the numbering skipped.
fn f_key_number(n: u8) -> Option<u8> {
    Some(match n {
        5 => 15,
        6..=10 => 17 + (n - 6),
        11 => 23,
        12 => 24,
        _ => return None,
    })
}

/// A character typed with Ctrl held, as the control code it stands for.
/// `Ctrl+C` is 0x03, `Ctrl+[` is Escape, and so on down the C0 range.
pub fn control_code(c: char, alt: bool) -> Option<Vec<u8>> {
    let byte = match c {
        ' ' | '@' => 0x00,
        'a'..='z' => c as u8 - b'a' + 1,
        'A'..='Z' => c as u8 - b'A' + 1,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' => 0x1e,
        '_' | '?' => 0x1f,
        _ => return None,
    };
    let mut out = Vec::with_capacity(2);
    if alt {
        out.push(0x1b);
    }
    out.push(byte);
    Some(out)
}

/// A cell of the visible grid as a point in the whole buffer, which is where
/// the scrollback lives above line zero.
pub fn point_at<T: EventListener>(term: &Term<T>, (col, line): (usize, usize)) -> Point {
    let offset = term.grid().display_offset() as i32;
    Point::new(Line(line as i32 - offset), Column(col))
}

/// Begin a drag selection at a cell, or carry one on to it.
///
/// `right_half` is which half of the cell the pointer is in, and it is not
/// cosmetic: alacritty settles a range by ordering the two anchors and then
/// dropping the first cell if that anchor says `Right` and the last if it says
/// `Left`. Fixing the sides -- `Left` to start, `Right` to extend -- is
/// therefore right only while the drag runs left to right. Drag the other way
/// and the ordering swaps them, so a cell goes at each end, which reads as the
/// first character of the line quietly refusing to be copied.
///
/// Taking the side from the pointer is what every terminal does, and it comes
/// out right both ways round: the outer half of whichever cell the drag began
/// on faces away from the selection, and so does the one it ended on.
pub fn select_at<T: EventListener>(
    term: &mut Term<T>,
    cell: (usize, usize),
    right_half: bool,
    start: bool,
) {
    let point = point_at(term, cell);
    let side = if right_half { Side::Right } else { Side::Left };
    match start {
        true => term.selection = Some(Selection::new(SelectionType::Simple, point, side)),
        false => {
            if let Some(sel) = term.selection.as_mut() {
                sel.update(point, side);
            }
        }
    }
}

/// Where a search with nothing to carry on from begins.
///
/// Backwards starts at the bottom right of what is on screen, so the visible
/// rows are searched before the history above them. Starting at the top left
/// instead -- which is what the view's own first line is -- steps away from
/// every row the reader can see *and* every row above it, and `<C-S-f>` said
/// there was no match for text that was plainly on the screen.
pub fn search_origin<T: EventListener>(term: &Term<T>, back: bool) -> Point {
    let grid = term.grid();
    let offset = grid.display_offset() as i32;
    match back {
        true => Point::new(
            Line(grid.screen_lines() as i32 - 1 - offset),
            Column(grid.columns().saturating_sub(1)),
        ),
        false => Point::new(Line(-offset), Column(0)),
    }
}

/// Where the cursor is, in cells, for the renderer to draw over.
/// The row is where the cursor is *on screen*, which is not where it is in the
/// buffer once the view has been scrolled back: the caller draws nothing when
/// the row falls past the last line, so the cursor leaves with the rows it
/// belongs to instead of hanging on the scrollback at its old height.
pub fn cursor_cell<T: EventListener>(term: &Term<T>) -> (usize, usize) {
    let grid = term.grid();
    let p = grid.cursor.point;
    (p.column.0, (p.line.0.max(0) as usize).saturating_add(grid.display_offset()))
}

/// Whether the arrows should be sent as SS3 rather than CSI.
pub fn app_cursor<T: EventListener>(term: &Term<T>) -> bool {
    use alacritty_terminal::term::TermMode;
    term.mode().contains(TermMode::APP_CURSOR)
}

/// Whether the program has asked to be told about the mouse, and in which
/// encoding. nvim asks at startup (`\e[?1002h\e[?1006h`), as do htop, tmux and
/// most other full-screen programs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseReport {
    Off,
    /// `\e[<b;x;yM` (mode 1006): any column, and what everything current asks for.
    Sgr,
    /// `\e[M` and three bytes, the original X10 form. Columns past 223 cannot
    /// be said in it, so they are said as 223.
    Plain,
}

pub fn mouse_report<T: EventListener>(term: &Term<T>) -> MouseReport {
    use alacritty_terminal::term::TermMode;
    let mode = term.mode();
    match (mode.intersects(TermMode::MOUSE_MODE), mode.contains(TermMode::SGR_MOUSE)) {
        (false, _) => MouseReport::Off,
        (true, true) => MouseReport::Sgr,
        (true, false) => MouseReport::Plain,
    }
}

/// One notch of the wheel as the program asked to hear it: buttons 64 (up)
/// and 65 (down) at the cell under the pointer, counted from zero here and
/// from one on the wire. A wheel has no release, so there is no `m` form.
pub fn wheel_report(up: bool, col: usize, line: usize, how: MouseReport) -> Vec<u8> {
    let button = if up { 64 } else { 65 };
    match how {
        MouseReport::Off => Vec::new(),
        MouseReport::Sgr => format!("\x1b[<{button};{};{}M", col + 1, line + 1).into_bytes(),
        MouseReport::Plain => {
            let at = |n: usize| (32 + 1 + n.min(222)) as u8;
            vec![0x1b, b'[', b'M', 32 + button, at(col), at(line)]
        }
    }
}

/// Whether a full-screen program is drawing: `nvim`, `less`, `htop` and the
/// rest switch to the alternate screen on the way in and back out on the way
/// out.
///
/// The alternate grid is built with a scrollback of zero lines — see
/// `Grid::new(num_lines, num_cols, 0)` in alacritty — so while this is true
/// there is nothing above the viewport to scroll to. That makes it the right
/// question to ask before the pane keeps a scrolling key or a wheel turn for
/// itself: the answer decides between moving a scrollback that exists and
/// handing the gesture to the program that owns the screen.
pub fn alt_screen<T: EventListener>(term: &Term<T>) -> bool {
    use alacritty_terminal::term::TermMode;
    term.mode().contains(TermMode::ALT_SCREEN)
}

/// `Alt` held with an ordinary character, in the meta-prefix form every shell
/// and readline expects: `ESC` then the character.
///
/// The same convention [`control_code`] already uses for `Alt` with a control
/// chord. Without this, `Alt`+letter reached the pane as a key with no bytes
/// behind it and was dropped on the floor, so `Alt-b` and `Alt-f` never moved
/// readline's cursor.
pub fn meta_char(c: char) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + c.len_utf8());
    out.push(0x1b);
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    out
}

/// The visible grid as rows of cells, for a renderer that cannot hold the
/// lock while it paints.
pub fn snapshot<T: EventListener>(term: &Term<T>) -> Vec<Vec<CellView>> {
    let grid = term.grid();
    let lines = grid.screen_lines();
    let cols = grid.columns();
    // Indexing by `Line` is relative to the active area, where line zero is the
    // top of the screen and the scrollback is above it at negative lines --
    // `display_offset` is not applied for you. Copying `0..lines` therefore
    // returned the same rows however far back the view had been scrolled: the
    // keys moved the offset, the badge read it back and said "16 lines back",
    // and the screen did not move. The same subtraction is in `point`, which
    // has always had to do this to turn a click into a buffer position.
    let offset = grid.display_offset() as i32;
    let sel = term.selection.as_ref().and_then(|s| s.to_range(term));
    let mut out = Vec::with_capacity(lines);
    for l in 0..lines {
        let line = Line(l as i32 - offset);
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let cell = &grid[line][Column(c)];
            let at = Point::new(line, Column(c));
            row.push(CellView {
                c: cell.c,
                fg: cell.fg,
                bg: cell.bg,
                flags: cell.flags,
                selected: sel.is_some_and(|r| r.contains(at)),
            });
        }
        out.push(row);
    }
    out
}

/// One cell, copied out from under the lock.
#[derive(Clone, Copy, Debug)]
pub struct CellView {
    pub c: char,
    pub fg: alacritty_terminal::vte::ansi::Color,
    pub bg: alacritty_terminal::vte::ansi::Color,
    pub flags: alacritty_terminal::term::cell::Flags,
    /// Inside the drag, or inside what a search just found. Both set the same
    /// selection, so both are drawn the same way, and until v0.20.4 neither
    /// was drawn at all -- a drag copied text with no sign of what it took,
    /// and a search that worked was indistinguishable from one that did not.
    pub selected: bool,
}

/// The processes whose parent is `pid`. Read when a key asks, not every frame:
/// one snapshot of the process table on Windows, `/proc` on Linux, `pgrep` on
/// macOS. Empty when the platform will not say.
pub fn children(pid: u32) -> Vec<u32> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };
        let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = unsafe { Process32FirstW(snap, &mut e) }.is_ok();
        while ok {
            if e.th32ParentProcessID == pid && e.th32ProcessID != pid {
                out.push(e.th32ProcessID);
            }
            ok = unsafe { Process32NextW(snap, &mut e) }.is_ok();
        }
        let _ = unsafe { CloseHandle(snap) };
        out
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
        dir.flatten()
            .filter_map(|d| d.file_name().to_str()?.parse::<u32>().ok())
            .filter(|&child| {
                // `pid (comm) state ppid ...`; the name may hold spaces and
                // parentheses, so the fields are counted from the last `)`.
                std::fs::read_to_string(format!("/proc/{child}/stat")).ok().is_some_and(|stat| {
                    stat.rsplit_once(')')
                        .and_then(|(_, rest)| rest.split_whitespace().nth(1)?.parse::<u32>().ok())
                        == Some(pid)
                })
            })
            .collect()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("pgrep")
            .args(["-P", &pid.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect())
            .unwrap_or_default()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Vec::new()
    }
}

/// One step of a scrollback search: the next match of `needle` from the last
/// one, `found`, in the direction asked, selected and scrolled onto the screen.
/// A free function over the grid so that the walk can be tested without a PTY.
fn search_in(term: &mut Term<Proxy>, found: &mut Option<(Point, Point)>, needle: &str, back: bool) -> bool {
    let Ok(mut re) = RegexSearch::new(needle) else { return false };
    // From just past the last match *in the direction of this search*, so a
    // repeat walks the matches rather than finding the same one. The match's
    // two ends are kept rather than one stepped-past point: that point was
    // stepped the way the *last* search went, so the first press after turning
    // round started beside the current match and found it again -- `<C-S-b>`
    // after `<C-S-n>` did nothing once (#93, 1.9g).
    let origin = match *found {
        Some((start, _)) if back => start.sub(&*term, Boundary::Grid, 1),
        Some((_, end)) => end.add(&*term, Boundary::Grid, 1),
        None => search_origin(term, back),
    };
    let dir = if back { Direction::Left } else { Direction::Right };
    let Some(m) = term.search_next(&mut re, origin, dir, Side::Left, None) else {
        // Wrap: a search that runs off the end starts again.
        *found = None;
        return false;
    };
    let hit = *m.start();
    // Put the line holding the match on screen.
    let want = (-hit.line.0).max(0);
    let now = term.grid().display_offset() as i32;
    term.scroll_display(Scroll::Delta(want - now));
    term.selection = Some(Selection::new(SelectionType::Simple, hit, Side::Left));
    if let Some(sel) = term.selection.as_mut() {
        sel.update(*m.end(), Side::Right);
    }
    *found = Some((hit, *m.end()));
    true
}

/// A terminal built without a PTY, for tests anywhere in the crate.
#[cfg(test)]
pub mod testing {
    use super::*;

    /// A terminal with `lines` rows of screen and room to scroll back.
    pub fn term(cols: usize, lines: usize) -> Term<Proxy> {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let proxy = Proxy { tx, wake: Arc::new(|| {}) };
        let cfg = Config { scrolling_history: 200, ..Default::default() };
        Term::new(cfg, &Size::new(cols, lines), proxy)
    }

    /// Feed `text` through the parser, as the PTY reader thread would.
    pub fn feed(t: &mut Term<Proxy>, text: &str) {
        let mut parser = alacritty_terminal::vte::ansi::Processor::<
            alacritty_terminal::vte::ansi::StdSyncHandler,
        >::default();
        for b in text.as_bytes() {
            parser.advance(t, &[*b]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{feed, term};
    use super::*;

    /// Turning round mid-search moves at once: one `<C-S-b>` undoes one
    /// `<C-S-n>` (#93, 1.9g). Before, the first press after a change of
    /// direction found the match it was already on.
    #[test]
    fn a_search_turns_round_in_one_press() {
        let mut t = term(40, 5);
        for i in 0..30 {
            feed(&mut t, &format!("line {i:02} hit\r\n"));
        }
        let mut found = None;
        let line = |f: &Option<(Point, Point)>| f.expect("a match").0.line.0;
        // `back` walks toward the older lines, as `<C-S-f>` and `<C-S-n>` do.
        assert!(search_in(&mut t, &mut found, "hit", true));
        let first = line(&found);
        assert!(search_in(&mut t, &mut found, "hit", true));
        let second = line(&found);
        assert!(second < first, "a second press goes further back: {first} then {second}");

        assert!(search_in(&mut t, &mut found, "hit", false));
        assert_eq!(line(&found), first, "one press the other way comes straight back");
        assert!(search_in(&mut t, &mut found, "hit", true));
        assert_eq!(line(&found), second, "and one more returns to where it was");
    }

    /// `children` finds a process this one started: the question `<C-S-t>`
    /// asks of the shell before ending it (Q21). Asserted on the child's own
    /// pid rather than on "none before, one after", because tests running
    /// beside this one start processes of their own (git, mostly).
    #[test]
    fn a_started_process_is_found_among_the_children() {
        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "ping -n 30 127.0.0.1 >nul"]).spawn()
        } else {
            std::process::Command::new("sleep").arg("30").spawn()
        }
        .expect("a child process to look for");
        let found = children(std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        assert!(found.contains(&child.id()), "{} among {found:?}", child.id());
    }

    /// The scrollback keys moved the view and the screen did not follow.
    ///
    /// `snapshot` copied `Line(0)..Line(screen_lines)`, which is the active
    /// area whatever `display_offset` says, so `<S-PageUp>` and the wheel both
    /// changed a number nothing was reading. The badge made it worse by being
    /// right: it said "16 lines back" over a screen that had not moved.
    #[test]
    fn scrolling_back_shows_the_older_lines() {
        let mut t = term(20, 4);
        for i in 0..20 {
            feed(&mut t, &format!("line{i}\r\n"));
        }
        let row = |t: &Term<Proxy>, n: usize| -> String {
            snapshot(t)[n].iter().map(|c| c.c).collect::<String>().trim_end().to_string()
        };

        // The cursor sits on a fresh line below `line19`, so the top of a
        // four-line screen is `line17`, not `line16`.
        let bottom = row(&t, 0);
        assert_eq!(bottom, "line17");

        t.scroll_display(Scroll::Delta(3));
        assert_eq!(t.grid().display_offset(), 3, "the offset is the easy half");

        let scrolled = row(&t, 0);
        assert_ne!(scrolled, bottom, "the screen has to follow the offset");
        assert_eq!(scrolled, "line14", "three older than the row that was on top");

        t.scroll_display(Scroll::Bottom);
        assert_eq!(row(&t, 0), bottom, "and come back");
    }

    /// The cursor belongs to a row, so it travels with it.
    #[test]
    fn the_cursor_leaves_with_its_row() {
        let mut t = term(20, 4);
        for i in 0..20 {
            feed(&mut t, &format!("line{i}\r\n"));
        }
        let (_, before) = cursor_cell(&t);
        assert!(before < 4, "on screen to start with: {before}");

        t.scroll_display(Scroll::Delta(3));
        let (_, after) = cursor_cell(&t);
        assert_eq!(after, before + 3, "it moves down as older lines come in above");
        assert!(after >= 4, "and is off the screen, so the pane draws nothing");
    }

    /// A drag copied the text and showed nothing, so there was no way to see
    /// what was about to be copied. The cells carry the flag now.
    #[test]
    fn a_selection_marks_the_cells_it_covers() {
        use alacritty_terminal::index::Side;
        use alacritty_terminal::selection::{Selection, SelectionType};

        let mut t = term(20, 4);
        feed(&mut t, "hello world\r\n");
        let row = 0;
        let mut sel = Selection::new(
            SelectionType::Simple,
            Point::new(Line(row), Column(0)),
            Side::Left,
        );
        sel.update(Point::new(Line(row), Column(4)), Side::Right);
        t.selection = Some(sel);

        let rows = snapshot(&t);
        let marked: String = rows[row as usize]
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.c)
            .collect();
        assert_eq!(marked, "hello", "exactly the dragged cells");
        assert!(rows[1].iter().all(|c| !c.selected), "and nothing on other rows");
    }

    /// Which match a search lands on first.
    ///
    /// Alacritty wraps, so a backwards search finds *something* from anywhere;
    /// the origin decides what. From the top line of the view it steps over
    /// every row underneath and goes into the history, landing far from where
    /// the reader is looking even when the word is on screen. From the bottom
    /// right it finds the nearest one going up, which is what a terminal's
    /// find does.
    #[test]
    fn a_backwards_search_starts_at_the_bottom_of_the_view() {
        use alacritty_terminal::term::search::RegexSearch;

        let mut t = term(20, 4);
        feed(&mut t, "target early\r\n");
        for i in 0..20 {
            feed(&mut t, &format!("filler{i}\r\n"));
        }
        feed(&mut t, "target late\r\n");

        let origin = search_origin(&t, true);
        assert_eq!(origin.line, Line(3), "the bottom row of the view, not the top");

        let mut re = RegexSearch::new("target").unwrap();
        let near = t
            .search_next(&mut re, origin, Direction::Left, Side::Left, None)
            .expect("there are two of them");
        assert_eq!(near.start().line, Line(2), "the one on screen, just above the origin");

        let from_top = Point::new(Line(0), Column(0));
        let far = t
            .search_next(&mut re, from_top, Direction::Left, Side::Left, None)
            .expect("wrapping means this finds one too");
        assert!(far.start().line < Line(0), "but the old one, up in the history");
    }

    /// Forwards still starts at the top of the view.
    #[test]
    fn a_forwards_search_starts_at_the_top_of_the_view() {
        let mut t = term(20, 4);
        for i in 0..20 {
            feed(&mut t, &format!("line{i}\r\n"));
        }
        assert_eq!(search_origin(&t, false).line, Line(0));
        t.scroll_display(Scroll::Delta(2));
        assert_eq!(t.grid().display_offset(), 2, "there is history to move into");
        assert_eq!(search_origin(&t, false).line, Line(-2), "which moves with the view");
    }

    /// Dragging the other way must select the same text.
    ///
    /// The sides used to be fixed -- `Left` to start, `Right` to extend --
    /// which is right only while the drag runs left to right. Alacritty
    /// settles a range by ordering the two anchors and then dropping the first
    /// cell if that anchor reads `Right` and the last if it reads `Left`, so a
    /// backwards drag swapped them into exactly the two cases that drop, and
    /// lost a character at each end. Reported as the first character of the
    /// line refusing to be copied.
    #[test]
    fn a_drag_selects_the_same_text_in_either_direction() {
        let selected = |t: &Term<Proxy>| -> String {
            snapshot(t)[0].iter().filter(|c| c.selected).map(|c| c.c).collect()
        };
        // The pointer is on the outer half of each end: the left half of the
        // cell the drag starts from and the right half of the one it ends on,
        // whichever way round those are.
        let mut t = term(20, 4);
        feed(&mut t, "hello world");

        select_at(&mut t, (0, 0), false, true);
        select_at(&mut t, (4, 0), true, false);
        assert_eq!(selected(&t), "hello", "left to right");

        let mut t = term(20, 4);
        feed(&mut t, "hello world");
        select_at(&mut t, (4, 0), true, true);
        select_at(&mut t, (0, 0), false, false);
        assert_eq!(selected(&t), "hello", "right to left, and the h is not dropped");
    }

    /// The half the pointer is in is what makes that work, so it has to count.
    #[test]
    fn the_half_of_the_cell_decides_what_is_included() {
        let selected = |t: &Term<Proxy>| -> String {
            snapshot(t)[0].iter().filter(|c| c.selected).map(|c| c.c).collect()
        };
        let mut t = term(20, 4);
        feed(&mut t, "hello world");

        // Starting on the *right* half of `h` leaves it out, which is what a
        // terminal does and what makes a backwards drag come out right.
        select_at(&mut t, (0, 0), true, true);
        select_at(&mut t, (4, 0), true, false);
        assert_eq!(selected(&t), "ello", "the cell the drag started past is not in it");
    }

    fn s(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).unwrap().replace('\x1b', "<ESC>")
    }

    #[test]
    fn the_plain_keys_send_what_a_terminal_expects() {
        let n = Mods::default();
        // Enter is a carriage return, not a newline, and Backspace is DEL.
        assert_eq!(encode(Special::Enter, n, false), b"\r");
        assert_eq!(encode(Special::Backspace, n, false), b"\x7f");
        assert_eq!(encode(Special::Tab, n, false), b"\t");
        assert_eq!(s(encode(Special::Escape, n, false)), "<ESC>");
        assert_eq!(s(encode(Special::Up, n, false)), "<ESC>[A");
        assert_eq!(s(encode(Special::Delete, n, false)), "<ESC>[3~");
        assert_eq!(s(encode(Special::F(1), n, false)), "<ESC>OP");
        assert_eq!(s(encode(Special::F(5), n, false)), "<ESC>[15~");
        assert_eq!(s(encode(Special::F(12), n, false)), "<ESC>[24~");
    }

    /// vim and readline put the terminal in application-cursor mode and then
    /// expect SS3 arrows; sending CSI there types letters into the buffer.
    #[test]
    fn the_arrows_follow_the_terminals_cursor_mode() {
        let n = Mods::default();
        assert_eq!(s(encode(Special::Up, n, true)), "<ESC>OA");
        assert_eq!(s(encode(Special::Down, n, true)), "<ESC>OB");
        assert_eq!(s(encode(Special::Right, n, true)), "<ESC>OC");
        assert_eq!(s(encode(Special::Left, n, true)), "<ESC>OD");
        // Only the arrows change; the rest is the same either way.
        assert_eq!(encode(Special::Enter, n, true), encode(Special::Enter, n, false));
    }

    #[test]
    fn a_modifier_turns_a_key_into_its_parameterised_form() {
        let ctrl = Mods { ctrl: true, ..Default::default() };
        let shift = Mods { shift: true, ..Default::default() };
        let both = Mods { ctrl: true, shift: true, ..Default::default() };
        // 1 + shift(1) + alt(2) + ctrl(4), which is xterm's numbering.
        assert_eq!(s(encode(Special::Right, ctrl, false)), "<ESC>[1;5C");
        assert_eq!(s(encode(Special::Right, shift, false)), "<ESC>[1;2C");
        assert_eq!(s(encode(Special::Right, both, false)), "<ESC>[1;6C");
        // Application-cursor mode gives way to the modifier form.
        assert_eq!(s(encode(Special::Up, ctrl, true)), "<ESC>[1;5A");
        // Alt on a key with no parameterised form is the escape prefix.
        let alt = Mods { alt: true, ..Default::default() };
        assert_eq!(s(encode(Special::Enter, alt, false)), "<ESC>\r");
    }

    #[test]
    fn ctrl_with_a_letter_is_the_control_code_it_stands_for() {
        assert_eq!(control_code('c', false), Some(vec![0x03]));
        assert_eq!(control_code('C', false), Some(vec![0x03]));
        assert_eq!(control_code('d', false), Some(vec![0x04]));
        assert_eq!(control_code('[', false), Some(vec![0x1b]));
        assert_eq!(control_code(' ', false), Some(vec![0x00]));
        // Alt as well: the escape prefix in front of the control code.
        assert_eq!(control_code('c', true), Some(vec![0x1b, 0x03]));
        // Nothing sensible to send, so nothing is sent.
        assert_eq!(control_code('é', false), None);
    }

    fn osc7(path: &str) -> Vec<u8> {
        format!("\x1b]7;file://host{path}\x07").into_bytes()
    }

    #[test]
    fn a_shell_saying_where_it_is_is_understood() {
        let mut partial = Vec::new();
        assert_eq!(
            scan_osc7(&mut partial, &osc7("/home/user/src")),
            vec![PathBuf::from("/home/user/src")]
        );
        // Terminated by ST rather than BEL, which is equally correct.
        let st = b"\x1b]7;file:///tmp\x1b\\";
        assert_eq!(scan_osc7(&mut partial, st), vec![PathBuf::from("/tmp")]);
        // Percent-escapes, which is how a space arrives.
        let esc = b"\x1b]7;file://h/a%20b/c\x07";
        assert_eq!(scan_osc7(&mut partial, esc), vec![PathBuf::from("/a b/c")]);
        // Ordinary output carries none, and must not be mistaken for one.
        assert!(scan_osc7(&mut partial, b"just some output\r\n").is_empty());
        // The tail of every read is kept in case a start marker was split
        // across it, but only ever those few bytes: output does not pile up.
        for _ in 0..50 {
            scan_osc7(&mut partial, &vec![b'x'; 4096]);
        }
        assert!(partial.len() < 4, "the carry stays small: {}", partial.len());
    }

    /// A read stops wherever the pipe happened to fill, which can be in the
    /// middle of the escape sequence.
    #[test]
    fn a_sequence_split_across_two_reads_is_still_one() {
        let whole = osc7("/var/log");
        for at in 1..whole.len() {
            let mut partial = Vec::new();
            let first = scan_osc7(&mut partial, &whole[..at]);
            let second = scan_osc7(&mut partial, &whole[at..]);
            let got: Vec<PathBuf> = first.into_iter().chain(second).collect();
            assert_eq!(got, vec![PathBuf::from("/var/log")], "split at {at}");
        }
    }

    #[test]
    fn two_in_one_read_are_both_seen() {
        let mut partial = Vec::new();
        let mut chunk = osc7("/one");
        chunk.extend_from_slice(b"some output\n");
        chunk.extend_from_slice(&osc7("/two"));
        assert_eq!(
            scan_osc7(&mut partial, &chunk),
            vec![PathBuf::from("/one"), PathBuf::from("/two")]
        );
    }

    /// A sequence that never terminates must not grow the buffer for ever.
    #[test]
    fn an_unterminated_sequence_is_given_up_on() {
        let mut partial = Vec::new();
        assert!(scan_osc7(&mut partial, b"\x1b]7;file://h/start").is_empty());
        assert!(!partial.is_empty(), "it is still waiting for the end");
        for _ in 0..10 {
            scan_osc7(&mut partial, &vec![b'x'; 1024]);
        }
        assert!(partial.len() <= MAX_OSC, "got {}", partial.len());
    }

    #[test]
    fn a_windows_file_url_names_a_windows_path() {
        // `file:///C:/dev/filer`: the slash before the drive letter goes.
        let got = from_file_url(b"file:///C:/dev/filer").unwrap();
        assert_eq!(got, crate::util::normalize(Path::new("C:/dev/filer")));
        // A bare root stays one.
        assert_eq!(from_file_url(b"file:///"), Some(PathBuf::from("/")));
        // Anything that is not a file URL is not a directory.
        assert_eq!(from_file_url(b"http://example.com/"), None);
        assert_eq!(from_file_url(b"nonsense"), None);
    }

    /// 29.4 / #101: a share comes back as a share, however many slashes the
    /// shell put in front of the host. Windows only: elsewhere `//x` is an
    /// ordinary path and `normalize` folds it, so both sides would agree
    /// whether the slashes were kept or not.
    #[cfg(windows)]
    #[test]
    fn a_share_keeps_its_two_leading_slashes() {
        let want = crate::util::normalize(Path::new("//localhost/C$/dev"));
        assert_eq!(from_file_url(b"file://///localhost/C$/dev"), Some(want.clone()), "PowerShell's five");
        assert_eq!(from_file_url(b"file:////localhost/C$/dev"), Some(want), "and four");
    }

    /// A paste is bracketed only when the program on the other end asked, and
    /// the markers go outside the text rather than into it.
    #[test]
    fn a_paste_is_bracketed_only_when_it_was_asked_for() {
        assert_eq!(bracket("ls -l", false), b"ls -l".to_vec());
        assert_eq!(bracket("ls -l", true), b"\x1b[200~ls -l\x1b[201~".to_vec());
        // The case the brackets exist for: without them these are two commands
        // the shell runs, with them two lines it holds.
        assert_eq!(
            bracket("one\rtwo", true),
            b"\x1b[200~one\rtwo\x1b[201~".to_vec()
        );
        // Nothing to paste stays nothing, not a pair of bare markers going to a
        // shell that would print them.
        assert_eq!(bracket("", false), Vec::<u8>::new());
        assert_eq!(bracket("", true), Vec::<u8>::new());
    }

    /// `Esc` as a win32-input-mode record: the fields tcell reads it by, and
    /// **no release record after it**.
    ///
    /// The release is what lost the key. A program in VT input mode gets the
    /// press as a lone `0x1b` and waits 50ms; a release arriving inside that
    /// wait is turned by tcell into a second ESC, which makes the first one an
    /// Alt prefix, and the key never comes out. So the assertion that matters
    /// is that exactly one record is sent, and that it is a press.
    #[test]
    fn escape_goes_as_one_key_press_and_no_release() {
        let s = |b: Vec<u8>| String::from_utf8(b).unwrap();
        let esc = s(win32_key(0x1b, 1, 0x1b, Mods::default()));
        assert_eq!(esc, "\x1b[27;1;27;1;0;1_", "VK_ESCAPE, scan code 1, pressed");
        assert_eq!(esc.matches('_').count(), 1, "one record, no release after it");

        let shift = Mods { shift: true, ..Default::default() };
        let ctrl_alt = Mods { ctrl: true, alt: true, ..Default::default() };
        assert_eq!(s(win32_key(0x1b, 1, 0x1b, shift)), "\x1b[27;1;27;1;16;1_");
        assert_eq!(s(win32_key(0x1b, 1, 0x1b, ctrl_alt)), "\x1b[27;1;27;1;10;1_");
    }

    /// The PTY log spells control characters out, and cannot be misread.
    ///
    /// `\e[?9001;0$y` has to read as the reply it is, and a literal backslash
    /// followed by `e` in the shell's output must not look like an ESC.
    #[test]
    fn the_pty_log_spells_the_bytes_out() {
        assert_eq!(escape_bytes(b"\x1b[?9001;0$y"), "\\e[?9001;0$y");
        assert_eq!(escape_bytes(b"a\r\n\tb\x07\x7f"), "a\\r\\n\\tb\\x07\\x7F");
        assert_eq!(escape_bytes(br"C:\e"), r"C:\\e", "a real backslash is doubled");
        assert_eq!(escape_bytes("日本".as_bytes()), "日本", "text stays text");
    }

    /// Each chunk is one line: time, direction, bytes -- appended, so a second
    /// pane in the same run adds to the file rather than replacing it.
    #[test]
    fn the_pty_log_writes_one_line_per_chunk() {
        let dir = crate::util::test_dir("pty-log");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pty.log");
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
        let log: PtyLog = Some(Arc::new(Mutex::new(PtyLogFile { file, start: Instant::now() })));
        log_pty(&log, "out", b"\x1b[?9001h");
        log_pty(&log, "in reply", b"\x1b[?6c");
        log_pty(&None, "in key", b"ignored when the log is off");
        drop(log);

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "one line per chunk, nothing when off: {text:?}");
        assert!(lines[0].ends_with("out       \\e[?9001h"), "{:?}", lines[0]);
        assert!(lines[1].ends_with("in reply  \\e[?6c"), "{:?}", lines[1]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A path reaches the shell as one word -- in the form *that* shell reads.
    ///
    /// The single quote is the whole point. This test used to assert the POSIX
    /// escape for every shell, which is how the bug survived: PowerShell reads
    /// `'it'\''s'` as the string `it`, a stray backslash, and an unterminated
    /// string, and sits at `>>` waiting for the rest.
    #[test]
    fn a_path_reaches_the_shell_as_one_word() {
        use Quoting::*;
        // Nothing awkward in it, so nothing is added, whatever the shell.
        for how in [PowerShell, Posix, Cmd] {
            assert_eq!(quote("/home/user/src", how), "/home/user/src", "{how:?}");
        }

        // A Windows path is bare only where `\` is just a separator. This line
        // asserted the bare form for `Posix` too, which is how Git Bash came to
        // receive `cd R:\Temp\x` and read it as `R:Tempx`: a backslash escapes
        // the character after it, so the separators ate the directory names and
        // no ordinary path could be walked into at all.
        assert_eq!(quote(r"C:\dev\filer", PowerShell), r"C:\dev\filer");
        assert_eq!(quote(r"C:\dev\filer", Cmd), r"C:\dev\filer");
        assert_eq!(quote(r"C:\dev\filer", Posix), r"'C:\dev\filer'", "quoted, so `\\` survives");
        // Forward slashes are a separator in every shell, so those stay bare --
        // the quoting is about the backslash, not about being a Windows path.
        assert_eq!(quote("C:/dev/filer", Posix), "C:/dev/filer");

        // A space would split it in two without the quotes.
        assert_eq!(quote("/a b/c", PowerShell), "'/a b/c'");
        assert_eq!(quote("/a b/c", Posix), "'/a b/c'");
        assert_eq!(quote("/a b/c", Cmd), "\"/a b/c\"");

        // The one they disagree about.
        assert_eq!(quote("it's", PowerShell), "'it''s'", "doubled, not escaped");
        assert_eq!(quote("it's", Posix), r"'it'\''s'");
        // cmd does not treat `'` as anything, so it needs no help at all.
        assert_eq!(quote("it's", Cmd), "\"it's\"");

        assert_eq!(quote("", PowerShell), "''");
        assert_eq!(quote("", Posix), "''");
        assert_eq!(quote("", Cmd), "\"\"");
    }
    /// Q29: PowerShell 7 is preferred on Windows when it is there, and
    /// nothing changes anywhere else.
    #[test]
    fn the_default_shell_is_pwsh_on_windows_when_it_is_installed() {
        assert_eq!(pick_default_shell(true, true), Some("pwsh".to_owned()));
        assert_eq!(pick_default_shell(true, false), None, "5.1, the platform default");
        assert_eq!(pick_default_shell(false, true), None, "a pwsh on Linux is not the login shell");
    }


    /// What `[term] shell` names decides the convention, and the fallback is
    /// the platform's rather than one convention for everybody.
    #[test]
    fn the_shells_name_picks_the_quoting() {
        let native = if cfg!(windows) { Quoting::PowerShell } else { Quoting::Posix };
        // `None` is what `tty::Options` starts by default.
        assert_eq!(Quoting::for_shell(None), native, "the platform default shell");

        assert_eq!(Quoting::for_shell(Some("powershell")), Quoting::PowerShell);
        assert_eq!(Quoting::for_shell(Some("pwsh")), Quoting::PowerShell);
        assert_eq!(Quoting::for_shell(Some("cmd")), Quoting::Cmd);
        assert_eq!(Quoting::for_shell(Some("bash")), Quoting::Posix);
        assert_eq!(Quoting::for_shell(Some("zsh")), Quoting::Posix);

        // A full path, and the case Windows writes it in.
        assert_eq!(Quoting::for_shell(Some(r"C:\WINDOWS\System32\cmd.exe")), Quoting::Cmd);
        assert_eq!(Quoting::for_shell(Some(r"C:\Program Files\PowerShell\7\pwsh.exe")), Quoting::PowerShell);
        assert_eq!(Quoting::for_shell(Some("/usr/bin/bash")), Quoting::Posix);

        // Something nobody listed: guess by platform rather than by habit.
        assert_eq!(Quoting::for_shell(Some("nushell")), native);
    }
}

#[cfg(test)]
mod readme_snippet {
    use super::scan_osc7;
    use std::path::{Path, PathBuf};

    /// The path the parser should arrive at, written the way the shell writes
    /// it — with forward slashes and no drive-letter slash.
    ///
    /// Not a Windows literal such as `r"C:\dev\filer"`: on Linux a backslash is
    /// an ordinary character, so that literal is one long component and the
    /// comparison fails on the machine the tests are usually run on. Putting
    /// the expected value through the same `normalize` the parser ends with
    /// keeps these tests about what they are about — the OSC framing and the
    /// escaping — and leaves separator spelling to `util::normalize`'s own
    /// tests.
    fn as_the_shell_names_it(path: &str) -> PathBuf {
        crate::util::normalize(Path::new(path))
    }

    /// Exactly what the PowerShell hook in the README emits, byte for byte.
    ///
    /// `[Console]::Write("$([char]27)]7;file:///$p$([char]27)\")` with `$p` a
    /// Windows path whose backslashes have been turned into forward ones. If
    /// this stops parsing, the README is telling people to paste something that
    /// does not work — and they will conclude the feature is broken rather than
    /// the instructions.
    fn as_powershell_writes_it(drive_path: &str) -> Vec<u8> {
        format!("\x1b]7;file:///{drive_path}\x1b\\").into_bytes()
    }

    #[test]
    fn the_readme_hook_is_understood() {
        let mut carry = Vec::new();
        assert_eq!(
            scan_osc7(&mut carry, &as_powershell_writes_it("C:/dev/filer")),
            vec![as_the_shell_names_it("C:/dev/filer")],
        );
    }

    /// The README says spaces and non-ASCII need no escaping. That is a promise
    /// about this parser, so it is checked here.
    #[test]
    fn spaces_and_japanese_need_no_escaping() {
        let mut carry = Vec::new();
        assert_eq!(
            scan_osc7(&mut carry, &as_powershell_writes_it("C:/my docs/報告書")),
            vec![as_the_shell_names_it("C:/my docs/報告書")],
        );
    }

    /// And escaped anyway, since the README calls that optional rather than
    /// wrong.
    #[test]
    fn percent_escaped_is_accepted_too() {
        let mut carry = Vec::new();
        assert_eq!(
            scan_osc7(&mut carry, &as_powershell_writes_it("C:/my%20docs")),
            vec![as_the_shell_names_it("C:/my docs")],
        );
    }
}

/// Q27: keys as win32-input-mode records once the other end asks for them.
#[cfg(test)]
mod win32_input_tests {
    use super::*;

    fn s(b: Vec<u8>) -> String {
        String::from_utf8(b).unwrap()
    }

    /// The request is seen however the reads fall, and the last one wins.
    #[test]
    fn the_request_is_seen_even_when_a_read_cuts_it() {
        let mut tail = Vec::new();
        assert_eq!(scan_win32_mode(&mut tail, b"hello \x1b[?90"), None);
        assert_eq!(scan_win32_mode(&mut tail, b"01h and more"), Some(true), "across two reads");
        assert_eq!(scan_win32_mode(&mut tail, b"\x1b[?9001l"), Some(false));
        assert_eq!(scan_win32_mode(&mut tail, b"\x1b[?9001h\x1b[?9001l"), Some(false), "the last one");
        assert_eq!(scan_win32_mode(&mut tail, b"\x1b[?1049h"), None, "another mode is not this one");
        assert!(tail.len() < 10, "and the carry stays short: {}", tail.len());
    }

    /// `\e[?9001$p` was answered "not recognised"; now it is answered as it is.
    #[test]
    fn the_query_is_answered_with_the_state() {
        assert_eq!(answer_win32_query("\x1b[?9001;0$y".into(), true), "\x1b[?9001;1$y");
        assert_eq!(answer_win32_query("\x1b[?9001;0$y".into(), false), "\x1b[?9001;2$y");
        assert_eq!(answer_win32_query("\x1b[?1049;1$y".into(), true), "\x1b[?1049;1$y", "others untouched");
    }

    /// `<S-End>` -- the key that took lazygit's `<Esc>` with it -- is one
    /// record with no ESC in front of its own, and so is every special key.
    #[test]
    fn special_keys_are_records() {
        let shift = Mods { shift: true, ..Default::default() };
        assert_eq!(s(special_record(Special::End, shift)), "\x1b[35;79;0;1;272;1_");
        assert_eq!(s(special_record(Special::Escape, Mods::default())), "\x1b[27;1;27;1;0;1_");
        assert_eq!(s(special_record(Special::Enter, Mods::default())), "\x1b[13;28;13;1;0;1_");
        assert_eq!(s(special_record(Special::BackTab, Mods::default())), "\x1b[9;15;9;1;16;1_");
        assert_eq!(s(special_record(Special::F(5), Mods::default())), "\x1b[116;63;0;1;0;1_");
        assert_eq!(s(special_record(Special::F(12), Mods::default())), "\x1b[123;88;0;1;0;1_");
    }

    /// Ctrl carries the control code, Alt the character, Alt+Shift the capital.
    #[test]
    fn chords_are_records() {
        let ctrl = Mods { ctrl: true, ..Default::default() };
        let alt = Mods { alt: true, ..Default::default() };
        let alt_shift = Mods { alt: true, shift: true, ..Default::default() };
        assert_eq!(s(char_record('c', ctrl).unwrap()), "\x1b[67;46;3;1;8;1_");
        assert_eq!(s(char_record('b', alt).unwrap()), "\x1b[66;48;98;1;2;1_");
        assert_eq!(s(char_record('b', alt_shift).unwrap()), "\x1b[66;48;66;1;18;1_");
        assert_eq!(s(char_record('1', alt).unwrap()), "\x1b[49;2;49;1;2;1_");
        assert_eq!(char_record('[', ctrl), None, "no record form: the caller keeps the old bytes");
        assert_eq!(char_record('a', Mods::default()), None, "a plain letter is text");
    }
}

/// The alternate screen, and the meta prefix — the two things the pane needs
/// in order to know when a key is its own and when it belongs to a program.
#[cfg(test)]
mod alt_screen_tests {
    use super::testing::{feed, term};
    use super::*;

    /// `nvim` and `less` switch with DECSET 1049 and back with DECRST.
    #[test]
    fn the_flag_follows_the_escape_sequence() {
        let mut t = term(20, 5);
        assert!(!alt_screen(&t), "a fresh terminal is on the primary screen");

        feed(&mut t, "\x1b[?1049h");
        assert!(alt_screen(&t), "a full-screen program has taken the screen");

        feed(&mut t, "\x1b[?1049l");
        assert!(!alt_screen(&t), "and given it back on the way out");
    }

    /// Q28: what nvim sends at startup turns the wheel into mouse reports, in
    /// the SGR form it asked for; turning the mouse off again turns them off.
    #[test]
    fn a_program_that_asks_for_the_mouse_is_told_about_the_wheel() {
        let mut t = term(80, 24);
        assert_eq!(mouse_report(&t), MouseReport::Off);

        feed(&mut t, "\x1b[?1049h\x1b[?1002h\x1b[?1006h");
        assert_eq!(mouse_report(&t), MouseReport::Sgr);
        assert_eq!(wheel_report(true, 9, 4, MouseReport::Sgr), b"\x1b[<64;10;5M");
        assert_eq!(wheel_report(false, 0, 0, MouseReport::Sgr), b"\x1b[<65;1;1M");

        feed(&mut t, "\x1b[?1006l");
        assert_eq!(mouse_report(&t), MouseReport::Plain, "still reporting, the old way");
        assert_eq!(wheel_report(true, 9, 4, MouseReport::Plain), [0x1b, b'[', b'M', 96, 42, 37]);
        assert_eq!(wheel_report(false, 500, 0, MouseReport::Plain)[4], 255, "a far column is capped");

        feed(&mut t, "\x1b[?1002l");
        assert_eq!(mouse_report(&t), MouseReport::Off, "and `less` never asks");
        assert!(wheel_report(true, 0, 0, MouseReport::Off).is_empty());
    }

    /// The reason the flag is the right question: there is nothing to scroll
    /// to up there, so a scrolling key spent on the alternate screen is spent
    /// on nothing.
    #[test]
    fn the_alternate_screen_has_no_scrollback() {
        let mut t = term(20, 3);
        for i in 0..40 {
            feed(&mut t, &format!("line {i}\r\n"));
        }
        t.scroll_display(alacritty_terminal::grid::Scroll::Top);
        assert!(t.grid().display_offset() > 0, "the primary screen scrolls back");

        feed(&mut t, "\x1b[?1049h");
        t.scroll_display(alacritty_terminal::grid::Scroll::Top);
        assert_eq!(t.grid().display_offset(), 0, "the alternate screen does not");
    }

    /// `Alt`+letter is `ESC` then the letter, the same prefix `control_code`
    /// puts in front of a control chord.
    #[test]
    fn meta_is_escape_then_the_character() {
        assert_eq!(meta_char('b'), vec![0x1b, b'b']);
        assert_eq!(meta_char('f'), vec![0x1b, b'f']);
        assert_eq!(meta_char('J'), vec![0x1b, b'J']);
        // Whatever the character costs in UTF-8, the prefix is one byte.
        assert_eq!(meta_char('あ'), [vec![0x1b], "あ".as_bytes().to_vec()].concat());
        // The same shape `control_code` produces for Alt with Ctrl.
        assert_eq!(control_code('b', true).unwrap()[0], 0x1b);
    }
}
