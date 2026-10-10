//! The settings screen (`<C-,>`), as every uchmk app draws it: `ito_prefs`
//! makes the frame, the nav and the search, and the shared Language and
//! CLOCK rows; kura hands it its pages and its colours.
//!
//! General writes common.toml (the language and the clock every uchmk app
//! shares) through `App::write_common`, off the UI thread. Advanced says
//! where the config is read from and what was wrong with it.

use std::path::PathBuf;

use egui::{Rect, RichText, Ui};
use ito_prefs::{button, clock_card, language_row, row, section, sep, Nav, Words};
use ito_theme::{mix, Colors};

use crate::app::{App, Overlay};
use crate::config::cmd::Act;
use crate::config::theme::{is_light, Theme};

/// kura's own words on the screen, in one language.
pub struct KuraWords {
    pub general: &'static str,
    pub general_lead: &'static str,
    pub language_note: &'static str,
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
}

pub const EN: KuraWords = KuraWords {
    general: "General",
    general_lead: "The language and the clock, shared with every uchmk app through common.toml.",
    language_note: "The words of this screen and the clock's weekday; the rest of kura is in English for now",
    advanced: "Advanced",
    advanced_lead: "Where kura reads its settings, and what it found wrong in them.",
    folders: "CONFIG FOLDERS",
    kura_note: "kura.toml, and kura's own keymap.toml and theme.toml",
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
};

pub const JA: KuraWords = KuraWords {
    general: "一般",
    general_lead: "言語と時計。common.toml を通して uchmk のどのアプリにも同じものが当たる。",
    language_note: "この画面の言葉と時計の曜日。kura のほかの言葉はいまは英語",
    advanced: "詳細",
    advanced_lead: "kura が設定を読む場所と、読んだときに見つかった問題。",
    folders: "設定のフォルダ",
    kura_note: "kura.toml と、kura だけの keymap.toml・theme.toml",
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
const ADVANCED: usize = 1;

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
    let warnings = app.cfg.warnings.clone();
    let folders = [
        Folder { name: "kura", note: k.kura_note, dir: App::kura_config_dir() },
        Folder { name: "yazi", note: k.yazi_note, dir: crate::config::config_home("YAZI_CONFIG_HOME") },
        Folder { name: "uchmk", note: k.common_note, dir: app.common_base.clone() },
    ];
    let Overlay::Settings(ov) = &mut app.overlay else { return };
    let keys = std::mem::take(&mut ov.keys);
    let mut state = std::mem::take(&mut ov.state);

    let pages = [(k.general, k.general_lead), (k.advanced, k.advanced_lead)];
    let mut index = vec![
        (GENERAL, w.language),
        (GENERAL, k.language_note),
        (GENERAL, w.clock),
        (GENERAL, w.show_time),
        (GENERAL, w.time_format),
        (GENERAL, w.show_date),
        (GENERAL, w.date_format),
        (GENERAL, w.weekday),
        (ADVANCED, k.folders),
        (ADVANCED, k.problems),
        (ADVANCED, k.reload),
        (ADVANCED, k.reload_note),
    ];
    for f in &folders {
        index.extend([(ADVANCED, f.name), (ADVANCED, f.note)]);
    }
    let nav = Nav { pages: &pages, index: &index, words: w, file: "kura.toml" };

    let mut changes = Vec::new();
    let mut copy: Option<String> = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(full));
    child.set_clip_rect(full);
    child.style_mut().visuals = c.visuals();
    let out = ito_prefs::show(&mut child, c, &mut state, &nav, keys, |ui, page, l| match page {
        GENERAL => {
            section(ui, l, &w.language.to_uppercase(), |ui| {
                changes.extend(language_row(ui, l, w, &language, k.language_note));
            });
            changes.extend(clock_card(ui, l, w, &clock));
        }
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
    }
    if out.close {
        queued.push(Act::Settings);
    }
    if out.open_file {
        app.open_kura_toml();
    }
    if let Some(path) = copy {
        match crate::exec::set_clipboard(&path) {
            Ok(()) => app.toast(format!("{}: {path}", k.copied)),
            Err(e) => app.error(format!("Copy failed: {e}")),
        }
    }
    app.write_common(changes);
}
