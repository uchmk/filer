use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::util;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Dir,
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

#[derive(Clone, Debug)]
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
}

impl Entry {
    pub fn from_dir_entry(de: &std::fs::DirEntry) -> Self {
        let path = de.path();
        let name = util::file_name(&path);
        let ft = de.file_type().ok();
        let md = de.metadata().ok(); // symlink_metadata semantics on Windows/Unix alike
        Self::build(path, name, ft, md)
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
            dir_size: None,
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
