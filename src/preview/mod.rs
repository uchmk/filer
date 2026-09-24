//! Preview generation, entirely off the UI thread.
//!
//! A single worker keeps previews cheap and in order: the cursor usually moves
//! faster than a file can be read, so stale requests are dropped rather than
//! queued behind newer ones.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crossbeam_channel::{Receiver, Sender};

mod font_preview;
mod image_preview;
mod markdown;
mod shell_thumb;
mod svg_preview;
mod symbols;
mod text;

/// COM for a worker thread that talks to the Windows shell.
pub use shell_thumb::init_thread as init_com_thread;

/// Identity of a preview: re-reading is only needed when one of these changes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub path: PathBuf,
    pub len: u64,
    pub mtime: Option<SystemTime>,
    /// Images are decoded for a specific box size.
    pub box_size: (u32, u32),
    /// Text columns across the pane; rendered Markdown is wrapped to fit.
    pub cols: u16,
}

#[derive(Debug)]
pub struct Request {
    pub id: u64,
    pub key: Key,
    pub mime: &'static str,
    pub ext: Option<String>,
    pub max_bytes: usize,
    pub tab_size: u8,
    pub syntect_theme: String,
}

/// A run of text sharing one style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub color: Option<[u8; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    /// Inline code, painted on a tinted background.
    pub code: bool,
}

/// Markdown laid out for reading. Lines are already wrapped to `body_cols`,
/// and each span starts at the column the widths before it add up to, so the
/// UI can place spans on a cell grid even where glyph widths disagree.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub lines: Vec<DocLine>,
    pub toc: Vec<TocEntry>,
    /// Width of the outline column; 0 when the pane is too narrow for one.
    pub toc_cols: u16,
    pub body_cols: u16,
}

impl Doc {
    /// The source line a rendered line came from.
    pub fn src_for_line(&self, line: usize) -> usize {
        self.lines.get(line).or(self.lines.last()).map_or(0, |l| l.src)
    }

    /// The first rendered line of the block holding a source line (or of the
    /// nearest one above it, for blank source lines).
    pub fn line_for_src(&self, src: usize) -> usize {
        let Some(last) = self.lines.iter().rposition(|l| l.src <= src) else { return 0 };
        let at = self.lines[last].src;
        self.lines[..last].iter().rposition(|l| l.src != at).map_or(0, |i| i + 1)
    }
}

/// Columns a string takes on the cell grid, with CJK characters counting two.
pub fn cells(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

#[derive(Clone, Debug, Default)]
pub struct DocLine {
    pub spans: Vec<Span>,
    pub kind: LineKind,
    /// Columns taken by quote bars and list markers; code bands and rules
    /// start here.
    pub indent: u16,
    /// 0-based line in the source this came from.
    pub src: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    #[default]
    Text,
    /// Last line of a heading, which H1 and H2 underline.
    Heading(u8),
    Code,
    Rule,
}

/// A line of an outline: a Markdown heading, or a declaration in source code.
#[derive(Clone, Debug, PartialEq)]
pub struct TocEntry {
    /// 1 for the outermost entries.
    pub level: u8,
    /// Indented already; the UI elides whatever doesn't fit.
    pub label: String,
    /// Line it jumps to: the first rendered line of a heading, or the source
    /// line of a declaration.
    pub line: usize,
}

/// Width of an outline column whose widest label takes `widest` cells, in a
/// pane `cols` wide.
pub fn outline_cols(widest: usize, cols: u16) -> u16 {
    let max = 32.min(cols / 3);
    (widest.min(100) as u16 + 1).clamp(16.min(max), max)
}

#[derive(Clone, Debug)]
pub enum Payload {
    /// `outline` lists the declarations of source code, if its grammar marks any.
    Text { lines: Vec<Vec<Span>>, truncated: bool, total_lines: usize, outline: Vec<TocEntry> },
    /// Markdown carries both views so switching between them needs no reload.
    Markdown { doc: Doc, source: Vec<Vec<Span>>, truncated: bool, total_lines: usize },
    /// Raw RGBA plus its dimensions; the UI turns this into a texture. The
    /// caption goes under it (the source size, a font's name, ...).
    Image { width: u32, height: u32, rgba: Arc<Vec<u8>>, caption: String },
    Binary { lines: Vec<String>, total: u64 },
    Meta { rows: Vec<(String, String)> },
    Error(String),
}

#[derive(Debug)]
pub struct Response {
    pub key: Key,
    pub payload: Payload,
}

pub struct Previewer {
    tx: Sender<Request>,
    pub rx: Receiver<Response>,
    next_id: AtomicU64,
}

impl Previewer {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, req_rx) = crossbeam_channel::unbounded::<Request>();
        let (res_tx, rx) = crossbeam_channel::unbounded::<Response>();
        std::thread::Builder::new()
            .name("preview".into())
            .spawn(move || {
                shell_thumb::init_thread();
                let mut syntax = text::Highlighter::default();
                while let Ok(req) = req_rx.recv() {
                    // Skip anything already superseded while we were busy.
                    let mut req = req;
                    while let Ok(newer) = req_rx.try_recv() {
                        req = newer;
                    }
                    let payload = render(&req, &mut syntax);
                    if res_tx.send(Response { key: req.key, payload }).is_err() {
                        return;
                    }
                    wake();
                }
            })
            .expect("spawn preview worker");
        Self { tx, rx, next_id: AtomicU64::new(1) }
    }

    pub fn request(&self, mut req: Request) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        req.id = id;
        let _ = self.tx.send(req);
        id
    }
}

fn render(req: &Request, syntax: &mut text::Highlighter) -> Payload {
    let path = &req.key.path;
    let mime = req.mime;

    if mime == "image/svg+xml" {
        return svg_preview::render(path, req.key.box_size).unwrap_or_else(Payload::Error);
    }
    if crate::mime::is_image(mime) {
        if crate::mime::is_decodable_image(mime) {
            return match image_preview::render(path, req.key.box_size) {
                Ok(p) => p,
                Err(e) => Payload::Error(e),
            };
        }
        return thumbnail(path, req, "No thumbnail handler (HEIC / AVIF need the HEIF / AV1 extensions)");
    }
    if mime.starts_with("video/") {
        return thumbnail(path, req, "No video thumbnail (codec not installed?)");
    }
    if mime.starts_with("audio/") {
        return thumbnail(path, req, "No cover art");
    }
    if mime == "application/pdf" {
        return thumbnail(path, req, "No PDF thumbnail handler (Acrobat Reader or PowerToys add one)");
    }

    // An archive's table of contents, rendered as lines so it scrolls and
    // truncates like any other text preview.
    if crate::fs::archive::Format::from_path(path).is_some() {
        return archive_listing(path);
    }

    if mime == "font/sfnt" {
        return match req.ext.as_deref() {
            Some("woff" | "woff2") => meta(path, req, "WOFF fonts are compressed; not previewed"),
            _ => font_preview::render(path, req.key.box_size).unwrap_or_else(Payload::Error),
        };
    }

    let head = match read_head(path, req.max_bytes.max(4096)) {
        Ok(b) => b,
        Err(e) => return Payload::Error(e),
    };

    if looks_binary(&head) {
        if crate::mime::is_text(mime) {
            // Mis-guessed by extension; fall through to a hex dump anyway.
        }
        return binary(path, &head);
    }

    text::render(&head, req, syntax)
}

fn read_head(path: &std::path::Path, max: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity(max.min(64 * 1024));
    f.take(max as u64).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn looks_binary(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(8192)];
    if probe.is_empty() {
        return false;
    }
    if probe.contains(&0) {
        return true;
    }
    // Rough control-character ratio; UTF-8 text stays well under this.
    let weird = probe
        .iter()
        .filter(|&&b| b < 0x09 || (0x0e..0x20).contains(&b))
        .count();
    weird * 100 / probe.len() > 2
}

fn binary(path: &std::path::Path, head: &[u8]) -> Payload {
    let total = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut lines = Vec::new();
    for (i, chunk) in head.chunks(16).take(512).enumerate() {
        let mut s = format!("{:08x}  ", i * 16);
        for (j, b) in chunk.iter().enumerate() {
            s.push_str(&format!("{b:02x} "));
            if j == 7 {
                s.push(' ');
            }
        }
        for _ in chunk.len()..16 {
            s.push_str("   ");
        }
        s.push_str(" |");
        for b in chunk {
            s.push(if (0x20..0x7f).contains(b) { *b as char } else { '.' });
        }
        s.push('|');
        lines.push(s);
    }
    Payload::Binary { lines, total }
}

/// The shell's thumbnail, or a metadata card saying why there is none.
fn thumbnail(path: &std::path::Path, req: &Request, note: &str) -> Payload {
    shell_thumb::render(path, req.key.box_size).unwrap_or_else(|_| meta(path, req, note))
}

/// How many entries of an archive are read for the pane. Enough to see what a
/// release tarball holds, few enough that a 200k-file archive does not stall
/// the worker on a preview nobody asked to read in full.
const ARCHIVE_ENTRIES: usize = 2000;

/// What is inside an archive, one entry a line: size, then name. Directories
/// are dimmed and carry no size, the way the file list draws them.
fn archive_listing(path: &std::path::Path) -> Payload {
    let (entries, more) = match crate::fs::archive::list(path, ARCHIVE_ENTRIES) {
        Ok(v) => v,
        // A password-protected or damaged archive still has a name and a size
        // worth showing, so it falls back to the card rather than an error.
        Err(e) => return Payload::Meta {
            rows: vec![
                ("Name".into(), crate::util::file_name(path)),
                ("Note".into(), format!("Cannot list: {e}")),
            ],
        },
    };
    let total = entries.len();
    let dim = Some([0x79, 0x80, 0x90]);
    let lines: Vec<Vec<Span>> = entries
        .into_iter()
        .map(|e| {
            let size = match e.dir {
                true => format!("{:>9}  ", "—"),
                false => format!("{:>9}  ", crate::util::human_size(e.size)),
            };
            vec![
                Span { text: size, color: dim, ..Default::default() },
                Span { text: e.name, color: None, ..Default::default() },
            ]
        })
        .collect();
    Payload::Text { lines, truncated: more, total_lines: total, outline: Vec::new() }
}

fn meta(path: &std::path::Path, _req: &Request, note: &str) -> Payload {
    let mut rows = vec![("Name".into(), crate::util::file_name(path))];
    if let Ok(md) = std::fs::metadata(path) {
        rows.push(("Size".into(), crate::util::human_size(md.len())));
        rows.push(("Modified".into(), crate::util::fmt_time(md.modified().ok(), "%Y-%m-%d %H:%M:%S")));
    }
    rows.push(("Note".into(), note.to_owned()));
    Payload::Meta { rows }
}
