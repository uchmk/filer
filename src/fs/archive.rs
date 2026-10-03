//! Packing and unpacking, in pure Rust.
//!
//! zip / tar / flate2 / sevenz-rust2 do the work, so nothing here shells out to
//! 7-Zip and nothing links a C library: a machine without 7-Zip installed
//! behaves like one with it, and an ARM64 build needs no toolchain beyond
//! cargo.
//!
//! An archive is untrusted input. Every entry name goes through [`safe_dest`]
//! before anything is written, so one rule covers all three readers.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

/// What an archive is, as far as this app is concerned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    SevenZ,
}

impl Format {
    /// The format a name asks for, or `None` when it names no archive.
    /// `.tar.gz` is looked at before `.gz` so the double extension wins.
    pub fn from_path(p: &Path) -> Option<Format> {
        let name = p.file_name()?.to_string_lossy().to_ascii_lowercase();
        for (suffix, f) in [
            (".tar.gz", Format::TarGz),
            (".tgz", Format::TarGz),
            (".zip", Format::Zip),
            (".tar", Format::Tar),
            (".7z", Format::SevenZ),
        ] {
            // `.zip` alone is a file called nothing, not an archive.
            if name.len() > suffix.len() && name.ends_with(suffix) {
                return Some(f);
            }
        }
        None
    }

    /// Whether [`compress`] can write this format. 7z is read-only here:
    /// sevenz-rust2 packs only what it can encode, and the formats above cover
    /// what a file manager is asked for.
    pub fn can_write(self) -> bool {
        true
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::SevenZ => "7z",
        }
    }
}

/// Called as each entry lands, with its name and its size in bytes. Returning
/// `false` stops the job where it is.
pub type OnEntry<'a> = &'a mut dyn FnMut(&str, u64) -> bool;

/// Where an entry of `name` may be written under `dest`.
///
/// An archive names its own entries, so it can ask for `../../autorun` or
/// `C:\Windows\x` and `dest.join()` would happily oblige. Only plain
/// components are kept: anything that climbs, roots or carries a drive letter
/// is refused, and so is a name that lands back on `dest` itself.
fn safe_dest(dest: &Path, name: &str) -> Option<PathBuf> {
    let mut out = dest.to_path_buf();
    let mut pushed = false;
    for c in Path::new(name).components() {
        match c {
            Component::Normal(s) => {
                out.push(s);
                pushed = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    pushed.then_some(out)
}

/// Unpack `archive` into `dest`, which is created if it is not there.
pub fn extract(archive: &Path, dest: &Path, on_entry: OnEntry<'_>) -> io::Result<()> {
    let Some(format) = Format::from_path(archive) else {
        return Err(io::Error::other("not an archive this build can read"));
    };
    std::fs::create_dir_all(dest)?;
    match format {
        Format::Zip => extract_zip(archive, dest, on_entry),
        Format::Tar => extract_tar(&mut BufReader::new(File::open(archive)?), dest, on_entry),
        Format::TarGz => {
            let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(archive)?));
            extract_tar(&mut BufReader::new(gz), dest, on_entry)
        }
        Format::SevenZ => extract_7z(archive, dest, on_entry),
    }
}

fn extract_zip(archive: &Path, dest: &Path, on_entry: OnEntry<'_>) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(archive)?))
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut refused = 0usize;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| io::Error::other(e.to_string()))?;
        let name = entry.name().to_owned();
        let Some(path) = safe_dest(dest, &name) else {
            refused += 1;
            continue;
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let size = entry.size();
        let when = zip_entry_time(entry.extra_data_fields(), entry.last_modified());
        let mut out = BufWriter::new(File::create(&path)?);
        io::copy(&mut entry, &mut out)?;
        out.flush()?;
        set_time(out, when)?;
        if !on_entry(&name, size) {
            return Ok(());
        }
    }
    refused_error(refused)
}

/// The entry's own time onto the file just written. Without it every file
/// came out stamped with the moment it was unpacked, so a round trip through
/// filer's own `E` and `e` lost the dates `E` had kept (#156). A time the
/// archive does not carry leaves the file as it is.
fn set_time(out: BufWriter<File>, when: Option<std::time::SystemTime>) -> io::Result<()> {
    if let Some(when) = when {
        out.into_inner().map_err(|e| e.into_error())?.set_modified(when)?;
    }
    Ok(())
}

/// The entry's modification time, from the most exact field it carries: NTFS
/// (`0x000a`, to 100 ns, what 7-Zip writes on Windows), then the extended
/// timestamp (`0x5455`, to the second, what Info-ZIP and filer write), then
/// the MS-DOS field every zip has. The DOS field alone is two-second steps in
/// local time, and 7-Zip rounds it *up*: `2019-02-28 23:59:59` came back as
/// `2019-03-01 00:00:00`, a day and a month later, where 7-Zip and Explorer
/// both read the exact time from the same archive (#174).
fn zip_entry_time<'a>(
    extra: impl Iterator<Item = &'a zip::ExtraField>,
    dos: Option<zip::DateTime>,
) -> Option<std::time::SystemTime> {
    let extra: Vec<&zip::ExtraField> = extra.collect();
    let ntfs = extra.iter().find_map(|f| match f {
        zip::ExtraField::Ntfs(n) if n.mtime() != 0 => from_filetime(n.mtime()),
        _ => None,
    });
    let unix = || {
        extra.iter().find_map(|f| match f {
            zip::ExtraField::ExtendedTimestamp(t) => {
                t.mod_time().map(|s| std::time::UNIX_EPOCH + std::time::Duration::from_secs(u64::from(s)))
            }
            _ => None,
        })
    };
    ntfs.or_else(unix).or_else(|| dos.and_then(from_zip_time))
}

/// A Windows FILETIME: 100 ns ticks since 1601-01-01 UTC.
fn from_filetime(ticks: u64) -> Option<std::time::SystemTime> {
    const TO_UNIX: u64 = 11_644_473_600;
    let (secs, rest) = (ticks / 10_000_000, ticks % 10_000_000);
    let gap = std::time::Duration::new(secs.abs_diff(TO_UNIX), 0);
    let at = if secs >= TO_UNIX { std::time::UNIX_EPOCH.checked_add(gap)? } else { std::time::UNIX_EPOCH.checked_sub(gap)? };
    at.checked_add(std::time::Duration::from_nanos(rest * 100))
}

/// A zip's MS-DOS time, which is local time with no zone -- the way `zip_time`
/// writes it, so the two meet in the middle.
fn from_zip_time(t: zip::DateTime) -> Option<std::time::SystemTime> {
    let day = chrono::NaiveDate::from_ymd_opt(i32::from(t.year()), u32::from(t.month()), u32::from(t.day()))?;
    let at = day.and_hms_opt(u32::from(t.hour()), u32::from(t.minute()), u32::from(t.second()))?;
    // The zero the format falls back to says nothing about the file.
    if t.year() <= 1980 && t.month() == 1 && t.day() == 1 {
        return None;
    }
    at.and_local_timezone(chrono::Local).earliest().map(Into::into)
}

fn extract_tar<R: Read>(reader: &mut R, dest: &Path, on_entry: OnEntry<'_>) -> io::Result<()> {
    let mut tar = tar::Archive::new(reader);
    let mut refused = 0usize;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        let Some(path) = safe_dest(dest, &name) else {
            refused += 1;
            continue;
        };
        let size = entry.size();
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let when = entry
            .header()
            .mtime()
            .ok()
            .filter(|&s| s > 0)
            .map(|s| std::time::UNIX_EPOCH + std::time::Duration::from_secs(s));
        let mut out = BufWriter::new(File::create(&path)?);
        io::copy(&mut entry, &mut out)?;
        out.flush()?;
        set_time(out, when)?;
        if !on_entry(&name, size) {
            return Ok(());
        }
    }
    refused_error(refused)
}

fn extract_7z(archive: &Path, dest: &Path, on_entry: OnEntry<'_>) -> io::Result<()> {
    let file = File::open(archive)?;
    let mut refused = 0usize;
    let mut stop = false;
    // sevenz-rust2 hands each entry to this closure with the destination it
    // worked out itself; that one is ignored in favour of `safe_dest`. Not a
    // precaution against this library in particular -- zip and tar are read
    // the same way -- but it is what made RUSTSEC-2026-0245, a path traversal
    // in the predecessor's own extraction, something this program never
    // reached.
    let res = sevenz_rust2::decompress_with_extract_fn(file, dest, |entry, reader, _their_dest| {
        if stop {
            return Ok(false);
        }
        let name = entry.name().to_owned();
        let Some(path) = safe_dest(dest, &name) else {
            refused += 1;
            return Ok(true);
        };
        let mut write = || -> io::Result<()> {
            if entry.is_directory() {
                std::fs::create_dir_all(&path)?;
                return Ok(());
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = BufWriter::new(File::create(&path)?);
            io::copy(reader, &mut out)?;
            out.flush()?;
            let when = entry.has_last_modified_date.then(|| entry.last_modified_date().into());
            set_time(out, when)
        };
        write().map_err(sevenz_rust2::Error::from)?;
        if !entry.is_directory() && !on_entry(&name, entry.size()) {
            stop = true;
            return Ok(false);
        }
        Ok(true)
    });
    res.map_err(|e| io::Error::other(e.to_string()))?;
    refused_error(refused)
}

fn refused_error(refused: usize) -> io::Result<()> {
    match refused {
        0 => Ok(()),
        n => Err(io::Error::other(format!(
            "{n} entr{} refused: the name escapes the destination",
            if n == 1 { "y" } else { "ies" }
        ))),
    }
}

/// One entry of an archive, as the preview pane wants it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub name: String,
    pub size: u64,
    pub dir: bool,
}

/// What is inside `archive`, up to `limit` entries. The flag says whether the
/// archive holds more than were read.
///
/// Only the table of contents is touched where the format has one: a zip's
/// central directory and a 7z's header are read without inflating anything. A
/// tar has no index, so its entries are walked — but the data is skipped, not
/// decompressed into memory.
pub fn list(archive: &Path, limit: usize) -> io::Result<(Vec<Listed>, bool)> {
    let Some(format) = Format::from_path(archive) else {
        return Err(io::Error::other("not an archive this build can read"));
    };
    let mut out: Vec<Listed> = Vec::new();
    let mut more = false;
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(archive)?))
                .map_err(|e| io::Error::other(e.to_string()))?;
            more = zip.len() > limit;
            for i in 0..zip.len().min(limit) {
                let e = zip.by_index(i).map_err(|e| io::Error::other(e.to_string()))?;
                out.push(Listed { name: e.name().to_owned(), size: e.size(), dir: e.is_dir() });
            }
        }
        Format::Tar => list_tar(&mut BufReader::new(File::open(archive)?), limit, &mut out, &mut more)?,
        Format::TarGz => {
            let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(archive)?));
            list_tar(&mut BufReader::new(gz), limit, &mut out, &mut more)?
        }
        Format::SevenZ => {
            let reader = sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty())
                .map_err(|e| io::Error::other(e.to_string()))?;
            let files = &reader.archive().files;
            more = files.len() > limit;
            for f in files.iter().take(limit) {
                out.push(Listed {
                    name: f.name().to_owned(),
                    size: f.size(),
                    dir: f.is_directory(),
                });
            }
        }
    }
    Ok((out, more))
}

/// How locked an archive is, as far as its table of contents shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encryption {
    No,
    /// The names are readable; the contents need a password.
    Entries,
    /// The table of contents is itself encrypted, so nothing below is known.
    Header,
}

/// What an archive holds, without its names.
///
/// [`list`] is for the preview pane, which wants the entries themselves. This
/// is for a summary, and keeps no name at all, so a 200k-entry archive costs a
/// few words rather than a few megabytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Summary {
    pub format: Format,
    pub files: u64,
    pub dirs: u64,
    /// Uncompressed bytes of the files counted.
    pub bytes: u64,
    pub encryption: Encryption,
    /// Whether the scan stopped at `limit` rather than at the end.
    pub more: bool,
}

/// Count what is inside `archive`, reading at most `limit` entries.
///
/// Only the table of contents is touched, the same way [`list`] does it, so
/// nothing is inflated however large the archive is.
pub fn summarize(archive: &Path, limit: usize) -> io::Result<Summary> {
    let Some(format) = Format::from_path(archive) else {
        return Err(io::Error::other("not an archive this build can read"));
    };
    let mut s = Summary {
        format,
        files: 0,
        dirs: 0,
        bytes: 0,
        encryption: Encryption::No,
        more: false,
    };
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(archive)?))
                .map_err(|e| io::Error::other(e.to_string()))?;
            s.more = zip.len() > limit;
            for i in 0..zip.len().min(limit) {
                // `by_index` refuses an encrypted entry with `Password required`
                // before it hands one back, which is why the preview pane cannot
                // list an encrypted zip at all. `by_index_raw` skips that check
                // and still answers `encrypted()` -- and it builds no
                // decompressor, so it is the cheaper call here either way.
                let e = zip.by_index_raw(i).map_err(|e| io::Error::other(e.to_string()))?;
                if e.encrypted() {
                    s.encryption = Encryption::Entries;
                }
                if e.is_dir() {
                    s.dirs += 1;
                } else {
                    s.files += 1;
                    s.bytes += e.size();
                }
            }
        }
        Format::Tar => summarize_tar(&mut BufReader::new(File::open(archive)?), limit, &mut s)?,
        Format::TarGz => {
            let gz = flate2::read::GzDecoder::new(BufReader::new(File::open(archive)?));
            summarize_tar(&mut BufReader::new(gz), limit, &mut s)?
        }
        Format::SevenZ => {
            // An encrypted header is a state worth reporting, not a failure:
            // the archive is fine, it just cannot be read without a password.
            let a = match sevenz_rust2::Archive::open(archive) {
                Ok(a) => a,
                Err(sevenz_rust2::Error::PasswordRequired) => {
                    s.encryption = Encryption::Header;
                    return Ok(s);
                }
                Err(e) => return Err(io::Error::other(e.to_string())),
            };
            let aes = a.blocks.iter().flat_map(|b| b.coders.iter()).any(|c| {
                c.encoder_method_id() == sevenz_rust2::EncoderMethod::ID_AES256_SHA256
            });
            if aes {
                s.encryption = Encryption::Entries;
            }
            s.more = a.files.len() > limit;
            for f in a.files.iter().take(limit) {
                if f.is_directory() {
                    s.dirs += 1;
                } else {
                    s.files += 1;
                    s.bytes += f.size();
                }
            }
        }
    }
    Ok(s)
}

fn summarize_tar<R: Read>(reader: &mut R, limit: usize, s: &mut Summary) -> io::Result<()> {
    let mut tar = tar::Archive::new(reader);
    for (seen, entry) in tar.entries()?.enumerate() {
        let entry = entry?;
        if seen == limit {
            s.more = true;
            break;
        }
        if entry.header().entry_type().is_dir() {
            s.dirs += 1;
        } else {
            s.files += 1;
            s.bytes += entry.size();
        }
    }
    Ok(())
}

fn list_tar<R: Read>(
    reader: &mut R,
    limit: usize,
    out: &mut Vec<Listed>,
    more: &mut bool,
) -> io::Result<()> {
    let mut tar = tar::Archive::new(reader);
    for entry in tar.entries()? {
        let entry = entry?;
        if out.len() == limit {
            *more = true;
            break;
        }
        out.push(Listed {
            name: entry.path()?.to_string_lossy().into_owned(),
            size: entry.size(),
            dir: entry.header().entry_type().is_dir(),
        });
    }
    Ok(())
}

/// Pack `srcs` into `archive`. Names inside are taken relative to `base`, so
/// a folder keeps its shape and a file keeps its own name.
pub fn compress(
    srcs: &[PathBuf],
    base: &Path,
    archive: &Path,
    format: Format,
    on_entry: OnEntry<'_>,
) -> io::Result<()> {
    if !format.can_write() {
        return Err(io::Error::other(format!("cannot write {}", format.label())));
    }
    let members = walk(srcs, base);
    let out = BufWriter::new(File::create(archive)?);
    // Each writer is closed by hand rather than on drop: a gzip trailer or a
    // zip central directory written by a destructor has nowhere to report a
    // failure, and a truncated archive still looks openable.
    let mut out = match format {
        Format::Zip => write_zip(&members, out, on_entry)?,
        Format::Tar => write_tar(&members, out, on_entry)?,
        Format::TarGz => {
            let gz = flate2::write::GzEncoder::new(out, flate2::Compression::default());
            write_tar(&members, gz, on_entry)?.finish()?
        }
        Format::SevenZ => write_7z(&members, out, on_entry)?,
    };
    out.flush()?;
    out.into_inner().map_err(io::Error::other)?.sync_all()
}

/// One thing to put in an archive: where it is on disk, what it is called
/// inside, and whether it is a directory.
struct Member {
    path: PathBuf,
    name: String,
    dir: bool,
}

/// Every file under `srcs`, named relative to `base`. Directories come before
/// what they hold, so a reader can create them in order.
fn walk(srcs: &[PathBuf], base: &Path) -> Vec<Member> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = srcs.to_vec();
    stack.reverse();
    while let Some(p) = stack.pop() {
        let Ok(md) = std::fs::symlink_metadata(&p) else { continue };
        let name = match p.strip_prefix(base) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            // Not under `base` (a selection from elsewhere): its own name will do.
            Err(_) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        };
        if name.is_empty() {
            continue;
        }
        if md.file_type().is_symlink() {
            // Store what a link points at, and never walk through it: a link
            // back up the tree would pack forever.
            if std::fs::metadata(&p).map(|t| t.is_file()).unwrap_or(false) {
                out.push(Member { path: p, name, dir: false });
            }
            continue;
        }
        if md.is_dir() {
            out.push(Member { path: p.clone(), name, dir: true });
            let Ok(rd) = std::fs::read_dir(&p) else { continue };
            let mut children: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            children.sort();
            children.reverse();
            stack.extend(children);
        } else {
            out.push(Member { path: p, name, dir: false });
        }
    }
    out
}

/// The file's own modification time, as a zip entry holds it: local time to
/// the even second (the MS-DOS form). Without it every entry was stamped
/// 1980-01-01, the format's zero, while `.7z` and `.tar.gz` kept the real
/// times (#96). `None` for a time the format cannot hold, which then falls
/// back to that zero rather than failing the whole archive.
fn zip_time(path: &Path) -> Option<zip::DateTime> {
    use chrono::{Datelike, Timelike};
    let when: chrono::DateTime<chrono::Local> = std::fs::metadata(path).ok()?.modified().ok()?.into();
    zip::DateTime::from_date_and_time(
        u16::try_from(when.year()).ok()?,
        when.month() as u8,
        when.day() as u8,
        when.hour() as u8,
        when.minute() as u8,
        when.second() as u8,
    )
    .ok()
}

/// The file's modification time as the extended timestamp holds it: whole
/// seconds since 1970, in 32 bits. `None` outside that range, and the entry
/// then has the DOS field only.
fn unix_secs(path: &Path) -> Option<u32> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    u32::try_from(modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()).ok()
}

fn write_zip<W: Write + io::Seek>(
    members: &[Member],
    out: W,
    on_entry: OnEntry<'_>,
) -> io::Result<W> {
    let mut zip = zip::ZipWriter::new(out);
    let opts = zip::write::FullFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for m in members {
        let to_io = |e: zip::result::ZipError| io::Error::other(e.to_string());
        let mut opts = match zip_time(&m.path) {
            Some(t) => opts.clone().last_modified_time(t),
            None => opts.clone(),
        };
        // The exact second as well, in the extended timestamp: the DOS field
        // alone is two-second steps, so another tool unpacking filer's zip got
        // every odd second one lower (#174).
        if let Some(secs) = unix_secs(&m.path) {
            let mut field = vec![1u8];
            field.extend_from_slice(&secs.to_le_bytes());
            opts.add_extra_data(0x5455, field, false).map_err(to_io)?;
        }
        if m.dir {
            zip.add_directory(&m.name, opts).map_err(to_io)?;
            continue;
        }
        zip.start_file(&m.name, opts).map_err(to_io)?;
        let mut f = BufReader::new(File::open(&m.path)?);
        let size = io::copy(&mut f, &mut zip)?;
        if !on_entry(&m.name, size) {
            break;
        }
    }
    zip.finish().map_err(|e| io::Error::other(e.to_string()))
}

/// 7z, which compresses harder than the rest and is what people reach for when
/// the archive has to travel.
///
/// The encoder was already in the binary: `sevenz-rust2` turns its `compress`
/// feature on by default, so it was being built and never called. Reading came
/// first and writing was left out until the writer had been looked at; it
/// takes one entry at a time, which is what the progress callback needs.
fn write_7z<W: Write + io::Seek>(
    members: &[Member],
    out: W,
    on_entry: OnEntry<'_>,
) -> io::Result<W> {
    let to_io = |e: sevenz_rust2::Error| io::Error::other(e.to_string());
    let mut z = sevenz_rust2::ArchiveWriter::new(out).map_err(to_io)?;
    for m in members {
        let entry = sevenz_rust2::ArchiveEntry::from_path(&m.path, m.name.clone());
        // A directory is a name and nothing to read, and it is not reported:
        // the callback counts files, the way the zip and tar writers do, and
        // a progress bar that counted folders would not match its own total.
        if m.dir {
            z.push_archive_entry::<&[u8]>(entry, None).map_err(to_io)?;
            continue;
        }
        let f = BufReader::new(File::open(&m.path)?);
        let size = z.push_archive_entry(entry, Some(f)).map_err(to_io)?.size;
        if !on_entry(&m.name, size) {
            break;
        }
    }
    z.finish()
}

fn write_tar<W: Write>(members: &[Member], out: W, on_entry: OnEntry<'_>) -> io::Result<W> {
    let mut tar = tar::Builder::new(out);
    for m in members {
        if m.dir {
            tar.append_dir(&m.name, &m.path)?;
            continue;
        }
        let mut f = File::open(&m.path)?;
        let size = std::fs::metadata(&m.path).map(|md| md.len()).unwrap_or(0);
        tar.append_file(&m.name, &mut f)?;
        if !on_entry(&m.name, size) {
            break;
        }
    }
    tar.into_inner()
}

/// Where an archive unpacks: a folder in `into` named after it, with the
/// extension dropped. `report.tar.gz` gives `report`, not `report.tar`.
/// An archive whose top level is one folder and nothing else is unpacked as
/// that folder, not inside a second one named after the archive: most archives
/// that are downloaded have exactly that shape, and `e` gave `to-pack_1\to-pack\`
/// two levels down (Q43, #156). `into` is the folder it was unpacked into; the
/// result is where the contents are now. A name already taken beside it is
/// stepped past (`unique` decides how), as the wrapper's own name was.
///
/// Anything else -- loose files, two folders, a lone file -- stays wrapped,
/// which is the case the wrapper is for: an archive that would scatter.
pub fn lift_lone_folder(into: &Path, unique: impl Fn(&Path) -> PathBuf) -> std::io::Result<PathBuf> {
    let mut entries = std::fs::read_dir(into)?;
    let (Some(only), None) = (entries.next().transpose()?, entries.next()) else {
        return Ok(into.to_path_buf());
    };
    if !only.file_type()?.is_dir() {
        return Ok(into.to_path_buf());
    }
    let Some(parent) = into.parent() else { return Ok(into.to_path_buf()) };
    // Out of the wrapper under a name nothing has, then the wrapper goes, and
    // only then the real name is chosen -- the wrapper is often called exactly
    // what is inside it, and has to be gone before that name is free.
    let parked = unique(&parent.join(format!(".{}.unpacking", only.file_name().to_string_lossy())));
    std::fs::rename(only.path(), &parked)?;
    std::fs::remove_dir(into)?;
    let target = unique(&parent.join(only.file_name()));
    std::fs::rename(&parked, &target)?;
    Ok(target)
}

pub fn extract_dir(archive: &Path, into: &Path) -> PathBuf {
    let name = archive.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let stem = [".tar.gz", ".tgz", ".zip", ".tar", ".7z"]
        .iter()
        .find(|s| lower.ends_with(**s))
        .map(|s| &name[..name.len() - s.len()])
        .unwrap_or(&name);
    into.join(if stem.is_empty() { "extracted" } else { stem })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_decides_the_format() {
        assert_eq!(Format::from_path(Path::new("a/b.zip")), Some(Format::Zip));
        assert_eq!(Format::from_path(Path::new("B.ZIP")), Some(Format::Zip));
        assert_eq!(Format::from_path(Path::new("x.tar")), Some(Format::Tar));
        // The double extension beats the single one.
        assert_eq!(Format::from_path(Path::new("x.tar.gz")), Some(Format::TarGz));
        assert_eq!(Format::from_path(Path::new("x.tgz")), Some(Format::TarGz));
        assert_eq!(Format::from_path(Path::new("x.7z")), Some(Format::SevenZ));
        assert_eq!(Format::from_path(Path::new("notes.txt")), None);
        // A name that is nothing but the extension names no archive.
        assert_eq!(Format::from_path(Path::new(".zip")), None);
        // Every format read here can be written as well, since v0.27.0.
        assert!([Format::Zip, Format::Tar, Format::TarGz, Format::SevenZ]
            .iter()
            .all(|f| f.can_write()));
    }

    /// An archive names its own entries, so the names are an attacker's to
    /// choose. Nothing may land outside the directory the user asked for.
    #[test]
    fn an_entry_cannot_climb_out_of_the_destination() {
        let dest = Path::new("/out");
        assert_eq!(safe_dest(dest, "a/b.txt"), Some(PathBuf::from("/out/a/b.txt")));
        assert_eq!(safe_dest(dest, "./a"), Some(PathBuf::from("/out/a")));
        // Climbing, rooted and drive-qualified names are refused outright.
        assert_eq!(safe_dest(dest, "../evil"), None);
        assert_eq!(safe_dest(dest, "a/../../evil"), None);
        assert_eq!(safe_dest(dest, "/etc/passwd"), None);
        // Nothing to write: an entry that names the destination itself.
        assert_eq!(safe_dest(dest, ""), None);
        assert_eq!(safe_dest(dest, "."), None);
    }

    /// #96: a zip entry carries its file's own time, not 1980-01-01.
    #[test]
    fn a_zip_entry_keeps_its_files_time() {
        let dir = crate::util::test_dir("zip-time");
        let file = dir.join("dated.txt");
        std::fs::write(&file, "x").unwrap();
        // 2021-06-15 12:34:56 local time, a moment no other test would pick.
        let when = chrono::NaiveDate::from_ymd_opt(2021, 6, 15)
            .and_then(|d| d.and_hms_opt(12, 34, 56))
            .and_then(|t| t.and_local_timezone(chrono::Local).single())
            .unwrap();
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(when.into()).unwrap();

        let archive = dir.join("out.zip");
        compress(&[file], &dir, &archive, Format::Zip, &mut |_, _| true).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
        let entry = zip.by_name("dated.txt").unwrap();
        let t = entry.last_modified().expect("a time is stored");
        assert_eq!((t.year(), t.month(), t.day()), (2021, 6, 15), "the file's day, not 1980-01-01");
        assert_eq!((t.hour(), t.minute(), t.second()), (12, 34, 56), "to the even second");
    }

    /// #156: unpacking puts each entry's time back, for all three formats.
    /// Packed by filer itself, so it is the round trip a user would make.
    #[test]
    fn unpacking_keeps_the_entries_times() {
        let dir = crate::util::test_dir("unpack-time");
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        let when = chrono::NaiveDate::from_ymd_opt(2021, 6, 15)
            .and_then(|d| d.and_hms_opt(12, 34, 56))
            .and_then(|t| t.and_local_timezone(chrono::Local).single())
            .unwrap();
        for f in ["a.txt", "sub/b.txt"] {
            let p = src.join(f);
            std::fs::write(&p, f).unwrap();
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(when.into()).unwrap();
        }
        for (format, name) in [(Format::Zip, "x.zip"), (Format::TarGz, "x.tar.gz"), (Format::SevenZ, "x.7z")] {
            let archive = dir.join(name);
            compress(std::slice::from_ref(&src), &dir, &archive, format, &mut |_, _| true).unwrap();
            let out = dir.join(format!("out-{name}"));
            extract(&archive, &out, &mut |_, _| true).unwrap();
            for f in ["src/a.txt", "src/sub/b.txt"] {
                let got: chrono::DateTime<chrono::Local> =
                    std::fs::metadata(out.join(f)).unwrap().modified().unwrap().into();
                assert_eq!(got.timestamp(), when.timestamp(), "{name}: {f} keeps 12:34:56, not the time it was unpacked");
            }
        }
    }

    /// #174: an odd second survives filer's own zip, through the extended
    /// timestamp; the DOS field alone came back one second low.
    #[test]
    fn a_zip_keeps_an_odd_second() {
        let dir = crate::util::test_dir("zip-odd");
        let file = dir.join("odd.txt");
        std::fs::write(&file, "x").unwrap();
        let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_623_760_497); // ...:57, odd
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(when).unwrap();
        let archive = dir.join("odd.zip");
        compress(std::slice::from_ref(&file), &dir, &archive, Format::Zip, &mut |_, _| true).unwrap();
        let out = dir.join("out");
        extract(&archive, &out, &mut |_, _| true).unwrap();
        assert_eq!(std::fs::metadata(out.join("odd.txt")).unwrap().modified().unwrap(), when);
    }

    /// #174: a zip as 7-Zip writes it -- the DOS field rounded up past
    /// midnight, the exact time in an NTFS field -- unpacks to the exact time,
    /// not to the next day and month.
    #[test]
    fn another_tools_exact_time_wins_over_the_rounded_one() {
        let dir = crate::util::test_dir("zip-ntfs");
        // 2019-02-28 23:59:59.5 UTC as a FILETIME.
        let exact = std::time::UNIX_EPOCH + std::time::Duration::new(1_551_398_399, 500_000_000);
        let ticks = (1_551_398_399u64 + 11_644_473_600) * 10_000_000 + 5_000_000;
        let mut ntfs = Vec::new();
        ntfs.extend_from_slice(&0u32.to_le_bytes()); // reserved
        ntfs.extend_from_slice(&1u16.to_le_bytes()); // attribute 1: the three times
        ntfs.extend_from_slice(&24u16.to_le_bytes());
        for t in [ticks, ticks, ticks] {
            ntfs.extend_from_slice(&t.to_le_bytes());
        }
        let mut opts = zip::write::FullFileOptions::default()
            .last_modified_time(zip::DateTime::from_date_and_time(2019, 3, 1, 0, 0, 0).unwrap());
        opts.add_extra_data(0x000a, ntfs, false).unwrap();
        let archive = dir.join("7z-made.zip");
        let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
        zip.start_file("late.txt", opts).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();

        let out = dir.join("out");
        extract(&archive, &out, &mut |_, _| true).unwrap();
        assert_eq!(std::fs::metadata(out.join("late.txt")).unwrap().modified().unwrap(), exact);
    }

    /// The FILETIME epoch is 1601; the arithmetic is the whole of it.
    #[test]
    fn a_filetime_is_read_from_1601() {
        assert_eq!(from_filetime(116_444_736_000_000_000), Some(std::time::UNIX_EPOCH));
        let one_and_a_bit = std::time::UNIX_EPOCH + std::time::Duration::new(1, 100);
        assert_eq!(from_filetime(116_444_736_010_000_001), Some(one_and_a_bit));
    }

    /// Q43: one folder at the top comes out as that folder; anything else
    /// keeps the wrapper.
    #[test]
    fn a_lone_top_folder_is_not_wrapped_again() {
        let unique = |p: &Path| crate::fs::ops::unique_name(p);
        let dir = crate::util::test_dir("lift");

        // The wrapper has the same name as what is inside it.
        let into = dir.join("to-pack");
        std::fs::create_dir_all(into.join("to-pack").join("sub")).unwrap();
        std::fs::write(into.join("to-pack").join("a.txt"), "a").unwrap();
        let got = lift_lone_folder(&into, unique).unwrap();
        assert_eq!(got, dir.join("to-pack"));
        assert!(got.join("a.txt").is_file() && got.join("sub").is_dir(), "the contents, one level up");
        assert!(!got.join("to-pack").exists(), "no second level");

        // The name beside it is taken: stepped past, like the wrapper's.
        let into = dir.join("again");
        std::fs::create_dir_all(into.join("to-pack")).unwrap();
        assert_eq!(lift_lone_folder(&into, unique).unwrap(), dir.join("to-pack_1"));
        assert!(!into.exists(), "the empty wrapper is gone");

        // Loose files, or more than one thing, stay wrapped.
        let loose = dir.join("loose");
        std::fs::create_dir_all(&loose).unwrap();
        std::fs::write(loose.join("x.txt"), "x").unwrap();
        assert_eq!(lift_lone_folder(&loose, unique).unwrap(), loose);
        let two = dir.join("two");
        std::fs::create_dir_all(two.join("a")).unwrap();
        std::fs::create_dir_all(two.join("b")).unwrap();
        assert_eq!(lift_lone_folder(&two, unique).unwrap(), two);
    }

    #[test]
    fn an_archive_unpacks_into_a_folder_named_after_it() {
        let into = Path::new("/a");
        assert_eq!(extract_dir(Path::new("/a/report.zip"), into), PathBuf::from("/a/report"));
        // The whole double extension goes, not just the `.gz`.
        assert_eq!(extract_dir(Path::new("/a/report.tar.gz"), into), PathBuf::from("/a/report"));
        assert_eq!(extract_dir(Path::new("/b/report.7z"), into), PathBuf::from("/a/report"));
    }

    /// The round trip is the real test of the writers: pack a small tree, read
    /// it back, and see the same names and bytes.
    #[test]
    fn a_tree_survives_being_packed_and_unpacked() {
        let base = crate::util::test_dir("archive");
        for format in [Format::Zip, Format::Tar, Format::TarGz, Format::SevenZ] {
            // One subdirectory per format, so the four runs cannot see each
            // other's files.
            let root = base.join(format.label());
            let src = root.join("src");
            std::fs::create_dir_all(src.join("sub")).unwrap();
            std::fs::write(src.join("top.txt"), b"top").unwrap();
            std::fs::write(src.join("sub").join("deep.txt"), b"deep").unwrap();

            let archive = root.join(format!("out.{}", format.label()));
            let mut packed = Vec::new();
            compress(std::slice::from_ref(&src), &root, &archive, format, &mut |n, _| {
                packed.push(n.to_owned());
                true
            })
            .unwrap();
            assert_eq!(packed.len(), 2, "{:?}: both files are packed", format);

            // The table of contents names the same things, without unpacking.
            let (listed, more) = list(&archive, 100).unwrap();
            assert!(!more, "{:?}: four entries is not a hundred", format);
            let names: Vec<&str> = listed.iter().map(|l| l.name.trim_end_matches('/')).collect();
            assert!(names.contains(&"src/top.txt"), "{:?}: got {names:?}", format);
            assert!(names.contains(&"src/sub/deep.txt"), "{:?}: got {names:?}", format);
            let top = listed.iter().find(|l| l.name == "src/top.txt").unwrap();
            assert_eq!(top.size, 3, "{:?}: `top` is three bytes", format);
            assert!(!top.dir);
            assert!(
                listed.iter().any(|l| l.dir && l.name.starts_with("src")),
                "{:?}: the directories are listed too, and marked: {listed:?}",
                format
            );

            // Reading only the first entry says there are more.
            let (few, more) = list(&archive, 1).unwrap();
            assert_eq!(few.len(), 1);
            assert!(more, "{:?}: a limit that cuts the listing says so", format);

            // The summary counts the same tree without keeping a name.
            let sum = summarize(&archive, 100).unwrap();
            assert_eq!(sum.format, format);
            assert_eq!(sum.files, 2, "{:?}: two files", format);
            assert!(sum.dirs >= 1, "{:?}: at least `src` itself: {sum:?}", format);
            assert_eq!(sum.bytes, 7, "{:?}: `top` plus `deep`", format);
            assert_eq!(sum.encryption, Encryption::No, "{:?}: nothing was locked", format);
            assert!(!sum.more, "{:?}: four entries is not a hundred", format);
            assert!(summarize(&archive, 1).unwrap().more, "{:?}: a cut scan says so", format);

            let out = root.join("out");
            extract(&archive, &out, &mut |_, _| true).unwrap();
            assert_eq!(std::fs::read(out.join("src").join("top.txt")).unwrap(), b"top");
            assert_eq!(
                std::fs::read(out.join("src").join("sub").join("deep.txt")).unwrap(),
                b"deep"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
