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

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use alacritty_terminal::event::{Event as PtyEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
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
}

/// Longest OSC 7 worth waiting for. A path cannot sensibly be longer, and a
/// stream that never terminates one must not grow this for ever.
const MAX_OSC: usize = 4096;

impl io::Read for Tapped {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.reader().read(buf)?;
        for path in scan_osc7(&mut self.partial, &buf[..n]) {
            let _ = self.cwd.send(path);
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
    // Windows spells it `file:///C:/dir`, which is a path once the slash goes.
    let trimmed = match decoded.as_bytes() {
        [b'/', c, b':', ..] if c.is_ascii_alphabetic() => &decoded[1..],
        _ => decoded,
    };
    Some(crate::util::normalize(Path::new(trimmed)))
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
    cwd_rx: Receiver<PathBuf>,
    /// Where the shell says it is, when it says so at all. A shell that does
    /// not send OSC 7 leaves this `None` for ever, which is why it only ever
    /// suppresses work rather than driving any.
    pub shell_cwd: Option<PathBuf>,
}

impl Terminal {
    /// Start a shell in `cwd`. The cell size is what the PTY is told, so a
    /// program asking for pixels (an image protocol, say) gets the truth.
    pub fn spawn(
        cwd: &Path,
        size: Size,
        cell: (u16, u16),
        wake: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let options = tty::Options {
            shell: None,
            working_directory: Some(cwd.to_path_buf()),
            drain_on_exit: false,
            env: Default::default(),
            #[cfg(target_os = "windows")]
            escape_args: true,
        };
        let window = window_size(size, cell);
        let pty = tty::new(&options, window, 0)?;
        let (cwd_tx, cwd_rx) = crossbeam_channel::unbounded();
        let pty = Tapped { inner: pty, cwd: cwd_tx, partial: Vec::new() };

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
            cwd_rx,
            shell_cwd: None,
        })
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
                PtyEvent::PtyWrite(text) => self.send(text.into_bytes()),
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
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    /// Type text in, as a paste rather than as keys.
    pub fn paste(&self, text: &str) {
        // Carriage returns are what a terminal calls Enter; a pasted `\n`
        // that stays a newline confuses a line editor.
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        self.send(text.into_bytes());
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
        let quoted = quote(&cwd.to_string_lossy());
        self.send(format!("cd {quoted}\r").into_bytes());
    }

    /// Run the terminal's own locked grid through `f`. Locking is the caller's
    /// business to keep short: the reader thread wants it back.
    pub fn with_grid<R>(&self, f: impl FnOnce(&Term<Proxy>) -> R) -> R {
        f(&self.term.lock())
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

/// Wrap a path for a shell that is about to read it as one word.
///
/// Single quotes are the only form every POSIX shell agrees on, and PowerShell
/// reads them the same way; a single quote inside is closed, escaped and
/// reopened, which both understand.
pub fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || "_-./:\\".contains(c)) {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
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

/// Where the cursor is, in cells, for the renderer to draw over.
pub fn cursor_cell<T: EventListener>(term: &Term<T>) -> (usize, usize) {
    let p = term.grid().cursor.point;
    (p.column.0, p.line.0.max(0) as usize)
}

/// Whether the arrows should be sent as SS3 rather than CSI.
pub fn app_cursor<T: EventListener>(term: &Term<T>) -> bool {
    use alacritty_terminal::term::TermMode;
    term.mode().contains(TermMode::APP_CURSOR)
}

/// The visible grid as rows of cells, for a renderer that cannot hold the
/// lock while it paints.
pub fn snapshot<T: EventListener>(term: &Term<T>) -> Vec<Vec<CellView>> {
    let grid = term.grid();
    let lines = grid.screen_lines();
    let cols = grid.columns();
    let mut out = Vec::with_capacity(lines);
    for l in 0..lines {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let cell = &grid[Line(l as i32)][Column(c)];
            row.push(CellView { c: cell.c, fg: cell.fg, bg: cell.bg, flags: cell.flags });
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
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_path_reaches_the_shell_as_one_word() {
        assert_eq!(quote("/home/user/src"), "/home/user/src");
        assert_eq!(quote(r"C:\dev\filer"), r"C:\dev\filer");
        // A space would split it in two without the quotes.
        assert_eq!(quote("/a b/c"), "'/a b/c'");
        // A quote inside is closed, escaped and reopened.
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
    }
}
