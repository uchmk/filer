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
        ov.focused = true;
    }
    if resp.changed() && ov.text != before {
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
    // The waiting half is worth saying here too: `;` and `:` differ by nothing
    // visible once the prompt is open.
    let waits = if *block { "waits for it" } else { "returns at once" };
    let text = format!("$@ all · $0 first · $1 second · no placeholder → appended    ({what}, {waits})");

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

pub fn help(app: &App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    dim(ui, full);
    let rect = modal_rect(full, 0.86, 0.86);
    let inner = modal_frame(ui, rect, &app.cfg.theme, "Keys — <Esc> to close, j/k to scroll", f, row_h);
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(inner);

    // Config provenance first — it answers "did it pick up my yazi config?"
    // before the key list answers "what is bound to what".
    let mut lines: Vec<(String, String, String, bool)> = Vec::new();
    lines.push(("config".into(), String::new(), String::new(), false));
    if app.cfg.loaded.is_empty() {
        lines.push((String::new(), "(no config files found; using defaults)".into(), String::new(), false));
    }
    for p in &app.cfg.loaded {
        lines.push((String::new(), p.display().to_string(), String::new(), false));
    }
    for w in &app.cfg.warnings {
        lines.push((String::new(), w.clone(), String::new(), true));
    }
    lines.push((String::new(), String::new(), String::new(), false));
    lines.push(("keys".into(), String::new(), String::new(), false));
    for b in &app.cfg.keymap.mgr {
        lines.push((
            crate::config::keys::render_seq(&b.on),
            if b.desc.is_empty() { b.raw.clone() } else { b.desc.clone() },
            b.raw.clone(),
            false,
        ));
    }

    let rows = ((inner.height() / row_h).floor() as usize).max(1);
    let start = app.help_scroll.min(lines.len().saturating_sub(1));
    for (i, (keys, desc, raw, is_error)) in lines[start..].iter().take(rows).enumerate() {
        let y = inner.top() + i as f32 * row_h;
        painter.text(
            egui::pos2(inner.left(), y),
            Align2::LEFT_TOP,
            keys,
            f.clone(),
            theme.which_cand.fg.unwrap_or(theme.fg),
        );
        painter.text(
            egui::pos2(inner.left() + 130.0, y),
            Align2::LEFT_TOP,
            desc,
            f.clone(),
            if *is_error { theme.progress_error } else { theme.fg },
        );
        if !raw.is_empty() {
            painter.text(
                egui::pos2(inner.right(), y),
                Align2::RIGHT_TOP,
                raw,
                f.clone(),
                theme.fg_dim,
            );
        }
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
        painter.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            "No tasks",
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
            format!("{}  {}  [{}]", t.kind.verb(), t.label, t.state.label()),
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
        let mut detail = format!(
            "{}/{} files · {} / {}",
            t.files_done,
            t.files,
            crate::util::human_size(t.bytes_done),
            crate::util::human_size(t.bytes)
        );
        if let Some(s) = t.speed() {
            detail.push_str(&format!(" · {}/s", crate::util::human_size(s)));
        }
        if let Some(eta) = t.eta() {
            detail.push_str(&format!(" · {} left", crate::util::fmt_duration(eta)));
        }
        painter.text(egui::pos2(inner.left(), y), Align2::LEFT_TOP, detail, f.clone(), theme.fg_dim);
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

    let lines = c.body.len() as f32 + 3.0 + button_rows;
    let rect = Rect::from_center_size(full.center(), Vec2::new(width, row_h * lines + 48.0));
    let inner = modal_frame(ui, rect, &app.cfg.theme, &c.title, f, row_h);
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(inner);

    let mut y = inner.top();
    for l in &c.body {
        painter.text(
            egui::pos2(inner.left(), y),
            Align2::LEFT_TOP,
            crate::util::ellipsize_middle(l, (inner.width() / (f.size * 0.6)) as usize),
            f.clone(),
            theme.fg,
        );
        y += row_h;
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
    let resp = ui.put(
        field,
        egui::TextEdit::singleline(&mut p.query)
            .id(egui::Id::new("filer-pick"))
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
pub fn diff(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    use crate::diff::Outcome;

    dim(ui, full);
    let theme = app.cfg.theme.clone();
    let Overlay::Diff(ov) = &mut app.overlay else { return };
    let title = format!(
        "{}  ↔  {} — n/N differences, q to close",
        crate::util::file_name(&ov.left),
        crate::util::file_name(&ov.right),
    );
    let rect = modal_rect(full, 0.92, 0.86);
    let inner = modal_frame(ui, rect, &theme, &title, f, row_h);
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
        Some(Outcome::Rows { rows, truncated, rough }) => (rows, *truncated, *rough),
    };

    // One row is given up to the footer, but never the last one.
    let visible = ((inner.height() / row_h).floor() as usize).max(2) - 1;
    ov.offset = ov.offset.min(rows.len().saturating_sub(1));
    let top = ov.offset.min(rows.len().saturating_sub(visible.min(rows.len())));

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
            if !row.same {
                painter.rect_filled(
                    Rect::from_min_size(egui::pos2(x - 2.0, y), Vec2::new(half, row_h)),
                    CornerRadius::same(2),
                    mark.gamma_multiply(0.22),
                );
            }
            painter.text(
                egui::pos2(x, y),
                Align2::LEFT_TOP,
                format!("{:>4} {}", l.no, crate::util::ellipsize_middle(&l.text, cols)),
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
