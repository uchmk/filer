//! Configuration: yazi's own files first, then this app's optional overrides.
//!
//! Search order (later wins):
//!   1. `$YAZI_CONFIG_HOME`, else yazi's own directory — `yazi.toml`, `keymap.toml`, `theme.toml`
//!   2. `$FILER_CONFIG_HOME`, else `<base>/filer`      — the same three, plus `filer.toml`
//!
//! `<base>` is `%APPDATA%` on Windows and `$XDG_CONFIG_HOME` (default `~/.config`)
//! on Unix, macOS included. Layer 1 is `<base>/yazi/config` on Windows but
//! `<base>/yazi` on Unix, because that is where yazi itself keeps them.

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
    /// What draws the window: `auto` (GL on Windows when the machine has it,
    /// else wgpu's pick), `vulkan`, `dx12`, `metal` or `gl`. `WGPU_BACKEND` overrides it for one run (Q70: an AMD
    /// driver thread keeps a core busy under Vulkan and DX12 while filer
    /// sits idle, and only GL stops it, #204, #207, #232).
    pub backend: String,
}

impl Ui {
    /// `[ui] backend` as the wgpu name it stands for: `None` for `auto` (or
    /// empty), an error naming the value for anything else unknown.
    pub fn backend_name(&self) -> Result<Option<&'static str>, String> {
        let name = match self.backend.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => return Ok(None),
            "vulkan" | "vk" => "vulkan",
            "dx12" | "d3d12" => "dx12",
            "metal" | "mtl" => "metal",
            "gl" | "opengl" | "gles" => "gl",
            _ => {
                return Err(format!(
                    "[ui] backend = \"{}\" is not one of auto, vulkan, dx12, metal, gl; drawing with the default",
                    self.backend
                ))
            }
        };
        // Two of them exist on one platform only, which needs no adapter
        // search to say: "no adapter for it" read as "install a driver" (#239).
        let only = match name {
            "metal" if !cfg!(target_os = "macos") => Some("macOS"),
            "dx12" if !cfg!(windows) => Some("Windows"),
            _ => None,
        };
        match only {
            Some(os) => Err(format!("[ui] backend = \"{name}\" is {os} only; drawing with the default")),
            None => Ok(Some(name)),
        }
    }
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
            backend: "auto".into(),
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
    /// The shell came from `FILER_TERM_SHELL`, not from a file.
    #[serde(skip)]
    pub from_env: bool,
    /// The args came from `FILER_TERM_ARGS` (Q81).
    #[serde(skip)]
    pub args_from_env: bool,
    /// `FILER_TERM_ARGS` was set but `FILER_TERM_SHELL` was not, so it counted
    /// for nothing; `filer env` says so (#267).
    #[serde(skip)]
    pub args_unused: bool,
    /// The `[term] args` that `FILER_TERM_SHELL` set aside, for `filer env`
    /// to name: without it, seeing them gone took the process's command line
    /// (#190).
    #[serde(skip)]
    pub dropped_args: Vec<String>,
}

impl TermCfg {
    /// `FILER_TERM_SHELL` names the pane's shell for this run, over
    /// `[term] shell` (Q51). Checking a row in another shell meant pointing
    /// `FILER_CONFIG_HOME` at an empty folder and losing every other setting
    /// with it (#176). The whole value is the program, a path with spaces
    /// included, and `[term] args` are dropped: they were written for the
    /// shell this one replaces.
    ///
    /// `FILER_TERM_ARGS` gives that shell its arguments (Q81): words split at
    /// white space, with `"…"` keeping its spaces together. It counts only
    /// beside `FILER_TERM_SHELL`, since it is written for that shell.
    fn take_env(&mut self, var: Option<std::ffi::OsString>, args: Option<std::ffi::OsString>) {
        let Some(shell) = var.and_then(|v| v.into_string().ok()).filter(|v| !v.trim().is_empty()) else {
            self.args_unused = args.and_then(|v| v.into_string().ok()).is_some_and(|v| !v.trim().is_empty());
            return;
        };
        let dropped_args = std::mem::take(&mut self.args);
        let args = args.and_then(|v| v.into_string().ok()).map(|v| split_words(&v)).unwrap_or_default();
        *self = Self { shell: shell.trim().to_owned(), args_from_env: !args.is_empty(), args_unused: false, args, from_env: true, dropped_args };
    }
}

/// `-NoProfile "-Command x y"` as words: split at white space, and a quoted
/// run keeps its spaces (the quotes themselves go).
fn split_words(text: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted, mut started) = (Vec::new(), String::new(), false, false);
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
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
    /// Behind an `Arc` because every draw function starts by taking a copy to
    /// get out of the borrow checker's way, and the theme carries the icon and
    /// filetype tables -- a hundred-odd `String`s by default, more from a real
    /// `theme.toml`. Cloning that six times a frame was several hundred heap
    /// allocations per frame to read some colours. Nothing mutates it after
    /// `load` builds it, so sharing costs nothing.
    pub theme: std::sync::Arc<Theme>,
    pub ui: Ui,
    /// Line-jump templates, keyed by [`crate::exec::editor_key`].
    pub line_args: HashMap<String, String>,
    /// Config files that were actually read, for the help panel.
    pub loaded: Vec<PathBuf>,
    /// Those of them none of whose settings took effect: a parse error, or a
    /// section in the other file's shape that stops the whole file parsing.
    /// The panel marks them, as it marks a file not read yet; listed plainly
    /// beside the warning that said nothing was read, the panel said both
    /// (#203).
    pub unread: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

/// Which kinds of file failed to parse in one read, and where their messages
/// are in `warnings` -- what [`Config::reload`] needs to keep the last good
/// values in their place.
#[derive(Default)]
struct Broken {
    yazi: bool,
    keymap: bool,
    theme: bool,
    filer: bool,
    /// Indices into `warnings` of the parse errors, to say what was kept.
    said: Vec<usize>,
}

impl Broken {
    fn any(&self) -> bool {
        self.yazi || self.keymap || self.theme || self.filer
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = Self::read(&dirs_to_read()).0;
        cfg.term.take_env(std::env::var_os("FILER_TERM_SHELL"), std::env::var_os("FILER_TERM_ARGS"));
        cfg
    }

    /// `<C-F5>`: read the files again, but a file that no longer parses keeps
    /// what it gave last time rather than falling back to the defaults (Q47).
    /// The reload is pressed while a file is being edited, which is exactly
    /// when it is most likely to be half-written; a `[ui] font_size` that
    /// jumped back to 14 at every typo made the edit impossible to check (#171).
    /// At start there is nothing to keep, and a broken file means defaults.
    ///
    /// `prev` is emptied of what is kept: it is the config being replaced.
    pub fn reload(prev: &mut Config) -> Self {
        let mut cfg = Self::reload_from(prev, &dirs_to_read());
        cfg.term.take_env(std::env::var_os("FILER_TERM_SHELL"), std::env::var_os("FILER_TERM_ARGS"));
        cfg
    }

    fn reload_from(prev: &mut Config, dirs: &[PathBuf]) -> Self {
        let (mut cfg, broken) = Self::read(dirs);
        if !broken.any() {
            return cfg;
        }
        use std::mem::take;
        if broken.yazi {
            cfg.yazi = take(&mut prev.yazi);
        }
        if broken.keymap {
            cfg.keymap = take(&mut prev.keymap);
        }
        if broken.theme {
            cfg.theme = prev.theme.clone();
        }
        if broken.filer {
            cfg.ui = take(&mut prev.ui);
            cfg.term = take(&mut prev.term);
            cfg.preview = take(&mut prev.preview);
            cfg.line_args = take(&mut prev.line_args);
        }
        for &i in &broken.said {
            cfg.warnings[i] += "\n(the last settings read from it stay in force until it parses again)";
        }
        cfg
    }

    fn read(dirs: &[PathBuf]) -> (Self, Broken) {
        let mut broken = Broken::default();
        let mut warnings = Vec::new();
        let mut loaded = Vec::new();
        let mut unread = Vec::new();

        let mut yazi_cfg = YaziToml::default();
        let mut keymap_texts: Vec<(String, String)> = Vec::new();
        let mut theme = Theme::default();
        let mut ui = Ui::default();
        let mut term = TermCfg::default();
        let mut preview: Vec<PreviewRule> = Vec::new();
        let mut line_args: HashMap<String, String> = HashMap::new();

        for dir in dirs {
            if let Some(text) = read(dir, "yazi.toml", &mut loaded) {
                let wrong = Misplaced::in_file(&text, ConfigFile::Yazi);
                wrong.warn(&at(dir, "yazi.toml"), "filer.toml", &mut warnings);
                match toml::from_str::<YaziToml>(&text) {
                    Ok(v) => yazi_cfg = merge_yazi(yazi_cfg, v),
                    // The serde message for a `preview` in the wrong shape is
                    // "invalid type: map, expected a string", pointing at a line
                    // whose `[[preview]]` is spelled exactly as its own
                    // documentation spells it. `wrong` has already said which
                    // key it is and which file it goes in.
                    Err(_) if wrong.breaks_parse => {
                        broken.yazi = true;
                        unread.push(dir.join("yazi.toml"));
                    }
                    Err(e) => {
                        broken.yazi = true;
                        unread.push(dir.join("yazi.toml"));
                        broken.said.push(warnings.len());
                        warnings.push(format!("{}: {}", at(dir, "yazi.toml"), parse_error(&text, &e)));
                    }
                }
            }
            if let Some(text) = read(dir, "keymap.toml", &mut loaded) {
                if !Keymap::parses(&text) {
                    unread.push(dir.join("keymap.toml"));
                }
                keymap_texts.push((at(dir, "keymap.toml"), text));
            }
            if let Some(text) = read(dir, "theme.toml", &mut loaded) {
                match toml::from_str::<theme::ThemeToml>(&text) {
                    Ok(v) => theme.apply(&v),
                    Err(e) => {
                        broken.theme = true;
                        unread.push(dir.join("theme.toml"));
                        broken.said.push(warnings.len());
                        warnings.push(format!("{}: {}", at(dir, "theme.toml"), parse_error(&text, &e)));
                    }
                }
            }
            if let Some(text) = read(dir, "filer.toml", &mut loaded) {
                let wrong = Misplaced::in_file(&text, ConfigFile::Filer);
                wrong.warn(&at(dir, "filer.toml"), "yazi.toml", &mut warnings);
                match toml::from_str::<FilerToml>(&text) {
                    Ok(v) => {
                        // Here rather than at start-up only, so `filer env`
                        // says it too: it printed `Warnings : none` while the
                        // window warned (#239, #240).
                        if let Err(e) = v.ui.backend_name() {
                            warnings.push(format!("{}: {e}", at(dir, "filer.toml")));
                        }
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
                    Err(_) if wrong.breaks_parse => {
                        broken.filer = true;
                        unread.push(dir.join("filer.toml"));
                    }
                    Err(e) => {
                        broken.filer = true;
                        unread.push(dir.join("filer.toml"));
                        broken.said.push(warnings.len());
                        warnings.push(format!("{}: {}", at(dir, "filer.toml"), parse_error(&text, &e)));
                    }
                }
            }
        }

        let refs: Vec<(&str, &str)> = keymap_texts.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
        // `load_named` puts the parse errors first among its warnings, one per
        // file that failed, in file order.
        let km_broken = refs.iter().filter(|(_, t)| !Keymap::parses(t)).count();
        broken.keymap = km_broken > 0;
        broken.said.extend(warnings.len()..warnings.len() + km_broken);
        let (keymap, mut km_warnings) = Keymap::load_named(&refs);
        warnings.append(&mut km_warnings);

        let theme = std::sync::Arc::new(theme);
        (Self { yazi: yazi_cfg, keymap, theme, ui, term, preview, line_args, loaded, unread, warnings }, broken)
    }

    pub fn state_dir() -> PathBuf {
        if let Ok(p) = std::env::var("FILER_STATE_HOME") {
            return PathBuf::from(p);
        }
        dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("filer")
    }
}

/// The directory both config layers sit under, when no variable overrides them.
///
/// `dirs::config_dir()` is right on Windows (`%APPDATA%`) but not on macOS, where
/// it answers `~/Library/Application Support` -- and yazi does not keep its config
/// there. yazi uses XDG on every Unix, macOS included, so following `dirs` there
/// had filer hunting for a `yazi.toml` in a directory yazi never writes to. Since
/// filer's whole premise is reading yazi's own files, it has to look where yazi
/// put them.
#[cfg(windows)]
fn base_config_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

#[cfg(not(windows))]
fn base_config_dir() -> Option<PathBuf> {
    // `dirs::config_dir()` honors `XDG_CONFIG_HOME` on Linux but not on macOS,
    // so read it here to get the same answer on both.
    xdg_base(std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from), dirs::home_dir())
}

/// Split out from `base_config_dir` so the rules can be tested without setting a
/// variable the rest of the test binary shares.
#[cfg(not(windows))]
fn xdg_base(var: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    match var {
        // XDG says a relative value is to be ignored, as yazi's own reader does.
        // An empty value arrives as `Some("")`, which is not absolute either.
        Some(p) if p.is_absolute() => Some(p),
        _ => home.map(|h| h.join(".config")),
    }
}

/// Where yazi's own three files live: `…/yazi/config` on Windows, `…/yazi` on Unix.
///
/// The trailing `config` is a Windows-only quirk of yazi's layout, not part of the
/// name, so it cannot be appended unconditionally.
fn yazi_config_dir(base: &Path) -> PathBuf {
    let dir = base.join("yazi");
    if cfg!(windows) { dir.join("config") } else { dir }
}

/// The two variables that name a config directory, in search order.
pub const CONFIG_VARS: [&str; 2] = ["YAZI_CONFIG_HOME", "FILER_CONFIG_HOME"];

/// Where one of [`CONFIG_VARS`] points when it is not set — which is the usual case.
///
/// Exposed so a path can be written `%FILER_CONFIG_HOME%` and still resolve on a
/// machine where nobody set it. The `gc` and `gy` keys used to hardcode
/// `%APPDATA%/filer` and `%APPDATA%/yazi/config`, which named nothing outside
/// Windows: an unset `%VAR%` expands to the empty string, so `gc` walked to
/// `/filer`. Returns `None` for any other name, leaving ordinary variables alone.
pub fn config_dir_default(var: &str) -> Option<PathBuf> {
    let base = base_config_dir()?;
    match var {
        "YAZI_CONFIG_HOME" => Some(yazi_config_dir(&base)),
        "FILER_CONFIG_HOME" => Some(base.join("filer")),
        _ => None,
    }
}

/// The directory a config variable names, set or not.
///
/// An empty value counts as unset, the way an exported-but-blank variable is
/// meant in a shell, rather than resolving to the filesystem root.
pub fn config_home(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => config_dir_default(var),
    }
}

/// Later directories override earlier ones.
pub fn config_dirs() -> Vec<PathBuf> {
    distinct(CONFIG_VARS.iter().filter_map(|v| config_home(v)).collect())
}

/// The directories, each once. With `YAZI_CONFIG_HOME` and `FILER_CONFIG_HOME`
/// naming one folder, its `filer.toml` was read twice, the help panel and
/// `filer env` listed the folder twice, and `<C-F5>` counted every file double
/// (#180). Every layer reads the same files, so the first of two is enough.
/// Two spellings of one folder (`C:\cfg` and `c:\cfg\`) are one folder too.
fn distinct(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for dir in dirs {
        let key = comparable(&dir);
        if !seen.contains(&key) {
            seen.push(key);
            out.push(dir);
        }
    }
    out
}

/// A folder as it is on disk when it is there, and with its `.` and trailing
/// separators dropped when it is not. Letter case does not tell two Windows
/// folders apart.
fn comparable(dir: &Path) -> PathBuf {
    let p = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.components().collect());
    if cfg!(windows) { PathBuf::from(p.to_string_lossy().to_lowercase()) } else { p }
}

/// The directories [`Config::load`] actually opens files in.
///
/// Separate from [`config_dirs`] because the two answer different questions.
/// `config_dirs` says *where filer looks*, and the help panel and `filer env`
/// print that whether or not anything is there -- so a test about the listing
/// still wants the real answer. This says *what gets read*, and under
/// `cfg(test)` that is nothing: the state every test was written against.
///
/// Until v0.47.27 both were the same function, so `cargo test` read whoever
/// ran it's `%APPDATA%`: a `[[preview]]` naming `pdftoppm`, or a `keymap.toml`
/// moving `j`, failed five tests that passed everywhere else. CI has no config
/// and neither does a cloud container, so **the only machines that could see
/// it were the ones nobody ran the suite on.**
///
/// No knob to point it somewhere else, deliberately. Test config would have to
/// live in a `static`, and a process-wide first-wins one that a single test
/// set would change what every other test running beside it reads -- which is
/// the objection to `std::env::set_var` here, not an improvement on it. The
/// day a test needs to load a config file, it can take the directory as an
/// argument instead.
#[cfg(test)]
fn dirs_to_read() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(test))]
fn dirs_to_read() -> Vec<PathBuf> {
    config_dirs()
}

/// A file in a config directory, written the way the platform writes a path.
///
/// `format!("{}/{name}", dir.display())` put a forward slash in the middle of a
/// Windows path -- `…\\Roaming\\filer/yazi.toml` -- in the one message whose
/// whole job is to name the file to go and edit.
fn at(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
}

/// Every file name either config directory is searched for.
///
/// Shared so the help panel and `filer env` cannot come to different answers
/// about what is on disk -- which is the confusion they exist to settle.
pub const FILES: [&str; 4] = ["yazi.toml", "keymap.toml", "theme.toml", "filer.toml"];

/// Which of the two config files is being read.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ConfigFile {
    /// yazi's own: keys, colors, openers, sorting.
    Yazi,
    /// This app's: `[ui]`, `[term]`, `[[preview]]`, `[line_args]`.
    Filer,
}

/// Sections written into one config file that the other one reads.
///
/// Both files ignore keys they do not know, which is what lets a config written
/// for another version of yazi still load -- and also what made putting a
/// section in the wrong half of the pair silent. `[term] shell = "pwsh"` in
/// `yazi.toml` left the terminal pane on the platform default with nothing on
/// screen to say why, and `filer env` reported the shell it was not using.
///
/// `preview` is the one name both files use, for different things: yazi's
/// `[preview]` is a table of sizes and filters, filer's `[[preview]]` an array
/// of commands. So the shape decides which file a `preview` belongs to, and
/// getting it wrong is worse than ignored -- it fails the whole file, taking
/// every opener in it along, under a type error that names neither the section
/// nor the file that wants it.
#[derive(Debug, Default, PartialEq, Eq)]
struct Misplaced {
    /// The sections, spelled the way TOML spells them: `[term]`, `[[preview]]`.
    sections: Vec<String>,
    /// Whether one of them is a `preview` in the other file's shape, which does
    /// not merely go unread: nothing in the file is read.
    breaks_parse: bool,
}

impl Misplaced {
    fn in_file(text: &str, file: ConfigFile) -> Self {
        // Only a well-formed file is worth inspecting. A syntax error is the one
        // case where the parser's own message, with its line and column, is the
        // better one, so leave it to say so.
        let Ok(table) = text.parse::<toml::Table>() else { return Self::default() };

        let foreign: &[&str] = match file {
            ConfigFile::Yazi => &["ui", "term", "line_args"],
            ConfigFile::Filer => &["mgr", "manager", "opener", "open", "tasks"],
        };
        let mut out = Self::default();
        for key in foreign.iter().filter(|k| table.contains_key(**k)) {
            out.sections.push(format!("[{key}]"));
        }
        // An array of tables here, a table there; each file's own shape is the
        // one it does not complain about.
        match table.get("preview") {
            Some(v) if file == ConfigFile::Yazi && v.is_array() => {
                out.sections.push("[[preview]]".into());
                out.breaks_parse = true;
            }
            Some(v) if file == ConfigFile::Filer && v.is_table() => {
                out.sections.push("[preview]".into());
                out.breaks_parse = true;
            }
            _ => {}
        }
        out
    }

    /// One line per section: which one, where it goes, and what it cost.
    fn warn(&self, path: &str, other: &str, warnings: &mut Vec<String>) {
        let cost = match self.breaks_parse {
            true => ", and nothing in this file was read",
            false => " and was ignored",
        };
        for s in &self.sections {
            warnings.push(format!("{path}: {s} belongs in {other}{cost}"));
        }
    }
}

/// A TOML parse error as the warnings show it, with a hint when the error is a
/// backslash inside a "double-quoted" string.
///
/// `run = "C:\Users\me\nvim.exe"` is the commonest way to break a config on
/// Windows, and the parser's own message -- "too few unicode value digits" or
/// "missing escaped value" -- names the escape rules, not the fix.
pub(crate) fn parse_error(text: &str, e: &toml::de::Error) -> String {
    let mut said = e.to_string().trim_end().to_string();
    // The span lands on the backslash, just after it, or -- for `\U` with too
    // few hex digits -- up to nine characters on, still inside the escape.
    let at_escape = e.span().is_some_and(|s| {
        let Some(before) = text.get(..s.start) else { return false };
        let escape = before.rfind('\\').map(|i| &before[i + 1..]);
        escape.is_some_and(|tail| tail.len() <= 9 && tail.chars().all(|c| c.is_ascii_alphanumeric()))
            || text.get(s.start..).is_some_and(|a| a.starts_with('\\'))
    });
    if at_escape {
        said += "\n(a backslash in \"double quotes\" starts an escape: write a Windows path in 'single quotes')";
    }
    said
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
    // The later file's additions are nearer: its `prepend_rules` before the
    // earlier one's, its `append_rules` after them.
    next.open.prepend_rules.extend(base.open.prepend_rules);
    let mut append = base.open.append_rules;
    append.append(&mut next.open.append_rules);
    next.open.append_rules = append;
    next
}

/// Where the two config layers are looked for.
#[cfg(test)]
mod dirs_tests {
    use super::*;

    /// The trailing `config` belongs to yazi's Windows layout only. Appending it
    /// everywhere sent filer to `~/.config/yazi/config/yazi.toml`, which yazi
    /// never writes -- so a Linux user's existing yazi config went unread.
    /// The suite reads no config of its own, whatever machine it runs on.
    ///
    /// Until v0.47.27 it read whoever ran it's, so a `[[preview]]` naming
    /// `pdftoppm` or a `keymap.toml` moving `j` failed five tests that passed
    /// everywhere else. CI has no config and neither does a cloud container,
    /// so the only machines that could see it were the ones nobody ran the
    /// suite on -- and when two of them finally did, the answer on offer was
    /// "five failures are normal here", which is how a sixth goes unnoticed.
    ///
    /// Asserted through `Config::load` rather than on `dirs_to_read` alone, so
    /// that routing `load` back to the ambient directories fails here too.
    #[test]
    fn the_suite_never_reads_the_machines_own_config() {
        assert!(dirs_to_read().is_empty(), "no directory is opened under cfg(test)");
        let cfg = Config::load();
        assert!(cfg.loaded.is_empty(), "so nothing was loaded: {:?}", cfg.loaded);
        assert!(cfg.warnings.is_empty(), "and nothing complained: {:?}", cfg.warnings);
        // `config_dirs` is the other question -- where filer *looks* -- and the
        // help panel and `filer env` print it whether or not anything is there.
        assert!(!config_dirs().is_empty(), "which is still answered");
    }

    #[test]
    fn the_yazi_layer_follows_yazis_own_layout() {
        let got = yazi_config_dir(Path::new("/base"));
        if cfg!(windows) {
            assert_eq!(got, Path::new("/base").join("yazi").join("config"));
        } else {
            assert_eq!(got, Path::new("/base").join("yazi"));
        }
    }

    /// filer's own layer has no such quirk: it is `<base>/filer` on every platform.
    ///
    /// Reads the ambient environment, so it steps aside when either variable is
    /// set rather than making the suite depend on how it was launched.
    #[test]
    fn the_filer_layer_is_one_level_down_everywhere() {
        let overridden = std::env::var_os("YAZI_CONFIG_HOME").is_some()
            || std::env::var_os("FILER_CONFIG_HOME").is_some();
        if overridden || base_config_dir().is_none() {
            return;
        }
        let dirs = config_dirs();
        assert_eq!(dirs.len(), 2, "both layers are always listed: {dirs:?}");
        assert_eq!(dirs[1].file_name().unwrap(), "filer");
        assert_ne!(dirs[0], dirs[1], "a layer that overrode itself would merge nothing");
    }

    /// #180: one folder named by both variables is read once, however it is
    /// spelled, and two folders stay two.
    #[test]
    fn a_folder_named_twice_is_read_once() {
        let root = crate::util::test_dir("cfgtwice");
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let spelled = PathBuf::from(format!("{}{}", a.join(".").display(), std::path::MAIN_SEPARATOR));
        assert_eq!(distinct(vec![a.clone(), spelled]), vec![a.clone()]);
        assert_eq!(distinct(vec![a.clone(), b.clone()]), vec![a.clone(), b]);
        let gone = root.join("missing");
        assert_eq!(distinct(vec![gone.clone(), gone.join(".")]), vec![gone], "not there yet is still one folder");
        #[cfg(windows)]
        assert_eq!(distinct(vec![a.clone(), PathBuf::from(a.display().to_string().to_uppercase())]), vec![a]);
    }

    /// `XDG_CONFIG_HOME` is only honored when absolute, and an unset or empty
    /// value falls back to `~/.config` rather than dropping the layer entirely.
    #[cfg(not(windows))]
    #[test]
    fn a_relative_xdg_config_home_is_ignored() {
        let home = Some(PathBuf::from("/home/u"));
        let dotconfig = Some(PathBuf::from("/home/u/.config"));
        assert_eq!(xdg_base(Some(PathBuf::from("/xdg")), home.clone()), Some(PathBuf::from("/xdg")));
        assert_eq!(xdg_base(Some(PathBuf::from("relative")), home.clone()), dotconfig);
        assert_eq!(xdg_base(Some(PathBuf::from("")), home.clone()), dotconfig);
        assert_eq!(xdg_base(None, home), dotconfig);
        assert_eq!(xdg_base(None, None), None, "no home means no default to offer");
    }
}

/// The two config files, and what each one is allowed to say.
#[cfg(test)]
mod line_mode_from_the_config {
    use super::*;

    /// `[mgr] linemode` is a known set of names now, so a typo in `yazi.toml` is
    /// rejected with a warning rather than quietly leaving the column blank.
    #[test]
    fn a_known_name_parses_and_a_typo_does_not() {
        use crate::fs::entry::Linemode as L;
        let ok: yazi::YaziToml =
            toml::from_str("[mgr]\nlinemode = \"permissions\"\n").expect("a known name");
        assert_eq!(ok.mgr.linemode, L::Permissions);

        // yazi's alias, which `SortBy` keeps for the same reason.
        let alias: yazi::YaziToml =
            toml::from_str("[mgr]\nlinemode = \"modified\"\n").expect("an alias");
        assert_eq!(alias.mgr.linemode, L::Mtime);

        let bad = toml::from_str::<yazi::YaziToml>("[mgr]\nlinemode = \"mtiem\"\n");
        assert!(bad.is_err(), "a typo is an error, which the loader reports as a warning");
    }
}

#[cfg(test)]
mod shared_theme {
    use super::*;

    /// Every draw function opens with `app.cfg.theme.clone()` to get out of the
    /// borrow checker's way, six times a frame. The theme carries the icon and
    /// filetype tables, so while it was a plain `Theme` each of those copies
    /// re-allocated every `String` in them -- several hundred allocations a
    /// frame, to read some colours. Behind an `Arc` the copy is a refcount bump.
    ///
    /// This fails to compile rather than fails to assert if the `Arc` is taken
    /// away again, which is the point of writing it.
    #[test]
    fn a_draw_function_shares_the_theme_rather_than_copying_it() {
        let cfg = Config::load();
        let one = cfg.theme.clone();
        let two = cfg.theme.clone();
        assert!(std::sync::Arc::ptr_eq(&one, &two), "both copies are the same theme");

        // And the tables are large enough for the difference to matter -- this
        // is the default, before anyone's own `theme.toml` adds to them.
        let entries = one.filetypes.len()
            + one.icon_dirs.len()
            + one.icon_exts.len()
            + one.icon_files.len()
            + one.icon_globs.len();
        assert!(entries > 50, "the default theme carries {entries} entries, each with a String");
    }
}

#[cfg(test)]
mod files {
    use super::*;

    /// Q51: `FILER_TERM_SHELL` wins over `[term]`, drops the args written for
    /// the other shell, and an empty or blank value is no value.
    #[test]
    fn the_variable_names_the_shell_for_one_run() {
        let file = || TermCfg { shell: "pwsh".into(), args: vec!["-NoLogo".into()], ..TermCfg::default() };
        let mut t = file();
        t.take_env(Some(r"C:\Program Files\Git\bin\bash.exe ".into()), None);
        assert_eq!((t.shell.as_str(), t.args.len(), t.from_env), (r"C:\Program Files\Git\bin\bash.exe", 0, true));
        assert_eq!(t.dropped_args, ["-NoLogo"], "kept for `filer env` to name (#190)");
        for unset in [None, Some("".into()), Some("  ".into())] {
            let mut t = file();
            t.take_env(unset, Some("-x".into()));
            assert_eq!((t.shell.as_str(), t.args.len(), t.from_env), ("pwsh", 1, false));
        }
    }

    /// Q81: `FILER_TERM_ARGS` splits at white space, keeps a quoted run whole,
    /// and counts only beside `FILER_TERM_SHELL`.
    #[test]
    fn the_args_variable_is_split_into_words() {
        assert_eq!(split_words(r#"-NoProfile  -Command "a b" c"d e"f"#), ["-NoProfile", "-Command", "a b", "cd ef"]);
        assert_eq!(split_words(r#" "" x"#), ["", "x"]);
        assert!(split_words("   ").is_empty());
        let mut t = TermCfg { shell: "pwsh".into(), args: vec!["-NoLogo".into()], ..TermCfg::default() };
        t.take_env(Some("bash".into()), Some("--norc -i".into()));
        assert_eq!((t.args.clone(), t.args_from_env), (vec!["--norc".to_string(), "-i".to_string()], true));
        let mut t = TermCfg { shell: "pwsh".into(), args: vec!["-NoLogo".into()], ..TermCfg::default() };
        t.take_env(None, Some("--norc".into()));
        assert_eq!((t.args.clone(), t.args_from_env, t.args_unused), (vec!["-NoLogo".to_string()], false, true));
        t.take_env(None, Some("  ".into()));
        assert!(!t.args_unused, "a blank value is no value");
    }

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

    /// A section in the wrong half of the pair is named, with where it goes.
    ///
    /// This is the whole point of the check: `[term]` in `yazi.toml` is not an
    /// error anywhere, it simply does nothing, and every report of the problem
    /// arrives as "my shell setting is being ignored".
    #[test]
    fn a_filer_section_in_yazi_toml_is_named() {
        let m = Misplaced::in_file("[term]\nshell = \"pwsh\"\n", ConfigFile::Yazi);
        assert_eq!(m.sections, ["[term]"]);
        assert!(!m.breaks_parse, "yazi.toml still parses; the section is just unread");

        let mut w = Vec::new();
        m.warn(r"C:\x\yazi.toml", "filer.toml", &mut w);
        assert_eq!(w, [r"C:\x\yazi.toml: [term] belongs in filer.toml and was ignored"]);

        let text = "[ui]\na = 1\n[term]\nb = 2\n[line_args]\nc = \"d\"\n";
        assert_eq!(Misplaced::in_file(text, ConfigFile::Yazi).sections, ["[ui]", "[term]", "[line_args]"]);
    }

    /// A backslash escape in double quotes gets the single-quote hint; other
    /// parse errors do not.
    #[test]
    fn parse_errors_at_a_backslash_say_to_use_single_quotes() {
        const HINT: &str = "write a Windows path in 'single quotes')";
        for text in ["[opener]\nedit = \"C:\\Users\\me\"\n", "[a]\nb = \"C:\\dev\"\n"] {
            let e = text.parse::<toml::Table>().unwrap_err();
            assert!(parse_error(text, &e).ends_with(HINT), "{text:?}: {}", parse_error(text, &e));
        }
        let text = "[mgr\nratio = 1\n";
        let e = text.parse::<toml::Table>().unwrap_err();
        let said = parse_error(text, &e);
        assert!(!said.contains(HINT) && !said.ends_with('\n'), "{said:?}");
    }

    /// `[[preview]]` in `yazi.toml` costs the whole file, and says so.
    ///
    /// yazi's `[preview]` is a table, so serde reads the array's first entry as
    /// the `wrap` string and reports "invalid type: map, expected a string" -- a
    /// message that sends the reader to a line they copied out of filer's own
    /// README. Worse, the failure drops the file entirely, openers and all.
    #[test]
    fn a_preview_rule_in_yazi_toml_is_named_as_fatal() {
        let text = "[mgr]\nshow_hidden = true\n\n[[preview]]\nmatch = \"*.pdf\"\nrun = \"x\"\n";
        assert!(toml::from_str::<YaziToml>(text).is_err(), "this is the parse that fails");

        let m = Misplaced::in_file(text, ConfigFile::Yazi);
        assert_eq!(m.sections, ["[[preview]]"]);
        assert!(m.breaks_parse);

        let mut w = Vec::new();
        m.warn(r"C:\x\yazi.toml", "filer.toml", &mut w);
        let said = r"C:\x\yazi.toml: [[preview]] belongs in filer.toml, and nothing in this file was read";
        assert_eq!(w, [said]);
    }

    /// #203: a file none of whose settings took effect is recorded as such,
    /// for the help panel; a file with one misplaced section and the rest
    /// read is not.
    #[test]
    fn files_nothing_was_read_from_are_recorded() {
        let dir = crate::util::test_dir("cfg-unread");
        std::fs::write(dir.join("yazi.toml"), "[[preview]]\nmatch = \"*.pdf\"\nrun = \"x\"\n").unwrap();
        std::fs::write(dir.join("theme.toml"), "[mgr\n").unwrap();
        std::fs::write(dir.join("keymap.toml"), "[[mgr.keymap]\n").unwrap();
        std::fs::write(dir.join("filer.toml"), "[ui]\nfont_size = 16.0\n[opener]\nedit = []\n").unwrap();
        let (cfg, _) = Config::read(std::slice::from_ref(&dir));
        let mut unread: Vec<String> = cfg.unread.iter().map(|p| crate::util::file_name(p)).collect();
        unread.sort();
        assert_eq!(unread, ["keymap.toml", "theme.toml", "yazi.toml"], "filer.toml was read, [opener] aside");
        assert_eq!(cfg.loaded.len(), 4, "all four were read from disk");
    }

    /// Each file's own `preview` shape is left alone.
    #[test]
    fn the_right_preview_shape_is_not_flagged() {
        let yazi = "[preview]\nmax_width = 600\nimage_filter = \"triangle\"\n";
        assert_eq!(Misplaced::in_file(yazi, ConfigFile::Yazi), Misplaced::default());
        // And the same table in filer.toml is the one that belongs elsewhere.
        let m = Misplaced::in_file(yazi, ConfigFile::Filer);
        assert_eq!(m.sections, ["[preview]"]);
        assert!(m.breaks_parse);

        let rules = "[[preview]]\nmatch = \"*.pdf\"\nrun = \"x\"\n";
        assert_eq!(Misplaced::in_file(rules, ConfigFile::Filer), Misplaced::default());
    }

    /// The check runs both ways: yazi's tables in `filer.toml` are dead too.
    #[test]
    fn a_yazi_section_in_filer_toml_is_named() {
        let text = "[opener]\nedit = []\n\n[mgr]\nratio = [1, 4, 3]\n";
        let m = Misplaced::in_file(text, ConfigFile::Filer);
        assert_eq!(m.sections, ["[mgr]", "[opener]"]);
        assert!(!m.breaks_parse);
    }

    /// A file that is not valid TOML is the parser's to explain, not this.
    ///
    /// Its message carries the line and column; a guess at which section is in
    /// the wrong file would only bury it.
    #[test]
    fn a_broken_file_is_left_to_the_parser() {
        assert_eq!(Misplaced::in_file("[term\nshell =", ConfigFile::Yazi), Misplaced::default());
    }

    /// The shipped example belongs where it says to copy it.
    #[test]
    fn the_example_is_in_the_right_file() {
        let text = std::fs::read_to_string("filer.example.toml").expect("the shipped example");
        assert_eq!(Misplaced::in_file(&text, ConfigFile::Filer), Misplaced::default());
    }

    /// TESTING.md 33.12: two misplaced sections in one file, and the cost is
    /// the file's rather than each section's.
    ///
    /// `[term]` alone is merely ignored (33.13) and `[[preview]]` alone takes
    /// the file down (33.11), so a file holding both has a `[term]` that would
    /// have been ignored in any case -- except that nothing in the file was
    /// read, which is what the row asks both lines to say. The cost lives on
    /// `Misplaced` and not on each section for exactly this reason, and a
    /// per-section cost would have read "`[term]` … was ignored" under a
    /// `[[preview]]` line saying the file went unread: two answers to "did my
    /// shell setting load", one of them wrong.
    #[test]
    fn two_misplaced_sections_both_report_the_files_cost() {
        let text = "[mgr]\nshow_hidden = true\n\n[term]\nshell = \"pwsh\"\n\n\
                    [[preview]]\nmatch = \"*.pdf\"\nrun = \"x\"\n";
        let m = Misplaced::in_file(text, ConfigFile::Yazi);
        assert_eq!(m.sections, ["[term]", "[[preview]]"], "the foreign ones, in the file's order");
        assert!(m.breaks_parse, "one of them is fatal, so the file is");

        let mut w = Vec::new();
        m.warn(r"C:\x\yazi.toml", "filer.toml", &mut w);
        assert_eq!(
            w,
            [
                r"C:\x\yazi.toml: [term] belongs in filer.toml, and nothing in this file was read",
                r"C:\x\yazi.toml: [[preview]] belongs in filer.toml, and nothing in this file was read",
            ],
            "a line each, and both name the file's cost rather than the section's",
        );
    }

    /// TESTING.md 33.14: the sentence the other way round names `yazi.toml`.
    ///
    /// The same shape read from the other side, because "which file does this
    /// go in" is the only thing either warning is for and half of them point
    /// the other way. `[opener]` is the row's own example and it is the
    /// expensive one to get wrong: every opener in the file is dead.
    #[test]
    fn a_yazi_section_in_filer_toml_is_told_where_to_go() {
        let m = Misplaced::in_file("[opener]\nedit = []\n", ConfigFile::Filer);
        let mut w = Vec::new();
        m.warn(r"C:\x\filer.toml", "yazi.toml", &mut w);
        assert_eq!(w, [r"C:\x\filer.toml: [opener] belongs in yazi.toml and was ignored"]);
    }

    /// A `filer.toml` written before `[term]` existed still reads.
    #[test]
    fn an_older_config_is_unaffected() {
        let text = std::fs::read_to_string("filer.example.toml").expect("the shipped example");
        let cfg: FilerToml = toml::from_str(&text).expect("the example has to parse");
        assert_eq!(cfg.ui.font_size, 14.0);
    }

    /// #239, #240: a `[ui] backend` filer cannot use is a warning when the
    /// file is read, so `filer env` says it as the window does.
    #[test]
    fn a_bad_backend_is_warned_about_when_the_file_is_read() {
        let dir = crate::util::test_dir("backend-warn");
        std::fs::write(dir.join("filer.toml"), "[ui]\nbackend = \"directx\"\n").unwrap();
        let (cfg, _) = Config::read(std::slice::from_ref(&dir));
        let said: Vec<_> = cfg.warnings.iter().filter(|w| w.contains("\"directx\"")).collect();
        assert_eq!(said.len(), 1, "{:?}", cfg.warnings);
        assert!(said[0].contains("filer.toml: [ui] backend"), "names the file: {}", said[0]);
        std::fs::write(dir.join("filer.toml"), "[ui]\nbackend = \"gl\"\n").unwrap();
        let (cfg, _) = Config::read(std::slice::from_ref(&dir));
        assert!(!cfg.warnings.iter().any(|w| w.contains("backend")), "{:?}", cfg.warnings);
    }
}

/// Q47: what `<C-F5>` does with a file that stopped parsing.
#[cfg(test)]
mod reload_tests {
    use super::*;

    /// A `filer.toml` broken mid-edit keeps the `[ui]` it gave last time, and
    /// the warning says so; a fresh start on the same file gets the defaults.
    /// The files that still parse are read as usual.
    #[test]
    fn a_broken_file_keeps_what_it_gave_last_time() {
        let dir = crate::util::test_dir("reload-broken");
        std::fs::write(dir.join("filer.toml"), "[ui]\nfont_size = 28.0\n").unwrap();
        std::fs::write(dir.join("keymap.toml"), "[[mgr.prepend_keymap]]\non = \"<F9>\"\nrun = \"quit\"\n").unwrap();
        let dirs = vec![dir.clone()];
        let f9 = |c: &Config| c.keymap.mgr.iter().any(|b| crate::config::keys::render_seq(&b.on) == "<F9>");
        let mut cfg = Config::read(&dirs).0;
        assert_eq!(cfg.ui.font_size, 28.0);
        assert!(f9(&cfg));

        std::fs::write(dir.join("filer.toml"), "[ui]\nfont_size = 28.0\nthis line is not toml\n").unwrap();
        let mut cfg = Config::reload_from(&mut cfg, &dirs);
        assert_eq!(cfg.ui.font_size, 28.0, "kept from the last good read");
        assert!(f9(&cfg), "the keymap still parses and is read as usual");
        let said = cfg.warnings.first().cloned().unwrap_or_default();
        assert!(said.contains("filer.toml") && said.ends_with("stay in force until it parses again)"), "{said}");
        assert_eq!(Config::read(&dirs).0.ui.font_size, Ui::default().font_size, "at start there is nothing to keep");

        // The keymap the same way, while the fixed `filer.toml` is read again.
        std::fs::write(dir.join("filer.toml"), "[ui]\nfont_size = 20.0\n").unwrap();
        std::fs::write(dir.join("keymap.toml"), "[[mgr.prepend_keymap]\n").unwrap();
        let cfg = Config::reload_from(&mut cfg, &dirs);
        assert_eq!(cfg.ui.font_size, 20.0, "fixed, so read again");
        assert!(f9(&cfg), "the last keymap that parsed");
        let said = cfg.warnings.iter().find(|w| w.contains("keymap.toml")).cloned().unwrap_or_default();
        assert!(said.ends_with("stay in force until it parses again)"), "{said}");
    }
}
