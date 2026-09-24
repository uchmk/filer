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
        link,
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
        if i > 0 && (digits.len() - i) % 3 == 0 {
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
        assert!(inspect(&dir.join("notes.txt")).is_empty());
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
