//! yazi-compatible theming: `theme.toml` colors, `[filetype]` rules and
//! `[icon]` rules, on top of the common theme every uchmk app shares
//! (common.toml's `theme`, `tsumugi Dark` unless chosen otherwise; Q98).

use std::sync::Arc;

use egui::Color32;
use ito_theme::{mix, Colors};
use serde::Deserialize;

use crate::fs::Entry;
use crate::glob;

// ------------------------------------------------------------------ raw schema

#[derive(Deserialize, Debug, Default, Clone)]
pub struct RawStyle {
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub dim: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub reversed: bool,
}

#[derive(Deserialize, Debug, Default)]
pub struct ThemeToml {
    /// yazi's `[app]`: `overall` is the whole window's style, its background
    /// above all -- the one colour a light theme has to change first (#203).
    #[serde(default)]
    pub app: AppTheme,
    #[serde(default, alias = "manager")]
    pub mgr: MgrTheme,
    #[serde(default)]
    pub status: StatusTheme,
    #[serde(default)]
    pub filetype: FiletypeTheme,
    #[serde(default)]
    pub icon: IconTheme,
    #[serde(default)]
    pub which: WhichTheme,
    #[serde(default)]
    pub git: GitTheme,
}

/// yazi's `[app]` section.
#[derive(Deserialize, Debug, Default)]
pub struct AppTheme {
    #[serde(default)]
    pub overall: RawStyle,
}

/// yazi's `[git]` section, which its git plugin colors its signs with.
#[derive(Deserialize, Debug, Default)]
pub struct GitTheme {
    #[serde(default)]
    pub modified: RawStyle,
    #[serde(default)]
    pub deleted: RawStyle,
    #[serde(default)]
    pub added: RawStyle,
    #[serde(default)]
    pub untracked: RawStyle,
    /// yazi names this one for a conflicted merge.
    #[serde(default)]
    pub updated: RawStyle,
}

#[derive(Deserialize, Debug, Default)]
pub struct MgrTheme {
    #[serde(default)]
    pub cwd: RawStyle,
    #[serde(default)]
    pub hovered: RawStyle,
    #[serde(default)]
    pub preview_hovered: RawStyle,
    #[serde(default)]
    pub find_keyword: RawStyle,
    #[serde(default)]
    pub find_position: RawStyle,
    #[serde(default)]
    pub marker_copied: RawStyle,
    #[serde(default)]
    pub marker_cut: RawStyle,
    #[serde(default)]
    pub marker_marked: RawStyle,
    #[serde(default)]
    pub marker_selected: RawStyle,
    #[serde(default)]
    pub tab_active: RawStyle,
    #[serde(default)]
    pub tab_inactive: RawStyle,
    #[serde(default)]
    pub border_style: RawStyle,
    #[serde(default)]
    pub syntect_theme: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct StatusTheme {
    #[serde(default)]
    pub overall: RawStyle,
    #[serde(default)]
    pub mode_normal: RawStyle,
    #[serde(default)]
    pub mode_select: RawStyle,
    #[serde(default)]
    pub mode_unset: RawStyle,
    #[serde(default)]
    pub progress_normal: RawStyle,
    #[serde(default)]
    pub progress_error: RawStyle,
}

#[derive(Deserialize, Debug, Default)]
pub struct FiletypeTheme {
    #[serde(default)]
    pub rules: Vec<FiletypeRule>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FiletypeRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub is: Option<String>,
    #[serde(flatten)]
    pub style: RawStyle,
}

#[derive(Deserialize, Debug, Default)]
pub struct IconTheme {
    #[serde(default)]
    pub globs: Vec<IconRule>,
    #[serde(default)]
    pub dirs: Vec<IconRule>,
    #[serde(default)]
    pub exts: Vec<IconRule>,
    #[serde(default)]
    pub files: Vec<IconRule>,
    #[serde(default)]
    pub conds: Vec<CondIcon>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct IconRule {
    pub name: String,
    pub text: String,
    #[serde(default)]
    pub fg: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CondIcon {
    #[serde(rename = "if")]
    pub cond: String,
    pub text: String,
    #[serde(default)]
    pub fg: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct WhichTheme {
    #[serde(default)]
    pub cols: Option<u8>,
    #[serde(default)]
    pub cand: RawStyle,
    #[serde(default)]
    pub rest: RawStyle,
    #[serde(default)]
    pub desc: RawStyle,
}

// --------------------------------------------------------------------- palette

/// ANSI-ish palette used when a theme names a color instead of giving a hex.
/// Tuned for a dark background.
/// `from` moved `by` of the way to `to`, per channel.
fn toward(from: Color32, to: Color32, by: f32) -> Color32 {
    let step = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * by).round() as u8;
    Color32::from_rgb(step(from.r(), to.r()), step(from.g(), to.g()), step(from.b(), to.b()))
}

pub fn parse_color(s: &str) -> Option<Color32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    Some(match s.to_ascii_lowercase().as_str() {
        "reset" | "default" => return None,
        "black" => Color32::from_rgb(0x1e, 0x20, 0x26),
        "red" => Color32::from_rgb(0xd0, 0x51, 0x51),
        "green" => Color32::from_rgb(0x5b, 0xa8, 0x5b),
        "yellow" => Color32::from_rgb(0xc9, 0xa5, 0x54),
        "blue" => Color32::from_rgb(0x52, 0x8b, 0xd6),
        "magenta" => Color32::from_rgb(0xa8, 0x73, 0xd0),
        "cyan" => Color32::from_rgb(0x46, 0xa5, 0xa5),
        "white" | "gray" | "grey" => Color32::from_rgb(0xbf, 0xc4, 0xcf),
        "darkgray" | "darkgrey" => Color32::from_rgb(0x6b, 0x71, 0x80),
        "lightblack" => Color32::from_rgb(0x3a, 0x3f, 0x4b),
        "lightred" => Color32::from_rgb(0xf0, 0x71, 0x78),
        "lightgreen" => Color32::from_rgb(0x8e, 0xd0, 0x8e),
        "lightyellow" => Color32::from_rgb(0xe8, 0xc8, 0x7a),
        "lightblue" => Color32::from_rgb(0x7a, 0xb8, 0xf5),
        "lightmagenta" => Color32::from_rgb(0xc9, 0x9c, 0xf0),
        "lightcyan" => Color32::from_rgb(0x6f, 0xd0, 0xd0),
        "lightwhite" => Color32::from_rgb(0xe8, 0xeb, 0xf2),
        _ => return parse_hex(s),
    })
}

fn parse_hex(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');
    let n = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(n(0)?, n(2)?, n(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(n(0)?, n(2)?, n(4)?, n(6)?)),
        3 => {
            let d = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
            Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

// ------------------------------------------------------------- resolved styles

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Style {
    pub fg: Option<Color32>,
    pub bg: Option<Color32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub reversed: bool,
}

impl Style {
    pub fn fg(c: Color32) -> Self {
        Self { fg: Some(c), ..Default::default() }
    }

    fn from_raw(r: &RawStyle) -> Self {
        Self {
            fg: r.fg.as_deref().and_then(parse_color),
            bg: r.bg.as_deref().and_then(parse_color),
            bold: r.bold,
            italic: r.italic,
            underline: r.underline,
            dim: r.dim,
            reversed: r.reversed,
        }
    }

    /// Overlay a user style over a default; only the parts the user set win.
    fn overlay(self, r: &RawStyle) -> Self {
        let u = Self::from_raw(r);
        Self {
            fg: u.fg.or(self.fg),
            bg: u.bg.or(self.bg),
            bold: u.bold || self.bold,
            italic: u.italic || self.italic,
            underline: u.underline || self.underline,
            dim: u.dim || self.dim,
            reversed: u.reversed || self.reversed,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileRule {
    pub name: Option<String>,
    pub mime: Option<String>,
    pub is: Option<String>,
    pub style: Style,
}

#[derive(Clone, Debug)]
pub struct Icon {
    pub text: String,
    pub fg: Option<Color32>,
}

#[derive(Clone, Debug)]
pub struct Theme {
    // window chrome
    pub bg: Color32,
    pub bg_alt: Color32,
    pub border: Color32,
    pub fg: Color32,
    pub fg_dim: Color32,

    pub cwd: Style,
    pub hovered: Style,
    pub hovered_bg: Color32,
    pub inactive_hovered_bg: Color32,
    pub preview_hovered: Style,
    pub find_keyword: Style,
    pub find_position: Style,
    pub marker_copied: Color32,
    pub marker_cut: Color32,
    pub marker_marked: Color32,
    pub marker_selected: Color32,
    pub tab_active: Style,
    pub tab_inactive: Style,

    pub mode_normal: Style,
    pub mode_select: Style,
    pub mode_unset: Style,
    pub status_bg: Color32,
    pub progress_fg: Color32,
    pub progress_error: Color32,
    /// Something worth reading that did not stop anything: a config line that
    /// cannot take effect, most often. Red is the colour of a thing that
    /// failed, and spending it on advice teaches the reader that the program
    /// breaks on startup -- the first config warning shipped in red and came
    /// straight back as "an error message appears when I open it".
    pub warning: Color32,

    pub which_cols: usize,
    pub which_cand: Style,
    pub which_rest: Style,
    pub which_desc: Style,

    pub git_modified: Color32,
    pub git_deleted: Color32,
    pub git_added: Color32,
    pub git_untracked: Color32,
    pub git_conflict: Color32,

    pub syntect_theme: String,

    pub filetypes: Vec<FileRule>,
    pub icon_globs: Vec<(String, Icon)>,
    pub icon_dirs: Vec<(String, Icon)>,
    pub icon_exts: Vec<(String, Icon)>,
    pub icon_files: Vec<(String, Icon)>,
    pub icon_dir_default: Icon,
    pub icon_file_default: Icon,
    pub icon_link_default: Icon,

    /// The common theme these colours start from (Q98): what the settings
    /// screen's Theme page shows as chosen.
    pub common: Colors,
    /// Whether a `theme.toml` set the window's own colours (`[app] overall`),
    /// so the settings screen draws in those rather than the common theme's.
    pub overall: bool,
    /// Whether [`Theme::without_nerd_icons`] took the glyphs out, to do it
    /// again when the common theme changes underneath.
    pub plain_icons: bool,
    /// The `theme.toml`s read, in order, to lay over another common theme.
    layers: Arc<Vec<ThemeToml>>,
}

/// The default text, and the colour of a plain file's name: light, for the
/// dark default background.
const DARK_TEXT: Color32 = Color32::from_rgb(0xc8, 0xcd, 0xd8);
/// What a plain file's name and the warning are on a light background
/// (Q93): 7.9:1 and 6.6:1 on white, where the dark-background pair read at
/// 1.6:1.
const LIGHT_TEXT: Color32 = Color32::from_rgb(0x4b, 0x51, 0x60);
const LIGHT_WARNING: Color32 = Color32::from_rgb(0x7a, 0x56, 0x00);

/// Whether dark text reads better on `bg` than light text: WCAG's relative
/// luminance against the point where black and white contrast alike.
pub(crate) fn is_light(bg: Color32) -> bool {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(bg.r()) + 0.7152 * lin(bg.g()) + 0.0722 * lin(bg.b()) > 0.179
}

/// The warning on a dark background: the common dark theme's yellow, for a
/// `theme.toml` that puts a dark window over a light common theme.
const DARK_WARNING: Color32 = Color32::from_rgb(0xe8, 0xc8, 0x7a);

impl Default for Theme {
    fn default() -> Self {
        Self::from_common(&default_colors())
    }
}

/// The common theme when common.toml chooses none: `tsumugi Dark`, which is
/// the colours kura had built in before there was a common theme.
pub fn default_colors() -> Colors {
    ito_theme::builtin()[0].colors
}

impl Theme {
    /// kura's colours from a common theme's (Q98): every colour kura draws
    /// is one of the theme's, or mixed from them, so `tsumugi Light` gives a
    /// light kura that matches tsumugi and mimamori.
    pub fn from_common(c: &Colors) -> Self {
        let dim = quiet(c);
        // Text on a filled accent: the mode, the active tab.
        let on = |bg: Color32| Style { fg: Some(c.on_accent()), bg: Some(bg), bold: true, ..Default::default() };
        Self {
            bg: c.bg,
            bg_alt: c.panel,
            border: c.border,
            fg: c.fg,
            fg_dim: dim,

            cwd: Style::fg(c.run),
            hovered: Style::default(),
            hovered_bg: mix(c.bg, c.blue, if c.light { 0.35 } else { 0.3 }),
            inactive_hovered_bg: mix(c.bg, c.fg, 0.1),
            preview_hovered: Style { underline: true, ..Default::default() },
            // White on the yellow reads at 3:1, so the text or the ground,
            // whichever reads better on it.
            find_keyword: Style {
                fg: Some(text_on(c, c.wait)),
                bg: Some(c.wait),
                bold: true,
                ..Default::default()
            },
            find_position: Style::fg(c.magenta),
            marker_copied: c.done,
            marker_cut: c.err,
            marker_marked: c.run,
            marker_selected: c.wait,
            tab_active: on(c.blue),
            tab_inactive: Style::fg(dim),

            mode_normal: on(c.blue),
            mode_select: on(c.done),
            mode_unset: on(c.err),
            status_bg: mix(c.panel, c.fg, 0.03),
            progress_fg: c.blue,
            progress_error: c.err,
            // A light theme's yellow is a mark's, 3:1; as text it is darkened
            // to read at 4.5 (Q93).
            warning: if c.light { mix(c.wait, Color32::BLACK, 0.3) } else { c.wait },

            which_cols: 3,
            which_cand: Style { fg: Some(c.run), bold: true, ..Default::default() },
            which_rest: Style::fg(dim),
            which_desc: Style::fg(c.magenta),

            // Git signs, in the colors the TODO asks for and yazi uses:
            // changed is yellow, added green, untracked quiet, a conflict red.
            git_modified: c.wait,
            git_deleted: c.err,
            git_added: c.done,
            git_untracked: dim,
            git_conflict: c.err,

            syntect_theme: if c.light { "base16-ocean.light" } else { "base16-ocean.dark" }.into(),

            filetypes: default_filetypes(c),
            icon_globs: Vec::new(),
            icon_dirs: default_dir_icons(c),
            icon_exts: default_ext_icons(c),
            icon_files: default_file_icons(c),
            icon_dir_default: Icon { text: "\u{f07b}".into(), fg: Some(c.blue) },
            icon_file_default: Icon { text: "\u{f15b}".into(), fg: None },
            icon_link_default: Icon { text: "\u{f0c1}".into(), fg: Some(c.run) },

            common: *c,
            overall: false,
            plain_icons: false,
            layers: Arc::default(),
        }
    }

    /// The common theme `c` with the `theme.toml`s read over it, in order.
    pub fn layered(c: &Colors, layers: Vec<ThemeToml>) -> Self {
        let mut t = Self::from_common(c);
        for l in &layers {
            t.apply(l);
        }
        t.layers = Arc::new(layers);
        t
    }

    /// The same `theme.toml`s over another common theme: what choosing a
    /// theme on the settings screen, or in common.toml, does.
    pub fn with_common(&self, c: &Colors) -> Self {
        let mut t = Self::from_common(c);
        for l in self.layers.iter() {
            t.apply(l);
        }
        t.layers = self.layers.clone();
        if self.plain_icons {
            t.without_nerd_icons();
        }
        t
    }

    /// Whether the window is light, for egui's own widgets.
    pub fn light(&self) -> bool {
        is_light(self.bg)
    }

    /// Swap the Nerd Font glyphs for plain ASCII when no icon font is available.
    pub fn without_nerd_icons(&mut self) {
        self.plain_icons = true;
        self.icon_globs.clear();
        self.icon_dirs.clear();
        self.icon_exts.clear();
        self.icon_files.clear();
        self.icon_dir_default = Icon { text: "/".into(), fg: self.icon_dir_default.fg };
        self.icon_file_default = Icon { text: " ".into(), fg: None };
        self.icon_link_default = Icon { text: "~".into(), fg: self.icon_link_default.fg };
    }

    /// The color a git sign is drawn in. `Clean` never reaches here, since it
    /// has no sign to draw, but it answers in the plain foreground anyway.
    pub fn git_color(&self, state: crate::fs::git::State) -> Color32 {
        use crate::fs::git::State;
        match state {
            State::Clean => self.fg,
            State::Untracked => self.git_untracked,
            State::Staged => self.git_added,
            State::Modified => self.git_modified,
            State::Deleted => self.git_deleted,
            State::Conflict => self.git_conflict,
        }
    }

    pub fn apply(&mut self, t: &ThemeToml) {
        // First, so the rest of the file can still set its own colours over a
        // background chosen here. The second background -- panels, the help
        // box -- follows it a step towards the text, as the built-in pair do.
        let before = self.fg;
        let fg = t.app.overall.fg.as_deref().and_then(parse_color);
        if let Some(fg) = fg {
            self.fg = fg;
            self.overall = true;
        }
        if let Some(bg) = t.app.overall.bg.as_deref().and_then(parse_color) {
            self.overall = true;
            let light = is_light(bg);
            // Q93: the text and the yellow were chosen for the background
            // underneath, and on the other kind they read at 1.5:1 (#213).
            // They turn over -- these two, and the plain file name, which is
            // the text's colour -- unless the file set the text itself.
            let turned = light != is_light(self.bg);
            if turned {
                let (text, warning) = if light { (LIGHT_TEXT, LIGHT_WARNING) } else { (DARK_TEXT, DARK_WARNING) };
                if fg.is_none() {
                    self.fg = text;
                }
                self.warning = warning;
                for r in &mut self.filetypes {
                    if r.style.fg == Some(before) {
                        r.style.fg = Some(text);
                    }
                }
            }
            // The cursor's dark bars would put dark text on dark blue; on a
            // light window they are a tint of it instead, the way `bg_alt`
            // is. `[mgr] hovered` still sets its own below.
            if light || turned {
                self.hovered_bg = toward(bg, self.progress_fg, if light { 0.35 } else { 0.3 });
                self.inactive_hovered_bg = toward(bg, self.fg, 0.1);
            }
            self.bg = bg;
            self.bg_alt = toward(bg, self.fg, 0.03);
        }
        self.cwd = self.cwd.overlay(&t.mgr.cwd);
        self.hovered = self.hovered.overlay(&t.mgr.hovered);
        if let Some(bg) = self.hovered.bg {
            self.hovered_bg = bg;
        }
        self.preview_hovered = self.preview_hovered.overlay(&t.mgr.preview_hovered);
        self.find_keyword = self.find_keyword.overlay(&t.mgr.find_keyword);
        self.find_position = self.find_position.overlay(&t.mgr.find_position);
        if let Some(c) = t.mgr.marker_copied.fg.as_deref().and_then(parse_color) {
            self.marker_copied = c;
        }
        if let Some(c) = t.mgr.marker_cut.fg.as_deref().and_then(parse_color) {
            self.marker_cut = c;
        }
        if let Some(c) = t.mgr.marker_marked.fg.as_deref().and_then(parse_color) {
            self.marker_marked = c;
        }
        if let Some(c) = t.mgr.marker_selected.fg.as_deref().and_then(parse_color) {
            self.marker_selected = c;
        }
        self.tab_active = self.tab_active.overlay(&t.mgr.tab_active);
        self.tab_inactive = self.tab_inactive.overlay(&t.mgr.tab_inactive);
        if let Some(c) = t.mgr.border_style.fg.as_deref().and_then(parse_color) {
            self.border = c;
        }
        self.mode_normal = self.mode_normal.overlay(&t.status.mode_normal);
        self.mode_select = self.mode_select.overlay(&t.status.mode_select);
        self.mode_unset = self.mode_unset.overlay(&t.status.mode_unset);
        if let Some(c) = t.status.overall.bg.as_deref().and_then(parse_color) {
            self.status_bg = c;
        }
        if let Some(c) = t.status.progress_normal.fg.as_deref().and_then(parse_color) {
            self.progress_fg = c;
        }
        if let Some(c) = t.status.progress_error.fg.as_deref().and_then(parse_color) {
            self.progress_error = c;
        }
        if let Some(c) = t.which.cols {
            self.which_cols = (c as usize).clamp(1, 6);
        }
        self.which_cand = self.which_cand.overlay(&t.which.cand);
        self.which_rest = self.which_rest.overlay(&t.which.rest);
        self.which_desc = self.which_desc.overlay(&t.which.desc);
        for (raw, slot) in [
            (&t.git.modified, &mut self.git_modified),
            (&t.git.deleted, &mut self.git_deleted),
            (&t.git.added, &mut self.git_added),
            (&t.git.untracked, &mut self.git_untracked),
            (&t.git.updated, &mut self.git_conflict),
        ] {
            if let Some(c) = raw.fg.as_deref().and_then(parse_color) {
                *slot = c;
            }
        }
        if let Some(s) = t.mgr.syntect_theme.as_deref() {
            if !s.is_empty() {
                self.syntect_theme = s.to_string();
            }
        }

        // User filetype rules take precedence over the built-ins.
        let mut rules: Vec<FileRule> = t
            .filetype
            .rules
            .iter()
            .map(|r| FileRule {
                name: r.name.clone(),
                mime: r.mime.clone(),
                is: r.is.clone(),
                style: Style::from_raw(&r.style),
            })
            .collect();
        rules.append(&mut self.filetypes);
        self.filetypes = rules;

        prepend_icons(&mut self.icon_globs, &t.icon.globs);
        prepend_icons(&mut self.icon_dirs, &t.icon.dirs);
        prepend_icons(&mut self.icon_exts, &t.icon.exts);
        prepend_icons(&mut self.icon_files, &t.icon.files);
        for c in &t.icon.conds {
            let icon = Icon { text: c.text.clone(), fg: c.fg.as_deref().and_then(parse_color) };
            match c.cond.as_str() {
                "dir" => self.icon_dir_default = icon,
                "link" | "orphan" => self.icon_link_default = icon,
                _ => {}
            }
        }
    }

    /// Color for an entry, from the first matching `[filetype]` rule.
    pub fn style_for(&self, entry: &Entry, mime: &str) -> Style {
        for r in &self.filetypes {
            if rule_matches(r.name.as_deref(), r.mime.as_deref(), r.is.as_deref(), entry, mime) {
                return r.style;
            }
        }
        Style::fg(self.fg)
    }

    pub fn icon_for(&self, entry: &Entry) -> &Icon {
        let lower = entry.name.to_lowercase();
        for (pat, icon) in &self.icon_globs {
            if glob::matches(pat, &entry.name, true) {
                return icon;
            }
        }
        if entry.is_dir_like() {
            for (name, icon) in &self.icon_dirs {
                if name.eq_ignore_ascii_case(&entry.name) {
                    return icon;
                }
            }
            if entry.kind.is_link() {
                return &self.icon_link_default;
            }
            return &self.icon_dir_default;
        }
        for (name, icon) in &self.icon_files {
            if name.eq_ignore_ascii_case(&lower) {
                return icon;
            }
        }
        if let Some(ext) = &entry.ext {
            for (name, icon) in &self.icon_exts {
                if name.eq_ignore_ascii_case(ext) {
                    return icon;
                }
            }
        }
        if entry.kind.is_link() {
            return &self.icon_link_default;
        }
        &self.icon_file_default
    }
}

fn prepend_icons(dest: &mut Vec<(String, Icon)>, src: &[IconRule]) {
    let mut v: Vec<(String, Icon)> = src
        .iter()
        .map(|r| {
            (r.name.clone(), Icon { text: r.text.clone(), fg: r.fg.as_deref().and_then(parse_color) })
        })
        .collect();
    v.append(dest);
    *dest = v;
}

fn rule_matches(
    name: Option<&str>,
    mime: Option<&str>,
    is: Option<&str>,
    entry: &Entry,
    entry_mime: &str,
) -> bool {
    if let Some(is) = is {
        let ok = match is {
            "dir" => entry.is_dir_like(),
            "hidden" => entry.hidden,
            "link" => entry.kind.is_link(),
            "orphan" => matches!(entry.kind, crate::fs::Kind::Link { broken: true, .. }),
            "exec" => entry
                .ext
                .as_deref()
                .is_some_and(|e| matches!(e, "exe" | "bat" | "cmd" | "com" | "ps1" | "msi")),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    if let Some(pat) = name {
        // A trailing slash restricts the rule to directories.
        let (pat, dir_only) = match pat.strip_suffix('/') {
            Some(p) => (p, true),
            None => (pat, false),
        };
        if dir_only && !entry.is_dir_like() {
            return false;
        }
        let pat = if pat.is_empty() { "*" } else { pat };
        if !glob::matches(pat, &entry.name, true) {
            return false;
        }
    }
    if let Some(pat) = mime {
        if !glob::matches(pat, entry_mime, true) {
            return false;
        }
    }
    name.is_some() || mime.is_some() || is.is_some()
}

// ------------------------------------------------------------ built-in content

/// A built-in rule's colour in the common theme `t`. The tables below are
/// written in `tsumugi Dark`'s hexes, and each of those is the theme's own
/// colour of that name, so another theme recolours them; the rest are a
/// language's own colour (Rust's orange, Python's yellow) and stay.
/// The quiet text: a dark theme's dim a step into the ground, as kura's
/// own grey was; a light theme's dim as it is, which reads at 3:1 already.
fn quiet(t: &Colors) -> Color32 {
    if t.light {
        t.dim
    } else {
        mix(t.dim, t.bg, 0.25)
    }
}

/// Of the theme's text and its ground, the one that reads better on `fill`.
fn text_on(t: &Colors, fill: Color32) -> Color32 {
    if ito_theme::contrast(t.fg, fill) >= ito_theme::contrast(t.bg, fill) {
        t.fg
    } else {
        t.bg
    }
}

fn c(t: &Colors, hex: u32) -> Color32 {
    match hex {
        0xf07178 => t.err,
        0x6fd0d0 => t.run,
        0x7ab8f5 => t.blue,
        0xc99cf0 => t.magenta,
        0xe8c87a => t.wait,
        0x8ed08e => t.done,
        0xc8cdd8 => t.fg,
        0x798090 => quiet(t),
        // The deeper red of a PDF, Java, Ruby.
        0xd05151 => mix(t.err, Color32::BLACK, 0.15),
        _ => Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8),
    }
}

fn default_filetypes(t: &Colors) -> Vec<FileRule> {
    let mk = |name: Option<&str>, mime: Option<&str>, is: Option<&str>, col: u32, bold: bool| FileRule {
        name: name.map(str::to_owned),
        mime: mime.map(str::to_owned),
        is: is.map(str::to_owned),
        style: Style { fg: Some(c(t, col)), bold, ..Default::default() },
    };
    vec![
        mk(None, None, Some("orphan"), 0xf07178, false),
        mk(None, None, Some("link"), 0x6fd0d0, false),
        mk(Some("*/"), None, None, 0x7ab8f5, true),
        mk(None, Some("image/*"), None, 0xc99cf0, false),
        mk(None, Some("video/*"), None, 0xe8c87a, false),
        mk(None, Some("audio/*"), None, 0xe8c87a, false),
        mk(None, Some("application/zip"), None, 0xf07178, false),
        mk(None, Some("application/gzip"), None, 0xf07178, false),
        mk(None, Some("application/x-7z-compressed"), None, 0xf07178, false),
        mk(None, Some("application/vnd.rar"), None, 0xf07178, false),
        mk(None, Some("application/x-tar"), None, 0xf07178, false),
        mk(None, Some("application/zstd"), None, 0xf07178, false),
        mk(None, Some("application/x-xz"), None, 0xf07178, false),
        mk(None, Some("application/pdf"), None, 0xd05151, false),
        mk(None, Some("application/vnd.microsoft.portable-executable"), None, 0x8ed08e, true),
        mk(None, Some("application/x-sharedlib"), None, 0x798090, false),
        mk(None, Some("text/*"), None, 0xc8cdd8, false), // the text
        mk(None, None, Some("hidden"), 0x798090, false),
    ]
}

fn icon(t: &Colors, glyph: &str, col: Option<u32>) -> Icon {
    Icon { text: glyph.to_owned(), fg: col.map(|h| c(t, h)) }
}

fn default_dir_icons(t: &Colors) -> Vec<(String, Icon)> {
    [
        (".git", "\u{e5fb}", Some(0xf07178u32)),
        (".github", "\u{e5fd}", None),
        ("node_modules", "\u{e5fa}", Some(0xd05151)),
        ("target", "\u{f085}", Some(0x798090)),
        ("src", "\u{f121}", Some(0x7ab8f5)),
        ("Desktop", "\u{f108}", None),
        ("Documents", "\u{f02d}", None),
        ("Downloads", "\u{f019}", None),
        ("Pictures", "\u{f03e}", None),
        ("Music", "\u{f001}", None),
        ("Videos", "\u{f03d}", None),
    ]
    .into_iter()
    .map(|(n, g, col)| (n.to_owned(), icon(t, g, col)))
    .collect()
}

fn default_file_icons(t: &Colors) -> Vec<(String, Icon)> {
    [
        ("cargo.toml", "\u{e7a8}", Some(0xdea584u32)),
        ("cargo.lock", "\u{e7a8}", Some(0x798090)),
        ("package.json", "\u{e718}", Some(0x8ed08e)),
        ("dockerfile", "\u{f308}", Some(0x7ab8f5)),
        ("makefile", "\u{f085}", Some(0x798090)),
        ("readme.md", "\u{f48a}", Some(0xe8c87a)),
        ("license", "\u{f718}", Some(0x798090)),
        (".gitignore", "\u{e702}", Some(0xf07178)),
        (".env", "\u{f462}", Some(0xe8c87a)),
    ]
    .into_iter()
    .map(|(n, g, col)| (n.to_owned(), icon(t, g, col)))
    .collect()
}

fn default_ext_icons(t: &Colors) -> Vec<(String, Icon)> {
    [
        ("rs", "\u{e7a8}", Some(0xdea584u32)),
        ("toml", "\u{e6b2}", Some(0x9c6644)),
        ("py", "\u{e73c}", Some(0xffd43b)),
        ("js", "\u{e74e}", Some(0xe8c87a)),
        ("mjs", "\u{e74e}", Some(0xe8c87a)),
        ("ts", "\u{e628}", Some(0x7ab8f5)),
        ("tsx", "\u{e7ba}", Some(0x7ab8f5)),
        ("jsx", "\u{e7ba}", Some(0x6fd0d0)),
        ("json", "\u{e60b}", Some(0xe8c87a)),
        ("html", "\u{e736}", Some(0xf07178)),
        ("css", "\u{e749}", Some(0x7ab8f5)),
        ("scss", "\u{e749}", Some(0xc99cf0)),
        ("md", "\u{e73e}", Some(0xc8cdd8)),
        ("go", "\u{e627}", Some(0x6fd0d0)),
        ("c", "\u{e61e}", Some(0x7ab8f5)),
        ("h", "\u{f0fd}", Some(0xc99cf0)),
        ("cpp", "\u{e61d}", Some(0x7ab8f5)),
        ("cs", "\u{f81a}", Some(0x8ed08e)),
        ("java", "\u{e738}", Some(0xd05151)),
        ("kt", "\u{e634}", Some(0xc99cf0)),
        ("rb", "\u{e739}", Some(0xd05151)),
        ("php", "\u{e73d}", Some(0xc99cf0)),
        ("lua", "\u{e620}", Some(0x7ab8f5)),
        ("sh", "\u{f489}", Some(0x8ed08e)),
        ("bash", "\u{f489}", Some(0x8ed08e)),
        ("ps1", "\u{ebc7}", Some(0x7ab8f5)),
        ("bat", "\u{ebc7}", Some(0x8ed08e)),
        ("cmd", "\u{ebc7}", Some(0x8ed08e)),
        ("yml", "\u{e615}", Some(0xc99cf0)),
        ("yaml", "\u{e615}", Some(0xc99cf0)),
        ("xml", "\u{f72d}", Some(0xe8c87a)),
        ("sql", "\u{e706}", Some(0x6fd0d0)),
        ("txt", "\u{f15c}", None),
        ("log", "\u{f18d}", Some(0x798090)),
        ("pdf", "\u{f1c1}", Some(0xd05151)),
        ("zip", "\u{f410}", Some(0xe8c87a)),
        ("7z", "\u{f410}", Some(0xe8c87a)),
        ("rar", "\u{f410}", Some(0xe8c87a)),
        ("gz", "\u{f410}", Some(0xe8c87a)),
        ("tar", "\u{f410}", Some(0xe8c87a)),
        ("zst", "\u{f410}", Some(0xe8c87a)),
        ("png", "\u{f1c5}", Some(0xc99cf0)),
        ("jpg", "\u{f1c5}", Some(0xc99cf0)),
        ("jpeg", "\u{f1c5}", Some(0xc99cf0)),
        ("gif", "\u{f1c5}", Some(0xc99cf0)),
        ("webp", "\u{f1c5}", Some(0xc99cf0)),
        ("svg", "\u{f1c5}", Some(0xe8c87a)),
        ("ico", "\u{f1c5}", Some(0xc99cf0)),
        ("bmp", "\u{f1c5}", Some(0xc99cf0)),
        ("mp4", "\u{f03d}", Some(0xe8c87a)),
        ("mkv", "\u{f03d}", Some(0xe8c87a)),
        ("webm", "\u{f03d}", Some(0xe8c87a)),
        ("mov", "\u{f03d}", Some(0xe8c87a)),
        ("mp3", "\u{f001}", Some(0xe8c87a)),
        ("flac", "\u{f001}", Some(0xe8c87a)),
        ("wav", "\u{f001}", Some(0xe8c87a)),
        ("exe", "\u{f17a}", Some(0x8ed08e)),
        ("msi", "\u{f17a}", Some(0x8ed08e)),
        ("dll", "\u{f085}", Some(0x798090)),
        ("ttf", "\u{f031}", Some(0xc8cdd8)),
        ("otf", "\u{f031}", Some(0xc8cdd8)),
        ("db", "\u{e706}", Some(0x6fd0d0)),
        ("sqlite", "\u{e706}", Some(0x6fd0d0)),
        ("lock", "\u{f023}", Some(0x798090)),
    ]
    .into_iter()
    .map(|(n, g, col)| (n.to_owned(), icon(t, g, col)))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ito_theme::contrast;

    /// The colour a plain file's name is drawn in.
    fn plain(t: &Theme) -> Color32 {
        t.filetypes.iter().find(|r| r.mime.as_deref() == Some("text/*")).unwrap().style.fg.unwrap()
    }

    /// #203: yazi's `[app] overall` sets the window's background and text,
    /// and the second background follows the first.
    #[test]
    fn overall_sets_the_window_colours() {
        let t: ThemeToml = toml::from_str("[app]\noverall = { bg = \"#ffffff\", fg = \"#000000\" }\n").unwrap();
        let mut theme = Theme::default();
        theme.apply(&t);
        assert_eq!(theme.bg, Color32::WHITE);
        assert_eq!(theme.fg, Color32::BLACK);
        assert_eq!(theme.bg_alt, Color32::from_rgb(247, 247, 247), "a step towards the text");

        // Without it, nothing moves.
        let mut plain = Theme::default();
        plain.apply(&toml::from_str::<ThemeToml>("").unwrap());
        assert_eq!((plain.bg, plain.fg), (Theme::default().bg, Theme::default().fg));
    }

    /// Q93: on a light background the warning and a plain file's name turn
    /// dark, and both read at 4.5:1 or better on the background, the panels'
    /// second one and the cursor's bar; a dark background keeps the light pair.
    #[test]
    fn a_light_background_darkens_the_warning_and_plain_names() {
        let dark = Theme::default();
        assert!(contrast(dark.warning, dark.bg) >= 4.5 && contrast(plain(&dark), dark.bg) >= 4.5);
        assert!(contrast(plain(&dark), dark.hovered_bg) >= 4.5, "the cursor's row, dark");

        for toml in [
            "[app]\noverall = { bg = \"#ffffff\", fg = \"#222222\" }\n",
            "[app]\noverall = { bg = \"#ffffff\" }\n",
            "[app]\noverall = { bg = \"#f0ead6\" }\n",
        ] {
            let mut t = Theme::default();
            t.apply(&toml::from_str::<ThemeToml>(toml).unwrap());
            for bg in [t.bg, t.bg_alt, t.hovered_bg, t.inactive_hovered_bg] {
                assert!(contrast(t.warning, bg) >= 4.5, "warning on {bg:?}: {toml}");
                assert!(contrast(plain(&t), bg) >= 4.5, "plain name on {bg:?}: {toml}");
                assert!(contrast(t.fg, bg) >= 4.5, "text on {bg:?}: {toml}");
            }
        }
        // A text colour the file sets is the file's.
        let mut t = Theme::default();
        t.apply(&toml::from_str::<ThemeToml>("[app]\noverall = { bg = \"#ffffff\", fg = \"#222222\" }\n").unwrap());
        assert_eq!(t.fg, Color32::from_rgb(0x22, 0x22, 0x22));

        // A dark background chosen in the file keeps the light pair.
        let mut t = Theme::default();
        t.apply(&toml::from_str::<ThemeToml>("[app]\noverall = { bg = \"#000000\" }\n").unwrap());
        assert_eq!((t.warning, plain(&t), t.fg), (dark.warning, plain(&dark), dark.fg));
        assert_eq!(t.hovered_bg, dark.hovered_bg);
    }

    /// Q98: with no theme.toml, kura is the common theme, so every built-in
    /// one, light or dark, reads: the text on each ground, the quiet text,
    /// the warnings, the found words.
    #[test]
    fn every_common_theme_reads_in_kura() {
        for th in ito_theme::builtin() {
            let t = Theme::from_common(&th.colors);
            let n = &th.name;
            assert_eq!(t.light(), th.colors.light, "{n}");
            assert_eq!((t.bg, t.fg), (th.colors.bg, th.colors.fg), "{n} is the window");
            for bg in [t.bg, t.bg_alt, t.hovered_bg, t.inactive_hovered_bg] {
                assert!(contrast(t.fg, bg) >= 4.0, "{n}: text on {bg:?}");
                assert!(contrast(plain(&t), bg) >= 4.0, "{n}: plain name on {bg:?}");
            }
            assert!(contrast(t.warning, t.bg) >= 4.5, "{n}: warning");
            assert!(contrast(t.fg_dim, t.bg) >= 3.5, "{n}: quiet text");
            let kw = t.find_keyword;
            assert!(contrast(kw.fg.unwrap(), kw.bg.unwrap()) >= 3.5, "{n}: a found word");
        }
    }

    /// No common.toml is `tsumugi Dark`, kura's colours before Q98.
    #[test]
    fn the_default_is_tsumugi_dark() {
        let t = Theme::default();
        let c = ito_theme::builtin()[0].colors;
        assert_eq!(t.common, c);
        assert_eq!((t.bg, t.fg, t.border), (c.bg, c.fg, c.border));
        assert!(!t.light() && !t.overall);
        assert_eq!(t.syntect_theme, "base16-ocean.dark");
    }

    /// theme.toml is drawn over the common theme, whichever it is, and stays
    /// over it when another is chosen; so do the ASCII icons.
    #[test]
    fn theme_toml_is_drawn_over_the_common_theme() {
        let builtin = ito_theme::builtin();
        let light = builtin.iter().find(|t| t.name == "tsumugi Light").unwrap().colors;
        let marker = "[mgr]\nmarker_selected = { fg = \"#ff0000\" }\n";
        let t = Theme::layered(&light, vec![toml::from_str(marker).unwrap()]);
        assert!(t.light() && !t.overall, "the window is still the common theme's");
        assert_eq!(t.marker_selected, Color32::from_rgb(255, 0, 0));
        assert_eq!(t.syntect_theme, "base16-ocean.light");

        let mut back = t.with_common(&builtin[0].colors);
        assert!(!back.light());
        assert_eq!(back.marker_selected, Color32::from_rgb(255, 0, 0), "the layer goes along");
        back.without_nerd_icons();
        let again = back.with_common(&light);
        assert!(again.light() && again.plain_icons);
        assert_eq!(again.icon_dir_default.text, "/");
        assert_eq!(again.marker_selected, Color32::from_rgb(255, 0, 0));

        // A theme.toml that paints the window dark over a light theme turns
        // the rest with it.
        let dark = "[app]\noverall = { bg = \"#101010\" }\n";
        let t = Theme::layered(&light, vec![toml::from_str(dark).unwrap()]);
        assert!(!t.light() && t.overall);
        for bg in [t.bg, t.bg_alt, t.hovered_bg] {
            assert!(contrast(t.fg, bg) >= 4.5, "text on {bg:?}");
            assert!(contrast(plain(&t), bg) >= 4.5, "plain name on {bg:?}");
        }
        assert!(contrast(t.warning, t.bg) >= 4.5);
    }
}
