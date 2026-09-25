//! Rendering. The whole frame is painted from a snapshot of app state; any
//! interaction turns into an [`Act`] that runs after drawing, so the borrow
//! checker never gets in the way of the layout code.

mod list;
mod overlay;
mod preview;
mod term;

use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Stroke, Ui, Vec2};

use crate::app::{self, App, Overlay, PreviewState};
use crate::preview::Payload;
use crate::config::cmd::{Act, Step};
use crate::config::theme::{Style, Theme};
use crate::core::folder::Folder;
use crate::fs::git;
use crate::util;

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    let mut queued: Vec<Act> = Vec::new();
    let size = app.cfg.ui.font_size;
    let f = font(size);
    let row_h = (size * 1.35 + app.cfg.ui.row_padding).round();

    let full = ui.max_rect();
    ui.painter().rect_filled(full, CornerRadius::ZERO, app.cfg.theme.bg);
    // Rebuilt every frame as the panes are laid out.
    app.pane_rects.clear();

    let header_h = row_h * 2.0 + 8.0;
    let status_h = row_h + 8.0;
    let which_h = if app.which.is_empty() || !app.overlay.is_none() {
        0.0
    } else {
        (row_h * ((app.which.len() as f32 / 3.0).ceil().min(8.0)) + 14.0).min(full.height() * 0.4)
    };
    let input_h = if matches!(app.overlay, Overlay::Input(_)) { row_h + 14.0 } else { 0.0 };

    let header = Rect::from_min_size(full.left_top(), Vec2::new(full.width(), header_h));
    let status = Rect::from_min_size(
        egui::pos2(full.left(), full.bottom() - status_h),
        Vec2::new(full.width(), status_h),
    );
    // The terminal takes the bottom third, and never so much that the list
    // it sits under stops being usable.
    let term_h = match app.term.is_some() {
        true => (full.height() * 0.35).clamp(row_h * 4.0, full.height() - header_h - row_h * 6.0),
        false => 0.0,
    };
    let bottom_extra = which_h + input_h + term_h;
    let body = Rect::from_min_max(
        egui::pos2(full.left(), header.bottom()),
        egui::pos2(full.right(), status.top() - bottom_extra),
    );

    draw_header(app, ui, header, &f, row_h);
    draw_body(app, ui, body, &f, row_h, &mut queued);
    if term_h > 0.0 {
        let r = Rect::from_min_size(
            egui::pos2(full.left(), status.top() - bottom_extra),
            Vec2::new(full.width(), term_h),
        );
        term::draw(app, ui, r, &f, row_h);
    }
    draw_status(app, ui, status, &f);

    // Over the panes, but under the prompts: a `rename` typed with the panel up
    // still has to be readable.
    if app.quick {
        overlay::quick(app, ui, full, &f, row_h, &mut queued);
    }

    if which_h > 0.0 {
        let r = Rect::from_min_size(
            egui::pos2(full.left(), status.top() - which_h),
            Vec2::new(full.width(), which_h),
        );
        overlay::which(app, ui, r, &f, row_h);
    }
    if input_h > 0.0 {
        let r = Rect::from_min_size(
            egui::pos2(full.left(), status.top() - input_h),
            Vec2::new(full.width(), input_h),
        );
        // Above the prompt, since a bulk rename is judged by what it will do
        // rather than by the rule that says it.
        overlay::bulk(app, ui, full, &f, row_h, r.top());
        overlay::input(app, ui, r, &f, &mut queued);
    }

    match &app.overlay {
        Overlay::Help => overlay::help(app, ui, full, &f, row_h),
        Overlay::Tasks(_) => overlay::tasks(app, ui, full, &f, row_h),
        Overlay::Confirm(_) => overlay::confirm(app, ui, full, &f, row_h, &mut queued),
        Overlay::Pick(_) => overlay::pick(app, ui, full, &f, row_h, &mut queued),
        Overlay::Spot(_) => overlay::spot(app, ui, full, &f, row_h),
        Overlay::Diff(_) => overlay::diff(app, ui, full, &f, row_h),
        _ => {}
    }

    draw_drag(app, ui, &f);

    if app.pending_bookmark.is_some() {
        overlay::bookmark_hint(app, ui, full, &f, row_h);
    }
    draw_toasts(app, ui, full, &f, row_h);

    app.run(&queued);
}

// ---------------------------------------------------------------- header

fn draw_header(app: &mut App, ui: &mut Ui, rect: Rect, f: &FontId, row_h: f32) {
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, theme.bg_alt);
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme.border),
    );

    // Tab strip
    let mut x = rect.left() + 8.0;
    let y = rect.top() + 4.0;
    let mut clicked: Option<usize> = None;
    for (i, tab) in app.tabs.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, util::ellipsize_middle(&tab.name(), 18));
        let g = painter.layout_no_wrap(label, f.clone(), theme.fg);
        let w = g.size().x + 8.0;
        let chip = Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, row_h));
        let st = if i == app.active { theme.tab_active } else { theme.tab_inactive };
        if let Some(bg) = st.bg {
            painter.rect_filled(chip, CornerRadius::same(4), bg);
        }
        painter.galley(
            egui::pos2(x + 4.0, y + (row_h - g.size().y) / 2.0),
            g,
            st.fg.unwrap_or(theme.fg),
        );
        let resp = ui.interact(chip, ui.id().with(("tab", i)), egui::Sense::click());
        if resp.clicked() {
            clicked = Some(i);
        }
        x += w + 4.0;
    }
    if let Some(i) = clicked {
        app.act(Act::TabSwitch { n: i as i64, relative: false });
    }

    // Right-hand summary
    let tab = app.tab();
    let total = tab.current.view.len();
    let sel = tab.selected.len();
    let mut right = format!("{} items", total);
    if sel > 0 {
        right = format!("{sel} selected · {right}");
    }
    if tab.show_hidden {
        right.push_str(" · hidden shown");
    }
    painter.text(
        egui::pos2(rect.right() - 10.0, y + row_h / 2.0),
        Align2::RIGHT_CENTER,
        right,
        f.clone(),
        app.cfg.theme.fg_dim,
    );

    // Breadcrumb
    let theme = &app.cfg.theme;
    let y2 = rect.top() + row_h + 6.0;
    let cwd = if app.in_search_view() {
        app.tab().current.path.display().to_string()
    } else {
        app.tab().cwd.display().to_string()
    };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = rect.width() - 20.0;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    job.append(
        &cwd,
        0.0,
        egui::TextFormat {
            font_id: f.clone(),
            color: theme.cwd.fg.unwrap_or(theme.fg),
            ..Default::default()
        },
    );
    if let Some(name) = app.tab().current.hovered_name() {
        let sep = if cwd.ends_with('\\') || cwd.ends_with('/') { "" } else { "\\" };
        job.append(
            &format!("{sep}{name}"),
            0.0,
            egui::TextFormat { font_id: f.clone(), color: theme.fg, ..Default::default() },
        );
    }
    let g = painter.layout_job(job);
    painter.galley(egui::pos2(rect.left() + 10.0, y2), g, theme.fg);
}

// ------------------------------------------------------------------ body

fn draw_body(app: &mut App, ui: &mut Ui, body: Rect, f: &FontId, row_h: f32, queued: &mut Vec<Act>) {
    let mut ratio = {
        let r = &app.cfg.yazi.mgr.ratio;
        [
            r.first().copied().unwrap_or(1),
            r.get(1).copied().unwrap_or(3),
            r.get(2).copied().unwrap_or(4),
        ]
    };
    if app.max_preview {
        ratio[2] = 9999;
    }
    if app.hide_parent || app.in_search_view() {
        ratio[0] = 0;
    }
    if app.split.is_some() {
        // The second pane takes the parent column's place, at the same width
        // as the list it sits next to.
        ratio[0] = ratio[1];
    }
    let gap = 6.0;
    let total: f32 = ratio.iter().map(|&v| v as f32).sum::<f32>().max(1.0);
    let usable = body.width() - gap * 2.0;
    let widths = [
        usable * ratio[0] as f32 / total,
        usable * ratio[1] as f32 / total,
        usable * ratio[2] as f32 / total,
    ];

    let mut x = body.left();
    let rects: Vec<Rect> = widths
        .iter()
        .map(|w| {
            let r = Rect::from_min_size(egui::pos2(x, body.top()), Vec2::new(*w, body.height()));
            x += w + gap;
            r
        })
        .collect();

    let theme = app.cfg.theme.clone();

    // --- the other pane, or the parent directory in its place ---
    let ctx = PaneCtx { theme: &theme, font: f, row_h };
    if let Some(sp) = app.split {
        let left = if sp.right { sp.other } else { app.active };
        if widths[0] > 24.0 {
            draw_pane(app, ui, rects[0], left, &ctx, queued);
        }
        let right = if sp.right { app.active } else { sp.other };
        draw_pane(app, ui, rects[1], right, &ctx, queued);
    } else {
        if widths[0] > 24.0 {
            draw_parent(app, ui, rects[0], &ctx, queued);
        }
        draw_pane(app, ui, rects[1], app.active, &ctx, queued);
    }

    // --- preview ---
    // With the quick-look panel up it owns the preview: it covers this column
    // anyway, and two rects asking the worker for two image sizes every frame
    // would have it rendering the same picture back and forth.
    if widths[2] > 24.0 && !app.quick {
        let rect = rects[2];
        ui.painter().rect_filled(rect, CornerRadius::same(4), theme.bg_alt);
        draw_preview(app, ui, rect, f, row_h, queued);
    }
}

/// Paint whatever the preview worker has for the hovered file into `rect`.
///
/// The side column and the quick-look panel both come through here, so they
/// cannot drift apart, and `preview.box_size` — the size images are rendered
/// at — is set by whichever of them is on screen.
pub(super) fn draw_preview(
    app: &mut App,
    ui: &mut Ui,
    rect: Rect,
    f: &FontId,
    row_h: f32,
    queued: &mut Vec<Act>,
) {
    let theme = app.cfg.theme.clone();
    // A zoomed image asks for a bigger decode, so magnifying shows the picture
    // rather than a blurred copy of the pane-sized one.
    let pane_px = (
        (rect.width() * 2.0).max(64.0) as u32,
        (rect.height() * 2.0).max(64.0) as u32,
    );
    // `image_input` first: only it knows the pane's size, so it is what leaves
    // the fit scale behind for the decode box to be judged against.
    image_input(app, ui, rect);
    app.preview.box_size = app::zoom_box(pane_px, app.preview.zoom, app.preview.fit);
    match &app.preview.state {
        PreviewState::Dir(folder) => {
            let st = list::ListStyle {
                theme: &theme,
                font: f.clone(),
                row_h,
                active: false,
                linemode: "",
            };
            let mut p = clone_view(folder);
            p.offset = app.tabs[app.active].preview_offset.min(p.view.len().saturating_sub(1));
            // Nothing is hovered in a preview, so park the cursor off-list.
            p.cursor = usize::MAX;
            list::draw(ui, rect.shrink(2.0), &p, &st, &|_| list::RowFlags {
                selected: false,
                yanked: None,
                git: git::State::Clean,
            }, false);
        }
        other => {
            // Rendered Markdown is laid out in the worker on a grid of
            // monospace cells, so it needs to know how many fit.
            let cell = ui.painter().layout_no_wrap("M".repeat(20), f.clone(), theme.fg).size().x / 20.0;
            app.preview.cols = ((rect.width() - 24.0) / cell).max(0.0) as u16;
            let st = preview::PreviewStyle {
                theme: &theme,
                font: f.clone(),
                bold: app.bold_font.then(|| FontId::new(f.size, FontFamily::Name("bold".into()))),
                cell,
                row_h,
                wrap: app.cfg.yazi.preview.wrap == "yes",
                render_markdown: app.render_markdown,
                outline_focus: app.preview.outline,
                minimap: app.cfg.ui.minimap,
                zoom: app.preview.zoom,
                pan: app.preview.pan,
            };
            let drawn = preview::draw(
                ui,
                rect.shrink(2.0),
                other,
                app.preview.texture.as_ref(),
                app.tabs[app.active].preview_offset,
                &st,
            );
            if let Some(line) = drawn.scroll_to {
                app.tabs[app.active].preview_offset = line;
            }
            if let Some((k, line)) = drawn.jump {
                app.tabs[app.active].preview_offset = line;
                if app.preview.outline.is_some() {
                    app.preview.outline = Some(k);
                }
            }
            let lines = drawn.lines;
            let rows = ((rect.height() / row_h).floor() as usize).max(1);
            let max = lines.saturating_sub(rows / 2);
            if app.tabs[app.active].preview_offset > max {
                app.tabs[app.active].preview_offset = max;
            }
            if ui.rect_contains_pointer(rect) {
                // Ctrl and the wheel is the image zoom, so it must not scroll
                // the pane with the same turn.
                let (scroll, ctrl) =
                    ui.ctx().input(|i| (i.smooth_scroll_delta.y, i.modifiers.command));
                if scroll.abs() > 0.5 && !ctrl {
                    let delta = -(scroll / row_h * 1.5) as i64;
                    queued.push(Act::Seek(Step::Rel(delta)));
                }
            }
        }
    }
}

/// Mouse handling for the image preview: drag to pan, `Ctrl` and the wheel to
/// zoom about the pointer, double-click back to fit.
///
/// Separate from the painting because it is the only part that writes: the
/// painter sees a scale and an offset and draws them.
fn image_input(app: &mut App, ui: &mut Ui, rect: Rect) {
    let PreviewState::Ready(Payload::Image { source, .. }) = &app.preview.state else {
        return;
    };
    // The picture's own size, not the texture's: a re-decode at a higher
    // resolution must not change how big it looks.
    let (w, h) = (source.0 as f32, source.1 as f32);
    let avail = rect.shrink(8.0);
    // The pane is the only one that knows how big it is, so it leaves the fit
    // scale behind for `zoom in` to start from.
    app.preview.fit = app::image_fit(avail.size(), w, h);

    let resp = ui.interact(avail, ui.id().with("image"), egui::Sense::click_and_drag());
    let fit = app.preview.fit;
    if resp.dragged() {
        // Dragging means a zoom: fitting the pane has nothing to pan.
        app.preview.zoom.get_or_insert(fit);
        app.preview.pan += resp.drag_delta();
    }
    if let Some(p) = resp.hover_pos() {
        let (scroll, ctrl) = ui.ctx().input(|i| (i.smooth_scroll_delta.y, i.modifiers.command));
        // A plain wheel keeps scrolling the pane, as it does over text; `Ctrl`
        // is the zoom, the way it is everywhere else.
        if ctrl && scroll.abs() > 0.5 {
            let zoom = *app.preview.zoom.get_or_insert(fit);
            let (next, pan) =
                app::zoom_at(zoom, app.preview.pan, avail.center(), p, 1.0 + scroll * 0.004);
            app.preview.zoom = Some(next);
            app.preview.pan = pan;
        }
    }
    if resp.double_clicked() {
        // The quickest way back out of being lost inside a photograph.
        app.preview.zoom = None;
        app.preview.pan = Vec2::ZERO;
    }
    if resp.hovered() && app.preview.zoom.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let shown = Vec2::new(w, h) * app.preview.zoom.unwrap_or(app.preview.fit);
    app.preview.pan = app::clamp_pan(app.preview.pan, shown, avail.size());
}

/// What every file-list column needs from the frame.
struct PaneCtx<'a> {
    theme: &'a Theme,
    font: &'a FontId,
    row_h: f32,
}

/// The read-only column showing the directory above the current one.
fn draw_parent(app: &mut App, ui: &mut Ui, rect: Rect, ctx: &PaneCtx, queued: &mut Vec<Act>) {
    let PaneCtx { theme, font: f, row_h } = *ctx;
    if let Some(parent) = &app.tabs[app.active].parent {
        let st = list::ListStyle {
            theme,
            font: f.clone(),
            row_h,
            active: false,
            linemode: "",
        };
        let mut p = clone_view(parent);
        let rows = ((rect.height() / row_h).floor() as usize).max(1);
        p.clamp_offset(rows, app.cfg.yazi.mgr.scrolloff as usize);
        let res = list::draw(ui, rect, &p, &st, &|_e| list::RowFlags {
            selected: false,
            yanked: None,
            git: git::State::Clean,
        }, false);
        if let Some(row) = res.clicked.or(res.double_clicked) {
            if let Some(e) = p.at(row) {
                if e.is_dir_like() {
                    queued.push(Act::Cd { target: e.path.display().to_string(), interactive: false });
                }
            }
        }
    }
}

/// One file list: the middle column, or one side of the split. The pane with
/// the keys is the one showing `app.active`; a click on the other pane takes
/// the keys first, so the gesture still lands on the focused tab.
fn draw_pane(
    app: &mut App,
    ui: &mut Ui,
    rect: Rect,
    idx: usize,
    ctx: &PaneCtx,
    queued: &mut Vec<Act>,
) {
    let PaneCtx { theme, font: f, row_h } = *ctx;
    let focused = idx == app.active;
    let rows = ((rect.height() / row_h).floor() as usize).max(1);
    app.set_page_rows(idx, rows);
    let scrolloff = app.cfg.yazi.mgr.scrolloff as usize;
    app.tabs[idx].current.clamp_offset(rows, scrolloff);

    let selected: std::collections::BTreeSet<std::path::PathBuf> = app.tabs[idx].selected.clone();
    let yank_paths = app.yank.paths.clone();
    let yank_cut = app.yank.cut;
    let linemode = app.tabs[idx].linemode.clone();
    let st = list::ListStyle {
        theme,
        font: f.clone(),
        row_h,
        // The outline has the keys while it is focused; dim the cursor. The
        // pane without the keys is dimmed for the same reason.
        active: focused && app.preview.outline.is_none(),
        linemode: &linemode,
    };
    // Where this pane is, so a drop let go anywhere can find its target.
    app.pane_rects.push((idx, rect));
    let has_filter = app.tabs[idx].current.filter.is_some();
    let git = app.git_status(&app.tabs[idx].cwd);
    let res = list::draw(
        ui,
        rect,
        &app.tabs[idx].current,
        &st,
        &|e| list::RowFlags {
            selected: selected.contains(&e.path),
            yanked: if yank_paths.contains(&e.path) { Some(yank_cut) } else { None },
            git: git.as_ref().map(|g| g.get(&e.name)).unwrap_or(git::State::Clean),
        },
        has_filter,
    );
    // Which side has the keys should be clear at a glance.
    if app.split.is_some() {
        let color = if focused { theme.cwd.fg.unwrap_or(theme.fg) } else { theme.border };
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
    }
    if res.scrolled != 0 {
        app.tabs[idx].current.scroll(res.scrolled, rows);
        app.tabs[idx].sync_visual();
    }
    // Shift / Ctrl (Cmd on macOS) turn a click into a selection gesture, so
    // it never counts as a double-click to open with.
    let multi = res.mods.shift || res.mods.command;
    // Clicking the list takes the keys back from the outline.
    if let Some(row) = res.clicked.or(if multi { res.double_clicked } else { None }) {
        app.focus_pane(idx);
        app.preview.outline = None;
        let tab = &mut app.tabs[idx];
        if res.mods.shift {
            tab.shift_click(row);
        } else if res.mods.command {
            tab.ctrl_click(row);
        } else {
            tab.click(row);
        }
    }
    if let Some(row) = res.double_clicked.filter(|_| !multi) {
        app.focus_pane(idx);
        app.preview.outline = None;
        app.tabs[idx].current.cursor = row;
        // `enter` on a file moves into its outline; a double-click opens.
        queued.push(Act::Open { interactive: false, hovered: true });
    }
    // Right-click asks what can be done with the row it landed on, so the pane
    // and the cursor move there first.
    if let Some(row) = res.secondary_clicked {
        app.focus_pane(idx);
        app.preview.outline = None;
        app.tabs[idx].right_click(row);
        queued.push(Act::Menu);
    }

    // Dragging to the other pane. A drag that starts on a row with the plain
    // pointer is a drag of files; with Shift or Ctrl held the click is a
    // selection gesture and stays one.
    if let Some(row) = res.drag_started.filter(|_| !multi) {
        app.start_drag(idx, row);
    }
    if res.drag_stopped && app.drag.is_some() {
        let pos = ui.ctx().input(|i| i.pointer.interact_pos());
        let onto = pos.and_then(|p| {
            app.pane_rects.iter().find(|(_, r)| r.contains(p)).map(|(i, _)| *i)
        });
        // Shift is the move modifier, as it is in Explorer; a plain drag copies.
        let cut = ui.ctx().input(|i| i.modifiers.shift);
        app.drop_drag(onto, cut);
    }
}

/// What a drag in flight looks like: the pane it would land in outlined, and
/// what is being carried named under the pointer.
fn draw_drag(app: &App, ui: &mut Ui, f: &FontId) {
    let Some(drag) = &app.drag else { return };
    let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) else { return };
    let theme = &app.cfg.theme;
    let accent = theme.cwd.fg.unwrap_or(theme.fg);
    let painter = ui.painter();

    let over = app.pane_rects.iter().find(|(i, r)| *i != drag.from && r.contains(pos));
    if let Some((_, r)) = over {
        painter.rect_stroke(
            *r,
            CornerRadius::same(4),
            Stroke::new(2.0, accent),
            egui::StrokeKind::Inside,
        );
    }
    // Shift means move, so say which one is about to happen.
    let verb = match ui.ctx().input(|i| i.modifiers.shift) {
        true => "move",
        false => "copy",
    };
    let text = match over.is_some() {
        true => format!("{verb} {}", drag.label),
        // Nowhere to land yet: name the files without promising anything.
        false => drag.label.clone(),
    };
    let g = painter.layout_no_wrap(text, f.clone(), theme.fg);
    let at = pos + Vec2::new(12.0, 8.0);
    let bg = Rect::from_min_size(at, g.size()).expand(4.0);
    painter.rect_filled(bg, CornerRadius::same(3), theme.bg_alt);
    painter.rect_stroke(
        bg,
        CornerRadius::same(3),
        Stroke::new(1.0, if over.is_some() { accent } else { theme.border }),
        egui::StrokeKind::Inside,
    );
    painter.galley(at, g, theme.fg);
}

/// The renderer needs its own cursor/offset for the read-only columns.
fn clone_view(f: &Folder) -> Folder {
    Folder {
        path: f.path.clone(),
        entries: f.entries.clone(),
        view: f.view.clone(),
        hits: f.hits.clone(),
        cursor: f.cursor,
        offset: f.offset,
        state: f.state.clone(),
        scan_id: f.scan_id,
        filter: f.filter.clone(),
    }
}

// ---------------------------------------------------------------- status

fn draw_status(app: &mut App, ui: &mut Ui, rect: Rect, f: &FontId) {
    let theme = &app.cfg.theme;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, theme.status_bg);
    painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, theme.border));

    let tab = &app.tabs[app.active];
    let (mode_label, mode_style): (&str, Style) = match &tab.visual {
        Some(v) if v.unset => ("UNSET", theme.mode_unset),
        Some(_) => ("SELECT", theme.mode_select),
        None => ("NORMAL", theme.mode_normal),
    };

    let mut x = rect.left() + 8.0;
    let cy = rect.center().y;
    let label = format!(" {mode_label} ");
    let g = painter.layout_no_wrap(label, f.clone(), mode_style.fg.unwrap_or(theme.fg));
    let chip = Rect::from_min_size(
        egui::pos2(x, cy - g.size().y / 2.0 - 2.0),
        Vec2::new(g.size().x, g.size().y + 4.0),
    );
    if let Some(bg) = mode_style.bg {
        painter.rect_filled(chip, CornerRadius::same(3), bg);
    }
    painter.galley(egui::pos2(x, cy - g.size().y / 2.0), g, theme.fg);
    x += chip.width() + 10.0;

    let mut left = String::new();
    if let Some(e) = tab.current.hovered() {
        left.push_str(&list::permissions(e));
        left.push_str("  ");
        if !e.is_dir_like() {
            left.push_str(&util::human_size(e.len));
            left.push_str("  ");
        }
        left.push_str(&util::fmt_time(e.modified, "%Y-%m-%d %H:%M"));
    }
    painter.text(egui::pos2(x, cy), Align2::LEFT_CENTER, left, f.clone(), theme.fg_dim);

    // Right side: task progress, filter/find state, position.
    let mut right: Vec<String> = Vec::new();
    if let Some(t) = running_task(app) {
        let mut s = format!("{} {:>3.0}%", t.kind.verb(), t.fraction() * 100.0);
        if t.state == crate::app::TaskState::Paused {
            s.push_str(" paused");
        }
        if let Some(b) = t.speed() {
            s.push_str(&format!("  {}/s", util::human_size(b)));
        }
        if let Some(eta) = t.eta() {
            s.push_str(&format!("  {}", util::fmt_duration(eta)));
        }
        // What else is waiting, so a queue is never a surprise.
        let waiting = app
            .tasks
            .iter()
            .filter(|o| o.id != t.id && o.state.is_live())
            .count();
        if waiting > 0 {
            s.push_str(&format!("  +{waiting}"));
        }
        right.push(s);
    }
    if let Some(h) = &app.search {
        right.push(match h.via {
            crate::config::cmd::SearchVia::Content => format!("grepping {}…", h.query),
            crate::config::cmd::SearchVia::Name => format!("searching {}…", h.query),
        });
    }
    // The branch, when the directory is in a repository at all.
    if let Some(g) = app.git_status(&tab.cwd) {
        if !g.branch.is_empty() {
            right.push(format!(" {} ", g.branch));
        }
    }
    if let Some(fl) = &tab.current.filter {
        if !fl.query.is_empty() {
            right.push(format!("filter: {}", fl.query));
        }
    }
    if let Some(fd) = &tab.finder {
        if !fd.query.is_empty() {
            right.push(format!("find: {}", fd.query));
        }
    }
    if !app.yank.paths.is_empty() {
        right.push(format!(
            "{} {}",
            if app.yank.cut { "cut" } else { "yank" },
            app.yank.paths.len()
        ));
    }
    let pos = if tab.current.view.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", tab.current.cursor + 1, tab.current.view.len())
    };
    right.push(pos);
    painter.text(
        egui::pos2(rect.right() - 10.0, cy),
        Align2::RIGHT_CENTER,
        right.join("   "),
        f.clone(),
        theme.fg_dim,
    );

    // A thin progress strip along the bottom while work is running.
    if let Some(t) = running_task(app) {
        let w = rect.width() * t.fraction();
        let color = match t.state {
            crate::app::TaskState::Paused => theme.fg_dim,
            _ => theme.progress_fg,
        };
        painter.rect_filled(
            Rect::from_min_size(rect.left_bottom() - Vec2::new(0.0, 2.0), Vec2::new(w, 2.0)),
            CornerRadius::ZERO,
            color,
        );
    }
}

/// The job the status bar speaks for: the one being worked on, or the one
/// parked mid-way, which is worth saying more than a queue of jobs that have
/// not begun.
fn running_task(app: &App) -> Option<&crate::app::Task> {
    use crate::app::TaskState;
    app.tasks
        .iter()
        .find(|t| t.state == TaskState::Running)
        .or_else(|| app.tasks.iter().find(|t| t.state == TaskState::Paused))
}

// ---------------------------------------------------------------- toasts

fn draw_toasts(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    if app.toasts.is_empty() {
        return;
    }
    let theme = &app.cfg.theme;
    let painter = ui.painter();
    let mut y = full.top() + 8.0;
    for t in app.toasts.iter().rev().take(5) {
        let color = if t.error { theme.progress_error } else { theme.fg };
        let g = painter.layout_no_wrap(t.text.clone(), f.clone(), color);
        let w = g.size().x + 20.0;
        let r = Rect::from_min_size(
            egui::pos2(full.right() - w - 12.0, y),
            Vec2::new(w, row_h + 8.0),
        );
        painter.rect_filled(r, CornerRadius::same(4), theme.bg_alt);
        painter.rect_stroke(
            r,
            CornerRadius::same(4),
            Stroke::new(1.0, if t.error { theme.progress_error } else { theme.border }),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            egui::pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0),
            g,
            color,
        );
        y += r.height() + 6.0;
    }
}

pub fn modal_rect(full: Rect, w_frac: f32, h_frac: f32) -> Rect {
    let w = (full.width() * w_frac).min(full.width() - 40.0);
    let h = (full.height() * h_frac).min(full.height() - 40.0);
    Rect::from_center_size(full.center(), Vec2::new(w, h))
}

pub fn modal_frame(ui: &Ui, rect: Rect, theme: &Theme, title: &str, f: &FontId, row_h: f32) -> Rect {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(6), theme.bg_alt);
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0, theme.border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.left_top() + Vec2::new(14.0, 10.0),
        Align2::LEFT_TOP,
        title,
        f.clone(),
        theme.cwd.fg.unwrap_or(theme.fg),
    );
    Rect::from_min_max(
        rect.left_top() + Vec2::new(14.0, 12.0 + row_h),
        rect.right_bottom() - Vec2::new(14.0, 12.0),
    )
}

pub fn dim(ui: &Ui, full: Rect) {
    ui.painter().rect_filled(full, CornerRadius::ZERO, Color32::from_black_alpha(140));
}
