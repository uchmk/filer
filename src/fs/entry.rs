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
        if is_symlink {
            match std::fs::metadata(&path) {
                Ok(target) => kind = Kind::Link { to_dir: target.is_dir(), broken: false },
                Err(_) => kind = Kind::Link { to_dir: false, broken: true },
            }
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
