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

mod image_preview;
mod text;

/// Identity of a preview: re-reading is only needed when one of these changes.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub path: PathBuf,
    pub len: u64,
    pub mtime: Option<SystemTime>,
    /// Images are decoded for a specific box size.
    pub box_size: (u32, u32),
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

/// A run of text sharing one color.
#[derive(Clone, Debug)]
pub struct Span {
    pub text: String,
    pub color: Option<[u8; 3]>,
}

#[derive(Clone, Debug)]
pub enum Payload {
    Text { lines: Vec<Vec<Span>>, truncated: bool, total_lines: usize },
    /// Raw RGBA plus its dimensions; the UI turns this into a texture.
    Image { width: u32, height: u32, rgba: Arc<Vec<u8>>, source: (u32, u32) },
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

    if crate::mime::is_image(mime) {
        if crate::mime::is_decodable_image(mime) {
            return match image_preview::render(path, req.key.box_size) {
                Ok(p) => p,
                Err(e) => Payload::Error(e),
            };
        }
        return meta(path, req, "Image preview not supported for this format");
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

fn meta(path: &std::path::Path, _req: &Request, note: &str) -> Payload {
    let mut rows = vec![("Name".into(), crate::util::file_name(path))];
    if let Ok(md) = std::fs::metadata(path) {
        rows.push(("Size".into(), crate::util::human_size(md.len())));
        rows.push(("Modified".into(), crate::util::fmt_time(md.modified().ok(), "%Y-%m-%d %H:%M:%S")));
    }
    rows.push(("Note".into(), note.to_owned()));
    Payload::Meta { rows }
}
