//! Deserialization of yazi's `yazi.toml`.
//!
//! Every field is optional and unknown keys are ignored, so a config written
//! for a newer (or older) yazi still loads. Anything this app cannot honor is
//! simply unused rather than an error.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::fs::SortBy;

#[derive(Deserialize, Debug, Default)]
pub struct YaziToml {
    #[serde(default, alias = "manager")]
    pub mgr: Mgr,
    #[serde(default)]
    pub preview: Preview,
    /// Named opener lists, e.g. `edit`, `open`, `play`.
    #[serde(default)]
    pub opener: BTreeMap<String, Vec<Opener>>,
    #[serde(default)]
    pub open: Open,
    #[serde(default)]
    pub tasks: Tasks,
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Mgr {
    /// Width weights of the parent / current / preview columns.
    pub ratio: Vec<u32>,
    pub sort_by: SortBy,
    pub sort_sensitive: bool,
    pub sort_reverse: bool,
    pub sort_dir_first: bool,
    pub linemode: String,
    pub show_hidden: bool,
    pub show_symlink: bool,
    pub scrolloff: u32,
    pub title_format: String,
}

impl Default for Mgr {
    fn default() -> Self {
        Self {
            ratio: vec![1, 4, 3],
            sort_by: SortBy::Natural,
            sort_sensitive: false,
            sort_reverse: false,
            sort_dir_first: true,
            linemode: "none".into(),
            show_hidden: false,
            show_symlink: true,
            scrolloff: 5,
            title_format: "Filer: {cwd}".into(),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Preview {
    pub wrap: String,
    pub tab_size: u8,
    pub max_width: u32,
    pub max_height: u32,
    pub image_filter: String,
    pub image_quality: u8,
    pub image_delay: u32,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            wrap: "no".into(),
            tab_size: 2,
            max_width: 900,
            max_height: 900,
            image_filter: "triangle".into(),
            image_quality: 75,
            image_delay: 30,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Opener {
    pub run: String,
    #[serde(default)]
    pub block: bool,
    #[serde(default)]
    pub orphan: bool,
    #[serde(default)]
    pub desc: Option<String>,
    /// `"windows"`, `"unix"`, `"macos"` or absent (any platform).
    #[serde(default, rename = "for")]
    pub platform: Option<String>,
}

impl Opener {
    pub fn matches_platform(&self) -> bool {
        match self.platform.as_deref() {
            None => true,
            Some("windows") => cfg!(windows),
            Some("macos") => cfg!(target_os = "macos"),
            Some("unix") | Some("linux") => cfg!(unix),
            Some(_) => false,
        }
    }

    pub fn label(&self) -> String {
        self.desc.clone().unwrap_or_else(|| self.run.clone())
    }
}

#[derive(Deserialize, Debug, Default)]
pub struct Open {
    #[serde(default)]
    pub rules: Vec<OpenRule>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OpenRule {
    /// Glob against the file name; a trailing `/` means "directories only".
    #[serde(default)]
    pub name: Option<String>,
    /// Glob against the guessed mime type.
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(rename = "use")]
    pub use_: super::StrOrVec,
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Tasks {
    pub micro_workers: u32,
    pub macro_workers: u32,
}

impl Default for Tasks {
    fn default() -> Self {
        Self {
            micro_workers: 10,
            macro_workers: 10,
        }
    }
}
