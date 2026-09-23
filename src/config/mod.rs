//! Configuration: yazi's own files first, then this app's optional overrides.
//!
//! Search order (later wins):
//!   1. `$YAZI_CONFIG_HOME` or `%APPDATA%/yazi/config`  — `yazi.toml`, `keymap.toml`, `theme.toml`
//!   2. `$FILER_CONFIG_HOME` or `%APPDATA%/filer`       — the same three, plus `filer.toml`

pub mod cmd;
pub mod keymap;
pub mod keys;
pub mod theme;
pub mod yazi;

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub use keymap::Keymap;
pub use theme::Theme;
pub use yazi::YaziToml;

/// A TOML value that may be written either as a string or as an array of strings.
#[derive(Debug, Clone, Default)]
pub struct StrOrVec(pub Vec<String>);

impl<'de> Deserialize<'de> for StrOrVec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            One(String),
            Many(Vec<String>),
        }
        Ok(match Helper::deserialize(d)? {
            Helper::One(s) => StrOrVec(vec![s]),
            Helper::Many(v) => StrOrVec(v),
        })
    }
}

/// GUI-only settings, from `filer.toml`. yazi has no equivalent for these.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Ui {
    pub font_size: f32,
    /// Extra vertical padding per row, in points.
    pub row_padding: f32,
    /// Explicit font file paths, tried in order before the built-in list.
    pub fonts: Vec<String>,
    /// Bold faces, tried before the `-Bold` siblings of whatever `fonts`
    /// resolved to. Without any, bold is faked by overstriking.
    pub bold_fonts: Vec<String>,
    /// Show Markdown laid out for reading (with an outline) instead of as
    /// highlighted source; `toggle_render` flips it at runtime.
    pub render_markdown: bool,
    /// `auto`, `nerd`, `ascii` or `none`.
    pub icons: String,
    pub animations: bool,
    /// Milliseconds the cursor must rest before a preview is requested.
    pub preview_debounce_ms: u64,
    pub max_text_bytes: usize,
    /// Directories remembered for `z` (jump).
    pub max_history: usize,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            row_padding: 4.0,
            fonts: Vec::new(),
            bold_fonts: Vec::new(),
            render_markdown: true,
            icons: "auto".into(),
            animations: false,
            preview_debounce_ms: 40,
            max_text_bytes: 256 * 1024,
            max_history: 200,
            window_width: 1360.0,
            window_height: 860.0,
        }
    }
}

#[derive(Deserialize, Debug, Default)]
struct FilerToml {
    #[serde(default)]
    ui: Ui,
}

pub struct Config {
    pub yazi: YaziToml,
    pub keymap: Keymap,
    pub theme: Theme,
    pub ui: Ui,
    /// Config files that were actually read, for the help panel.
    pub loaded: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

impl Config {
    pub fn load() -> Self {
        let mut warnings = Vec::new();
        let mut loaded = Vec::new();

        let dirs = config_dirs();
        let mut yazi_cfg = YaziToml::default();
        let mut keymap_texts: Vec<String> = Vec::new();
        let mut theme = Theme::default();
        let mut ui = Ui::default();

        for dir in &dirs {
            if let Some(text) = read(dir, "yazi.toml", &mut loaded) {
                match toml::from_str::<YaziToml>(&text) {
                    Ok(v) => yazi_cfg = merge_yazi(yazi_cfg, v),
                    Err(e) => warnings.push(format!("{}/yazi.toml: {e}", dir.display())),
                }
            }
            if let Some(text) = read(dir, "keymap.toml", &mut loaded) {
                keymap_texts.push(text);
            }
            if let Some(text) = read(dir, "theme.toml", &mut loaded) {
                match toml::from_str::<theme::ThemeToml>(&text) {
                    Ok(v) => theme.apply(&v),
                    Err(e) => warnings.push(format!("{}/theme.toml: {e}", dir.display())),
                }
            }
            if let Some(text) = read(dir, "filer.toml", &mut loaded) {
                match toml::from_str::<FilerToml>(&text) {
                    Ok(v) => ui = v.ui,
                    Err(e) => warnings.push(format!("{}/filer.toml: {e}", dir.display())),
                }
            }
        }

        let refs: Vec<&str> = keymap_texts.iter().map(String::as_str).collect();
        let (keymap, mut km_warnings) = Keymap::load(&refs);
        warnings.append(&mut km_warnings);

        Self { yazi: yazi_cfg, keymap, theme, ui, loaded, warnings }
    }

    pub fn state_dir() -> PathBuf {
        if let Ok(p) = std::env::var("FILER_STATE_HOME") {
            return PathBuf::from(p);
        }
        dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("filer")
    }
}

/// Later directories override earlier ones.
pub fn config_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    match std::env::var_os("YAZI_CONFIG_HOME") {
        Some(p) => out.push(PathBuf::from(p)),
        None => {
            if let Some(c) = dirs::config_dir() {
                out.push(c.join("yazi").join("config"));
            }
        }
    }
    match std::env::var_os("FILER_CONFIG_HOME") {
        Some(p) => out.push(PathBuf::from(p)),
        None => {
            if let Some(c) = dirs::config_dir() {
                out.push(c.join("filer"));
            }
        }
    }
    out
}

fn read(dir: &Path, name: &str, loaded: &mut Vec<PathBuf>) -> Option<String> {
    let p = dir.join(name);
    let text = std::fs::read_to_string(&p).ok()?;
    loaded.push(p);
    Some(text)
}

/// A later `yazi.toml` replaces only the tables it actually defines. TOML gives
/// us no "was it present" signal for `#[serde(default)]` fields, so this keeps
/// list-shaped settings (openers, open rules) additive and lets scalars win.
fn merge_yazi(base: YaziToml, mut next: YaziToml) -> YaziToml {
    for (k, v) in base.opener {
        next.opener.entry(k).or_insert(v);
    }
    if next.open.rules.is_empty() {
        next.open.rules = base.open.rules;
    }
    next
}
