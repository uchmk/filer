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
                let wrong = Misplaced::in_file(&text, ConfigFile::Yazi);
                wrong.warn(&at(dir, "yazi.toml"), "filer.toml", &mut warnings);
                match toml::from_str::<YaziToml>(&text) {
                    Ok(v) => yazi_cfg = merge_yazi(yazi_cfg, v),
                    // The serde message for a `preview` in the wrong shape is
                    // "invalid type: map, expected a string", pointing at a line
                    // whose `[[preview]]` is spelled exactly as its own
                    // documentation spells it. `wrong` has already said which
                    // key it is and which file it goes in.
                    Err(_) if wrong.breaks_parse => {}
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
                let wrong = Misplaced::in_file(&text, ConfigFile::Filer);
                wrong.warn(&at(dir, "filer.toml"), "yazi.toml", &mut warnings);
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
                    Err(_) if wrong.breaks_parse => {}
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

/// The two config files, and what each one is allowed to say.
#[cfg(test)]
mod files {
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

    /// A `filer.toml` written before `[term]` existed still reads.
    #[test]
    fn an_older_config_is_unaffected() {
        let text = std::fs::read_to_string("filer.example.toml").expect("the shipped example");
        let cfg: FilerToml = toml::from_str(&text).expect("the example has to parse");
        assert_eq!(cfg.ui.font_size, 14.0);
    }
}
