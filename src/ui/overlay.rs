use egui::{Align2, CornerRadius, FontId, Rect, Stroke, Ui, Vec2};

use super::{dim, modal_frame, modal_rect};
use crate::app::{App, InputKind, Overlay, TaskState};
use crate::config::cmd::Act;

pub fn which(app: &App, ui: &mut Ui, rect: Rect, f: &FontId, row_h: f32) {
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, theme.bg_alt);
    painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, theme.border));

    let cols = theme.which_cols;
    let col_w = (rect.width() - 20.0) / cols as f32;
    let per_col = app.which.len().div_ceil(cols).max(1);
    for (i, (keys, desc, raw)) in app.which.iter().enumerate() {
        let col = i / per_col;
        let row = i % per_col;
        let x = rect.left() + 10.0 + col as f32 * col_w;
        let y = rect.top() + 7.0 + row as f32 * row_h;
        if y + row_h > rect.bottom() {
            continue;
        }
        let g = painter.layout_no_wrap(
            keys.clone(),
            f.clone(),
            theme.which_cand.fg.unwrap_or(theme.fg),
        );
        let kw = g.size().x;
        painter.galley(egui::pos2(x, y), g, theme.fg);
        let text = if desc.is_empty() { raw.clone() } else { desc.clone() };
        painter.text(
            egui::pos2(x + kw + 10.0, y),
            Align2::LEFT_TOP,
            crate::util::ellipsize_middle(&text, ((col_w - kw - 24.0) / (f.size * 0.6)) as usize),
            f.clone(),
            theme.which_desc.fg.unwrap_or(theme.fg_dim),
        );
    }
}

/// Paste the clipboard into a prompt on a right-click, the way a terminal does.
///
/// `<C-v>` already works: egui's `TextEdit` handles the platform's paste event.
/// But the prompts that most want text from somewhere else — `cd`, `s`, `;` —
/// are usually reached with a hand still on the mouse, having just copied a path
/// out of Explorer or a browser, and egui gives a text field neither a context
/// menu nor any way to ask for the clipboard. So both halves are done here.
///
/// Where it lands: egui moves the caret on a press of *any* button, the
/// secondary one included, so the text goes where the click was -- or over the
/// selection, when the click fell on it; `selected` is the field's selection as
/// the previous frame left it, since by now egui has collapsed it. Line breaks
/// become spaces, which is what egui itself does with a multi-line paste into a
/// one-line field — matching `<C-v>` matters more than any other choice here,
/// since a mouse paste that behaved differently would be a second thing to
/// learn.
///
/// `Ok(false)` means there was nothing to do. `Err` is a clipboard that could
/// not be read, which on every platform is also how an empty one reads.
fn right_click_paste(
    ui: &Ui,
    resp: &egui::Response,
    id: egui::Id,
    text: &mut String,
    selected: Option<std::ops::Range<usize>>,
) -> Result<bool, String> {
    if !resp.secondary_clicked() {
        return Ok(false);
    }
    let add = one_line(&crate::exec::get_clipboard()?);
    if add.is_empty() {
        return Ok(false);
    }
    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
    let chars = text.chars().count();
    let at = match state.cursor.char_range() {
        Some(r) => {
            let r = r.as_sorted_char_range();
            usize::from(r.start).min(chars)..usize::from(r.end).min(chars)
        }
        // Never clicked into: the end is where typing would have gone.
        None => chars..chars,
    };
    let at = paste_over(at, selected, chars);
    let caret = at.start + add.chars().count();
    *text = splice(text, at, &add);
    let one = egui::text::CCursorRange::one(egui::text::CCursor::new(caret));
    state.cursor.set_char_range(Some(one));
    state.store(ui.ctx(), id);
    Ok(true)
}

/// The clipboard as one line. A line break is one space whichever way it was
/// spelled: folding `\r` and `\n` one at a time made a Windows CRLF two, where
/// `<C-v>` makes one (30.4, #104).
fn one_line(clip: &str) -> String {
    clip.replace("\r\n", "\n").replace(['\r', '\n'], " ")
}

/// Where a right-click's paste goes. egui has collapsed any selection to the
/// click by the time the paste runs, so `at` is the click; `selected` is what
/// the frame before had selected. A click on the selection pastes over it, as
/// a browser and a terminal do (30.3, #104); anywhere else, where it fell.
fn paste_over(
    at: std::ops::Range<usize>,
    selected: Option<std::ops::Range<usize>>,
    chars: usize,
) -> std::ops::Range<usize> {
    match selected {
        Some(s) if s.start < s.end && s.end <= chars && (s.start..=s.end).contains(&at.start) => s,
        _ => at,
    }
}

/// The field's selection as it stands, in characters: read before the field
/// is drawn, it is what the previous frame left, which a press is about to
/// collapse.
fn selection(ui: &Ui, id: egui::Id) -> Option<std::ops::Range<usize>> {
    let state = egui::text_edit::TextEditState::load(ui.ctx(), id)?;
    let r = state.cursor.char_range()?.as_sorted_char_range();
    Some(usize::from(r.start)..usize::from(r.end))
}

/// The selection as it was when the right button went **down**.
///
/// A right-click is a press in one frame and a release in a later one, and the
/// paste happens on the release. egui collapses the selection on the press, so
/// by the release the frame before holds a caret, not the range: v0.55.0 read
/// that and pasted at the click on a real mouse, while a press and release sent
/// together (one frame) replaced as meant (#107). So the range is kept from the
/// frame the button went down.
fn selection_at_press(ui: &Ui, id: egui::Id) -> Option<std::ops::Range<usize>> {
    let key = id.with("selection-at-press");
    if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
        let now = selection(ui, id);
        ui.data_mut(|d| d.insert_temp(key, now));
    }
    ui.data(|d| d.get_temp::<Option<std::ops::Range<usize>>>(key)).flatten().or_else(|| selection(ui, id))
}

/// `text` with the character range `at` replaced by `add`.
///
/// Characters, not bytes. egui counts the caret in characters, while `&str`
/// indexes in bytes, and a path holding a Japanese folder name has more of the
/// second than the first — a caret index used as a byte offset lands inside a
/// character and panics.
fn splice(text: &str, at: std::ops::Range<usize>, add: &str) -> String {
    let byte = |n: usize| text.char_indices().nth(n).map_or(text.len(), |(i, _)| i);
    let mut out = String::with_capacity(text.len() + add.len());
    out.push_str(&text[..byte(at.start)]);
    out.push_str(add);
    out.push_str(&text[byte(at.end)..]);
    out
}

pub fn input(app: &mut App, ui: &mut Ui, rect: Rect, f: &FontId, queued: &mut Vec<Act>) {
    let theme_bg = app.cfg.theme.bg_alt;
    let theme_border = app.cfg.theme.border;
    let theme_fg = app.cfg.theme.fg;
    let accent = app.cfg.theme.cwd.fg.unwrap_or(theme_fg);

    ui.painter().rect_filled(rect, CornerRadius::ZERO, theme_bg);
    ui.painter()
        .line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, theme_border));

    // Tab hands the listing to the scan pool, so the answer can be a moment
    // behind on a slow share. Say so rather than look like the key did nothing.
    let waiting = app.completing();
    let gutter = if waiting { 22.0 } else { 0.0 };

    let Overlay::Input(ov) = &mut app.overlay else { return };
    let title = format!("{}:", ov.title);
    let g = ui.painter().layout_no_wrap(title, f.clone(), accent);
    let tw = g.size().x;
    ui.painter()
        .galley(egui::pos2(rect.left() + 10.0, rect.center().y - g.size().y / 2.0), g, accent);

    let field = Rect::from_min_max(
        egui::pos2(rect.left() + tw + 18.0, rect.top() + 5.0),
        egui::pos2(rect.right() - 10.0 - gutter, rect.bottom() - 5.0),
    );
    let id = egui::Id::new("filer-input");
    let before = ov.text.clone();
    let selected = selection_at_press(ui, id);
    let resp = ui.put(
        field,
        egui::TextEdit::singleline(&mut ov.text)
            .id(id)
            .font(egui::FontSelection::FontId(f.clone()))
            .frame(egui::Frame::NONE)
            .vertical_align(egui::Align::Center)
            .desired_width(field.width())
            .text_color(theme_fg),
    );
    if !ov.focused {
        resp.request_focus();
        // Where the prompt asked the caret to be, or what to select. Until
        // v0.55.0 this was worked out and never handed to the field, so every
        // prompt opened with the caret wherever egui left it.
        if let Some((from, to)) = ov.initial_selection.take() {
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            let range = egui::text::CCursorRange::two(
                egui::text::CCursor::new(from),
                egui::text::CCursor::new(to),
            );
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), id);
        }
        ov.focused = true;
    }
    let clip = right_click_paste(ui, &resp, id, &mut ov.text, selected);
    if (resp.changed() || matches!(clip, Ok(true))) && ov.text != before {
        app.input_changed();
    }
    if waiting {
        let g = ui.painter().layout_no_wrap("…".into(), f.clone(), theme_border);
        ui.painter().galley(
            egui::pos2(rect.right() - 10.0 - g.size().x, rect.center().y - g.size().y / 2.0),
            g,
            theme_border,
        );
    }
    // Said out loud because the right-click looked like it did nothing.
    if let Err(e) = clip {
        app.error(format!("Could not read the clipboard: {e}"));
    }
    let _ = queued;
}

/// The hovered file, big, over the panes — macOS's Quick Look.
///
/// A panel rather than an overlay on purpose: `Act::Quick` takes no keys, so
/// `j` and `k` keep walking the list and this follows them down it. `<A-j>` /
/// `<A-k>` scroll it, since it shares `preview_offset` with the side column.
pub fn quick(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32, queued: &mut Vec<Act>) {
    dim(ui, full);
    let theme = app.cfg.theme.clone();
    let name = match app.tabs[app.active].current.hovered() {
        Some(e) => e.name.clone(),
        None => "(nothing here)".into(),
    };
    let rect = modal_rect(full, 0.86, 0.88);
    let inner = modal_frame(ui, rect, &theme, &name, f, row_h);
    // Nothing else on screen changed, so say how to get out.
    ui.painter().text(
        rect.right_top() + Vec2::new(-14.0, 10.0),
        Align2::RIGHT_TOP,
        "Esc to close",
        f.clone(),
        theme.fg_dim,
    );
    super::draw_preview(app, ui, inner, f, row_h, queued);
}

/// What the shell prompt does with the selection, above the shell prompt.
///
/// `;` and `:` look like a bare command line, and read as a poor one: nothing
/// on screen says that the selected paths are handed to the command, which is
/// the entire point of having them. The count is there for the same reason —
/// "3 files" answers "what is this about to run on" before it runs.
pub fn shell_hint(app: &App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32, bottom: f32) {
    let theme = &app.cfg.theme;
    let Overlay::Input(ov) = &app.overlay else { return };
    let InputKind::Shell { block } = &ov.kind else { return };

    let n = app.tab().targets().len();
    let what = match n {
        0 => "nothing selected".to_owned(),
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    // `;` and `:` differ by nothing visible once the prompt is open, so say
    // which one this is. The difference is the console, not the waiting:
    // `--block` only asks Windows for `CREATE_NEW_CONSOLE` (see
    // `exec::configure`), and neither key makes filer wait for the command.
    let console = if *block { "new console" } else { "no console" };
    let text = format!("$@ all · $0 first · $1 second · no placeholder → appended    ({what}, {console})");

    let rect = Rect::from_min_max(
        egui::pos2(full.left() + 20.0, bottom - row_h - 16.0),
        egui::pos2(full.left() + 20.0 + (full.width() - 40.0).min(900.0), bottom - 6.0),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), theme.bg_alt);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        f.clone(),
        theme.fg_dim,
    );
}

/// The live preview under the bulk-rename prompt: what every selected file is
/// about to be called, and what is wrong with any of it. Redrawn on every
/// keystroke, which is why [`App::bulk_preview`] reads the directory out of the
/// listing in memory rather than off the disk.
/// What the prompt accepts, kept where it is being typed.
///
/// The rules are not guessable — `{n:3}` in particular — and the prompt is the
/// one moment anyone needs them. `rename::LEGEND_EXAMPLES` holds the same forms
/// for a test to parse, so this line cannot quietly outlive the syntax it
/// describes.
const LEGEND: &str = "{name} {ext} {n} {n:3} zero-padded  ·  s/pattern/replacement/gi";

pub fn bulk(app: &App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32, bottom: f32) {
    const MAX_ROWS: usize = 14;
    let theme = &app.cfg.theme;
    let Overlay::Input(ov) = &app.overlay else { return };
    let InputKind::Bulk { paths } = &ov.kind else { return };

    let (rows, trouble) = match app.bulk_preview(paths, &ov.text) {
        Ok(rows) => {
            let bad = rows.iter().filter(|r| r.problem.is_some()).count();
            (rows, (bad > 0).then(|| format!("{bad} name(s) cannot be used — Enter is refused")))
        }
        // A rule that does not parse yet is the normal state halfway through
        // typing one, so it reads as a note rather than an error.
        Err(e) => (Vec::new(), Some(e)),
    };

    let shown = rows.len().min(MAX_ROWS);
    // +1 for the legend, which is always there: it is a reference, and hiding
    // it once someone starts typing takes it away exactly when it is wanted.
    let lines =
        1 + shown + usize::from(rows.len() > shown) + usize::from(trouble.is_some());
    let h = row_h * lines as f32 + 20.0;
    // A long selection would push the top of the panel off a short window.
    let top = (bottom - h - 6.0).max(full.top() + 4.0);
    let rect = Rect::from_min_max(
        egui::pos2(full.left() + 20.0, top),
        egui::pos2(full.left() + 20.0 + (full.width() - 40.0).min(900.0), bottom - 6.0),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), theme.bg_alt);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );

    // The arrow column is set by the longest name on show, so the new names
    // line up and a stray change is easy to spot.
    let widest = rows
        .iter()
        .take(shown)
        .map(|r| crate::util::file_name(&r.from).chars().count())
        .max()
        .unwrap_or(0);
    let mut y = rect.top() + 10.0;
    painter.text(
        egui::pos2(rect.left() + 12.0, y),
        Align2::LEFT_TOP,
        LEGEND,
        f.clone(),
        theme.fg_dim,
    );
    y += row_h;
    for r in rows.iter().take(shown) {
        let from = crate::util::file_name(&r.from);
        let pad = " ".repeat(widest.saturating_sub(from.chars().count()));
        let (tail, color) = match &r.problem {
            Some(why) => (format!("{}   ({why})", r.to), theme.progress_error),
            None if r.to == from => (r.to.clone(), theme.fg_dim),
            None => (r.to.clone(), theme.cwd.fg.unwrap_or(theme.fg)),
        };
        painter.text(
            egui::pos2(rect.left() + 12.0, y),
            Align2::LEFT_TOP,
            format!("{from}{pad}  →  {tail}"),
            f.clone(),
            color,
        );
        y += row_h;
    }
    if rows.len() > shown {
        painter.text(
            egui::pos2(rect.left() + 12.0, y),
            Align2::LEFT_TOP,
            format!("… {} more", rows.len() - shown),
            f.clone(),
            theme.fg_dim,
        );
        y += row_h;
    }
    if let Some(note) = trouble {
        painter.text(
            egui::pos2(rect.left() + 12.0, y),
            Align2::LEFT_TOP,
            note,
            f.clone(),
            theme.progress_error,
        );
    }
}

pub fn bookmark_hint(app: &App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    let theme = &app.cfg.theme;
    let op = match app.pending_bookmark {
        Some(crate::app::BookmarkOp::Save) => "Save bookmark as…",
        Some(crate::app::BookmarkOp::Jump) => "Jump to bookmark…",
        Some(crate::app::BookmarkOp::Delete) => "Delete bookmark…",
        None => return,
    };
    let lines: Vec<String> = app
        .bookmarks
        .iter()
        .map(|b| format!("  {}   {}", b.key, b.path.display()))
        .collect();
    let h = row_h * (lines.len() as f32 + 1.5) + 20.0;
    let rect = Rect::from_min_size(
        egui::pos2(full.left() + 20.0, full.bottom() - h - 60.0),
        Vec2::new((full.width() * 0.5).min(560.0), h),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), theme.bg_alt);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.left_top() + Vec2::new(12.0, 8.0),
        Align2::LEFT_TOP,
        op,
        f.clone(),
        theme.cwd.fg.unwrap_or(theme.fg),
    );
    for (i, l) in lines.iter().enumerate() {
        painter.text(
            rect.left_top() + Vec2::new(12.0, 8.0 + row_h * (i as f32 + 1.2)),
            Align2::LEFT_TOP,
            l,
            f.clone(),
            theme.fg_dim,
        );
    }
}

/// A row of the help panel.
struct HelpRow {
    keys: String,
    text: String,
    /// The command a key runs, shown on the right.
    raw: String,
    warning: bool,
    /// Where clicking this row goes. A config file is revealed in the list
    /// (its directory, with it under the cursor); a directory is opened.
    goes_to: Option<Act>,
}

impl HelpRow {
    fn blank() -> Self {
        Self { keys: String::new(), text: String::new(), raw: String::new(), warning: false, goes_to: None }
    }
    fn heading(k: &str) -> Self {
        Self { keys: k.into(), ..Self::blank() }
    }
    fn said(text: String) -> Self {
        Self { text, ..Self::blank() }
    }
}

/// What the panel says about configuration.
///
/// Every directory that is searched, not only the ones something was found in:
/// "where does `filer.toml` go" is the question a panel listing loaded files
/// cannot answer, because the answer is a file that does not exist yet. An
/// empty directory is the most useful row on the list for the reader who needs
/// it, and the only one that was missing.
/// How the keymap in force spells the key that runs `act`, if one does.
///
/// Looked up rather than written down: the panel whose whole job is to say what
/// the keys are is the last place a key should be hardcoded, since a reader who
/// has rebound one is exactly the reader being told the wrong thing.
fn key_for(app: &App, act: &Act) -> Option<String> {
    let one = std::slice::from_ref(act);
    let found = app.cfg.keymap.mgr.iter().find(|b| b.run.as_slice() == one)?;
    Some(crate::config::keys::render_seq(&found.on))
}

fn config_rows(app: &App, dirs: &[std::path::PathBuf]) -> Vec<HelpRow> {
    let mut out = vec![HelpRow::heading("config")];
    for dir in dirs {
        let here: Vec<&std::path::PathBuf> =
            app.cfg.loaded.iter().filter(|p| p.parent() == Some(dir.as_path())).collect();
        // A file that is on disk now but was not among the ones read. Writing a
        // config with the window already open is the ordinary way to get here,
        // and the panel used to answer it with "nothing here" while the file sat
        // in that very directory -- which reads as "filer cannot see it" rather
        // than "filer has not looked since".
        let unread: Vec<std::path::PathBuf> = crate::config::FILES
            .iter()
            .map(|n| dir.join(n))
            .filter(|p| p.is_file() && !here.contains(&p))
            .collect();
        out.push(HelpRow {
            text: format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR),
            raw: match here.is_empty() && unread.is_empty() {
                true => "nothing here".into(),
                false => String::new(),
            },
            goes_to: Some(Act::Cd { target: dir.display().to_string(), interactive: false }),
            ..HelpRow::blank()
        });
        for p in here {
            // Read, and none of it took effect: marked like a file not read
            // yet, with the warning below saying why (#203).
            let unused = app.cfg.unread.contains(p);
            out.push(HelpRow {
                text: format!("    {}", crate::util::file_name(p)),
                raw: match unused {
                    true => "nothing in it was read — see below".into(),
                    false => String::new(),
                },
                warning: unused,
                goes_to: Some(Act::Reveal(p.display().to_string())),
                ..HelpRow::blank()
            });
        }
        let reread = match key_for(app, &Act::ConfigReload) {
            Some(k) => format!("on disk, not read yet — {k} re-reads config"),
            None => "on disk, not read yet — reload config to pick it up".into(),
        };
        for p in unread {
            out.push(HelpRow {
                text: format!("    {}", crate::util::file_name(&p)),
                raw: reread.clone(),
                warning: true,
                goes_to: Some(Act::Reveal(p.display().to_string())),
                ..HelpRow::blank()
            });
        }
    }
    // A file from somewhere else entirely: `FILER_CONFIG_HOME` moved after it
    // was read, or a path no longer under any searched directory.
    for p in app.cfg.loaded.iter().filter(|p| !p.parent().is_some_and(|d| dirs.iter().any(|x| x == d))) {
        out.push(HelpRow {
            text: p.display().to_string(),
            goes_to: Some(Act::Reveal(p.display().to_string())),
            ..HelpRow::blank()
        });
    }
    for w in &app.cfg.warnings {
        out.push(HelpRow { text: w.clone(), warning: true, ..HelpRow::blank() });
    }
    out
}

/// The directories the help panel names, and looks into for files not read yet.
#[cfg(not(test))]
fn shown_config_dirs() -> Vec<std::path::PathBuf> {
    crate::config::config_dirs()
}

/// Under test, a folder that is not there for each: the real ones hold whatever
/// the person running the suite keeps in them. On a machine with a yazi and a
/// filer config, their seven files were listed as "not read yet" and pushed
/// the key list's heading out of the frame, so a help test failed there and
/// nowhere else -- CI has no config (#136). `config_rows`' own tests pass their
/// directories in and still look on disk.
#[cfg(test)]
pub(crate) fn shown_config_dirs() -> Vec<std::path::PathBuf> {
    crate::config::CONFIG_VARS.iter().map(|v| std::env::temp_dir().join("filer-test-no-config").join(v)).collect()
}

/// The line under the config rows when nothing was read. Nothing read is not
/// the same as nothing there: a file written since filer started is on disk,
/// listed just above, and "nothing found" under it said the opposite (#203).
fn defaults_note(app: &App, rows: &[HelpRow]) -> Option<HelpRow> {
    if !app.cfg.loaded.is_empty() {
        return None;
    }
    let waiting = rows.iter().any(|r| r.raw.starts_with("on disk, not read yet"));
    Some(HelpRow::said(match waiting {
        true => "(nothing read yet; the defaults are in use)".into(),
        false => "(nothing found in either; the defaults are in use)".into(),
    }))
}

/// What the help panel lists, top to bottom: drawn by [`help`], and copied as
/// text by [`help_text`] so the two cannot disagree.
fn help_lines(app: &App) -> Vec<HelpRow> {
    // Config provenance first — it answers "did it pick up my yazi config?"
    // before the key list answers "what is bound to what".
    let mut lines = config_rows(app, &shown_config_dirs());
    lines.extend(defaults_note(app, &lines));
    lines.push(HelpRow::blank());
    // What the mouse does that no key does, so it is in no key list: the
    // right-click paste was only in the README (#104). Above the keys, which
    // end the panel.
    lines.push(HelpRow::heading("the mouse"));
    lines.push(HelpRow::said(
        "right-click in a prompt or the terminal pane pastes the clipboard, over the selection if there is one".into(),
    ));
    lines.push(HelpRow::blank());
    let row = |b: &crate::config::keymap::Binding| HelpRow {
        keys: crate::config::keys::render_seq(&b.on),
        text: if b.desc.is_empty() { b.raw.clone() } else { b.desc.clone() },
        raw: b.raw.clone(),
        warning: false,
        goes_to: None,
    };
    // Opened from the pane, the keys that work there come first. The list's
    // alone gave the wrong answer in the one place it was asked: `<A-k>` read
    // "Scroll the preview up", which in the pane scrolls the terminal (39.9).
    if app.term_focus {
        lines.push(HelpRow::heading("keys in the terminal pane"));
        lines.extend(app.cfg.keymap.term.iter().map(row));
        lines.push(HelpRow::blank());
        lines.push(HelpRow::heading("keys in the list (<C-t> to get there)"));
    } else {
        lines.push(HelpRow::heading("keys"));
    }
    lines.extend(app.cfg.keymap.mgr.iter().map(row));
    lines
}

/// The help panel as text, for `C` (Q48): a heading on a line of its own, then
/// one `keys<TAB>description<TAB>command` line per binding, the way spot's
/// `copy all` lays out its rows. "Is my new key listed" is a question about
/// text, and before this it could only be answered from a screenshot (#171).
/// Also the number of key lines, for the toast.
pub(crate) fn help_text(app: &App) -> (String, usize) {
    let mut keys = 0;
    let text: Vec<String> = help_lines(app)
        .iter()
        .map(|r| match (r.keys.is_empty(), r.text.is_empty()) {
            (false, true) => r.keys.clone(),
            (false, false) => {
                keys += 1;
                format!("{}\t{}\t{}", r.keys, r.text, r.raw)
            }
            _ if r.raw.is_empty() => r.text.clone(),
            _ => format!("{}\t{}", r.text, r.raw),
        })
        .collect();
    (text.join("\n") + "\n", keys)
}

pub fn help(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32, queued: &mut Vec<Act>) {
    dim(ui, full);
    let rect = modal_rect(full, 0.86, 0.86);
    let title =
        "Keys — <Esc> close, j/k scroll, <A-j>/<A-k> half a page, C copies it all, click a config path to go to it";
    let inner = modal_frame(ui, rect, &app.cfg.theme, title, f, row_h);
    let theme = app.cfg.theme.clone();
    let lines = help_lines(app);

    let rows = ((inner.height() / row_h).floor() as usize).max(1);
    // What the keys need to know to page and to stop; only the renderer knows
    // how tall the panel came out.
    app.help_rows = rows;
    app.help_lines = lines.len();
    // The last line at the bottom, not at the top: a panel showing one row of a
    // list it has room for twenty of is not the end of a scroll.
    let stop = lines.len().saturating_sub(rows);
    // The wheel, over the panel rather than over the list it covers.
    if ui.rect_contains_pointer(rect) {
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        let moved = crate::ui::wheel_whole(&mut app.help_scroll_rows, -scroll / row_h * 1.5);
        if moved != 0 {
            app.help_scroll = (app.help_scroll as i64 + moved).clamp(0, stop as i64) as usize;
        }
    }
    // A smaller font fits more lines, so a scroll position that was at the
    // bottom before a `<C-+>` has to come back to it.
    app.help_scroll = app.help_scroll.min(stop);
    let start = app.help_scroll;
    let pointer = ui.rect_contains_pointer(inner).then(|| ui.ctx().pointer_latest_pos()).flatten();
    let clicked = ui.input(|i| i.pointer.primary_clicked());
    let mut went = None;

    let painter = ui.painter_at(inner);
    for (i, row) in lines[start..].iter().take(rows).enumerate() {
        let y = inner.top() + i as f32 * row_h;
        let at = Rect::from_min_size(
            egui::pos2(inner.left(), y),
            Vec2::new(inner.width(), row_h),
        );
        // Only the config paths answer to the pointer; a key list is a key
        // list and a row that lit up under the cursor would only mislead.
        let live = row.goes_to.is_some() && pointer.is_some_and(|p| at.contains(p));
        if live {
            painter.rect_filled(at, CornerRadius::same(3), theme.hovered_bg);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if clicked {
                went = row.goes_to.clone();
            }
        }
        painter.text(
            egui::pos2(inner.left(), y),
            Align2::LEFT_TOP,
            &row.keys,
            f.clone(),
            theme.which_cand.fg.unwrap_or(theme.fg),
        );
        let color = match (row.warning, row.goes_to.is_some()) {
            (true, _) => theme.warning,
            // A path reads as somewhere to go, in the colour the breadcrumb
            // already uses for a directory.
            (false, true) => theme.cwd.fg.unwrap_or(theme.fg),
            (false, false) => theme.fg,
        };
        painter.text(
            egui::pos2(inner.left() + 130.0, y),
            Align2::LEFT_TOP,
            &row.text,
            f.clone(),
            color,
        );
        if !row.raw.is_empty() {
            painter.text(
                egui::pos2(inner.right(), y),
                Align2::RIGHT_TOP,
                &row.raw,
                f.clone(),
                theme.fg_dim,
            );
        }
    }

    // Going somewhere means looking at it, so the panel gets out of the way.
    if let Some(act) = went {
        app.overlay = Overlay::None;
        queued.push(act);
    }
}

pub fn tasks(app: &App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    dim(ui, full);
    let rect = modal_rect(full, 0.7, 0.6);
    let title = "Tasks — j/k move · p pause · x cancel · t to the front · <Esc> to close";
    let inner = modal_frame(ui, rect, &app.cfg.theme, title, f, row_h);
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(inner);

    if app.tasks.is_empty() {
        // Saying what puts something here, because an empty panel called a
        // task manager reads as one you are meant to add to. Nothing is added
        // by hand: the operations queue themselves, and this is where they are
        // watched.
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            "No tasks — copying, moving, deleting, extracting and compressing queue here",
            f.clone(),
            theme.fg_dim,
        );
        return;
    }
    let cursor = match &app.overlay {
        Overlay::Tasks(ov) => ov.cursor,
        _ => usize::MAX,
    };
    let mut y = inner.top();
    for (i, t) in app.tasks.iter().enumerate() {
        let color = match t.state {
            TaskState::Failed => theme.progress_error,
            TaskState::Done => theme.marker_copied,
            TaskState::Paused | TaskState::Queued => theme.fg_dim,
            _ => theme.fg,
        };
        if i == cursor {
            // The row the keys act on, marked the way the list marks its own.
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(inner.left() - 6.0, y - 2.0),
                    Vec2::new(inner.width() + 12.0, row_h + 4.0),
                ),
                CornerRadius::same(3),
                theme.hovered_bg,
            );
        }
        painter.text(
            egui::pos2(inner.left(), y),
            Align2::LEFT_TOP,
            t.headline(),
            f.clone(),
            color,
        );
        y += row_h;
        let bar = Rect::from_min_size(egui::pos2(inner.left(), y + 2.0), Vec2::new(inner.width(), 4.0));
        painter.rect_filled(bar, CornerRadius::same(2), theme.border);
        painter.rect_filled(
            Rect::from_min_size(bar.min, Vec2::new(bar.width() * t.fraction(), 4.0)),
            CornerRadius::same(2),
            theme.progress_fg,
        );
        y += 10.0;
        painter.text(egui::pos2(inner.left(), y), Align2::LEFT_TOP, t.detail(), f.clone(), theme.fg_dim);
        y += row_h;
        for e in t.errors.iter().take(3) {
            painter.text(
                egui::pos2(inner.left() + 12.0, y),
                Align2::LEFT_TOP,
                e,
                f.clone(),
                theme.progress_error,
            );
            y += row_h;
        }
        y += 6.0;
        if y > inner.bottom() {
            break;
        }
    }
}

pub fn confirm(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32, queued: &mut Vec<Act>) {
    dim(ui, full);
    let Overlay::Confirm(c) = &app.overlay else { return };

    // Measure the buttons before the frame is sized. Six of them do not fit on
    // one row at this width, and the row used to be laid out without ever
    // comparing against the right edge: the last one — Cancel, the only way out
    // — was sliced in half by the modal's clip and read as `[q] Ca`.
    let labels: Vec<String> =
        c.options.iter().map(|(k, l)| format!(" [{k}] {l} ")).collect();
    let fg = app.cfg.theme.fg;
    let widths: Vec<f32> = labels
        .iter()
        .map(|t| ui.painter().layout_no_wrap(t.clone(), f.clone(), fg).size().x + 6.0)
        .collect();

    let width = (full.width() * 0.6).min(760.0);
    // 14px of padding on each side, from `modal_frame`.
    let inner_w = (width - 28.0).max(1.0);
    let mut button_rows = 1.0f32;
    let mut used = 0.0f32;
    for w in &widths {
        if used > 0.0 && used + w > inner_w {
            button_rows += 1.0;
            used = 0.0;
        }
        used += w + 8.0;
    }

    // The body wraps to the box rather than losing its middle. It used to be
    // elided at a character count, and the junction question's second line
    // came out `holds the…ull path` on every machine while its path pair lost
    // the arrow and the link's own name -- the one thing that question is
    // there to show (#185).
    let body: Vec<_> =
        c.body.iter().map(|l| ui.painter().layout(l.clone(), f.clone(), fg, inner_w)).collect();
    let body_h: f32 = body.iter().map(|g| g.size().y.max(row_h)).sum();
    let height = body_h + row_h * (3.0 + button_rows) + 48.0;
    let rect = Rect::from_center_size(full.center(), Vec2::new(width, height));
    let inner = modal_frame(ui, rect, &app.cfg.theme, &c.title, f, row_h);
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(inner);

    let mut y = inner.top();
    for g in body {
        let h = g.size().y.max(row_h);
        painter.galley(egui::pos2(inner.left(), y), g, theme.fg);
        y += h;
    }
    y += row_h * 0.5;

    let mut x = inner.left();
    let mut hit: Option<char> = None;
    for (i, (key, _)) in c.options.iter().enumerate() {
        let w = widths[i];
        // Wrap rather than run off the edge. A button drawn past `inner` is
        // clipped away but still answers the mouse, so an invisible target is
        // worse than a wrapped one.
        if x > inner.left() && x + w > inner.right() {
            x = inner.left();
            y += row_h + 8.0;
        }
        let g = painter.layout_no_wrap(labels[i].clone(), f.clone(), theme.fg);
        let r = Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, row_h + 4.0));
        painter.rect_filled(r, CornerRadius::same(4), theme.status_bg);
        painter.galley(egui::pos2(x + 3.0, y + 2.0), g, theme.fg);
        if ui
            .interact(r, ui.id().with(("confirm", *key)), egui::Sense::click())
            .clicked()
        {
            hit = Some(*key);
        }
        x += w + 8.0;
    }
    if let Some(ch) = hit {
        app.answer_confirm(ch);
    }
    let _ = queued;
}

pub fn pick(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32, queued: &mut Vec<Act>) {
    dim(ui, full);
    let rect = modal_rect(full, 0.66, 0.6);
    let title = match &app.overlay {
        Overlay::Pick(p) => p.title.clone(),
        _ => return,
    };
    let inner = modal_frame(ui, rect, &app.cfg.theme, &title, f, row_h);
    let theme_fg = app.cfg.theme.fg;
    let theme_dim = app.cfg.theme.fg_dim;
    let hovered_bg = app.cfg.theme.hovered_bg;
    let accent = app.cfg.theme.which_cand.fg.unwrap_or(theme_fg);
    let hl_fg = app.cfg.theme.find_keyword.fg.unwrap_or(theme_fg);
    let hl_bg = app.cfg.theme.find_keyword.bg.unwrap_or(egui::Color32::TRANSPARENT);

    let field = Rect::from_min_size(inner.min, Vec2::new(inner.width(), row_h + 4.0));
    let Overlay::Pick(p) = &mut app.overlay else { return };

    let before = p.query.clone();
    let pick_id = egui::Id::new("filer-pick");
    let selected = selection_at_press(ui, pick_id);
    let resp = ui.put(
        field,
        egui::TextEdit::singleline(&mut p.query)
            .id(pick_id)
            .font(egui::FontSelection::FontId(f.clone()))
            .frame(egui::Frame::NONE)
            .hint_text("type to filter")
            .desired_width(field.width())
            .text_color(theme_fg),
    );
    if !p.focused {
        resp.request_focus();
        p.focused = true;
    }
    // The clipboard is ignored here rather than reported: a chooser has no room
    // for a toast under it, and the query is typed far more often than pasted.
    let _ = right_click_paste(ui, &resp, pick_id, &mut p.query, selected);
    if p.query != before {
        p.refilter();
    }

    let list = Rect::from_min_max(
        egui::pos2(inner.left(), field.bottom() + 6.0),
        inner.right_bottom(),
    );
    let painter = ui.painter_at(list);
    let rows = ((list.height() / row_h).floor() as usize).max(1);
    let start = p.cursor.saturating_sub(rows / 2).min(p.matches.len().saturating_sub(rows));

    let mut clicked: Option<usize> = None;
    for (i, (idx, _score, positions)) in p.matches[start..].iter().take(rows).enumerate() {
        let y = list.top() + i as f32 * row_h;
        let row_rect = Rect::from_min_size(egui::pos2(list.left(), y), Vec2::new(list.width(), row_h));
        if start + i == p.cursor {
            painter.rect_filled(row_rect, CornerRadius::same(3), hovered_bg);
        }
        let label = &p.items[*idx];
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = list.width() - 120.0;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        for (ci, ch) in label.chars().enumerate() {
            let hl = positions.contains(&ci);
            job.append(
                &ch.to_string(),
                0.0,
                egui::TextFormat {
                    font_id: f.clone(),
                    color: if hl { hl_fg } else { theme_fg },
                    background: if hl { hl_bg } else { egui::Color32::TRANSPARENT },
                    ..Default::default()
                },
            );
        }
        let g = painter.layout_job(job);
        painter.galley(egui::pos2(row_rect.left() + 6.0, y + 2.0), g, theme_fg);
        let detail = &p.details[*idx];
        if !detail.is_empty() {
            painter.text(
                egui::pos2(row_rect.right() - 6.0, y + 2.0),
                Align2::RIGHT_TOP,
                crate::util::ellipsize_middle(detail, 28),
                f.clone(),
                theme_dim,
            );
        }
        if ui
            .interact(row_rect, ui.id().with(("pick", i)), egui::Sense::click())
            .clicked()
        {
            clicked = Some(start + i);
        }
    }
    let _ = accent;

    if let Some(c) = clicked {
        if let Overlay::Pick(p) = &mut app.overlay {
            p.cursor = c;
        }
        app.submit_pick();
    }
    let _ = queued;
}

/// The spot panel: one block of `key  value` rows per section, the selected
/// row highlighted and kept in view.
/// Two files side by side, the lines that differ marked. The gutter carries
/// each side's own line number, so a line can be found in either file without
/// counting rows.
/// A compare tree row's path, a folder marked by the platform's own
/// separator: `thing\inner.txt` above `thing/` mixed the two on Windows.
fn tree_name(row: &crate::diff::TreeRow) -> String {
    let mut name = row.rel.display().to_string();
    if row.dir {
        name.push(std::path::MAIN_SEPARATOR);
    }
    name
}

pub fn diff(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    use crate::diff::Outcome;

    dim(ui, full);
    let theme = app.cfg.theme.clone();
    let Overlay::Diff(ov) = &mut app.overlay else { return };
    // `z` is offered only where it does something: a tree has rows to hide.
    // Each key is named with what it does: `n/N differences` read as a count
    // of differences that had not been filled in (#98).
    // A pair opened from a folder comparison goes back to it, and says so.
    let keys = if matches!(ov.outcome, Some(Outcome::Tree { .. })) {
        "n / N: next / previous difference · z: hide matches · Enter: compare files · q: close"
    } else if ov.back.is_some() {
        "n / N: next / previous difference · q: back to the folders"
    } else {
        "n / N: next / previous difference · q: close"
    };
    let title = format!(
        "{}  ↔  {} — {keys}",
        crate::util::file_name(&ov.left),
        crate::util::file_name(&ov.right),
    );
    let rect = modal_rect(full, 0.92, 0.86);
    let framed = modal_frame(ui, rect, &theme, &title, f, row_h);
    // Both full paths, under the names: comparing two folders of the same name
    // is the usual case, and the title alone could not say which was which.
    let cell = ui.painter().layout_no_wrap("M".into(), f.clone(), theme.fg).size().x.max(1.0);
    let room = ((framed.width() / cell) as usize).max(8);
    // Each side cut on its own, so a long left path cannot push the right
    // one off the line: the file names at the ends are what tell them apart.
    let half = room.saturating_sub(5) / 2;
    let side = |p: &std::path::Path| crate::util::ellipsize_middle(&p.display().to_string(), half.max(4));
    let paths = format!("{}  ↔  {}", side(&ov.left), side(&ov.right));
    ui.painter_at(framed).text(
        framed.left_top(),
        Align2::LEFT_TOP,
        paths,
        f.clone(),
        theme.fg_dim,
    );
    let inner = Rect::from_min_max(framed.min + Vec2::new(0.0, row_h), framed.max);
    let painter = ui.painter_at(inner);

    let note = |text: &str, color| {
        painter.text(inner.left_top(), Align2::LEFT_TOP, text, f.clone(), color);
    };
    let (rows, truncated, rough) = match &ov.outcome {
        None => return note("Comparing…", theme.fg_dim),
        Some(Outcome::Error(e)) => return note(e, theme.progress_error),
        Some(Outcome::Identical) => return note("The two files are identical.", theme.fg_dim),
        Some(Outcome::Binary { .. }) => {
            return note("Not text on both sides, and the bytes differ.", theme.fg_dim)
        }
        Some(Outcome::Tree { .. }) => {
            return diff_tree(ov, &theme, &painter, inner, f, row_h);
        }
        Some(Outcome::Rows { rows, truncated, rough }) => (rows, *truncated, *rough),
    };

    // One row is given up to the footer, but never the last one.
    let visible = ((inner.height() / row_h).floor() as usize).max(2) - 1;
    // Told to the keys, which otherwise cannot know where scrolling stops.
    ov.rows = visible;
    ov.offset = ov.offset.min(rows.len().saturating_sub(visible.min(rows.len())));
    let top = ov.offset;

    // Two equal halves with a hairline between them.
    let mid = inner.center().x;
    painter.line_segment(
        [egui::pos2(mid, inner.top()), egui::pos2(mid, inner.bottom())],
        Stroke::new(1.0, theme.border),
    );
    let cell = painter.layout_no_wrap("M".repeat(20), f.clone(), theme.fg).size().x / 20.0;
    let half = (mid - inner.left() - 12.0).max(0.0);
    let cols = ((half - 5.0 * cell) / cell).max(4.0) as usize;

    // Removed on the left, added on the right: the same two colors the git
    // signs use, so a changed line reads the same way it does in the listing.
    let mut y = inner.top();
    for row in rows.iter().skip(top).take(visible) {
        let sides = [
            (&row.left, inner.left(), theme.git_deleted),
            (&row.right, mid + 8.0, theme.git_added),
        ];
        for (side, x, mark) in sides {
            // Nothing on this side: the line exists only in the other file.
            let Some(l) = side else { continue };
            let number = format!("{:>4} ", l.no);
            if !row.same {
                painter.rect_filled(
                    Rect::from_min_size(egui::pos2(x - 2.0, y), Vec2::new(half, row_h)),
                    CornerRadius::same(2),
                    mark.gamma_multiply(0.22),
                );
                // The words that changed, stronger. Measured with the font
                // rather than counted in cells, so a wide character before
                // them does not push the mark off its word. A line cut to fit
                // has lost the positions they refer to, so it keeps the tint.
                let chars: Vec<char> = l.text.chars().collect();
                if chars.len() <= cols {
                    let width = |t: String| painter.layout_no_wrap(t, f.clone(), theme.fg).size().x;
                    for r in &l.changed {
                        let before: String = number.chars().chain(chars[..r.start].iter().copied()).collect();
                        let x0 = x + width(before);
                        let w = width(chars[r.clone()].iter().collect());
                        painter.rect_filled(
                            Rect::from_min_size(egui::pos2(x0, y), Vec2::new(w, row_h)),
                            CornerRadius::same(2),
                            mark.gamma_multiply(0.6),
                        );
                    }
                }
            }
            painter.text(
                egui::pos2(x, y),
                Align2::LEFT_TOP,
                format!("{number}{}", crate::util::ellipsize_middle(&l.text, cols)),
                f.clone(),
                theme.fg,
            );
        }
        y += row_h;
    }

    let mut foot = format!("{}–{} of {}", top + 1, (top + visible).min(rows.len()), rows.len());
    if rough {
        foot.push_str("  ·  too large to line up exactly");
    }
    if truncated {
        foot.push_str("  ·  cut short");
    }
    painter.text(
        egui::pos2(inner.left(), inner.bottom() - row_h),
        Align2::LEFT_TOP,
        foot,
        f.clone(),
        theme.fg_dim,
    );
}

/// Two trees as one list of the paths inside them, each marked with what was
/// found. A sibling of [`diff`] rather than a branch of it: that view is two
/// columns of numbered lines, and this is a list with a cursor, so they agree on
/// the frame and nothing else.
fn diff_tree(
    ov: &mut crate::app::DiffOverlay,
    theme: &crate::config::theme::Theme,
    painter: &egui::Painter,
    inner: Rect,
    f: &FontId,
    row_h: f32,
) {
    use crate::diff::{Outcome, TreeState};
    let shown = ov.shown();
    let Some(Outcome::Tree { rows, counts, truncated }) = &ov.outcome else { return };
    let (counts, truncated) = (*counts, *truncated);
    if rows.is_empty() {
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            "The two folders hold the same paths, and every file matches.",
            f.clone(),
            theme.fg_dim,
        );
        return;
    }

    if shown.is_empty() {
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            "Every path matches; the matching rows are hidden (z shows them).",
            f.clone(),
            theme.fg_dim,
        );
        return;
    }

    let visible = ((inner.height() / row_h).floor() as usize).max(2) - 1;
    ov.rows = visible;
    ov.cursor = ov.cursor.min(shown.len() - 1);
    // Keep the cursor on screen, the way the spot panel does.
    if ov.cursor < ov.offset {
        ov.offset = ov.cursor;
    } else if ov.cursor >= ov.offset + visible {
        ov.offset = ov.cursor + 1 - visible;
    }
    ov.offset = ov.offset.min(shown.len().saturating_sub(visible.min(shown.len())));
    let top = ov.offset;

    let cell = painter.layout_no_wrap("M".repeat(20), f.clone(), theme.fg).size().x / 20.0;
    let mut y = inner.top();

    for (i, row) in shown.iter().map(|&r| &rows[r]).enumerate().skip(top).take(visible) {
        if i == ov.cursor {
            painter.rect_filled(
                Rect::from_min_size(inner.left_top() + Vec2::new(0.0, y - inner.top()), Vec2::new(inner.width(), row_h)),
                CornerRadius::same(3),
                theme.hovered_bg,
            );
        }
        // The sign says which side, so the list reads without a legend: `<` and
        // `>` point at the tree that has it, `~` is both-but-different.
        let (sign, color) = match row.state {
            TreeState::LeftOnly => ("<", theme.git_deleted),
            TreeState::RightOnly => (">", theme.git_added),
            TreeState::Differ => ("~", theme.git_modified),
            TreeState::Same => ("=", theme.fg_dim),
            TreeState::Unread => ("?", theme.fg_dim),
        };
        painter.text(inner.left_top() + Vec2::new(0.0, y - inner.top()), Align2::LEFT_TOP, sign, f.clone(), color);

        let size = match row.state {
            TreeState::Differ if !row.dir => format!(
                "  {} → {}",
                crate::util::human_size(row.left),
                crate::util::human_size(row.right)
            ),
            TreeState::LeftOnly if !row.dir => format!("  {}", crate::util::human_size(row.left)),
            TreeState::RightOnly if !row.dir => format!("  {}", crate::util::human_size(row.right)),
            _ => String::new(),
        };
        let name = tree_name(row);
        let cols = ((inner.width() / cell) as usize).saturating_sub(4 + size.chars().count());
        painter.text(
            inner.left_top() + Vec2::new(2.0 * cell, y - inner.top()),
            Align2::LEFT_TOP,
            format!("{}{size}", crate::util::ellipsize_middle(&name, cols.max(4))),
            f.clone(),
            if matches!(row.state, TreeState::Same) { theme.fg_dim } else { theme.fg },
        );
        y += row_h;
    }

    let mut foot = format!(
        "{} only left  ·  {} only right  ·  {} differ  ·  {} match",
        counts.left_only, counts.right_only, counts.differ, counts.same
    );
    if counts.unread > 0 {
        foot.push_str(&format!("  ·  {} too big to read", counts.unread));
    }
    if truncated {
        foot.push_str("  ·  cut short");
    }
    // Said, so that a list with no `=` in it is not read as "nothing matched".
    if ov.hide_same {
        foot.push_str("  ·  matches hidden (z)");
    }
    painter.text(
        egui::pos2(inner.left(), inner.bottom() - row_h),
        Align2::LEFT_TOP,
        foot,
        f.clone(),
        theme.fg_dim,
    );
}

pub fn spot(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    dim(ui, full);
    let sections = app.spot_sections();
    let name = app.tabs[app.active].current.hovered().map(|e| e.name.clone()).unwrap_or_default();
    let rect = modal_rect(full, 0.7, 0.7);
    let title = format!("Spot: {name} — <Esc> to close");
    let inner = modal_frame(ui, rect, &app.cfg.theme, &title, f, row_h);
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(inner);

    if sections.is_empty() {
        painter.text(inner.left_top(), Align2::LEFT_TOP, "Nothing to spot", f.clone(), theme.fg_dim);
        return;
    }

    // Flatten into lines: a header per section, then its rows. `Some(i)` marks
    // the i-th row overall, which is what the cursor counts.
    let mut lines: Vec<(Option<usize>, &str, &str)> = Vec::new();
    let mut n = 0;
    for (k, s) in sections.iter().enumerate() {
        if k > 0 {
            lines.push((None, "", ""));
        }
        lines.push((None, &s.title, ""));
        for (key, value) in &s.rows {
            lines.push((Some(n), key, value));
            n += 1;
        }
    }

    let Overlay::Spot(ov) = &mut app.overlay else { return };
    ov.cursor = ov.cursor.min(n.saturating_sub(1));
    let visible = ((inner.height() / row_h).floor() as usize).max(1);
    let at = lines.iter().position(|l| l.0 == Some(ov.cursor)).unwrap_or(0);
    // Keep the section header in view when the cursor is on its first row.
    let top = if at > 0 && lines[at - 1].0.is_none() { at - 1 } else { at };
    if top < ov.scroll {
        ov.scroll = top;
    } else if at >= ov.scroll + visible {
        ov.scroll = at + 1 - visible;
    }
    ov.scroll = ov.scroll.min(lines.len().saturating_sub(visible));

    let accent = theme.cwd.fg.unwrap_or(theme.fg);
    let key_w = 130.0;
    let value_chars = ((inner.width() - key_w) / (f.size * 0.6)).max(4.0) as usize;
    for (i, (row, key, value)) in lines.iter().skip(ov.scroll).take(visible).enumerate() {
        let y = inner.top() + i as f32 * row_h;
        match row {
            None => {
                painter.text(egui::pos2(inner.left(), y), Align2::LEFT_TOP, *key, f.clone(), accent);
            }
            Some(r) => {
                if *r == ov.cursor {
                    let band = Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(inner.width(), row_h));
                    painter.rect_filled(band, CornerRadius::same(3), theme.hovered_bg);
                }
                painter.text(
                    egui::pos2(inner.left() + 12.0, y),
                    Align2::LEFT_TOP,
                    *key,
                    f.clone(),
                    theme.fg_dim,
                );
                painter.text(
                    egui::pos2(inner.left() + key_w, y),
                    Align2::LEFT_TOP,
                    crate::util::ellipsize_middle(value, value_chars),
                    f.clone(),
                    theme.fg,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{one_line, paste_over, splice};

    /// A paste goes where the caret is, and a selection is replaced rather than
    /// pushed aside — the same two cases every text field has.
    #[test]
    fn pastes_at_the_caret() {
        assert_eq!(splice("cd ", 3..3, "D:/work"), "cd D:/work");
        assert_eq!(splice("cd old", paste_over(4..4, Some(3..6), 6), "new"), "cd new", "a click on the selection");
        assert_eq!(splice("cd old", paste_over(1..1, Some(3..6), 6), "X"), "cXd old", "a click beside it");
        assert_eq!(paste_over(2..2, Some(2..2), 6), 2..2, "a caret is not a selection");
        assert_eq!(paste_over(0..0, None, 0), 0..0);
        assert_eq!(one_line("line one\r\nline two"), "line one line two", "CRLF is one space");
        assert_eq!(one_line("a\nb\rc"), "a b c");
        assert_eq!(splice("cd old", 3..6, "new"), "cd new");
        assert_eq!(splice("", 0..0, "x"), "x");
        // A caret past the end (a stored one from longer text) clamps rather
        // than panics; the caller clamps, so this only has to not misplace it.
        assert_eq!(splice("ab", 2..2, "c"), "abc");
    }

    /// The reason `splice` counts characters: egui's caret is a character index,
    /// and a prompt holding a Japanese path has more bytes than characters. A
    /// byte offset used here would land inside a character and panic.
    #[test]
    fn a_multibyte_prompt_is_not_cut_in_half() {
        assert_eq!(splice("報告書", 1..1, "X"), "報X告書");
        assert_eq!(splice("cd 報告書", 3..6, "資料"), "cd 資料");
        assert_eq!(splice("画像", 2..2, "です"), "画像です");
    }
}

#[cfg(test)]
mod help_config_rows {
    use super::*;

    /// Every searched directory is listed, found in or not.
    ///
    /// "Where does `filer.toml` go" is the one question a list of loaded files
    /// cannot answer, because the answer is a file that does not exist yet.
    /// The empty directory is the row that answers it, and it was the row that
    /// was missing — the panel used to show only what it had read.
    #[test]
    fn an_empty_directory_is_still_a_row() {
        let yazi = std::path::PathBuf::from("/tmp/filer-help/yazi");
        let mine = std::path::PathBuf::from("/tmp/filer-help/filer");

        let ctx = egui::Context::default();
        let mut app = App::new(crate::config::Config::load(), std::env::temp_dir(), ctx);
        app.cfg.loaded = vec![yazi.join("keymap.toml"), yazi.join("theme.toml")];
        app.cfg.warnings = vec!["[mgr] `\'` is bound twice".into()];

        let rows = config_rows(&app, &[yazi.clone(), mine.clone()]);
        let text: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();

        assert!(text.iter().any(|t| t.starts_with(&yazi.display().to_string())), "{text:?}");
        assert!(
            text.iter().any(|t| t.starts_with(&mine.display().to_string())),
            "the directory nothing was found in is named anyway: {text:?}",
        );
        assert_eq!(
            rows.iter().filter(|r| r.raw == "nothing here").count(),
            1,
            "and only that one is marked empty",
        );
        assert!(text.iter().any(|t| t.trim() == "keymap.toml"), "{text:?}");

        // The paths go somewhere; the heading and the warning do not.
        let file = rows.iter().find(|r| r.text.trim() == "keymap.toml").unwrap();
        assert!(matches!(file.goes_to, Some(Act::Reveal(_))), "a file is revealed in the list");
        let empty = rows.iter().find(|r| r.raw == "nothing here").unwrap();
        assert!(matches!(empty.goes_to, Some(Act::Cd { .. })), "a directory is opened");
        let heading = rows.iter().find(|r| r.keys == "config").unwrap();
        assert!(heading.goes_to.is_none(), "a heading is not a link");
        let warned = rows.iter().find(|r| r.warning).unwrap();
        assert!(warned.goes_to.is_none(), "nor is a warning");
    }
    /// #203: a file read but none of whose settings took effect is marked,
    /// and a file waiting on disk turns "nothing found" into "nothing read
    /// yet".
    #[test]
    fn a_file_nothing_was_read_from_is_marked() {
        let dir = crate::util::test_dir("help-none-read");
        let broken = dir.join("yazi.toml");
        std::fs::write(&broken, "[[preview]]\nname = \"*.md\"\n").expect("write the config");
        let ctx = egui::Context::default();
        let mut app = App::new(crate::config::Config::load(), dir.clone(), ctx);
        app.cfg.loaded = vec![broken.clone()];
        app.cfg.unread = vec![broken.clone()];
        let rows = config_rows(&app, std::slice::from_ref(&dir));
        let row = rows.iter().find(|r| r.text.trim() == "yazi.toml").expect("listed");
        assert!(row.warning, "marked");
        assert_eq!(row.raw, "nothing in it was read — see below");

        // Nothing read at startup, and a file there now.
        app.cfg.loaded.clear();
        app.cfg.unread.clear();
        let rows = config_rows(&app, std::slice::from_ref(&dir));
        let note = defaults_note(&app, &rows).expect("nothing was read");
        assert_eq!(note.text, "(nothing read yet; the defaults are in use)");
        let empty = crate::util::test_dir("help-none-there");
        let rows = config_rows(&app, std::slice::from_ref(&empty));
        assert_eq!(defaults_note(&app, &rows).unwrap().text, "(nothing found in either; the defaults are in use)");
    }

    /// A config file written after the window opened is named, not hidden.
    ///
    /// `filer.toml` created while filer is running is the ordinary way to reach
    /// this: the file is right there in the directory the panel is listing, and
    /// the panel said "nothing here" -- which reads as filer being unable to see
    /// it rather than not having looked since it started.
    #[test]
    fn a_file_on_disk_that_was_not_read_says_so() {
        let dir = crate::util::test_dir("help-unread");
        let written = dir.join("filer.toml");
        std::fs::write(&written, "[ui]\nfont_size = 16.0\n").expect("write the config");

        let ctx = egui::Context::default();
        let mut app = App::new(crate::config::Config::load(), dir.clone(), ctx);
        // What startup read: not this file, because it did not exist yet.
        app.cfg.loaded = vec![dir.join("keymap.toml")];

        let rows = config_rows(&app, std::slice::from_ref(&dir));
        let unread = rows
            .iter()
            .find(|r| r.text.trim() == "filer.toml")
            .expect("the file on disk is a row of its own");
        assert!(unread.warning, "and it is marked, not listed as read");
        assert!(unread.raw.contains("not read yet"), "{:?}", unread.raw);
        // The key comes from the keymap rather than from this string.
        assert!(unread.raw.contains("<C-F5>"), "{:?}", unread.raw);
        let goes = Some(Act::Reveal(written.display().to_string()));
        assert_eq!(unread.goes_to, goes, "clicking it still goes to the file");
        assert!(
            !rows.iter().any(|r| r.raw == "nothing here"),
            "a directory holding an unread file is not empty",
        );

        std::fs::remove_file(&written).ok();
    }

    /// A directory holding one config file, and an app that has not read it.
    ///
    /// Written to disk rather than faked, because `config_rows` decides what is
    /// unread by asking the filesystem (`p.is_file()`) and comparing that with
    /// `cfg.loaded`. A test that only set `loaded` would be testing nothing:
    /// the row exists because the file does.
    fn unread(label: &str) -> (std::path::PathBuf, std::path::PathBuf, App) {
        let dir = crate::util::test_dir(label);
        let written = dir.join("filer.toml");
        std::fs::write(&written, "[ui]\nfont_size = 16.0\n").expect("write the config");
        let ctx = egui::Context::default();
        let mut app = App::new(crate::config::Config::load(), dir.clone(), ctx);
        app.cfg.loaded = Vec::new();
        (dir, written, app)
    }

    /// TESTING.md 33.17: once the config has been re-read, the row is an
    /// ordinary one.
    ///
    /// The marker has to come off, not merely be joined by a second row. It
    /// says "filer has not looked since it started", and after `<C-F5>` that is
    /// no longer true -- a row still carrying it would send the reader to press
    /// the key again, which is the one thing that cannot help. `loaded` is what
    /// `Config::load` fills in, so putting the path there is what a reload
    /// leaves behind; the panel is not told about the reload any other way.
    #[test]
    fn a_file_that_has_since_been_read_loses_its_marker() {
        let (dir, written, mut app) = unread("help-reread");

        let before = config_rows(&app, std::slice::from_ref(&dir));
        let row = before.iter().find(|r| r.text.trim() == "filer.toml").expect("a row before");
        assert!(row.warning, "unread to begin with, or the test proves nothing");

        // What the reload changed: the file is now among the ones that were read.
        app.cfg.loaded = vec![written.clone()];
        let after = config_rows(&app, std::slice::from_ref(&dir));
        let rows: Vec<&HelpRow> = after.iter().filter(|r| r.text.trim() == "filer.toml").collect();
        assert_eq!(rows.len(), 1, "one row for one file, not the read one and the unread one");
        assert!(!rows[0].warning, "and it is no longer marked");
        assert_eq!(rows[0].raw, "", "nor does it carry the note: {:?}", rows[0].raw);
        assert!(
            !after.iter().any(|r| r.raw.contains("not read yet")),
            "nothing in the panel still says so: {:?}",
            after.iter().map(|r| r.raw.as_str()).collect::<Vec<_>>(),
        );
    }

    /// TESTING.md 33.18: the note on the row names the key that is bound now.
    ///
    /// `the_reload_key_is_looked_up` covers the lookup; this covers the row,
    /// which is the part the reader sees. The two are worth separating because
    /// the note is a `format!` and the bug this guards against is a default
    /// written back into the string -- something a passing `key_for` would not
    /// notice at all.
    #[test]
    fn a_rebound_reload_key_is_the_one_the_unread_row_names() {
        let (dir, _written, mut app) = unread("help-rebound");
        let text = "[[mgr.keymap]]\non = \"<F9>\"\nrun = \"config_reload\"\n";
        let (km, warnings) = crate::config::Keymap::load(&[text]);
        assert!(warnings.is_empty(), "the rebinding has to load clean: {warnings:?}");
        app.cfg.keymap = km;

        let rows = config_rows(&app, std::slice::from_ref(&dir));
        let row = rows.iter().find(|r| r.text.trim() == "filer.toml").expect("the unread row");
        assert_eq!(row.raw, "on disk, not read yet — <F9> re-reads config");
        assert!(!row.raw.contains("<C-F5>"), "the default is not written into it: {:?}", row.raw);
    }

    /// The reload key is read out of the keymap in force.
    #[test]
    fn the_reload_key_is_looked_up() {
        let ctx = egui::Context::default();
        let mut app = App::new(crate::config::Config::load(), std::env::temp_dir(), ctx);
        assert_eq!(key_for(&app, &Act::ConfigReload).as_deref(), Some("<C-F5>"));

        let text = "[[mgr.keymap]]\non = \"<F9>\"\nrun = \"config_reload\"\n";
        let (km, _) = crate::config::Keymap::load(&[text]);
        app.cfg.keymap = km;
        let named = key_for(&app, &Act::ConfigReload);
        assert_eq!(named.as_deref(), Some("<F9>"), "a rebound key is the one the panel names");
    }
}

/// The folder comparison, on screen.
///
/// `compare_trees` and the keys already have tests of their own; what these add
/// is that the result reaches the frame -- the signs, the footer's tally and the
/// message an identical pair gets. TESTING.md 45.1, 45.5 and 45.7.
#[cfg(test)]
mod diff_frame {
    use crate::app::{DiffOverlay, Overlay};
    use crate::diff::{Outcome, TreeCounts, TreeRow, TreeState};
    use crate::ui::harness::Screen;
    use std::path::PathBuf;

    fn row(rel: &str, state: TreeState) -> TreeRow {
        TreeRow { rel: PathBuf::from(rel), state, dir: false, left: 1, right: 2 }
    }

    /// A window with the comparison of `rows` open.
    fn showing(rows: Vec<TreeRow>) -> Screen {
        let mut s = Screen::open(crate::util::test_dir("frame-diff"));
        s.app.overlay = Overlay::Diff(DiffOverlay {
            left: PathBuf::from("l"),
            right: PathBuf::from("r"),
            outcome: Some(Outcome::Tree {
                counts: TreeCounts::of(&rows),
                rows,
                truncated: false,
            }),
            offset: 0,
            rows: 10,
            cursor: 0,
            hide_same: false, back: None,
        });
        s
    }

    /// `z` on screen: the matches leave the list, the footer still counts them
    /// and says they are hidden, and `z` again brings them back (Q23).
    #[test]
    fn hidden_matches_leave_the_list_and_the_footer_says_so() {
        let mut s = showing(vec![row("changed.txt", TreeState::Differ), row("same.txt", TreeState::Same)]);
        let f = s.typed("z");
        assert!(f.says("changed.txt"), "the difference stays: {:?}", f.texts);
        assert!(!f.says("same.txt"), "the match is gone from the list: {:?}", f.texts);
        assert!(f.says("1 match"), "and still counted: {:?}", f.texts);
        assert!(f.says("matches hidden (z)"), "and the footer says why it is missing: {:?}", f.texts);
        assert!(f.says("z: hide matches"), "the title offers the key: {:?}", f.texts);

        let f = s.typed("z");
        assert!(f.says("same.txt"), "back: {:?}", f.texts);
        assert!(!f.says("matches hidden"), "{:?}", f.texts);
    }

    /// Every sign is drawn, and the footer counts each kind.
    #[test]
    fn the_signs_and_the_tally_are_on_screen() {
        let mut s = showing(vec![
            row("only-left.txt", TreeState::LeftOnly),
            row("only-right.txt", TreeState::RightOnly),
            row("changed.txt", TreeState::Differ),
            row("same.txt", TreeState::Same),
            row("huge.bin", TreeState::Unread),
        ]);
        let f = s.draw();

        for name in ["only-left.txt", "only-right.txt", "changed.txt", "same.txt", "huge.bin"] {
            assert!(f.says(name), "the row for {name} is drawn: {:?}", f.texts);
        }
        for sign in ["<", ">", "~", "=", "?"] {
            assert!(
                f.texts.iter().any(|t| t == sign),
                "the sign {sign} is drawn on its own: {:?}",
                f.texts,
            );
        }
        assert!(
            f.says("1 only left") && f.says("1 only right") && f.says("1 differ"),
            "the footer counts each kind: {:?}",
            f.texts,
        );
        // TESTING.md 45.7: a pair too big to read is its own count, not a match.
        assert!(f.says("1 match"), "and the matches: {:?}", f.texts);
        assert!(f.says("1 too big to read"), "separately from the unread: {:?}", f.texts);
    }

    /// Two identical trees say so rather than drawing an empty pane.
    ///
    /// TESTING.md 45.5. `compare_trees` drops the rows that match when nothing
    /// differs, so "no rows" and "nothing to compare" look the same from here --
    /// which is why the message exists and why it is worth asserting.
    #[test]
    fn an_identical_pair_says_so() {
        let f = showing(Vec::new()).draw();
        assert!(f.says("every file matches"), "{:?}", f.texts);
    }

    /// The cursor is drawn on the row it is on, and follows `j`.
    ///
    /// The keys are tested in `app::diff_tree_keys`; this is that the selection
    /// is *visible*, which is the difference between a list you can navigate and
    /// one that only looks static. TESTING.md 45.2.
    #[test]
    fn the_selection_is_drawn_and_moves() {
        let mut s = showing(vec![
            row("a.txt", TreeState::Differ),
            row("b.txt", TreeState::Differ),
            row("c.txt", TreeState::Differ),
        ]);
        let hl = s.app.cfg.theme.hovered_bg;
        let top_of_highlight = |s: &mut Screen| {
            let f = s.draw();
            let rects = f.filled(hl);
            assert_eq!(rects.len(), 1, "one row is highlighted, got {rects:?}");
            rects[0].top()
        };

        let first = top_of_highlight(&mut s);
        s.typed("jj");
        let now = top_of_highlight(&mut s);
        assert!(now > first, "the highlight moved down: {first} then {now}");
    }
}

/// TESTING.md section 34: the help panel's own scrolling.
///
/// `app::help_keys` already drives the `[help]` layer with `help_rows` and
/// `help_lines` written in by hand, which is the arithmetic. What it cannot
/// answer is where those two numbers come from: only the renderer knows how
/// tall the panel came out, and a panel that measured itself wrong would pass
/// every one of those tests while paging by the wrong distance on screen. These
/// drive the real frame, so the distances are the panel's own height, and the
/// lines asserted on are the lines that were painted.
#[cfg(test)]
mod help_frame {
    use crate::app::{InputKind, InputOverlay, Overlay, SpotOverlay, TasksOverlay};
    use crate::ui::harness::Screen;
    use egui::{Event, Key, Modifiers, Pos2};

    fn alt() -> Modifiers {
        Modifiers { alt: true, ..Default::default() }
    }

    /// Ctrl as a window sends it: egui sets `command` alongside `ctrl` on every
    /// platform but macOS, and `keys::from_egui` reads either.
    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, command: true, ..Default::default() }
    }

    fn chord(key: Key, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// One notch of the wheel with the pointer at `at`.
    ///
    /// The pointer has to move in the same frame: `rect_contains_pointer` is
    /// what decides which surface a turn belongs to, and a context that has
    /// never seen a pointer position answers no to all of them.
    fn wheel(s: &mut Screen, at: Pos2) {
        s.feed(vec![
            Event::PointerMoved(at),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ]);
    }

    /// A point over the left-hand file list, where a turn of the wheel belongs
    /// to the list unless a panel has claimed it.
    fn over_the_list(s: &Screen) -> Pos2 {
        Pos2::new(s.rect().width() * 0.45, s.rect().height() * 0.5)
    }

    /// The panel open on filer's own defaults, having measured itself once.
    fn showing_help(label: &str) -> Screen {
        let mut s = Screen::open(crate::util::test_dir(label));
        s.typed("~");
        assert!(matches!(s.app.overlay, Overlay::Help), "`~` opened the panel");
        s
    }

    /// 39.9: opened from the terminal pane, the panel lists the pane's own keys
    /// first, under their own heading, and the list's after them.
    #[test]
    fn help_from_the_pane_lists_the_panes_keys_first() {
        let mut s = Screen::open(crate::util::test_dir("help-pane"));
        s.app.term_focus = true;
        s.app.act(crate::config::cmd::Act::Help);
        let f = s.draw();
        let at = |needle: &str| f.texts.iter().position(|t| t == needle);
        let pane = at("keys in the terminal pane").expect("the pane's heading");
        let term_up = at("Scroll the terminal up").expect("the pane's own `<A-k>`");
        let list = at("keys in the list (<C-t> to get there)").expect("the list's heading");
        assert!(pane < term_up && term_up < list, "the pane's keys come first: {:?}", f.texts);

        // From the list, nothing changes.
        let mut s = showing_help("help-list");
        let f = s.draw();
        assert!(f.texts.iter().any(|t| t == "keys"));
        assert!(!f.texts.iter().any(|t| t == "keys in the terminal pane"));
    }

    /// #104: the right-click paste is not a key, so it has its own words
    /// rather than being missing from the panel.
    #[test]
    fn the_help_says_what_the_mouse_does() {
        let mut s = showing_help("help-mouse");
        let f = s.draw();
        assert!(f.texts.iter().any(|t| t.starts_with("right-click in a prompt")), "{:?}", f.texts);
    }

    /// A directory of `n` files, listed. Long enough that the list underneath
    /// has somewhere to scroll to, which is what makes 34.9 assertable.
    fn listing(label: &str, n: usize) -> Screen {
        let dir = crate::util::test_dir(label);
        let mut entries = Vec::new();
        for i in 0..n {
            let p = dir.join(format!("f{i:03}.txt"));
            std::fs::write(&p, "x").unwrap();
            entries.push(crate::fs::Entry::from_path(p).unwrap());
        }
        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir, std::sync::Arc::new(entries), true);
        s
    }

    /// The `[mgr]` bindings start this far down the list; everything above them
    /// is the config section the panel opens with.
    fn head(s: &Screen) -> usize {
        s.app.help_lines - s.app.cfg.keymap.mgr.len()
    }

    /// The text the panel draws in the middle column for line `line`, which has
    /// to be one of the `[mgr]` bindings.
    fn text_of(s: &Screen, line: usize) -> String {
        let b = &s.app.cfg.keymap.mgr[line - head(s)];
        match b.desc.is_empty() {
            true => b.raw.clone(),
            false => b.desc.clone(),
        }
    }

    /// The panel measures itself from the frame, and the defaults are long
    /// enough for the rest of the section to mean anything.
    ///
    /// The section says so in its own words -- "the list is long enough to
    /// scroll only if the keymap is; the defaults are" -- and every check below
    /// rests on it, so it is asserted once rather than assumed nine times.
    #[test]
    fn the_panel_measures_itself_against_its_own_height() {
        let mut s = showing_help("frame-help-open");
        let f = s.draw();
        assert!(
            f.says("Keys — <Esc> close"),
            "the panel names its own keys along the top: {:?}",
            f.texts,
        );
        assert!(s.app.help_rows > 0, "the renderer counted the rows that fit");
        assert!(
            s.app.help_lines > s.app.help_rows,
            "{} lines in a panel {} rows tall, so there is a scroll to test",
            s.app.help_lines,
            s.app.help_rows,
        );
        // The config section comes first, and its heading is the first line --
        // which is what the checks below watch leave the top of the panel.
        assert!(f.texts.contains(&"config".to_owned()), "line 0 is on screen: {:?}", f.texts);
    }

    /// 34.1: `j` / `k` and the arrows move one line, and the panel redraws from
    /// it.
    #[test]
    fn one_line_on_j_k_and_on_the_arrows() {
        let mut s = showing_help("frame-help-line");
        let heading = "config".to_owned();

        let f = s.typed("j");
        assert_eq!(s.app.help_scroll, 1);
        assert!(!f.texts.contains(&heading), "the first line has gone off the top: {:?}", f.texts);
        let f = s.typed("k");
        assert_eq!(s.app.help_scroll, 0);
        assert!(f.texts.contains(&heading), "and come back: {:?}", f.texts);

        s.feed(vec![chord(Key::ArrowDown, Modifiers::NONE)]);
        assert_eq!(s.app.help_scroll, 1, "<Down> is the same one line");
        s.feed(vec![chord(Key::ArrowUp, Modifiers::NONE)]);
        assert_eq!(s.app.help_scroll, 0, "<Up> likewise");
    }

    /// 34.2, 34.3 and 34.4: half a panel on the Alt and the Ctrl pair, a whole
    /// one on the page keys.
    #[test]
    fn half_a_panel_on_both_pairs_and_a_whole_one_on_the_page_keys() {
        let mut s = showing_help("frame-help-page");
        let rows = s.app.help_rows;
        let half = rows / 2;
        assert!(half > 0, "the panel is more than one row tall");

        for (down, up, step, name) in [
            (chord(Key::J, alt()), chord(Key::K, alt()), half, "<A-j>/<A-k>"),
            (chord(Key::D, ctrl()), chord(Key::U, ctrl()), half, "<C-d>/<C-u>"),
            (
                chord(Key::PageDown, Modifiers::NONE),
                chord(Key::PageUp, Modifiers::NONE),
                rows,
                "<PageDown>/<PageUp>",
            ),
        ] {
            s.feed(vec![down]);
            assert_eq!(s.app.help_scroll, step, "{name} moved by {step}");
            s.feed(vec![up]);
            assert_eq!(s.app.help_scroll, 0, "{name} came back");
        }
    }

    /// 34.2 again, the part the arithmetic cannot state: "half the panel's
    /// height -- not the file list's".
    ///
    /// `<C-d>` is half a page on both surfaces, so the bug this rules out is the
    /// panel paging by the height of the list it is drawn over. The two are
    /// different numbers only because the panel is inset and spends a row on its
    /// title, which is a fact about the frame: nothing but a drawn window has
    /// both heights to compare.
    #[test]
    fn half_a_panel_is_not_half_the_list_underneath() {
        let mut s = listing("frame-help-not-list", 200);
        s.draw();
        // The list first, with no panel over it.
        s.feed(vec![chord(Key::D, ctrl())]);
        let in_the_list = s.app.tab().current.cursor;
        let list_rows = s.app.tab().page_rows;
        assert_eq!(in_the_list, list_rows / 2, "the list moved by half of its own page");

        let mut s = listing("frame-help-not-list-2", 200);
        s.typed("~");
        s.feed(vec![chord(Key::D, ctrl())]);
        assert_eq!(s.app.help_scroll, s.app.help_rows / 2, "the panel used its own height");
        assert_ne!(
            s.app.help_rows, list_rows,
            "the panel is {} rows and the list {list_rows}, so the two distances can be told apart",
            s.app.help_rows,
        );
        assert_ne!(
            s.app.help_scroll, in_the_list,
            "a panel that moved {in_the_list} would have paged by the list's height",
        );
    }

    /// 34.5, 34.6 and 34.7: the scroll stops with the last line at the bottom
    /// of a full panel, and one press comes straight back.
    ///
    /// The regression this guards is worth spelling out: `help_scroll` was
    /// incremented raw, so `j` held down ran the number hundreds past the end
    /// while the panel sat still, and every press back up moved a number
    /// nothing was drawing from. The keys looked dead for exactly as many
    /// presses as had been wasted.
    #[test]
    fn it_stops_with_the_last_line_at_the_bottom_of_a_full_panel() {
        let mut s = showing_help("frame-help-bottom");
        let full = s.draw().texts.len();
        let rows = s.app.help_rows;
        let stop = s.app.help_lines - rows;
        let last = s.app.help_lines - 1;

        let f = s.typed("G");
        assert_eq!(s.app.help_scroll, stop, "the last line is the bottom one, not the top one");
        assert!(
            f.texts.contains(&text_of(&s, last)),
            "the last line is on screen: {:?}",
            f.texts.last(),
        );
        assert!(
            f.texts.contains(&text_of(&s, stop)),
            "and so is the line {rows} above it, which is what makes the panel full",
        );
        // Two strings per line at least -- the keys column and the description
        // -- so a panel showing one line could not come near this.
        assert!(
            f.texts.len() + 2 >= full,
            "the panel is as full at the end of the scroll as at its start: {} against {full}",
            f.texts.len(),
        );

        // 34.6: one `k` moves, rather than spending a press undoing an
        // overshoot that was never drawn.
        let f = s.typed("k");
        assert_eq!(s.app.help_scroll, stop - 1);
        assert!(
            f.texts.contains(&text_of(&s, stop - 1)),
            "the line above has come into view: {:?}",
            f.texts,
        );

        // 34.7: `gg` is the top, and there is nothing above it.
        let f = s.typed("gg");
        assert_eq!(s.app.help_scroll, 0);
        assert!(f.texts.contains(&"config".to_owned()), "back to the first line: {:?}", f.texts);
        s.typed("k");
        assert_eq!(s.app.help_scroll, 0, "`k` at the top does nothing");
    }

    /// 34.8 and 34.9: the wheel turns the panel, and the list underneath stays
    /// exactly where it was.
    ///
    /// The control comes first on purpose. The same events over the same window
    /// with no panel open *do* scroll the list, so this cannot pass by the
    /// wheel never having arrived -- which is the way a test like this goes
    /// quietly wrong.
    #[test]
    fn the_wheel_turns_the_panel_and_leaves_the_list_alone() {
        let mut s = listing("frame-help-wheel", 200);
        s.draw();
        let at = over_the_list(&s);

        wheel(&mut s, at);
        assert!(
            s.app.tab().current.offset > 0,
            "the control: with nothing over it, the list answers the wheel",
        );

        s.typed("~");
        s.draw();
        let before = (s.app.tab().current.offset, s.app.tab().current.cursor);
        let middle = s.rect().center();
        wheel(&mut s, middle);
        assert!(s.app.help_scroll > 0, "the panel took the turn");
        assert_eq!(
            (s.app.tab().current.offset, s.app.tab().current.cursor), before,
            "and the list did not move under it -- which only shows once the panel closes",
        );
    }

    /// 34.10 and 34.11: a panel over the list owns the wheel; a one-row prompt
    /// does not.
    ///
    /// The prompt is the case that must keep working: it is one row at the
    /// bottom and the list above it is exactly what is being read while it is
    /// open, so taking the wheel away there would be the fix overshooting.
    #[test]
    fn a_panel_owns_the_wheel_and_a_prompt_leaves_it() {
        for (label, overlay) in [
            ("tasks", Overlay::Tasks(TasksOverlay { cursor: 0 })),
            ("spot", Overlay::Spot(SpotOverlay { cursor: 0, scroll: 0 })),
        ] {
            let mut s = listing(&format!("frame-help-modal-{label}"), 200);
            s.app.overlay = overlay;
            s.draw();
            let before = (s.app.tab().current.offset, s.app.tab().current.cursor);
            let at = over_the_list(&s);
            wheel(&mut s, at);
            assert_eq!(
                (s.app.tab().current.offset, s.app.tab().current.cursor), before,
                "nothing moves under the {label} panel",
            );
        }

        let mut s = listing("frame-help-prompt", 200);
        s.app.overlay = Overlay::Input(InputOverlay {
            kind: InputKind::Filter,
            title: "filter".into(),
            text: String::new(),
            initial_selection: None,
            focused: true,
            completion: Vec::new(),
            completion_at: 0,
        });
        s.draw();
        let at = over_the_list(&s);
        wheel(&mut s, at);
        assert!(
            s.app.tab().current.offset > 0,
            "a filter prompt is one row; the list above it still scrolls",
        );
    }

    /// 34.12: each of the four keys closes the panel, through the same event
    /// path the window uses.
    #[test]
    fn four_keys_close_it() {
        for text in ["q", "~"] {
            let mut s = showing_help(&format!("frame-help-close-{text}"));
            s.typed(text);
            assert!(s.app.overlay.is_none(), "`{text}` closed the panel");
        }
        for (name, key) in [("<F1>", Key::F1), ("<Esc>", Key::Escape)] {
            let mut s = showing_help(&format!("frame-help-close-{}", key.name()));
            s.feed(vec![chord(key, Modifiers::NONE)]);
            assert!(s.app.overlay.is_none(), "`{name}` closed the panel");
        }
    }

    /// 34.13: a key added to the `[help]` layer scrolls the drawn panel.
    ///
    /// The panel whose subject is the keymap is the one that used to read its
    /// keys off the event loop, where no rebinding could reach them. That the
    /// layer is consulted is `app::help_keys`'s check; that the panel then
    /// draws from somewhere else is this one.
    #[test]
    fn a_rebound_key_scrolls_the_drawn_panel() {
        let mut s = Screen::open(crate::util::test_dir("frame-help-rebind"));
        let (km, _) = crate::config::Keymap::load(&["[[help.keymap]]\non = \"n\"\nrun = \"arrow 1\"\n"]);
        s.app.cfg.keymap = km;
        s.typed("~");
        assert!(s.draw().texts.contains(&"config".to_owned()), "the first line is on screen");

        let f = s.typed("n");
        assert_eq!(s.app.help_scroll, 1, "the added key scrolls");
        assert!(!f.texts.contains(&"config".to_owned()), "and the panel redrew from line 1");
    }

    /// 34.14, the half of it the program actually reaches: a panel that grew
    /// taller comes back to the new bottom without a key being pressed.
    ///
    /// More lines fit, so the last line is reached from further up the list, and
    /// a scroll position saved against the old height is now past the end of the
    /// new one. The renderer re-clamps for exactly that.
    ///
    /// `Act::Scale` is run directly rather than pressed, because **`<C-->` does
    /// not reach it while the panel is open** -- the `[help]` layer has no scale
    /// binding and does not fall through to `[mgr]`. That is reported in
    /// QA-REPORT.md rather than asserted here; what is asserted is the clamp,
    /// which is what the row is about and what regressed before v0.34.0.
    ///
    /// Pressing the chord here instead would look like it worked and prove
    /// nothing: `main` switches egui's own `zoom_with_keyboard` off on the real
    /// context and the harness does not, so in a test egui resizes the window
    /// itself, at `end_pass`, whether or not any binding ran. QA-REPORT.md
    /// proposes closing that gap.
    #[test]
    fn a_taller_panel_comes_back_to_the_new_bottom() {
        let mut s = showing_help("frame-help-scale");
        s.typed("G");
        let rows = s.app.help_rows;
        let lines = s.app.help_lines;
        assert_eq!(s.app.help_scroll, lines - rows, "parked at the bottom to begin with");

        s.app.act(crate::config::cmd::Act::Scale(crate::config::cmd::ScaleTo::Out));
        // `set_zoom_factor` lands at the start of the next frame, so the panel
        // is only taller one frame later -- which is the frame this asserts on.
        let f = s.draw();
        assert!(
            s.app.help_rows > rows,
            "a smaller font fits more lines: {rows} then {}",
            s.app.help_rows,
        );
        assert_eq!(s.app.help_lines, lines, "the list itself is the same length");
        assert_eq!(
            s.app.help_scroll, lines - s.app.help_rows,
            "still parked at the bottom, at the stop the new height put there",
        );
        assert!(
            f.texts.contains(&text_of(&s, lines - 1)),
            "and the last line is still on screen: {:?}",
            f.texts.last(),
        );
    }
}

/// TESTING.md 13.10 to 13.16: the spot panel's Link section.
///
/// The section is `spot::link`'s, and the worker is what normally runs it.
/// Called here on the test's own thread and put where the worker would have put
/// it, so the provider stays in the picture: a section written out by hand would
/// assert only that the renderer can draw rows.
///
/// The symlink rows are `#[cfg(unix)]`. Creating one on Windows needs Developer
/// Mode or an elevated shell -- which is what 13.8 is about -- so a test that
/// made one would fail on the runner the checklist is written for. The hardlink
/// rows need no privilege anywhere and so run on both. What is left for a
/// machine is 13.14 (`Also at` is Windows-only: Unix counts the names but cannot
/// list them), 13.16 (another program holding the file open), and 13.10 to 13.12
/// on Windows.
#[cfg(test)]
mod spot_link_section {
    use crate::app::{Overlay, SpotOverlay};
    use crate::ui::harness::{Painted, Screen};
    use std::path::Path;
    use std::sync::Arc;

    /// The spot panel open on `names[0]`, with the worker's findings already in.
    fn showing(dir: &Path, names: &[&str]) -> Screen {
        let entries: Vec<crate::fs::Entry> =
            names.iter().map(|n| crate::fs::Entry::from_path(dir.join(n)).unwrap()).collect();
        let mut s = Screen::open(dir.to_path_buf());
        s.app.tabs[0].current =
            crate::core::folder::Folder::from_entries(dir.to_path_buf(), Arc::new(entries), true);
        s.app.overlay = Overlay::Spot(SpotOverlay { cursor: 0, scroll: 0 });
        let on = dir.join(names[0]);
        s.app.spotted = Some((on.clone(), crate::spot::inspect(&on)));
        s
    }

    /// Whether the section was drawn at all, by its title.
    ///
    /// `Link` exactly: `Link to a file` is the `File` section's own `Kind` and
    /// is on screen for every symlink whether this section appears or not.
    fn has_link_section(f: &Painted) -> bool {
        f.texts.iter().any(|t| t == "Link")
    }

    /// The value drawn beside `key` in the Link section.
    ///
    /// A section is drawn as its title and then key, value, key, value in paint
    /// order, so the row is found by walking forward from the title -- from the
    /// title because `Kind` is a row in the `File` section as well.
    ///
    /// Read as a whole string rather than searched for, because the panel elides
    /// a value too wide for its column *itself*, with a `…` in the middle: a
    /// long path is not on screen in full and `says` would never find it.
    fn row(f: &Painted, key: &str) -> Option<String> {
        let title = f.texts.iter().position(|t| t == "Link")?;
        let at = f.texts[title..].iter().position(|t| t == key)? + title;
        f.texts.get(at + 1).cloned()
    }

    /// 13.10: an absolute symlink says what it is, where it points and where
    /// that lands.
    #[cfg(unix)]
    #[test]
    fn a_symlink_says_where_it_points_and_where_that_lands() {
        let dir = crate::util::test_dir("spot-link-abs");
        std::fs::write(dir.join("t.txt"), "x").unwrap();
        std::os::unix::fs::symlink(dir.join("t.txt"), dir.join("abs")).unwrap();

        let f = showing(&dir, &["abs", "t.txt"]).draw();
        assert!(has_link_section(&f), "the section is drawn: {:?}", f.texts);
        assert_eq!(
            row(&f, "Kind").as_deref(), Some("Symlink"),
            "Kind says which sort of link: {:?}", f.texts,
        );
        let target = row(&f, "Target").expect("a Target row");
        let resolves = row(&f, "Resolves").expect("a Resolves row");
        // Written absolute, so the stored path already is where it lands: the
        // two rows agreeing is what makes 13.11's disagreeing mean something.
        assert_eq!(target, resolves, "both rows name the target: {:?}", f.texts);
        assert!(target.ends_with("t.txt"), "which is the file: {target}");
    }

    /// 13.11: a relative link's two rows differ, which is the point of the pair.
    ///
    /// `_` writes this form and `-` the other, so the two rows reading the same
    /// would hide the one difference that decides whether the link survives
    /// being moved.
    #[cfg(unix)]
    #[test]
    fn a_relative_link_stores_one_path_and_resolves_to_another() {
        let dir = crate::util::test_dir("spot-link-rel");
        std::fs::write(dir.join("t.txt"), "x").unwrap();
        std::os::unix::fs::symlink("t.txt", dir.join("rel")).unwrap();

        let f = showing(&dir, &["rel", "t.txt"]).draw();
        assert_eq!(
            row(&f, "Kind").as_deref(), Some("Symlink (relative)"),
            "Kind says relative, not just Symlink: {:?}", f.texts,
        );
        let target = row(&f, "Target").expect("a Target row");
        let resolves = row(&f, "Resolves").expect("a Resolves row");
        assert_eq!(target, "t.txt", "Target is the path as stored: {:?}", f.texts);
        assert_ne!(target, resolves, "and Resolves is not: {:?}", f.texts);
        assert!(resolves.starts_with('/'), "it is absolute: {resolves}");
        assert!(resolves.ends_with("t.txt"), "and still the same file: {resolves}");
    }

    /// 13.12: a broken link still gets its section, and Resolves carries the
    /// reason.
    #[cfg(unix)]
    #[test]
    fn a_broken_link_still_gets_a_section_and_says_why() {
        let dir = crate::util::test_dir("spot-link-dead");
        std::os::unix::fs::symlink(dir.join("gone.txt"), dir.join("dead")).unwrap();

        let f = showing(&dir, &["dead"]).draw();
        assert!(has_link_section(&f), "the section is there anyway: {:?}", f.texts);
        let resolves = row(&f, "Resolves").expect("a Resolves row");
        assert!(resolves.starts_with("no ("), "it says it did not: {resolves}");
        assert!(resolves.ends_with(')'), "with the OS's own reason inside: {resolves}");
    }

    /// 13.13: a hardlink is named and counted -- the only place in the app one
    /// is visible at all.
    #[test]
    fn a_hardlink_is_named_and_counted() {
        let dir = crate::util::test_dir("spot-link-hard");
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        std::fs::hard_link(dir.join("a.txt"), dir.join("b.txt")).unwrap();

        let f = showing(&dir, &["a.txt", "b.txt"]).draw();
        assert!(has_link_section(&f), "the section is drawn: {:?}", f.texts);
        assert_eq!(row(&f, "Kind").as_deref(), Some("Hardlink"), "Kind: {:?}", f.texts);
        assert_eq!(row(&f, "Links").as_deref(), Some("2"), "and the count: {:?}", f.texts);
        assert!(row(&f, "Target").is_none(), "a hardlink has no target to name");
    }

    /// 13.15: an ordinary file with one name gets no section, rather than one
    /// saying `1`.
    #[test]
    fn an_ordinary_file_gets_no_link_section() {
        let dir = crate::util::test_dir("spot-link-none");
        std::fs::write(dir.join("lone.txt"), "x").unwrap();

        let f = showing(&dir, &["lone.txt"]).draw();
        assert!(f.says("Spot:"), "the panel is open: {:?}", f.texts);
        assert!(!has_link_section(&f), "and says nothing about links: {:?}", f.texts);
        assert!(
            !f.texts.iter().any(|t| t == "Links"),
            "no count either, which would be noise on every file: {:?}", f.texts,
        );
    }
}

/// Fixtures shared by TESTING.md sections 5, 11 and 21.
///
/// All three are a panel drawn over the listing and driven from the keyboard, so
/// all three need the same two things: a directory that is already listed -- the
/// harness runs no scan, so a listing nobody built by hand is empty -- and a way
/// to press what is not a printable character. Kept in one place so the three
/// sections cannot end up disagreeing about what "a listing" is.
#[cfg(test)]
pub(crate) mod overlays {
    use crate::core::folder::Folder;
    use crate::ui::harness::{Painted, Screen};
    use std::path::PathBuf;
    use std::sync::Arc;

    /// A chord, the way the window delivers one.
    pub(crate) fn chord(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// Ctrl as egui reports it: `command` is set alongside `ctrl` on every
    /// platform but macOS, and `keys::from_egui` reads either.
    pub(crate) fn ctrl() -> egui::Modifiers {
        egui::Modifiers { ctrl: true, command: true, ..Default::default() }
    }

    /// `<Enter>`, which is a key event and not text: a prompt's `submit_input`
    /// hangs off `on_key_event`, where the keymap never sees it.
    pub(crate) fn enter() -> egui::Event {
        chord(egui::Key::Enter, egui::Modifiers::NONE)
    }

    /// A directory holding `files`, listed, with the cursor on the first of
    /// them. Every file gets one byte, which is enough for a name to exist and
    /// for a size column to have something to print.
    pub(crate) fn showing(label: &str, files: &[&str]) -> (PathBuf, Screen) {
        let dir = crate::util::test_dir(label);
        let mut entries = Vec::new();
        for name in files {
            let p = dir.join(name);
            std::fs::write(&p, "x").unwrap();
            entries.push(crate::fs::Entry::from_path(p).unwrap());
        }
        let mut s = Screen::open(dir.clone());
        s.app.tabs[0].current = Folder::from_entries(dir.clone(), Arc::new(entries), true);
        (dir, s)
    }

    /// Every drawn string holding `needle`, which is how a view with two columns
    /// is asked whether *both* of them said something.
    pub(crate) fn saying<'a>(f: &'a Painted, needle: &str) -> Vec<&'a String> {
        f.texts.iter().filter(|t| t.contains(needle)).collect()
    }
}

/// TESTING.md section 5: two files, side by side.
///
/// `diff::compare` is pure and `diff::tests` covers the pairing. What no test
/// there can answer is whether the pairing reaches the screen: the two columns
/// are laid out by the renderer, the tint saying which side a line left is
/// painted there, and `ov.rows` -- how far one press of `G` or `<C-d>` goes --
/// is a number only the renderer knows, measured from the panel's own height.
/// `app::diff_scrolling` drives the same keys with that number written in by
/// hand; these drive them against the number a frame worked out.
#[cfg(test)]
mod compare_frame {
    use super::overlays::{chord, ctrl, saying};
    use crate::app::{DiffOverlay, Overlay};
    use crate::ui::harness::{Painted, Screen};

    /// The comparison of two files holding `left` and `right`, already open.
    ///
    /// `compare_files` is the worker's own work done on this thread, so the rows
    /// are the real pairing rather than a fixture written to agree with the
    /// assertion below it.
    fn comparing(label: &str, left: &str, right: &str) -> Screen {
        let dir = crate::util::test_dir(label);
        let (l, r) = (dir.join("compare-left.txt"), dir.join("compare-right.txt"));
        std::fs::write(&l, left).unwrap();
        std::fs::write(&r, right).unwrap();
        let outcome = crate::diff::compare_files(&l, &r, 1 << 20);
        let mut s = Screen::open(dir);
        s.app.overlay = Overlay::Diff(DiffOverlay {
            left: l,
            right: r,
            outcome: Some(outcome),
            offset: 0,
            // What `start_compare` opens with. The renderer replaces it with the
            // number of rows that fit, which is what these tests are here for.
            rows: 1,
            cursor: 0,
            hide_same: false, back: None,
        });
        s
    }

    /// `n` lines reading `line 0`, `line 1`, ... as one file's text.
    fn numbered(n: usize) -> String {
        let body: Vec<String> = (0..n).map(|i| format!("line {i}")).collect();
        format!("{}\n", body.join("\n"))
    }

    /// The footer's `x–y of z`: the only reading on screen of where the view is
    /// parked, and therefore the only way to ask a frame what a key did.
    fn footer(f: &Painted) -> String {
        f.texts
            .iter()
            .find(|t| t.contains('–') && t.contains(" of "))
            .unwrap_or_else(|| panic!("no `x–y of z` footer was drawn: {:?}", f.texts))
            .clone()
    }

    /// The `x` of that footer: the first row on screen, counting from 1.
    fn top(f: &Painted) -> usize {
        let foot = footer(f);
        let head = foot.split('–').next().unwrap_or_default().trim().to_owned();
        head.parse().unwrap_or_else(|_| panic!("footer does not start with a row: {foot}"))
    }

    /// The `z`: how many rows the comparison came out as.
    fn total(f: &Painted) -> usize {
        let foot = footer(f);
        let tail = foot.rsplit(" of ").next().unwrap_or_default();
        let tail = tail.split("  ·").next().unwrap_or_default().trim().to_owned();
        tail.parse().unwrap_or_else(|_| panic!("footer does not end in a total: {foot}"))
    }

    /// 5.1 and 5.4: two gutters, each counting its own file.
    ///
    /// The insertion is what makes the second half worth asserting. From `delta`
    /// on, the same content is line 4 on the left and line 5 on the right, so a
    /// gutter that numbered the *rows* would print one number for both and read
    /// as if nothing had been inserted at all.
    #[test]
    fn each_side_is_numbered_from_its_own_file() {
        let mut s = comparing(
            "frame-compare-numbers",
            "alpha\nbeta\ngamma\ndelta\nepsilon\n",
            "alpha\nbeta\nGAMMA\nextra\ndelta\nepsilon\n",
        );
        let f = s.draw();

        // The head is the same line on both sides, so it is drawn twice with the
        // same number -- once per column. That is 5.1's "line numbers on each
        // side" put as something a frame can answer.
        assert_eq!(
            saying(&f, "1 alpha").len(),
            2,
            "the first line is numbered in both gutters: {:?}",
            f.texts,
        );
        assert!(f.says("   3 gamma") && f.says("   3 GAMMA"), "the edit: {:?}", f.texts);
        // 5.4: past the insertion the two sides differ by one.
        assert!(f.says("   4 delta"), "the left is still counting 4: {:?}", f.texts);
        assert!(f.says("   5 delta"), "the right has reached 5: {:?}", f.texts);
        assert!(f.says("   4 extra"), "and the inserted line is numbered too: {:?}", f.texts);
        assert_eq!(total(&f), 6, "six rows for five lines and an insertion: {}", footer(&f));
    }

    /// 5.2 and 5.3: the edited line sits opposite the line it replaced, and each
    /// side carries the colour its git sign does.
    ///
    /// Both readings come out of the same two rectangles. A row that differs is
    /// tinted on whichever sides it has, so the tints say which rows changed,
    /// which half of the panel each belongs to, and -- by sharing a `y` -- that
    /// the replacement was paired with what it replaced rather than listed as a
    /// removal and an addition a screen apart.
    #[test]
    fn a_replacement_is_tinted_on_both_sides_of_one_row() {
        let mut s = comparing(
            "frame-compare-tint",
            "alpha\nbeta\ngamma\ndelta\n",
            "alpha\nbeta\nGAMMA\ndelta\n",
        );
        let theme = s.app.cfg.theme.clone();
        let mid = s.rect().center().x;
        let f = s.draw();

        // The tint is the git colour at 22%: the same theme entry the signs in
        // the listing use, which is what 5.3 is asking about. What shade that
        // comes out as is still an eye's job.
        let removed = f.filled(theme.git_deleted.gamma_multiply(0.22));
        let added = f.filled(theme.git_added.gamma_multiply(0.22));
        assert_eq!(removed.len(), 1, "one line was replaced: {removed:?}");
        assert_eq!(added.len(), 1, "and one line replaced it: {added:?}");
        assert!(removed[0].center().x < mid, "the removal is in the left column");
        assert!(added[0].center().x > mid, "the addition is in the right one");
        assert_eq!(
            removed[0].top(),
            added[0].top(),
            "and both are the same row, which is the whole point of the view",
        );
    }

    /// 5.6: the scrolling keys move the view and the footer keeps up.
    ///
    /// The distances are the renderer's own, so what is asserted is their order:
    /// `j` is a row, `<C-d>` is a page, `G` is the end. A panel that measured
    /// itself wrong is a panel where those three collapse into each other.
    #[test]
    fn the_footer_counts_along_with_the_scrolling_keys() {
        let text = numbered(400);
        let mut s =
            comparing("frame-compare-scroll", &text, &text.replace("line 199", "LINE 199"));
        let f = s.draw();
        assert_eq!(top(&f), 1, "it opens at the top: {}", footer(&f));
        assert_eq!(total(&f), 400, "and counts every row: {}", footer(&f));

        assert_eq!(top(&s.typed("j")), 2, "`j` is one row: {}", footer(&s.draw()));
        assert_eq!(top(&s.typed("k")), 1, "`k` is one row back");

        let half = top(&s.feed(vec![chord(egui::Key::D, ctrl())]));
        assert!(half > 2, "`<C-d>` is a page rather than a row: {half}");
        assert_eq!(top(&s.feed(vec![chord(egui::Key::U, ctrl())])), 1, "`<C-u>` comes back");

        let bot = top(&s.typed("G"));
        assert!(bot > half, "`G` goes further than a page: {bot} against {half}");
        assert_eq!(top(&s.typed("gg")), 1, "and `gg` is the top again");
    }

    /// 5.6a, 5.6b and 5.6c: what the bottom of the view means.
    ///
    /// Until v0.22.1 `G` parked the offset a screenful past the last row, so `k`
    /// spent a pane's worth of presses climbing back into the part that is drawn
    /// and looked like a dead key. `app::diff_scrolling` covers the arithmetic
    /// with `ov.rows` written in by hand; this is the same three checks against
    /// the height the panel actually came out as.
    #[test]
    fn the_bottom_is_where_the_last_row_is() {
        let text = numbered(400);
        let mut s =
            comparing("frame-compare-bottom", &text, &text.replace("line 199", "LINE 199"));

        let bot = top(&s.typed("G"));
        assert!(bot > 1, "`G` moved at all: {bot}");
        // 5.6a: one press of `k` moves one row, straight away.
        assert_eq!(top(&s.typed("k")), bot - 1, "`k` after `G` moves exactly one row");

        s.typed("G");
        // 5.6b: and `j` at the bottom stays there, with the last row on screen.
        let f = s.typed("j");
        assert_eq!(top(&f), bot, "`j` at the bottom does not walk off it");
        assert!(f.says("400 line 399"), "the last row is still drawn: {:?}", f.texts);

        // 5.6c: a comparison shorter than the pane has nowhere to go.
        let mut s = comparing("frame-compare-short", "a\nb\nc\n", "a\nB\nc\n");
        let f = s.typed("G");
        assert_eq!(top(&f), 1, "nothing moved: {}", footer(&f));
        assert_eq!(total(&f), 3, "every row was on screen already: {}", footer(&f));
    }

    /// 5.5: `n` and `N` walk between the differences, not down the lines of one.
    ///
    /// The first difference is five lines long on purpose. A `n` that stepped one
    /// row at a time would land on row 52 instead of the second block, which is
    /// exactly the distinction the row is asking about.
    #[test]
    fn n_walks_between_the_blocks_and_says_when_it_runs_out() {
        let left: Vec<String> = (0..400).map(|i| format!("line {i}")).collect();
        let mut right = left.clone();
        for line in right.iter_mut().take(55).skip(50) {
            *line = line.to_uppercase();
        }
        right[150] = right[150].to_uppercase();
        let mut s = comparing(
            "frame-compare-find",
            &format!("{}\n", left.join("\n")),
            &format!("{}\n", right.join("\n")),
        );
        assert_eq!(top(&s.draw()), 1, "it starts at the top");

        assert_eq!(top(&s.typed("n")), 51, "`n` parks on the first difference");
        assert_eq!(
            top(&s.typed("n")),
            151,
            "and the next skips the rest of that five-line block",
        );
        let f = s.typed("n");
        assert_eq!(top(&f), 151, "there is no third, so nothing moves");
        assert!(f.says("At the last difference"), "and it says so: {:?}", f.texts);

        assert_eq!(top(&s.typed("N")), 51, "`N` walks back the same way");
        let f = s.typed("N");
        assert_eq!(top(&f), 51, "and stops at the first");
        assert!(f.says("At the first difference"), "saying which end: {:?}", f.texts);
        // The newer end replaces the older rather than contradicting it.
        assert!(!f.says("At the last difference"), "one end at a time: {:?}", f.texts);
    }

    /// 5.7 and 5.8: the two answers that are a sentence rather than a view.
    ///
    /// Both exist so that "nothing was drawn" is never what a comparison looks
    /// like. Identical files would otherwise be thousands of matching rows, and
    /// two binaries a pane of mojibake.
    #[test]
    fn identical_and_binary_each_say_so_in_words() {
        let same = "one\ntwo\n";
        let mut s = comparing("frame-compare-same", same, same);
        let f = s.draw();
        assert!(f.says("The two files are identical."), "{:?}", f.texts);
        assert!(!f.says(" of "), "and no footer counting rows: {:?}", f.texts);

        // A NUL byte is what `as_text` declines on, and the two differ in length
        // so the bytes are not equal either.
        let mut s = comparing("frame-compare-binary", "a\0b\n", "a\0bc\n");
        let f = s.draw();
        assert!(f.says("Not text on both sides, and the bytes differ."), "{:?}", f.texts);
    }

    /// Under the names, both full paths: two folders of one name are the
    /// usual comparison, and the title could not tell them apart.
    #[test]
    fn the_full_paths_are_under_the_title() {
        let mut s = comparing("frame-compare-paths", "a\n", "b\n");
        let f = s.draw();
        let Overlay::Diff(ov) = &s.app.overlay else { panic!("not comparing") };
        let head: String = ov.left.display().to_string().chars().take(8).collect();
        let line = f.texts.iter().find(|t| t.starts_with(&head) && t.contains('↔'));
        let line = line.unwrap_or_else(|| panic!("no paths line: {:?}", f.texts));
        assert!(line.contains("compare-left.txt") && line.contains("compare-right.txt"), "{line}");
    }

    /// A folder row ends in the platform's separator, the one its children use.
    #[test]
    fn a_folder_row_uses_the_native_separator() {
        let row = |rel: &str, dir| crate::diff::TreeRow {
            rel: rel.into(),
            state: crate::diff::TreeState::Same,
            dir,
            left: 0,
            right: 0,
        };
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(super::tree_name(&row("thing", true)), format!("thing{sep}"));
        assert_eq!(super::tree_name(&row("a.txt", false)), "a.txt");
    }

    /// 5.10: `q` closes it, and the title says so before it is pressed.
    #[test]
    fn q_closes_it_and_the_title_said_it_would() {
        let mut s = comparing("frame-compare-close", "a\n", "b\n");
        let f = s.draw();
        assert!(
            f.says("compare-left.txt  ↔  compare-right.txt"),
            "the title names both files: {:?}",
            f.texts,
        );
        assert!(f.says("q: close"), "and the key that closes it: {:?}", f.texts);
        // #98: `n/N differences` read as a count; the keys say what they do.
        assert!(f.says("n / N: next / previous difference"), "{:?}", f.texts);

        s.typed("q");
        assert!(matches!(s.app.overlay, Overlay::None), "`q` closed the comparison");
        assert!(!s.draw().says("q: close"), "and the panel is gone");
    }
}

/// TESTING.md section 11: bulk rename.
///
/// `rename::plan` decides every new name and every refusal, and `rename::tests`
/// covers it. What those cannot see is the panel: the preview is redrawn from
/// `bulk_preview` on every frame, `→` and the reason in brackets are the only
/// place a refusal is said before Enter, and the batch is applied from a key
/// rather than from a call. All of that is on this side of the renderer, and all
/// of it runs on the UI thread -- a rename is not a job, so a frame is enough to
/// watch one happen.
#[cfg(test)]
mod bulk_frame {
    use super::overlays::{chord, ctrl, enter, showing};
    use crate::app::{InputKind, Overlay};
    use crate::ui::harness::{Painted, Screen};
    use std::path::{Path, PathBuf};

    /// The twelve photographs of 11.1, plus the three files the clash and the
    /// swap checks need.
    fn fixture(label: &str) -> (PathBuf, Vec<String>, Screen) {
        let photos: Vec<String> = (1..=12).map(|i| format!("IMG_{i:04}.jpg")).collect();
        let mut files: Vec<&str> = photos.iter().map(String::as_str).collect();
        files.extend(["ab.txt", "ba.txt", "in the way.txt"]);
        let (dir, s) = showing(label, &files);
        (dir, photos, s)
    }

    /// Select `names` and open the prompt on them.
    fn selecting(s: &mut Screen, dir: &Path, names: &[String]) {
        for n in names {
            s.app.tabs[0].selected.insert(dir.join(n));
        }
        s.typed("R");
        assert!(
            matches!(&s.app.overlay, Overlay::Input(ov) if matches!(ov.kind, InputKind::Bulk { .. })),
            "`R` opened the bulk prompt",
        );
    }

    /// Replace the rule in the field with `rule`, the way a person does it:
    /// select the whole field, then type.
    ///
    /// `<C-a>` is egui's, not the keymap's -- `Overlay::Input` hands text
    /// straight to the field. It is here because the prompt opens on
    /// `{name}{ext}`. Since v0.55.0 that rule opens selected, so typing alone
    /// would replace it too (`prompt_selection` tests that); this keeps the
    /// person's own habit, and still works if a later change moves the caret.
    fn rule(s: &mut Screen, rule: &str) -> Painted {
        s.feed(vec![chord(egui::Key::A, ctrl())]);
        s.typed(rule);
        // The preview panel is drawn before the field takes the keystroke, so it
        // is one frame behind what has been typed. At sixty frames a second
        // nobody sees that; a test that asserted on the same frame would be
        // reading the rule before last.
        s.draw()
    }

    /// The panel's `from  →  to` rows, in the order they were drawn.
    fn rows(f: &Painted) -> Vec<&String> {
        f.texts.iter().filter(|t| t.contains("  →  ")).collect()
    }

    /// What is on disk now, sorted, so a rename can be checked without a rescan
    /// -- the listing is the scan worker's to refresh and no worker runs here.
    fn on_disk(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// 11.1: the prompt opens on the rule that changes nothing, with every
    /// selected file listed under it.
    ///
    /// The identity rule is what makes the panel readable the moment it opens:
    /// twelve rows of `IMG_0001.jpg → IMG_0001.jpg` say what the columns mean
    /// before anything has been typed into them.
    #[test]
    fn the_prompt_opens_on_the_rule_that_changes_nothing() {
        let (dir, photos, mut s) = fixture("frame-bulk-open");
        selecting(&mut s, &dir, &photos);
        let f = s.draw();

        assert!(f.says("Bulk rename:"), "the prompt is titled: {:?}", f.texts);
        assert!(f.texts.contains(&"{name}{ext}".to_owned()), "on the identity rule");
        assert!(
            f.says("{name} {ext} {n} {n:3} zero-padded"),
            "with the legend above it: {:?}",
            f.texts,
        );
        let drawn = rows(&f);
        assert_eq!(drawn.len(), 12, "one row per selected file: {drawn:?}");
        assert_eq!(drawn[0], "IMG_0001.jpg  →  IMG_0001.jpg", "and nothing changes yet");
        assert!(!f.says("cannot be used"), "nothing is wrong with it: {:?}", f.texts);
    }

    /// 11.2: the panel follows the rule as it is typed.
    #[test]
    fn the_panel_follows_the_rule_being_typed() {
        let (dir, photos, mut s) = fixture("frame-bulk-typing");
        selecting(&mut s, &dir, &photos);

        let f = rule(&mut s, "holiday-{n:2}{ext}");
        let drawn = rows(&f);
        assert_eq!(drawn.len(), 12, "still one row per file: {drawn:?}");
        assert_eq!(drawn[0], "IMG_0001.jpg  →  holiday-01.jpg", "zero-padded from one");
        assert_eq!(drawn[11], "IMG_0012.jpg  →  holiday-12.jpg", "through to twelve");
    }

    /// 11.3, 11.4 and 11.5: Enter renames them all, `u` puts every name back in
    /// one step, and `U` renames them again.
    ///
    /// One undo for the whole batch is the point: twelve separate steps would
    /// mean twelve presses of `u`, and half a rename is not a state anyone asked
    /// for.
    #[test]
    fn enter_renames_them_all_and_one_undo_puts_them_back() {
        let (dir, photos, mut s) = fixture("frame-bulk-apply");
        selecting(&mut s, &dir, &photos);
        rule(&mut s, "holiday-{n:2}{ext}");

        let f = s.feed(vec![enter()]);
        assert!(f.says("Renamed 12 file(s)"), "the toast counts them: {:?}", f.texts);
        assert!(matches!(s.app.overlay, Overlay::None), "and the prompt closed");
        let after = on_disk(&dir);
        assert!(after.contains(&"holiday-01.jpg".to_owned()), "renamed: {after:?}");
        assert!(after.contains(&"holiday-12.jpg".to_owned()), "all twelve: {after:?}");
        assert!(!after.iter().any(|n| n.starts_with("IMG_")), "none left over: {after:?}");

        let f = s.typed("u");
        assert!(f.says("Put 12 name(s) back"), "one step, and it says so: {:?}", f.texts);
        let back = on_disk(&dir);
        assert!(back.contains(&"IMG_0001.jpg".to_owned()), "the old names: {back:?}");
        assert!(back.contains(&"IMG_0012.jpg".to_owned()), "all of them: {back:?}");
        assert!(!back.iter().any(|n| n.starts_with("holiday-")), "and only those: {back:?}");

        let f = s.typed("U");
        assert!(f.says("Renamed 12 file(s)"), "`U` does it again: {:?}", f.texts);
        assert!(
            on_disk(&dir).contains(&"holiday-07.jpg".to_owned()),
            "and the new names are back",
        );
    }

    /// 11.6: two files headed for one name is said on the rows and refused at
    /// Enter.
    ///
    /// A plain name with no placeholder in it gives every file the same one,
    /// which is the easiest way to reach the check and the one a person reaches
    /// by accident. The first row is refused for a different reason -- the name
    /// belongs to a file that is not moving -- and the rest for colliding with
    /// it, so both wordings are on screen at once.
    #[test]
    fn two_files_headed_for_one_name_are_refused_before_and_at_enter() {
        let (dir, photos, mut s) = fixture("frame-bulk-clash");
        selecting(&mut s, &dir, &photos);
        let f = rule(&mut s, "in the way.txt");

        let drawn = rows(&f);
        assert_eq!(drawn.len(), 12, "every row is still shown: {drawn:?}");
        assert!(
            drawn[0].contains("(already in this directory)"),
            "the first row wants a name that is taken: {}",
            drawn[0],
        );
        assert!(
            drawn[1..].iter().all(|r| r.contains("(two files would get this name)")),
            "and every row after it collides with the first: {drawn:?}",
        );
        assert!(
            f.says("12 name(s) cannot be used — Enter is refused"),
            "the panel counts them and says Enter will not go: {:?}",
            f.texts,
        );

        let before = on_disk(&dir);
        let f = s.feed(vec![enter()]);
        assert!(
            f.says("IMG_0001.jpg: already in this directory"),
            "Enter names the row it stopped on: {:?}",
            f.texts,
        );
        assert_eq!(on_disk(&dir), before, "and renamed nothing at all");
    }

    /// 11.7: one file onto a name that is already in the directory.
    ///
    /// The same refusal as 11.6's first row, reached with nothing to collide
    /// with, so it cannot be the duplicate check answering by accident.
    #[test]
    fn one_file_onto_a_name_already_there_is_refused() {
        let (dir, _, mut s) = fixture("frame-bulk-taken");
        selecting(&mut s, &dir, &["IMG_0001.jpg".to_owned()]);
        let f = rule(&mut s, "in the way.txt");

        let drawn = rows(&f);
        assert_eq!(drawn.len(), 1, "one file, one row: {drawn:?}");
        assert!(drawn[0].contains("(already in this directory)"), "{}", drawn[0]);
        assert!(f.says("1 name(s) cannot be used"), "counted: {:?}", f.texts);

        let before = on_disk(&dir);
        s.feed(vec![enter()]);
        assert_eq!(on_disk(&dir), before, "nothing moved");
    }

    /// 11.8 and 11.9: two files swapping names is neither row's problem, and it
    /// goes through.
    ///
    /// A loop of renames cannot do this -- the second one lands on a name the
    /// first just took -- so `rename::order` parks one file aside first. The
    /// contents are what prove it happened: two files that swapped names still
    /// have the bytes they started with.
    #[test]
    fn a_swap_is_no_row_s_problem_and_the_names_really_change_places() {
        let (dir, _, mut s) = fixture("frame-bulk-swap");
        std::fs::write(dir.join("ab.txt"), "A").unwrap();
        std::fs::write(dir.join("ba.txt"), "B").unwrap();
        selecting(&mut s, &dir, &["ab.txt".to_owned(), "ba.txt".to_owned()]);

        let f = rule(&mut s, "s/^([ab])([ab])/$2$1/");
        let drawn = rows(&f);
        assert_eq!(drawn, ["ab.txt  →  ba.txt", "ba.txt  →  ab.txt"], "the two rows");
        assert!(!f.says("cannot be used"), "and neither is a problem: {:?}", f.texts);

        let f = s.feed(vec![enter()]);
        assert!(f.says("Renamed 2 file(s)"), "both went: {:?}", f.texts);
        assert_eq!(
            std::fs::read_to_string(dir.join("ab.txt")).unwrap(),
            "B",
            "`ab.txt` now holds what `ba.txt` did",
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("ba.txt")).unwrap(),
            "A",
            "and the other way about",
        );

        s.typed("u");
        assert_eq!(std::fs::read_to_string(dir.join("ab.txt")).unwrap(), "A", "`u` swaps back");
        assert_eq!(std::fs::read_to_string(dir.join("ba.txt")).unwrap(), "B");
    }

    /// 11.9b: a name is only going spare when the file holding it is moving.
    ///
    /// Before v0.7.0 being in the selection was enough, and a rule that left one
    /// file's name alone was allowed to rename another onto it -- which failed at
    /// the last moment, with half the batch applied.
    #[test]
    fn a_name_its_owner_is_keeping_is_not_going_spare() {
        let (dir, _, mut s) = fixture("frame-bulk-not-vacated");
        selecting(&mut s, &dir, &["ab.txt".to_owned(), "in the way.txt".to_owned()]);
        let f = rule(&mut s, "s/^ab/in the way/");

        let drawn = rows(&f);
        assert_eq!(drawn.len(), 2, "both selected files are listed: {drawn:?}");
        assert!(
            drawn[0].starts_with("ab.txt") && drawn[0].contains("in the way.txt"),
            "the rule renames `ab.txt` onto the other one: {}",
            drawn[0],
        );
        assert!(
            drawn[0].contains("(already in this directory)"),
            "and that is refused, because the other file is not moving: {}",
            drawn[0],
        );

        let before = on_disk(&dir);
        s.feed(vec![enter()]);
        assert_eq!(on_disk(&dir), before, "so nothing moved");
    }

    /// 11.10: a substitution's group references reach the new name.
    #[test]
    fn group_references_reach_the_new_name() {
        let (dir, photos, mut s) = fixture("frame-bulk-groups");
        selecting(&mut s, &dir, &photos);
        let f = rule(&mut s, r"s/IMG_(\d+)/photo-$1/");

        let drawn = rows(&f);
        assert_eq!(drawn[0], "IMG_0001.jpg  →  photo-0001.jpg", "the digits came through");
        assert!(!f.says("cannot be used"), "and nothing is wrong: {:?}", f.texts);

        s.feed(vec![enter()]);
        assert!(on_disk(&dir).contains(&"photo-0001.jpg".to_owned()), "and it applied");
    }

    /// 11.11: a placeholder nobody knows is named, and nothing is renamed.
    ///
    /// A rule half-typed does not parse either, so this reads as a note rather
    /// than an error -- but it still has to say *which* placeholder, since
    /// `{nane}` for `{name}` is the whole reason anyone is looking.
    #[test]
    fn an_unknown_placeholder_is_named_and_nothing_moves() {
        let (dir, photos, mut s) = fixture("frame-bulk-unknown");
        selecting(&mut s, &dir, &photos);
        let f = rule(&mut s, "{nope}");

        assert!(
            f.says("unknown `{nope}`; use name, ext or n"),
            "the note names the placeholder and the ones that exist: {:?}",
            f.texts,
        );
        assert!(rows(&f).is_empty(), "and no row is offered: {:?}", f.texts);

        let before = on_disk(&dir);
        s.feed(vec![enter()]);
        assert_eq!(on_disk(&dir), before, "Enter renamed nothing");
    }
}

#[cfg(test)]
mod shell_hint_frame {
    use super::overlays::{chord, showing};

    /// The hint under the shell prompt says which of `;` and `:` is open.
    ///
    /// It is the only thing that does: the prompt itself is the same widget
    /// either way. This is pinned because the line used to say `;` "returns at
    /// once" and `:` "waits for it", and **neither key waits** --
    /// `exec::configure` spends `--block` on `CREATE_NEW_CONSOLE`, and nothing
    /// in `exec::shell` calls `wait`. Someone reading "waits for it" runs
    /// `git log -5` under `:`, watches the console open and close inside a
    /// millisecond, and concludes the key is broken.
    #[test]
    fn the_shell_hint_says_which_key_opened_it() {
        let (_dir, mut s) = showing("frame-shell-hint", &["a.txt"]);

        let f = s.typed(";");
        assert!(f.says("no console"), "`;` hides the console: {:?}", f.texts);
        assert!(!f.says("new console"), "and does not claim a new one: {:?}", f.texts);

        s.feed(vec![chord(egui::Key::Escape, egui::Modifiers::NONE)]);
        let f = s.typed(":");
        assert!(f.says("new console"), "`:` gives it one: {:?}", f.texts);

        // Whichever key it was, the placeholders are the same, and neither line
        // promises a wait.
        assert!(f.says("$@ all"), "the placeholder legend stays: {:?}", f.texts);
        assert!(!f.says("waits"), "nothing says filer waits: {:?}", f.texts);
    }
}

/// Q31: a prompt that opens with text in it opens with that text selected, so
/// what is typed or pasted replaces it. `InputOverlay::initial_selection` was
/// worked out for every prompt and never handed to the field before v0.55.0.
#[cfg(test)]
mod prompt_selection {
    use super::overlays::showing;
    use crate::app::Overlay;
    use crate::ui::harness::Screen;

    fn field(s: &Screen) -> String {
        match &s.app.overlay {
            Overlay::Input(ov) => ov.text.clone(),
            _ => panic!("no prompt is open"),
        }
    }

    /// `cd` opens on where you are; a path typed or pasted replaces it
    /// rather than landing on the end of it (30.1, #104).
    #[test]
    fn the_cd_prompt_opens_with_the_path_selected() {
        let (_dir, mut s) = showing("q31-cd", &["a.txt"]);
        s.typed("g");
        s.typed(" ");
        s.draw();
        s.typed("C:/elsewhere");
        s.draw();
        assert_eq!(field(&s), "C:/elsewhere");
    }

    /// `r` selects the name without its extension, as `F2` does.
    #[test]
    fn rename_selects_the_name_up_to_the_extension() {
        let (_dir, mut s) = showing("q31-rename", &["report.txt"]);
        s.typed("r");
        s.draw();
        s.typed("notes");
        s.draw();
        assert_eq!(field(&s), "notes.txt");
    }

    /// The bulk rule opens on `{name}{ext}`, selected.
    #[test]
    fn bulk_rename_opens_with_its_rule_selected() {
        let (_dir, mut s) = showing("q31-bulk", &["a.txt", "b.txt"]);
        s.typed("R");
        s.draw();
        s.typed("x-{name}");
        s.draw();
        assert_eq!(field(&s), "x-{name}");
    }

    /// 30.3 / #107: a right-click on the selection replaces it, with the press
    /// and the release in **different** frames, as a real mouse sends them.
    /// v0.55.0 read the selection the frame before the release, which egui had
    /// already collapsed on the press, and pasted at the click instead.
    #[test]
    fn a_right_click_over_the_selection_replaces_it() {
        let (_dir, mut s) = showing("q31-rclick", &["report.txt"]);
        s.typed("r");
        let f = s.draw();
        assert_eq!(field(&s), "report.txt");
        // On the field's text, which `r` opened with `report` selected.
        let i = f.texts.iter().rposition(|t| t == "report.txt").expect("the field is drawn, after the list");
        let at = f.places[i] + egui::vec2(8.0, 6.0);
        crate::exec::fake_clipboard("notes");
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        s.feed(vec![egui::Event::PointerMoved(at), press(true)]);
        s.feed(vec![press(false)]);
        s.draw();
        assert_eq!(field(&s), "notes.txt", "the selected stem was replaced, not added to");
    }

    /// The archive's name selects the part before `.zip`.
    #[test]
    fn the_archive_name_selects_its_stem() {
        let (_dir, mut s) = showing("q31-compress", &["a.txt"]);
        s.typed("E");
        s.draw();
        s.typed("bundle");
        s.draw();
        assert_eq!(field(&s), "bundle.zip");
    }
}

#[cfg(test)]
mod confirm_frame {
    use crate::app::{ConfirmAction, ConfirmOverlay, Overlay};
    use crate::ui::harness::Screen;

    /// #185: a body line longer than the box wraps, every character drawn.
    /// It used to be cut in the middle at ~87 characters, so the junction
    /// question read `holds the…ull path` and its path pair lost the arrow
    /// and the link's own name.
    #[test]
    fn a_long_body_line_wraps_rather_than_losing_its_middle() {
        let long = format!("{}  →  {}", "C:/Users/someone/AppData/Local/Temp/filer-scratch/w/a1/zdst/the-link", "C:/Users/someone/AppData/Local/Temp/filer-scratch/w/a1/real");
        let sentence = "A junction needs neither. Unlike the symlink it holds the full path, not a relative one,";
        let mut s = Screen::open(crate::util::test_dir("confirm-wrap")).sized(1000.0, 700.0);
        // 1000 px wide puts the box at 600: both lines are longer than that.
        s.app.overlay = Overlay::Confirm(ConfirmOverlay {
            title: "Make a junction instead?".into(),
            body: vec![sentence.to_owned(), long.clone()],
            options: vec![('y', "Make the junction".into()), ('n', "No".into())],
            action: ConfirmAction::Junctions { links: Vec::new() },
            dest: None,
        });
        let f = s.draw();
        for line in [sentence, long.as_str()] {
            let drawn = f.drawn(line).unwrap_or_else(|| panic!("{line:?} not drawn: {:?}", f.texts));
            let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            let got: String = drawn.chars().filter(|c| !c.is_whitespace()).collect();
            assert_eq!(got, squeezed, "every character of {line:?} reaches the screen");
        }
        let (a, b) = (f.placed(sentence).unwrap(), f.placed(&long).unwrap());
        let button = f.placed(" [y] Make the junction ").expect("the button is drawn");
        assert!(a.y < b.y && b.y < button.y, "sentence, path, then buttons, top to bottom: {a:?} {b:?} {button:?}");
        assert!(f.says(" [n] No "), "the buttons are still there: {:?}", f.texts);
    }
}

