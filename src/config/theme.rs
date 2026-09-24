//! yazi-compatible theming: `theme.toml` colors, `[filetype]` rules and
//! `[icon]` rules, on top of a built-in dark theme.

use egui::Color32;
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
}

impl Default for Theme {
    fn default() -> Self {
        let fg = Color32::from_rgb(0xc8, 0xcd, 0xd8);
        Self {
            bg: Color32::from_rgb(0x16, 0x18, 0x1d),
            bg_alt: Color32::from_rgb(0x1b, 0x1e, 0x24),
            border: Color32::from_rgb(0x2c, 0x30, 0x39),
            fg,
            fg_dim: Color32::from_rgb(0x79, 0x80, 0x90),

            cwd: Style::fg(Color32::from_rgb(0x6f, 0xd0, 0xd0)),
            hovered: Style::default(),
            hovered_bg: Color32::from_rgb(0x2f, 0x4a, 0x6b),
            inactive_hovered_bg: Color32::from_rgb(0x26, 0x2b, 0x34),
            preview_hovered: Style { underline: true, ..Default::default() },
            find_keyword: Style {
                fg: Some(Color32::from_rgb(0x1a, 0x1a, 0x1a)),
                bg: Some(Color32::from_rgb(0xe8, 0xc8, 0x7a)),
                bold: true,
                ..Default::default()
            },
            find_position: Style::fg(Color32::from_rgb(0xc9, 0x9c, 0xf0)),
            marker_copied: Color32::from_rgb(0x8e, 0xd0, 0x8e),
            marker_cut: Color32::from_rgb(0xf0, 0x71, 0x78),
            marker_marked: Color32::from_rgb(0x6f, 0xd0, 0xd0),
            marker_selected: Color32::from_rgb(0xe8, 0xc8, 0x7a),
            tab_active: Style {
                fg: Some(Color32::from_rgb(0x16, 0x18, 0x1d)),
                bg: Some(Color32::from_rgb(0x7a, 0xb8, 0xf5)),
                bold: true,
                ..Default::default()
            },
            tab_inactive: Style::fg(Color32::from_rgb(0x79, 0x80, 0x90)),

            mode_normal: Style {
                fg: Some(Color32::from_rgb(0x16, 0x18, 0x1d)),
                bg: Some(Color32::from_rgb(0x7a, 0xb8, 0xf5)),
                bold: true,
                ..Default::default()
            },
            mode_select: Style {
                fg: Some(Color32::from_rgb(0x16, 0x18, 0x1d)),
                bg: Some(Color32::from_rgb(0x8e, 0xd0, 0x8e)),
                bold: true,
                ..Default::default()
            },
            mode_unset: Style {
                fg: Some(Color32::from_rgb(0x16, 0x18, 0x1d)),
                bg: Some(Color32::from_rgb(0xf0, 0x71, 0x78)),
                bold: true,
                ..Default::default()
            },
            status_bg: Color32::from_rgb(0x20, 0x23, 0x2b),
            progress_fg: Color32::from_rgb(0x7a, 0xb8, 0xf5),
            progress_error: Color32::from_rgb(0xf0, 0x71, 0x78),

            which_cols: 3,
            which_cand: Style {
                fg: Some(Color32::from_rgb(0x6f, 0xd0, 0xd0)),
                bold: true,
                ..Default::default()
            },
            which_rest: Style::fg(Color32::from_rgb(0x79, 0x80, 0x90)),
            which_desc: Style::fg(Color32::from_rgb(0xc9, 0x9c, 0xf0)),

            // Git signs, in the colors the TODO asks for and yazi uses:
            // changed is yellow, added green, untracked quiet, a conflict red.
            git_modified: Color32::from_rgb(0xe8, 0xc8, 0x7a),
            git_deleted: Color32::from_rgb(0xf0, 0x71, 0x78),
            git_added: Color32::from_rgb(0x8e, 0xd0, 0x8e),
            git_untracked: Color32::from_rgb(0x79, 0x80, 0x90),
            git_conflict: Color32::from_rgb(0xf0, 0x71, 0x78),

            syntect_theme: "base16-ocean.dark".into(),

            filetypes: default_filetypes(),
            icon_globs: Vec::new(),
            icon_dirs: default_dir_icons(),
            icon_exts: default_ext_icons(),
            icon_files: default_file_icons(),
            icon_dir_default: Icon {
                text: "\u{f07b}".into(),
                fg: Some(Color32::from_rgb(0x7a, 0xb8, 0xf5)),
            },
            icon_file_default: Icon { text: "\u{f15b}".into(), fg: None },
            icon_link_default: Icon {
                text: "\u{f0c1}".into(),
                fg: Some(Color32::from_rgb(0x6f, 0xd0, 0xd0)),
            },
        }
    }
}

impl Theme {
    /// Swap the Nerd Font glyphs for plain ASCII when no icon font is available.
    pub fn without_nerd_icons(&mut self) {
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

fn c(hex: u32) -> Option<Color32> {
    Some(Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8))
}

fn default_filetypes() -> Vec<FileRule> {
    let mk = |name: Option<&str>, mime: Option<&str>, is: Option<&str>, col: u32, bold: bool| FileRule {
        name: name.map(str::to_owned),
        mime: mime.map(str::to_owned),
        is: is.map(str::to_owned),
        style: Style { fg: c(col), bold, ..Default::default() },
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
        mk(None, Some("text/*"), None, 0xc8cdd8, false),
        mk(None, None, Some("hidden"), 0x798090, false),
    ]
}

fn icon(glyph: &str, col: Option<u32>) -> Icon {
    Icon { text: glyph.to_owned(), fg: col.and_then(c) }
}

fn default_dir_icons() -> Vec<(String, Icon)> {
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
    .map(|(n, g, col)| (n.to_owned(), icon(g, col)))
    .collect()
}

fn default_file_icons() -> Vec<(String, Icon)> {
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
    .map(|(n, g, col)| (n.to_owned(), icon(g, col)))
    .collect()
}

fn default_ext_icons() -> Vec<(String, Icon)> {
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
    .map(|(n, g, col)| (n.to_owned(), icon(g, col)))
    .collect()
}
