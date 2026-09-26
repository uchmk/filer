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

use std::collections::HashMap;
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
    /// Draw the shape of the whole file down the right of the text preview.
    pub minimap: bool,
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
            minimap: true,
            preview_debounce_ms: 40,
            max_text_bytes: 256 * 1024,
            max_history: 200,
            window_width: 1360.0,
            window_height: 860.0,
        }
    }
}

/// `[term]`: what the terminal pane runs.
///
/// Empty means the platform's own default, which is what alacritty picks when
/// it is told nothing: `powershell` on Windows -- Windows PowerShell 5.1, not
/// `pwsh` -- and the login shell elsewhere. Those are different programs
/// reading different profiles, so a hook that works in one is simply absent in
/// the other, and there was no way to say which one to start.
#[derive(Deserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct TermCfg {
    /// The program, e.g. `pwsh`. Looked up on `PATH`, or an absolute path.
    #[serde(default)]
    pub shell: String,
    /// Arguments for it, e.g. `["-NoLogo"]`. Ignored without a `shell`.
    #[serde(default)]
    pub args: Vec<String>,
}

/// `[[preview]]`: a command that draws a file filer cannot draw itself.
///
/// The shape is deliberately one thing, not two. A PDF's pages and a video's
/// seconds are the same problem — "give me picture number N of this file" —
/// and a single `{n}` covers both, so paging keys, caching and the "there is
/// no more" case are written once rather than per format.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PreviewRule {
    /// Which files this draws, as a glob over the file name: `*.pdf`,
    /// `*.{mp4,mkv,webm}`.
    #[serde(rename = "match")]
    pub pattern: String,
    /// The command line. `{path}` is the file, `{out}` a path to write a
    /// picture to (with no extension — `pdftoppm` appends its own, and
    /// `ffmpeg` is told `{out}.png`), and `{n}` the page or second wanted.
    pub run: String,
    /// What `{n}` is at the start: page 1 for a document, second 0 for a video.
    #[serde(default = "one")]
    pub first: i64,
    /// How far one press of `<A-j>` moves `{n}`. One page, or ten seconds.
    #[serde(default = "one")]
    pub step: i64,
    /// What the pane writes under the picture. `{n}` is where the number
    /// goes, so `page {n}` reads "page 3" and `{n}s` reads "50s" — a unit
    /// that only ever went in front produced "s 50", which is nobody's idea
    /// of fifty seconds. Without `{n}` it is used as a prefix, and empty
    /// leaves just the number.
    #[serde(default)]
    pub unit: String,
}

fn one() -> i64 {
    1
}

impl PreviewRule {
    /// The first rule whose pattern matches, or nothing.
    ///
    /// First rather than best: the file is read top to bottom and a later
    /// entry overriding an earlier one by being more specific would be a rule
    /// nobody can see in the file itself.
    pub fn for_path<'a>(rules: &'a [Self], path: &std::path::Path) -> Option<&'a Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        rules.iter().find(|r| crate::glob::matches(&r.pattern.to_ascii_lowercase(), &name, true))
    }
}

#[derive(Deserialize, Debug, Default)]
struct FilerToml {
    #[serde(default)]
    ui: Ui,
    #[serde(default)]
    term: TermCfg,
    #[serde(default)]
    preview: Vec<PreviewRule>,
    /// `[line_args]`: editor name (`mikan`, `notepad++`) to the arguments that
    /// open a file at a line, e.g. `"-l {line} {path}"`.
    #[serde(default)]
    line_args: HashMap<String, String>,
}

pub struct Config {
    pub yazi: YaziToml,
    pub keymap: Keymap,
    /// What the terminal pane runs; empty `shell` means the platform default.
    pub term: TermCfg,
    /// Commands that draw what filer cannot, in the order they are tried.
    pub preview: Vec<PreviewRule>,
    pub theme: Theme,
    pub ui: Ui,
    /// Line-jump templates, keyed by [`crate::exec::editor_key`].
    pub line_args: HashMap<String, String>,
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
        let mut term = TermCfg::default();
        let mut preview: Vec<PreviewRule> = Vec::new();
        let mut line_args: HashMap<String, String> = HashMap::new();

        for dir in &dirs {
            if let Some(text) = read(dir, "yazi.toml", &mut loaded) {
                match toml::from_str::<YaziToml>(&text) {
                    Ok(v) => yazi_cfg = merge_yazi(yazi_cfg, v),
                    Err(e) => warnings.push(format!("{}: {e}", at(dir, "yazi.toml"))),
                }
            }
            if let Some(text) = read(dir, "keymap.toml", &mut loaded) {
                keymap_texts.push(text);
            }
            if let Some(text) = read(dir, "theme.toml", &mut loaded) {
                match toml::from_str::<theme::ThemeToml>(&text) {
                    Ok(v) => theme.apply(&v),
                    Err(e) => warnings.push(format!("{}: {e}", at(dir, "theme.toml"))),
                }
            }
            if let Some(text) = read(dir, "filer.toml", &mut loaded) {
                match toml::from_str::<FilerToml>(&text) {
                    Ok(v) => {
                        ui = v.ui;
                        term = v.term;
                        preview = v.preview;
                        for (name, template) in v.line_args {
                            if crate::exec::template_is_valid(&template) {
                                line_args.insert(crate::exec::editor_key(&name), template);
                            } else {
                                warnings.push(format!(
                                    "{}: [line_args] {name}: needs exactly one {{path}}",
                                    at(dir, "filer.toml")
                                ));
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("{}: {e}", at(dir, "filer.toml"))),
                }
            }
        }

        let refs: Vec<&str> = keymap_texts.iter().map(String::as_str).collect();
        let (keymap, mut km_warnings) = Keymap::load(&refs);
        warnings.append(&mut km_warnings);

        Self { yazi: yazi_cfg, keymap, theme, ui, term, preview, line_args, loaded, warnings }
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

/// A file in a config directory, written the way the platform writes a path.
///
/// `format!("{}/{name}", dir.display())` put a forward slash in the middle of a
/// Windows path -- `…\\Roaming\\filer/yazi.toml` -- in the one message whose
/// whole job is to name the file to go and edit.
fn at(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
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

#[cfg(test)]
mod term_shell {
    use super::*;

    /// `[term]` decides what the pane starts, and says nothing by default.
    ///
    /// The default matters as much as the setting. On Windows, telling
    /// alacritty nothing gets `powershell` — Windows PowerShell 5.1, whose
    /// `$PROFILE` is a different file from `pwsh`'s, so a shell hook put in
    /// one is absent in the other with nothing on screen to say why. An empty
    /// `shell` has to keep meaning "the platform's own", because that is what
    /// every existing install already gets.
    #[test]
    fn it_is_absent_until_it_is_asked_for() {
        let none: FilerToml = toml::from_str("[ui]\nfont_size = 14.0\n").unwrap();
        assert_eq!(none.term, TermCfg::default());
        assert!(none.term.shell.is_empty(), "nothing said means the platform default");

        let named: FilerToml = toml::from_str("[term]\nshell = \"pwsh\"\n").unwrap();
        assert_eq!(named.term.shell, "pwsh");
        assert!(named.term.args.is_empty(), "args are optional");

        let with_args: FilerToml =
            toml::from_str("[term]\nshell = \"pwsh\"\nargs = [\"-NoLogo\"]\n").unwrap();
        assert_eq!(with_args.term.args, vec!["-NoLogo".to_string()]);
    }

    /// `[[preview]]` parses, defaults sensibly, and the first match wins.
    #[test]
    fn a_preview_rule_is_read_and_matched() {
        let cfg: FilerToml = toml::from_str(
            "[[preview]]\n\
             match = \"*.pdf\"\n\
             run = 'pdftoppm -f {n} -l {n} {path} {out}'\n\
             first = 1\n\
             unit = \"page\"\n\
             \n\
             [[preview]]\n\
             match = \"*.{mp4,mkv}\"\n\
             run = 'ffmpeg -ss {n} -i {path} {out}.png'\n\
             first = 0\n\
             step = 10\n",
        )
        .unwrap();
        assert_eq!(cfg.preview.len(), 2);
        // `step` defaults to one page; `first` to page one.
        assert_eq!(cfg.preview[0].step, 1);
        assert_eq!(cfg.preview[1].step, 10);
        assert_eq!(cfg.preview[1].first, 0);
        assert_eq!(cfg.preview[1].unit, "", "a unit is optional");

        let m = |p: &str| {
            PreviewRule::for_path(&cfg.preview, std::path::Path::new(p)).map(|r| r.pattern.as_str())
        };
        assert_eq!(m("/a/report.pdf"), Some("*.pdf"));
        // Case does not decide it: `.PDF` is how a download arrives.
        assert_eq!(m("/a/REPORT.PDF"), Some("*.pdf"));
        assert_eq!(m("/a/clip.mkv"), Some("*.{mp4,mkv}"));
        assert_eq!(m("/a/notes.txt"), None, "anything else is filer's own job");
    }

    /// A `filer.toml` written before `[term]` existed still reads.
    #[test]
    fn an_older_config_is_unaffected() {
        let text = std::fs::read_to_string("filer.example.toml").expect("the shipped example");
        let cfg: FilerToml = toml::from_str(&text).expect("the example has to parse");
        assert_eq!(cfg.ui.font_size, 14.0);
    }
}
