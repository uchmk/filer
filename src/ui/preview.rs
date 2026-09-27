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

/// Fixtures shared by TESTING.md sections 38 and 42.
///
/// Both are about what is drawn beside the preview -- the outline's rule, the
/// minimap's strip and the card against it -- so both need the same window: one
/// wide enough that the pane can spare a column for the outline *and* seven for
/// the map, with a payload already delivered because the harness runs no
/// workers.
#[cfg(test)]
mod chrome {
    use crate::preview::{Extent, MapRow, Payload, Span, TocEntry};
    use crate::ui::harness::{Painted, Screen};
    use egui::Rect;

    /// Wide enough for the outline column. The preview pane has to clear
    /// `SOURCE_OUTLINE_MIN_COLS` *after* the map has taken its seven, which
    /// 1920 does not: the column appears between 2200 and 2560.
    pub(super) const WIDE: f32 = 2560.0;

    /// A window on one long file, with its preview already in place.
    ///
    /// `preview.key` is set as well as `preview.state`, because `<BackTab>`
    /// asks `preview_ready()` first -- a payload whose key names no file looks
    /// to the app like somebody else's preview, and the key does nothing.
    pub(super) fn screen(label: &str, state: crate::app::PreviewState) -> Screen {
        let dir = crate::util::test_dir(label);
        let file = dir.join("long.rs");
        std::fs::write(&file, "x").unwrap();
        let entries = std::sync::Arc::new(vec![crate::fs::Entry::from_path(file.clone()).unwrap()]);
        let mut s = Screen::open(dir.clone()).sized(WIDE, 1080.0);
        s.app.tabs[s.app.active].current =
            crate::core::folder::Folder::from_entries(dir.clone(), entries, true);
        s.app.preview.key = Some(crate::preview::Key {
            path: file,
            len: 1,
            mtime: None,
            box_size: (0, 0),
            cols: 0,
            n: 0,
        });
        s.app.preview.state = state;
        s
    }

    /// `n` numbered lines, one span each.
    pub(super) fn plain(n: usize) -> Vec<Vec<Span>> {
        (0..n).map(|i| vec![Span { text: format!("line {i}"), ..Default::default() }]).collect()
    }

    /// Rows for the map: a shape that varies, so the bands are not one block.
    pub(super) fn rows(n: usize) -> Vec<MapRow> {
        (0..n).map(|i| MapRow { indent: (i % 8) as u16, len: 40, color: None }).collect()
    }

    /// A text payload of `lines`, mapped and with `outline` as its contents.
    pub(super) fn source(
        lines: Vec<Vec<Span>>,
        outline: Vec<TocEntry>,
        total: usize,
    ) -> crate::app::PreviewState {
        let n = lines.len();
        crate::app::PreviewState::Ready(Payload::Text {
            map: rows(n),
            lines,
            extent: Extent { truncated: total != n, total },
            outline,
        })
    }

    /// Six entries, which is two more than the column needs to appear.
    pub(super) fn toc() -> Vec<TocEntry> {
        (0..6).map(|i| TocEntry { level: 1, line: i * 40, label: format!("fn item{i}") }).collect()
    }

    /// A chord, the way the window delivers one.
    pub(super) fn key(k: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    pub(super) fn ctrl() -> egui::Modifiers {
        egui::Modifiers { ctrl: true, ..Default::default() }
    }

    /// `<BackTab>`, which is how Shift and Tab reach the keymap.
    pub(super) fn back_tab() -> egui::Event {
        key(egui::Key::Tab, egui::Modifiers { shift: true, ..Default::default() })
    }

    pub(super) fn moved(to: egui::Pos2) -> egui::Event {
        egui::Event::PointerMoved(to)
    }

    pub(super) fn button(at: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Where the minimap's strip was drawn.
    ///
    /// Found rather than computed, because the arithmetic that puts it there is
    /// what the frame is being asked about. The viewport box is the one
    /// rectangle in `hovered_bg` at four tenths, and it spans the strip's width
    /// exactly, so it names the column; the strip itself is the `bg` rectangle
    /// in that column.
    pub(super) fn strip(f: &Painted, theme: &crate::config::Theme) -> Rect {
        let tint = theme.hovered_bg.gamma_multiply(0.4);
        let x = f.filled(tint).first().expect("the minimap's viewport box").x_range();
        f.filled(theme.bg).into_iter().find(|r| r.x_range() == x).expect("the strip behind it")
    }

    /// The colour the hover card is filled with, which is what makes it
    /// findable: a card is one rectangle and nothing else in the frame is that
    /// shade.
    pub(super) fn card_fill(theme: &crate::config::Theme) -> egui::Color32 {
        theme.preview_hovered.bg.unwrap_or_else(|| super::mix(theme.bg, theme.fg, 0.06))
    }

    /// Hover `y` on the strip and hold still until the card is due.
    ///
    /// Two frames: one that delivers the pointer, and one a second later, since
    /// the card waits `tooltip_delay` from the moment the pointer arrived.
    pub(super) fn hover(s: &mut Screen, at: egui::Pos2) -> Painted {
        s.feed(vec![moved(at)]);
        s.wait(1.0).draw()
    }

    /// The card's own text: the only string starting with the line number, which
    /// the card draws right-aligned ahead of the line itself.
    pub(super) fn card_text(f: &Painted, line: usize, of: usize) -> Option<&String> {
        let lead = format!("{:>width$} ", line, width = of.to_string().len().max(2));
        f.texts.iter().find(|t| t.starts_with(&lead))
    }
}

/// TESTING.md section 38: the rule that says which pane has the keys.
///
/// "Colour, so it needs eyes" is what the section says, and it was right until
/// the harness learned to read a stroke: a rule is a one-pixel line, which
/// carries no fill at all. What the frame can now answer is the part the
/// section is actually about -- that the two panes give *one* answer, in one
/// colour, and that exactly one of them is lit at a time.
#[cfg(test)]
mod focus_rule_frame {
    use super::chrome::*;

    /// 38.3 and 38.8: the outline's rule is the accent while it has the keys
    /// and the border when it has not, and a theme with no accent still tells
    /// the two apart.
    #[test]
    fn the_outlines_rule_is_accent_only_while_it_has_the_keys() {
        let mut s = screen("focus-outline", source(plain(400), toc(), 400));
        let theme = s.app.cfg.theme.clone();
        let accent = crate::ui::focus_rule(&theme, true);
        let border = crate::ui::focus_rule(&theme, false);
        assert_ne!(accent, border, "a theme drawing both alike would say nothing");

        let f = s.draw();
        assert!(f.says("Contents"), "the column is on screen to begin with: {:?}", f.texts);
        assert!(f.stroked(accent).is_empty(), "and its rule is not lit: {:?}", f.stroked(accent));
        // The rule is vertical and as tall as the pane, which is what tells it
        // from the borders every panel draws.
        let rule = |c| -> Vec<egui::Rect> {
            let mut v: Vec<egui::Rect> = f.stroked(c);
            v.retain(|r| r.width() < 1.0 && r.height() > 500.0);
            v
        };
        assert_eq!(rule(border).len(), 1, "one vertical rule, in the border colour");

        s.feed(vec![back_tab()]);
        assert!(s.app.preview.outline.is_some(), "`<BackTab>` handed it the keys");
        let f = s.draw();
        let lit: Vec<egui::Rect> =
            f.stroked(accent).into_iter().filter(|r| r.width() < 1.0).collect();
        assert_eq!(lit.len(), 1, "now exactly one rule is accent: {lit:?}");
        assert!(lit[0].height() > 500.0, "and it is the outline's, down the pane: {:?}", lit[0]);

        // 38.8: a theme that sets no `tab_active` background falls back to the
        // foreground rather than to the border, so the signal survives it.
        std::sync::Arc::make_mut(&mut s.app.cfg.theme).tab_active.bg = None;
        let theme = s.app.cfg.theme.clone();
        let f = s.draw();
        assert_eq!(crate::ui::focus_rule(&theme, true), theme.fg);
        let lit: Vec<egui::Rect> = f
            .stroked(theme.fg)
            .into_iter()
            .filter(|r| r.width() < 1.0 && r.height() > 500.0)
            .collect();
        assert_eq!(lit.len(), 1, "still lit, in the foreground colour: {lit:?}");
    }

    /// 38.1, 38.2, 38.6 and 38.9: the terminal's rule follows the keys, and the
    /// mouse moves them as well as `<C-t>` does.
    ///
    /// This one starts a real shell, because the pane is not drawn at all
    /// without one -- `app.term` is what the layout measures. That is what
    /// `<C-t>` does on a real machine too, so it is the same door; the shell is
    /// never typed at and the PTY is shut down when the `Terminal` is dropped.
    #[test]
    fn the_terminals_rule_follows_the_keys_and_the_mouse() {
        let mut s = screen("focus-term", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let accent = crate::ui::focus_rule(&theme, true);
        let border = crate::ui::focus_rule(&theme, false);
        let cursor = theme.cwd.fg.unwrap_or(theme.fg);
        let full = s.rect();

        let f = s.feed(vec![key(egui::Key::T, ctrl())]);
        assert!(s.app.term.is_some(), "`<C-t>` opened the pane and started a shell");
        assert!(s.app.term_focus, "and gave it the keys");
        // The pane's rule runs along its top, the full width of the window.
        // Which y that is has to come out of the frame rather than be assumed:
        // the header and the status bar draw full-width rules of their own in
        // the border colour, so "a horizontal rule" is not enough to name this
        // one by.
        let top = |f: &crate::ui::harness::Painted, c| -> Vec<egui::Rect> {
            let mut v: Vec<egui::Rect> = f.stroked(c);
            v.retain(|r| r.height() < 1.0 && r.width() > full.width() - 1.0);
            v
        };
        let lit = top(&f, accent);
        assert_eq!(lit.len(), 1, "38.1: the rule along the top is accent: {lit:?}");
        let rule_y = lit[0].top();
        assert!(rule_y > full.center().y, "the pane is the bottom third: {rule_y}");
        let at_pane = |f: &crate::ui::harness::Painted, c| -> Vec<egui::Rect> {
            top(f, c).into_iter().filter(|r| (r.top() - rule_y).abs() < 0.5).collect()
        };
        // 38.9: the cell cursor is filled while the pane has the keys. One cell,
        // so it is told from every other rectangle by being cell-sized.
        let cell = |f: &crate::ui::harness::Painted| -> usize {
            f.filled(cursor).iter().filter(|r| r.width() < 20.0 && r.height() < 30.0).count()
        };
        assert_eq!(cell(&f), 1, "the cursor is a filled cell");

        // 38.2: the keys go back to the list and the rule goes grey with them.
        let f = s.feed(vec![key(egui::Key::T, ctrl())]);
        assert!(!s.app.term_focus, "the second press handed them back");
        assert!(s.app.term.is_some(), "without closing the pane");
        assert!(top(&f, accent).is_empty(), "38.2: nothing is accent now");
        assert_eq!(at_pane(&f, border).len(), 1, "the same rule, in the border colour");
        assert_eq!(cell(&f), 0, "and the cell cursor is hollow");
        let hollow: Vec<egui::Rect> = f
            .stroked(theme.fg_dim)
            .into_iter()
            .filter(|r| r.width() < 20.0 && r.height() < 30.0)
            .collect();
        assert_eq!(hollow.len(), 1, "38.9: outlined rather than gone: {hollow:?}");

        // 38.6: a click inside the pane does what the key does. Well below the
        // rule, so it cannot be landing on the list above it.
        let inside = egui::pos2(full.center().x, rule_y + 40.0);
        s.feed(vec![moved(inside), button(inside, true)]);
        s.feed(vec![button(inside, false)]);
        assert!(s.app.term_focus, "38.6: the click took the keys");
        // The next frame, not that one: the pane reads `term_focus` to colour
        // its rule and only then asks whether it was clicked, so the click is
        // answered on the repaint it asks for. Nobody sees the frame in
        // between; a test drawing only the one would see nothing else.
        let f = s.draw();
        assert_eq!(at_pane(&f, accent).len(), 1, "and the rule followed it");
    }

    /// 38.4, 38.5 and 38.7: with both panes on screen exactly one rule is lit,
    /// and the two read one definition of what "lit" means.
    #[test]
    fn with_both_panes_open_exactly_one_rule_is_accent() {
        let mut s = screen("focus-both", source(plain(400), toc(), 400));
        let theme = s.app.cfg.theme.clone();
        let accent = crate::ui::focus_rule(&theme, true);
        let border = crate::ui::focus_rule(&theme, false);
        let full = s.rect();
        // The outline's rule is vertical and as tall as the pane; the
        // terminal's is horizontal and as wide as the window. The header and
        // the status bar draw wide rules of their own in the border colour, so
        // the terminal's is picked out by the y the accent case reports.
        let down = |f: &crate::ui::harness::Painted, c| -> Vec<egui::Rect> {
            f.stroked(c).into_iter().filter(|r| r.width() < 1.0 && r.height() > 500.0).collect()
        };
        let across = |f: &crate::ui::harness::Painted, c| -> Vec<egui::Rect> {
            f.stroked(c)
                .into_iter()
                .filter(|r| r.height() < 1.0 && r.width() > full.width() - 1.0)
                .collect()
        };

        s.feed(vec![back_tab()]);
        assert!(s.app.preview.outline.is_some(), "the outline has the keys");
        s.feed(vec![key(egui::Key::T, ctrl())]);
        // Opening the terminal takes the keys off the outline, which is what
        // keeps 38.4 true: nothing has to choose between two lit rules,
        // because only one pane is ever focused.
        assert!(s.app.term_focus && s.app.preview.outline.is_none(), "the keys moved over");
        let f = s.draw();
        assert!(f.says("Contents"), "38.4: the outline is still on screen: {:?}", f.texts);
        let lit = across(&f, accent);
        assert_eq!(lit.len(), 1, "the terminal's rule is accent: {lit:?}");
        let rule_y = lit[0].top();
        let at_pane = |f: &crate::ui::harness::Painted, c| -> Vec<egui::Rect> {
            across(f, c).into_iter().filter(|r| (r.top() - rule_y).abs() < 0.5).collect()
        };
        assert!(down(&f, accent).is_empty(), "and it is the only one: {:?}", down(&f, accent));
        assert_eq!(down(&f, border).len(), 1, "the outline's is grey");

        // 38.5: the other way round.
        s.feed(vec![key(egui::Key::T, ctrl())]);
        s.feed(vec![back_tab()]);
        assert!(!s.app.term_focus && s.app.preview.outline.is_some(), "the keys moved back");
        let f = s.draw();
        assert_eq!(down(&f, accent).len(), 1, "now the outline's is accent");
        assert!(at_pane(&f, accent).is_empty(), "and the terminal's is not");
        assert_eq!(at_pane(&f, border).len(), 1, "the terminal's is grey");

        // 38.7, in the part that does not need a file on disk: both rules read
        // `tab_active`, so changing it once changes both. A `theme.toml` and a
        // `<C-F5>` would arrive at the same `Theme`; this sets it there.
        let red = egui::Color32::from_rgb(255, 0, 0);
        std::sync::Arc::make_mut(&mut s.app.cfg.theme).tab_active.bg = Some(red);
        let f = s.draw();
        assert_eq!(down(&f, red).len(), 1, "the outline's rule turned red: {:?}", f.strokes);
        s.feed(vec![key(egui::Key::T, ctrl())]);
        let f = s.draw();
        assert_eq!(at_pane(&f, red).len(), 1, "and so does the terminal's, from the same entry");
    }
}

/// TESTING.md section 42: the card against the minimap's strip.
///
/// The geometry and the clamp had unit tests from the start; what had nobody was
/// the wiring -- that a pointer held on the strip really does produce a card,
/// where, after how long, and carrying which line. All of that is reachable
/// because the harness can deliver egui's own pointer events and move the clock
/// the delay is measured against.
#[cfg(test)]
mod minimap_hover_frame {
    use super::chrome::*;

    /// 42.1 and 42.2: a pointer held on the strip gets a card naming the line,
    /// and the card keeps off the strip and inside the pane.
    ///
    /// The delay is egui's `tooltip_delay`, which is half a second by default --
    /// not the "about 0.4 s" the row says. The check is that the card is *not*
    /// there a fifth of a second in and *is* there after the delay, rather than
    /// a number of its own, so a themed delay does not break it.
    #[test]
    fn a_held_pointer_gets_a_card_naming_the_line_under_it() {
        let mut s = screen("hover-card", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        let st = strip(&f, &theme);

        let at = egui::pos2(st.center().x, st.center().y);
        let f = s.feed(vec![moved(at)]);
        assert!(f.filled(fill).is_empty(), "no card the instant the pointer arrives");
        let f = s.wait(0.2).draw();
        assert!(f.filled(fill).is_empty(), "nor a fifth of a second in");

        let f = s.wait(1.0).draw();
        let card = f.filled(fill);
        assert_eq!(card.len(), 1, "the card is up once the delay is out: {card:?}");
        let card = card[0];
        // Halfway down four hundred lines.
        let text = card_text(&f, 201, 400).unwrap_or_else(|| panic!("{:?}", f.texts));
        assert!(text.ends_with("line 200"), "the line itself follows the number: {text:?}");

        // 42.2: left of the strip, never over it, and inside the pane.
        assert!(card.right() <= st.left(), "clear of the strip: {card:?} against {st:?}");
        assert!(card.top() >= st.top() && card.bottom() <= st.bottom(), "inside: {card:?}");
    }

    /// 42.4: the bottom pixel of the strip is the last line, not one past it.
    ///
    /// The clamp `line_at_y` grew in v0.40.0, tested there on its own. This is
    /// the same bug asked of the frame: a card reading `401` of 400, or no card
    /// at all, is what it looked like.
    #[test]
    fn the_bottom_pixel_names_the_last_line() {
        let mut s = screen("hover-bottom", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        let st = strip(&f, &theme);

        let f = hover(&mut s, egui::pos2(st.center().x, st.bottom() - 0.5));
        assert_eq!(f.filled(fill).len(), 1, "a card, not a blank");
        let text = card_text(&f, 400, 400).unwrap_or_else(|| panic!("{:?}", f.texts));
        assert!(text.ends_with("line 399"), "the last line: {text:?}");
        assert!(card_text(&f, 401, 400).is_none(), "and never one past the end");
    }

    /// 42.3: clicking where the card points goes there, with the line centred.
    #[test]
    fn a_click_lands_on_the_line_the_card_named() {
        let mut s = screen("hover-click", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let f = s.draw();
        let st = strip(&f, &theme);
        assert_eq!(s.app.tabs[s.app.active].preview_offset, 0, "at the top to begin with");

        // Three quarters down: line 300 of 400.
        let at = egui::pos2(st.center().x, st.top() + st.height() * 0.75);
        let f = hover(&mut s, at);
        let named = card_text(&f, 301, 400).unwrap_or_else(|| panic!("{:?}", f.texts));
        assert!(named.ends_with("line 300"), "the card points at it: {named:?}");

        s.feed(vec![button(at, true)]);
        s.feed(vec![button(at, false)]);
        let offset = s.app.tabs[s.app.active].preview_offset;
        assert!(offset > 0, "the click moved the preview");
        // Centred, not put at the top: the line is somewhere in the middle of
        // what is now on screen.
        assert!(offset < 300, "the line named is below the first one drawn: {offset}");
        let f = s.draw();
        let first = f.texts.iter().find(|t| t.starts_with("line ")).expect("the body is drawn");
        assert_eq!(first, &format!("line {offset}"), "and the body starts there");
    }

    /// 42.8: the pointer leaving the strip takes the card with it, and the
    /// delay starts again when it comes back.
    #[test]
    fn leaving_the_strip_takes_the_card_and_restarts_the_delay() {
        let mut s = screen("hover-leave", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        let st = strip(&f, &theme);

        let at = egui::pos2(st.center().x, st.center().y);
        let f = hover(&mut s, at);
        assert_eq!(f.filled(fill).len(), 1, "a card to begin with");

        let f = s.feed(vec![moved(egui::pos2(st.left() - 200.0, st.center().y))]);
        assert!(f.filled(fill).is_empty(), "nothing left behind off the strip");

        let f = s.feed(vec![moved(at)]);
        assert!(f.filled(fill).is_empty(), "and the delay starts again, not where it left off");
        let f = s.wait(1.0).draw();
        assert_eq!(f.filled(fill).len(), 1, "then the card is back");
    }

    /// 42.5: a drag keeps the card up, so the place can be found before the
    /// button is let go -- even once the pointer has left the strip.
    #[test]
    fn a_drag_keeps_the_card_following() {
        let mut s = screen("hover-drag", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        let st = strip(&f, &theme);

        let at = egui::pos2(st.center().x, st.center().y);
        s.feed(vec![moved(at), button(at, true)]);
        // Off the strip entirely, which without the drag would end the card.
        let away = egui::pos2(st.left() - 300.0, st.center().y - 200.0);
        let f = s.wait(1.0).feed(vec![moved(away)]);
        assert_eq!(f.filled(fill).len(), 1, "the card is still up mid-drag: {:?}", f.texts);
        assert!(s.app.tabs[s.app.active].preview_offset > 0, "and the drag is scrolling");

        let f = s.feed(vec![button(away, false)]);
        assert!(f.filled(fill).is_empty(), "letting go off the strip ends it");
    }

    /// 42.6 and 42.7: a blank line is the number on its own, and a very long
    /// one is cut rather than laid out in full.
    #[test]
    fn a_blank_line_is_the_number_alone_and_a_long_one_is_cut() {
        let mut lines = plain(400);
        lines[100] = Vec::new();
        lines[200] = vec![crate::preview::Span { text: "x".repeat(2000), ..Default::default() }];
        let mut s = screen("hover-shapes", source(lines, Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        let st = strip(&f, &theme);

        let y = |k: usize| st.top() + (k as f32 + 0.5) / 400.0 * st.height();

        let f = hover(&mut s, egui::pos2(st.center().x, y(100)));
        let blank = card_text(&f, 101, 400).unwrap_or_else(|| panic!("{:?}", f.texts));
        assert_eq!(blank.trim(), "101", "the number and nothing else: {blank:?}");
        let narrow = f.filled(fill)[0].width();

        // A fresh hover, so the delay is measured from arriving here.
        s.feed(vec![moved(egui::pos2(10.0, 10.0))]);
        let f = hover(&mut s, egui::pos2(st.center().x, y(200)));
        let long = card_text(&f, 201, 400).unwrap_or_else(|| panic!("{:?}", f.texts));
        assert_eq!(long.chars().count(), 204, "two hundred characters and the number: {}", long.len());
        assert_eq!(long.lines().count(), 1, "on one row");
        assert!(f.filled(fill)[0].width() > narrow, "and a wider card than the blank one's");
    }

    /// 42.9: `[mgr] preview_hovered` reaches the card -- the key that did
    /// nothing at all before v0.40.0.
    #[test]
    fn the_card_follows_the_themes_preview_hovered() {
        let mut s = screen("hover-theme", source(plain(400), Vec::new(), 400));
        let red = egui::Color32::from_rgb(255, 0, 0);
        let plainly = card_fill(&s.app.cfg.theme);
        assert_ne!(plainly, red, "the default is not what the theme will ask for");
        std::sync::Arc::make_mut(&mut s.app.cfg.theme).preview_hovered.bg = Some(red);
        let theme = s.app.cfg.theme.clone();
        let f = s.draw();
        let st = strip(&f, &theme);

        let f = hover(&mut s, egui::pos2(st.center().x, st.center().y));
        assert_eq!(f.filled(red).len(), 1, "the card took the theme's background");
        assert!(f.filled(plainly).is_empty(), "and not the built-in one");
    }

    /// 42.13: `<A-n>` takes the strip away, and so there is nothing to hover.
    #[test]
    fn no_strip_means_no_card() {
        let mut s = screen("hover-off", source(plain(400), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        let st = strip(&f, &theme);

        let at = egui::pos2(st.center().x, st.center().y);
        assert_eq!(hover(&mut s, at).filled(fill).len(), 1, "a card while the map is on");

        let f = s.feed(vec![key(egui::Key::N, egui::Modifiers { alt: true, ..Default::default() })]);
        assert!(!s.app.cfg.ui.minimap, "`<A-n>` turned it off");
        assert!(f.filled(fill).is_empty(), "the card went with the strip");
        let tint = theme.hovered_bg.gamma_multiply(0.4);
        assert!(f.filled(tint).is_empty(), "and so did the viewport box");
        let f = s.wait(1.0).draw();
        assert!(f.filled(fill).is_empty(), "and waiting does not bring it back");
    }

    /// 42.11: the strip is the source view's. Rendered Markdown has none, so
    /// `M` is what puts it there.
    #[test]
    fn markdown_gets_a_strip_in_the_source_view_only() {
        use crate::preview::{Doc, DocLine, Extent, LineKind, Payload};
        let n = 400;
        let doc = Doc {
            lines: (0..n)
                .map(|i| DocLine {
                    spans: vec![crate::preview::Span {
                        text: format!("rendered {i}"),
                        ..Default::default()
                    }],
                    kind: LineKind::Text,
                    indent: 0,
                    src: i,
                })
                .collect(),
            toc: Vec::new(),
            toc_cols: 0,
            body_cols: 80,
        };
        let state = crate::app::PreviewState::Ready(Payload::Markdown {
            doc,
            source: plain(n),
            map: rows(n),
            extent: Extent { truncated: false, total: n },
        });
        let mut s = screen("hover-md", state);
        let theme = s.app.cfg.theme.clone();
        let tint = theme.hovered_bg.gamma_multiply(0.4);
        assert!(s.app.render_markdown, "rendered to begin with, which is the default");

        let f = s.draw();
        assert!(f.says("rendered 0"), "the rendered body is drawn: {:?}", f.texts);
        assert!(f.filled(tint).is_empty(), "and there is no strip beside it");

        let f = s.typed("M");
        assert!(!s.app.render_markdown, "`M` switched to the source");
        assert!(f.says("line 0"), "the source is drawn: {:?}", f.texts);
        let st = strip(&f, &theme);
        let f = hover(&mut s, egui::pos2(st.center().x, st.center().y));
        assert_eq!(f.filled(card_fill(&theme)).len(), 1, "and the card works there");
    }

    /// 42.10: the card is drawn over the outline column and leaves it intact.
    #[test]
    fn the_card_covers_the_outline_and_leaves_nothing_behind() {
        let mut s = screen("hover-outline", source(plain(400), toc(), 400));
        let theme = s.app.cfg.theme.clone();
        let fill = card_fill(&theme);
        let f = s.draw();
        assert!(f.says("Contents"), "the column is up: {:?}", f.texts);
        let st = strip(&f, &theme);

        let at = egui::pos2(st.center().x, st.center().y);
        let f = hover(&mut s, at);
        let card = f.filled(fill);
        assert_eq!(card.len(), 1, "a card over the column");
        assert!(f.says("fn item0"), "with the column still drawn under it: {:?}", f.texts);

        let f = s.feed(vec![moved(egui::pos2(10.0, 10.0))]);
        assert!(f.filled(fill).is_empty(), "and nothing left behind when it goes");
        assert!(f.says("Contents"), "the column is as it was: {:?}", f.texts);
    }

    /// 42.12: a truncated file is numbered by the lines that were read, so the
    /// card and the body agree about which line is which.
    ///
    /// Thirty lines of four hundred, few enough that the whole of what was read
    /// fits in the pane -- which is when the body draws its own note saying how
    /// much it has not got, so the two numbers can be compared in one frame.
    #[test]
    fn a_truncated_file_numbers_the_lines_it_has() {
        let mut s = screen("hover-cut", source(plain(30), Vec::new(), 400));
        let theme = s.app.cfg.theme.clone();
        let f = s.draw();
        assert!(f.says("400 lines total (truncated)"), "the body says so: {:?}", f.texts);
        let st = strip(&f, &theme);

        let f = hover(&mut s, egui::pos2(st.center().x, st.bottom() - 0.5));
        let last = card_text(&f, 30, 30).unwrap_or_else(|| panic!("{:?}", f.texts));
        assert!(last.ends_with("line 29"), "the last line that was read: {last:?}");
        assert!(card_text(&f, 400, 30).is_none(), "not the file's own last line");
    }
}
