//! The file list column.
//!
//! Rows are painted directly rather than built from widgets: only the visible
//! window is touched, so a directory with 200k entries costs the same as one
//! with twenty.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, Ui, Vec2};

use crate::config::theme::{Style, Theme};
use crate::core::folder::{Folder, LoadState};
use crate::fs::Entry;
use crate::util;

pub struct ListStyle<'a> {
    pub theme: &'a Theme,
    pub font: FontId,
    pub row_h: f32,
    /// Dimmed columns (parent / preview) get a quieter cursor.
    pub active: bool,
    pub linemode: &'a str,
}

pub struct RowFlags {
    pub selected: bool,
    pub yanked: Option<bool>, // Some(true) = cut
    pub git: crate::fs::git::State,
}

pub struct ListResult {
    pub clicked: Option<usize>,
    pub double_clicked: Option<usize>,
    /// Right-click, which opens the context menu on that row.
    pub secondary_clicked: Option<usize>,
    /// The row a drag began on, the frame it began.
    pub drag_started: Option<usize>,
    /// The drag that began here has been let go, wherever the pointer is now.
    pub drag_stopped: bool,
    /// Wheel movement over this pane, in rows; the caller keeps the
    /// remainder, since it is the one that lives between frames.
    pub scroll_rows: f32,
    /// Modifiers held down for the click above.
    pub mods: egui::Modifiers,
}

pub fn draw(
    ui: &mut Ui,
    rect: Rect,
    folder: &Folder,
    st: &ListStyle<'_>,
    flags: &dyn Fn(&Entry) -> RowFlags,
    hits: bool,
) -> ListResult {
    let painter = ui.painter_at(rect);
    let rows = ((rect.height() / st.row_h).floor() as usize).max(1);
    let mut out = ListResult {
        clicked: None,
        double_clicked: None,
        secondary_clicked: None,
        drag_started: None,
        drag_stopped: false,
        scroll_rows: 0.0,
        mods: egui::Modifiers::NONE,
    };

    match &folder.state {
        LoadState::Error(e) => {
            painter.text(
                rect.left_top() + Vec2::new(8.0, 6.0),
                Align2::LEFT_TOP,
                e,
                st.font.clone(),
                st.theme.progress_error,
            );
            return out;
        }
        LoadState::Loading if folder.entries.is_empty() => {
            painter.text(
                rect.left_top() + Vec2::new(8.0, 6.0),
                Align2::LEFT_TOP,
                "…",
                st.font.clone(),
                st.theme.fg_dim,
            );
            return out;
        }
        _ => {}
    }

    if folder.view.is_empty() {
        painter.text(
            rect.left_top() + Vec2::new(8.0, 6.0),
            Align2::LEFT_TOP,
            "(empty)",
            st.font.clone(),
            st.theme.fg_dim,
        );
        return out;
    }

    let start = folder.offset.min(folder.view.len().saturating_sub(1));
    let end = (start + rows).min(folder.view.len());

    for (i, row) in (start..end).enumerate() {
        let Some(entry) = folder.at(row) else { continue };
        let y = rect.top() + i as f32 * st.row_h;
        let row_rect = Rect::from_min_size(
            egui::pos2(rect.left(), y),
            Vec2::new(rect.width(), st.row_h),
        );
        let f = flags(entry);
        let hovered = row == folder.cursor;

        if hovered {
            let bg = if st.active { st.theme.hovered_bg } else { st.theme.inactive_hovered_bg };
            painter.rect_filled(row_rect, CornerRadius::same(3), bg);
        }

        // Marker bar: selection state first, then the yank register.
        let marker = if f.selected {
            Some(st.theme.marker_selected)
        } else {
            f.yanked.map(|cut| if cut { st.theme.marker_cut } else { st.theme.marker_copied })
        };
        if let Some(color) = marker {
            painter.rect_filled(
                Rect::from_min_size(row_rect.left_top() + Vec2::new(1.0, 2.0), Vec2::new(3.0, st.row_h - 4.0)),
                CornerRadius::same(2),
                color,
            );
        }

        let mime = crate::mime::guess(entry);
        let style = st.theme.style_for(entry, mime);
        let base_color = style.fg.unwrap_or(st.theme.fg);
        let icon = st.theme.icon_for(entry);
        let mut x = row_rect.left() + 8.0;

        if !icon.text.trim().is_empty() {
            let g = painter.layout_no_wrap(
                icon.text.clone(),
                st.font.clone(),
                icon.fg.unwrap_or(base_color),
            );
            let w = g.size().x.max(st.font.size);
            painter.galley(
                egui::pos2(x, y + (st.row_h - g.size().y) / 2.0),
                g,
                base_color,
            );
            x += w + 8.0;
        }

        // Right-hand line mode text is laid out first so the name knows its budget.
        let right = linemode_text(entry, st.linemode);
        let mut right_w = 0.0;
        if !right.is_empty() {
            let g = painter.layout_no_wrap(right.clone(), st.font.clone(), st.theme.fg_dim);
            right_w = g.size().x + 10.0;
            painter.galley(
                egui::pos2(row_rect.right() - g.size().x - 6.0, y + (st.row_h - g.size().y) / 2.0),
                g,
                st.theme.fg_dim,
            );
        }

        // The git sign sits between the name and the line mode, so it stays
        // put as the name grows and the two never collide.
        if let Some(mark) = f.git.mark() {
            let color = st.theme.git_color(f.git);
            let g = painter.layout_no_wrap(mark.to_string(), st.font.clone(), color);
            let w = g.size().x;
            painter.galley(
                egui::pos2(row_rect.right() - right_w - w - 2.0, y + (st.row_h - g.size().y) / 2.0),
                g,
                color,
            );
            right_w += w + 8.0;
        }

        let avail = (row_rect.right() - right_w - x - 6.0).max(16.0);
        let mut name = entry.name.clone();
        if let crate::fs::Kind::Link { .. } = entry.kind {
            name.push_str("  ->");
        }
        let positions = if hits { folder.hit_at(row) } else { &[] };
        let job = name_job(&name, positions, &st.font, base_color, &style, st.theme, avail);
        let galley = painter.layout_job(job);
        painter.galley(
            egui::pos2(x, y + (st.row_h - galley.size().y) / 2.0),
            galley,
            base_color,
        );
    }

    // Scrollbar hint when the list overflows.
    if folder.view.len() > rows {
        let track = Rect::from_min_size(
            egui::pos2(rect.right() - 3.0, rect.top()),
            Vec2::new(2.0, rect.height()),
        );
        let frac = rows as f32 / folder.view.len() as f32;
        let h = (track.height() * frac).max(16.0);
        let t = folder.offset as f32 / (folder.view.len() - rows) as f32;
        let y = track.top() + (track.height() - h) * t;
        painter.rect_filled(
            Rect::from_min_size(egui::pos2(track.left(), y), Vec2::new(2.0, h)),
            CornerRadius::same(1),
            st.theme.border,
        );
    }

    // Interaction
    let id = ui.id().with(("list", rect.left() as i32, rect.top() as i32));
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    // `hover_pos` is the fallback: a press that egui reports without an
    // interaction position still names the row the pointer is over.
    if let Some(pos) = resp.interact_pointer_pos().or_else(|| resp.hover_pos()) {
        let row = start + (((pos.y - rect.top()) / st.row_h).floor().max(0.0) as usize);
        if row < end {
            if resp.double_clicked() {
                out.double_clicked = Some(row);
            } else if resp.clicked() {
                out.clicked = Some(row);
            } else if resp.secondary_clicked() {
                out.secondary_clicked = Some(row);
            }
            if resp.drag_started() {
                out.drag_started = Some(row);
            }
            if out.clicked.is_some() || out.double_clicked.is_some() {
                out.mods = ui.ctx().input(|i| i.modifiers);
            }
        }
    }
    // Let go anywhere, not just over the row it started on.
    out.drag_stopped = resp.drag_stopped();
    if ui.rect_contains_pointer(rect) {
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        out.scroll_rows = -scroll / st.row_h * 1.5;
    }
    out
}

fn name_job(
    name: &str,
    positions: &[usize],
    font: &FontId,
    color: Color32,
    style: &Style,
    theme: &Theme,
    max_width: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');

    let fmt = |c: Color32, bg: Color32, underline: bool| TextFormat {
        font_id: font.clone(),
        color: c,
        background: bg,
        italics: style.italic,
        underline: if underline {
            Stroke::new(1.0, c)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    };

    if positions.is_empty() {
        job.append(name, 0.0, fmt(color, Color32::TRANSPARENT, style.underline));
        return job;
    }

    let hl_fg = theme.find_keyword.fg.unwrap_or(color);
    let hl_bg = theme.find_keyword.bg.unwrap_or(Color32::TRANSPARENT);
    let mut buf = String::new();
    let mut buf_hl = false;
    for (i, ch) in name.chars().enumerate() {
        let is_hl = positions.contains(&i);
        if is_hl != buf_hl && !buf.is_empty() {
            let f = if buf_hl {
                fmt(hl_fg, hl_bg, false)
            } else {
                fmt(color, Color32::TRANSPARENT, style.underline)
            };
            job.append(&buf, 0.0, f);
            buf.clear();
        }
        buf_hl = is_hl;
        buf.push(ch);
    }
    if !buf.is_empty() {
        let f = if buf_hl {
            fmt(hl_fg, hl_bg, false)
        } else {
            fmt(color, Color32::TRANSPARENT, style.underline)
        };
        job.append(&buf, 0.0, f);
    }
    job
}

pub fn linemode_text(entry: &Entry, mode: &str) -> String {
    match mode {
        "size" => entry.display_size().unwrap_or_default(),
        "mtime" | "modified" => util::fmt_time(entry.modified, "%Y-%m-%d %H:%M"),
        "btime" | "created" => util::fmt_time(entry.created, "%Y-%m-%d %H:%M"),
        "permissions" => permissions(entry),
        "owner" => String::new(),
        _ => String::new(),
    }
}

pub fn permissions(entry: &Entry) -> String {
    let mut s = String::new();
    s.push(match entry.kind {
        crate::fs::Kind::Dir => 'd',
        crate::fs::Kind::Link { .. } => 'l',
        crate::fs::Kind::File => '-',
    });
    s.push('r');
    s.push(if entry.readonly { '-' } else { 'w' });
    if entry.hidden {
        s.push('h');
    }
    s
}
