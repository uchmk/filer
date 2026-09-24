use egui::{Align2, CornerRadius, FontId, Rect, Stroke, Ui, Vec2};

use super::{dim, modal_frame, modal_rect};
use crate::app::{App, Overlay, TaskState};
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
    let lines = c.body.len() as f32 + 4.0;
    let rect = Rect::from_center_size(
        full.center(),
        Vec2::new((full.width() * 0.6).min(760.0), row_h * lines + 48.0),
    );
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
    for (key, label) in &c.options {
        let text = format!(" [{key}] {label} ");
        let g = painter.layout_no_wrap(text, f.clone(), theme.fg);
        let w = g.size().x + 6.0;
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
