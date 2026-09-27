//! What the spot panel (`<Tab>`) says about a file.
//!
//! The panel is a list of [`Section`]s. [`base`] comes straight from the
//! listing, so it shows at once; [`inspect`] reads the file on the spot worker
//! and runs each provider in turn. A new kind of detail is one more provider:
//! a function returning `Option<Section>`, added to the list in [`inspect`].

use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, Sender};

use crate::fs::{Entry, Kind};
use crate::util;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

impl Section {
    fn new(title: &str) -> Self {
        Self { title: title.into(), rows: Vec::new() }
    }

    fn row(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.rows.push((key.into(), value));
        }
    }
}

const TIME: &str = "%Y-%m-%d %H:%M:%S";

/// What the listing already knows: no I/O.
pub fn base(entry: &Entry) -> Section {
    let mut s = Section::new("File");
    s.row("Name", entry.name.clone());
    s.row("Path", entry.path.display().to_string());
    s.row("Kind", match entry.kind {
        Kind::Dir => "Directory",
        Kind::File => "File",
        Kind::Link { broken: true, .. } => "Link (broken)",
        Kind::Link { to_dir: true, .. } => "Link to a directory",
        Kind::Link { .. } => "Link to a file",
    });
    s.row("Mime", crate::mime::guess(entry));
    if entry.is_dir_like() {
        if let Some(n) = entry.dir_size {
            s.row("Items", n.to_string());
        }
    } else {
        s.row("Size", format!("{} ({} bytes)", util::human_size(entry.len), grouped(entry.len)));
    }
    s.row("Created", util::fmt_time(entry.created, TIME));
    s.row("Modified", util::fmt_time(entry.modified, TIME));
    s.row("Accessed", util::fmt_time(entry.accessed, TIME));
    let attrs: Vec<&str> = [(entry.hidden, "hidden"), (entry.readonly, "read-only")]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
    s.row("Attributes", if attrs.is_empty() { "normal".into() } else { attrs.join(", ") });
    s
}

/// Details that take reading the file. Runs on the spot worker.
pub fn inspect(path: &Path) -> Vec<Section> {
    let providers: &[fn(&Path) -> Option<Section>] = &[
        // `link` is about the path rather than the bytes, so it stays first.
        // Then the three that answer "what is this" -- a package, a document, a
        // program -- because that is what the panel was opened for, and because
        // `Act::Copy` counts rows across every section, so the rows worth
        // copying belong near the top. `text` follows them: it has the most rows
        // and the least identity. The last three keep their old relative order.
        link,
        archive,
        document,
        executable,
        text,
        image,
        font,
        directory,
        // Windows property-system values (media length, bitrate, EXIF, ...)
        // go here as one more provider reading `SHGetPropertyStoreFromParsingName`;
        // the worker thread has COM initialized for it.
    ];
    providers.iter().filter_map(|p| p(path)).collect()
}

fn link(path: &Path) -> Option<Section> {
    let target = std::fs::read_link(path).ok()?;
    let mut s = Section::new("Link");
    s.row("Target", target.display().to_string());
    s.row("Resolves", match std::fs::canonicalize(path) {
        Ok(real) => plain(&real),
        Err(e) => format!("no ({e})"),
    });
    Some(s)
}

/// The image's own size and format, read from its header.
fn image(path: &Path) -> Option<Section> {
    use image::ImageDecoder;
    if path.is_dir() {
        return None;
    }
    let reader = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let format = reader.format()?;
    let decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    let mut s = Section::new("Image");
    s.row("Dimensions", format!("{w} × {h}"));
    s.row("Format", format.extensions_str().first().map_or(String::new(), |x| x.to_uppercase()));
    s.row("Color", format!("{:?}", decoder.color_type()));
    Some(s)
}

/// Entries read for the summary. The panel keeps no names, so this is a budget
/// on time, not on memory: a monstrous archive must not hold the worker.
const MAX_ARCHIVE_ENTRIES: usize = 20_000;

fn archive(path: &Path) -> Option<Section> {
    use crate::fs::archive::{self, Encryption};
    let sum = archive::summarize(path, MAX_ARCHIVE_ENTRIES).ok()?;
    let mut s = Section::new("Archive");
    s.row("Format", sum.format.label());
    s.row("Encrypted", match sum.encryption {
        Encryption::No => "no",
        Encryption::Entries => "yes (entries need a password)",
        Encryption::Header => "yes (the listing itself)",
    });
    // Nothing below is known when the table of contents is sealed, and a total
    // taken from a scan that stopped early would read as the whole of it.
    if sum.encryption != Encryption::Header {
        s.row("Entries", format!("{} files, {} folders", grouped(sum.files), grouped(sum.dirs)));
        if sum.bytes > 0 {
            s.row("Unpacked", format!("{} ({} bytes)", util::human_size(sum.bytes), grouped(sum.bytes)));
            if !sum.more {
                if let Ok(m) = std::fs::metadata(path) {
                    s.row("Ratio", format!("{}% of unpacked", m.len() * 100 / sum.bytes));
                }
            }
        }
    }
    if sum.more {
        s.row("Scanned", format!("first {} entries", grouped(MAX_ARCHIVE_ENTRIES as u64)));
    }
    Some(s)
}

/// How much of a file the text details are read from. A 2 GB log still gets its
/// line endings answered; the `Scanned` row says the answer is partial.
const MAX_TEXT_SCAN: u64 = 1 << 20;
/// Sniffed first, so nothing more than this is read for a JPEG.
const TEXT_SNIFF_BYTES: usize = 8 << 10;

/// UTF-16 has to be recognised before the binary test, not after: it is full of
/// NUL bytes, so `looks_binary` calls it binary and the encoding row could
/// never say UTF-16 at all.
fn text(path: &Path) -> Option<Section> {
    use std::io::Read;
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() || meta.len() == 0 {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = Vec::new();
    f.by_ref().take(TEXT_SNIFF_BYTES as u64).read_to_end(&mut head).ok()?;
    if head.is_empty() {
        return None;
    }
    let (bom, wide) = match head.first_chunk::<3>() {
        Some([0xef, 0xbb, 0xbf]) => ("UTF-8 (EF BB BF)", None),
        _ => match head.first_chunk::<2>() {
            Some([0xff, 0xfe]) => ("UTF-16 LE (FF FE)", Some(false)),
            Some([0xfe, 0xff]) => ("UTF-16 BE (FE FF)", Some(true)),
            _ => ("none", None),
        },
    };
    if wide.is_none() && crate::preview::looks_binary(&head) {
        return None;
    }
    // Read the rest of the budget, the sniff included.
    let mut bytes = head;
    if meta.len() > bytes.len() as u64 {
        let want = MAX_TEXT_SCAN.saturating_sub(bytes.len() as u64);
        let mut rest = Vec::new();
        f.take(want).read_to_end(&mut rest).ok()?;
        bytes.extend_from_slice(&rest);
    }
    let partial = (bytes.len() as u64) < meta.len();
    let (body, encoding) = match wide {
        Some(big) => {
            let units: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| if big { u16::from_be_bytes(*c) } else { u16::from_le_bytes(*c) })
                .collect();
            (String::from_utf16_lossy(&units), if big { "UTF-16 BE" } else { "UTF-16 LE" })
        }
        None => {
            let start = if bom.starts_with("UTF-8") { 3 } else { 0 };
            match std::str::from_utf8(&bytes[start..]) {
                Ok(t) => (t.to_owned(), "UTF-8"),
                // Lossy rather than nothing: the line endings and the longest
                // line are still worth having from a file with a bad byte in it.
                Err(_) => (String::from_utf8_lossy(&bytes[start..]).into_owned(), "UTF-8 (invalid bytes)"),
            }
        }
    };

    let crlf = body.matches("\r\n").count() as u64;
    let lf = body.matches('\n').count() as u64 - crlf;
    let cr = body.matches('\r').count() as u64 - crlf;
    let mut s = Section::new("Text");
    s.row("Encoding", encoding);
    s.row("BOM", bom);
    s.row("Line endings", match (lf, crlf, cr) {
        (0, 0, 0) => "none".to_string(),
        (n, 0, 0) => format!("LF ({})", grouped(n)),
        (0, n, 0) => format!("CRLF ({})", grouped(n)),
        (0, 0, n) => format!("CR ({})", grouped(n)),
        (a, b, c) => {
            let mut parts = Vec::new();
            if a > 0 { parts.push(format!("LF {}", grouped(a))); }
            if b > 0 { parts.push(format!("CRLF {}", grouped(b))); }
            if c > 0 { parts.push(format!("CR {}", grouped(c))); }
            format!("mixed: {}", parts.join(", "))
        }
    });
    // A claim about the end of the file, so only where the end was read.
    if !partial {
        s.row("Final newline", if body.ends_with('\n') { "yes" } else { "no" });
    }
    if let Some((n, chars)) = body
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.chars().count()))
        .max_by_key(|&(_, chars)| chars)
    {
        s.row("Longest line", format!("{} chars (line {})", grouped(chars as u64), grouped(n as u64)));
    }
    let widths: Vec<u16> = body
        .lines()
        .filter_map(|l| {
            let spaces = l.len() - l.trim_start_matches(' ').len();
            (spaces > 0 && !l.trim().is_empty()).then_some(spaces as u16)
        })
        .collect();
    let tabs = body.lines().filter(|l| l.starts_with('\t')).count();
    s.row("Indentation", match (widths.is_empty(), tabs) {
        (true, 0) => String::new(),
        (true, _) => "tabs".to_string(),
        (false, 0) => match indent_step(&widths) {
            Some(w) => format!("spaces, {w}"),
            None => "spaces".to_string(),
        },
        (false, t) => format!("mixed: spaces {} lines, tabs {}", grouped(widths.len() as u64), grouped(t as u64)),
    });
    if partial {
        s.row("Scanned", format!("first {} of {}", util::human_size(bytes.len() as u64), util::human_size(meta.len())));
    }
    Some(s)
}

/// The indent unit, as the greatest common divisor of the widths seen: 4 out of
/// a file indented 4/8/12, 2 out of 2/4/6. `None` when they share nothing,
/// which is the honest answer for ragged indentation.
fn indent_step(widths: &[u16]) -> Option<u16> {
    fn gcd(a: u16, b: u16) -> u16 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let step = widths.iter().copied().reduce(gcd)?;
    (2..=8).contains(&step).then_some(step)
}

/// An OOXML document's own property sheet.
///
/// `.doc` / `.xls` / `.ppt` need no special case: `Kind::of` declines the
/// pre-2007 formats, which are neither zip nor XML.
fn document(path: &Path) -> Option<Section> {
    let rows = crate::preview::office_properties(path);
    if rows.is_empty() {
        return None;
    }
    let mut s = Section::new("Document");
    for (k, v) in rows {
        s.row(k, v);
    }
    Some(s)
}

/// The front of the file: a DOS stub's `e_lfanew`, an ELF ident, a Mach-O header.
const HEADER_BYTES: u64 = 0x40;
/// Read at `e_lfanew`: the signature, the COFF header, and enough of the
/// optional header to reach the subsystem at its offset 68.
const PE_HEADER_BYTES: u64 = 0x80;
/// A sane `e_lfanew`. Past this the field is noise, not an offset.
const MAX_LFANEW: u64 = 1 << 20;
/// Slices of a universal binary. Also the test that tells one from a Java class
/// file, which shares its magic -- see `executable`.
const MAX_FAT_SLICES: u32 = 16;

/// The format, architecture and kind of a program, read from its own header.
///
/// Nothing here is gated on the host: a PE is parsed on Linux and an ELF on
/// Windows, which is the point -- this is for checking what a cross build
/// actually produced. Architectures are named as Rust names its targets, so the
/// answer can be held against the triple the artifact was built for, and an
/// unrecognised value is shown as the number rather than guessed at.
fn executable(path: &Path) -> Option<Section> {
    let head = read_at(path, 0, HEADER_BYTES)?;
    let magic = u32::from_le_bytes(*head.first_chunk::<4>()?);
    let mut s = Section::new("Executable");
    match () {
        _ if head.starts_with(b"MZ") => {
            let lfanew = u32::from_le_bytes(*head.get(0x3c..0x40)?.first_chunk::<4>()?) as u64;
            if lfanew == 0 || lfanew > MAX_LFANEW {
                return None;
            }
            let pe = read_at(path, lfanew, PE_HEADER_BYTES)?;
            if !pe.starts_with(b"PE\0\0") {
                return None; // a DOS stub with no PE header behind it
            }
            let machine = le16(&pe, 4)?;
            let characteristics = le16(&pe, 22)?;
            let opt = le16(&pe, 24)?;
            s.row("Format", match opt {
                0x10b => "PE32".to_string(),
                0x20b => "PE32+".to_string(),
                other => format!("PE (optional header {other:#x})"),
            });
            s.row("Architecture", coff_machine(machine));
            s.row("Kind", if characteristics & 0x2000 != 0 { "DLL" } else { "executable" });
            // The subsystem sits at offset 68 of the optional header in both
            // PE32 and PE32+: the two differ only in the image-base field and
            // line up again well before this one.
            if let Some(sub) = le16(&pe, 92) {
                s.row("Subsystem", match sub {
                    1 => "native".to_string(),
                    2 => "Windows GUI".to_string(),
                    3 => "console".to_string(),
                    9 => "Windows CE".to_string(),
                    10 => "EFI application".to_string(),
                    other => format!("{other} (unknown)"),
                });
            }
        }
        _ if head.starts_with(b"\x7fELF") => {
            let class = *head.get(4)?;
            let big = *head.get(5)? == 2;
            let rd = |at: usize| -> Option<u16> {
                let c = head.get(at..at + 2)?.first_chunk::<2>()?;
                Some(if big { u16::from_be_bytes(*c) } else { u16::from_le_bytes(*c) })
            };
            s.row("Format", match class {
                1 => "ELF 32-bit".to_string(),
                2 => "ELF 64-bit".to_string(),
                other => format!("ELF (class {other})"),
            });
            s.row("Architecture", elf_machine(rd(18)?));
            s.row("Byte order", if big { "big" } else { "little" });
            s.row("Kind", match rd(16)? {
                1 => "object file".to_string(),
                2 => "executable".to_string(),
                3 => "shared object / PIE".to_string(),
                4 => "core dump".to_string(),
                other => format!("{other} (unknown)"),
            });
        }
        // Both byte orders of both widths. The thin header is native-endian, so
        // the reversed magics are the same file seen from the other side.
        _ if matches!(magic, 0xfeed_face | 0xfeed_facf | 0xcefa_edfe | 0xcffa_edfe) => {
            let wide = matches!(magic, 0xfeed_facf | 0xcffa_edfe);
            let swapped = matches!(magic, 0xcefa_edfe | 0xcffa_edfe);
            let rd = |at: usize| -> Option<u32> {
                let c = head.get(at..at + 4)?.first_chunk::<4>()?;
                Some(if swapped { u32::from_be_bytes(*c) } else { u32::from_le_bytes(*c) })
            };
            s.row("Format", if wide { "Mach-O 64-bit" } else { "Mach-O 32-bit" });
            s.row("Architecture", mach_cpu(rd(4)?));
            s.row("Kind", match rd(12)? {
                2 => "executable".to_string(),
                6 => "dynamic library".to_string(),
                8 => "bundle".to_string(),
                other => format!("{other} (unknown)"),
            });
        }
        // `CAFEBABE` is also the magic of a Java class file. The word after it
        // is a universal binary's slice count -- small -- where a class file has
        // its version there, which is 45 or more. That is the whole test.
        _ if magic.swap_bytes() == 0xcafe_babe || magic.swap_bytes() == 0xcafe_babf => {
            let wide = magic.swap_bytes() == 0xcafe_babf;
            let n = u32::from_be_bytes(*head.get(4..8)?.first_chunk::<4>()?);
            if n == 0 || n > MAX_FAT_SLICES {
                return None;
            }
            let each = if wide { 32 } else { 20 };
            let all = read_at(path, 0, 8 + u64::from(n) * each)?;
            let mut names = Vec::new();
            for i in 0..n as usize {
                let at = 8 + i * each as usize;
                let cpu = u32::from_be_bytes(*all.get(at..at + 4)?.first_chunk::<4>()?);
                names.push(mach_cpu(cpu));
            }
            s.row("Format", "Mach-O universal");
            s.row("Slices", names.join(", "));
        }
        _ => return None,
    }
    Some(s)
}

/// `max` bytes from `at`, or `None` if the file is shorter or unreadable.
fn read_at(path: &Path, at: u64, max: u64) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    if at > 0 {
        f.seek(SeekFrom::Start(at)).ok()?;
    }
    let mut buf = Vec::new();
    f.take(max).read_to_end(&mut buf).ok()?;
    (!buf.is_empty()).then_some(buf)
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(*b.get(at..at + 2)?.first_chunk::<2>()?))
}

fn coff_machine(m: u16) -> String {
    match m {
        0x8664 => "x86_64".to_string(),
        0xaa64 => "aarch64".to_string(),
        0x14c => "i686".to_string(),
        0x1c0 | 0x1c4 => "arm".to_string(),
        0x200 => "ia64".to_string(),
        other => format!("{other:#x} (unknown)"),
    }
}

fn elf_machine(m: u16) -> String {
    match m {
        0x3e => "x86_64".to_string(),
        0xb7 => "aarch64".to_string(),
        0x03 => "i686".to_string(),
        0x28 => "arm".to_string(),
        0xf3 => "riscv64".to_string(),
        0x15 => "powerpc64".to_string(),
        0x16 => "s390x".to_string(),
        other => format!("{other:#x} (unknown)"),
    }
}

fn mach_cpu(c: u32) -> String {
    match c {
        0x0100_0007 => "x86_64".to_string(),
        0x0100_000c => "aarch64".to_string(),
        7 => "i686".to_string(),
        12 => "arm".to_string(),
        other => format!("{other:#x} (unknown)"),
    }
}

const MAX_FONT_BYTES: u64 = 64 << 20;

fn font(path: &Path) -> Option<Section> {
    let ext = util::extension(&util::file_name(path))?.to_ascii_lowercase();
    if !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
        return None;
    }
    if std::fs::metadata(path).ok()?.len() > MAX_FONT_BYTES {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    use ttf_parser::name_id::{FAMILY, FULL_NAME, SUBFAMILY, TYPOGRAPHIC_FAMILY, TYPOGRAPHIC_SUBFAMILY, VERSION};
    let mut s = Section::new("Font");
    s.row("Family", font_name(&face, TYPOGRAPHIC_FAMILY).or_else(|| font_name(&face, FAMILY)).unwrap_or_default());
    s.row("Style", font_name(&face, TYPOGRAPHIC_SUBFAMILY).or_else(|| font_name(&face, SUBFAMILY)).unwrap_or_default());
    s.row("Full name", font_name(&face, FULL_NAME).unwrap_or_default());
    s.row("Version", font_name(&face, VERSION).unwrap_or_default());
    s.row("Weight", face.weight().to_number().to_string());
    s.row("Monospaced", if face.is_monospaced() { "yes" } else { "no" });
    s.row("Glyphs", face.number_of_glyphs().to_string());
    if let Some(n) = ttf_parser::fonts_in_collection(&data) {
        s.row("Faces", n.to_string());
    }
    Some(s)
}

/// The English name if there is one, else any readable one.
fn font_name(face: &ttf_parser::Face, id: u16) -> Option<String> {
    let mut any = None;
    for n in face.names() {
        if n.name_id != id || !n.is_unicode() {
            continue;
        }
        let Some(s) = n.to_string().filter(|s| !s.trim().is_empty()) else { continue };
        if n.language_id == 0x0409 {
            return Some(s);
        }
        any.get_or_insert(s);
    }
    any
}

fn directory(path: &Path) -> Option<Section> {
    if !path.is_dir() {
        return None;
    }
    let (mut files, mut dirs, mut bytes) = (0u64, 0u64, 0u64);
    for e in std::fs::read_dir(path).ok()?.flatten() {
        match e.file_type() {
            Ok(t) if t.is_dir() => dirs += 1,
            _ => {
                files += 1;
                bytes += e.metadata().map_or(0, |m| m.len());
            }
        }
    }
    let mut s = Section::new("Directory");
    s.rows.push(("Files".into(), grouped(files)));
    s.rows.push(("Directories".into(), grouped(dirs)));
    s.row("Files' size", format!("{} ({} bytes)", util::human_size(bytes), grouped(bytes)));
    Some(s)
}

/// `1234567` as `1,234,567`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `canonicalize` hands out `\\?\` paths; show them as Explorer would.
fn plain(p: &Path) -> String {
    util::unverbatim(p).display().to_string()
}

// ------------------------------------------------------------------- worker

pub struct Response {
    pub path: PathBuf,
    pub sections: Vec<Section>,
}

/// Runs [`inspect`] off the UI thread, newest request first: a slow file (a
/// big directory, a network drive) never holds the panel up.
pub struct Spotter {
    tx: Sender<PathBuf>,
    pub rx: Receiver<Response>,
}

impl Spotter {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, req_rx) = crossbeam_channel::unbounded::<PathBuf>();
        let (res_tx, rx) = crossbeam_channel::unbounded::<Response>();
        std::thread::Builder::new()
            .name("spot".into())
            .spawn(move || {
                crate::preview::init_com_thread();
                while let Ok(mut path) = req_rx.recv() {
                    while let Ok(newer) = req_rx.try_recv() {
                        path = newer;
                    }
                    let sections = inspect(&path);
                    if res_tx.send(Response { path, sections }).is_err() {
                        return;
                    }
                    wake();
                }
            })
            .expect("spawn spot worker");
        Self { tx, rx }
    }

    pub fn request(&self, path: PathBuf) {
        let _ = self.tx.send(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("filer-spot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn value<'a>(s: &'a Section, key: &str) -> Option<&'a str> {
        s.rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// A little PE / ELF / Mach-O front, built by hand. Nothing is read off the
    /// machine, so the answers are the same on all six targets.
    fn pe(machine: u16, opt: u16, characteristics: u16, subsystem: u16) -> Vec<u8> {
        let mut v = vec![0u8; 0x40];
        v[0] = b'M';
        v[1] = b'Z';
        v[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        let mut pe = vec![0u8; 0x80];
        pe[0..4].copy_from_slice(b"PE\0\0");
        pe[4..6].copy_from_slice(&machine.to_le_bytes());
        pe[22..24].copy_from_slice(&characteristics.to_le_bytes());
        pe[24..26].copy_from_slice(&opt.to_le_bytes());
        pe[92..94].copy_from_slice(&subsystem.to_le_bytes());
        v.extend_from_slice(&pe);
        v
    }

    fn elf(class: u8, big: bool, machine: u16, etype: u16) -> Vec<u8> {
        let mut v = vec![0u8; 0x40];
        v[0..4].copy_from_slice(b"\x7fELF");
        v[4] = class;
        v[5] = if big { 2 } else { 1 };
        let (t, m) = if big { (etype.to_be_bytes(), machine.to_be_bytes()) } else { (etype.to_le_bytes(), machine.to_le_bytes()) };
        v[16..18].copy_from_slice(&t);
        v[18..20].copy_from_slice(&m);
        v
    }

    fn spotted(dir: &Path, name: &str, bytes: &[u8]) -> Vec<Section> {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        inspect(&p)
    }

    fn section<'a>(all: &'a [Section], title: &str) -> Option<&'a Section> {
        all.iter().find(|s| s.title == title)
    }

    #[test]
    fn an_archive_is_counted_without_being_unpacked() {
        let dir = temp_dir("archive");
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"aaa").unwrap();
        std::fs::write(src.join("b.txt"), b"bbbb").unwrap();
        let zip = dir.join("out.zip");
        crate::fs::archive::compress(
            std::slice::from_ref(&src),
            &dir,
            &zip,
            crate::fs::archive::Format::Zip,
            &mut |_, _| true,
        )
        .unwrap();
        let all = inspect(&zip);
        let s = section(&all, "Archive").expect("an archive gets a section: {all:?}");
        assert_eq!(value(s, "Format"), Some("zip"));
        assert_eq!(value(s, "Encrypted"), Some("no"));
        assert_eq!(value(s, "Entries"), Some("2 files, 1 folders"));
        assert!(value(s, "Unpacked").unwrap().contains("7 bytes"), "{s:?}");
        // A plain file is not an archive, whatever else it may be.
        assert!(section(&inspect(&dir.join("src/a.txt")), "Archive").is_none());
    }

    #[test]
    fn line_endings_and_the_bom_are_reported() {
        let dir = temp_dir("text");
        let lf = spotted(&dir, "lf.txt", b"a\nb\nc\n");
        assert_eq!(value(section(&lf, "Text").unwrap(), "Line endings"), Some("LF (3)"));
        assert_eq!(value(section(&lf, "Text").unwrap(), "BOM"), Some("none"));
        assert_eq!(value(section(&lf, "Text").unwrap(), "Final newline"), Some("yes"));

        let crlf = spotted(&dir, "crlf.txt", b"a\r\nb\r\n");
        assert_eq!(value(section(&crlf, "Text").unwrap(), "Line endings"), Some("CRLF (2)"));

        let mixed = spotted(&dir, "mixed.txt", b"a\r\nb\nc\n");
        let v = value(section(&mixed, "Text").unwrap(), "Line endings").unwrap();
        assert!(v.starts_with("mixed:") && v.contains("LF 2") && v.contains("CRLF 1"), "{v}");

        let bom = spotted(&dir, "bom.txt", "\u{feff}hi\n".as_bytes());
        let s = section(&bom, "Text").unwrap();
        assert_eq!(value(s, "BOM"), Some("UTF-8 (EF BB BF)"));
        assert_eq!(value(s, "Encoding"), Some("UTF-8"));

        let no_nl = spotted(&dir, "nonl.txt", b"tail");
        assert_eq!(value(section(&no_nl, "Text").unwrap(), "Final newline"), Some("no"));
    }

    /// The ordering regression: UTF-16 is full of NUL bytes, so a binary test
    /// run before the BOM test would make this section unreachable forever.
    #[test]
    fn utf16_is_text_even_though_it_is_full_of_nul_bytes() {
        let dir = temp_dir("utf16");
        let mut le = vec![0xff, 0xfe];
        for u in "hi\n".encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        let s = spotted(&dir, "le.txt", &le);
        let s = section(&s, "Text").expect("UTF-16 LE is text");
        assert_eq!(value(s, "Encoding"), Some("UTF-16 LE"));
        assert_eq!(value(s, "BOM"), Some("UTF-16 LE (FF FE)"));
        assert_eq!(value(s, "Line endings"), Some("LF (1)"));

        let mut be = vec![0xfe, 0xff];
        for u in "hi\n".encode_utf16() {
            be.extend_from_slice(&u.to_be_bytes());
        }
        let s = spotted(&dir, "be.txt", &be);
        assert_eq!(value(section(&s, "Text").unwrap(), "Encoding"), Some("UTF-16 BE"));
    }

    #[test]
    fn a_binary_file_has_no_text_section() {
        let dir = temp_dir("binary");
        let all = spotted(&dir, "blob.bin", &[0x00, 0x01, 0x02, 0xff, 0xfd, 0x00, 0x7f]);
        assert!(section(&all, "Text").is_none(), "{all:?}");
    }

    #[test]
    fn the_longest_line_and_the_indent_step_are_found() {
        let dir = temp_dir("indent");
        let all = spotted(&dir, "src.rs", b"a\n    four\n        eight\n            twelve!!\n");
        let s = section(&all, "Text").unwrap();
        assert_eq!(value(s, "Indentation"), Some("spaces, 4"));
        assert_eq!(value(s, "Longest line"), Some("20 chars (line 4)"));

        let all = spotted(&dir, "tabs.rs", b"a\n\tone\n\ttwo\n");
        assert_eq!(value(section(&all, "Text").unwrap(), "Indentation"), Some("tabs"));

        assert_eq!(indent_step(&[4, 8, 12]), Some(4));
        assert_eq!(indent_step(&[2, 4, 6]), Some(2));
        assert_eq!(indent_step(&[3, 5]), None, "sharing only 1 is ragged, not a step");
    }

    #[test]
    fn a_pe_is_named_by_its_coff_machine() {
        let dir = temp_dir("pe");
        let all = spotted(&dir, "x64.exe", &pe(0x8664, 0x20b, 0, 3));
        let s = section(&all, "Executable").expect("{all:?}");
        assert_eq!(value(s, "Format"), Some("PE32+"));
        assert_eq!(value(s, "Architecture"), Some("x86_64"));
        assert_eq!(value(s, "Kind"), Some("executable"));
        assert_eq!(value(s, "Subsystem"), Some("console"));

        let all = spotted(&dir, "arm.dll", &pe(0xaa64, 0x20b, 0x2000, 2));
        let s = section(&all, "Executable").unwrap();
        assert_eq!(value(s, "Architecture"), Some("aarch64"));
        assert_eq!(value(s, "Kind"), Some("DLL"));
        assert_eq!(value(s, "Subsystem"), Some("Windows GUI"));

        // An unknown machine shows the number rather than a guess.
        let all = spotted(&dir, "odd.exe", &pe(0x1234, 0x10b, 0, 3));
        let s = section(&all, "Executable").unwrap();
        assert_eq!(value(s, "Format"), Some("PE32"));
        assert_eq!(value(s, "Architecture"), Some("0x1234 (unknown)"));

        // `MZ` with nothing behind it is not one of these.
        let mut stub = vec![0u8; 0x40];
        stub[0] = b'M';
        stub[1] = b'Z';
        assert!(section(&spotted(&dir, "stub.exe", &stub), "Executable").is_none());
    }

    #[test]
    fn an_elf_is_named_by_its_machine_and_its_byte_order() {
        let dir = temp_dir("elf");
        let all = spotted(&dir, "a.out", &elf(2, false, 0x3e, 2));
        let s = section(&all, "Executable").unwrap();
        assert_eq!(value(s, "Format"), Some("ELF 64-bit"));
        assert_eq!(value(s, "Architecture"), Some("x86_64"));
        assert_eq!(value(s, "Byte order"), Some("little"));
        assert_eq!(value(s, "Kind"), Some("executable"));

        let all = spotted(&dir, "arm.so", &elf(2, false, 0xb7, 3));
        let s = section(&all, "Executable").unwrap();
        assert_eq!(value(s, "Architecture"), Some("aarch64"));
        assert_eq!(value(s, "Kind"), Some("shared object / PIE"));

        // Byte 5 is honoured rather than assumed: read as little-endian, this
        // machine would come out as `0x3e00`.
        let all = spotted(&dir, "big.elf", &elf(2, true, 0x15, 2));
        let s = section(&all, "Executable").unwrap();
        assert_eq!(value(s, "Byte order"), Some("big"));
        assert_eq!(value(s, "Architecture"), Some("powerpc64"));
    }

    #[test]
    fn a_universal_binary_lists_its_slices_and_a_java_class_does_not() {
        let dir = temp_dir("fat");
        let mut fat = Vec::new();
        fat.extend_from_slice(&0xcafe_babeu32.to_be_bytes());
        fat.extend_from_slice(&2u32.to_be_bytes());
        for cpu in [0x0100_0007u32, 0x0100_000c] {
            fat.extend_from_slice(&cpu.to_be_bytes());
            fat.extend_from_slice(&[0u8; 16]); // cpusubtype, offset, size, align
        }
        let all = spotted(&dir, "universal", &fat);
        let s = section(&all, "Executable").expect("{all:?}");
        assert_eq!(value(s, "Format"), Some("Mach-O universal"));
        assert_eq!(value(s, "Slices"), Some("x86_64, aarch64"));

        // The same magic, but the word after it is a class file's version (52),
        // not a slice count. This is the whole test between the two.
        let mut class = Vec::new();
        class.extend_from_slice(&0xcafe_babeu32.to_be_bytes());
        class.extend_from_slice(&52u32.to_be_bytes());
        class.extend_from_slice(&[0u8; 32]);
        assert!(section(&spotted(&dir, "T.class", &class), "Executable").is_none());
    }

    /// The one end-to-end check: whatever built the test binary, its own header
    /// has to read back as the platform it was built for.
    #[test]
    fn the_test_binary_describes_itself() {
        let me = std::env::current_exe().unwrap();
        let s = executable(&me).expect("the test binary is an executable");
        let format = value(&s, "Format").unwrap();
        let want = if cfg!(windows) {
            "PE"
        } else if cfg!(target_os = "macos") {
            "Mach-O"
        } else {
            "ELF"
        };
        assert!(format.starts_with(want), "{format} should be a {want}");
        assert!(value(&s, "Architecture").is_some());
    }

    #[test]
    fn a_document_names_its_author_and_an_old_one_says_nothing() {
        let dir = temp_dir("office");
        let core = r#"<?xml version="1.0"?><cp:coreProperties>
<dc:title/><dc:creator>Ada &amp; Co</dc:creator><cp:revision>12</cp:revision>
<dcterms:created>2026-08-14T09:12:00Z</dcterms:created></cp:coreProperties>"#;
        let app = r#"<Properties><Application>Microsoft Office Word</Application>
<AppVersion>16.0000</AppVersion><Words>4231</Words></Properties>"#;
        let docx = dir.join("a.docx");
        let f = std::fs::File::create(&docx).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opt: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (name, body) in [("docProps/core.xml", core), ("docProps/app.xml", app)] {
            z.start_file(name, opt).unwrap();
            std::io::Write::write_all(&mut z, body.as_bytes()).unwrap();
        }
        z.finish().unwrap();

        let all = inspect(&docx);
        let s = section(&all, "Document").expect("{all:?}");
        assert_eq!(value(s, "Author"), Some("Ada & Co"), "the five entities are decoded");
        assert_eq!(value(s, "Revision"), Some("12"));
        assert_eq!(value(s, "Created"), Some("2026-08-14 09:12:00 UTC"), "T and Z rewritten");
        assert_eq!(value(s, "Application"), Some("Microsoft Office Word 16.0000"));
        assert_eq!(value(s, "Words"), Some("4231"));
        // `<dc:title/>` is empty, and reading past it would have swallowed the
        // rest of the part -- the author would have come out as the whole file.
        assert_eq!(value(s, "Title"), None);

        // The pre-2007 formats decline themselves, with no error.
        std::fs::write(dir.join("old.doc"), b"\xd0\xcf\x11\xe0nonsense").unwrap();
        assert!(section(&inspect(&dir.join("old.doc")), "Document").is_none());
    }

    #[test]
    fn base_describes_a_text_file() {
        let dir = temp_dir("base");
        let path = dir.join("notes.txt");
        std::fs::write(&path, "x".repeat(1234)).unwrap();
        let s = base(&Entry::from_path(path).unwrap());
        assert_eq!(value(&s, "Name"), Some("notes.txt"));
        assert_eq!(value(&s, "Kind"), Some("File"));
        assert!(value(&s, "Size").unwrap().contains("1,234 bytes"));
        assert!(value(&s, "Modified").is_some_and(|v| !v.is_empty()));
        // Since v0.39.0 a text file gets one section of its own, and only that
        // one: it is not an archive, a document or a program.
        let all = inspect(&dir.join("notes.txt"));
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[0].title, "Text");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_image_dimensions() {
        let dir = temp_dir("image");
        // Named `.dat` on purpose: the format is sniffed, not taken from the name.
        let path = dir.join("pic.dat");
        image::RgbaImage::new(7, 3).save_with_format(&path, image::ImageFormat::Png).unwrap();
        let sections = inspect(&path);
        let img = sections.iter().find(|s| s.title == "Image").unwrap();
        assert_eq!(value(img, "Dimensions"), Some("7 × 3"));
        assert_eq!(value(img, "Format"), Some("PNG"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn counts_a_directory() {
        let dir = temp_dir("dir");
        std::fs::write(dir.join("a"), "12").unwrap();
        std::fs::write(dir.join("b"), "345").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let sections = inspect(&dir);
        let d = sections.iter().find(|s| s.title == "Directory").unwrap();
        assert_eq!(value(d, "Files"), Some("2"));
        assert_eq!(value(d, "Directories"), Some("1"));
        assert!(value(d, "Files' size").unwrap().contains("5 bytes"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn groups_digits() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
