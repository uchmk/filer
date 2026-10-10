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
mod csv;
mod markdown;
mod external;
mod office;
mod shell_thumb;
mod svg_preview;
mod symbols;
mod text;

/// COM for a worker thread that talks to the Windows shell.
pub use shell_thumb::init_thread as init_com_thread;
/// OOXML document properties, for the spot panel.
pub use office::properties as office_properties;

/// How much of a file a deep preview reads. A content search stops at a
/// megabyte; the file it found is read further, since a match past that is
/// still worth showing once the search has pointed at the file.
pub const DEEP_BYTES: usize = 4 << 20;

/// Identity of a preview: re-reading is only needed when one of these changes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub path: PathBuf,
    pub len: u64,
    pub mtime: Option<SystemTime>,
    /// Images are decoded for a specific box size.
    pub box_size: (u32, u32),
    /// Text columns across the pane; rendered Markdown and a CSV table are laid
    /// out to fit it, so for those the width is part of what was read.
    pub cols: u16,
    /// Which picture of the file: a PDF's page, a video's second. Part of the
    /// key so that paging back to one already seen is instant, and so that two
    /// pages of the same file are never mistaken for each other.
    pub n: i64,
    /// Read further than the usual cut (`DEEP_BYTES`, `text::DEEP_MAX_LINES`):
    /// in the result of `S` / `F` a match can sit past the usual cut, and a
    /// preview that ends before it has nothing to colour or walk to.
    pub deep: bool,
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
    /// Markdown is drawn laid out, so its colored source is not on screen: the
    /// first screen of a long one goes out before any of the source is colored.
    pub markdown_rendered: bool,
    /// The command that draws this file, when one is configured for it. Copied
    /// in rather than looked up: the worker has no config, in the same way it
    /// is handed `tab_size` and the theme.
    pub preview: Option<crate::config::PreviewRule>,
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
    /// Where `m` matches in the rendered lines `from..to`: for each, the byte
    /// ranges of its spans' text laid end to end. A paragraph cut into several
    /// lines to fit the pane is searched as the one run of text it is (`a.*b`
    /// finds an `a` and a `b` on different lines), so a match is found where
    /// the source line has it, however the pane wraps it. The quote bars and
    /// list markers in front of a line are not text and never match.
    pub fn marks(&self, m: &ito_match::Matcher, from: usize, to: usize) -> Vec<Vec<std::ops::Range<usize>>> {
        let to = to.min(self.lines.len());
        let mut out = vec![Vec::new(); to.saturating_sub(from)];
        let mut at = from.min(to);
        while at > 0 && self.lines.get(at).is_some_and(|l| l.wrap) {
            at -= 1;
        }
        while at < to {
            let mut end = at + 1;
            while self.lines.get(end).is_some_and(|l| l.wrap) {
                end += 1;
            }
            // Each line's content: where it starts in the line's own text, how
            // long it is, and where it sits in the joined text.
            let mut joined = String::new();
            let mut parts = Vec::new();
            for l in &self.lines[at..end] {
                let (mut cells_in, mut skip) = (0, 0);
                for s in &l.spans {
                    if cells_in >= usize::from(l.indent) {
                        break;
                    }
                    cells_in += cells(&s.text);
                    skip += s.text.len();
                }
                let whole: String = l.spans.iter().map(|s| s.text.as_str()).collect();
                let body = &whole[skip.min(whole.len())..];
                if !parts.is_empty() && l.kind != LineKind::Code {
                    let wide = |c: Option<char>| c.is_some_and(|c| cells(c.encode_utf8(&mut [0; 4])) >= 2);
                    if !(wide(joined.chars().next_back()) && wide(body.chars().next())) {
                        joined.push(' ');
                    }
                }
                parts.push((joined.len(), skip, body.len()));
                joined.push_str(body);
            }
            for r in m.ranges(&joined) {
                for (k, &(off, skip, len)) in parts.iter().enumerate() {
                    let (s, e) = (r.start.max(off), r.end.min(off + len));
                    let i = at + k;
                    if s < e && (from..to).contains(&i) {
                        out[i - from].push(skip + s - off..skip + e - off);
                    }
                }
            }
            at = end;
        }
        out
    }

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

/// One line as the minimap needs it: where its text starts, how far it runs,
/// and what color it mostly is.
///
/// Six bytes a line, so even a file at the read limit costs tens of kilobytes.
/// The color stays `None` where syntect had nothing to say, so the theme —
/// which `config_reload` can change under a preview already on screen — still
/// gets to decide what the default is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapRow {
    /// Leading whitespace, in cells.
    pub indent: u16,
    /// Cells of text after the indent. Zero for a blank line.
    pub len: u16,
    pub color: Option<[u8; 3]>,
}

/// Squash highlighted lines into what the minimap draws. Built on the worker,
/// beside the lines themselves: walking every span of a ten-thousand-line file
/// is not something to do again on each frame.
pub fn minimap(lines: &[Vec<Span>]) -> Vec<MapRow> {
    lines
        .iter()
        .map(|spans| {
            let mut row = MapRow::default();
            let mut started = false;
            // The widest span speaks for the line, so code with a comment after
            // it still reads as code.
            let mut widest = 0usize;
            for span in spans {
                let trimmed = span.text.trim_start();
                if !started {
                    let lead = span.text.len() - trimmed.len();
                    row.indent = row.indent.saturating_add(cells(&span.text[..lead]) as u16);
                    if trimmed.is_empty() {
                        continue;
                    }
                    started = true;
                }
                let w = cells(trimmed.trim_end());
                row.len = row.len.saturating_add(cells(span.text.trim_start()) as u16);
                if w > widest {
                    widest = w;
                    row.color = span.color;
                }
            }
            row
        })
        .collect()
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
    /// Carries on the line above: the same paragraph (or code line) cut to the
    /// pane's width, so a search sees the two as one.
    pub wrap: bool,
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

/// An image's scale as the reader means it: 1.0 at its 1:1. `zoom` is points
/// per `source` pixel; `ppp` the display's physical pixels per point (Q65).
pub fn shown_scale(zoom: f32, own: f32, vector: bool, ppp: f32) -> f32 {
    match vector {
        true => zoom * own,
        false => zoom * ppp,
    }
}

/// The zoom that is an image's 1:1 (see [`shown_scale`]).
pub fn actual_zoom(own: f32, vector: bool, ppp: f32) -> f32 {
    1.0 / shown_scale(1.0, own, vector, ppp).max(f32::EPSILON)
}

/// The largest side a zoomed image is decoded or rendered at.
pub const MAX_DECODE: u32 = 4096;

/// How much of a file a payload holds.
///
/// The two always travel together -- nothing reads one without the other, and
/// the only thing either is for is the note under the last line -- so they were
/// two fields on two payload variants and two arguments on four draw functions.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Extent {
    /// Whether what is held stops short of the whole file.
    pub truncated: bool,
    /// Lines counted, before any cap on lines was applied. The whole file's
    /// count unless `cut`.
    pub total: usize,
    /// The read itself stopped at `max_text_bytes`, so `total` is the lines
    /// in what was read and the file goes on past them. Said as such: "5237
    /// lines total" for a file cut at 1 MiB was the count of the first MiB,
    /// and read as the file's (#205).
    pub cut: bool,
    /// A table that stopped at this many rows, however long the file.
    pub rows: Option<usize>,
}

impl Extent {
    /// The line under the last one shown.
    pub fn note(&self) -> String {
        let lines = match self.cut {
            true => format!("{} lines read, and the file goes on", self.total),
            false => format!("{} lines total", self.total),
        };
        match self.rows {
            Some(n) => format!("… the table stops at {n} rows; {lines}"),
            None => format!("… {lines} (truncated)"),
        }
    }

    /// The count for spot's `Lines` row: `5237+` when the read was cut.
    pub fn lines(&self) -> String {
        match self.cut {
            true => format!("{}+", self.total),
            false => self.total.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Payload {
    /// `outline` lists the declarations of source code, if its grammar marks any.
    Text {
        lines: Vec<Vec<Span>>,
        /// One entry per line, for the minimap.
        map: Vec<MapRow>,
        extent: Extent,
        outline: Vec<TocEntry>,
    },
    /// Markdown carries both views so switching between them needs no reload.
    /// `map` describes `source`, which is why the minimap is only shown for the
    /// source view: a rendered line and a source line are not the same line.
    Markdown { doc: Doc, source: Vec<Vec<Span>>, map: Vec<MapRow>, extent: Extent },
    /// Raw RGBA plus its dimensions; the UI turns this into a texture. The
    /// caption goes under it (the source size, a font's name, ...).
    ///
    /// `width` and `height` are the decode, which is only ever as big as the
    /// box asked for. `source` is the picture itself, and is what the geometry
    /// is built from: a zoom re-decodes at a higher resolution, and if the size
    /// on screen followed the texture instead, the image would jump the moment
    /// the sharper copy arrived.
    Image {
        width: u32,
        height: u32,
        source: (u32, u32),
        /// One of the file's own pixels, in `source` pixels: 1 for a raster.
        /// An SVG is laid out far larger than it says it is (see
        /// `svg_preview::vector_source`), and its scale and its 1:1 are
        /// reckoned against its own size, not that one.
        own: f32,
        /// Drawn from a vector (an SVG): its 1:1 is its own units as the
        /// display's logical pixels, as a browser shows it. A raster's 1:1 is
        /// one of its pixels to one of the screen's, the only scale at which
        /// it is sharp (Q65).
        vector: bool,
        rgba: Arc<Vec<u8>>,
        caption: String,
    },
    Binary { lines: Vec<String>, total: u64 },
    Meta { rows: Vec<(String, String)> },
    Error(String),
}

#[derive(Debug)]
pub struct Response {
    pub key: Key,
    pub payload: Payload,
    /// The first screen of a long text, sent while the rest is colored. Shown,
    /// but not cached: the whole one follows under the same key.
    pub partial: bool,
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
                let wake = std::rc::Rc::new(wake);
                let mut syntax = text::Highlighter::default();
                let mut warmed = false;
                loop {
                    if !warmed {
                        warmed = syntax.warm(&|| req_rx.is_empty());
                    }
                    let Ok(req) = req_rx.recv() else { return };
                    // Skip anything already superseded while we were busy.
                    let mut req = req;
                    while let Ok(newer) = req_rx.try_recv() {
                        req = newer;
                    }
                    let (tx, newer, key, wake_early) = (res_tx.clone(), req_rx.clone(), req.key.clone(), wake.clone());
                    syntax.early = Some(Box::new(move |payload| {
                        if let Some(payload) = payload {
                            let _ = tx.send(Response { key: key.clone(), payload, partial: true });
                            wake_early();
                        }
                        // Moving on through a folder colors only the first
                        // screen of each file it passes.
                        newer.is_empty()
                    }));
                    let payload = render(&req, &mut syntax);
                    syntax.early = None;
                    if std::mem::take(&mut syntax.abandoned) {
                        continue;
                    }
                    if res_tx.send(Response { key: req.key, payload, partial: false }).is_err() {
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

    // A configured command comes first, so that a rule for `*.pdf` is what
    // decides, not the shell's one-page thumbnail handler underneath it.
    if let Some(rule) = &req.preview {
        return external_picture(rule, req);
    }

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

    // Word, Excel and PowerPoint as their own text. Before the archive
    // branch, since these are zips and would otherwise be listed as one --
    // a table of contents full of `word/document.xml` tells nobody anything.
    if let Some(kind) = office::Kind::of(req.ext.as_deref()) {
        return match office::read(path, kind, req.max_bytes) {
            Ok(doc) => office_text(doc, req, syntax),
            // The card rather than an error: the name, size and dates are
            // still worth having for a file that cannot be read.
            Err(e) => meta(path, req, &e),
        };
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

    // Before the binary test: UTF-16 is full of NUL bytes, and a Notepad
    // "Unicode" save was hex-dumped as `binary` while spot, which asks in
    // this order, called it text (#214).
    if let Some(text) = utf16_text(&head) {
        let cut = head.len() >= req.max_bytes;
        return text::render_cut(text.as_bytes(), cut, req, syntax);
    }

    if looks_binary(&head) {
        if crate::mime::is_text(mime) {
            // Mis-guessed by extension; fall through to a hex dump anyway.
        }
        return binary(path, &head);
    }

    // Before the text fallback, and after `looks_binary` above: a binary blob
    // named `.csv` still hex-dumps, and `max_bytes` has already done its cutting.
    if mime == "text/csv" {
        let delim = csv::delimiter(req.ext.as_deref());
        let dim = syntax.theme(&req.syntect_theme).map(markdown::dim_of).unwrap_or_default();
        return csv::render(&head, delim, req.key.cols, dim, req.max_bytes);
    }
    text::render(&head, req, syntax)
}

fn read_head(path: &std::path::Path, max: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    // `crate::spot::reason`: the OS's own sentence follows its language, and
    // the spot panel's `Resolves` row already says `not found, os error 2`.
    let f = std::fs::File::open(path).map_err(|e| broken_link_reason(path).unwrap_or_else(|| crate::spot::reason(&e)))?;
    let mut buf = Vec::with_capacity(max.min(64 * 1024));
    f.take(max as u64).read_to_end(&mut buf).map_err(|e| crate::spot::reason(&e))?;
    Ok(buf)
}

/// A broken junction fails `open` with `access denied, os error 5` on Windows
/// while `Resolves` says `not found, os error 2` (#274). For a link whose
/// target cannot be reached, say what `Resolves` says: the error from asking
/// for the target's own metadata.
fn broken_link_reason(path: &std::path::Path) -> Option<String> {
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink()) || std::fs::read_link(path).is_ok();
    if !is_link {
        return None;
    }
    std::fs::metadata(path).err().map(|e| crate::spot::reason(&e))
}

/// Text behind a UTF-16 byte-order mark, as UTF-8. `None` without one.
pub(crate) fn utf16_text(bytes: &[u8]) -> Option<String> {
    let big = match bytes.first_chunk::<2>()? {
        [0xff, 0xfe] => false,
        [0xfe, 0xff] => true,
        _ => return None,
    };
    let units: Vec<u16> = bytes[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| if big { u16::from_be_bytes(*c) } else { u16::from_le_bytes(*c) })
        .collect();
    Some(String::from_utf16_lossy(&units))
}

pub(crate) fn looks_binary(bytes: &[u8]) -> bool {
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

/// An Office document's text as a text preview.
///
/// Through the same path as a plain text file, so that everything the text
/// preview can do -- scrolling, the minimap, the outline in the side column --
/// works here without knowing what it is looking at.
fn office_text(
    doc: office::Read1,
    req: &Request,
    syntax: &mut text::Highlighter,
) -> Payload {
    let _ = (req, syntax);
    let total = doc.lines.len();
    // Plain, not highlighted: there is no grammar for "the text that was in a
    // spreadsheet", and guessing one by extension would colour it as XML --
    // which is what it was stored as and not what is being shown.
    match text::plain(&doc.lines.join("\n"), office_extent(total, doc.truncated)) {
        // The extent `plain` worked out is the one to keep -- it counted the
        // lines it was actually handed.
        Payload::Text { lines, map, extent, .. } => {
            Payload::Text { lines, map, extent, outline: doc.outline }
        }
        other => other,
    }
}

/// How much of an Office document is shown. Up to the text preview's own
/// cap, so a document longer than that is truncated whether or not the
/// reader stopped; and a reader that stopped has counted only what it read
/// (16.11: a 4500-row workbook said nothing, and a longer one said `5000
/// lines total`).
fn office_extent(total: usize, stopped: bool) -> Extent {
    Extent { truncated: stopped || total > text::MAX_LINES, total, cut: stopped, rows: None }
}

/// Draw a file with the command configured for it.
///
/// The number is in the caption rather than anywhere structural: the pane
/// already knows how to show a picture with a line under it, and "page 3" is
/// the same kind of fact as an image's dimensions.
fn external_picture(rule: &crate::config::PreviewRule, req: &Request) -> Payload {
    let n = req.key.n;
    let drawn = match external::draw(rule, &req.key.path, n) {
        Ok(d) => d,
        // The end of a document arrives as a failure from the command, which
        // is the only way it can: nothing asked how many pages there were.
        // Saying which page was refused matters, because the reader pressed a
        // key and has to know it was not the key that failed.
        Err(e) => return Payload::Error(format!("{}: {e}", caption(rule, n))),
    };
    match image_preview::render(&drawn.png, req.key.box_size) {
        Ok(Payload::Image { width, height, source, own, vector, rgba, .. }) => Payload::Image {
            width,
            height,
            source,
            own,
            vector,
            rgba,
            caption: caption(rule, n),
        },
        Ok(other) => other,
        Err(e) => Payload::Error(e),
    }
}

/// What goes under the picture: `page 3`, `50s`, or just `3`.
fn caption(rule: &crate::config::PreviewRule, n: i64) -> String {
    match rule.unit.as_str() {
        "" => n.to_string(),
        u if u.contains("{n}") => u.replace("{n}", &n.to_string()),
        u => format!("{u} {n}"),
    }
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
    let map = minimap(&lines);
    Payload::Text { lines, map, extent: Extent { truncated: more, total, ..Default::default() }, outline: Vec::new() }
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

#[cfg(test)]
mod tests {
    /// #266: a file that is not there reads `not found, os error N` in the
    /// preview's own words, not in the OS's language.
    #[test]
    fn a_missing_file_is_named_in_english() {
        let dir = crate::util::test_dir("preview-missing");
        let said = super::read_head(&dir.join("gone.txt"), 4096).unwrap_err();
        assert!(said.starts_with("not found, os error ") && said.is_ascii(), "{said}");
    }

    /// #274: a broken link reads the same in the preview as in spot's `Resolves`.
    #[cfg(unix)]
    #[test]
    fn a_broken_link_is_named_like_resolves() {
        let dir = crate::util::test_dir("preview-broken-link");
        std::os::unix::fs::symlink(dir.join("gone"), dir.join("link")).unwrap();
        let said = super::read_head(&dir.join("link"), 4096).unwrap_err();
        assert!(said.starts_with("not found, os error "), "{said}");
    }

    /// What the note under an Office preview says (16.11 on the machine).
    #[test]
    fn an_office_preview_says_how_much_it_shows() {
        assert!(!super::office_extent(300, false).truncated);
        let mid = super::office_extent(4500, false);
        assert!(mid.truncated, "4500 lines are more than the 4000 shown");
        assert_eq!(mid.note(), "… 4500 lines total (truncated)");
        let long = super::office_extent(5000, true);
        assert_eq!(long.note(), "… 5000 lines read, and the file goes on (truncated)");
    }

    /// The caption reads as the thing it counts.
    ///
    /// A unit that could only go in front turned fifty seconds into `s 50`.
    /// `{n}` says where the number belongs, which is in front for a page and
    /// behind for a second.
    /// Q65: on a 150% display a raster's 1:1 is one of its pixels to one of
    /// the screen's, and an SVG's is its own units as logical pixels -- as a
    /// browser shows it. At 100% the two agree.
    #[test]
    fn one_to_one_is_physical_for_a_raster_and_logical_for_an_svg() {
        let raster = actual_zoom(1.0, false, 1.5);
        assert!((raster - 1.0 / 1.5).abs() < 1e-6, "a 100 px PNG is 66.7 points, 100 screen pixels");
        assert!((shown_scale(raster, 1.0, false, 1.5) - 1.0).abs() < 1e-6, "and says 1:1");
        let own = 40.96; // a 100-unit SVG laid out at 4096
        let svg = actual_zoom(own, true, 1.5);
        assert!((svg * 4096.0 - 100.0).abs() < 1e-3, "100 points, 150 screen pixels");
        assert!((shown_scale(svg, own, true, 1.5) - 1.0).abs() < 1e-6);
        assert_eq!(actual_zoom(1.0, false, 1.0), 1.0, "at 100% nothing changes");
    }

    /// #214: a UTF-16 file (Notepad's "Unicode" save) is previewed as text,
    /// both byte orders, not hex-dumped for its NUL bytes.
    #[test]
    fn utf16_previews_as_text() {
        let dir = crate::util::test_dir("preview-utf16");
        for (name, big) in [("le.txt", false), ("be.txt", true)] {
            let mut bytes = if big { vec![0xfe, 0xff] } else { vec![0xff, 0xfe] };
            for u in "hello\r\nworld\r\n".encode_utf16() {
                bytes.extend(if big { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            std::fs::write(dir.join(name), bytes).unwrap();
            match super::for_tests::render(&dir.join(name)) {
                Payload::Text { lines, .. } => {
                    let first: String = lines[0].iter().map(|s| s.text.as_str()).collect();
                    assert_eq!(first.trim_end(), "hello", "{name}");
                    assert_eq!(lines.len(), 2, "{name}");
                }
                other => panic!("{name}: not text: {other:?}"),
            }
        }
    }

    #[test]
    fn the_caption_puts_the_number_where_it_belongs() {
        let rule = |unit: &str| crate::config::PreviewRule {
            pattern: "*".into(),
            run: String::new(),
            first: 1,
            step: 1,
            unit: unit.into(),
        };
        assert_eq!(caption(&rule("page {n}"), 3), "page 3");
        assert_eq!(caption(&rule("{n}s"), 50), "50s");
        // No `{n}`: in front, which is what the older rules meant.
        assert_eq!(caption(&rule("page"), 3), "page 3");
        // And nothing at all is just the number.
        assert_eq!(caption(&rule(""), 7), "7");
    }

    use super::*;

    fn span(text: &str, color: Option<[u8; 3]>) -> Span {
        Span { text: text.into(), color, ..Default::default() }
    }

    #[test]
    fn a_minimap_row_records_the_indent_and_the_width() {
        let rows = minimap(&[
            vec![span("    let x = 1;", None)],
            vec![span("", None)],
            vec![span("fn main() {", None)],
        ]);

        assert_eq!(rows[0], MapRow { indent: 4, len: 10, color: None });
        assert_eq!(rows[1], MapRow::default(), "a blank line leaves a gap");
        assert_eq!(rows[2].indent, 0);
    }

    /// The widest span decides the color, so a line of code with a comment
    /// after it still reads as code rather than as a comment.
    #[test]
    fn the_widest_span_gives_the_row_its_color() {
        const CODE: [u8; 3] = [0x80, 0xc0, 0xff];
        const NOTE: [u8; 3] = [0x60, 0x60, 0x60];

        let long_code = minimap(&[vec![
            span("let answer = compute();", Some(CODE)),
            span(" // why", Some(NOTE)),
        ]]);
        assert_eq!(long_code[0].color, Some(CODE));

        let all_comment = minimap(&[vec![span("x;", Some(CODE)), span(" // a long explanation", Some(NOTE))]]);
        assert_eq!(all_comment[0].color, Some(NOTE));
    }

    /// The indent is counted across spans: a highlighter is free to hand back
    /// the leading whitespace on its own.
    #[test]
    fn an_indent_split_across_spans_still_counts() {
        let rows = minimap(&[vec![span("  ", None), span("  ", None), span("x", None)]]);

        assert_eq!(rows[0].indent, 4);
        assert_eq!(rows[0].len, 1);
    }

    #[test]
    fn a_wide_character_counts_as_two_cells() {
        let rows = minimap(&[vec![span("日本語", None)]]);

        assert_eq!(rows[0].len, 6);
    }
}

#[cfg(test)]
pub(crate) mod for_tests {
    /// What the preview worker would answer for `path`, run on this thread.
    pub(crate) fn render(path: &std::path::Path) -> super::Payload {
        let entry = crate::fs::Entry::from_path(path.to_path_buf()).unwrap();
        let req = super::Request {
            id: 0,
            key: super::Key {
                path: entry.path.clone(),
                len: entry.len,
                mtime: None,
                box_size: (800, 600),
                cols: 80,
                n: 0,
                deep: false,
            },
            mime: crate::mime::guess(&entry),
            ext: entry.ext.clone(),
            max_bytes: 1 << 20,
            tab_size: 4,
            syntect_theme: "base16-ocean.dark".into(),
            markdown_rendered: true,
            preview: None,
        };
        super::render(&req, &mut super::text::Highlighter::default())
    }
}
