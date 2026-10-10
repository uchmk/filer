//! Rendering. The whole frame is painted from a snapshot of app state; any
//! interaction turns into an [`Act`] that runs after drawing, so the borrow
//! checker never gets in the way of the layout code.

mod list;
pub(crate) mod overlay;
pub(crate) mod preview;
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

/// The colour a pane's separating rule takes, accented while that pane holds
/// the keys.
///
/// Defined once because the two panes that have such a rule -- the outline and
/// the terminal -- have to agree. The terminal's went plain in v0.20.2, on the
/// grounds that its own cursor already says the same thing (filled when
/// focused, hollow when not). That reads well on its own and badly beside the
/// outline, which never stopped accenting: with both open, one pane answers
/// "are the keys here" in colour and the other in a detail a few hundred pixels
/// away, and the terminal looks like the one that is broken. A cell cursor is
/// also easy to miss on a shell prompt that is already blinking something.
pub fn focus_rule(theme: &Theme, focused: bool) -> Color32 {
    match focused {
        true => theme.tab_active.bg.unwrap_or(theme.fg),
        false => theme.border,
    }
}

/// The terminal pane's share of the window's height until the border above it
/// is dragged (Q94).
pub const TERM_SHARE: f32 = 0.35;

/// The terminal pane's height in a window `full_h` tall: `share` of it, but at
/// least four rows, and never so much that the list above keeps fewer than six
/// (`chrome` is the header and the status bar). On a window too short for
/// both, the four rows win.
fn term_height(full_h: f32, chrome: f32, row_h: f32, share: f32) -> f32 {
    let least = row_h * 4.0;
    (full_h * share).clamp(least, (full_h - chrome - row_h * 6.0).max(least))
}

/// The border between the list and the terminal pane, dragged to change the
/// pane's height and double-clicked to halve the two (Q94). It is
/// `tsumugi-layout`'s divider, the one tsumugi's panes have, over a split of
/// two: the list and the terminal. Drawn after the terminal so it takes the
/// pointer from the pane's top rows. The height goes in `app.term_share` and is
/// not saved; a share rather than pixels, so it follows the window's size.
fn term_border(app: &mut App, ui: &Ui, body: Rect, term_h: f32, full_h: f32) {
    use tsumugi_layout::{Dir, Node, ui as lay};
    let span = Rect::from_min_max(body.left_top(), egui::pos2(body.right(), body.bottom() + term_h));
    if span.height() < 1.0 || full_h < 1.0 {
        return;
    }
    let layout = Node::Split {
        dir: Dir::Down,
        ratio: body.height().max(0.0) / span.height(),
        first: Box::new(Node::Leaf(0u8)),
        second: Box::new(Node::Leaf(1u8)),
    };
    let look = lay::Look { gap: 0.0, grab: 8.0, line: focus_rule(&app.cfg.theme, true) };
    let moved = lay::dividers(ui, ui.id().with("term-border"), &layout, lay::from_egui(span), look);
    if let Some(lay::Moved::Dragging(Node::Split { ratio, .. }) | lay::Moved::Halved(Node::Split { ratio, .. })) = moved {
        app.term_share = term_share(span.height(), ratio, full_h);
    }
}

/// The share of a window `full_h` tall that the terminal pane takes when the
/// split of `span` above the status bar gives the list `ratio` of it.
fn term_share(span: f32, ratio: f32, full_h: f32) -> f32 {
    (span * (1.0 - ratio) / full_h).clamp(0.05, 0.95)
}

/// Turn wheel movement, measured in rows, into whole rows, keeping the part
/// that is not yet one. In `tsumugi-pane` with the terminal pane, which uses it
/// too; its tests are there.
pub use tsumugi_pane::wheel_whole;

/// The listing's right-hand summary.
///
/// The yank register sits next to the selection and says which of the two it
/// is, because otherwise the states are told apart only by the colour of a 3px
/// bar -- and not even that while a file is both, since the selection's colour
/// wins there and the yank goes invisible under it. The register also carries
/// across directories, which is where it matters most: what `p` would paste
/// here is a fact about the register, not about anything on screen.
/// The tab's folder has not been listed yet -- a jump still waiting on the
/// first answer, typically from a host that may never give one.
fn waiting(tab: &crate::core::tab::Tab) -> bool {
    tab.current.state == crate::core::folder::LoadState::Loading && tab.current.entries.is_empty()
}

fn summary(total: usize, selected: usize, yank: Option<(usize, bool)>, hidden: bool, walking: bool, usage: Option<u64>) -> String {
    // While a usage walk runs the rows are the children measured so far, not
    // what the folder holds (#114), so the count says it is still growing.
    let mut out = if walking { format!("{total} measured so far") } else { crate::util::items(total) };
    // Once it is done, the total stays in the header: the toast that said it
    // is gone in six seconds, and with it the only sign this is the usage view
    // rather than the folder (#122).
    if let Some(bytes) = usage.filter(|_| !walking) {
        out.push_str(&format!(" · {} total", crate::util::human_size(bytes)));
    }
    if let Some((n, cut)) = yank {
        out = format!("{n} {} · {out}", if cut { "cut" } else { "copied" });
    }
    if selected > 0 {
        out = format!("{selected} selected · {out}");
    }
    if hidden {
        out.push_str(" · hidden shown");
    }
    out
}

/// egui's dark look, with a caret that does not blink (Q38). A blinking one
/// redraws the window twice a second for as long as a prompt is open, which
/// was the 0.14-0.30 CPU-s per 10 s #103 and #110 measured with `f` left open
/// and nothing touched; egui already confines it to the toggles and stops it
/// in a background window, so not blinking was the only way to zero.
pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.text_cursor.blink = false;
    v
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
    app.preview.rect = None;

    let header_h = header_height(row_h);
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
    // The terminal takes the bottom third (or what the border above it was
    // dragged to), and never so much that the list it
    // sits under stops being usable -- until `term_max`, where the point is to
    // hand the pane the window. A third is plenty for a shell and cramped for a
    // full-screen program: gh-dash drew its own split panes inside 35% of the
    // height and there was nowhere to put them.
    //
    // Maximised, it goes all the way to the top: the header and the list are
    // not drawn at all, rather than squeezed to a sliver. A sliver is worse than
    // nothing -- it costs the pane rows and shows too little to read.
    //
    // The status bar stays. It is one row, and it is what says filer is still
    // here rather than that a terminal has taken the window.
    let maxed = app.term.is_some() && app.max_term;
    let term_h = match (app.term.is_some(), maxed) {
        (_, true) => full.height() - status_h,
        (true, false) => term_height(full.height(), header_h + status_h, row_h, app.term_share),
        (false, _) => 0.0,
    };
    let bottom_extra = which_h + input_h + term_h;
    let body = Rect::from_min_max(
        egui::pos2(full.left(), header.bottom()),
        egui::pos2(full.right(), status.top() - bottom_extra),
    );

    // Skipped rather than drawn into an inverted rectangle: `body` is built
    // downwards from the header, so at this height its bottom is above its top.
    if !maxed {
        draw_header(app, ui, header, &f, row_h);
        draw_body(app, ui, body, &f, row_h, &mut queued);
    }
    if term_h > 0.0 {
        let r = Rect::from_min_size(
            egui::pos2(full.left(), status.top() - bottom_extra),
            Vec2::new(full.width(), term_h),
        );
        term::draw(app, ui, r, &f, row_h);
        if !maxed {
            term_border(app, ui, body, term_h, full.height());
        }
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
        overlay::shell_hint(app, ui, full, &f, row_h, r.top());
        overlay::name_hint(app, ui, full, &f, row_h, r.top());
        overlay::input(app, ui, r, &f, &mut queued);
    }

    match &app.overlay {
        Overlay::Help => overlay::help(app, ui, full, &f, row_h, &mut queued),
        Overlay::Tasks(_) => overlay::tasks(app, ui, full, &f, row_h),
        Overlay::Confirm(_) => overlay::confirm(app, ui, full, &f, row_h, &mut queued),
        Overlay::Pick(_) => overlay::pick(app, ui, full, &f, row_h, &mut queued),
        Overlay::Spot(_) => overlay::spot(app, ui, full, &f, row_h),
        Overlay::Diff(_) => overlay::diff(app, ui, full, &f, row_h),
        _ => {}
    }

    drop_drag_here(app, ui);
    draw_drag(app, ui, &f);

    if app.pending_bookmark.is_some() {
        overlay::bookmark_hint(app, ui, full, &f, row_h);
    }
    draw_toasts(app, ui, full, &f, row_h);

    app.run(&queued);
}

// ---------------------------------------------------------------- header

/// The two halves of the path line: the directory, dimmed, and the file under
/// the cursor.
///
/// `found` is the hovered row's own path, and is only passed in a search view.
/// There the rows come from anywhere under the root the search started from, so
/// joining that root to the file name — which is what this used to do — spells
/// out a path that usually does not exist, and reads as though a match three
/// directories down were sitting in the top one.
fn breadcrumb(
    found: Option<&std::path::Path>,
    dir: &std::path::Path,
    hovered_name: Option<&str>,
) -> (String, Option<String>) {
    match found {
        Some(p) => (
            p.parent().map_or_else(String::new, |d| d.display().to_string()),
            p.file_name().map(|n| n.to_string_lossy().into_owned()),
        ),
        None => (dir.display().to_string(), hovered_name.map(str::to_owned)),
    }
}

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
    let yank = (!app.yank.paths.is_empty()).then_some((app.yank.paths.len(), app.yank.cut));
    let usage = app.in_usage_view().then(|| tab.current.entries.iter().map(|e| e.usage_bytes()).sum());
    // Nothing has been listed yet: a count would be a claim about a folder
    // that has not answered, and `0 items` on a host still being dialled read
    // as having arrived at an empty one (#105).
    let right = match waiting(tab) {
        true => "listing…".to_owned(),
        false => summary(total, sel, yank, tab.show_hidden, app.usage.is_some(), usage),
    };
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
    // A search view's rows come from anywhere under the root it was started
    // from, so the row's own path is the only one that is true. Joining the
    // root to the file name — which is what this did — spells out a path that
    // usually does not exist, and reads as if the match were in the top
    // directory when it was three levels down.
    let found = app
        .in_search_view()
        .then(|| app.tab().current.hovered().map(|e| e.path.clone()))
        .flatten();
    let (cwd, name) = breadcrumb(
        found.as_deref(),
        if app.in_search_view() { &app.tab().current.path } else { &app.tab().cwd },
        app.tab().current.hovered_name(),
    );
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
    if let Some(name) = &name {
        // The platform's own separator. This was a literal `\` until v0.45.0,
        // which reads as a path on Windows and as an escape everywhere else:
        // the header on Linux said `/home/you\notes.md`. The test for it is in
        // `whole_frame`, which is how it turned up -- nothing had drawn a
        // header off Windows before.
        let sep = if cwd.is_empty() || cwd.ends_with('\\') || cwd.ends_with('/') {
            String::new()
        } else {
            std::path::MAIN_SEPARATOR.to_string()
        };
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
    // Filled again below only if the parent column is drawn this frame.
    app.parent_shown.clear();
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
            draw_pane(app, ui, rects[0], 0, left, &ctx, queued);
        }
        let right = if sp.right { app.active } else { sp.other };
        draw_pane(app, ui, rects[1], 1, right, &ctx, queued);
    } else {
        if widths[0] > 24.0 {
            draw_parent(app, ui, rects[0], &ctx, queued);
        }
        draw_pane(app, ui, rects[1], 1, app.active, &ctx, queued);
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
    app.preview.rect = Some(rect);
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
                linemode: crate::fs::entry::Linemode::None,
                usage_max: 0,
            };
            let mut p = clone_view(folder);
            // A directory preview scrolls with the same `preview_offset`, so it
            // owes `seek` the same ceiling. Without it, `max_offset` would keep
            // whatever the last file left behind and `<A-j>` would run off the
            // end of a short listing.
            let dir_max = p.view.len().saturating_sub(1);
            p.offset = app.tabs[app.active].preview_offset.min(dir_max);
            // Nothing is hovered in a preview, so park the cursor off-list.
            p.cursor = usize::MAX;
            list::draw(ui, rect.shrink(2.0), &p, &st, &|_| list::RowFlags {
                selected: false,
                yanked: None,
                git: git::State::Clean,
            }, false);
            app.preview.max_offset = dir_max;
            if app.tabs[app.active].preview_offset > dir_max {
                app.tabs[app.active].preview_offset = dir_max;
            }
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
                rect.shrink(PREVIEW_INSET),
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
            // What `seek` clamps against next time. The correction below stays
            // as well: the content can shrink under a stationary offset — a
            // narrower window re-wraps Markdown to more lines, a rescan brings
            // a shorter file — and nothing has asked to scroll when it does.
            let lines = drawn.lines;
            let rows = ((rect.height() / row_h).floor() as usize).max(1);
            let max = lines.saturating_sub(rows / 2);
            app.preview.max_offset = max;
            if app.tabs[app.active].preview_offset > max {
                app.tabs[app.active].preview_offset = max;
            }
            // A panel over the panes owns the wheel; see `Overlay::is_modal`.
            if ui.rect_contains_pointer(rect) && !app.overlay.is_modal() {
                // Ctrl and the wheel is the image zoom, so it must not scroll
                // the pane with the same turn.
                let (scroll, ctrl) =
                    ui.ctx().input(|i| (i.smooth_scroll_delta.y, i.modifiers.command));
                if !ctrl {
                    let rows = -scroll / row_h * 1.5;
                    let delta = wheel_whole(&mut app.preview_scroll_rows, rows);
                    if delta != 0 {
                        queued.push(Act::Seek(Step::Rel(delta)));
                    }
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
/// How far the preview's contents sit inside its column.
const PREVIEW_INSET: f32 = 2.0;

fn image_input(app: &mut App, ui: &mut Ui, rect: Rect) {
    let PreviewState::Ready(Payload::Image { source, .. }) = &app.preview.state else {
        return;
    };
    // The picture's own size, not the texture's: a re-decode at a higher
    // resolution must not change how big it looks.
    let (w, h) = (source.0 as f32, source.1 as f32);
    // The very rectangle the picture is drawn and clipped to: the pan was
    // clamped against one 2 px wider a side, so a picture pushed into a
    // corner stopped with its edge cut off (#218).
    let avail = preview::image_area(rect.shrink(PREVIEW_INSET));
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
        // A plain wheel keeps scrolling the pane, as it does over text; `Ctrl`
        // is the zoom, the way it is everywhere else. egui turns a turn with
        // `Ctrl` held into a zoom factor and leaves the scroll at zero, so the
        // factor is what to read: reading the scroll with `Ctrl` held, as this
        // did, never saw a turn at all (#202, 19.6). A pinch on a touchpad
        // arrives the same way.
        let factor = ui.ctx().input(|i| i.zoom_delta());
        if (factor - 1.0).abs() > 1e-4 {
            let zoom = *app.preview.zoom.get_or_insert(fit);
            let (next, pan) = app::zoom_at(zoom, app.preview.pan, avail.center(), p, factor);
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

/// What a click in the parent column asks for.
///
/// A directory is somewhere to go. A file used to be nothing at all: the
/// branch tested `is_dir_like` and simply had no else, so half the rows in a
/// column that draws them identically answered the mouse and half ignored it,
/// with nothing to say which was which. Going up to where the file lives and
/// putting the cursor on it is the only thing a click there can reasonably
/// mean -- and it is what the same click already does from the help panel's
/// list of config files.
fn parent_click(e: &crate::fs::Entry) -> Act {
    let target = e.path.display().to_string();
    match e.is_dir_like() {
        true => Act::Cd { target, interactive: false },
        false => Act::Reveal(target),
    }
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
            linemode: crate::fs::entry::Linemode::None,
            usage_max: 0,
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
                queued.push(parent_click(e));
            }
        }
        app.parent_shown = res.shown;
    }
}

/// One file list: the middle column, or one side of the split. The pane with
/// the keys is the one showing `app.active`; a click on the other pane takes
/// the keys first, so the gesture still lands on the focused tab. `place` is
/// which column it is drawn in (0 the left, 1 the middle), for the state that
/// belongs to the place rather than to the tab shown there.
fn draw_pane(
    app: &mut App,
    ui: &mut Ui,
    rect: Rect,
    place: usize,
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
    let linemode = app.tabs[idx].linemode;
    let st = list::ListStyle {
        theme,
        font: f.clone(),
        row_h,
        // The outline has the keys while it is focused; dim the cursor. The
        // pane without the keys is dimmed for the same reason.
        active: focused && app.preview.outline.is_none(),
        linemode,
        usage_max: app.usage_max,
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
    // A panel over the list owns the wheel; see `Overlay::is_modal`.
    let scrolled = match app.overlay.is_modal() {
        true => 0,
        false => wheel_whole(&mut app.list_scroll_rows[place], res.scroll_rows),
    };
    if scrolled != 0 {
        let scrolloff = app.cfg.yazi.mgr.scrolloff as usize;
        app.tabs[idx].current.scroll(scrolled, rows, scrolloff);
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
        // Shift is the move modifier, as it is in Explorer; a plain drag copies.
        // Where it landed is worked out in `drop_drag_here`, after both panes
        // are drawn: from here, the left pane -- drawn first -- knew only its
        // own place, so a drop on the right one landed nowhere (#208).
        // The release's own modifiers, so Shift is read as it was when the
        // button came up; the frame's are the fallback.
        let shift = ui.ctx().input(|i| {
            i.events
                .iter()
                .find_map(|e| match e {
                    egui::Event::PointerButton { pressed: false, modifiers, .. } => Some(modifiers.shift),
                    _ => None,
                })
                .unwrap_or(i.modifiers.shift)
        });
        app.drag_released = Some(shift);
    }
}

/// Place a drag let go this frame, now that every pane has said where it is.
fn drop_drag_here(app: &mut App, ui: &Ui) {
    let Some(cut) = app.drag_released.take() else { return };
    let pos = ui.ctx().input(|i| i.pointer.interact_pos());
    let onto = pos.and_then(|p| app.pane_rects.iter().find(|(_, r)| r.contains(p)).map(|(i, _)| *i));
    app.drop_drag(onto, cut);
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
            crate::config::cmd::SearchVia::Fuzzy => format!("fuzzy searching {}…", h.query),
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
            app.yank.paths.len(),
            if app.yank.cut { "cut" } else { "copied" },
        ));
    }
    let pos = if waiting(tab) {
        "…".to_string()
    } else if tab.current.view.is_empty() {
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

/// How many lines of one message are shown, so that several long ones at
/// once do not take the window. A config warning no longer reaches it: since
/// Q84 its toast is the first line, and the rest is in `~`.
const TOAST_LINES: usize = 8;

/// The first `max` lines, with a marker when there were more.
fn clip_lines(s: &str, max: usize) -> String {
    match s.lines().nth(max) {
        None => s.to_owned(),
        Some(_) => s.lines().take(max).chain(["…"]).collect::<Vec<_>>().join("\n"),
    }
}

/// The tab strip and the breadcrumb, one row each.
fn header_height(row_h: f32) -> f32 {
    row_h * 2.0 + 8.0
}

fn draw_toasts(app: &mut App, ui: &mut Ui, full: Rect, f: &FontId, row_h: f32) {
    if app.toasts.is_empty() {
        return;
    }
    let theme = &app.cfg.theme;
    let painter = ui.painter();
    // Below the header, not on top of it. The right-hand end of the header
    // carries the item count and, when there is one, "N selected" — and a
    // selection left over from an earlier command is exactly what makes a
    // message like "none of the 2 selected item(s) is an archive" worth
    // reading. Covering the answer with the question is a poor trade. Below
    // the whole header: one row down cleared the tab strip and landed on the
    // breadcrumb, cutting a long path in half (#191).
    let mut y = full.top() + header_height(row_h) + 6.0;
    // As wide as the message needs, up to half the window, and wrapped after
    // that. `layout_no_wrap` was fine while every message was one short line
    // and wrong the moment one was not: a config error carries the offending
    // line and a row of carets under it, so it ran off both edges at once.
    let max_w = (full.width() * 0.5).clamp(240.0, (full.width() - 24.0).max(240.0));
    for t in app.toasts.iter().rev().take(5) {
        let color = match t.level {
            crate::app::Level::Info => theme.fg,
            crate::app::Level::Warn => theme.warning,
            crate::app::Level::Error => theme.progress_error,
        };
        let text = match t.count {
            0 | 1 => t.text.clone(),
            n => format!("{} ×{n}", t.text),
        };
        let g = painter.layout(clip_lines(&text, TOAST_LINES), f.clone(), color, max_w - 20.0);
        let w = g.size().x + 20.0;
        // The galley's own height, not one row. A five-line parse error drawn
        // in a one-row box spilled out of both ends of it -- over the header
        // above and the file list below -- because the text is centred in the
        // box and the box was the wrong size.
        let r = Rect::from_min_size(
            egui::pos2(full.right() - w - 12.0, y),
            Vec2::new(w, (g.size().y + 8.0).max(row_h + 8.0)),
        );
        painter.rect_filled(r, CornerRadius::same(4), theme.bg_alt);
        painter.rect_stroke(
            r,
            CornerRadius::same(4),
            // The border follows the text, so a warning is framed in its own
            // colour rather than borrowing the plain one and reading as chrome.
            Stroke::new(1.0, if t.level == crate::app::Level::Info { theme.border } else { color }),
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

/// The focus rule is one colour, used by every pane that draws one.
#[cfg(test)]
mod focus_rule_tests {
    use super::*;

    /// Focused has to be visibly different from not, and has to be the accent
    /// the outline already uses -- the terminal drawing its rule in some other
    /// colour would be a third answer to the same question.
    #[test]
    fn focused_is_the_accent_and_unfocused_is_the_border() {
        let theme = Theme::default();
        let on = focus_rule(&theme, true);
        let off = focus_rule(&theme, false);
        assert_ne!(on, off, "a rule that looks the same either way says nothing");
        assert_eq!(off, theme.border);
        assert_eq!(on, theme.tab_active.bg.unwrap_or(theme.fg));
    }

    /// A theme that clears `tab_active.bg` still gets a visible rule rather
    /// than falling back to the border and losing the signal entirely.
    #[test]
    fn a_theme_without_the_accent_still_marks_focus() {
        let mut theme = Theme::default();
        theme.tab_active.bg = None;
        assert_eq!(focus_rule(&theme, true), theme.fg);
        assert_ne!(focus_rule(&theme, true), focus_rule(&theme, false));
    }
}

#[cfg(test)]
mod toast_tests {
    use super::{clip_lines, TOAST_LINES};

    #[test]
    fn a_short_message_is_left_alone() {
        assert_eq!(clip_lines("one line", TOAST_LINES), "one line");
        assert_eq!(clip_lines("a\nb\nc", 3), "a\nb\nc");
    }

    /// The shape a `toml` error actually arrives in: the complaint, the line
    /// it is about, and carets under it. All of it fits, and all of it is the
    /// part that says where to look.
    #[test]
    fn a_parse_error_survives_whole() {
        let err = "TOML parse error at line 23, column 1\n  |\n23 | [[preview]]\n   | ^^^^^^^^^^\ninvalid type: map, expected a string";
        assert_eq!(clip_lines(err, TOAST_LINES), err);
        assert_eq!(err.lines().count(), 5);
    }

    #[test]
    fn past_the_limit_it_says_there_was_more() {
        let many = (1..=12).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        let out = clip_lines(&many, 3);
        assert_eq!(out, "1\n2\n3\n…");
        // The marker is a line, so what is drawn is one more than asked for
        // rather than one of the message being silently dropped for it.
        assert_eq!(out.lines().count(), 4);
    }

    #[test]
    fn exactly_the_limit_gets_no_marker() {
        assert_eq!(clip_lines("a\nb", 2), "a\nb");
        assert_eq!(clip_lines("a\nb\nc", 2), "a\nb\n…");
    }
}

#[cfg(test)]
mod breadcrumb_tests {
    use super::breadcrumb;
    use std::path::{Path, PathBuf};

    /// Built with `join` rather than written out: a literal `C:\a\b` is one
    /// component on Linux, where `parent()` finds nothing, and the test would
    /// fail everywhere but the platform it was written for.
    fn p(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    /// A match found three directories down must say where it actually is.
    /// The old code appended the file name to the directory the search started
    /// from, so this read as though the file were in the top directory.
    #[test]
    fn a_search_hit_shows_its_own_directory() {
        let hit = p(&["dev", "filer", "docs", "guide", "README.md"]);
        let root = p(&["dev", "filer"]);
        let (dir, name) = breadcrumb(Some(&hit), &root, Some("README.md"));
        assert_eq!(dir, p(&["dev", "filer", "docs", "guide"]).display().to_string());
        assert_eq!(name.as_deref(), Some("README.md"));
    }

    /// Two hits with the same name are told apart by the directory half, which
    /// is the whole reason this matters.
    #[test]
    fn two_hits_of_the_same_name_differ() {
        let root = p(&["proj"]);
        let a = p(&["proj", "one", "README.md"]);
        let b = p(&["proj", "two", "README.md"]);
        assert_ne!(
            breadcrumb(Some(&a), &root, Some("README.md")).0,
            breadcrumb(Some(&b), &root, Some("README.md")).0,
        );
    }

    /// Outside a search nothing changes: the tab's directory, and whatever the
    /// cursor is on.
    #[test]
    fn an_ordinary_listing_uses_the_tab_directory() {
        let dir_in = p(&["dev", "filer"]);
        let (dir, name) = breadcrumb(None, &dir_in, Some("Cargo.toml"));
        assert_eq!(dir, dir_in.display().to_string());
        assert_eq!(name.as_deref(), Some("Cargo.toml"));
    }

    /// An empty listing has no file half to show.
    #[test]
    fn nothing_hovered_is_just_the_directory() {
        let (dir, name) = breadcrumb(None, Path::new("anywhere"), None);
        assert_eq!(dir, "anywhere");
        assert_eq!(name, None);
    }
}

#[cfg(test)]
mod summary_line {
    use super::*;

    /// The register says which of the two it is, and is there at all.
    ///
    /// The marker bar cannot answer either question: a file that is selected
    /// *and* yanked draws in the selection's colour, so `y` then `<Space>`
    /// turns green to yellow and the register stops being visible anywhere on
    /// the row — and once the cursor is in another directory there is no row
    /// to look at in the first place.
    /// Q38: the caret stays put, so an open prompt asks for no frames.
    #[test]
    fn the_caret_does_not_blink() {
        assert!(!visuals().text_cursor.blink);
        assert!(visuals().dark_mode, "still egui's dark look");
    }

    #[test]
    fn it_names_the_register_and_the_selection_apart() {
        assert_eq!(summary(19, 0, None, false, false, None), "19 items");
        // #237: one is `1 item`, and none is still `0 items`.
        assert_eq!(summary(1, 0, None, false, false, None), "1 item");
        assert_eq!(summary(0, 0, None, false, false, None), "0 items");
        assert_eq!(summary(19, 1, None, false, false, None), "1 selected · 19 items");
        assert_eq!(summary(19, 0, Some((1, false)), false, false, None), "1 copied · 19 items");
        assert_eq!(summary(19, 0, Some((2, true)), false, false, None), "2 cut · 19 items");

        // Both at once is the case the colours cannot show.
        assert_eq!(
            summary(19, 1, Some((1, false)), false, false, None),
            "1 selected · 1 copied · 19 items",
        );
        assert_eq!(summary(19, 3, Some((2, true)), true, false, None),
            "3 selected · 2 cut · 19 items · hidden shown");
        // #114: mid-walk the usage view's rows are only what has been measured.
        assert_eq!(summary(2, 0, None, false, true, None), "2 measured so far");
        // #122: once the walk is done the header keeps the total.
        assert_eq!(summary(30, 0, None, false, false, Some(2048)), "30 items · 2.0 K total");
        assert_eq!(summary(2, 0, None, false, true, Some(2048)), "2 measured so far", "not mid-walk");
    }
}

#[cfg(test)]
mod parent_column {
    use super::*;

    /// Both kinds of row answer a click, and say what they mean by it.
    ///
    /// The column draws files and directories the same way, so a click that
    /// works on one and is swallowed by the other is indistinguishable from a
    /// broken mouse. A directory is somewhere to go; a file is somewhere to go
    /// *and* something to put the cursor on, which is what `Reveal` is.
    #[test]
    fn a_file_is_revealed_and_a_directory_entered() {
        let dir = crate::util::test_dir("parent-click");
        let _ = std::fs::create_dir_all(dir.join("sub"));
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();

        let as_dir = crate::fs::Entry::from_path(dir.join("sub")).unwrap();
        let as_file = crate::fs::Entry::from_path(file.clone()).unwrap();

        assert_eq!(
            parent_click(&as_dir),
            Act::Cd { target: dir.join("sub").display().to_string(), interactive: false },
        );
        assert_eq!(parent_click(&as_file), Act::Reveal(file.display().to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Running the real drawing code with no window and no GPU.
///
/// egui's frame is two halves, and only the second one needs an adapter:
/// laying out and tessellating is pure CPU, and what needs a driver is turning
/// the resulting meshes into pixels. A test can stop after the first half --
/// and every `Shape::Text` in what comes back carries its galley, so the
/// strings that would have been on screen come back as strings.
///
/// That is enough to assert on the chrome, the overlays and the panes without a
/// baseline image, which matters because [TESTING.md] had ruled the whole
/// category out along with screenshot comparison. Screenshots do need a GPU;
/// asking what the frame *said* does not.
///
/// What this deliberately does not do is run the worker threads. The real frame
/// loop opens with `drain_channels` and `kick_scans`, and a scan landing
/// mid-test would replace the listing the test had just set up. Tests here fill
/// the state in and assert on the drawing; the workers are covered where they
/// live.
#[cfg(test)]
pub(crate) mod harness {
    use super::*;

    /// What one frame put on screen.
    pub(crate) struct Painted {
        /// Every string drawn, in paint order.
        pub texts: Vec<String>,
        /// Only the characters that actually got a glyph, in the same order as
        /// [`Self::texts`].
        ///
        /// The two differ wherever egui had to make a string fit: a galley's
        /// `text()` is what it was *asked* to lay out, so a name cut short to
        /// one row comes back through [`Self::texts`] whole and there is no way
        /// to tell it from one that fitted. The rows hold what was drawn, `…`
        /// included, which is the only way to ask whether something was elided
        /// and where.
        ///
        /// A wrapped string arrives here as its rows run together, with no
        /// separator: nothing in this crate wraps a string whose elision is
        /// worth asking about, so joining keeps the index aligned with
        /// [`Self::texts`] rather than inventing a character that was not drawn.
        pub glyphs: Vec<String>,
        /// Where each string was put, in the same order as [`Self::texts`]: the
        /// galley's top-left corner in window coordinates.
        ///
        /// What this answers is the *layout* rather than the contents -- whether
        /// the icon column left one character before the name or a whole em,
        /// whether two panes start their rows at the same height. Neither shows
        /// up in the strings, and the first of them is what a reader notices
        /// before any of the words.
        pub places: Vec<egui::Pos2>,
        /// Every drawn string and the colour it was drawn in, in paint order.
        ///
        /// The same strings as [`Self::texts`], which stays because most
        /// assertions do not care what colour a word was. This one exists for
        /// the ones that are *only* about the colour: a config warning has to
        /// be the theme's `warning` and not `progress_error`, and the two say
        /// exactly the same words.
        pub inked: Vec<(String, Color32)>,
        /// Every filled rectangle and the colour it was filled with, in paint
        /// order. The colour is what makes a highlight findable: a cursor row is
        /// the theme's `hovered_bg` and nothing else in the frame is.
        pub rects: Vec<(Rect, Color32)>,
        /// Every outline and every rule, with the colour of the stroke rather
        /// than of any fill. A focus rule is a one-pixel line and a toast's
        /// level is in its border, so neither reaches [`Self::rects`], whose
        /// colour is the fill -- `bg_alt` for every toast alike.
        pub strokes: Vec<(Rect, Color32)>,
    }

    impl Painted {
        /// Whether any drawn string contains `needle`.
        pub fn says(&self, needle: &str) -> bool {
            self.texts.iter().any(|t| t.contains(needle))
        }

        /// How much of `text` actually reached the screen, for the galley that
        /// was laid out from exactly that string.
        ///
        /// Exactly, not `contains`: a file's name is also inside the path the
        /// header draws for it, and that path is elided on its own account, so a
        /// needle would find the header first and answer about the wrong galley.
        /// The answer is a shorter string ending in `…` when it was elided, and
        /// `text` back again when it fitted.
        pub fn drawn(&self, text: &str) -> Option<&str> {
            let i = self.texts.iter().position(|t| t == text)?;
            self.glyphs.get(i).map(String::as_str)
        }

        /// Where the galley laid out from exactly `text` ended up.
        ///
        /// Exactly, for the reason [`Self::drawn`] is exact: a file's name is
        /// also inside the path the header draws, and the header is somewhere
        /// else entirely, so a `contains` would answer about the wrong galley.
        pub fn placed(&self, text: &str) -> Option<egui::Pos2> {
            let i = self.texts.iter().position(|t| t == text)?;
            self.places.get(i).copied()
        }

        /// Every rectangle that falls inside `area`, for asking where something
        /// was drawn rather than only whether it was.
        pub fn rects_in(&self, area: Rect) -> Vec<Rect> {
            self.rects.iter().filter(|(r, _)| area.contains_rect(*r)).map(|(r, _)| *r).collect()
        }

        /// Every rectangle filled with exactly `fill`, topmost first.
        pub fn filled(&self, fill: Color32) -> Vec<Rect> {
            self.rects.iter().filter(|(_, c)| *c == fill).map(|(r, _)| *r).collect()
        }

        /// The colours every drawn string containing `needle` was drawn in.
        ///
        /// Empty when the words are not on screen at all, which is a different
        /// answer from "on screen in the wrong colour" -- so a test asserts on
        /// both the length and the contents.
        pub fn ink(&self, needle: &str) -> Vec<Color32> {
            self.inked.iter().filter(|(t, _)| t.contains(needle)).map(|(_, c)| *c).collect()
        }

        /// Every outline or rule stroked in exactly this colour.
        pub fn stroked(&self, color: Color32) -> Vec<Rect> {
            self.strokes.iter().filter(|(_, c)| *c == color).map(|(r, _)| *r).collect()
        }
    }

    /// An [`App`] and the [`egui::Context`] its frames run in.
    ///
    /// The two have to be the same `Context` the whole way: `App::new` keeps a
    /// clone to wake the window with, and `handle_input` reads the events back
    /// out of it. Handing a test two would look like it worked and deliver no
    /// keys.
    pub(crate) struct Screen {
        pub app: App,
        ctx: egui::Context,
        size: Vec2,
        /// The clock the frames are drawn on, in seconds. egui takes it from
        /// the raw input, and the UI measures its own delays against it.
        time: f64,
    }

    impl Screen {
        /// filer's own defaults, listing `at`, in a 1280x800 window.
        pub(crate) fn open(at: impl Into<std::path::PathBuf>) -> Self {
            Self::with_config(crate::config::Config::load(), at)
        }

        /// The same, on a config a test built rather than the machine's.
        ///
        /// `App::new` is where a config warning becomes the toast that says to
        /// go and look, so a test about that toast has to be holding the
        /// config *before* the app is made. Nothing here reads the disk: the
        /// warnings come out of the same `Keymap::load` a real `keymap.toml`
        /// goes through, from its text.
        pub(crate) fn with_config(
            cfg: crate::config::Config,
            at: impl Into<std::path::PathBuf>,
        ) -> Self {
            let ctx = egui::Context::default();
            ctx.set_visuals(super::visuals());
            let app = App::new(cfg, at.into(), ctx.clone());
            Self { app, ctx, size: Vec2::new(1280.0, 800.0), time: 0.0 }
        }

        /// A different window size -- narrow enough to drop the minimap, short
        /// enough to scroll.
        pub(crate) fn sized(mut self, w: f32, h: f32) -> Self {
            self.size = Vec2::new(w, h);
            self
        }

        /// The window frames are drawn into.
        pub(crate) fn rect(&self) -> Rect {
            Rect::from_min_size(egui::Pos2::ZERO, self.size)
        }

        /// The frame loop's other half, once: take what the workers have
        /// answered, ask again, and draw.
        ///
        /// [`Screen::draw`] leaves the workers out on purpose (see the module's
        /// own note), and for a listing set up by hand that is the only safe
        /// thing to do. But the preview *arriving* is what several of
        /// TESTING.md's sections are about, and it cannot be set up by hand and
        /// still be that: the payload has to come back through the real channel.
        /// So this mirrors `Filer::ui`'s order exactly -- drain, then
        /// `kick_scans`, then `request_preview`, then draw -- and leaves the
        /// deciding to the caller.
        pub(crate) fn turn(&mut self) -> Painted {
            // The same context the frame draws in, because `drain_channels`
            // uploads an image's texture into it: a second one would register
            // the texture where nothing looks for it.
            let ctx = self.ctx.clone();
            self.app.drain_channels(&ctx);
            self.app.kick_scans();
            self.app.request_preview(false);
            self.draw()
        }

        /// Turns until the hovered file's preview is on screen, or ten seconds
        /// have gone by.
        ///
        /// The debounce goes to zero first. It is there so that a cursor still
        /// moving does not touch the disk, and a test whose cursor is not moving
        /// only waits on it -- 40ms per look at a file, and section 27 asks for
        /// ten files in a row.
        pub(crate) fn settle(&mut self) -> Painted {
            self.app.cfg.ui.preview_debounce_ms = 0;
            let mut f = self.turn();
            for _ in 0..1000 {
                if self.preview_arrived() {
                    return f;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
                f = self.turn();
            }
            f
        }

        /// Whether the preview on screen is the hovered file's, fully loaded.
        ///
        /// `App::preview_ready` says exactly this and is private, so this is the
        /// same two questions asked from outside: a `Ready` payload, and a key
        /// naming the file the cursor is on. Either alone would pass while the
        /// pane showed the file before it.
        ///
        /// And a third: the key asked for is in the cache. A re-layout keeps
        /// the old payload `Ready` while the new one is read, and the very first
        /// look at a file is one -- its request goes out at the default box
        /// size, before any frame has measured the pane. Stopping there left the
        /// real-size answer in flight, and a cursor that moved on made it stale,
        /// so coming back found nothing cached (27.3 failed on the Windows
        /// runner, where the first read wins the race).
        pub(crate) fn preview_arrived(&self) -> bool {
            let key = self.app.preview.key.as_ref();
            matches!(self.app.preview.state, crate::app::PreviewState::Ready(_))
                && key.map(|k| &k.path) == self.app.tab().current.hovered().map(|e| &e.path)
                && key.is_some_and(|k| self.app.preview.cache.peek(k).is_some())
        }

        /// One frame, with nothing typed.
        pub(crate) fn draw(&mut self) -> Painted {
            self.feed(Vec::new())
        }

        /// Let `secs` pass before the next frame.
        ///
        /// Some of the UI waits on a clock rather than on an event -- the
        /// minimap's hover card holds back for `tooltip_delay` after the
        /// pointer arrives -- and that clock is the one egui reads out of the
        /// raw input. Skipping the wait forward beats drawing the eighteen
        /// frames a sixtieth of a second apart that would otherwise cover it.
        pub(crate) fn wait(&mut self, secs: f64) -> &mut Self {
            self.time += secs;
            self
        }

        /// One frame, after `text` arrives as egui delivers typing: one
        /// [`egui::Event::Text`] per character.
        pub(crate) fn typed(&mut self, text: &str) -> Painted {
            self.feed(text.chars().map(|c| egui::Event::Text(c.to_string())).collect())
        }

        /// One frame, after `events` arrive the way the window sees them.
        pub(crate) fn feed(&mut self, events: Vec<egui::Event>) -> Painted {
            // A frame's worth of clock, the same step egui would have guessed
            // for itself had the raw input left `time` unset.
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            // Cloned out of `self` so the closure can hold `self.app` mutably.
            let ctx = self.ctx.clone();
            let app = &mut self.app;
            let out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                crate::handle_input(app, &ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(app.cfg.theme.bg))
                    .show(ui, |ui| draw(app, ui));
            });
            // epaint panics if a delta is dropped with no renderer having taken
            // it, which is exactly what this is: there is nothing to upload to.
            let mut textures = out.textures_delta;
            textures.clear();

            let mut painted = Painted {
                texts: Vec::new(),
                glyphs: Vec::new(),
                places: Vec::new(),
                inked: Vec::new(),
                rects: Vec::new(),
                strokes: Vec::new(),
            };
            for clipped in &out.shapes {
                collect(&clipped.shape, &mut painted);
            }
            painted
        }
    }

    /// `Shape::Vec` nests, so the walk has to recurse: a pane's contents arrive
    /// as one shape holding the rest.
    fn collect(shape: &egui::epaint::Shape, into: &mut Painted) {
        use egui::epaint::Shape;
        match shape {
            Shape::Text(t) => {
                let text = t.galley.text().to_owned();
                into.texts.push(text.clone());
                into.glyphs.push(
                    t.galley.rows.iter().flat_map(|r| r.glyphs.iter()).map(|g| g.chr).collect(),
                );
                into.places.push(t.pos);
                // `Painter::text` lays the galley out in the colour it is given
                // and passes the same colour as the fallback, so for a string
                // drawn in one colour this is that colour. A galley built from
                // a `LayoutJob` of several colours -- a preview line, a hover
                // card -- reports only the fallback, which is why the checks
                // that care read a rule or a fill instead.
                into.inked.push((text, t.override_text_color.unwrap_or(t.fallback_color)));
            }
            Shape::Rect(r) => {
                into.rects.push((r.rect, r.fill));
                if r.stroke.width > 0.0 {
                    into.strokes.push((r.rect, r.stroke.color));
                }
            }
            // A rule: `Painter::vline` and `Painter::line_segment` both land
            // here, and a zero-area `Rect` is still the right answer for where
            // it was drawn.
            Shape::LineSegment { points, stroke } => {
                into.strokes.push((Rect::from_two_pos(points[0], points[1]), stroke.color));
            }
            Shape::Vec(v) => v.iter().for_each(|s| collect(s, into)),
            _ => {}
        }
    }
}

/// The whole frame, drawn with no window: what [`harness`] is for.
///
/// These are the checks that no unit test could reach, because what they are
/// about is the wiring rather than any one function. The pieces below all had
/// their own tests already and still could not answer "is this on screen".
#[cfg(test)]
mod whole_frame {
    use super::harness::Screen;
    use crate::preview::{Extent, MapRow};
    use egui::Rect;

    /// The chrome names the directory and counts what is in it.
    ///
    /// Three separate readings of the same state -- the header's path, the
    /// status bar's count, and the position -- which is what makes it worth
    /// asserting together: a listing that reached one of them and not the
    /// others is the shape of bug this catches.
    #[test]
    fn the_chrome_says_where_you_are() {
        let dir = crate::util::test_dir("frame-chrome");
        std::fs::write(dir.join("one.txt"), "1").unwrap();
        std::fs::write(dir.join("two.txt"), "2").unwrap();
        let entries = std::sync::Arc::new(vec![
            crate::fs::Entry::from_path(dir.join("one.txt")).unwrap(),
            crate::fs::Entry::from_path(dir.join("two.txt")).unwrap(),
        ]);

        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir.clone(), entries, true);

        let f = s.draw();
        assert!(f.says(&dir.display().to_string()), "the path is in the header: {:?}", f.texts);
        assert!(f.says("2 items"), "the status bar counts them: {:?}", f.texts);
        assert!(f.says("1/2"), "and says which one the cursor is on: {:?}", f.texts);
        assert!(f.says("NORMAL"), "the mode is drawn: {:?}", f.texts);
        assert!(f.says("one.txt") && f.says("two.txt"), "the rows: {:?}", f.texts);
        // The hovered name is joined to the directory with the platform's own
        // separator, which was a literal `\` until v0.45.0.
        assert!(
            f.says(&dir.join("one.txt").display().to_string()),
            "the header spells a path this platform would accept: {:?}",
            f.texts,
        );
    }

    /// #191: a toast starts below the breadcrumb, not on it. The breadcrumb
    /// is the header's second row; a toast one row down cleared the tab strip
    /// and cut a long path in half.
    #[test]
    fn a_toast_leaves_the_breadcrumb_alone() {
        let dir = crate::util::test_dir("frame-toast-crumb");
        std::fs::write(dir.join("one.txt"), "1").unwrap();
        let entries = std::sync::Arc::new(vec![crate::fs::Entry::from_path(dir.join("one.txt")).unwrap()]);
        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current = crate::core::folder::Folder::from_entries(dir.clone(), entries, true);
        s.app.toast("a message for the corner");

        let f = s.draw();
        let crumb = dir.join("one.txt").display().to_string();
        let at = |needle: &str| f.texts.iter().position(|t| t.contains(needle)).map(|i| f.places[i]);
        let crumb_y = at(&crumb).expect("the breadcrumb is drawn").y;
        let toast_y = at("a message for the corner").expect("the toast is drawn").y;
        let hovered = s.app.cfg.theme.hovered_bg;
        let row_h = f.rects.iter().find(|(_, c)| *c == hovered).expect("the cursor row").0.height();
        assert!(
            toast_y >= crumb_y + row_h,
            "the toast's text ({toast_y}) is a row below the breadcrumb's ({crumb_y}, row {row_h})",
        );
    }

    /// #105: a folder still waiting for its first listing gets no count -- not
    /// `0 items` and `0/0`, which read as having arrived somewhere empty.
    #[test]
    fn a_folder_not_yet_listed_is_not_counted() {
        let dir = crate::util::test_dir("frame-waiting");
        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current = crate::core::folder::Folder::loading(dir.join("far"), None);
        let f = s.draw();
        assert!(f.says("listing…"), "{:?}", f.texts);
        assert!(!f.says("0 items") && !f.says("0/0"), "{:?}", f.texts);
    }

    /// Visual mode says so, rather than only behaving differently.
    ///
    /// The word on screen is `SELECT`, not `VISUAL`: `v` is yazi's visual mode
    /// and the indicator names what it does rather than what the key is called.
    #[test]
    fn visual_mode_is_visible() {
        let dir = crate::util::test_dir("frame-visual");
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let entries =
            std::sync::Arc::new(vec![crate::fs::Entry::from_path(dir.join("a.txt")).unwrap()]);

        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir.clone(), entries, true);
        assert!(s.draw().says("NORMAL"), "before");

        let f = s.typed("v");
        assert!(f.says("SELECT"), "after `v`: {:?}", f.texts);
        assert!(f.says("1 selected"), "and the status bar counts it: {:?}", f.texts);
    }

    /// A chord and the character it would have typed arrive together, and only
    /// the chord runs.
    ///
    /// This is the v0.38.0 bug, which shipped: Windows sends `<A-m>` as a key
    /// event *and* then as `Text("m")`, so one keystroke ran `send_pane --cut`
    /// and went on to offer the line-mode menu as well. `m` is a prefix in the
    /// default keymap (`m s`, `m t`, ...), so a leaked character is not a
    /// silent state change -- it puts the which-key panel on screen listing
    /// `m s`, `m t` and the rest, which is what makes this assertable from a
    /// frame at all.
    #[test]
    fn an_alt_chord_does_not_also_type_its_letter() {
        let dir = crate::util::test_dir("frame-chord");
        let mut s = Screen::open(dir);

        // The control: the character on its own does open the menu, so a test
        // that stopped catching the leak would fail here rather than pass
        // quietly.
        let bare = s.typed("m");
        assert!(bare.says("Line mode"), "`m` alone offers the menu: {:?}", bare.texts);
        s.app.pending.clear();
        s.app.which.clear();

        let alt = egui::Modifiers { alt: true, ..Default::default() };
        let chord = s.feed(vec![
            egui::Event::Key {
                key: egui::Key::M,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: alt,
            },
            egui::Event::Text("m".into()),
        ]);
        assert!(
            !chord.says("Line mode"),
            "the chord was handled and the text dropped: {:?}",
            chord.texts,
        );
        assert!(s.app.pending.is_empty(), "no half-typed chord is left over");
    }

    /// A long file puts a minimap down the right of the preview; the flag and a
    /// narrow window each take it away.
    ///
    /// `split_minimap` decides the geometry and is unit-tested on its own; what
    /// this adds is that the decision is reached from a frame, with the pane
    /// widths the real layout hands it. The strip is bands, not text, so this is
    /// the one check here that reads rectangles.
    ///
    /// TESTING.md 2.1 and 2.8. Named here because that is where the manual
    /// checklist reads coverage from -- see `examples/make-testcheck.rs`.
    #[test]
    fn a_long_file_gets_a_strip_and_a_narrow_window_does_not() {
        // Wide enough for the preview pane to clear `MINIMAP_MIN_COLS`. At
        // 1280 it does not -- the pane comes out at 54 columns against the 56
        // the map asks for -- which is the check two windows down.
        let mut s = screen_showing_a_long_file().sized(1920.0, 1080.0);
        let full = s.rect();
        // The strip is the last few columns of the window, since the preview is
        // the rightmost pane. Counting the same area with the minimap on and off
        // is what makes this a check rather than a guess at a threshold: the
        // bands are one rectangle per chunk of lines and nothing else there is.
        let strip = Rect::from_x_y_ranges(full.right() - 70.0..=full.right(), full.y_range());

        let on = s.draw().rects_in(strip).len();
        assert!(on > 100, "400 lines put bands down the strip, got {on}");

        s.app.cfg.ui.minimap = false;
        let off = s.draw().rects_in(strip).len();
        assert!(off < 10, "the flag takes them away: {off} left");

        // TESTING.md 2.8: the map goes before the text becomes unreadable.
        let mut narrow = screen_showing_a_long_file().sized(1000.0, 800.0);
        let thin = narrow.rect();
        let thin = Rect::from_x_y_ranges(thin.right() - 70.0..=thin.right(), thin.y_range());
        assert!(
            narrow.draw().rects_in(thin).len() < 10,
            "a pane too narrow for the map draws none of it",
        );
    }

    /// A window with one long file hovered and its preview already delivered.
    ///
    /// The payload is built here rather than scanned, because the harness does
    /// not run the workers: what is under test is the drawing.
    fn screen_showing_a_long_file() -> Screen {
        let dir = crate::util::test_dir("frame-minimap");
        std::fs::write(dir.join("long.rs"), "x").unwrap();
        let entries =
            std::sync::Arc::new(vec![crate::fs::Entry::from_path(dir.join("long.rs")).unwrap()]);
        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir.clone(), entries, true);
        s.app.preview.state = crate::app::PreviewState::Ready(crate::preview::Payload::Text {
            lines: Vec::new(),
            map: (0..400)
                .map(|i| MapRow { indent: (i % 8) as u16, len: 40, color: None })
                .collect(),
            extent: Extent { truncated: false, total: 400, ..Default::default() },
            outline: Vec::new(),
        });
        s
    }
}

/// TESTING.md section 10: the yank register, said out loud.
///
/// The register is told apart from the selection by a 3px bar, and the bar
/// loses: a file that is both draws in the selection's colour, so `y` then
/// `<Space>` leaves nothing on the row to say what `p` would paste. A cursor
/// in another directory has no row to look at in the first place. The words in
/// the header are the one reading that survives both, which is what these
/// check -- with the bar's colour alongside, since the harness sees exactly
/// which theme colour a rectangle was filled with.
#[cfg(test)]
mod yank_frame {
    use super::harness::{Painted, Screen};
    use std::path::PathBuf;

    /// `one.txt` under the cursor and a `dst` directory below it to paste
    /// into. The listing is built here rather than scanned: the harness does
    /// not run the workers, and a scan landing mid-test would replace it.
    fn screen(label: &str) -> (PathBuf, Screen) {
        let dir = crate::util::test_dir(label);
        std::fs::write(dir.join("one.txt"), "1").unwrap();
        std::fs::create_dir_all(dir.join("dst")).unwrap();
        let entries = std::sync::Arc::new(vec![
            crate::fs::Entry::from_path(dir.join("one.txt")).unwrap(),
            crate::fs::Entry::from_path(dir.join("dst")).unwrap(),
        ]);
        let mut s = Screen::open(dir.clone());
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir.clone(), entries, true);
        (dir, s)
    }

    /// How many of the frame's strings carry `needle`.
    fn saying(f: &Painted, needle: &str) -> usize {
        f.texts.iter().filter(|t| t.contains(needle)).count()
    }

    /// 10.1 and 10.8: `y` puts a green bar on the row, and both the header and
    /// the status line say `1 copied` in those words.
    #[test]
    fn a_copy_is_green_on_the_row_and_named_twice_in_the_chrome() {
        let (_dir, mut s) = screen("frame-yank-copy");
        let copied = s.app.cfg.theme.marker_copied;
        let cut = s.app.cfg.theme.marker_cut;

        let before = s.draw();
        assert!(!before.says("copied"), "nothing is in the register yet: {:?}", before.texts);
        assert!(before.filled(copied).is_empty(), "and no bar is drawn for it");

        let f = s.typed("y");
        assert!(f.says("1 copied"), "the register is named: {:?}", f.texts);
        assert_eq!(f.filled(copied).len(), 1, "one row carries the copied bar");
        assert!(f.filled(cut).is_empty(), "a copy is not a cut");
        // 10.8: the status line says the same thing in the same words, which
        // is the point of them sharing the phrasing at all.
        assert_eq!(
            saying(&f, "1 copied"), 2,
            "the header's summary and the status line both say it: {:?}", f.texts,
        );
    }

    /// 10.2: `<Space>` on the file that was just yanked turns the bar yellow.
    ///
    /// The selection's colour winning is by design, and it is exactly why the
    /// header has to carry both counts: at this point the row says `selected`
    /// and nothing at all says `copied`.
    #[test]
    fn the_selection_colour_wins_and_the_header_carries_both() {
        let (_dir, mut s) = screen("frame-yank-both");
        let copied = s.app.cfg.theme.marker_copied;
        let selected = s.app.cfg.theme.marker_selected;

        s.typed("y");
        let f = s.typed(" ");
        assert!(
            f.says("1 selected · 1 copied"),
            "the header spells out both states: {:?}", f.texts,
        );
        assert_eq!(f.filled(selected).len(), 1, "the row's bar is the selection's colour");
        assert!(
            f.filled(copied).is_empty(),
            "and the yank has gone invisible under it, which is the whole reason for the words",
        );
    }

    /// 10.3: `x` is the same in red, and says `cut`.
    #[test]
    fn a_cut_is_red_and_says_so() {
        let (_dir, mut s) = screen("frame-yank-cut");
        let copied = s.app.cfg.theme.marker_copied;
        let cut = s.app.cfg.theme.marker_cut;

        let f = s.typed("x");
        assert!(f.says("1 cut"), "the register says which of the two it is: {:?}", f.texts);
        assert!(!f.says("1 copied"), "and not the other one: {:?}", f.texts);
        assert_eq!(f.filled(cut).len(), 1, "one row carries the cut bar");
        assert!(f.filled(copied).is_empty());
        assert_eq!(saying(&f, "1 cut"), 2, "header and status line again: {:?}", f.texts);
    }

    /// 10.4: the register crosses a directory boundary, where no row can.
    ///
    /// What `p` would paste is a fact about the register, not about anything
    /// on screen, so leaving the directory the files came from must not take
    /// the count away with it.
    #[test]
    fn the_register_outlives_the_directory_it_came_from() {
        let (dir, mut s) = screen("frame-yank-away");
        let copied = s.app.cfg.theme.marker_copied;

        s.typed("y");
        // `j` onto `dst`, `l` into it. The listing there is empty because the
        // scan is a worker's job and no worker runs here -- which is the case
        // this check is about.
        let f = s.typed("jl");
        assert_ne!(s.app.tab().cwd, dir, "the tab moved");
        assert!(f.says("1 copied"), "the register came along: {:?}", f.texts);
        assert!(f.filled(copied).is_empty(), "with no row here to show it");
    }

    /// 10.5 and 10.6: a copy can be pasted again, a cut cannot.
    ///
    /// `p` empties the register only when it was a cut, because the files it
    /// names have moved and a second paste would be looking for them where
    /// they no longer are.
    #[test]
    fn a_copy_outlives_the_paste_and_a_cut_does_not() {
        let (_dir, mut s) = screen("frame-yank-paste");
        s.typed("y");
        let f = s.typed("jlp");
        assert!(f.says("1 copied"), "the register stays, so `p` pastes again: {:?}", f.texts);
        assert_eq!(s.app.yank.paths.len(), 1);

        let (_dir, mut s) = screen("frame-cut-paste");
        s.typed("x");
        assert!(s.draw().says("1 cut"), "in the register to begin with");
        // `1 cut` rather than `cut`: the yank's own toast says "(cut)" and is
        // still on screen, so the loose word would never go away.
        let f = s.typed("jlp");
        assert!(!f.says("1 cut"), "the cut is spent and the count is gone: {:?}", f.texts);
        assert!(s.app.yank.paths.is_empty(), "the register emptied");
    }

    /// 10.7: `X` and `Y` both put the register back, and the count leaves the
    /// header with it.
    #[test]
    fn unyank_takes_the_count_away() {
        for (key, yank, count) in [("Y", "y", "1 copied"), ("X", "x", "1 cut")] {
            let (_dir, mut s) = screen(&format!("frame-unyank-{key}"));
            let f = s.typed(yank);
            assert!(f.says(count), "`{count}` is in the register: {:?}", f.texts);

            let f = s.typed(key);
            assert!(!f.says(count), "`{key}` cleared it: {:?}", f.texts);
            assert!(s.app.yank.paths.is_empty(), "`{key}` emptied the register");
        }
    }
}

/// Fixtures shared by TESTING.md sections 6, 18 and 36.
///
/// All three are about how the body is divided into columns -- a second pane
/// taking the parent's place, a panel drawn over the lot, a preview column
/// widened until the list is gone -- so they need the same two things: a window
/// with a listing already in it, and a way to press a chord. Kept in one place
/// because a second pane built slightly differently in each module would make
/// the three sections disagree about what "split" means.
#[cfg(test)]
mod panes {
    use super::harness::Screen;
    use crate::core::folder::Folder;
    use crate::preview::{Extent, Payload, Span};
    use std::path::PathBuf;
    use std::sync::Arc;

    /// A chord, the way the window delivers one.
    pub(super) fn key(k: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    pub(super) fn ctrl() -> egui::Modifiers {
        egui::Modifiers { ctrl: true, ..Default::default() }
    }

    pub(super) fn ctrl_shift() -> egui::Modifiers {
        egui::Modifiers { ctrl: true, shift: true, ..Default::default() }
    }

    pub(super) fn alt() -> egui::Modifiers {
        egui::Modifiers { alt: true, ..Default::default() }
    }

    pub(super) fn esc() -> egui::Event {
        key(egui::Key::Escape, egui::Modifiers::NONE)
    }

    /// The listing `paths` would scan to, built here because the harness runs
    /// no workers: a scan landing mid-test would replace whatever was set up.
    pub(super) fn listing(dir: &std::path::Path, paths: &[&str]) -> Arc<Vec<crate::fs::Entry>> {
        Arc::new(
            paths
                .iter()
                .map(|n| crate::fs::Entry::from_path(dir.join(n)).unwrap())
                .collect(),
        )
    }

    /// One directory holding `a.txt`, `b.txt` and `sub/`, with the cursor on
    /// `a.txt`. The directory is third so that a test can put the cursor on one
    /// by moving to the end of the list.
    pub(super) fn one(label: &str) -> Screen {
        let dir = crate::util::test_dir(label);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let entries = listing(&dir, &["a.txt", "b.txt", "sub"]);
        let mut s = Screen::open(dir.clone());
        s.app.tabs[0].current = Folder::from_entries(dir.clone(), entries, true);
        s
    }

    /// Two tabs on two directories, so that `<C-w>` borrows the second rather
    /// than making one -- and so that the panes have different contents, which
    /// is what every send between them needs.
    ///
    /// `left` holds `one.txt` and `two.txt`; `right` holds `far.txt`. The keys
    /// start in `left`.
    pub(super) fn two(label: &str) -> (PathBuf, PathBuf, Screen) {
        let root = crate::util::test_dir(label);
        let (left, right) = (root.join("left"), root.join("right"));
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("one.txt"), "1").unwrap();
        std::fs::write(left.join("two.txt"), "2").unwrap();
        std::fs::write(right.join("far.txt"), "f").unwrap();

        let mut s = Screen::open(left.clone());
        s.app.tabs[0].cwd = left.clone();
        s.app.tabs[0].current =
            Folder::from_entries(left.clone(), listing(&left, &["one.txt", "two.txt"]), true);

        let sort = s.app.tabs[0].sort;
        let mut tab = crate::core::tab::Tab::new(
            right.clone(),
            sort,
            false,
            crate::fs::entry::Linemode::None,
        );
        tab.cwd = right.clone();
        tab.current = Folder::from_entries(right.clone(), listing(&right, &["far.txt"]), true);
        s.app.tabs.push(tab);

        (left, right, s)
    }

    /// A 200-line text payload, so that `seek` has somewhere to go.
    ///
    /// `preview.max_offset` is written by whichever pane drew last, and with no
    /// payload it stays 0 -- an `<A-j>` against an empty preview moves nothing
    /// and would look like the key was not wired up.
    pub(super) fn long_text() -> crate::app::PreviewState {
        crate::app::PreviewState::Ready(Payload::Text {
            lines: (0..200)
                .map(|i| vec![Span { text: format!("line {i}"), ..Default::default() }])
                .collect(),
            map: Vec::new(),
            extent: Extent { truncated: false, total: 200, ..Default::default() },
            outline: Vec::new(),
        })
    }

    /// The first preview line the frame drew, which is how far `seek` has got.
    pub(super) fn first_preview_line(f: &super::harness::Painted) -> Option<&String> {
        f.texts.iter().find(|t| t.starts_with("line "))
    }

    /// How many of the frame's strings are exactly `needle`.
    ///
    /// Exactly, not `contains`: a file name also turns up inside the header's
    /// path, so "is the name drawn on its own" -- as a row, or as a panel's
    /// title -- is a question `Painted::says` cannot answer.
    pub(super) fn drawn_alone(f: &super::harness::Painted, needle: &str) -> usize {
        f.texts.iter().filter(|t| *t == needle).count()
    }

    /// Wait for a file operation to land, up to ten seconds.
    ///
    /// The ops worker is a real thread and really copies, so a send between the
    /// panes can be checked where it counts: in the other pane's directory. Ten
    /// seconds is far more than a one-byte copy needs; it is there so that a
    /// loaded machine cannot turn this into a flake.
    pub(super) fn lands(path: &std::path::Path) -> bool {
        for _ in 0..1000 {
            if path.exists() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    /// Every pane the last frame laid out, left to right.
    pub(super) fn pane_rects(s: &Screen) -> Vec<egui::Rect> {
        let mut rects: Vec<egui::Rect> = s.app.pane_rects.iter().map(|(_, r)| *r).collect();
        rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
        rects
    }
}

/// TESTING.md section 6: split view, and sending between the panes.
///
/// `<A-c>` and `<A-m>` are the only commands whose destination is a *pane*, so
/// most of the section is about what they refuse -- and both refusals are a
/// sentence on screen, which is exactly what a frame can read. The layout half
/// is arithmetic the renderer does every frame and nothing else asserts: that
/// the second pane takes the parent column's place at the list's own width, and
/// that the parent comes back when the split closes.
#[cfg(test)]
mod split_panes_frame {
    use super::panes::*;

    /// 18.7's drop: a file dragged from one list and let go over the other
    /// is copied there, or moved with Shift -- both ways round. From left to
    /// right it did nothing at all (#208): the left list, drawn first, placed
    /// the drop before the right one had said where it was.
    #[test]
    fn a_drop_lands_in_the_other_list_both_ways() {
        let (_left, _right, mut s) = two("panes-drop");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert!(s.app.split.is_some(), "the view split");
        let hovered = s.app.cfg.theme.hovered_bg;
        let f = s.draw();
        let row_h = f.rects.iter().find(|(_, c)| *c == hovered).expect("the cursor row").0.height();
        let panes = pane_rects(&s);
        // The first row of a list, and somewhere empty in the other.
        let row = |r: egui::Rect| egui::pos2(r.left() + 60.0, r.top() + row_h * 0.5);
        let drag = |s: &mut super::harness::Screen, from: egui::Pos2, to: egui::Pos2, modifiers: egui::Modifiers| {
            let button = |pressed| egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            s.feed(vec![egui::Event::PointerMoved(from), button(true)]);
            for k in 1..=8 {
                s.feed(vec![egui::Event::PointerMoved(from + (to - from) * (k as f32 / 8.0))]);
            }
            s.feed(vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }]);
            s.draw();
        };
        let jobs = |s: &super::harness::Screen| {
            s.app.tasks.iter().map(|t| (t.kind, t.label.clone())).collect::<Vec<_>>()
        };
        use crate::fs::ops::OpKind;
        drag(&mut s, row(panes[0]), panes[1].center(), egui::Modifiers::NONE);
        assert_eq!(jobs(&s), [(OpKind::Copy, "Copy 1 item(s)".to_owned())], "left onto right copies");

        let shift = egui::Modifiers { shift: true, ..Default::default() };
        drag(&mut s, row(panes[1]), panes[0].center(), shift);
        assert_eq!(jobs(&s).len(), 2, "{:?}", jobs(&s));
        assert_eq!(jobs(&s)[1].0, OpKind::Move, "right onto left, with Shift, moves");

        // Let go over the list it came from: nothing.
        drag(&mut s, row(panes[0]), panes[0].center(), egui::Modifiers::NONE);
        assert_eq!(jobs(&s).len(), 2, "a drop on its own list is no job: {:?}", jobs(&s));
    }

    /// 19.7: each list of a split keeps its own part of a turn.
    ///
    /// A turn too small to move a row is kept for the next one, so that slow
    /// turns add up. It was kept once for both lists, so most of a row over the
    /// left and most of a row over the right moved the right by one (#202).
    #[test]
    fn each_side_of_a_split_keeps_its_own_part_of_a_turn() {
        let (left, right, mut s) = two("panes-wheel");
        let names: Vec<String> = (0..200).map(|i| format!("f{i:03}.txt")).collect();
        for dir in [&left, &right] {
            for n in &names {
                std::fs::write(dir.join(n), "x").unwrap();
            }
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        use crate::core::folder::Folder;
        s.app.tabs[0].current = Folder::from_entries(left.clone(), listing(&left, &refs), true);
        s.app.tabs[1].current = Folder::from_entries(right.clone(), listing(&right, &refs), true);
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert!(s.app.split.is_some(), "the view split");
        let hovered = s.app.cfg.theme.hovered_bg;
        let f = s.draw();
        let row_h = f.rects.iter().find(|(_, c)| *c == hovered).expect("the cursor row").0.height();
        let panes = pane_rects(&s);
        let offsets = |s: &super::harness::Screen| (s.app.tabs[0].current.offset, s.app.tabs[1].current.offset);

        // Two thirds of a row, toward you, with the pointer over `place`; then
        // frames enough for egui's smoothing to hand all of it over.
        let turn = |s: &mut super::harness::Screen, place: egui::Rect| {
            s.feed(vec![
                egui::Event::PointerMoved(place.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -row_h * (2.0 / 3.0) / 1.5),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            for _ in 0..60 {
                s.draw();
            }
        };
        turn(&mut s, panes[0]);
        assert_eq!(offsets(&s), (0, 0), "two thirds of a row moves nothing");
        turn(&mut s, panes[1]);
        assert_eq!(offsets(&s), (0, 0), "nor does the same over the other list");
        turn(&mut s, panes[1]);
        assert_eq!(offsets(&s), (0, 1), "but a second one there makes a row, there");
    }

    /// 6.1, 6.2, 6.3 and 6.14: one press splits, the next moves the keys, the
    /// pane without them is dimmer, and `<C-S-w>` puts the parent column back.
    ///
    /// The cursor colour is the whole of 6.3: `hovered_bg` and
    /// `inactive_hovered_bg` are two different theme entries and exactly one
    /// row is drawn in each, so "which side has the keys" is readable from the
    /// frame rather than only from `app.active`.
    #[test]
    fn one_press_splits_the_view_and_the_next_moves_the_keys() {
        let (_left, _right, mut s) = two("panes-split");
        let hovered = s.app.cfg.theme.hovered_bg;
        let dimmed = s.app.cfg.theme.inactive_hovered_bg;
        assert_ne!(hovered, dimmed, "a theme that drew both the same would say nothing");

        // Unsplit: one pane, with the parent column to the left of it.
        let f = s.draw();
        let before = pane_rects(&s);
        assert_eq!(before.len(), 1, "one pane to begin with");
        assert!(before[0].left() > 24.0, "the parent column is there, at {}", before[0].left());
        assert_eq!(f.filled(dimmed).len(), 0, "and nothing is dimmed with no other pane");

        let f = s.feed(vec![key(egui::Key::W, ctrl())]);
        assert!(s.app.split.is_some(), "the view split");
        let split = pane_rects(&s);
        assert_eq!(split.len(), 2, "two panes side by side");
        // 6.1: the parent column is gone and the second pane stands in its
        // place, "at the same width as the list it sits next to".
        assert!(split[0].left() < 1.0, "the left pane starts at the body's edge");
        assert!(
            (split[0].width() - split[1].width()).abs() < 1.0,
            "the two panes are the same width: {} and {}",
            split[0].width(),
            split[1].width(),
        );
        assert!(split[0].right() < split[1].left(), "side by side, not overlapping");
        // The preview keeps its column: its background is the one `bg_alt`
        // rectangle to the right of both panes.
        let preview = f
            .rects
            .iter()
            .filter(|(r, c)| *c == s.app.cfg.theme.bg_alt && r.left() > split[1].right())
            .count();
        assert_eq!(preview, 1, "the layout is pane, pane, preview");

        // 6.3: one cursor row in each colour, and the dim one is the pane the
        // keys are not in.
        assert_eq!(f.filled(hovered).len(), 1, "the focused pane's cursor: {:?}", f.texts);
        assert_eq!(f.filled(dimmed).len(), 1, "and the other pane's, dimmer");
        let lit = f.filled(hovered)[0];
        let other = s.app.other_pane().expect("the split names the other tab");
        let others = s.app.pane_rects.iter().find(|(i, _)| *i == other).unwrap().1;
        assert!(!others.contains_rect(lit), "the lit row is not in the pane without the keys");

        // 6.2: the same key moves the keys, and keeps moving them.
        let was = s.app.active;
        let f = s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_ne!(s.app.active, was, "`<C-w>` again moved the keys");
        assert!(s.app.split.is_some(), "without closing the split");
        assert_eq!(f.filled(hovered).len(), 1, "still exactly one lit cursor");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_eq!(s.app.active, was, "and again, back where it started");

        // 6.14: back to one pane, with the parent column returned.
        let f = s.feed(vec![key(egui::Key::W, ctrl_shift())]);
        assert!(s.app.split.is_none(), "`<C-S-w>` closed the second pane");
        let after = pane_rects(&s);
        assert_eq!(after.len(), 1, "one pane again");
        assert!(after[0].left() > 24.0, "and the parent column is back at {}", after[0].left());
        assert_eq!(f.filled(dimmed).len(), 0, "nothing left to dim");
    }

    /// 6.13: the second pane is made when there is nothing to borrow, and
    /// borrowed when there is.
    ///
    /// Two tabs is the case that would go unnoticed: making a third would leave
    /// a tab nobody asked for behind every `<C-w>`.
    #[test]
    fn a_second_pane_is_made_or_borrowed() {
        let mut s = one("panes-borrow");
        assert_eq!(s.app.tabs.len(), 1, "one tab to begin with");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_eq!(s.app.tabs.len(), 2, "a second tab was made on the same directory");
        assert_eq!(s.app.tabs[0].cwd, s.app.tabs[1].cwd, "showing where the first one is");

        let (_left, _right, mut s) = two("panes-borrow-two");
        assert_eq!(s.app.tabs.len(), 2);
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_eq!(s.app.tabs.len(), 2, "with one already open, the next is borrowed");
    }

    /// 6.4: the ordinary keys act on the focused pane and nothing else.
    ///
    /// `j` and `h` stand for the lot: no command knows about panes, they all go
    /// through `app.active`, so a cursor or a directory changing in the other
    /// pane would mean one of them had learned.
    #[test]
    fn the_ordinary_keys_move_one_pane_only() {
        let (_left, _right, mut s) = two("panes-focused-only");
        // Two presses: the first hands the keys to the pane that was borrowed,
        // and `left` is the pane with two rows for `j` to move between.
        s.feed(vec![key(egui::Key::W, ctrl())]);
        s.feed(vec![key(egui::Key::W, ctrl())]);
        let other = s.app.other_pane().unwrap();
        let (cursor, cwd) = (s.app.tabs[other].current.cursor, s.app.tabs[other].cwd.clone());

        s.typed("j");
        assert_eq!(s.app.tabs[other].current.cursor, cursor, "`j` left the other pane alone");
        assert_ne!(s.app.tab().current.cursor, cursor, "and moved the focused one");

        s.typed("h");
        assert_eq!(s.app.tabs[other].cwd, cwd, "`h` left the other pane where it was");
        assert_ne!(s.app.tab().cwd, cwd, "and walked the focused one out");
    }

    /// 6.5 and 6.6: `<A-c>` copies into the other pane's directory, and clears
    /// the selection afterwards.
    ///
    /// The destination is checked where it lands rather than in the task's
    /// label, which never names it: "whatever directory the other pane is
    /// showing" is the whole promise of the key, and a job submitted with the
    /// wrong `dest` would queue and read exactly the same.
    #[test]
    fn a_send_lands_in_the_other_pane_and_spends_the_selection() {
        let (_left, right, mut s) = two("panes-send-copy");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        // The keys start in `left`; the first `<C-w>` hands them to the pane
        // that was just borrowed, so take them back.
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_eq!(s.app.tab().current.view.len(), 2, "standing in `left`, on `one.txt`");

        s.typed("  ");
        assert_eq!(s.app.tab().selected.len(), 2, "both files selected");

        s.feed(vec![key(egui::Key::C, alt())]);
        assert!(
            !s.app.toasts.iter().any(|t| t.level == crate::app::Level::Error),
            "the send went through: {:?}",
            s.app.toasts.iter().map(|t| &t.text).collect::<Vec<_>>(),
        );
        // 6.6: unlike `y`, which keeps it.
        assert!(s.app.tab().selected.is_empty(), "the selection is spent");
        assert!(s.app.yank.paths.is_empty(), "and the register was never involved");

        assert!(lands(&right.join("one.txt")), "the first file reached the other pane");
        assert!(lands(&right.join("two.txt")), "and so did the second");
    }

    /// 6.7: `<A-m>` moves -- gone from this pane, present in the other.
    ///
    /// The chord is fed with the character it would also have typed, the way
    /// Windows sends it, because `m` is the line-mode prefix: a send that let
    /// the letter through would move the file *and* open a menu.
    #[test]
    fn a_cut_send_leaves_nothing_behind() {
        let (left, right, mut s) = two("panes-send-move");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        s.feed(vec![key(egui::Key::W, ctrl())]);

        let f = s.feed(vec![key(egui::Key::M, alt()), egui::Event::Text("m".into())]);
        assert!(!f.says("Line mode"), "the letter did not leak into the menu: {:?}", f.texts);
        assert!(lands(&right.join("one.txt")), "the file arrived in the other pane");
        assert!(!left.join("one.txt").exists(), "and left this one");
    }

    /// 6.8 and 6.9: both refusals are sentences on screen, not silence.
    ///
    /// Silence is the failure this pins. A send into the directory the files
    /// are already in would copy each one beside itself under a new name, and a
    /// send with no second pane has nowhere to go -- neither is something to
    /// work out from nothing happening.
    #[test]
    fn the_two_refusals_say_why_on_screen() {
        // 6.9: no second pane. The keymap's own notation is quoted back, which
        // is what makes the message actionable.
        let mut s = one("panes-refuse-unsplit");
        let f = s.feed(vec![key(egui::Key::C, alt())]);
        assert!(f.says("Open the second pane first (<C-w>)"), "{:?}", f.texts);
        assert!(s.app.tasks.is_empty(), "and nothing was queued");

        // 6.8: both panes in the same directory. `<C-w>` with one tab opens the
        // second pane on this very directory, which is the state the row asks
        // for -- the listing has to be in the cache for the new tab to fill.
        let mut s = one("panes-refuse-same");
        let dir = s.app.tab().cwd.clone();
        s.app.cache.put(dir.clone(), listing(&dir, &["a.txt", "b.txt", "sub"]));
        s.feed(vec![key(egui::Key::W, ctrl())]);
        let other = s.app.other_pane().unwrap();
        assert_eq!(s.app.tabs[other].cwd, s.app.tab().cwd, "both panes are in one directory");

        let f = s.feed(vec![key(egui::Key::C, alt())]);
        assert!(f.says("Both panes are in the same directory"), "{:?}", f.texts);
        assert!(s.app.tasks.is_empty(), "and nothing was queued");
    }

    /// 6.10: the yank register still crosses the panes, and can paste where
    /// neither of them is looking.
    ///
    /// `y` `<C-w>` `p` is the route `<A-c>` is a shortcut for, and it is the
    /// one that survives the refusals above: the register does not care which
    /// directory the cursor was in when it was filled.
    #[test]
    fn the_register_still_pastes_across_the_panes() {
        let (_left, right, mut s) = two("panes-yank-route");
        s.typed("y");
        assert_eq!(s.app.yank.paths.len(), 1, "one file in the register");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_eq!(s.app.tab().cwd, right, "the keys are in the other pane");
        s.typed("p");
        assert!(lands(&right.join("one.txt")), "and `p` pasted there");
    }

    /// 6.11: tabs still switch while the view is split, and switching to the
    /// tab the other pane shows moves the keys instead of showing it twice.
    #[test]
    fn tabs_still_switch_while_the_view_is_split() {
        let mut s = one("panes-tabs");
        let dir = s.app.tab().cwd.clone();
        let sort = s.app.tabs[0].sort;
        for _ in 0..2 {
            let mut t =
                crate::core::tab::Tab::new(dir.clone(), sort, false, crate::fs::entry::Linemode::None);
            t.cwd = dir.clone();
            s.app.tabs.push(t);
        }
        s.feed(vec![key(egui::Key::W, ctrl())]);
        let (before, tabs) = (s.app.active, s.app.tabs.len());

        s.typed("]");
        assert_ne!(s.app.active, before, "`]` still switches tabs");
        assert!(s.app.split.is_some(), "and the split survives it");
        s.typed("[");
        assert_eq!(s.app.active, before, "`[` comes back");
        assert_eq!(s.app.tabs.len(), tabs, "no tab was made or lost");

        // `1`-`9` onto the tab the other pane is showing: the keys move there,
        // rather than both panes ending up on one tab.
        let other = s.app.other_pane().unwrap();
        s.typed(&format!("{}", other + 1));
        assert_eq!(s.app.active, other, "the keys went to that pane");
        assert_eq!(s.app.other_pane(), Some(before), "and the pane they left holds the old tab");
        assert!(s.app.split.is_some(), "still two panes");
    }

    /// 6.12: closing one of the two tabs on screen ends the split rather than
    /// leaving a pane pointing at a tab that is gone.
    #[test]
    fn closing_a_pane_s_tab_ends_the_split() {
        let mut s = one("panes-close-tab");
        s.feed(vec![key(egui::Key::W, ctrl())]);
        assert_eq!(s.app.tabs.len(), 2);
        assert!(s.app.split.is_some());

        // egui-winit turns `<C-c>` into a clipboard event and never emits the
        // keypress, so this is the door the window actually uses.
        s.feed(vec![egui::Event::Copy]);
        assert_eq!(s.app.tabs.len(), 1, "the tab closed");
        assert!(s.app.split.is_none(), "and the split ended with it");
        assert!(!s.app.quit, "closing one of two tabs is not quitting");

        let after = pane_rects(&s);
        assert_eq!(after.len(), 1, "one pane, no stale second: {after:?}");
    }
}

/// TESTING.md section 18: quick look, and the rest of the panels.
///
/// Quick look is a panel rather than an overlay on purpose -- it takes no keys,
/// so the list keeps walking underneath and the panel follows it down. That is
/// the half of the section a frame can answer: which file the panel is naming
/// after a `j`, and whether `<A-j>` moved the text inside it.
#[cfg(test)]
mod quick_look_frame {
    use super::panes::*;

    /// 18.1 and 18.4: the panel names the hovered file, says how to leave, and
    /// both `<F3>` and `<Esc>` close it.
    #[test]
    fn the_panel_names_the_file_and_says_how_to_leave() {
        let mut s = one("quick-open");
        let f = s.draw();
        assert!(!f.says("Esc to close"), "nothing is up yet: {:?}", f.texts);

        let f = s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        assert!(s.app.quick, "`<F3>` put the panel up");
        assert!(f.says("Esc to close"), "and it says how to get out: {:?}", f.texts);
        // Twice: once as the list row underneath, once as the panel's title.
        // The header's path holds the name too, which is why this counts exact
        // matches rather than asking `says`.
        assert_eq!(drawn_alone(&f, "a.txt"), 2, "the name is the title as well: {:?}", f.texts);
        // Over the panes, not beside them.
        assert_eq!(
            f.filled(egui::Color32::from_black_alpha(140)).len(), 1,
            "the background is dimmed behind it",
        );

        let f = s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        assert!(!s.app.quick, "`<F3>` again closed it");
        assert!(!f.says("Esc to close"), "{:?}", f.texts);

        s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        let f = s.feed(vec![esc()]);
        assert!(!s.app.quick, "`<Esc>` closes it too");
        assert!(!f.says("Esc to close"), "{:?}", f.texts);
    }

    /// 18.2: the list still moves under the panel, and the panel follows it.
    ///
    /// This is why it is a panel and not an overlay, and the title is the only
    /// thing on screen that shows it: a `j` that moved the cursor and left the
    /// panel on the old file would look identical everywhere else.
    #[test]
    fn the_list_still_walks_under_the_panel() {
        let mut s = one("quick-follow");
        s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        assert_eq!(s.app.tab().current.cursor, 0);

        let f = s.typed("j");
        assert_eq!(s.app.tab().current.cursor, 1, "the list moved");
        assert_eq!(drawn_alone(&f, "b.txt"), 2, "and the panel followed it: {:?}", f.texts);
        assert_eq!(drawn_alone(&f, "a.txt"), 1, "leaving only the row behind: {:?}", f.texts);

        let f = s.typed("k");
        assert_eq!(s.app.tab().current.cursor, 0, "and `k` walks back");
        assert_eq!(drawn_alone(&f, "a.txt"), 2, "{:?}", f.texts);
    }

    /// 18.3: `<A-j>` and `<A-k>` scroll what is inside the panel.
    ///
    /// Read off the text rather than off `preview_offset`, because the panel
    /// and the side column share that field: what this asks is whether the
    /// panel is the thing being drawn from it.
    #[test]
    fn alt_j_and_alt_k_scroll_the_panel_itself() {
        let mut s = one("quick-scroll");
        s.app.preview.state = long_text();
        s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        let f = s.draw();
        assert_eq!(first_preview_line(&f).map(String::as_str), Some("line 0"), "at the top");

        let f = s.feed(vec![key(egui::Key::J, alt())]);
        assert_eq!(
            first_preview_line(&f).map(String::as_str), Some("line 5"),
            "`<A-j>` moved the panel's text down: {:?}", f.texts,
        );
        let f = s.feed(vec![key(egui::Key::K, alt())]);
        assert_eq!(
            first_preview_line(&f).map(String::as_str), Some("line 0"),
            "and `<A-k>` brought it back",
        );
    }

    /// 18.8: `<Tab>` puts the spot panel up, about the hovered file.
    ///
    /// The rows come from the listing's own facts, so they are on screen
    /// without a worker having answered -- which is what makes this checkable
    /// here at all.
    #[test]
    fn the_spot_panel_answers_tab() {
        let mut s = one("quick-spot");
        let f = s.feed(vec![key(egui::Key::Tab, egui::Modifiers::NONE)]);
        assert!(matches!(s.app.overlay, crate::app::Overlay::Spot(_)), "the panel is up");
        assert!(f.says("Spot: a.txt"), "titled with the file: {:?}", f.texts);
        assert!(f.says("<Esc> close"), "and says how to leave: {:?}", f.texts);
        assert!(f.says("<A-j>/<A-k> row"), "and how to move along the rows (#165): {:?}", f.texts);
        for row in ["Name", "Path", "Kind", "Size", "Modified"] {
            assert!(f.says(row), "the panel lists `{row}`: {:?}", f.texts);
        }
    }

    /// 18.9 and 18.10: the context menu and the command palette.
    ///
    /// Both are pick lists built from the config, and both answer a chord no
    /// other check in this section presses. The palette's filter is the half
    /// worth a frame: it is a text field, so a key typed into it has to end up
    /// narrowing the list rather than running a command.
    #[test]
    fn the_context_menu_and_the_palette_both_open() {
        let mut s = one("quick-menus");

        let shift_f10 = egui::Modifiers { shift: true, ..Default::default() };
        let f = s.feed(vec![key(egui::Key::F10, shift_f10)]);
        assert!(matches!(s.app.overlay, crate::app::Overlay::Pick(_)), "the menu is up");
        assert!(f.says("Actions: a.txt"), "about the hovered file: {:?}", f.texts);
        assert!(f.says("Run a shell command"), "with the openers listed: {:?}", f.texts);
        s.feed(vec![esc()]);

        let f = s.feed(vec![key(egui::Key::P, ctrl_shift())]);
        assert!(matches!(s.app.overlay, crate::app::Overlay::Pick(_)), "the palette is up");
        assert!(f.says("Commands"), "{:?}", f.texts);
        assert!(f.says("type to filter"), "{:?}", f.texts);
        let listed = f.texts.len();
        // A letter no command's description holds, so the list has to shrink.
        let f = s.typed("zzzz");
        assert!(
            f.texts.len() < listed,
            "typing filtered the list: {} strings, was {listed}", f.texts.len(),
        );
        assert!(!s.app.quit, "and the letters went into the field, not the keymap");
    }

    /// 18.11's second half, and 18.12: `'` and a letter jumps to the bookmark,
    /// and `z` lists the bookmarks above the recent directories.
    ///
    /// 18.11's first half -- `b` and a letter -- is reported rather than
    /// asserted: `b` is the management prefix in the default keymap, so the
    /// jump is `'` alone. See QA-REPORT.md.
    #[test]
    fn a_bookmark_is_reached_by_quote_and_listed_by_z() {
        let mut s = one("quick-jump");
        let dir = s.app.tab().cwd.clone();
        std::fs::create_dir_all(dir.join("marked")).unwrap();
        std::fs::create_dir_all(dir.join("seen")).unwrap();
        s.app.bookmarks.push(crate::app::Bookmark {
            key: "m".into(),
            path: dir.join("marked"),
            name: "marked".into(),
        });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        s.app.history.push(crate::app::Visit {
            path: dir.join("seen"),
            hits: 3,
            at: now - 2 * 60 * 60,
        });

        // 18.12: named bookmarks first, then the history with its age.
        let f = s.typed("z");
        assert!(matches!(s.app.overlay, crate::app::Overlay::Pick(_)), "the jump list is up");
        assert!(f.says("[m] "), "the bookmark carries its letter: {:?}", f.texts);
        assert!(f.says("marked"), "and its name: {:?}", f.texts);
        assert!(f.says("2h ago"), "the recent directory says how long ago: {:?}", f.texts);
        let bookmark = f.texts.iter().position(|t| t.starts_with("[m] ")).unwrap();
        let recent = f.texts.iter().position(|t| t == "2h ago").unwrap();
        assert!(bookmark < recent, "bookmarks are drawn above the history: {:?}", f.texts);
        s.feed(vec![esc()]);

        // 18.11: `'` then the letter.
        let f = s.typed("'");
        assert!(f.says("Jump to bookmark"), "`'` asks for a letter: {:?}", f.texts);
        s.typed("m");
        assert_eq!(s.app.tab().cwd, dir.join("marked"), "and the letter goes there");
    }
}

/// TESTING.md section 36: `T`, and how it differs from `<F3>`.
///
/// The two are easy to confuse and are not the same thing: `T` widens the
/// preview *column*, leaving the tab bar, the status bar and the background as
/// they are, while `<F3>` is a modal drawn over them at a measured size. Both
/// halves of that are geometry and colour, which is what this harness reads --
/// so the section's own point, that these are visibly different, is the one
/// thing here that no unit test could have made.
#[cfg(test)]
mod max_preview_frame {
    use super::panes::*;

    /// 36.1 and 36.2: `T` gives the body to the preview and puts it back, and
    /// takes nothing else with it.
    #[test]
    fn t_widens_the_preview_column_and_leaves_the_chrome_alone() {
        let mut s = one("max-columns");
        s.app.preview.state = long_text();

        let f = s.draw();
        let before = pane_rects(&s);
        assert_eq!(before.len(), 1);
        assert!(before[0].width() > 400.0, "the list has a column: {}", before[0].width());
        assert!(f.says("3 items") && f.says("1/3"), "the status bar: {:?}", f.texts);

        let f = s.typed("T");
        assert!(s.app.max_preview, "`T` maximized the preview");
        let after = pane_rects(&s);
        assert!(
            after.iter().all(|r| r.width() < 24.0),
            "the list column is squeezed to nothing: {after:?}",
        );
        // The preview took it: its background now spans nearly the whole body.
        let widest = f
            .rects
            .iter()
            .filter(|(_, c)| *c == s.app.cfg.theme.bg_alt)
            .map(|(r, _)| r.width())
            .fold(0.0f32, f32::max);
        assert!(widest > s.rect().width() * 0.9, "the preview has the body: {widest}");
        // 36.1: unchanged, undimmed, unframed.
        assert!(f.says("3 items") && f.says("1/3"), "the status bar is untouched: {:?}", f.texts);
        assert!(f.says(" 1 "), "and so is the tab bar: {:?}", f.texts);
        assert!(
            f.filled(egui::Color32::from_black_alpha(140)).is_empty(),
            "the background is not dimmed -- that is what `<F3>` does",
        );
        assert!(!f.says("Esc to close"), "no frame across the top: {:?}", f.texts);
        // The row itself, and nothing else: `<F3>` would add the name again as
        // a panel title, which is the contrast the next test measures.
        assert_eq!(drawn_alone(&f, "a.txt"), 1, "no file name as a title: {:?}", f.texts);

        s.typed("T");
        assert!(!s.app.max_preview);
        let restored = pane_rects(&s);
        assert_eq!(restored.len(), before.len(), "the columns came back");
        assert!(
            (restored[0].width() - before[0].width()).abs() < 1.0,
            "at the `[mgr] ratio` widths: {} was {}",
            restored[0].width(),
            before[0].width(),
        );
    }

    /// 36.3 and 36.4: both halves still answer keys while the list is invisible.
    ///
    /// The cursor walking a column nobody can see is the odd part of this
    /// state, and it is deliberate: the preview follows the cursor, so the list
    /// has to keep moving for `T` to be useful at all.
    #[test]
    fn the_list_and_the_preview_both_still_answer_keys() {
        let mut s = one("max-keys");
        s.app.preview.state = long_text();
        s.typed("T");

        let f = s.draw();
        assert!(f.says("1/3"), "on the first row: {:?}", f.texts);
        let f = s.typed("j");
        assert_eq!(s.app.tab().current.cursor, 1, "`j` walked the squeezed list");
        assert!(f.says("2/3"), "and the position says so: {:?}", f.texts);

        // 36.4: `<A-j>` moves the preview, not the cursor.
        let f = s.draw();
        assert_eq!(first_preview_line(&f).map(String::as_str), Some("line 0"));
        let f = s.feed(vec![key(egui::Key::J, alt())]);
        assert_eq!(
            first_preview_line(&f).map(String::as_str), Some("line 5"),
            "`<A-j>` scrolled the preview: {:?}", f.texts,
        );
        assert_eq!(s.app.tab().current.cursor, 1, "and left the cursor where it was");
    }

    /// 36.5 and 36.5a: `<Esc>` and `q` each put the columns back, and `q` only
    /// quits once nothing is in front.
    ///
    /// Both were reported from use. Before v0.36.1 `<Esc>` did nothing here and
    /// the first `q` quit the app outright -- with the list invisible, those
    /// are the two keys anyone reaches for.
    #[test]
    fn escape_and_q_each_restore_the_columns() {
        let mut s = one("max-escape");
        s.typed("T");
        s.feed(vec![esc()]);
        assert!(!s.app.max_preview, "`<Esc>` is a way out of this");
        assert!(!s.app.quit);

        s.typed("T");
        s.typed("q");
        assert!(!s.app.max_preview, "the first `q` restored the columns");
        assert!(!s.app.quit, "and did not quit");
        s.typed("q");
        assert!(s.app.quit, "the second `q` quits");
    }

    /// 36.5b and 36.5c: a bare `<Esc>` takes the most visible state first, and
    /// a targeted escape stays targeted.
    ///
    /// A maximized preview hides the list, so it goes before the filter. But
    /// `escape --filter` names what it is for, and a command that also undid
    /// the layout would be doing something it was not asked to.
    #[test]
    fn the_maximized_column_goes_before_the_filter_unless_asked_otherwise() {
        let filter = || crate::core::folder::Filter::new("a".into()).unwrap();

        let mut s = one("max-escape-filter");
        s.app.tabs[0].current.filter = Some(filter());
        s.typed("T");
        s.feed(vec![esc()]);
        assert!(!s.app.max_preview, "the first `<Esc>` restored the columns");
        assert!(s.app.tab().current.filter.is_some(), "and left the filter alone");
        s.feed(vec![esc()]);
        assert!(s.app.tab().current.filter.is_none(), "the second one cleared it");

        let mut s = one("max-escape-targeted");
        s.app.tabs[0].current.filter = Some(filter());
        s.typed("T");
        // What the command line would have run.
        s.app.act(crate::config::cmd::parse("escape --filter"));
        assert!(s.app.max_preview, "a targeted escape left the columns maximized");
        assert!(s.app.tab().current.filter.is_none(), "and cleared what it named");
    }

    /// 36.6 and 36.7: `<F3>` is a measurably different thing, and the two
    /// flags are independent.
    ///
    /// The size is the part worth pinning: 86% by 88% of the window, centred,
    /// with the file's name as its title. `T` draws no such rectangle at all,
    /// which is how a frame tells these two apart.
    #[test]
    fn quick_look_is_a_framed_panel_at_a_measured_size() {
        let mut s = one("max-versus-quick");
        let want = super::modal_rect(s.rect(), 0.86, 0.88);

        // The same rectangle, asked for by size and position rather than by
        // what encloses it: the window's own background contains the panel's
        // area without being the panel.
        let that_size = |r: &egui::Rect| {
            (r.width() - want.width()).abs() < 1.0
                && (r.height() - want.height()).abs() < 1.0
                && (r.center() - want.center()).length() < 1.0
        };

        let f = s.typed("T");
        assert!(
            !f.rects.iter().any(|(r, _)| that_size(r)),
            "a maximized column draws nothing that size",
        );

        let f = s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        assert!(
            f.rects.iter().any(|(r, c)| *c == s.app.cfg.theme.bg_alt && that_size(r)),
            "the panel is 86% x 88% of the window, centred: wanted {want:?}",
        );
        assert_eq!(
            f.filled(egui::Color32::from_black_alpha(140)).len(), 1,
            "with the background dimmed behind it",
        );
        assert!(f.says("Esc to close"), "{:?}", f.texts);
        assert_eq!(
            drawn_alone(&f, "a.txt"), 2,
            "and the name as its title, above the row: {:?}", f.texts,
        );

        // 36.7: the panel closes and the column stays maximized.
        let f = s.feed(vec![esc()]);
        assert!(!s.app.quick, "`<Esc>` took the panel");
        assert!(s.app.max_preview, "and left the column maximized -- two flags, not one");
        assert!(!f.says("Esc to close"), "{:?}", f.texts);
    }

    /// 36.8: turning `T` on clears `hide_parent`, and turning it off does not
    /// put it back.
    ///
    /// Deliberate, and it means `T` is not quite a round trip -- which is the
    /// reason the row exists. A reader who did not know would report the
    /// reappearing parent column as the bug.
    #[test]
    fn turning_t_on_brings_the_parent_pane_back_for_good() {
        let mut s = one("max-hide-parent");
        s.app.hide_parent = true;
        s.draw();
        let hidden = pane_rects(&s);
        assert!(hidden[0].left() < 24.0, "the parent column is gone: {hidden:?}");

        s.typed("T");
        assert!(!s.app.hide_parent, "`T` cleared it");
        s.typed("T");
        assert!(!s.app.max_preview);
        assert!(!s.app.hide_parent, "and toggling off does not hide it again");
        let back = pane_rects(&s);
        assert!(back[0].left() > 24.0, "so the parent column is back: {back:?}");
    }

    /// 36.9: a directory and a file with no preview both toggle without
    /// getting stuck.
    ///
    /// Nothing to draw in the widened column is the case that would panic if
    /// the layout assumed a payload, and "no stuck layout" is the half that
    /// matters: the key has to still put the columns back.
    #[test]
    fn a_directory_and_an_empty_preview_both_toggle() {
        let mut s = one("max-nothing");
        // The cursor onto `sub`, with no payload for it.
        s.typed("G");
        let on_dir = s
            .app
            .tab()
            .current
            .hovered()
            .is_some_and(|e| e.kind == crate::fs::entry::Kind::Dir);
        assert!(on_dir, "standing on `sub`");
        let f = s.typed("T");
        assert!(s.app.max_preview);
        assert!(f.says("3/3"), "the frame still drew: {:?}", f.texts);
        s.typed("T");
        assert!(!s.app.max_preview, "and `T` still puts it back");

        let mut s = one("max-no-payload");
        s.app.preview.state = crate::app::PreviewState::Empty;
        let f = s.typed("T");
        assert!(s.app.max_preview);
        assert!(f.says("1/3"), "{:?}", f.texts);
        s.typed("T");
        assert!(!s.app.max_preview);
    }

    /// 36.11: the help panel lists `T` with its description.
    ///
    /// A tall window because the panel draws only the lines that fit, and `T`
    /// sits a long way down the `[mgr]` layer.
    #[test]
    fn the_help_panel_lists_t_with_its_description() {
        let mut s = one("max-help").sized(1600.0, 2000.0);
        let f = s.typed("~");
        assert_eq!(drawn_alone(&f, "T"), 1, "the key is listed once: {:?}", f.texts);
        assert!(f.says("Maximize or restore the preview pane"), "with its own words");
        assert!(f.says("plugin toggle-pane max-preview"), "and what it runs");
    }

    /// 36.12 through 36.16: `q` closes what is in front, one layer at a time,
    /// and only quits with nothing up.
    ///
    /// The point of the table is that no panel is the odd one out, so they are
    /// walked together: a panel that quit the app instead of closing would be
    /// the v0.36.1 bug again, in a different place.
    #[test]
    fn q_closes_what_is_in_front_and_only_then_quits() {
        // 36.14: the panels with a keymap layer of their own.
        for open in ["~", "w"] {
            let mut s = one(&format!("max-q-{open}"));
            s.typed(open);
            assert!(!matches!(s.app.overlay, crate::app::Overlay::None), "`{open}` opened a panel");
            s.typed("q");
            assert!(matches!(s.app.overlay, crate::app::Overlay::None), "`q` closed it");
            assert!(!s.app.quit, "`q` on a panel must not quit");
        }
        let mut s = one("max-q-spot");
        s.feed(vec![key(egui::Key::Tab, egui::Modifiers::NONE)]);
        assert!(matches!(s.app.overlay, crate::app::Overlay::Spot(_)));
        s.typed("q");
        assert!(matches!(s.app.overlay, crate::app::Overlay::None), "the spotter closes too");
        assert!(!s.app.quit);

        // 36.15: with nothing up, the first press quits.
        let mut s = one("max-q-bare");
        s.typed("q");
        assert!(s.app.quit, "`q` with nothing in front quits");

        // 36.16: both on, three presses -- panel, then columns, then quit.
        let mut s = one("max-q-both");
        s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        s.typed("T");
        assert!(s.app.quick && s.app.max_preview, "both are up");
        s.typed("q");
        assert!(!s.app.quick, "the panel in front went first");
        assert!(s.app.max_preview, "one press does not undo two states");
        assert!(!s.app.quit);
        s.typed("q");
        assert!(!s.app.max_preview, "then the columns");
        assert!(!s.app.quit);
        s.typed("q");
        assert!(s.app.quit, "and then the process");

        // The same three states, walked with `<Esc>`, `<Esc>`, `q`.
        let mut s = one("max-q-escapes");
        s.feed(vec![key(egui::Key::F3, egui::Modifiers::NONE)]);
        s.typed("T");
        s.feed(vec![esc()]);
        assert!(!s.app.quick && s.app.max_preview);
        s.feed(vec![esc()]);
        assert!(!s.app.max_preview);
        assert!(!s.app.quit, "`<Esc>` never quits");
        s.typed("q");
        assert!(s.app.quit);
    }

    /// 36.17, in the half the code and the checklist agree on: `q` in front of
    /// a decision must not quit the app.
    ///
    /// The row also says nothing happens. It does -- any character dismisses a
    /// confirm prompt, `q` included -- so that half is reported rather than
    /// asserted here. See QA-REPORT.md.
    #[test]
    fn a_decision_prompt_is_not_a_way_out_of_the_process() {
        let mut s = one("max-q-confirm");
        s.typed("D");
        assert!(
            matches!(s.app.overlay, crate::app::Overlay::Confirm(_)),
            "a delete asks first",
        );
        s.typed("q");
        assert!(!s.app.quit, "`q` in front of a decision must not quit");

        // A pick list is a text field, so the letter narrows it and the keymap
        // never sees it.
        let mut s = one("max-q-pick");
        s.feed(vec![key(egui::Key::P, ctrl_shift())]);
        assert!(matches!(s.app.overlay, crate::app::Overlay::Pick(_)));
        s.typed("q");
        assert!(matches!(s.app.overlay, crate::app::Overlay::Pick(_)), "the list is still up");
        assert!(!s.app.quit, "and `q` went into the filter");
    }
}

/// TESTING.md section 33: a config warning, and the colour that says it is one.
///
/// The whole section is about a distinction the reader makes with their eyes --
/// yellow for "a line of yours cannot take effect", red for "something failed"
/// -- so until v0.47 it was out of reach of `cargo test` twice over: the colour
/// lives in a stroke and in a galley rather than in a fill, and the state needs
/// a `keymap.toml` of one's own. The second half is what `Screen::with_config`
/// is for: the warnings come out of the same `Keymap::load` that a real
/// `keymap.toml` goes through, from its text, so nothing here reads the disk.
#[cfg(test)]
mod config_warning_frame {
    use super::harness::Screen;
    use crate::config::{keymap::Keymap, Config};

    /// A `keymap.toml` binding `'` to something the defaults already bind it
    /// to, which is 33.1's own example.
    /// One key bound twice in the user's own file. Against a default it would
    /// be an override, not a warning (Q60).
    const ONE_DUPLICATE: &str = "[[mgr.prepend_keymap]]\non = [\"'\"]\nrun = \"plugin bookmarks jump\"\n\
         [[mgr.prepend_keymap]]\non = [\"'\"]\nrun = \"quit\"\n";

    /// Three lines the loader complains about, for 33.2's count.
    const THREE_COMPLAINTS: &str = "[[mgr.prepend_keymap]]\non = [\"'\"]\nrun = \"plugin bookmarks jump\"\n\
         [[mgr.prepend_keymap]]\non = [\"'\"]\nrun = \"quit\"\n\
         [[mgr.prepend_keymap]]\non = [\"<C-F11>\"]\nrun = \"quit\"\n\
         [[mgr.prepend_keymap]]\non = [\"<C-F11>\"]\nrun = \"close\"\n\
         [[mgr.prepend_keymap]]\non = [\"g\"]\nrun = \"quit\"\n";

    /// A window whose config carries exactly the warnings `user` provokes.
    fn screen(label: &str, user: &str) -> (Vec<String>, Screen) {
        let (_km, warnings) = Keymap::load(&[user]);
        assert!(!warnings.is_empty(), "the keymap under test has to provoke one: {user}");
        let cfg = Config { warnings: warnings.clone(), ..Config::load() };
        (warnings, Screen::with_config(cfg, crate::util::test_dir(label)))
    }

    /// A window whose config carries `warnings` verbatim, for the rows that are
    /// about the box rather than about what put the text in it.
    fn saying(label: &str, warnings: Vec<String>) -> Screen {
        let cfg = Config { warnings, ..Config::load() };
        Screen::with_config(cfg, crate::util::test_dir(label))
    }

    /// 33.1: the toast says what cannot take effect, and says it in yellow.
    ///
    /// Red is the whole point of the row -- v0.20.0 put this line up through
    /// `error` and the first person to see it went looking for the failure --
    /// so the check is that nothing in the frame is stroked in the failure
    /// colour, not merely that the warning colour turns up somewhere.
    #[test]
    fn a_duplicate_binding_is_yellow_and_never_red() {
        let (warnings, mut s) = screen("cfg-warn-one", ONE_DUPLICATE);
        let theme = s.app.cfg.theme.clone();
        assert_ne!(theme.warning, theme.progress_error, "a theme drawing both alike says nothing");

        let f = s.draw();
        assert_eq!(
            warnings[0],
            "[mgr] `'` is bound more than once; only `plugin bookmarks jump` (keymap.toml) runs, \
             not `quit` (keymap.toml)",
            "the wording TESTING.md 33.1 quotes",
        );
        assert!(f.says(&format!("Config: {}", warnings[0])), "on screen: {:?}", f.texts);
        assert_eq!(f.ink("Config: "), [theme.warning], "drawn in the warning colour");
        // The border follows the text, so the box says the same thing as the
        // words inside it.
        assert_eq!(f.stroked(theme.warning).len(), 1, "one box, framed in its own colour");
        assert!(
            f.stroked(theme.progress_error).is_empty(),
            "and nothing in the frame is framed as a failure",
        );
    }

    /// 33.2: with more than one, the toast says how many are waiting in `~`.
    #[test]
    fn three_warnings_say_how_many_more_there_are() {
        let (warnings, mut s) = screen("cfg-warn-three", THREE_COMPLAINTS);
        assert_eq!(warnings.len(), 3, "three lines, three complaints: {warnings:?}");

        let f = s.draw();
        let tail = format!("(+{} more, see `~`)", warnings.len() - 1);
        assert_eq!(tail, "(+2 more, see `~`)", "the wording TESTING.md 33.2 quotes");
        assert!(f.says(&tail), "the toast points at the panel: {:?}", f.texts);
        assert_eq!(
            f.texts.iter().filter(|t| t.starts_with("Config: ")).count(),
            1,
            "one toast for the lot, which is why it has to carry a count: {:?}",
            f.texts,
        );
    }

    /// 33.3: `~` lists the config files first and then every warning, all of
    /// them in the same yellow the toast used.
    #[test]
    fn the_panel_lists_the_files_and_then_the_warnings_in_the_same_yellow() {
        let (warnings, mut s) = screen("cfg-warn-panel", THREE_COMPLAINTS);
        let theme = s.app.cfg.theme.clone();
        // The panel's own list: under test it names empty stand-ins, not the
        // directories of whoever runs the suite (#136).
        let dirs = crate::ui::overlay::shown_config_dirs();
        let first_dir = dirs.first().expect("a config directory is searched").display().to_string();

        let f = s.typed("~");
        // Every warning is a row of its own here, unlike the toast.
        for w in &warnings {
            assert_eq!(
                f.inked.iter().filter(|(t, c)| t == w && *c == theme.warning).count(),
                1,
                "`{w}` is a row in the warning colour: {:?}",
                f.texts,
            );
        }
        // Provenance before complaint: "did it read my config" is answered
        // above "and what did it make of it".
        let dir_at = f.inked.iter().position(|(t, _)| t.starts_with(&first_dir));
        let warn_at = f.inked.iter().position(|(t, _)| t == &warnings[0]);
        let (dir_at, warn_at) = (dir_at.expect("the directory is named"), warn_at.unwrap());
        assert!(dir_at < warn_at, "the files come first: {dir_at} then {warn_at}");
    }

    /// 33.5, in the part that does not depend on the machine: a warning that
    /// the config no longer has leaves the panel when the config is re-read.
    ///
    /// What the reload's *toast* says cannot be asserted here, because
    /// `<C-F5>` reads the real machine's config files: on CI there are none and
    /// the line is `Reloaded 0 config file(s)`, on a machine with a `yazi.toml`
    /// of its own it is whatever that file provokes. The panel is a different
    /// matter -- it lists `cfg.warnings`, and the warning this test put there
    /// is one no config file could produce.
    #[test]
    fn a_reload_takes_the_stale_warning_off_the_panel() {
        let mut s = saying("cfg-warn-reload", vec!["[mgr] a warning no file wrote".into()]);
        let theme = s.app.cfg.theme.clone();
        let stale = "[mgr] a warning no file wrote".to_owned();

        let f = s.typed("~");
        assert!(f.inked.iter().any(|(t, c)| *t == stale && *c == theme.warning), "on the panel");
        // Out of the panel first: `config_reload` is a `[mgr]` binding, and the
        // help layer has one of its own that this is not.
        s.feed(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(matches!(s.app.overlay, crate::app::Overlay::None), "the panel is closed");

        let f5 = egui::Event::Key {
            key: egui::Key::F5,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers { ctrl: true, ..Default::default() },
        };
        s.feed(vec![f5]);
        assert!(!s.app.cfg.warnings.contains(&stale), "the re-read config has not got it");

        let f = s.typed("~");
        assert!(
            !f.inked.iter().any(|(t, _)| *t == stale),
            "and the panel no longer lists it: {:?}",
            f.texts,
        );
    }

    /// 33.7, 33.8 and 33.10: a parse error is one line in the corner, says
    /// the rest is in `~`, and stays inside the window.
    ///
    /// The five-line shape is `toml`'s, so the test provokes a real one rather
    /// than writing five lines of its own: `[mgr` with no `]` is the row's own
    /// example, and what came out of the parser is asserted before it is put on
    /// screen. Until Q84 the toast carried all five lines (cut at eight), and
    /// covered half the window and the top of the preview on every start (#174).
    #[test]
    fn a_parse_error_keeps_to_one_line_inside_the_window() {
        let err = toml::from_str::<crate::config::YaziToml>("[mgr\nratio = [1, 3, 4]\n")
            .expect_err("`[mgr` with no `]` does not parse")
            .to_string();
        assert_eq!(err.lines().count(), 5, "the five lines TESTING.md 33.7 counts: {err}");
        assert!(err.lines().next().unwrap().contains("line 1"), "the first line names the line: {err}");

        let mut s = saying("cfg-warn-box", vec![err.clone()]);
        let theme = s.app.cfg.theme.clone();
        let full = s.rect();
        let f = s.draw();
        let drawn = f.texts.iter().find(|t| t.starts_with("Config: ")).expect("the toast is on screen");
        assert_eq!(drawn.lines().count(), 1, "one line: {drawn:?}");
        assert!(drawn.ends_with(" — the rest in `~`"), "and says where the rest is: {drawn:?}");
        assert!(!drawn.contains('^'), "the carets are left to `~`: {drawn:?}");
        let boxes = f.stroked(theme.warning);
        assert_eq!(boxes.len(), 1, "one box");
        let b = boxes[0];
        assert!(full.contains_rect(b), "inside the window: {b:?} in {full:?}");
        assert!(b.top() > 20.0, "below the header rather than over it: {b:?}");
        // Five lines were over 100 high; one line and the padding is under 50.
        assert!(b.height() < 50.0, "one row high, not five: {b:?}");
        assert!(b.width() <= full.width() * 0.5 + 1.0, "no wider than half the window: {b:?}");

        // 33.8: a third of the screen. The line wraps rather than running
        // off, and keeps to the right edge.
        let narrow = saying("cfg-warn-narrow", vec![err.clone()]);
        let thin = narrow.rect().width() / 3.0;
        let mut narrow = narrow.sized(thin, 800.0);
        let window = narrow.rect();
        let f = narrow.draw();
        let b = f.stroked(theme.warning);
        assert_eq!(b.len(), 1, "still one box at {thin} wide");
        let b = b[0];
        assert!(window.contains_rect(b), "still inside: {b:?} in {window:?}");
        assert!(window.right() - b.right() < 20.0, "and still against the right edge: {b:?}");

        // 33.10: however long the message, the toast is its first line.
        let long: String = (1..=12).map(|i| format!("error line {i}\n")).collect();
        let mut s = saying("cfg-warn-clip", vec![long]);
        let f = s.draw();
        let drawn = f
            .texts
            .iter()
            .find(|t| t.starts_with("Config: error line 1"))
            .expect("the toast is on screen");
        assert_eq!(drawn, "Config: error line 1 — the rest in `~`");
    }

    /// 33.4: a real failure standing beside a config warning, in the other
    /// colour.
    ///
    /// The row's own recipe is an opener naming a program that is not
    /// installed, and that failure cannot be provoked here: `exec::shell`
    /// starts a shell, which starts fine, and the "no such program" arrives
    /// later out of the process's own exit -- which is why `launch` keeps
    /// listening for it. So the failure this raises is a different one, chosen
    /// for being synchronous and needing neither a disk nor a `PATH`: `follow`
    /// on a row that is not a link. What the row is actually about is reachable
    /// either way, because the colour is not the failure's, it is the level's.
    ///
    /// Both toasts at once, on purpose. A test for the warning alone passes
    /// while everything in the program is yellow, and it is the *pair* that a
    /// reader tells apart at a glance.
    #[test]
    fn a_failure_beside_the_warning_is_the_other_colour() {
        let (warnings, mut s) = screen("cfg-warn-red", ONE_DUPLICATE);
        let theme = s.app.cfg.theme.clone();
        assert_eq!(warnings.len(), 1, "one warning, so one yellow box: {warnings:?}");
        assert_ne!(theme.warning, theme.progress_error, "a theme drawing both alike says nothing");

        // Something for the cursor to be on that is not a link, so `follow` has
        // a row to refuse rather than an empty listing to ignore.
        let dir = s.app.tab().cwd.clone();
        let plain = crate::fs::Entry {
            path: dir.join("notes.txt"),
            name: "notes.txt".into(),
            ..Default::default()
        };
        s.app.tabs[0].current = crate::core::folder::Folder::from_entries(
            dir,
            std::sync::Arc::new(vec![plain]),
            true,
        );
        s.app.act(crate::config::cmd::Act::Follow);

        let f = s.draw();
        let failed = "Only a symlink can be followed";
        assert!(f.says(failed), "the failure is on screen too: {:?}", f.texts);
        assert_eq!(f.ink(failed), [theme.progress_error], "the failure is red");
        assert_eq!(f.ink("Config: "), [theme.warning], "and the config line is still not");
        // One box framed each way. Either colour alone would pass a test for
        // "the warning colour appears somewhere" while the other had taken it.
        assert_eq!(f.stroked(theme.progress_error).len(), 1, "one box framed as a failure");
        assert_eq!(f.stroked(theme.warning).len(), 1, "and one framed as advice");
    }

    /// 33.9's expectation: the boxes stack downward, each as tall as its own
    /// text, none over the next, and five at most.
    ///
    /// **Not 33.9's recipe.** Breaking three config files does not produce
    /// three boxes: `App::new` raises the first warning and a count of the rest
    /// (33.2), so any number of config problems is one toast. What the row
    /// describes is the toast area's own behaviour, and the messages here are
    /// raised the way a run of failures would raise them. The mismatch is in
    /// `QA-REPORT.md`; nothing about it is fixable from a test.
    ///
    /// Six, so the cap is tested by something being dropped rather than by
    /// counting to five. The oldest is the one to go: the newest message is the
    /// one the reader just caused.
    #[test]
    fn the_boxes_stack_downward_each_its_own_size() {
        let mut s = saying("cfg-warn-stack", Vec::new());
        let theme = s.app.cfg.theme.clone();
        assert!(s.app.toasts.is_empty(), "no config toast, so the count below is the test's own");
        for n in 1..=6 {
            s.app.warn((1..=n).map(|i| format!("error {n} line {i}\n")).collect::<String>());
        }

        let window = s.rect();
        let f = s.draw();
        let boxes = f.stroked(theme.warning);
        assert_eq!(boxes.len(), 5, "five slots for six messages: {:?}", f.texts);
        assert!(!f.says("error 1 line 1"), "and the oldest is the one dropped: {:?}", f.texts);

        // Paint order is newest first, so the tallest is at the top and each
        // box is shorter than the one above it -- which is "sized to its own
        // text" stated as something the frame can be asked.
        for pair in boxes.windows(2) {
            let (upper, lower) = (pair[0], pair[1]);
            assert!(lower.top() >= upper.bottom(), "{lower:?} starts below {upper:?}");
            assert!(lower.height() < upper.height(), "each its own height: {boxes:?}");
        }
        for b in &boxes {
            assert!(window.contains_rect(*b), "all five inside the window: {b:?} in {window:?}");
        }
    }

    /// TESTING.md 33.23 (v0.78.185): 33.22's warning, in a window too narrow
    /// for it, continues on the next rows, indented, every row drawn whole;
    /// `C` copies it as the one line it is.
    #[test]
    fn a_long_warning_wraps_on_the_panel_and_copies_as_one_line() {
        let user = "[[mgr.keymap]]\non = \"<F9>\"\nrun = \"config_reload\"\n";
        let (_km, warnings) = Keymap::load_named(&[("/home/me/.config/filer/keymap.toml", user)]);
        let warning = warnings.iter().find(|w| w.contains("did you mean")).cloned().expect("33.22's warning");
        let cfg = Config { warnings: vec![warning.clone()], ..Config::load() };
        let mut s = Screen::with_config(cfg, crate::util::test_dir("cfg-warn-wrap")).sized(520.0, 700.0);
        let theme = s.app.cfg.theme.clone();

        let f = s.typed("~");
        // The start-up toast says it too, in the same colour; it is not the panel.
        let rows: Vec<&String> =
            f.inked.iter().filter(|(t, c)| *c == theme.warning && !t.starts_with("Config: ")).map(|(t, _)| t).collect();
        assert!(rows.len() > 1, "wider than the panel, so more than one row: {rows:?}");
        assert!(rows[1..].iter().all(|r| r.starts_with("    ") && !r.starts_with("     ")), "indented by four: {rows:?}");
        let joined: String = rows.iter().enumerate().map(|(i, r)| if i == 0 { r.as_str() } else { &r[4..] }).collect();
        assert_eq!(joined, warning, "nothing lost between the rows");
        for r in &rows {
            let drawn = f.drawn(r).unwrap_or_default();
            assert!(!drawn.contains('\u{2026}') && drawn.trim_end().ends_with(r.trim_end()), "{r:?} was cut: {drawn:?}");
            let at = f.placed(r).expect("placed");
            assert!(at.x < s.rect().right(), "{r:?} starts inside the window");
        }

        s.typed("C");
        let copied = crate::exec::get_clipboard().expect("the copy");
        assert!(copied.lines().any(|l| l == warning), "the warning as one line: {copied}");
    }
}

/// TESTING.md section 13's list half: the `->` marker, the type column, and
/// what `g` `f` does with each kind of row.
///
/// The link rows are built here rather than made on disk. A real symlink needs
/// Developer Mode or an elevated shell on Windows -- 13.8 is about exactly that
/// -- so a test that created one would be a test that fails on the runner that
/// matters most. Everything the list and `follow` read is in the entry
/// (`kind` and `link_to`), and filling those in from a `read_link` is the scan
/// worker's job, which is the part of the section these cannot reach.
///
/// The toast's colour is not here either: 13.4 asks for red and the harness
/// records a string, not the ink it was drawn in.
#[cfg(test)]
mod link_rows {
    use super::harness::{Painted, Screen};
    use crate::core::folder::Folder;
    use crate::fs::{Entry, Kind};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// One link row, pointing at `to`.
    fn link(dir: &Path, name: &str, to: PathBuf, to_dir: bool, broken: bool) -> Entry {
        Entry {
            path: dir.join(name),
            name: name.to_owned(),
            kind: Kind::Link { to_dir, broken },
            link_to: Some(to),
            ..Default::default()
        }
    }

    /// A window listing exactly `entries`, with the cursor on the first.
    fn showing(dir: &Path, entries: Vec<Entry>) -> Screen {
        let mut s = Screen::open(dir.to_path_buf());
        s.app.tabs[0].current = Folder::from_entries(dir.to_path_buf(), Arc::new(entries), true);
        s
    }

    /// How many of the frame's strings are exactly `text`.
    ///
    /// Exactly, not `says`: the status line draws the hovered row's type column
    /// too, padded, and `Symlink` is a prefix of `Symlink (relative)` over in
    /// the spot panel. A column of three characters wants the strict form.
    fn drawn(f: &Painted, text: &str) -> usize {
        f.texts.iter().filter(|t| *t == text).count()
    }

    /// `sub/deep.txt` and three links beside it: to that directory, to that
    /// file, and to a name that is not there. A plain file last, as the control.
    ///
    /// The target of the file link is one directory down so that following it
    /// is a real move; a link beside its own target would land where it started
    /// and pass whatever `cd` did.
    fn four(label: &str) -> (PathBuf, Screen) {
        let dir = crate::util::test_dir(label);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("deep.txt"), "deep").unwrap();
        std::fs::write(dir.join("plain.txt"), "x").unwrap();
        let entries = vec![
            link(&dir, "to-dir", dir.join("sub"), true, false),
            link(&dir, "to-file", dir.join("sub").join("deep.txt"), false, false),
            link(&dir, "dead", dir.join("gone.txt"), false, true),
            Entry::from_path(dir.join("plain.txt")).unwrap(),
        ];
        let s = showing(&dir, entries);
        (dir, s)
    }

    /// 13.1: the name carries `->`, and `m` `p` turns the type column to `l`.
    #[test]
    fn a_link_is_marked_in_the_row_and_in_the_type_column() {
        let (_dir, mut s) = four("frame-link-mark");

        let f = s.draw();
        for name in ["to-dir", "to-file", "dead"] {
            assert_eq!(drawn(&f, &format!("{name}  ->")), 1, "{name} is marked: {:?}", f.texts);
        }
        assert_eq!(drawn(&f, "plain.txt"), 1, "and the ordinary file is not: {:?}", f.texts);

        // `m` is the line-mode prefix and `p` its permissions column, where the
        // first character is the kind.
        let f = s.typed("mp");
        assert_eq!(drawn(&f, "lrw"), 3, "three rows read as links: {:?}", f.texts);
        assert_eq!(drawn(&f, "-rw"), 1, "and one as a file: {:?}", f.texts);
    }

    /// 13.2: `g` `f` on a link to a directory opens it.
    #[test]
    fn g_f_on_a_link_to_a_directory_goes_into_it() {
        let (dir, mut s) = four("frame-link-dir");
        let f = s.typed("gf");
        assert_eq!(s.app.tab().cwd, dir.join("sub"), "the tab went to the target");
        assert!(f.says(" 1 sub "), "and the tab chip is named after it: {:?}", f.texts);
    }

    /// 13.3: `g` `f` on a link to a file lands on the file's directory, with the
    /// name left for the cursor.
    ///
    /// The cursor itself arrives with the listing, and no worker runs here, so
    /// what is assertable is the half that does not wait: the tab moved, and the
    /// name is in `memo`, which is where `Tab` restores the cursor from.
    #[test]
    fn g_f_on_a_link_to_a_file_lands_on_its_directory_with_the_name_remembered() {
        let (dir, mut s) = four("frame-link-file");
        s.typed("jgf");
        let sub = dir.join("sub");
        assert_eq!(s.app.tab().cwd, sub, "the tab went to the target's directory");
        assert_eq!(
            s.app.tab().memo.get(&sub).map(String::as_str), Some("deep.txt"),
            "with the file named for the cursor to land on",
        );
    }

    /// 13.4: a broken link names itself rather than going anywhere.
    #[test]
    fn g_f_on_a_broken_link_says_which_one() {
        let (dir, mut s) = four("frame-link-dead");
        let f = s.typed("jjgf");
        assert!(f.says("Broken link: dead"), "the name is in the message: {:?}", f.texts);
        assert_eq!(s.app.tab().cwd, dir, "and the tab stayed where it was");
    }

    /// 13.5: on an ordinary file the key says what it is for.
    ///
    /// Until v0.26.8 it did nothing at all, which from the outside is a key that
    /// is not bound -- so the message is the whole check.
    #[test]
    fn g_f_on_an_ordinary_file_says_what_the_key_is_for() {
        let (dir, mut s) = four("frame-link-plain");
        let f = s.typed("jjjgf");
        assert!(
            f.says("Only a symlink can be followed — a link shows -> after its name"),
            "{:?}", f.texts,
        );
        assert_eq!(s.app.tab().cwd, dir, "and nothing moved");
    }

    /// 13.6: in an empty directory the key says nothing at all.
    ///
    /// Asserted as "the frame did not change", because the check is the absence
    /// of a message and a test naming the message it does not expect would pass
    /// on a different wrong message.
    #[test]
    fn g_f_in_an_empty_directory_says_nothing() {
        let dir = crate::util::test_dir("frame-link-empty");
        let mut s = showing(&dir, Vec::new());
        let before = s.draw().texts;
        let after = s.typed("gf");
        assert_eq!(after.texts, before, "no row, so nothing to say about one");
        assert_eq!(s.app.tab().cwd, dir, "and nowhere to go");
    }
}

/// TESTING.md section 24: names the list has to draw without help.
///
/// Four of the seven rows are here. `Galley::text` hands back the string that
/// was laid out rather than the characters that fit, so `Painted::glyphs` reads
/// the rows instead: a name cut down by `list.rs` (`elide_at`, and `apart` for
/// neighbours) is asked for whole and drawn whole. The quoting 24.4 is about
/// lives in `tsumugi-pane` now, 24.5 wants the recycle bin, and 24.6 is a
/// PowerShell script.
#[cfg(test)]
mod awkward_names {
    use super::harness::Screen;
    use crate::core::folder::Folder;
    use std::path::Path;

    /// A window listing `names`, which are created in the directory first.
    fn showing(dir: &Path, names: &[&str]) -> Screen {
        for n in names {
            std::fs::write(dir.join(n), "x").unwrap();
        }
        let entries = super::panes::listing(dir, names);
        let mut s = Screen::open(dir.to_path_buf());
        s.app.tabs[0].current = Folder::from_entries(dir.to_path_buf(), entries, true);
        s
    }

    /// 24.1: a CJK name reaches the row whole.
    ///
    /// The column arithmetic behind it is not assertable -- that the glyphs are
    /// two cells wide and that the rows line up is what an eye is for -- but a
    /// name mangled on its way to the layout would show up here, and a name
    /// truncated at a byte boundary rather than a character one would not be a
    /// string the assertion could find at all.
    ///
    /// The second assertion is the one that needs the glyphs: `texts` is what
    /// the galley was *asked* for, so a name the column had to cut short is
    /// still in there whole. `drawn` is what came out the other end, which is
    /// what "drawn correctly" means.
    #[test]
    fn cjk_names_are_drawn_whole() {
        let dir = crate::util::test_dir("frame-names-cjk");
        let names = ["日本語のファイル名.txt", "中文文件名.md", "한국어.txt"];
        let f = showing(&dir, &names).draw();
        for n in names {
            assert!(f.texts.iter().any(|t| t == n), "{n} is drawn as itself: {:?}", f.texts);
            assert_eq!(f.drawn(n), Some(n), "and every character of it got a glyph");
        }
    }

    /// 24.2 (Q34): the very long name is cut inside its stem, so it fits its
    /// column and still ends in `name.txt`.
    ///
    /// Until v0.57.0 egui's `overflow_character` cut the *end* and `.txt` was
    /// lost; `list.rs` now does the cutting itself (`elide_at`) and hands egui a
    /// name that fits, so what the galley was asked for is what is drawn.
    #[test]
    fn a_very_long_name_is_cut_down_to_its_column() {
        let dir = crate::util::test_dir("frame-names-long");
        // The fixture's own name, from `scripts/make-fixtures.ps1`: 163
        // characters, which overflows the column at any window this draws at.
        let long = format!("very-{}name.txt", "long-".repeat(30));
        let f = showing(&dir, &[&long]).draw();

        let row = f
            .texts
            .iter()
            .find(|t| t.starts_with("very-long-") && t.contains('…'))
            .unwrap_or_else(|| panic!("the name is laid out elided: {:?}", f.texts));
        assert!(row.ends_with("name.txt"), "with the extension kept: {row:?}");
        assert!(row.chars().count() < long.chars().count(), "and shorter than the name: {row:?}");
        assert_eq!(f.drawn(row), Some(row.as_str()), "and it fits: every character got a glyph");
    }

    /// 24.3: two names differing only in case are two rows.
    ///
    /// On a case-insensitive filesystem -- which is the one the checklist is
    /// written for -- those two paths are one file, so what this can check is
    /// the half that is filer's either way: the list draws back the case it was
    /// handed and does not fold the pair into one row. That both are openable
    /// is still a row for a machine.
    #[test]
    fn two_names_differing_only_in_case_are_two_rows() {
        let dir = crate::util::test_dir("frame-names-case");
        let f = showing(&dir, &["UPPER.TXT", "upper.txt"]).draw();
        assert!(f.says("2 items"), "both are counted: {:?}", f.texts);
        assert_eq!(
            f.texts.iter().filter(|t| t.eq_ignore_ascii_case("upper.txt")).count(), 2,
            "and both are drawn: {:?}", f.texts,
        );
        for n in ["UPPER.TXT", "upper.txt"] {
            assert!(f.texts.iter().any(|t| t == n), "{n} keeps its case: {:?}", f.texts);
        }
    }

    /// Whether `row` is `name` with a middle cut out and `…` in its place.
    fn cut_from(row: &str, name: &str) -> bool {
        let Some((head, tail)) = row.split_once('…') else { return false };
        !head.is_empty() && name.starts_with(head) && name.ends_with(tail) && head.len() + tail.len() < name.len()
    }

    /// 24.7 (Q67): long names that differ only in the middle, in the parent
    /// column, are each cut to the column and no two neighbours read the same.
    ///
    /// The `<State:>` half of the row is a line on stdout and stays with the
    /// machine; what a frame can say is what the column drew.
    #[test]
    fn neighbouring_long_names_in_the_parent_column_read_apart() {
        let top = crate::util::test_dir("frame-names-apart");
        let names = [
            "filer-diagnostics-archive-xx-15484.log",
            "filer-diagnostics-preview-yy-15484.log",
            "filer-diagnostics-session-zz-15484.log",
            "filer-diagnostics-terminal-q-15484.log",
            "room",
        ];
        for n in &names[..4] {
            std::fs::write(top.join(n), "x").unwrap();
        }
        let room = top.join("room");
        std::fs::create_dir_all(&room).unwrap();
        let mut s = showing(&room, &[]);
        let mut parent = Folder::from_entries(top.clone(), super::panes::listing(&top, &names), true);
        assert!(parent.select_name("room"));
        s.app.tabs[0].parent = Some(parent);
        let f = s.draw();

        let rows: Vec<&String> = names[..4]
            .iter()
            .map(|n| {
                f.texts
                    .iter()
                    .find(|t| cut_from(t, n))
                    .unwrap_or_else(|| panic!("{n} is drawn cut down: {:?}", f.texts))
            })
            .collect();
        for r in &rows {
            assert_eq!(f.drawn(r), Some(r.as_str()), "and it fits its column: {r:?}");
        }
        for pair in rows.windows(2) {
            assert_ne!(pair[0], pair[1], "neighbours read apart: {rows:?}");
        }
    }
}

/// Shared groundwork for TESTING.md sections 9, 27 and 43.
///
/// All three are about the *preview*, and a preview is the one thing on screen
/// that no test can simply set up and draw: it exists because a worker answered.
/// So these tests use real files, the real preview thread, and the real channel,
/// and `Screen::settle` turns the frame loop until the answer lands. What that
/// buys over the payload tests in `preview::` is the pane's own width: `cols`
/// and `box_size` are measured by the draw, so a table laid out to the pane and
/// an outline column that only fits at some widths cannot be asked about any
/// other way.
#[cfg(test)]
mod preview_panes {
    pub(super) use super::harness::{Painted, Screen};

    /// A directory holding `files`, with the listing scanned and the cursor on
    /// the first name given.
    ///
    /// The listing comes from the real scanner rather than `from_entries`,
    /// because these tests already have to run the loop for the preview and a
    /// scan landing mid-test is only a hazard for a listing that was faked.
    pub(super) fn on(label: &str, files: &[(&str, &str)]) -> Screen {
        sized(label, files, 1280.0, 800.0)
    }

    /// Wide enough that the preview pane clears `MINIMAP_MIN_COLS`: at 1280 it
    /// is 53 columns and no map is drawn at all, which would make "is the map
    /// mapping the file" unanswerable rather than answered.
    pub(super) fn wide(label: &str, files: &[(&str, &str)]) -> Screen {
        sized(label, files, 1920.0, 1080.0)
    }

    /// The directory these tests list: a `room` *inside* the test directory,
    /// not the test directory itself.
    ///
    /// The extra level is what keeps the parent column out of the way, and that
    /// matters more than it looks. A pane whose folder is still being scanned
    /// draws `…`, exactly as the preview does while it waits, so a parent the
    /// scanner has not finished with puts a second `…` on the frame and
    /// `waiting` counts it. `test_dir`'s own parent is the system temp
    /// directory, which is as big as the machine happens to have made it -- 4000
    /// files is a slow enough scan to lose the race, and that is a test that
    /// passes on an empty `/tmp` and fails on a working one.
    ///
    /// Owning the parent fixes both halves: it holds one entry, so the scan is
    /// over before the first frame, and nothing else writes into it, so the
    /// watcher cannot flag it dirty and send it back to `Loading` halfway
    /// through a test. Every other test in the file shares the temp directory as
    /// a parent, so that last one is not hypothetical.
    pub(super) fn room(label: &str) -> std::path::PathBuf {
        let room = crate::util::test_dir(label).join("room");
        std::fs::create_dir_all(&room).unwrap();
        room
    }

    pub(super) fn sized(label: &str, files: &[(&str, &str)], w: f32, h: f32) -> Screen {
        let dir = room(label);
        for (name, body) in files {
            std::fs::write(dir.join(name), body).unwrap();
        }
        let mut s = open(dir).sized(w, h);
        listed(&mut s, files.len());
        s
    }

    /// A `Screen` on `dir` with the debounce off.
    pub(super) fn open(dir: std::path::PathBuf) -> Screen {
        let mut s = Screen::open(dir);
        s.app.cfg.ui.preview_debounce_ms = 0;
        s
    }

    /// Turn the loop until the listing holds `n` entries *and* the parent column
    /// has been scanned, so that the only `…` a later frame can draw is the
    /// preview's.
    pub(super) fn listed(s: &mut Screen, n: usize) {
        until(s, |s| {
            s.app.tab().current.entries.len() == n
                && s.app.tab().parent.as_ref().is_some_and(|p| {
                    p.state != crate::core::folder::LoadState::Loading
                })
        });
        assert_eq!(s.app.tab().current.entries.len(), n, "the scanner listed the fixtures");
        assert!(
            s.app.tab().parent.as_ref().is_some_and(|p| {
                p.state != crate::core::folder::LoadState::Loading
            }),
            "and the parent column, which draws `…` of its own while it waits",
        );
    }

    /// The table as it was laid out: each line of the payload's `doc`, spans
    /// joined.
    ///
    /// The padding that right-aligns a numeric column is its own span of
    /// spaces, and epaint draws no glyph for one, so `Painted::texts` cannot see
    /// it. The lines here are the ones the frame drew, at the width the frame
    /// measured -- which is the half of section 43 that no test from a string
    /// could reach.
    pub(super) fn table(s: &Screen) -> Vec<String> {
        match &s.app.preview.state {
            crate::app::PreviewState::Ready(crate::preview::Payload::Markdown { doc, .. }) => doc
                .lines
                .iter()
                .map(|l| l.spans.iter().map(|sp| sp.text.as_str()).collect())
                .collect(),
            other => panic!("a table is a Markdown-shaped payload, got {:?}", std::mem::discriminant(other)),
        }
    }

    /// Put the cursor on `name` and turn the loop until its preview is up.
    pub(super) fn look_at(s: &mut Screen, name: &str) -> Painted {
        let at = s.app.active;
        assert!(s.app.tabs[at].current.select_name(name), "`{name}` is in the listing");
        let f = s.settle();
        assert!(s.preview_arrived(), "`{name}`'s preview arrived: {:?}", f.texts);
        f
    }

    /// Put the preview slot back to "this session has never looked at anything".
    ///
    /// Both halves are needed and for different reasons: the slot, so the pane is
    /// not still showing a payload, and the cache, because `request_preview`
    /// answers a hit itself and would go straight to `Ready` without the worker.
    pub(super) fn forget(s: &mut Screen) {
        s.app.preview.cache.clear();
        s.app.preview.key = None;
        s.app.preview.state = crate::app::PreviewState::Empty;
    }

    /// Turn the loop until `done`, or ten seconds.
    ///
    /// `Screen::settle` stops at "the hovered file's preview is up", which is
    /// already true of a re-layout: the pane keeps the old table on screen while
    /// the new one is read, deliberately, so that a resize does not blink
    /// through `…`. A test about the re-layout therefore has to say what it is
    /// waiting for itself.
    pub(super) fn until(s: &mut Screen, mut done: impl FnMut(&Screen) -> bool) -> Painted {
        let mut f = s.turn();
        for _ in 0..1000 {
            if done(s) {
                return f;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            f = s.turn();
        }
        f
    }

    /// Whether the pane is showing the "nothing has arrived yet" placeholder.
    ///
    /// `…` on its own, counted exactly: the tab bar elides a long directory name
    /// with the same character, so `Painted::says` answers yes on every frame
    /// these tests draw and would make the assertion vacuous.
    ///
    /// Counting the whole frame is only sound because `room` owns the parent
    /// column -- a pane still being scanned draws this same placeholder. There is
    /// no narrowing it by area: `Painted` keeps where a *rectangle* was drawn and
    /// only what a string said, so "`…` inside the preview pane" is not a
    /// question it can answer.
    pub(super) fn waiting(f: &Painted) -> bool {
        f.texts.iter().filter(|t| *t == "\u{2026}").count() == 1
    }
}

/// TESTING.md section 9: the outline at the end of a file.
///
/// The section exists because the bug looked intermittent, and the reason it
/// looked that way is the whole of it: an entry only misbehaves when its line
/// sits past the furthest the pane can be scrolled, which needs a document whose
/// *last* heading is near its end. A pane's furthest scroll is `max_offset`, and
/// `max_offset` is written by the draw -- so this is a section that cannot be
/// tested without drawing, which is why it was left to a person with a real
/// window until now.
#[cfg(test)]
mod outline_end_frame {
    use super::panes::{alt, key};
    use super::preview_panes::*;

    /// A document that ends on a heading with one line under it -- the shape
    /// `TESTING-KEYS.md` has, and the one the section asks for.
    fn ends_on_a_heading() -> String {
        let mut t = String::from("# Title\n\nintro\n\n");
        for i in 0..8 {
            t.push_str(&format!("## Chapter {i}\n\n"));
            for j in 0..12 {
                t.push_str(&format!("body {i}-{j}\n"));
            }
            t.push('\n');
        }
        t.push_str("## The last heading\n\none line under it\n");
        t
    }

    /// `<BackTab>`, the key the outline actually answers.
    ///
    /// 9.1 names `<C-o>`, which is in no keymap -- see QA-REPORT.md. The command
    /// is `toggle_outline` and the default binding is Shift+Tab.
    fn focus() -> egui::Event {
        key(egui::Key::Tab, egui::Modifiers { shift: true, ..Default::default() })
    }

    fn down() -> egui::Event {
        key(egui::Key::ArrowDown, egui::Modifiers::NONE)
    }

    /// 9.1 and 9.2: the last entry stops at the end of the file, and holding the
    /// key there changes nothing at all.
    ///
    /// The numbers are the point. The last heading is on line 52 and the pane
    /// can only be scrolled to 40, so an unclamped jump would hand the draw an
    /// offset twelve lines past the content -- one frame drawn from beyond the
    /// end, per repeat, which is exactly what "one bad frame per repeat" was.
    #[test]
    fn the_last_entry_stops_at_the_end_of_the_file() {
        let mut s = on("outline-end", &[("notes.md", &ends_on_a_heading())]);
        look_at(&mut s, "notes.md");

        let entries: Vec<usize> = s.app.outline_entries().iter().map(|e| e.line).collect();
        let max = s.app.preview.max_offset;
        let last = *entries.last().expect("the headings became an outline");
        assert!(
            last > max,
            "the fixture puts its last heading past the furthest scroll ({last} > {max}), \
             which is the only case this section is about",
        );

        s.feed(vec![focus()]);
        assert_eq!(s.app.preview.outline, Some(0), "`<BackTab>` handed the keys over");

        let f = s.typed("G");
        assert_eq!(
            s.app.preview.outline, Some(entries.len() - 1),
            "`G` went to the last entry",
        );
        assert_eq!(
            s.app.tab().preview_offset, max,
            "and the preview stopped at the end of the file rather than at line {last}",
        );
        assert!(f.says("one line under it"), "the last line is on screen: {:?}", f.texts);

        // 9.2: the same frame, over and over. A held key is a repeat of this,
        // and "nothing flashes" is "the next frame is the same frame".
        let steady = f.texts.clone();
        for i in 0..5 {
            let f = s.feed(vec![down()]);
            assert_eq!(s.app.tab().preview_offset, max, "still at the end after {i} more");
            assert_eq!(f.texts, steady, "nothing moved on repeat {i}");
        }
    }

    /// 9.5: walking back up lands on each entry's own line again.
    ///
    /// The clamp is a ceiling, not a new home for the cursor: the two entries
    /// above the last one are also past `max_offset` and also clamp to it, and
    /// the first one that is not has to go to its own line. A fix that had left
    /// the offset pinned would pass the test above and fail here.
    #[test]
    fn walking_back_up_lands_on_each_entry_own_line() {
        let mut s = on("outline-back", &[("notes.md", &ends_on_a_heading())]);
        look_at(&mut s, "notes.md");
        let entries: Vec<usize> = s.app.outline_entries().iter().map(|e| e.line).collect();
        let max = s.app.preview.max_offset;

        s.feed(vec![focus()]);
        s.typed("G");
        for k in (0..entries.len() - 1).rev() {
            s.typed("k");
            assert_eq!(s.app.preview.outline, Some(k), "walked up to entry {k}");
            assert_eq!(
                s.app.tab().preview_offset, entries[k].min(max),
                "entry {k} is on line {}, and the pane stops at {max}", entries[k],
            );
        }
        // The one that matters: an entry well inside the file is on its own line,
        // not on the clamped one.
        let first_inside = entries.iter().position(|&l| l <= max).expect("some entry fits");
        assert!(entries[first_inside] < max, "and that line is not the ceiling");
    }

    /// 9.3: a document whose last heading has plenty of text after it is
    /// unchanged -- it was always correct here.
    ///
    /// Worth its own test rather than a note, because it is the control: if the
    /// clamp ever grew into "always stop at `max_offset`", every jump in a normal
    /// document would land in the wrong place and the test above would not notice.
    #[test]
    fn a_heading_with_the_file_still_below_it_goes_to_its_own_line() {
        let mut t = String::from("# Title\n\n");
        for i in 0..4 {
            t.push_str(&format!("## Chapter {i}\n\n"));
            for j in 0..12 {
                t.push_str(&format!("body {i}-{j}\n"));
            }
            t.push('\n');
        }
        for j in 0..80 {
            t.push_str(&format!("tail {j}\n"));
        }

        let mut s = on("outline-tail", &[("long.md", &t)]);
        look_at(&mut s, "long.md");
        let entries: Vec<usize> = s.app.outline_entries().iter().map(|e| e.line).collect();
        let last = *entries.last().unwrap();
        assert!(
            last < s.app.preview.max_offset,
            "this fixture's last heading has the rest of the file under it",
        );

        s.feed(vec![focus()]);
        let f = s.typed("G");
        assert_eq!(
            s.app.tab().preview_offset, last,
            "so the jump lands on the heading's own line, untouched by the clamp",
        );
        assert!(f.says("tail 0"), "and the text under it is what is on screen: {:?}", f.texts);
    }

    /// 9.4: `<A-j>` at the bottom of a long file is still steady.
    ///
    /// The same ceiling, reached by the other door. `seek` was clamped first and
    /// the outline's jump was missed, so the two paths are worth asserting
    /// together: they are one rule with two callers.
    #[test]
    fn alt_j_at_the_bottom_of_a_long_file_stays_put() {
        let mut s = on("outline-seek", &[("notes.md", &ends_on_a_heading())]);
        look_at(&mut s, "notes.md");
        let max = s.app.preview.max_offset;

        let mut f = s.draw();
        for _ in 0..40 {
            f = s.feed(vec![key(egui::Key::J, alt())]);
        }
        assert_eq!(s.app.tab().preview_offset, max, "`<A-j>` stopped at the end");

        let steady = f.texts.clone();
        for i in 0..5 {
            let f = s.feed(vec![key(egui::Key::J, alt())]);
            assert_eq!(s.app.tab().preview_offset, max, "still there after {i} more");
            assert_eq!(f.texts, steady, "and the frame did not change on repeat {i}");
        }
    }
}

/// TESTING.md section 27: the preview that would not arrive.
///
/// The section reads as a race and is written as one -- "only ever seen once, on
/// a first launch" -- but v0.12.0 found it was not: `on_preview` put the payload
/// in the cache and never on screen, so *every* first look at a file stayed on
/// `…` for ever and only a revisit worked. `preview_delivery` in `app.rs` guards
/// the state machine; what is left, and what these do, is the frame: that the
/// placeholder is what a pane shows while nothing has arrived, and that the
/// file's own text is what it shows once something has.
#[cfg(test)]
mod preview_arrival_frame {
    use super::panes::{alt, key};
    use super::preview_panes::*;

    /// 27.1 and 27.2: a file never opened in this session reaches the screen.
    ///
    /// 27.2 is the case that was broken, and it is not about cold starts: the
    /// cache is empty for this file either way, which is the only condition the
    /// bug needed. So the placeholder frame is asserted first -- otherwise a test
    /// that never saw `…` at all would pass for the wrong reason.
    #[test]
    fn a_file_never_seen_this_session_reaches_the_screen() {
        let body = "the quick brown fox\n".repeat(300);
        let mut s = on("arrive-first", &[("cold.txt", &body)]);

        // Asked, nothing back -- said rather than waited for. Getting the
        // listing up takes turns of the loop, and on a quick machine the
        // preview answers during them, so "has it arrived yet" at this point is
        // a race and not a fact. Emptying the slot and the cache is the state a
        // session that has never seen this file is in, which is 27.2's whole
        // condition; the read itself is still the worker's, through the channel,
        // which is what the frames below are about.
        forget(&mut s);
        s.app.request_preview(false);
        let f = s.draw();
        assert!(
            matches!(s.app.preview.state, crate::app::PreviewState::Loading),
            "the request is out and nothing has come back",
        );
        assert!(waiting(&f), "so the pane is showing the placeholder: {:?}", f.texts);
        assert!(!f.says("quick brown fox"), "and none of the file: {:?}", f.texts);

        let f = s.settle();
        assert!(!waiting(&f), "the placeholder is gone: {:?}", f.texts);
        assert!(f.says("the quick brown fox"), "and the file is on screen: {:?}", f.texts);
    }

    /// 27.3: walking off the file and back is still fine -- it was the cache.
    ///
    /// Asserted as "no worker was needed": one turn of the loop, and the text is
    /// there with no placeholder in between, because `request_preview` sets
    /// `Ready` from the cache itself. That is the path that always worked, and
    /// the one that hid the bug for six versions.
    #[test]
    fn walking_off_the_file_and_back_needs_no_worker() {
        let mut s = on("arrive-cache", &[("alpha.txt", "alpha lives here\n"), ("beta.txt", "beta\n")]);
        look_at(&mut s, "alpha.txt");
        s.typed("j");
        look_at(&mut s, "beta.txt");

        s.typed("k");
        assert_eq!(s.app.tab().current.hovered_name(), Some("alpha.txt"), "walked back");
        let f = s.turn();
        assert!(s.preview_arrived(), "one turn was enough: the cache answered");
        assert!(!waiting(&f), "with no `…` on the way: {:?}", f.texts);
        assert!(f.says("alpha lives here"), "and the right file: {:?}", f.texts);
    }

    /// The order the Windows runner hit, set up on purpose: the first answer
    /// comes back at the default box size, and the frame then measures the pane
    /// and asks again. The pane still shows the first payload, `Ready`, but the
    /// answer the cache will be asked for is not in yet -- so a helper that
    /// called this "arrived" let 27.3 walk away before it was.
    #[test]
    fn a_relayout_in_flight_has_not_arrived() {
        let mut s = on("arrive-relayout", &[("alpha.txt", "alpha lives here\n")]);
        look_at(&mut s, "alpha.txt");
        let real = s.app.preview.box_size;

        forget(&mut s);
        s.app.preview.box_size = (900, 900);
        s.app.request_preview(false);
        // A text payload uploads nothing, so any context will do.
        let ctx = egui::Context::default();
        for _ in 0..1000 {
            s.app.drain_channels(&ctx);
            if matches!(s.app.preview.state, crate::app::PreviewState::Ready(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(s.preview_arrived(), "the default-size answer is in, and cached");

        s.app.preview.box_size = real;
        s.app.request_preview(false);
        assert!(
            matches!(s.app.preview.state, crate::app::PreviewState::Ready(_)),
            "a re-layout keeps the old payload up",
        );
        assert!(!s.preview_arrived(), "but the one asked for is still on its way");
        s.settle();
        assert!(s.preview_arrived(), "until it lands");
    }

    /// 27.4: an image never seen this session zooms from its own fit.
    ///
    /// The same commit killed this and the section says it has never been
    /// exercised. `zoom in` steps from `preview.zoom.unwrap_or(preview.fit)`, and
    /// `fit` is seeded where the payload arrives -- inside the block that was
    /// dead -- so with the bug in place the step came from whatever the last
    /// image had been scaled to. Hence two images: a small one that fits at 100%,
    /// then a large one that does not.
    ///
    /// 27.4 says `+`; the keys are `<A-i>` and `<A-o>`, deliberately, and the
    /// keymap's own comment says why. See QA-REPORT.md.
    #[test]
    fn an_image_never_seen_zooms_from_its_own_fit() {
        // `room` rather than the test directory itself, for the reason its own
        // note gives: the parent column draws the same placeholder this test
        // reads, so the tests own the parent.
        let dir = room("arrive-image");
        image::RgbaImage::from_pixel(32, 24, image::Rgba([1u8, 2, 3, 255]))
            .save(dir.join("small.png"))
            .unwrap();
        image::RgbaImage::from_pixel(1600, 1200, image::Rgba([9u8, 8, 7, 255]))
            .save(dir.join("big.png"))
            .unwrap();
        let mut s = open(dir);
        listed(&mut s, 2);

        let f = look_at(&mut s, "small.png");
        assert!(f.says("32 × 24"), "the small one is up: {:?}", f.texts);
        assert_eq!(s.app.preview.fit, 1.0, "and a picture smaller than the pane is not blown up");

        let f = look_at(&mut s, "big.png");
        assert!(f.says("1600 × 1200"), "the large one is up: {:?}", f.texts);
        assert_eq!(s.app.preview.zoom, None, "walking onto it dropped the last zoom");
        let fit = s.app.preview.fit;
        assert!(fit < 1.0, "it does not fit the pane at 100%: {fit}");

        s.feed(vec![key(egui::Key::I, alt())]);
        let step = s.app.preview.zoom.expect("`<A-i>` set a zoom");
        assert!(
            (step - fit * 1.25).abs() < 1e-4,
            "one step up from *this* picture's fit ({fit}), not from the last one's 100%: {step}",
        );
    }

    /// 19.6: `Ctrl` and the wheel over a picture zooms it about the pointer,
    /// and the plain wheel does not.
    ///
    /// egui hands a turn with `Ctrl` held over as a zoom factor and leaves the
    /// scroll at zero; the pane read the scroll, so the turn did nothing on
    /// either machine (#202). The events here are the ones a window sends, the
    /// modifiers on the wheel event included -- that is what egui looks at.
    #[test]
    fn ctrl_and_the_wheel_zoom_a_picture() {
        let dir = room("wheel-zoom");
        image::RgbaImage::from_pixel(1600, 1200, image::Rgba([9u8, 8, 7, 255]))
            .save(dir.join("big.png"))
            .unwrap();
        let mut s = open(dir);
        listed(&mut s, 1);
        let f = look_at(&mut s, "big.png");
        assert!(f.says("1600 × 1200"), "the picture is up: {:?}", f.texts);
        let fit = s.app.preview.fit;
        // Well inside the preview column, the right-hand part of the window.
        let at = egui::Pos2::new(s.rect().width() * 0.8, s.rect().height() * 0.5);
        let turn = |modifiers| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 50.0),
            phase: egui::TouchPhase::Move,
            modifiers,
        };

        s.feed(vec![egui::Event::PointerMoved(at), turn(egui::Modifiers::NONE)]);
        assert_eq!(s.app.preview.zoom, None, "the plain wheel is not a zoom");

        let ctrl = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        s.feed(vec![egui::Event::PointerMoved(at), turn(ctrl)]);
        let zoom = s.app.preview.zoom.expect("`Ctrl` and the wheel set a zoom");
        assert!(zoom > fit * 1.05, "a turn away from you zooms in: {zoom} from {fit}");
    }

    /// 27.5: ten different files in a row, none revisited, all arrive.
    ///
    /// The cache holds 24, so nothing here is evicted and nothing is a hit
    /// either: ten first looks in a row, which is the run the section asks for
    /// after a restart.
    #[test]
    fn ten_files_in_a_row_all_arrive() {
        let bodies: Vec<(String, String)> = (0..10)
            .map(|i| (format!("f{i}.txt"), format!("this is file number {i}\n")))
            .collect();
        let files: Vec<(&str, &str)> =
            bodies.iter().map(|(n, b)| (n.as_str(), b.as_str())).collect();
        let mut s = on("arrive-ten", &files);
        // Getting the listing up previews whatever the cursor landed on, so the
        // first of the ten would otherwise be a hit and this would be nine.
        forget(&mut s);

        for (name, body) in &bodies {
            let f = look_at(&mut s, name);
            assert!(!waiting(&f), "{name} is not still waiting: {:?}", f.texts);
            assert!(f.says(body.trim_end()), "{name} put its own text up: {:?}", f.texts);
        }
    }
}

/// TESTING.md section 12, as far as a frame reaches: the two messages, the fork,
/// and the rules three of the delete rows share with a rename.
///
/// Most of the section is about `d`, and `d` is a job on the ops worker put back
/// by reading the trash -- the person's real trash, on whichever platform it is,
/// and on macOS one that cannot be read back at all. So 12.1 to 12.5 and 12.9 to 12.12
/// stay with the machine, and not because a frame cannot reach a worker:
/// `Screen::turn` drains the same channels the real loop does, which is how the
/// preview tests run. What it must not do is empty into someone's Recycle Bin,
/// and that is in QA-REPORT.md with what it would take.
///
/// Every id above is mid-line on purpose: `make-testcheck` reads a doc comment
/// that *begins* with one as a claim to have automated it, so a line wrapping
/// onto `12.12` would take that row off the human's list. The comment this
/// replaced did exactly that to 12.9 -- see QA-REPORT.md.
///
/// A rename is the one undoable action that happens on the spot, in
/// `apply_rename`. That is what makes 12.6 assertable at all -- and what lets
/// the rules behind 12.4, 12.8 and 12.9 be driven here, each over a rename
/// rather than a delete, with the rows themselves left standing. QA-REPORT.md
/// also has what 12.8 says versus what the code does.
#[cfg(test)]
mod undo_frame {
    use super::harness::Screen;
    use super::panes::{key, listing};
    use crate::app::{InputKind, InputOverlay, Overlay};
    use crate::core::folder::Folder;
    use std::path::{Path, PathBuf};

    /// One file, listed, with the cursor on it.
    fn one_file(label: &str) -> (PathBuf, Screen) {
        let dir = crate::util::test_dir(label);
        std::fs::write(dir.join("one.txt"), "1").unwrap();
        let mut s = Screen::open(dir.clone());
        s.app.tabs[0].current =
            Folder::from_entries(dir.clone(), listing(&dir, &["one.txt"]), true);
        (dir, s)
    }

    /// `r`'s prompt, answered with `to` and `<Enter>`.
    ///
    /// The prompt is filled in here rather than typed into: it is a native
    /// `TextEdit` whose contents come from egui's focus handling, so typing at
    /// it from a test would be a test of egui. What this does drive is the half
    /// that is filer's -- `confirm_input`, the rename itself, and the undo step
    /// it records.
    fn rename(s: &mut Screen, from: &Path, to: &str) {
        s.app.overlay = Overlay::Input(InputOverlay {
            kind: InputKind::Rename { from: from.to_path_buf() },
            title: "Rename".into(),
            text: to.to_owned(),
            initial_selection: None,
            focused: true,
            completion: Vec::new(),
            completion_at: 0,
        });
        s.feed(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    }

    /// 12.7: `u` on an empty stack says so instead of erring, and `U` has a
    /// message of its own.
    #[test]
    fn nothing_to_undo_is_said_rather_than_failed() {
        let (_dir, mut s) = one_file("frame-undo-empty");
        let f = s.typed("u");
        assert!(f.says("Nothing to undo"), "{:?}", f.texts);

        let (_dir, mut s) = one_file("frame-redo-empty");
        let f = s.typed("U");
        assert!(f.says("Nothing to redo"), "{:?}", f.texts);
        assert!(!f.says("Nothing to undo"), "the two are told apart: {:?}", f.texts);
    }

    /// 12.6: `r` then `u` puts the old name back, and the toast names it.
    #[test]
    fn a_rename_goes_back_under_the_old_name() {
        let (dir, mut s) = one_file("frame-undo-rename");
        rename(&mut s, &dir.join("one.txt"), "two.txt");
        assert!(dir.join("two.txt").exists(), "the rename happened");
        assert_eq!(s.app.undos.undo.len(), 1, "and left a step to take back");

        let f = s.typed("u");
        assert!(dir.join("one.txt").exists(), "the old name is back");
        assert!(!dir.join("two.txt").exists(), "and the new one is gone");
        assert!(f.says("Renamed back to one.txt"), "the toast names it: {:?}", f.texts);
    }

    /// The rule 12.8 is about: a fresh action drops what `u` had put on the way
    /// forward.
    ///
    /// The same file renamed twice; the row's own recipe, which renames
    /// another file, is `renaming_another_file_forks_history` below, and the
    /// create that forks it too is `a_new_file_forks_history_too`.
    #[test]
    fn a_fresh_action_forks_history() {
        let (dir, mut s) = one_file("frame-undo-fork");
        rename(&mut s, &dir.join("one.txt"), "two.txt");
        s.typed("u");
        assert_eq!(s.app.undos.redo.len(), 1, "the undone step is on the way forward");

        rename(&mut s, &dir.join("one.txt"), "three.txt");
        assert!(s.app.undos.redo.is_empty(), "the new action took that away");

        let f = s.typed("U");
        assert!(f.says("Nothing to redo"), "so there is nothing to do again: {:?}", f.texts);
        assert!(dir.join("three.txt").exists(), "and the newest name stands");
        assert!(!dir.join("two.txt").exists(), "the forked one did not come back");
    }

    /// The rule 12.4 states, in the form a frame reaches: `U` does again what
    /// `u` took back, and the toast says which way it went.
    ///
    /// Not 12.4 itself, which is the `U` after a delete and goes back through
    /// the ops worker. Which stack a step lands on is `app::tests`' own ground;
    /// what only a frame can say is that the key, the file on disk and the toast
    /// agree. `undone_label` and `redone_label` are two different sentences over
    /// the same step, so a redo that walked the rename the wrong way would still
    /// produce one of them.
    #[test]
    fn capital_u_does_the_rename_again_and_says_which_way_it_went() {
        let (dir, mut s) = one_file("frame-redo-rename");
        rename(&mut s, &dir.join("one.txt"), "two.txt");
        s.typed("u");
        assert!(dir.join("one.txt").exists(), "`u` put it back");

        let f = s.typed("U");
        assert!(f.says("Renamed to two.txt"), "`U` names where it went: {:?}", f.texts);
        assert!(dir.join("two.txt").exists(), "which is where the file is");
        assert!(!dir.join("one.txt").exists(), "and it is not under the old name");
        assert_eq!(s.app.undos.undo.len(), 1, "the step is back on the way out");
        assert!(s.app.undos.redo.is_empty(), "and off the way forward");

        // So the pair is walkable rather than one-way: `u` takes it back again.
        let f = s.typed("u");
        assert!(f.says("Renamed back to one.txt"), "{:?}", f.texts);
        assert_eq!(std::fs::read_to_string(dir.join("one.txt")).unwrap(), "1");
    }

    /// The rule 12.9 states, in the form a frame reaches: an undo with nowhere
    /// to put the file back says which name is in the way, keeps the step, and
    /// goes through when the way is clear.
    ///
    /// Not 12.9 itself -- that row deletes and restores, and both halves are ops
    /// worker jobs. The guard is the same one either way: `apply_rename` refuses
    /// to write over anything that exists, so `u` blocked by a name taken in the
    /// meantime is the rename half of the same rule. What makes it worth a frame
    /// is the second press: a step dropped on the failed attempt would leave
    /// "Nothing to undo" here, and the file stranded under its new name.
    #[test]
    fn an_undo_blocked_by_a_taken_name_can_be_pressed_again() {
        let (dir, mut s) = one_file("frame-undo-blocked");
        rename(&mut s, &dir.join("one.txt"), "two.txt");
        // Something else takes the old name while the undo is still on offer.
        std::fs::write(dir.join("one.txt"), "in the way").unwrap();

        let f = s.typed("u");
        assert!(f.says("Undo: Already exists: one.txt"), "which name it is: {:?}", f.texts);
        assert_eq!(s.app.undos.undo.len(), 1, "the step stays, to be tried again");
        assert!(s.app.undos.redo.is_empty(), "nothing went forward");
        assert_eq!(
            std::fs::read_to_string(dir.join("one.txt")).unwrap(), "in the way",
            "and the file standing in the way was not written over",
        );
        assert!(dir.join("two.txt").exists(), "so the renamed file has not moved");

        std::fs::remove_file(dir.join("one.txt")).unwrap();
        let f = s.typed("u");
        assert!(f.says("Renamed back to one.txt"), "the second press works: {:?}", f.texts);
        assert_eq!(std::fs::read_to_string(dir.join("one.txt")).unwrap(), "1");
        assert!(!dir.join("two.txt").exists(), "the new name is gone");
    }

    /// `a`'s prompt, opened by the key and answered with `text` and `<Enter>`.
    ///
    /// Filled in rather than typed into, for the reason `rename` gives; what is
    /// asserted is that `a` is the key that opens a create prompt at all.
    fn create(s: &mut Screen, text: &str) {
        s.typed("a");
        let Overlay::Input(ov) = &mut s.app.overlay else {
            panic!("`a` opens a prompt");
        };
        assert!(matches!(ov.kind, InputKind::Create), "and it is the create prompt");
        ov.text = text.to_owned();
        ov.focused = true;
        s.feed(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    }

    /// 12.8: a rename undone, then a rename of *another* file, and `U` has
    /// nothing left to do -- the second rename forked history.
    #[test]
    fn renaming_another_file_forks_history() {
        let (dir, mut s) = one_file("frame-undo-fork-other");
        std::fs::write(dir.join("other.txt"), "2").unwrap();
        s.app.tabs[0].current =
            Folder::from_entries(dir.clone(), listing(&dir, &["one.txt", "other.txt"]), true);
        rename(&mut s, &dir.join("one.txt"), "two.txt");
        s.typed("u");
        assert!(dir.join("one.txt").exists(), "`u` put the first one back");
        assert_eq!(s.app.undos.redo.len(), 1, "and left it on the way forward");

        rename(&mut s, &dir.join("other.txt"), "renamed.txt");
        let f = s.typed("U");
        assert!(f.says("Nothing to redo"), "the redo is gone: {:?}", f.texts);
        assert!(dir.join("one.txt").exists(), "the first file kept its old name");
        assert!(!dir.join("two.txt").exists(), "and was not renamed again");
        assert!(dir.join("renamed.txt").exists(), "while the second rename stands");
    }

    /// 12.8a: a new file from `a` is an undo step of its own, so it forks
    /// history the way a second rename does.
    #[test]
    fn a_new_file_forks_history_too() {
        let (dir, mut s) = one_file("frame-undo-fork-create");
        rename(&mut s, &dir.join("one.txt"), "two.txt");
        s.typed("u");
        assert_eq!(s.app.undos.redo.len(), 1, "the rename is on the way forward");

        create(&mut s, "fresh.txt");
        assert!(dir.join("fresh.txt").exists(), "the file was made");
        let f = s.typed("U");
        assert!(f.says("Nothing to redo"), "and that took the redo away: {:?}", f.texts);
        assert!(dir.join("one.txt").exists(), "so the rename was not done again");
        assert!(!dir.join("two.txt").exists());
    }

    /// 12.17: `u` after `a new/deep/note.txt` takes the file and both folders
    /// made for it, and says how many; `U` makes all three again; a file
    /// written to since is left alone, with an error saying why.
    #[test]
    fn undoing_a_new_file_takes_the_folders_made_for_it() {
        let (dir, mut s) = one_file("frame-undo-create-deep");
        create(&mut s, "new/deep/note.txt");
        let note = dir.join("new").join("deep").join("note.txt");
        assert!(note.is_file(), "the file and its folders were made");

        let f = s.typed("u");
        assert!(f.says("Removed note.txt and 2 folder(s)"), "{:?}", f.texts);
        assert!(!dir.join("new").exists(), "the folders went with the file");

        let f = s.typed("U");
        assert!(note.is_file(), "`U` makes all three again: {:?}", f.texts);

        std::fs::write(&note, "kept").unwrap();
        let f = s.typed("u");
        assert!(f.says("note.txt has been written to since"), "{:?}", f.texts);
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "kept", "the file stays");
        assert_eq!(s.app.undos.undo.len(), 1, "and so does the step");
    }
}

/// The cursor after a rename or a create, which lands only once the rescan the
/// action asked for comes back -- so these run the loop, on a real listing.
#[cfg(test)]
mod cursor_follows_frame {
    use super::panes::key;
    use super::preview_panes::{open, room, until, Screen};
    use crate::app::{InputKind, InputOverlay, Overlay};

    fn answer(s: &mut Screen, kind: InputKind, text: &str) {
        s.app.overlay = Overlay::Input(InputOverlay {
            kind,
            title: String::new(),
            text: text.to_owned(),
            initial_selection: None,
            focused: true,
            completion: Vec::new(),
            completion_at: 0,
        });
        s.feed(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    }

    fn hovered(s: &Screen) -> Option<String> {
        s.app.tab().current.hovered_name().map(str::to_owned)
    }

    /// 12.19: `r` to `zz.txt`, `a` `aa.txt` and `a` `new/deep/n.txt` each take
    /// the cursor with them, to the top and the bottom of a list longer than
    /// the pane and, for the nested one, to the folder `new`.
    #[test]
    fn the_cursor_follows_a_rename_and_a_new_name() {
        let dir = room("frame-cursor-follows");
        for i in 0..30 {
            std::fs::write(dir.join(format!("f{i:02}.txt")), "x").unwrap();
        }
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        let mut s = open(dir.clone());
        until(&mut s, |s| s.app.tab().current.entries.len() == 31 && s.app.settled());
        assert!(s.app.tabs[0].current.select_name("b.txt"), "b.txt is listed");

        answer(&mut s, InputKind::Rename { from: dir.join("b.txt") }, "zz.txt");
        until(&mut s, |s| hovered(s).as_deref() == Some("zz.txt"));
        assert_eq!(hovered(&s).as_deref(), Some("zz.txt"), "onto the new name, at the bottom");

        answer(&mut s, InputKind::Create, "aa.txt");
        until(&mut s, |s| hovered(s).as_deref() == Some("aa.txt"));
        assert_eq!(hovered(&s).as_deref(), Some("aa.txt"), "onto the new file, at the top");

        // Off the top row first: folders sort first, so `new` lands on row 0,
        // and a cursor that merely stayed on row 0 would pass.
        assert!(s.app.tabs[0].current.select_name("f15.txt"));
        answer(&mut s, InputKind::Create, "new/deep/n.txt");
        until(&mut s, |s| hovered(s).as_deref() == Some("new"));
        assert_eq!(hovered(&s).as_deref(), Some("new"), "onto the part of the path in this folder");
        assert!(dir.join("new").join("deep").join("n.txt").is_file());
    }
}

/// TESTING.md section 43: CSV / TSV as a table.
///
/// The section's own note says what is left after `preview::csv`'s unit tests:
/// "real files, from real tools, at a real pane width." The width is the half
/// that matters, and it is the half that had the bug -- `cols == 0` reads as 80
/// further down, so a table whose key left `cols` out sat at 80 columns for ever.
/// A pane's width is measured by the draw, so these run the real worker at the
/// real measured width and then resize the window and watch the table move.
#[cfg(test)]
mod csv_table_frame {
    use super::preview_panes::*;

    const SALES: &str = "region,units,price\nEast,12,3.50\nWest,7,11.25\nNorth,103,0.99\n";

    /// 43.1 and 43.2: an aligned table with a rule, numeric columns to the right,
    /// and a `.tsv` split on tabs rather than commas.
    ///
    /// The alignment is read off the laid-out lines rather than off the frame's
    /// strings: the padding is its own span of spaces and epaint draws no glyph
    /// for one. The rule and the header *are* asserted against the frame, which
    /// is what says the table reached the screen and not merely the payload.
    #[test]
    fn a_csv_is_an_aligned_table_and_a_tsv_splits_on_tabs() {
        let mut s = on("csv-table", &[("sales.csv", SALES), ("tabs.tsv", "a\tb\n1,5\t2\n")]);

        let f = look_at(&mut s, "sales.csv");
        assert_eq!(
            table(&s),
            vec![
                "region │ units │ price",
                "───────┼───────┼──────",
                "East   │    12 │  3.50",
                "West   │     7 │ 11.25",
                "North  │   103 │  0.99",
            ],
            "header, rule, rows -- and `units` and `price` are numeric, so they \
             are padded on the left while `region` is padded on the right",
        );
        assert!(f.says("───────┼───────┼──────"), "the rule is drawn: {:?}", f.texts);
        for cell in ["region", "units", "price", "North", "103", "0.99"] {
            assert!(f.says(cell), "`{cell}` is drawn: {:?}", f.texts);
        }
        assert!(!f.says("region,units,price"), "and not as raw text: {:?}", f.texts);

        // 43.2: the comma inside `1,5` is a character in a field, not a split.
        look_at(&mut s, "tabs.tsv");
        assert_eq!(table(&s), vec!["  a │ b", "────┼──", "1,5 │ 2"]);
    }

    /// 43.3: `M` shows the raw text, and `M` again goes back to the table.
    ///
    /// Through the keymap, because that is the half a unit test cannot have:
    /// `toggle_render` is one command shared with Markdown, and the two views
    /// come out of one payload, so "did the key reach it" and "is the other view
    /// still there" are the same question.
    #[test]
    fn m_switches_between_the_table_and_the_text_it_came_from() {
        let mut s = on("csv-toggle", &[("sales.csv", SALES)]);
        // The rule, not the whole row: a line is drawn one span at a time, so
        // no single string on screen holds a cell and its separator together.
        // The rule is one span, and nothing but a table draws one.
        const RULE: &str = "───────┼───────┼──────";

        let f = look_at(&mut s, "sales.csv");
        assert!(f.says(RULE), "the table is up: {:?}", f.texts);
        assert!(!f.says("region,units,price"), "{:?}", f.texts);

        let f = s.typed("M");
        assert!(!s.app.render_markdown, "`M` asked for the source");
        assert!(f.says("region,units,price"), "which is the file's own line: {:?}", f.texts);
        assert!(!f.says(RULE), "and not the table: {:?}", f.texts);

        let f = s.typed("M");
        assert!(s.app.render_markdown, "`M` again went back");
        assert!(f.says(RULE), "to the table: {:?}", f.texts);
        assert!(!f.says("region,units,price"), "{:?}", f.texts);
    }

    /// 43.4 and 43.5: the table re-lays out when the window is resized, and a
    /// pane too narrow for it squeezes the widest column and wraps the cells.
    ///
    /// This is the fix v0.41.0 shipped, and the only way to see it is to draw
    /// twice at two widths: `cols` is measured by the draw and goes into the
    /// preview's key, so a resize has to produce a *different key* and a second
    /// answer from the worker. A table that had stayed at 80 columns for ever
    /// would give the same lines both times.
    #[test]
    fn the_table_relays_out_when_the_window_is_resized() {
        let wide_row = "description,note\nan extremely long first column value here,short\n";
        let mut s = sized("csv-reflow", &[("wide.csv", wide_row)], 700.0, 800.0);
        look_at(&mut s, "wide.csv");

        let narrow = table(&s);
        let cols = s.app.preview.cols;
        assert!(cols < 40, "the pane really is narrow: {cols} columns");
        // 43.5: squeezed and wrapped, and no line wider than the pane.
        assert!(narrow.len() > 3, "the long cell wrapped over rows: {narrow:?}");
        for line in &narrow {
            assert!(
                crate::preview::cells(line) <= usize::from(cols),
                "`{line}` is {} cells wide in a pane of {cols}",
                crate::preview::cells(line),
            );
        }

        // 43.4: the same file, a wider window.
        let mut s = s.sized(1600.0, 800.0);
        let f = until(&mut s, |s| s.app.preview.cols > cols && table(s) != narrow);
        let wider = table(&s);
        assert!(
            s.app.preview.cols > cols,
            "the draw measured the new width: {} was {cols}", s.app.preview.cols,
        );
        assert_ne!(wider, narrow, "and the table was read again at it");
        assert_eq!(wider.len(), 3, "the long cell fits on one row now: {wider:?}");
        assert!(
            f.says("an extremely long first column value here"),
            "in one piece, on screen: {:?}", f.texts,
        );
    }

    /// 43.6, 43.7, 43.8, 43.10 and 43.13: the files that are awkward rather than
    /// wide.
    ///
    /// All five are parsing, and `preview::csv` tests each from a string. What is
    /// new here is that they are files -- a BOM written to disk and read back, a
    /// record with a newline inside it surviving `\n` on the way through -- and
    /// that the widths are measured against the pane the frame drew.
    #[test]
    fn quoted_fields_a_bom_ragged_rows_and_cjk_all_lay_out() {
        let mut s = wide("csv-odd", &[
            ("quoted.csv", "name,note\nx,\"a, and\nmore\"\n"),
            ("bom.csv", "\u{feff}id,name\n1,a\n"),
            ("ragged.csv", "a,b,c\n1\n2,3\n"),
            ("one.csv", "solo,row\n"),
            ("cjk.csv", "名前,備考\n山田,あい\n"),
        ]);

        // 43.6: one cell, on one row -- the newline inside the quotes is a space.
        look_at(&mut s, "quoted.csv");
        assert_eq!(table(&s), vec!["name │ note", "─────┼────────────", "x    │ a, and more"]);

        // 43.7: the BOM is not a character of the first header.
        let f = look_at(&mut s, "bom.csv");
        // `id` holds a number, so it is a numeric column too and pads on the left.
        assert_eq!(table(&s), vec!["id │ name", "───┼─────", " 1 │ a"]);
        assert!(!f.says("\u{feff}"), "and nothing stray reached the screen: {:?}", f.texts);

        // 43.8: short rows are padded out rather than panicking.
        look_at(&mut s, "ragged.csv");
        assert_eq!(table(&s), vec!["a │ b │ c", "──┼───┼──", "1 │   │ ", "2 │ 3 │ "]);

        // 43.10: one record is a row, and a rule under it would say something
        // untrue about the file.
        look_at(&mut s, "one.csv");
        assert_eq!(table(&s), vec!["solo │ row"]);

        // 43.13: widths in cells, not chars -- four columns of rule per heading.
        let f = look_at(&mut s, "cjk.csv");
        assert_eq!(table(&s), vec!["名前 │ 備考", "─────┼─────", "山田 │ あい"]);
        assert!(f.says("─────┼─────"), "drawn: {:?}", f.texts);
    }

    /// 43.12: a `.csv` that is actually binary is still a hex dump.
    ///
    /// The extension decides the *previewer*, so this is the one row of the
    /// section about what happens when that guess is wrong.
    #[test]
    fn a_csv_that_is_actually_binary_is_still_a_hex_dump() {
        let mut s = on("csv-binary", &[("bin.csv", "x,y\n\u{0}\u{1}\u{2}\u{3}\n")]);
        let f = look_at(&mut s, "bin.csv");
        assert!(
            matches!(
                s.app.preview.state,
                crate::app::PreviewState::Ready(crate::preview::Payload::Binary { .. }),
            ),
            "not a table",
        );
        assert!(f.says("binary · "), "the footer says so: {:?}", f.texts);
        assert!(f.says("78 2c 79 0a"), "and the bytes are on screen: {:?}", f.texts);
    }

    /// 43.11: the minimap, with a table up and with the text up.
    ///
    /// Half of this row does not match the program, and the half that does is the
    /// half worth having. With the table up there is **no minimap at all**:
    /// `ui::preview::draw` returns from the rendered-Markdown branch before any
    /// map is drawn, and a CSV table is a Markdown-shaped payload. `M` brings the
    /// source view, and there the map is of the file's own lines. Reported rather
    /// than called a pass -- see QA-REPORT.md -- and asserted here as what is
    /// drawn today, so that a change either way is visible.
    #[test]
    fn the_minimap_arrives_with_the_text_and_not_with_the_table() {
        let mut s = wide("csv-minimap", &[("sales.csv", &SALES.repeat(40))]);
        let strip = egui::Rect::from_min_max(
            egui::pos2(s.rect().width() - 120.0, 0.0),
            egui::pos2(s.rect().width(), s.rect().height()),
        );
        let f = look_at(&mut s, "sales.csv");
        assert!(s.app.cfg.ui.minimap, "the map is not switched off");
        assert!(
            s.app.preview.cols >= 56,
            "and the pane is wide enough for one: {} columns", s.app.preview.cols,
        );
        assert!(
            f.rects_in(strip).is_empty(),
            "yet no strip is drawn beside the table: {:?}", f.rects_in(strip),
        );

        let f = s.typed("M");
        assert!(!f.rects_in(strip).is_empty(), "the source view has a map");
        assert!(
            f.says("region,units,price"),
            "of the file's own lines, which are what it is beside: {:?}", f.texts,
        );
    }
}

/// `<C-c>` while the shell has the keys is the shell's, not the keymap's.
///
/// egui-winit never emits a key event for the clipboard chords -- it turns them
/// into `Copy` and `Cut` -- so filer puts the chord back by hand. It put it back
/// into the keymap, guarded only on no overlay being open. The terminal is not
/// an overlay, so with a shell focused `[mgr]` `close` ran: the tab closed, and
/// on the last tab filer quit and took the shell with it.
///
/// Section 1 on the Windows machine found it the way anybody would -- by
/// pressing the key that stops a running command.
#[cfg(test)]
mod terminal_chords {
    use super::harness::Screen;

    /// With the terminal focused, the chord must not reach `[mgr]`.
    ///
    /// `quit` is what the old path set, through `Act::Close` on a lone tab, so
    /// it is the flag that says whether the bug is back. The shell receiving
    /// `0x03` needs a pty and belongs to TESTING.md 1.19; what is asserted here
    /// is the half that lost people's work.
    #[test]
    fn ctrl_c_with_the_shell_focused_does_not_close_the_tab() {
        let mut s = Screen::open(crate::util::test_dir("term-chord-c"));
        assert_eq!(s.app.tabs.len(), 1, "one tab, so `close` would quit");

        s.app.term_focus = true;
        s.feed(vec![egui::Event::Copy]);
        assert!(!s.app.quit, "the shell's interrupt is not filer's `close`");
        assert_eq!(s.app.tabs.len(), 1, "and no tab was closed");

        // `Cut` is the same shape and was never checked; `<C-x>` is unbound in
        // the default keymap today, which is luck rather than a guard.
        s.feed(vec![egui::Event::Cut]);
        assert!(!s.app.quit, "nor is `<C-x>`");
    }

    /// And with the list focused it still is the keymap's, which is the whole
    /// reason the chord is put back at all.
    #[test]
    fn ctrl_c_with_the_list_focused_still_closes() {
        let mut s = Screen::open(crate::util::test_dir("term-chord-list"));
        assert!(!s.app.term_focus, "the list has the keys");
        s.feed(vec![egui::Event::Copy]);
        assert!(s.app.quit, "`<C-c>` is `close`, and this is the last tab");
    }
}

#[cfg(test)]
mod term_border_tests {
    use super::*;

    /// The share holds between the two limits, and the limits win outside
    /// them: four rows at least, six rows left to the list at most.
    #[test]
    fn the_terminal_height_follows_the_share_within_its_limits() {
        let (full, chrome, row) = (1000.0, 80.0, 20.0);
        assert_eq!(term_height(full, chrome, row, TERM_SHARE), 350.0);
        assert_eq!(term_height(full, chrome, row, 0.6), 600.0);
        assert_eq!(term_height(full, chrome, row, 0.01), 80.0);
        assert_eq!(term_height(full, chrome, row, 0.95), 800.0);
        // Too short for both: four rows, rather than a panic in `clamp`.
        assert_eq!(term_height(200.0, chrome, row, 0.5), 80.0);
    }

    /// Dropping the border at a ratio of the span gives the terminal the rest
    /// of the span, as a share of the window.
    #[test]
    fn a_dropped_border_gives_the_terminal_the_rest_of_the_span() {
        assert!((term_share(800.0, 0.5, 1000.0) - 0.4).abs() < 1e-6);
        assert!((term_share(800.0, 0.25, 1000.0) - 0.6).abs() < 1e-6);
        assert_eq!(term_share(800.0, 1.0, 1000.0), 0.05);
    }
}

/// 21.22d: the `r` and `E` prompts say a taken name while it is typed, read
/// off the listing (#264).
#[cfg(test)]
mod name_hint {
    use super::preview_panes::{open, room, until};
    use crate::app::{InputKind, InputOverlay, Overlay};

    fn typing(s: &mut super::preview_panes::Screen, kind: InputKind, text: &str) -> Option<String> {
        s.app.overlay = Overlay::Input(InputOverlay {
            kind,
            title: String::new(),
            text: text.to_owned(),
            initial_selection: None,
            focused: true,
            completion: Vec::new(),
            completion_at: 0,
        });
        s.app.name_hint()
    }

    #[test]
    fn a_taken_name_is_said_before_enter() {
        let dir = room("name-hint");
        for name in ["a.txt", "b.txt", "pack.zip"] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        let mut s = open(dir.clone());
        until(&mut s, |s| s.app.tab().current.entries.len() == 3 && s.app.settled());

        let rename = || InputKind::Rename { from: dir.join("a.txt") };
        let hint = typing(&mut s, rename(), "b.txt").unwrap_or_default();
        assert!(hint.contains("b.txt already exists — Enter is refused"), "{hint}");
        assert_eq!(typing(&mut s, rename(), "a.txt"), None, "its own name is no change");
        assert_eq!(typing(&mut s, rename(), "c.txt"), None, "a free name says nothing");
        assert_eq!(typing(&mut s, rename(), ""), None);

        assert!(s.app.tabs[0].current.select_name("a.txt"));
        let hint = typing(&mut s, InputKind::Compress, " pack.zip ").unwrap_or_default();
        assert!(hint.contains("pack.zip already exists — Enter asks"), "{hint}");
        assert_eq!(typing(&mut s, InputKind::Compress, "a.zip"), None);

        assert!(s.app.tabs[0].current.select_name("pack.zip"));
        let hint = typing(&mut s, InputKind::Compress, "pack.zip").unwrap_or_default();
        assert!(hint.contains("pack.zip is being packed — Enter is refused"), "{hint}");

        // Other prompts never say it.
        assert_eq!(typing(&mut s, InputKind::Create, "b.txt"), None);
    }
}
