use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, CursorIcon, FontId, Painter, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::PreviewState;
use crate::config::theme::Theme;
use crate::preview::{cells, outline_cols, Doc, Extent, LineKind, MapRow, Payload, Span, TocEntry};

pub struct PreviewStyle<'a> {
    pub theme: &'a Theme,
    pub font: FontId,
    /// A real bold face. Without one, bold text is overstruck.
    pub bold: Option<FontId>,
    /// Width of one monospace cell, the grid rendered Markdown is laid out on.
    pub cell: f32,
    pub row_h: f32,
    pub wrap: bool,
    /// Show Markdown rendered rather than as highlighted source.
    pub render_markdown: bool,
    /// The outline entry under the cursor while the keys drive the outline.
    pub outline_focus: Option<usize>,
    /// Draw the minimap where there is room for one.
    pub minimap: bool,
    /// How an image is scaled. `None` fits it to the pane.
    pub zoom: Option<f32>,
    /// How far a zoomed image has been dragged from centered.
    pub pan: Vec2,
}

#[derive(Default)]
pub struct Drawn {
    /// Scrollable lines, so the caller can clamp `seek`.
    pub lines: usize,
    /// A click in the outline: the entry, and the line to scroll to.
    pub jump: Option<(usize, usize)>,
    /// The minimap was clicked or dragged: scroll here.
    pub scroll_to: Option<usize>,
}

/// Left margin of text in the pane.
const PAD: f32 = 10.0;
/// Source code keeps a narrower pane to itself: its lines don't reflow around
/// an outline the way rendered Markdown does.
const SOURCE_OUTLINE_MIN_COLS: u16 = 100;
/// Width of the minimap strip. Narrower than this and the bands are a smudge.
const MINIMAP_COLS: u16 = 7;
/// Below this the body needs the width more than it needs a map.
const MINIMAP_MIN_COLS: u16 = 56;
/// Height of one minimap band. Two pixels is enough to read the shape of a
/// file and coarse enough that a thousand lines fit in a pane.
const BAND_H: f32 = 2.0;

pub fn draw(
    ui: &mut Ui,
    rect: Rect,
    state: &PreviewState,
    texture: Option<&egui::TextureHandle>,
    offset: usize,
    st: &PreviewStyle<'_>,
) -> Drawn {
    let painter = ui.painter_at(rect);
    let pane_rows = rows(rect, st);

    let lines = match state {
        PreviewState::Empty => 0,
        PreviewState::Loading => {
            painter.text(
                rect.left_top() + Vec2::new(PAD, 8.0),
                Align2::LEFT_TOP,
                "…",
                st.font.clone(),
                st.theme.fg_dim,
            );
            0
        }
        PreviewState::Dir(_) => 0, // drawn by the caller as a list
        PreviewState::Ready(payload) => match payload {
            Payload::Error(e) => {
                painter.text(
                    rect.left_top() + Vec2::new(PAD, 8.0),
                    Align2::LEFT_TOP,
                    e,
                    st.font.clone(),
                    st.theme.progress_error,
                );
                0
            }
            Payload::Text { lines, map, extent, outline } => {
                let (body, strip) = split_minimap(rect, map, st);
                let mut drawn = code(ui, &painter, body, lines, outline, *extent, offset, st);
                if let Some(strip) = strip {
                    drawn.scroll_to = minimap(ui, &painter, strip, map, offset, rows(body, st), st);
                    minimap_hover(ui, &painter, strip, body, lines, st);
                }
                return drawn;
            }
            Payload::Markdown { doc, source, map, extent } => {
                // Rendered Markdown gets no minimap: `map` describes the source,
                // and a rendered line is not the same line, so the viewport box
                // would point at the wrong place. Its "Contents" column already
                // answers "where am I". That is also why `preview.cols` — the
                // width Markdown is wrapped to — needs no adjusting for this.
                if st.render_markdown {
                    return markdown(ui, &painter, rect, doc, *extent, offset, st);
                }
                let (body, strip) = split_minimap(rect, map, st);
                let lines = text(ui, &painter, body, source, *extent, offset, st);
                let mut scroll_to = None;
                if let Some(strip) = strip {
                    scroll_to = minimap(ui, &painter, strip, map, offset, rows(body, st), st);
                    minimap_hover(ui, &painter, strip, body, source, st);
                }
                return Drawn { lines, jump: None, scroll_to };
            }
            Payload::Binary { lines, total } => {
                let start = offset.min(lines.len().saturating_sub(1));
                let end = (start + pane_rows.saturating_sub(1)).min(lines.len());
                for (i, line) in lines[start..end].iter().enumerate() {
                    painter.text(
                        pos2(rect.left() + PAD, rect.top() + i as f32 * st.row_h),
                        Align2::LEFT_TOP,
                        line,
                        st.font.clone(),
                        st.theme.fg_dim,
                    );
                }
                painter.text(
                    pos2(rect.left() + PAD, rect.bottom() - st.row_h),
                    Align2::LEFT_TOP,
                    format!("binary · {}", crate::util::human_size(*total)),
                    st.font.clone(),
                    st.theme.fg_dim,
                );
                lines.len()
            }
            Payload::Meta { rows: kv } => {
                for (i, (k, v)) in kv.iter().enumerate() {
                    let y = rect.top() + 8.0 + i as f32 * st.row_h;
                    painter.text(
                        pos2(rect.left() + PAD, y),
                        Align2::LEFT_TOP,
                        k,
                        st.font.clone(),
                        st.theme.fg_dim,
                    );
                    painter.text(
                        pos2(rect.left() + 110.0, y),
                        Align2::LEFT_TOP,
                        v,
                        st.font.clone(),
                        st.theme.fg,
                    );
                }
                0
            }
            Payload::Image { source, caption, .. } => {
                if let Some(tex) = texture {
                    let avail = rect.shrink(8.0);
                    // The picture's own size decides the geometry; the texture
                    // is only how finely it is sampled, and a re-decode for a
                    // zoom must not move anything.
                    let (w, h) = (source.0 as f32, source.1 as f32);
                    let fit = crate::app::image_fit(avail.size(), w, h);
                    let zoom = st.zoom.unwrap_or(fit);
                    let size = Vec2::new(w * zoom, h * zoom);
                    // Centred and clipped rather than UV-sliced: zooming out
                    // letterboxes on its own, and panning needs no bookkeeping
                    // about which corner of the texture is showing.
                    let shown = Rect::from_center_size(avail.center() + st.pan, size);
                    painter.with_clip_rect(avail).image(
                        tex.id(),
                        shown,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    // The scale belongs in the caption: whether what is on
                    // screen is the real pixels is the first thing to know.
                    let note = match st.zoom {
                        None => format!("{caption}  ·  fit {:.0}%", fit * 100.0),
                        Some(z) if (z - 1.0).abs() < 0.005 => format!("{caption}  ·  1:1"),
                        Some(z) => format!("{caption}  ·  {:.0}%", z * 100.0),
                    };
                    painter.text(
                        pos2(avail.center().x, avail.bottom() - st.row_h),
                        Align2::CENTER_TOP,
                        note,
                        st.font.clone(),
                        st.theme.fg_dim,
                    );
                }
                0
            }
        },
    };
    Drawn { lines, jump: None, scroll_to: None }
}

/// Take the minimap's strip off the right of the pane.
///
/// There is no strip when it is turned off, when there is nothing worth mapping,
/// or when the pane is too narrow to spare the width — the body needs the room
/// more than the reader needs a map.
fn split_minimap(rect: Rect, map: &[MapRow], st: &PreviewStyle<'_>) -> (Rect, Option<Rect>) {
    if !st.minimap || map.len() < 2 || pane_cols(rect, st) < MINIMAP_MIN_COLS {
        return (rect, None);
    }
    let split = rect.right() - MINIMAP_COLS as f32 * st.cell - 6.0;
    (
        Rect::from_x_y_ranges(rect.left()..=split, rect.y_range()),
        Some(Rect::from_x_y_ranges(split + 6.0..=rect.right(), rect.y_range())),
    )
}

/// The shape of the whole file in a narrow column, with the part on screen
/// framed. Clicking or dragging it scrolls there.
///
/// Where a line sits down the minimap, as a fraction of its height.
///
/// Both the bands and the box marking the viewport go through here, which is
/// the point: when they had a scale each they disagreed, and the picture and
/// the marker pointed at different parts of the same file.
fn at_line(line: usize, lines: usize) -> f32 {
    line.min(lines) as f32 / lines.max(1) as f32
}

/// The line the minimap points at, for a pointer at `y`.
///
/// The inverse of [`at_line`], and clamped at both ends. The bottom pixel of the
/// strip used to come out as `lines` -- one past the end -- which nothing
/// noticed while the only caller fed it straight into `saturating_sub` and had
/// the result re-clamped downstream. A hover card indexes `lines` with it.
fn line_at_y(y: f32, rect: Rect, lines: usize) -> usize {
    if lines == 0 {
        return 0;
    }
    let t = ((y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
    ((t * lines as f32) as usize).min(lines - 1)
}

/// Where the hover card goes: against the strip, beside the pointer, inside the
/// pane. Apart from the painting so the geometry can be tested without a window.
fn hover_card(pointer_y: f32, pane: Rect, strip: Rect, w: f32, h: f32) -> Rect {
    let w = w.min(pane.width() * 0.6);
    let right = strip.left() - 4.0;
    let top = (pointer_y - h / 2.0).clamp(pane.top(), (pane.bottom() - h).max(pane.top()));
    Rect::from_min_size(pos2(right - w, top), Vec2::new(w, h))
}

/// How much of a line the card lays out. `MAX_LINE_CHARS` already bounds the
/// worst case at 2000, but laying out 2000 glyphs on a hover frame is the cost
/// the minimap exists to avoid.
const HOVER_MAX_CHARS: usize = 200;

/// The line under the pointer, painted beside the strip: what a click here would
/// jump to.
///
/// Not part of [`minimap`], which is already at clippy's seven-argument limit
/// and is doing a different job. Nothing is kept between frames except the
/// moment the pointer arrived, and that lives in egui's own scratch space rather
/// than in app state: when a pointer entered a strip is not something the app
/// knows about the file, and holding it in `PreviewSlot` would mean invalidating
/// it on a file change, a payload swap, a resize and a minimap toggle -- four
/// ways to leave a stale card on the screen.
fn minimap_hover(
    ui: &Ui,
    painter: &Painter,
    strip: Rect,
    body: Rect,
    lines: &[Vec<Span>],
    st: &PreviewStyle<'_>,
) {
    let id = ui.id().with("minimap-hover");
    let over = ui.rect_contains_pointer(strip);
    let dragging = ui.ctx().dragged_id() == Some(ui.id().with("minimap"));
    if !over && !dragging {
        ui.data_mut(|d| d.remove_temp::<f64>(id));
        return;
    }
    let Some(p) = ui.ctx().pointer_latest_pos() else { return };
    let now = ui.input(|i| i.time);
    // When the pointer arrived. Measured from entering the strip rather than
    // from the pointer going still, because the card is wanted during a drag
    // too, and a dragging pointer never goes still.
    let since = ui.data_mut(|d| *d.get_temp_mut_or(id, now));
    let delay = f64::from(ui.style().interaction.tooltip_delay);
    if now - since < delay {
        // This app does not repaint continuously, so without this the card would
        // arrive on the next unrelated event instead of when the delay is up.
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(delay - (now - since)));
        return;
    }

    let k = line_at_y(p.y, strip, lines.len());
    let Some(spans) = lines.get(k) else { return };
    let hov = st.theme.preview_hovered;
    let mut job = LayoutJob::default();
    // The number first: a band carries no text by design, so "which line is
    // this" is the question the strip cannot answer on its own.
    job.append(
        &format!("{:>width$} ", k + 1, width = (lines.len().to_string().len()).max(2)),
        0.0,
        format(&Span::default(), st.theme.fg_dim, st),
    );
    let mut budget = HOVER_MAX_CHARS;
    for span in spans {
        if budget == 0 {
            break;
        }
        let text: String = span.text.chars().take(budget).collect();
        budget -= text.chars().count();
        let mut span = span.clone();
        span.underline |= hov.underline;
        span.bold |= hov.bold;
        span.italic |= hov.italic;
        let color = hov.fg.unwrap_or_else(|| span_color(&span, st));
        job.append(&text, 0.0, format(&span, color, st));
    }
    job.wrap.max_width = f32::INFINITY;
    let galley = painter.layout_job(job);

    let card = hover_card(p.y, body, strip, galley.size().x + 12.0, st.row_h + 4.0);
    painter.rect_filled(card, CornerRadius::same(3), hov.bg.unwrap_or_else(|| mix(st.theme.bg, st.theme.fg, 0.06)));
    painter.rect_stroke(card, CornerRadius::same(3), Stroke::new(1.0, st.theme.border), StrokeKind::Inside);
    painter.galley(card.left_top() + Vec2::new(6.0, 2.0), galley, st.theme.fg);
}

/// Deliberately not text: at two pixels a line a glyph is a smudge, and laying
/// out ten thousand of them would cost the frame. Each band is one rectangle
/// spanning the widest line in it, so a block of code reads as a block and a
/// blank run as a gap. Returns the line to scroll to, if it was pointed at.
fn minimap(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    map: &[MapRow],
    offset: usize,
    on_screen: usize,
    st: &PreviewStyle<'_>,
) -> Option<usize> {
    let theme = st.theme;
    painter.rect_filled(rect, CornerRadius::same(3), theme.bg);

    let bands = ((rect.height() / BAND_H).floor() as usize).max(1);
    let per = map.len().div_ceil(bands).max(1);
    // One scale, used by the bands and by the viewport box below. They used to
    // have one each: the bands stacked at a fixed `BAND_H` per chunk while the
    // box divided the line number by the total. Those agree only when the
    // chunking comes out even. Two hundred lines into a hundred slots gives
    // three lines a band and so sixty-seven bands, which draws two thirds of a
    // strip whose box is still measured against the whole of it — the picture
    // and the marker pointing at different places on the same file.
    let at = |line: usize| rect.top() + at_line(line, map.len()) * rect.height();
    let band_h = (at_line(per, map.len()) * rect.height()).max(1.0);
    // One cell of source maps to this much width, so a line of about 80
    // columns fills the strip and anything longer is simply clamped.
    let unit = rect.width() / 80.0;
    for (b, band) in map.chunks(per).enumerate() {
        let Some(lead) = band.iter().filter(|r| r.len > 0).max_by_key(|r| r.len) else {
            continue; // an all-blank band leaves a gap, which is the point
        };
        // The shallowest indent in the band keeps the left edge of a block
        // straight instead of ragged.
        let indent = band.iter().filter(|r| r.len > 0).map(|r| r.indent).min().unwrap_or(0);
        let x0 = (rect.left() + indent as f32 * unit).min(rect.right());
        let x1 = (x0 + lead.len as f32 * unit).min(rect.right());
        let y = at(b * per);
        if y + band_h > rect.bottom() + 0.5 {
            break;
        }
        let color = lead.color.map_or(theme.fg, |[r, g, b]| Color32::from_rgb(r, g, b));
        painter.rect_filled(
            Rect::from_min_max(pos2(x0, y), pos2(x1.max(x0 + 1.0), y + band_h - 0.5)),
            CornerRadius::ZERO,
            color.gamma_multiply(0.7),
        );
    }

    // --- where the pane is looking ---
    let view = Rect::from_x_y_ranges(rect.x_range(), at(offset)..=at(offset + on_screen));
    painter.rect_filled(view, CornerRadius::same(2), theme.hovered_bg.gamma_multiply(0.4));
    painter.rect_stroke(
        view,
        CornerRadius::same(2),
        Stroke::new(1.0, theme.border),
        StrokeKind::Inside,
    );

    // No cursor of its own. A resize cursor here promised something the
    // minimap does not do — it jumps to a line, it does not drag an edge — and
    // the arrow says "click me" perfectly well. Re-examined when the hover card
    // went in and kept: a card that fades in says "there is something here"
    // without the cursor promising a handle to drag.
    let resp = ui.interact(rect, ui.id().with("minimap"), Sense::click_and_drag());
    if !resp.clicked() && !resp.dragged() {
        return None;
    }
    let want = line_at_y(resp.interact_pointer_pos()?.y, rect, map.len());
    // Land with the line pointed at in the middle of the pane, not at its top.
    Some(want.saturating_sub(on_screen / 2))
}

fn rows(rect: Rect, st: &PreviewStyle<'_>) -> usize {
    ((rect.height() / st.row_h).floor() as usize).max(1)
}

/// Cells across the pane, as the worker counts them for Markdown.
fn pane_cols(rect: Rect, st: &PreviewStyle<'_>) -> u16 {
    ((rect.width() - 20.0) / st.cell).max(0.0) as u16
}

/// How far text sits below the top of its row, so it is centered in it.
fn lift(painter: &Painter, st: &PreviewStyle<'_>) -> f32 {
    let glyph_h = painter.layout_no_wrap("M".into(), st.font.clone(), st.theme.fg).size().y;
    ((st.row_h - glyph_h) / 2.0).max(0.0)
}

fn widest(entries: &[TocEntry]) -> usize {
    entries.iter().map(|e| cells(&e.label)).max().unwrap_or(0)
}

/// Width of an outline laid over the text of a pane too narrow for a column.
fn overlay_cols(widest: usize, cols: u16) -> u16 {
    let max = 32.min(cols / 2).max(12);
    (widest.min(100) as u16 + 1).clamp(12, max)
}

/// Where the separator left of an outline `cols` wide goes.
fn outline_sep(rect: Rect, cols: u16, st: &PreviewStyle<'_>) -> f32 {
    rect.right() - 8.0 - cols as f32 * st.cell - 1.5 * st.cell
}

/// Source code, with its outline in a column when the pane is wide enough for
/// both, or laid over the code while the keys drive the outline.
#[allow(clippy::too_many_arguments)]
fn code(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    lines: &[Vec<Span>],
    entries: &[TocEntry],
    extent: Extent,
    offset: usize,
    st: &PreviewStyle<'_>,
) -> Drawn {
    let cols = pane_cols(rect, st);
    let widest = widest(entries);
    let column = (entries.len() >= 2 && cols >= SOURCE_OUTLINE_MIN_COLS).then(|| outline_cols(widest, cols));
    let body = match column {
        Some(width) => {
            let right = outline_sep(rect, width, st) - 0.5 * st.cell;
            Rect::from_x_y_ranges(rect.left()..=right, rect.y_range())
        }
        None => rect,
    };
    let lines = text(ui, &painter.with_clip_rect(body), body, lines, extent, offset, st);
    let jump = match column {
        Some(width) => outline(ui, painter, rect, entries, width, offset, false, st),
        None if st.outline_focus.is_some() && !entries.is_empty() => {
            outline(ui, painter, rect, entries, overlay_cols(widest, cols), offset, true, st)
        }
        None => None,
    };
    Drawn { lines, jump, scroll_to: None }
}

/// Highlighted source, one file line per row.
fn text(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    lines: &[Vec<Span>],
    extent: Extent,
    offset: usize,
    st: &PreviewStyle<'_>,
) -> usize {
    let start = offset.min(lines.len().saturating_sub(1));
    let end = (start + rows(rect, st)).min(lines.len());
    let bold_dx = overstrike(ui, st);
    for (i, line) in lines[start..end].iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let job_for = |bold_only: bool| {
            let mut job = LayoutJob::default();
            job.wrap.max_width = if st.wrap { rect.width() - 16.0 } else { f32::INFINITY };
            job.wrap.max_rows = if st.wrap { 3 } else { 1 };
            job.wrap.break_anywhere = true;
            job.wrap.overflow_character = None;
            for span in line {
                let color = if bold_only && !span.bold { Color32::TRANSPARENT } else { span_color(span, st) };
                job.append(&span.text, 0.0, format(span, color, st));
            }
            job
        };
        let pos = pos2(rect.left() + PAD, rect.top() + i as f32 * st.row_h);
        painter.galley(pos, painter.layout_job(job_for(false)), st.theme.fg);
        // Without a bold face, bold spans are overstruck a pixel over; the
        // identical layout keeps both passes aligned.
        if st.bold.is_none() && line.iter().any(|s| s.bold) {
            painter.galley(pos + Vec2::new(bold_dx, 0.0), painter.layout_job(job_for(true)), st.theme.fg);
        }
    }
    if extent.truncated && end >= lines.len() {
        truncation_note(painter, rect, end - start, extent.total, st);
    }
    lines.len()
}

/// Markdown laid out for reading, with the outline in a column on the right.
fn markdown(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    doc: &Doc,
    extent: Extent,
    offset: usize,
    st: &PreviewStyle<'_>,
) -> Drawn {
    let theme = st.theme;
    let rows = rows(rect, st);
    let left = rect.left() + PAD;
    let right = left + doc.body_cols as f32 * st.cell;
    let x_at = |col: usize| left + col as f32 * st.cell;
    // Text sits in the middle of its row, so code bands and highlights frame it.
    let lift = lift(painter, st);
    let code_bg = mix(theme.bg_alt, theme.fg, 0.07);
    let bold_dx = overstrike(ui, st);

    // Glyphs a little wider than their cells must not spill into the outline.
    let body = painter.with_clip_rect(Rect::from_x_y_ranges(rect.left()..=right + st.cell, rect.y_range()));
    let start = offset.min(doc.lines.len().saturating_sub(1));
    let end = (start + rows).min(doc.lines.len());
    for (i, line) in doc.lines[start..end].iter().enumerate() {
        let y = rect.top() + i as f32 * st.row_h;
        let x0 = x_at(line.indent as usize);
        match line.kind {
            LineKind::Code => {
                let band = Rect::from_min_max(pos2(x0, y), pos2(right, y + st.row_h));
                body.rect_filled(band, CornerRadius::ZERO, code_bg);
            }
            LineKind::Rule => {
                body.hline(x0..=right, y + st.row_h / 2.0, Stroke::new(1.0, theme.border));
            }
            LineKind::Heading(level @ 1..=2) => {
                let color = if level == 1 { theme.fg_dim } else { theme.border };
                body.hline(x0..=right, y + st.row_h - 1.0, Stroke::new(1.0, color));
            }
            _ => {}
        }

        // Each span starts on its own column, so tables stay aligned even
        // where a fallback font's glyphs are not exactly one or two cells.
        let mut col = 0;
        let mut prev_end = left;
        for span in &line.spans {
            let w = cells(&span.text);
            if span.text.trim().is_empty() {
                col += w;
                continue;
            }
            let x = prev_end.max(x_at(col));
            let color = span_color(span, st);
            let galley = painter.layout_job(LayoutJob::single_section(span.text.clone(), format(span, color, st)));
            let size = galley.size();
            let pos = pos2(x, y + lift);
            if span.code {
                let chip = Rect::from_min_size(pos - vec2(2.0, 0.0), vec2(size.x + 4.0, size.y));
                body.rect_filled(chip, CornerRadius::same(3), code_bg);
            }
            if span.bold && st.bold.is_none() {
                body.galley(pos + vec2(bold_dx, 0.0), galley.clone(), color);
            }
            body.galley(pos, galley, color);
            prev_end = x + size.x;
            col += w;
        }
    }
    if extent.truncated && end >= doc.lines.len() {
        truncation_note(painter, rect, end - start, extent.total, st);
    }

    let jump = if doc.toc_cols > 0 && !doc.toc.is_empty() {
        outline(ui, painter, rect, &doc.toc, doc.toc_cols, start, false, st)
    } else if st.outline_focus.is_some() && !doc.toc.is_empty() {
        let cols = overlay_cols(widest(&doc.toc), pane_cols(rect, st));
        outline(ui, painter, rect, &doc.toc, cols, start, true, st)
    } else {
        None
    };
    Drawn { lines: doc.lines.len(), jump, scroll_to: None }
}

/// The outline on the right, `cols` cells wide: the entry being read (or the
/// one under the cursor, while the keys drive the outline) is highlighted and
/// kept in view, and clicking an entry scrolls to it. An `overlay` covers the
/// text beneath it.
#[allow(clippy::too_many_arguments)]
fn outline(
    ui: &Ui,
    painter: &Painter,
    rect: Rect,
    entries: &[TocEntry],
    cols: u16,
    top_line: usize,
    overlay: bool,
    st: &PreviewStyle<'_>,
) -> Option<(usize, usize)> {
    let theme = st.theme;
    let lift = lift(painter, st);
    let width = cols as f32 * st.cell;
    let sep = outline_sep(rect, cols, st);
    let left = sep + 1.5 * st.cell;
    let focused = st.outline_focus.is_some();
    let accent = theme.tab_active.bg.unwrap_or(theme.fg);
    if overlay {
        painter.rect_filled(Rect::from_x_y_ranges(sep..=rect.right(), rect.y_range()), CornerRadius::ZERO, theme.bg);
    }
    let rule = super::focus_rule(theme, focused);
    painter.vline(sep, rect.top() + 4.0..=rect.bottom() - 4.0, Stroke::new(1.0, rule));
    let (font, color) = match (&st.bold, focused) {
        (Some(bold), true) => (bold.clone(), accent),
        (None, true) => (st.font.clone(), accent),
        (_, false) => (st.font.clone(), theme.fg_dim),
    };
    painter.text(pos2(left, rect.top() + lift), Align2::LEFT_TOP, "Contents", font, color);

    let top_level = entries.iter().map(|e| e.level).min().unwrap_or(1);
    let current = match st.outline_focus {
        Some(k) => Some(k.min(entries.len().saturating_sub(1))),
        None => entries.iter().rposition(|e| e.line <= top_line),
    };
    let slots = rows(rect, st).saturating_sub(1).max(1);
    let first = current
        .map_or(0, |c| c.saturating_sub(slots / 2))
        .min(entries.len().saturating_sub(slots));
    let clip = painter.with_clip_rect(Rect::from_x_y_ranges(sep + 1.0..=rect.right(), rect.y_range()));
    let hover = mix(theme.border, theme.fg_dim, 0.4);

    let mut jump = None;
    for (k, entry) in entries.iter().enumerate().skip(first).take(slots) {
        let y = rect.top() + (k - first + 1) as f32 * st.row_h;
        let row = Rect::from_min_size(pos2(left - 4.0, y), vec2(width + 8.0, st.row_h));
        let resp = ui.interact(row, ui.id().with(("outline", k)), Sense::click());
        let here = current == Some(k);
        // Like the file list's cursor: bright only where the keys are.
        if here {
            let fill = if focused { theme.hovered_bg } else { theme.inactive_hovered_bg };
            clip.rect_filled(row, CornerRadius::same(3), fill);
        } else if resp.hovered() {
            clip.rect_stroke(row, CornerRadius::same(3), Stroke::new(1.0, hover), StrokeKind::Inside);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.clicked() {
            jump = Some((k, entry.line));
        }
        let color = if here || entry.level == top_level { theme.fg } else { theme.fg_dim };
        let font_id = match (&st.bold, here) {
            (Some(bold), true) => bold.clone(),
            _ => st.font.clone(),
        };
        let mut job = LayoutJob::single_section(entry.label.clone(), TextFormat { font_id, color, ..Default::default() });
        job.wrap.max_width = width;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        clip.galley(pos2(left, y + lift), clip.layout_job(job), color);
    }
    jump
}

fn truncation_note(painter: &Painter, rect: Rect, shown: usize, total_lines: usize, st: &PreviewStyle<'_>) {
    let y = rect.top() + shown as f32 * st.row_h;
    if y < rect.bottom() {
        painter.text(
            pos2(rect.left() + PAD, y),
            Align2::LEFT_TOP,
            format!("… {total_lines} lines total (truncated)"),
            st.font.clone(),
            st.theme.fg_dim,
        );
    }
}

fn format(span: &Span, color: Color32, st: &PreviewStyle<'_>) -> TextFormat {
    let font_id = match (&st.bold, span.bold) {
        (Some(bold), true) => bold.clone(),
        _ => st.font.clone(),
    };
    let line = Stroke::new(1.0, color);
    TextFormat {
        font_id,
        color,
        italics: span.italic,
        underline: if span.underline { line } else { Stroke::NONE },
        strikethrough: if span.strike { line } else { Stroke::NONE },
        ..Default::default()
    }
}

fn span_color(span: &Span, st: &PreviewStyle<'_>) -> Color32 {
    span.color
        .map(|[r, g, b]| readable(Color32::from_rgb(r, g, b), st.theme.bg, st.theme.fg))
        .unwrap_or(st.theme.fg)
}

/// How far to shift the second pass of a faked bold: about a pixel at
/// ordinary sizes, snapped to whole physical pixels.
fn overstrike(ui: &Ui, st: &PreviewStyle<'_>) -> f32 {
    let ppp = ui.ctx().pixels_per_point();
    (st.font.size / 18.0 * ppp).round().max(1.0) / ppp
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

/// Syntect themes are written against their own background, and some scopes
/// lean on that: base16's `invalid` paints the background color as foreground
/// on top of a red field. We only carry the foreground over, so such a span
/// would come out invisible — a `.d` file's `C:\dev\…` loses every character
/// after a backslash that way. Anything that low-contrast against our own
/// background falls back to the plain text color.
fn readable(fg: Color32, bg: Color32, fallback: Color32) -> Color32 {
    if contrast(fg, bg) < 1.5 {
        fallback
    } else {
        fg
    }
}

fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG relative luminance.
fn luminance(c: Color32) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invisible_spans_fall_back() {
        let bg = Color32::from_rgb(0x1a, 0x1b, 0x26);
        let fg = Color32::from_rgb(0xc0, 0xc5, 0xce);
        // base16-ocean's `invalid` foreground is its own background color.
        let invalid = Color32::from_rgb(0x2b, 0x30, 0x3b);
        assert_eq!(readable(invalid, bg, fg), fg);
        // Ordinary syntax colors, including dim comments, are left alone.
        let comment = Color32::from_rgb(0x65, 0x73, 0x7e);
        assert_eq!(readable(comment, bg, fg), comment);
        let string = Color32::from_rgb(0xa3, 0xbe, 0x8c);
        assert_eq!(readable(string, bg, fg), string);
    }

    /// A pane of `cols` columns, with a cell one point wide so the arithmetic
    /// in the tests is the arithmetic in the code.
    fn pane(cols: f32, theme: &Theme, minimap: bool) -> (Rect, PreviewStyle<'_>) {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(cols + 20.0, 400.0));
        let st = PreviewStyle {
            theme,
            font: FontId::monospace(10.0),
            bold: None,
            cell: 1.0,
            row_h: 10.0,
            wrap: false,
            render_markdown: false,
            outline_focus: None,
            minimap,
            zoom: None,
            pan: Vec2::ZERO,
        };
        (rect, st)
    }

    fn rows_of(n: usize) -> Vec<MapRow> {
        vec![MapRow { indent: 0, len: 10, color: None }; n]
    }

    /// The minimap takes its strip off the right, and the body keeps the rest.
    /// Without this the text would be painted under the map.
    #[test]
    fn the_minimap_narrows_the_body_by_its_own_width() {
        let theme = Theme::default();
        let (rect, st) = pane(120.0, &theme, true);

        let (body, strip) = split_minimap(rect, &rows_of(500), &st);
        let strip = strip.expect("a wide pane has room for a map");

        assert!(body.right() <= strip.left(), "they must not overlap");
        assert_eq!(strip.right(), rect.right(), "the map sits against the edge");
        assert_eq!(strip.width(), MINIMAP_COLS as f32, "one cell is one point here");
        assert_eq!(body.left(), rect.left());
    }

    /// Three ways there is no map, all of them deliberate: turned off, nothing
    /// worth mapping, and a pane that needs its width for the text.
    #[test]
    fn a_narrow_or_empty_or_disabled_pane_gets_no_minimap() {
        let theme = Theme::default();

        let (rect, st) = pane(120.0, &theme, false);
        assert!(split_minimap(rect, &rows_of(500), &st).1.is_none(), "turned off");

        let (rect, st) = pane(120.0, &theme, true);
        assert!(split_minimap(rect, &rows_of(1), &st).1.is_none(), "one line is not a map");

        let (rect, st) = pane(MINIMAP_MIN_COLS as f32 - 1.0, &theme, true);
        let (body, strip) = split_minimap(rect, &rows_of(500), &st);
        assert!(strip.is_none(), "too narrow to spare the columns");
        assert_eq!(body, rect, "and the body keeps all of it");
    }

    /// The outline column asks the same question of the body, not of the pane,
    /// so with a map up a pane has to be wider before an outline fits too.
    #[test]
    fn the_outline_column_is_judged_against_what_the_minimap_left() {
        let theme = Theme::default();
        let cols = SOURCE_OUTLINE_MIN_COLS as f32 + 3.0;

        let (rect, st) = pane(cols, &theme, false);
        assert!(pane_cols(rect, &st) >= SOURCE_OUTLINE_MIN_COLS, "an outline fits on its own");

        let (rect, st) = pane(cols, &theme, true);
        let (body, _) = split_minimap(rect, &rows_of(500), &st);
        assert!(
            pane_cols(body, &st) < SOURCE_OUTLINE_MIN_COLS,
            "and does not once the map has taken its strip"
        );
    }
}

#[cfg(test)]
mod minimap_scale {
    use super::{at_line, hover_card, line_at_y};
    use egui::{pos2, Rect};

    fn strip(top: f32, h: f32) -> Rect {
        Rect::from_min_max(pos2(200.0, top), pos2(220.0, top + h))
    }

    /// The bug the hover card would have tripped over: the bottom pixel of the
    /// strip mapped to `lines`, one past the end. Nothing noticed while the only
    /// caller fed the answer into `saturating_sub` and had it re-clamped further
    /// down; `lines[k]` would have been out of bounds on the first frame.
    #[test]
    fn the_pointer_never_indexes_past_the_end() {
        for lines in [1usize, 2, 7, 99, 100, 101, 4001] {
            for h in [1.0f32, 13.0, 200.0, 999.5] {
                let r = strip(50.0, h);
                assert_eq!(line_at_y(r.bottom(), r, lines), lines - 1, "{lines} lines, {h} tall");
                // Past either end, and exactly on the top edge.
                assert_eq!(line_at_y(r.bottom() + 500.0, r, lines), lines - 1);
                assert_eq!(line_at_y(r.top(), r, lines), 0);
                assert_eq!(line_at_y(r.top() - 500.0, r, lines), 0);
            }
        }
        // An empty file is not a division by zero, and not a panic.
        let r = strip(0.0, 100.0);
        assert_eq!(line_at_y(50.0, r, 0), 0);
    }

    /// The card and the click have to name the same line, for ever: one reads
    /// the pointer through `line_at_y`, the other draws the bands through
    /// `at_line`, and if they ever disagree the card points somewhere the click
    /// does not go.
    #[test]
    fn the_scale_round_trips() {
        let r = strip(10.0, 400.0);
        for n in [1usize, 2, 50, 400, 1000, 4001] {
            for l in [0, 1, n / 3, n / 2, n - 1] {
                let y = r.top() + at_line(l, n) * r.height();
                let back = line_at_y(y, r, n);
                assert!(
                    back.abs_diff(l) <= 1,
                    "{n} lines: line {l} drawn at {y} reads back as {back}"
                );
            }
        }
    }

    /// The card is painted with the pane's own painter, so a mistake here would
    /// be clipped rather than visible — which is exactly why it is asserted.
    #[test]
    fn the_hover_card_stays_in_the_pane() {
        let pane = Rect::from_min_max(pos2(0.0, 100.0), pos2(200.0, 500.0));
        let strip = Rect::from_min_max(pos2(200.0, 100.0), pos2(220.0, 500.0));
        for y in [-100.0f32, 100.0, 101.0, 300.0, 499.0, 500.0, 900.0] {
            let card = hover_card(y, pane, strip, 80.0, 18.0);
            assert!(card.right() <= strip.left(), "y={y}: {card:?} runs into the strip");
            assert!(card.top() >= pane.top() - 0.01, "y={y}: {card:?} above the pane");
            assert!(card.bottom() <= pane.bottom() + 0.01, "y={y}: {card:?} below the pane");
        }
        // A line too long for the pane is capped rather than drawn off the side.
        let wide = hover_card(300.0, pane, strip, 10_000.0, 18.0);
        assert!(wide.width() <= pane.width() * 0.6 + 0.01, "{wide:?}");
        assert!(wide.left() >= pane.left() - 0.01, "{wide:?}");
    }

    /// The bands have to reach the bottom of the strip. They did not: sized at
    /// a fixed height per chunk, they stopped wherever the chunking ran out,
    /// while the viewport box kept using the full height — so on a file of the
    /// wrong length the drawing filled two thirds of the strip and the box
    /// floated past the end of it.
    #[test]
    fn the_bands_fill_the_strip() {
        for lines in [1usize, 2, 7, 99, 100, 101, 202, 1000, 4001, 12345] {
            for bands in [1usize, 17, 100, 377] {
                let per = lines.div_ceil(bands).max(1);
                let chunks = lines.div_ceil(per);
                let last_top = at_line((chunks - 1) * per, lines);
                let bottom = last_top + at_line(per, lines);
                assert!(
                    bottom >= 1.0 - f32::EPSILON,
                    "lines {lines} bands {bands}: strip ends at {bottom}, not 1.0",
                );
            }
        }
    }

    /// The ends are the ends.
    #[test]
    fn the_scale_spans_nought_to_one() {
        assert_eq!(at_line(0, 500), 0.0);
        assert_eq!(at_line(500, 500), 1.0);
        // Past the end clamps rather than running off the strip.
        assert_eq!(at_line(9999, 500), 1.0);
    }

    /// An empty preview must not divide by its own length.
    #[test]
    fn no_lines_is_not_a_division_by_zero() {
        assert!(at_line(0, 0).is_finite());
        assert!(at_line(5, 0).is_finite());
    }
}
