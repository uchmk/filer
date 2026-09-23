use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Rect, Ui, Vec2};

use crate::app::PreviewState;
use crate::config::theme::Theme;
use crate::preview::Payload;

pub struct PreviewStyle<'a> {
    pub theme: &'a Theme,
    pub font: FontId,
    pub row_h: f32,
    pub wrap: bool,
}

/// Returns the number of scrollable lines, so the caller can clamp `seek`.
pub fn draw(
    ui: &mut Ui,
    rect: Rect,
    state: &PreviewState,
    texture: Option<&egui::TextureHandle>,
    offset: usize,
    st: &PreviewStyle<'_>,
) -> usize {
    let painter = ui.painter_at(rect);
    let rows = ((rect.height() / st.row_h).floor() as usize).max(1);

    match state {
        PreviewState::Empty => 0,
        PreviewState::Loading => {
            painter.text(
                rect.left_top() + Vec2::new(10.0, 8.0),
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
                    rect.left_top() + Vec2::new(10.0, 8.0),
                    Align2::LEFT_TOP,
                    e,
                    st.font.clone(),
                    st.theme.progress_error,
                );
                0
            }
            Payload::Text { lines, truncated, total_lines } => {
                let start = offset.min(lines.len().saturating_sub(1));
                let end = (start + rows).min(lines.len());
                for (i, line) in lines[start..end].iter().enumerate() {
                    let y = rect.top() + i as f32 * st.row_h;
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = if st.wrap { rect.width() - 16.0 } else { f32::INFINITY };
                    job.wrap.max_rows = if st.wrap { 3 } else { 1 };
                    job.wrap.break_anywhere = true;
                    job.wrap.overflow_character = None;
                    for span in line {
                        job.append(
                            &span.text,
                            0.0,
                            TextFormat {
                                font_id: st.font.clone(),
                                color: span
                                    .color
                                    .map(|[r, g, b]| {
                                        readable(
                                            Color32::from_rgb(r, g, b),
                                            st.theme.bg,
                                            st.theme.fg,
                                        )
                                    })
                                    .unwrap_or(st.theme.fg),
                                ..Default::default()
                            },
                        );
                    }
                    if line.is_empty() {
                        continue;
                    }
                    let galley = painter.layout_job(job);
                    painter.galley(egui::pos2(rect.left() + 10.0, y), galley, st.theme.fg);
                }
                if *truncated && end >= lines.len() {
                    let y = rect.top() + (end - start) as f32 * st.row_h;
                    if y < rect.bottom() {
                        painter.text(
                            egui::pos2(rect.left() + 10.0, y),
                            Align2::LEFT_TOP,
                            format!("… {total_lines} lines total (truncated)"),
                            st.font.clone(),
                            st.theme.fg_dim,
                        );
                    }
                }
                lines.len()
            }
            Payload::Binary { lines, total } => {
                let start = offset.min(lines.len().saturating_sub(1));
                let end = (start + rows.saturating_sub(1)).min(lines.len());
                for (i, line) in lines[start..end].iter().enumerate() {
                    painter.text(
                        egui::pos2(rect.left() + 10.0, rect.top() + i as f32 * st.row_h),
                        Align2::LEFT_TOP,
                        line,
                        st.font.clone(),
                        st.theme.fg_dim,
                    );
                }
                painter.text(
                    egui::pos2(rect.left() + 10.0, rect.bottom() - st.row_h),
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
                        egui::pos2(rect.left() + 10.0, y),
                        Align2::LEFT_TOP,
                        k,
                        st.font.clone(),
                        st.theme.fg_dim,
                    );
                    painter.text(
                        egui::pos2(rect.left() + 110.0, y),
                        Align2::LEFT_TOP,
                        v,
                        st.font.clone(),
                        st.theme.fg,
                    );
                }
                0
            }
            Payload::Image { width, height, source, .. } => {
                if let Some(tex) = texture {
                    let avail = rect.shrink(8.0);
                    let (w, h) = (*width as f32, *height as f32);
                    let scale = (avail.width() / w).min(avail.height() / h).min(1.0);
                    let size = Vec2::new(w * scale, h * scale);
                    let pos = egui::pos2(
                        avail.center().x - size.x / 2.0,
                        avail.top(),
                    );
                    painter.image(
                        tex.id(),
                        Rect::from_min_size(pos, size),
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    painter.text(
                        egui::pos2(avail.center().x, avail.top() + size.y + 6.0),
                        Align2::CENTER_TOP,
                        format!("{} × {}", source.0, source.1),
                        st.font.clone(),
                        st.theme.fg_dim,
                    );
                }
                0
            }
        },
    }
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
}
