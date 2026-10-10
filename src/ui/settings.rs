//! The settings screen (`<C-,>`), as every uchmk app draws it: `ito_prefs`
//! makes the frame, the nav and the search, and the shared Language and
//! CLOCK rows; kura hands it its pages and its colours.
//!
//! General writes common.toml (the language and the clock every uchmk app
//! shares) through `App::write_common`, off the UI thread. config.toml shows
//! the main rows of kura's own file and writes a change through
//! `App::write_kura`, off the UI thread and keeping the file's comments.
//! Advanced says where the config is read from and what was wrong with it.

use std::path::PathBuf;

use egui::{Rect, RichText, Ui};
use ito_prefs::{button, clock_card, field, language_row, row, scale_row, section, select, sep, switch, Drafts, Look, Nav, Words};
use ito_theme::{mix, Colors};

use crate::app::{App, KuraChange, Overlay};
use crate::config::cmd::Act;
use crate::config::theme::{is_light, Theme};

/// kura's own words on the screen, in one language.
pub struct KuraWords {
    pub general: &'static str,
    pub general_lead: &'static str,
    pub language_note: &'static str,
    pub scale_note: &'static str,
    pub advanced: &'static str,
    pub advanced_lead: &'static str,
    pub folders: &'static str,
    pub kura_note: &'static str,
    pub yazi_note: &'static str,
    pub common_note: &'static str,
    pub copy: &'static str,
    pub open: &'static str,
    pub copied: &'static str,
    pub problems: &'static str,
    pub no_problems: &'static str,
    pub no_problems_note: &'static str,
    pub reload: &'static str,
    pub reload_note: &'static str,
    pub reload_button: &'static str,
    pub no_folder: &'static str,
    pub kura_lead: &'static str,
    pub look: &'static str,
    pub font_size: &'static str,
    pub font_size_note: &'static str,
    pub row_padding: &'static str,
    pub row_padding_note: &'static str,
    pub icons: &'static str,
    pub icons_note: &'static str,
    pub minimap: &'static str,
    pub minimap_note: &'static str,
    pub preview: &'static str,
    pub render_markdown: &'static str,
    pub render_markdown_note: &'static str,
    pub debounce: &'static str,
    pub debounce_note: &'static str,
    pub max_text: &'static str,
    pub max_text_note: &'static str,
    pub list: &'static str,
    pub history: &'static str,
    pub history_note: &'static str,
    pub window: &'static str,
    pub width: &'static str,
    pub height: &'static str,
    pub size_note: &'static str,
    pub backend: &'static str,
    pub backend_note: &'static str,
    pub mcp: &'static str,
    pub mcp_note: &'static str,
    pub terminal: &'static str,
    pub shell: &'static str,
    pub shell_note: &'static str,
    pub shell_env_note: &'static str,
    pub shell_hint: &'static str,
    /// What a number field says of words that are not a number it takes:
    /// the row, the words, the lowest and the highest.
    pub not_a_number: fn(&str, &str, f64, f64) -> String,
}

fn not_a_number_en(label: &str, text: &str, lo: f64, hi: f64) -> String {
    format!("{label}: \"{text}\" is not a number from {lo} to {hi}")
}

fn not_a_number_ja(label: &str, text: &str, lo: f64, hi: f64) -> String {
    format!("{label}: 「{text}」は {lo} から {hi} までの数ではない")
}

pub const EN: KuraWords = KuraWords {
    general: "General",
    general_lead: "The language, the scale and the clock, shared with every uchmk app through common.toml.",
    language_note: "The words of this screen and the clock's weekday; the rest of kura is in English for now",
    scale_note: "The size of everything on screen (the same as Ctrl+= and Ctrl+-)",
    advanced: "Advanced",
    advanced_lead: "Where kura reads its settings, and what it found wrong in them.",
    folders: "CONFIG FOLDERS",
    kura_note: "config.toml, and kura's own keymap.toml and theme.toml",
    yazi_note: "yazi's yazi.toml, keymap.toml and theme.toml, read as they are",
    common_note: "common.toml: the language, the clock and the theme of every uchmk app",
    copy: "Copy",
    open: "Open",
    copied: "Copied",
    problems: "PROBLEMS READING THE CONFIG",
    no_problems: "No problems",
    no_problems_note: "Every config file read cleanly",
    reload: "Read the config again",
    reload_note: "After editing a file by hand (the same as Ctrl+F5)",
    reload_button: "Reload",
    no_folder: "(no config folder on this system)",
    kura_lead: "kura's own settings. A change is written into config.toml at once, keeping its comments, and read back as Ctrl+F5 does.",
    look: "LOOK",
    font_size: "Font size",
    font_size_note: "Points, for the list, the preview and the panels",
    row_padding: "Row padding",
    row_padding_note: "Points added to the height of each row of the list",
    icons: "Icons",
    icons_note: "auto: a Nerd Font's icons when one is installed, else plain letters",
    minimap: "Minimap",
    minimap_note: "The shape of the whole file down the right of a text preview (Alt+N)",
    preview: "PREVIEW",
    render_markdown: "Rendered Markdown",
    render_markdown_note: "How a Markdown preview starts; M switches the one on screen. Read at start",
    debounce: "Wait before previewing",
    debounce_note: "Milliseconds the cursor rests before the preview is asked for",
    max_text: "Text read for a preview",
    max_text_note: "Bytes of a text file the preview reads",
    list: "LIST",
    history: "Folders remembered",
    history_note: "How many folders z can jump back to",
    window: "WINDOW",
    width: "Window width",
    height: "Window height",
    size_note: "Points, when kura starts",
    backend: "Drawing",
    backend_note: "What draws the window; read at start (WGPU_BACKEND wins for one run)",
    mcp: "kura mcp",
    mcp_note: "Lets `kura mcp` read this window, for the same user only. Read at start",
    terminal: "TERMINAL",
    shell: "Shell",
    shell_note: "What Ctrl+T starts; empty for the system's default. A pane already open keeps its own",
    shell_env_note: "KURA_TERM_SHELL names the shell for this run",
    shell_hint: "default",
    not_a_number: not_a_number_en,
};

pub const JA: KuraWords = KuraWords {
    general: "一般",
    general_lead: "言語・倍率・時計。common.toml を通して uchmk のどのアプリにも同じものが当たる。",
    language_note: "この画面の言葉と時計の曜日。kura のほかの言葉はいまは英語",
    scale_note: "画面のすべての大きさ（Ctrl+= と Ctrl+- と同じ）",
    advanced: "詳細",
    advanced_lead: "kura が設定を読む場所と、読んだときに見つかった問題。",
    folders: "設定のフォルダ",
    kura_note: "config.toml と、kura だけの keymap.toml・theme.toml",
    yazi_note: "yazi の yazi.toml・keymap.toml・theme.toml をそのまま読む",
    common_note: "common.toml: uchmk のどのアプリにも当たる言語・時計・テーマ",
    copy: "コピー",
    open: "開く",
    copied: "コピーしました",
    problems: "設定を読んだときの問題",
    no_problems: "問題はありません",
    no_problems_note: "どの設定ファイルも問題なく読めた",
    reload: "設定を読み直す",
    reload_note: "ファイルを手で直したあとに（Ctrl+F5 と同じ）",
    reload_button: "読み直す",
    no_folder: "（この環境には設定のフォルダがない）",
    kura_lead: "kura だけの設定。変えるとすぐ config.toml に書き（コメントは残す）、Ctrl+F5 と同じに読み直す。",
    look: "見た目",
    font_size: "文字の大きさ",
    font_size_note: "ポイント。一覧・プレビュー・パネルの文字",
    row_padding: "行の余白",
    row_padding_note: "一覧の 1 行の高さに足すポイント",
    icons: "アイコン",
    icons_note: "auto は Nerd Font があればそのアイコン、無ければ文字",
    minimap: "ミニマップ",
    minimap_note: "テキストのプレビューの右にファイル全体の形（Alt+N）",
    preview: "プレビュー",
    render_markdown: "Markdown を整えて出す",
    render_markdown_note: "Markdown のプレビューの出し始め。画面のものは M で切り替える。起動時に読む",
    debounce: "プレビューまでの待ち",
    debounce_note: "カーソルが止まってからプレビューを頼むまでのミリ秒",
    max_text: "プレビューで読む量",
    max_text_note: "テキストファイルから読むバイト数",
    list: "一覧",
    history: "覚えるフォルダの数",
    history_note: "z で戻れるフォルダの数",
    window: "ウィンドウ",
    width: "ウィンドウの幅",
    height: "ウィンドウの高さ",
    size_note: "ポイント。起動時に読む",
    backend: "描画",
    backend_note: "ウィンドウを描くもの。起動時に読む（WGPU_BACKEND があればその回はそちら）",
    mcp: "kura mcp",
    mcp_note: "`kura mcp` がこのウィンドウを読めるようにする（同じユーザーだけ）。起動時に読む",
    terminal: "ターミナル",
    shell: "シェル",
    shell_note: "Ctrl+T で起動するもの。空なら OS の既定。開いているペインはそのまま",
    shell_env_note: "KURA_TERM_SHELL がこの回のシェルを決めている",
    shell_hint: "既定",
    not_a_number: not_a_number_ja,
};

impl KuraWords {
    pub fn of(language: &str) -> &'static KuraWords {
        match language {
            "ja" => &JA,
            _ => &EN,
        }
    }
}

/// kura's theme in the shape uchmk's screens paint with: the list's
/// background, the header's band for the nav, the text and its dim, and the
/// status colours for what is on, waiting and wrong.
pub fn colors(t: &Theme) -> Colors {
    let run = t.mode_normal.bg.unwrap_or(t.progress_fg);
    Colors {
        light: is_light(t.bg),
        bg: t.bg,
        side: t.bg_alt,
        panel: mix(t.bg, t.fg, 0.04),
        border: t.border,
        fg: t.fg,
        dim: t.fg_dim,
        wait: t.warning,
        run,
        err: t.progress_error,
        done: t.git_added,
        blue: run,
        magenta: t.marker_marked,
        ansi16: None,
    }
}

/// The pages, in the nav's order.
const GENERAL: usize = 0;
const KURA: usize = 1;
const ADVANCED: usize = 2;

/// What the config.toml page shows: the settings in force, read before the
/// screen takes the overlay.
struct KuraNow {
    font_size: String,
    row_padding: String,
    icons: String,
    minimap: bool,
    render_markdown: bool,
    debounce: String,
    max_text: String,
    history: String,
    width: String,
    height: String,
    backend: String,
    mcp: bool,
    shell: String,
    shell_from_env: bool,
}

impl KuraNow {
    fn of(app: &App) -> Self {
        let u = &app.cfg.ui;
        Self {
            font_size: u.font_size.to_string(),
            row_padding: u.row_padding.to_string(),
            icons: u.icons.trim().to_ascii_lowercase(),
            minimap: u.minimap,
            render_markdown: u.render_markdown,
            debounce: u.preview_debounce_ms.to_string(),
            max_text: u.max_text_bytes.to_string(),
            history: u.max_history.to_string(),
            width: u.window_width.to_string(),
            height: u.window_height.to_string(),
            backend: u.backend_name().ok().flatten().unwrap_or("auto").to_owned(),
            mcp: app.cfg.mcp.enable,
            shell: app.cfg.term.shell.clone(),
            shell_from_env: app.cfg.term.from_env,
        }
    }
}

/// A number row of config.toml: its key in `[ui]`, its words, what it is now,
/// and the numbers it takes.
struct Num<'a> {
    key: &'static str,
    label: &'a str,
    note: &'a str,
    now: &'a str,
    lo: f64,
    hi: f64,
    /// An integer in the file (`max_history = 200`), not a float.
    whole: bool,
}

/// Draw a number row: a field that writes on Enter or on leaving it.
/// Words that are not a number in range are said in `bad`, and not written.
fn number_row(ui: &mut Ui, l: Look, drafts: &mut Drafts, n: Num, k: &KuraWords, out: &mut Vec<KuraChange>, bad: &mut Option<String>) {
    let Some(text) = row(ui, l, n.label, n.note, |ui| field(ui, drafts, n.key, n.now, "", 110.0)) else { return };
    match number_toml(&text, n.lo, n.hi, n.whole) {
        Some(value) => out.push(KuraChange { table: "ui", key: n.key, value: Some(value) }),
        None => *bad = Some((k.not_a_number)(n.label, text.trim(), n.lo, n.hi)),
    }
}

/// `text` as the TOML for a number from `lo` to `hi`: `16.0` for a float,
/// so the file reads as the README writes it; `200` for a whole one.
fn number_toml(text: &str, lo: f64, hi: f64, whole: bool) -> Option<String> {
    let v: f64 = text.trim().parse().ok()?;
    if !v.is_finite() || v < lo || v > hi || (whole && v.fract() != 0.0) {
        return None;
    }
    Some(if whole { format!("{}", v as u64) } else { format!("{v:?}") })
}

/// The backends this system can draw with, as `[ui] backend` names them.
fn backends() -> Vec<(&'static str, &'static str)> {
    let mut list = vec![("auto", "auto"), ("vulkan", "Vulkan"), ("gl", "OpenGL")];
    if cfg!(windows) {
        list.push(("dx12", "DirectX 12"));
    }
    if cfg!(target_os = "macos") {
        list.push(("metal", "Metal"));
    }
    list
}

/// The config.toml page: the main rows of `[ui]`, `[mcp]` and `[term]`.
fn kura_page(ui: &mut Ui, l: Look, k: &KuraWords, now: &KuraNow, drafts: &mut Drafts, out: &mut Vec<KuraChange>, bad: &mut Option<String>) {
    let set = |key: &'static str, value: String| KuraChange { table: "ui", key, value: Some(value) };
    section(ui, l, k.look, |ui| {
        let n = Num { key: "font_size", label: k.font_size, note: k.font_size_note, now: &now.font_size, lo: 6.0, hi: 72.0, whole: false };
        number_row(ui, l, drafts, n, k, out, bad);
        sep(ui, l);
        let n = Num { key: "row_padding", label: k.row_padding, note: k.row_padding_note, now: &now.row_padding, lo: 0.0, hi: 40.0, whole: false };
        number_row(ui, l, drafts, n, k, out, bad);
        sep(ui, l);
        let icons = [("auto", "auto"), ("nerd", "nerd"), ("ascii", "ascii"), ("none", "none")];
        let picked = row(ui, l, k.icons, k.icons_note, |ui| select(ui, "icons", now.icons.as_str(), &icons));
        out.extend(picked.map(|v| set("icons", format!("\"{v}\""))));
        sep(ui, l);
        if row(ui, l, k.minimap, k.minimap_note, |ui| switch(ui, l, now.minimap)) {
            out.push(set("minimap", (!now.minimap).to_string()));
        }
    });
    section(ui, l, k.preview, |ui| {
        if row(ui, l, k.render_markdown, k.render_markdown_note, |ui| switch(ui, l, now.render_markdown)) {
            out.push(set("render_markdown", (!now.render_markdown).to_string()));
        }
        sep(ui, l);
        let n = Num { key: "preview_debounce_ms", label: k.debounce, note: k.debounce_note, now: &now.debounce, lo: 0.0, hi: 5000.0, whole: true };
        number_row(ui, l, drafts, n, k, out, bad);
        sep(ui, l);
        let n = Num { key: "max_text_bytes", label: k.max_text, note: k.max_text_note, now: &now.max_text, lo: 1024.0, hi: 1073741824.0, whole: true };
        number_row(ui, l, drafts, n, k, out, bad);
    });
    section(ui, l, k.list, |ui| {
        let n = Num { key: "max_history", label: k.history, note: k.history_note, now: &now.history, lo: 0.0, hi: 100000.0, whole: true };
        number_row(ui, l, drafts, n, k, out, bad);
    });
    section(ui, l, k.window, |ui| {
        let n = Num { key: "window_width", label: k.width, note: k.size_note, now: &now.width, lo: 200.0, hi: 20000.0, whole: false };
        number_row(ui, l, drafts, n, k, out, bad);
        sep(ui, l);
        let n = Num { key: "window_height", label: k.height, note: k.size_note, now: &now.height, lo: 150.0, hi: 20000.0, whole: false };
        number_row(ui, l, drafts, n, k, out, bad);
        sep(ui, l);
        let list = backends();
        let picked = row(ui, l, k.backend, k.backend_note, |ui| select(ui, "backend", now.backend.as_str(), &list));
        out.extend(picked.map(|v| set("backend", format!("\"{v}\""))));
        sep(ui, l);
        if row(ui, l, k.mcp, k.mcp_note, |ui| switch(ui, l, now.mcp)) {
            out.push(KuraChange { table: "mcp", key: "enable", value: Some((!now.mcp).to_string()) });
        }
    });
    section(ui, l, k.terminal, |ui| {
        let note = match now.shell_from_env {
            true => format!("{}\n{}", k.shell_note, k.shell_env_note),
            false => k.shell_note.to_owned(),
        };
        if let Some(text) = row(ui, l, k.shell, &note, |ui| field(ui, drafts, "shell", &now.shell, k.shell_hint, 220.0)) {
            let text = text.trim();
            // Empty is the system's default, which is the key not there.
            let value = (!text.is_empty()).then(|| toml::Value::String(text.to_owned()).to_string());
            out.push(KuraChange { table: "term", key: "shell", value });
        }
    });
}

/// A config folder on the Advanced page: its name, its note, and where it is.
struct Folder {
    name: &'static str,
    note: &'static str,
    dir: Option<PathBuf>,
}

/// Draw the screen over `full`, in place of everything else.
pub fn draw(app: &mut App, ui: &mut Ui, full: Rect, queued: &mut Vec<Act>) {
    let c = colors(&app.cfg.theme);
    ito_theme::set(c);
    let w = Words::of(&app.lang);
    let k = KuraWords::of(&app.lang);
    let language = app.common.language.clone().unwrap_or_else(|| "auto".to_owned());
    let clock = app.clock.clone();
    let scale = app.scale;
    let warnings = app.cfg.warnings.clone();
    let folders = [
        Folder { name: "kura", note: k.kura_note, dir: App::kura_config_dir() },
        Folder { name: "yazi", note: k.yazi_note, dir: crate::config::config_home("YAZI_CONFIG_HOME") },
        Folder { name: "uchmk", note: k.common_note, dir: app.common_base.clone() },
    ];
    let now = KuraNow::of(app);
    let Overlay::Settings(ov) = &mut app.overlay else { return };
    let keys = std::mem::take(&mut ov.keys);
    let mut state = std::mem::take(&mut ov.state);
    let mut drafts = std::mem::take(&mut ov.drafts);

    let pages = [(k.general, k.general_lead), ("config.toml", k.kura_lead), (k.advanced, k.advanced_lead)];
    let mut index = vec![
        (GENERAL, w.language),
        (GENERAL, k.language_note),
        (GENERAL, w.scale),
        (GENERAL, k.scale_note),
        (GENERAL, w.clock),
        (GENERAL, w.show_time),
        (GENERAL, w.time_format),
        (GENERAL, w.show_date),
        (GENERAL, w.date_format),
        (GENERAL, w.weekday),
        (KURA, k.look),
        (KURA, k.font_size),
        (KURA, k.font_size_note),
        (KURA, k.row_padding),
        (KURA, k.row_padding_note),
        (KURA, k.icons),
        (KURA, k.icons_note),
        (KURA, k.minimap),
        (KURA, k.minimap_note),
        (KURA, k.preview),
        (KURA, k.render_markdown),
        (KURA, k.render_markdown_note),
        (KURA, k.debounce),
        (KURA, k.debounce_note),
        (KURA, k.max_text),
        (KURA, k.max_text_note),
        (KURA, k.list),
        (KURA, k.history),
        (KURA, k.history_note),
        (KURA, k.window),
        (KURA, k.width),
        (KURA, k.height),
        (KURA, k.size_note),
        (KURA, k.backend),
        (KURA, k.backend_note),
        (KURA, k.mcp),
        (KURA, k.mcp_note),
        (KURA, k.terminal),
        (KURA, k.shell),
        (KURA, k.shell_note),
        (ADVANCED, k.folders),
        (ADVANCED, k.problems),
        (ADVANCED, k.reload),
        (ADVANCED, k.reload_note),
    ];
    for f in &folders {
        index.extend([(ADVANCED, f.name), (ADVANCED, f.note)]);
    }
    let nav = Nav { pages: &pages, index: &index, words: w, file: "config.toml" };

    let mut changes = Vec::new();
    let mut kura = Vec::new();
    let mut bad: Option<String> = None;
    let mut copy: Option<String> = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(full));
    child.set_clip_rect(full);
    child.style_mut().visuals = c.visuals();
    let out = ito_prefs::show(&mut child, c, &mut state, &nav, keys, |ui, page, l| match page {
        GENERAL => {
            section(ui, l, &w.language.to_uppercase(), |ui| {
                changes.extend(language_row(ui, l, w, &language, k.language_note));
            });
            section(ui, l, &w.scale.to_uppercase(), |ui| {
                changes.extend(scale_row(ui, l, w, scale, k.scale_note));
            });
            changes.extend(clock_card(ui, l, w, &clock));
        }
        KURA => kura_page(ui, l, k, &now, &mut drafts, &mut kura, &mut bad),
        _ => {
            section(ui, l, k.folders, |ui| {
                for (i, f) in folders.iter().enumerate() {
                    if i > 0 {
                        sep(ui, l);
                    }
                    let path = f.dir.as_ref().map_or_else(|| k.no_folder.to_owned(), |d| d.display().to_string());
                    let note = format!("{}\n{path}", f.note);
                    let (copied, opened) = row(ui, l, f.name, &note, |ui| {
                        if f.dir.is_none() {
                            return (false, false);
                        }
                        // Right to left: Open ends the row.
                        let opened = button(ui, l, k.open);
                        let copied = button(ui, l, k.copy);
                        (copied, opened)
                    });
                    if copied {
                        copy = Some(path.clone());
                    }
                    if opened {
                        // Out of the screen and into the folder, in the list.
                        queued.push(Act::Settings);
                        queued.push(Act::Cd { target: path, interactive: false });
                    }
                }
            });
            section(ui, l, k.problems, |ui| {
                if warnings.is_empty() {
                    row(ui, l, k.no_problems, k.no_problems_note, |_| ());
                }
                for (i, text) in warnings.iter().enumerate() {
                    if i > 0 {
                        sep(ui, l);
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new(text).monospace().size(12.0).color(l.c.wait));
                    ui.add_space(8.0);
                }
                sep(ui, l);
                if row(ui, l, k.reload, k.reload_note, |ui| button(ui, l, k.reload_button)) {
                    queued.push(Act::ConfigReload);
                }
            });
        }
    });
    if let Overlay::Settings(ov) = &mut app.overlay {
        ov.state = state;
        ov.drafts = drafts;
    }
    if out.close {
        queued.push(Act::Settings);
    }
    if out.open_file {
        app.open_own_config();
    }
    if let Some(path) = copy {
        match crate::exec::set_clipboard(&path) {
            Ok(()) => app.toast(format!("{}: {path}", k.copied)),
            Err(e) => app.error(format!("Copy failed: {e}")),
        }
    }
    app.write_common(changes);
    app.write_kura(kura);
    if let Some(e) = bad {
        app.error(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A float is written as the README writes it, a whole number without a
    /// point, and words that are not a number in range are not written.
    #[test]
    fn a_number_row_writes_toml_only_for_a_number_in_range() {
        assert_eq!(number_toml("16", 6.0, 72.0, false).as_deref(), Some("16.0"));
        assert_eq!(number_toml(" 12.5 ", 6.0, 72.0, false).as_deref(), Some("12.5"));
        assert_eq!(number_toml("200", 0.0, 100000.0, true).as_deref(), Some("200"));
        assert_eq!(number_toml("abc", 6.0, 72.0, false), None);
        assert_eq!(number_toml("", 6.0, 72.0, false), None);
        assert_eq!(number_toml("5", 6.0, 72.0, false), None);
        assert_eq!(number_toml("73", 6.0, 72.0, false), None);
        assert_eq!(number_toml("inf", 6.0, f64::INFINITY, false), None);
        assert_eq!(number_toml("1.5", 0.0, 100.0, true), None);
    }

    /// What was typed and the range it takes are said, in either language.
    #[test]
    fn the_kura_toml_page_says_why_a_number_is_not_written() {
        let en = (KuraWords::of("en").not_a_number)("Font size", "abc", 6.0, 72.0);
        assert!(en.contains("abc") && en.contains("6") && en.contains("72"), "{en}");
        let ja = (KuraWords::of("ja").not_a_number)("文字の大きさ", "abc", 6.0, 72.0);
        assert!(ja.contains("abc") && ja.contains("72"), "{ja}");
    }
}
