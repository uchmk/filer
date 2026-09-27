//! The terminal pane.
//!
//! One glyph per cell, painted the way the file list is painted: only what is
//! on screen is touched, and the font is the app's own so the two panes look
//! like one program. The grid is copied out from under the lock first — the
//! PTY reader thread wants it back, and laying out text takes longer than
//! copying a screenful of cells.

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Stroke, Ui, Vec2};

use crate::app::App;
use crate::config::theme::Theme;
use crate::terminal::{self, Mods, Size, Special};

/// How many cells fit, given the space and the font.
pub fn fit(rect: Rect, cell_w: f32, row_h: f32) -> Size {
    Size::new((rect.width() / cell_w).floor() as usize, (rect.height() / row_h).floor() as usize)
}

/// How far the view travels for a pixel of wheel movement. One notch of a
/// Windows wheel reaches egui as about 50 points, so at this rate a notch is
/// close to the three lines every other terminal moves.
const LINES_PER_PIXEL: f32 = 1.0;

pub fn draw(app: &mut App, ui: &mut Ui, rect: Rect, f: &FontId, row_h: f32) {
    let theme = app.cfg.theme.clone();
    let focused = app.term_focus;
    let painter = ui.painter_at(rect);
    // A monospace font gives every cell the same width, so one measurement
    // does for the whole grid.
    let cell_w = painter.layout_no_wrap("M".into(), f.clone(), theme.fg).size().x.max(1.0);
    painter.rect_filled(rect, CornerRadius::ZERO, theme.bg_alt);
    // Accented while the pane has the keys, the same as the outline's rule and
    // in the same colour -- see `super::focus_rule` for why it is shared.
    let rule = super::focus_rule(&theme, focused);
    painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, rule));

    let inner = rect.shrink2(Vec2::new(6.0, 4.0));
    let size = fit(inner, cell_w, row_h);

    // The pointer is read before the terminal is borrowed, so the two do not
    // fight over `app`.
    let id = ui.id().with("term-pane");
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    let pointer = resp.interact_pointer_pos();
    let over = ui.rect_contains_pointer(rect);
    // A panel over the pane owns the wheel; see `Overlay::is_modal`.
    let wheel = match over && !app.overlay.is_modal() {
        true => ui.ctx().input(|i| i.smooth_scroll_delta.y),
        false => 0.0,
    };
    // Read before the terminal is borrowed, so a clipboard that will not open
    // can still be reported through `app`.
    let pasting = resp.secondary_clicked().then(crate::exec::get_clipboard);
    let press = ui.ctx().input(|i| i.pointer.press_origin());
    if resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
        app.term_focus = true;
    }

    let Some(term) = &mut app.term else { return };
    term.resize(size, (cell_w.round() as u16, row_h.round() as u16));
    // Out from under the lock before any laying out happens.
    let (rows, cursor, app_cursor, alt_screen) = term.with_grid(|t| {
        (
            terminal::snapshot(t),
            terminal::cursor_cell(t),
            terminal::app_cursor(t),
            terminal::alt_screen(t),
        )
    });

    for (y, row) in rows.iter().enumerate() {
        let top = inner.top() + y as f32 * row_h;
        if top > inner.bottom() {
            break;
        }
        // Backgrounds first, run by run: one rectangle for a stretch of the
        // same color beats one per cell.
        let mut run: Option<(usize, Color32)> = None;
        for (x, cell) in row.iter().enumerate() {
            // A selected cell takes the same background the list gives the row
            // under the cursor: already proven to carry text in this palette,
            // and already the colour that means "this one" everywhere else.
            let bg = match cell.selected {
                true => theme.hovered_bg,
                false => color(cell.bg, &theme, true),
            };
            match run {
                Some((_, c)) if c == bg => {}
                Some((start, c)) => {
                    fill(&painter, inner, start, x, top, cell_w, row_h, c, &theme);
                    run = Some((x, bg));
                }
                None => run = Some((x, bg)),
            }
        }
        if let Some((start, c)) = run {
            fill(&painter, inner, start, row.len(), top, cell_w, row_h, c, &theme);
        }

        for (x, cell) in row.iter().enumerate() {
            if cell.c == ' ' || cell.c == '\0' {
                continue;
            }
            let mut fg = color(cell.fg, &theme, false);
            if cell.flags.contains(Flags::DIM) {
                fg = fg.linear_multiply(0.6);
            }
            if cell.flags.contains(Flags::INVERSE) {
                fg = color(cell.bg, &theme, true);
            }
            painter.text(
                egui::pos2(inner.left() + x as f32 * cell_w, top),
                Align2::LEFT_TOP,
                cell.c,
                f.clone(),
                fg,
            );
        }
    }

    // The cursor: filled while the pane has the keys, outlined when it does
    // not, which is the same language the split panes use.
    let (cx, cy) = cursor;
    if cy < size.lines && cx < size.cols {
        let at = Rect::from_min_size(
            egui::pos2(inner.left() + cx as f32 * cell_w, inner.top() + cy as f32 * row_h),
            Vec2::new(cell_w, row_h),
        );
        if focused {
            painter.rect_filled(at, CornerRadius::ZERO, theme.cwd.fg.unwrap_or(theme.fg));
            if let Some(cell) = rows.get(cy).and_then(|r| r.get(cx)) {
                if cell.c != ' ' {
                    painter.text(at.left_top(), Align2::LEFT_TOP, cell.c, f.clone(), theme.bg);
                }
            }
        } else {
            painter.rect_stroke(
                at,
                CornerRadius::ZERO,
                Stroke::new(1.0, theme.fg_dim),
                egui::StrokeKind::Inside,
            );
        }
    }

    // Somewhere above the bottom, which is easy to forget being in.
    let back = term.scrolled_back();
    if back > 0 {
        let note = format!(" {back} lines back — <S-End> to return ");
        let g = painter.layout_no_wrap(note, f.clone(), theme.bg);
        let at = egui::pos2(inner.right() - g.size().x - 6.0, inner.top() + 2.0);
        painter.rect_filled(
            Rect::from_min_size(at, g.size()),
            CornerRadius::same(3),
            theme.cwd.fg.unwrap_or(theme.fg),
        );
        painter.galley(at, g, theme.bg);
    }

    // Which cell the pointer is over, and which half of it. The half decides
    // whether that cell ends up inside the selection when the drag turns out
    // to run the other way; see `terminal::select_at`.
    let cell_at = |p: egui::Pos2| {
        let x = (p.x - inner.left()) / cell_w;
        let col = x.floor().max(0.0);
        let line = ((p.y - inner.top()) / row_h).floor().max(0.0) as usize;
        let cell = (col as usize).min(size.cols.saturating_sub(1));
        (cell, line.min(size.lines.saturating_sub(1)), x - col >= 0.5)
    };
    if let Some(p) = pointer {
        if resp.double_clicked() {
            let (col, line, _) = cell_at(p);
            term.select_word((col, line));
        } else if resp.drag_started() {
            // Where the button went down, not where the pointer is now: egui
            // calls a drag started only once it has moved past a threshold,
            // and a few points of that is most of a cell this narrow. Reading
            // the live position anchored the selection one cell along, so a
            // drag begun on the first character dropped it -- which is why it
            // had to be begun slightly to its left to keep it.
            let from = press.unwrap_or(p);
            let (col, line, right) = cell_at(from);
            term.select((col, line), right, true);
        } else if resp.dragged() {
            let (col, line, right) = cell_at(p);
            term.select((col, line), right, false);
        } else if resp.clicked() {
            // A plain click puts the caret nowhere; it just clears what was
            // selected, the way a terminal does.
            term.clear_selection();
        }
    }
    // Letting go of a selection copies it, which is what a terminal means by
    // selecting: there is no other step.
    if resp.drag_stopped() || resp.double_clicked() {
        if let Some(text) = term.selection() {
            let _ = crate::exec::set_clipboard(&text);
        }
    }
    // Select to copy, right-click to paste — the pair a terminal has always
    // had, and the half that was missing. `<C-v>` reaches here as egui's paste
    // event; a right-click is not one, so the clipboard was read above.
    //
    // What arrives is what `<C-v>` sends, `Terminal::paste` deciding both
    // times whether the bracketed-paste markers go on. Where they do, a
    // multi-line clipboard waits in the line editor instead of running itself.
    if let Some(Ok(text)) = &pasting {
        if !text.is_empty() {
            term.paste(text);
        }
    }
    // The wheel walks the scrollback rather than the file list under it --
    // except on the alternate screen, where there is no scrollback and the
    // program drawing it is the thing being scrolled. Sending arrows there is
    // what every terminal does, and it is the only way a pager or an editor in
    // this pane can be scrolled with the wheel at all.
    let rows = wheel * LINES_PER_PIXEL / row_h;
    let whole = crate::ui::wheel_whole(&mut app.term_scroll_rows, rows);
    if whole != 0 {
        if alt_screen {
            let key = if whole > 0 { Special::Up } else { Special::Down };
            let bytes = crate::terminal::encode(key, Mods::default(), app_cursor);
            let mut out = Vec::with_capacity(bytes.len() * whole.unsigned_abs() as usize);
            for _ in 0..whole.unsigned_abs() {
                out.extend_from_slice(&bytes);
            }
            term.send(out);
        } else {
            term.scroll(Scroll::Delta(whole as i32));
        }
    }
    // Said out loud because the right-click looked like it did nothing.
    if let Some(Err(e)) = pasting {
        app.error(format!("Could not read the clipboard: {e}"));
    }
}

#[allow(clippy::too_many_arguments)]
fn fill(
    painter: &egui::Painter,
    inner: Rect,
    from: usize,
    to: usize,
    top: f32,
    cell_w: f32,
    row_h: f32,
    c: Color32,
    theme: &Theme,
) {
    if c == theme.bg_alt || to <= from {
        return;
    }
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(inner.left() + from as f32 * cell_w, top),
            Vec2::new((to - from) as f32 * cell_w, row_h),
        ),
        CornerRadius::ZERO,
        c,
    );
}

/// An ANSI color as something to paint with.
///
/// The sixteen named colors come from the theme so the pane matches the rest
/// of the window; the 256-color cube and the true-color values are what the
/// program asked for and are used as given.
fn color(c: AnsiColor, theme: &Theme, is_bg: bool) -> Color32 {
    match c {
        AnsiColor::Named(n) => named(n, theme, is_bg),
        AnsiColor::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        AnsiColor::Indexed(i) => indexed(i, theme, is_bg),
    }
}

fn named(n: NamedColor, theme: &Theme, is_bg: bool) -> Color32 {
    use NamedColor::*;
    match n {
        Background => theme.bg_alt,
        Foreground => theme.fg,
        Cursor => theme.cwd.fg.unwrap_or(theme.fg),
        Black => Color32::from_rgb(0x1b, 0x1e, 0x24),
        Red => Color32::from_rgb(0xf0, 0x71, 0x78),
        Green => Color32::from_rgb(0x8e, 0xd0, 0x8e),
        Yellow => Color32::from_rgb(0xe8, 0xc8, 0x7a),
        Blue => Color32::from_rgb(0x7a, 0xb8, 0xf5),
        Magenta => Color32::from_rgb(0xc9, 0x9c, 0xf0),
        Cyan => Color32::from_rgb(0x6f, 0xd0, 0xd0),
        White => theme.fg,
        BrightBlack => theme.fg_dim,
        BrightRed => Color32::from_rgb(0xff, 0x96, 0x9c),
        BrightGreen => Color32::from_rgb(0xb0, 0xe8, 0xb0),
        BrightYellow => Color32::from_rgb(0xff, 0xe0, 0x9c),
        BrightBlue => Color32::from_rgb(0x9c, 0xd0, 0xff),
        BrightMagenta => Color32::from_rgb(0xe0, 0xbc, 0xff),
        BrightCyan => Color32::from_rgb(0x9c, 0xe8, 0xe8),
        BrightWhite => Color32::WHITE,
        _ => {
            if is_bg {
                theme.bg_alt
            } else {
                theme.fg
            }
        }
    }
}

/// The xterm 256-color palette: sixteen named, a 6×6×6 cube, then a grey ramp.
fn indexed(i: u8, theme: &Theme, is_bg: bool) -> Color32 {
    match i {
        0..=15 => {
            let n = [
                NamedColor::Black,
                NamedColor::Red,
                NamedColor::Green,
                NamedColor::Yellow,
                NamedColor::Blue,
                NamedColor::Magenta,
                NamedColor::Cyan,
                NamedColor::White,
                NamedColor::BrightBlack,
                NamedColor::BrightRed,
                NamedColor::BrightGreen,
                NamedColor::BrightYellow,
                NamedColor::BrightBlue,
                NamedColor::BrightMagenta,
                NamedColor::BrightCyan,
                NamedColor::BrightWhite,
            ][i as usize];
            named(n, theme, is_bg)
        }
        16..=231 => {
            let i = i - 16;
            let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(step(i / 36), step((i % 36) / 6), step(i % 6))
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            Color32::from_gray(v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_color_cube_lands_where_xterm_puts_it() {
        let theme = Theme::default();
        // The corners of the 6x6x6 cube.
        assert_eq!(indexed(16, &theme, false), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed(231, &theme, false), Color32::from_rgb(255, 255, 255));
        // Pure red is the first step of the red axis, not 0xff.
        assert_eq!(indexed(196, &theme, false), Color32::from_rgb(255, 0, 0));
        // The grey ramp at both ends.
        assert_eq!(indexed(232, &theme, false), Color32::from_gray(8));
        assert_eq!(indexed(255, &theme, false), Color32::from_gray(238));
        // The first sixteen come from the theme, so the pane matches.
        assert_eq!(indexed(7, &theme, false), theme.fg);
    }

    #[test]
    fn the_grid_is_measured_in_whole_cells() {
        let r = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(101.0, 55.0));
        let size = fit(r, 10.0, 20.0);
        assert_eq!((size.cols, size.lines), (10, 2), "a part-cell is not a cell");
        // A pane too small to hold anything still has to answer something a
        // grid can divide by.
        let tiny = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1.0, 1.0));
        let size = fit(tiny, 10.0, 20.0);
        assert_eq!((size.cols, size.lines), (1, 1));
    }
}
