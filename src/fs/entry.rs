use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::util;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Kind {
    Dir,
    #[default]
    File,
    /// Symlink / junction / reparse point, with the kind of its target when known.
    Link { to_dir: bool, broken: bool },
}

impl Kind {
    /// Whether entering this entry means changing directory.
    pub fn is_dir_like(self) -> bool {
        matches!(self, Kind::Dir | Kind::Link { to_dir: true, .. })
    }

    pub fn is_link(self) -> bool {
        matches!(self, Kind::Link { .. })
    }
}

/// What the right-hand column of the list says about each entry.
///
/// A `String` until v0.44.2, which meant a misspelling in `yazi.toml` or in a
/// `linemode` binding produced a blank column and no complaint: there was no
/// difference between "show nothing here", "show something nobody has
/// implemented" and "you typed `mtiem`". Spelled as a type, the config loader
/// rejects the first, the keymap loader warns about the second, and the third is
/// impossible.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Linemode {
    #[default]
    None,
    /// A file's length, and a directory's child count.
    Size,
    /// Everything under a folder, as the disk-usage view measured it.
    Usage,
    #[serde(alias = "modified")]
    Mtime,
    #[serde(alias = "created")]
    Btime,
    Permissions,
    /// yazi shows a file's owner here; nothing reads it yet, so the column is
    /// blank. Kept as a variant all the same -- a blank column that was asked
    /// for is not the same thing as a typo, and this is what lets the loader
    /// tell them apart.
    Owner,
}

impl Linemode {
    /// The spellings a config file or a `linemode` command may use. `modified`
    /// and `created` are yazi's own aliases, kept for the same reason
    /// [`super::sort::SortBy::parse`] keeps them.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "size" => Self::Size,
            "usage" => Self::Usage,
            "mtime" | "modified" => Self::Mtime,
            "btime" | "created" => Self::Btime,
            "permissions" => Self::Permissions,
            "owner" => Self::Owner,
            _ => return None,
        })
    }

    /// Whether the scan worker has to count directory children for this.
    ///
    /// Only `size` shows that count, and counting is a `read_dir` per visible
    /// folder -- so this being wrong is a listing that does needless work, or one
    /// that shows nothing where a number belongs. It was a string comparison
    /// against `"size"` sitting a long way from the mode's own definition.
    pub fn wants_dir_size(self) -> bool {
        matches!(self, Self::Size)
    }
}

/// `Default` so the places that build one can name only the fields they know
/// and let `..Default::default()` carry the rest -- the same shape `Span` uses.
/// Before this, adding a field meant the compiler pointing at four separate
/// constructors, two of them test helpers.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub ext: Option<String>,
    pub kind: Kind,
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub hidden: bool,
    pub readonly: bool,
    /// Where a link points, resolved against its own directory. Read here, on
    /// the scan worker, so following a link never waits on disk in the UI.
    pub link_to: Option<PathBuf>,
    /// Directory child count, filled in lazily by the size/count worker.
    pub dir_size: Option<u64>,
    /// Everything underneath, in bytes, as the usage walk counted it. Only the
    /// disk-usage view sets this: a directory's own `len` is the size of its
    /// entry on disk and `dir_size` counts children one level down, so neither
    /// can answer "how much room does this folder take".
    pub usage: Option<u64>,
    /// The usage walk stopped before the end of this one, so `usage` is a
    /// floor: shown as `≥` when something was counted and `?` when nothing was
    /// -- never as a plain `0 B`, which reads as empty (44.7).
    pub usage_cut: bool,
}

impl Entry {
    pub fn from_dir_entry(de: &std::fs::DirEntry) -> Self {
        let path = de.path();
        let name = util::file_name(&path);
        let ft = de.file_type().ok();
        let md = de.metadata().ok(); // symlink_metadata semantics on Windows/Unix alike
        Self::build(path, name, ft, md)
    }

    /// A directory that no filesystem was asked about.
    ///
    /// For the things that are shown as folders without being files: a
    /// server's shares, which have a name and nothing else — no size, no
    /// dates, no attributes. Reading those would mean a round trip to the
    /// server per share, and the list would still be the same list.
    // Only the share listing builds one, and that is Windows-only.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn directory(path: PathBuf, name: String) -> Self {
        Self { path, name, kind: Kind::Dir, ..Default::default() }
    }

    pub fn from_path(path: PathBuf) -> std::io::Result<Self> {
        let name = util::file_name(&path);
        let md = std::fs::symlink_metadata(&path)?;
        let ft = Some(md.file_type());
        Ok(Self::build(path, name, ft, Some(md)))
    }

    fn build(
        path: PathBuf,
        name: String,
        ft: Option<std::fs::FileType>,
        md: Option<Metadata>,
    ) -> Self {
        let is_symlink = ft.map(|f| f.is_symlink()).unwrap_or(false);
        let mut kind = match ft {
            Some(f) if f.is_dir() => Kind::Dir,
            Some(f) if f.is_symlink() => Kind::Link { to_dir: false, broken: true },
            _ => Kind::File,
        };
        let mut link_to = None;
        if is_symlink {
            match std::fs::metadata(&path) {
                Ok(target) => kind = Kind::Link { to_dir: target.is_dir(), broken: false },
                Err(_) => kind = Kind::Link { to_dir: false, broken: true },
            }
            link_to = std::fs::read_link(&path).ok().map(|t| link_target(&path, t));
        }
        let ext = if kind.is_dir_like() { None } else { util::extension(&name) };
        let (len, modified, created, accessed, hidden, readonly) = match &md {
            Some(md) => (
                md.len(),
                md.modified().ok(),
                md.created().ok(),
                md.accessed().ok(),
                is_hidden(&path, md, &name),
                md.permissions().readonly(),
            ),
            None => (0, None, None, None, name.starts_with('.'), false),
        };
        Self {
            path,
            name,
            ext,
            kind,
            len,
            modified,
            created,
            accessed,
            hidden,
            readonly,
            link_to,
            ..Default::default()
        }
    }

    pub fn is_dir_like(&self) -> bool {
        self.kind.is_dir_like()
    }

    /// Size shown in the list: real length for files, child count for directories.
    pub fn display_size(&self) -> Option<String> {
        if self.is_dir_like() {
            self.dir_size.map(|n| format!("{n}"))
        } else {
            Some(util::human_size(self.len))
        }
    }

    /// What the usage view shows: the measured total where there is one, and the
    /// file's own length otherwise, so a file needs no walk to be counted.
    pub fn usage_bytes(&self) -> u64 {
        self.usage.unwrap_or(if self.is_dir_like() { 0 } else { self.len })
    }
}

/// Absolutize what `read_link` handed back. A relative target is relative to
/// the link's own directory, and a junction's `\\?\` form is not what anyone
/// wants to look at or hand to a shell.
fn link_target(link: &Path, target: PathBuf) -> PathBuf {
    let joined = match link.parent() {
        Some(dir) if target.is_relative() => dir.join(target),
        _ => target,
    };
    util::normalize(&util::unverbatim(&joined))
}

#[cfg(windows)]
fn is_hidden(_path: &Path, md: &Metadata, name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let attrs = md.file_attributes();
    attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 || name.starts_with('.')
}

#[cfg(not(windows))]
fn is_hidden(_path: &Path, _md: &Metadata, name: &str) -> bool {
    name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn link_targets_are_absolute_and_plain() {
        let link = Path::new(r"C:\a\b\shortcut");
        assert_eq!(
            link_target(link, PathBuf::from(r"..\c\file.txt")),
            PathBuf::from(r"C:\a\c\file.txt")
        );
        assert_eq!(
            link_target(link, PathBuf::from(r"\\?\D:\junction")),
            PathBuf::from(r"D:\junction")
        );
        // A verbatim UNC target keeps the spelling Windows gave us.
        assert_eq!(
            link_target(link, PathBuf::from(r"\\?\UNC\host\share")),
            PathBuf::from(r"\\?\UNC\host\share")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn link_targets_are_absolute() {
        let link = Path::new("/a/b/shortcut");
        assert_eq!(
            link_target(link, PathBuf::from("../c/file.txt")),
            PathBuf::from("/a/c/file.txt")
        );
        assert_eq!(link_target(link, PathBuf::from("/d/other")), PathBuf::from("/d/other"));
    }
}
