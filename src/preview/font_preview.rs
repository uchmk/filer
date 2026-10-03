//! A specimen sheet for TrueType / OpenType fonts: the font's name, its
//! alphabet, and a waterfall of sizes, drawn in the font itself.

use std::path::Path;
use std::sync::Arc;

use ab_glyph::{point, Font, FontRef, GlyphId, PxScale, ScaleFont};

use super::Payload;

/// Big CJK collections run to tens of megabytes; beyond this, don't bother.
const MAX_BYTES: u64 = 128 * 1024 * 1024;

/// The preview box is twice the pane in points (see `ui::mod`), so sizes in
/// points are drawn at twice as many pixels.
const K: f32 = 2.0;

const ALPHABET: &[&str] = &[
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "abcdefghijklmnopqrstuvwxyz",
    "0123456789 !?&@#$%*()[]{}<>/+-=",
    "あいうえお アイウエオ 永和漢字",
];
const PANGRAM: &str = "The quick brown fox jumps over the lazy dog";
const WATERFALL: &[f32] = &[12.0, 18.0, 24.0, 36.0, 48.0];

pub fn render(path: &Path, box_size: (u32, u32)) -> Result<Payload, String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_BYTES {
        return Err(format!("font too large to preview ({})", crate::util::human_size(len)));
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    render_bytes(&data, &crate::util::file_name(path), box_size)
}

fn render_bytes(data: &[u8], file_name: &str, box_size: (u32, u32)) -> Result<Payload, String> {
    let face = ttf_parser::Face::parse(data, 0).map_err(|e| format!("not a font: {e}"))?;
    let font = FontRef::try_from_slice_and_index(data, 0).map_err(|e| format!("not a font: {e}"))?;
    let name = full_name(&face).unwrap_or_else(|| file_name.to_owned());

    // Symbol and icon fonts can't spell their own name; the caption still does.
    let titled = covers(&font, &name);
    let mut lines: Vec<(f32, &str)> = Vec::new();
    if titled {
        lines.push((24.0, name.as_str()));
    }
    lines.extend(ALPHABET.iter().filter(|s| covers(&font, s)).map(|s| (16.0, *s)));
    if covers(&font, PANGRAM) {
        lines.extend(WATERFALL.iter().map(|&pt| (pt, PANGRAM)));
    }

    let mut sheet = Sheet::new(box_size.0.max(64), box_size.1.max(64));
    let pad = 12.0 * K;
    let mut y = pad;
    for (i, &(pt, text)) in lines.iter().enumerate() {
        let sf = font.as_scaled(PxScale::from(pt * K));
        let baseline = y + sf.ascent();
        let bottom = baseline - sf.descent();
        if bottom > sheet.h as f32 {
            break;
        }
        sheet.draw(&font, pt * K, text, pad, baseline);
        y = bottom + sf.line_gap() + 4.0 * K;
        if i == 0 && titled {
            y += 8.0 * K; // set the title apart
        }
    }
    if lines.len() == usize::from(titled) {
        // Nothing to spell with: show what the font does have.
        y = sheet.grid(&font, 24.0 * K, pad, y);
    }
    if y <= pad {
        return Err("font has no drawable glyphs".into());
    }

    let mut caption = format!("{name} · {} glyphs", face.number_of_glyphs());
    if let Some(n) = ttf_parser::fonts_in_collection(data).filter(|&n| n > 1) {
        caption.push_str(&format!(" · face 1 of {n}"));
    }
    let (width, height, rgba) = sheet.finish(y.ceil() as u32 + pad as u32);
    // A rendered specimen is its own source: there is nothing sharper to ask for.
    Ok(Payload::Image { width, height, source: (width, height), own: 1.0, rgba, caption })
}

/// The English full name if there is one, else any readable full or family name.
fn full_name(face: &ttf_parser::Face) -> Option<String> {
    use ttf_parser::name_id::{FAMILY, FULL_NAME};
    let pick = |id: u16| {
        let mut any = None;
        for n in face.names() {
            if n.name_id != id || !n.is_unicode() {
                continue;
            }
            let Some(s) = n.to_string().filter(|s| !s.trim().is_empty()) else { continue };
            if n.language_id == 0x0409 {
                return Some(s);
            }
            any.get_or_insert(s);
        }
        any
    };
    pick(FULL_NAME).or_else(|| pick(FAMILY))
}

/// Whether the font can draw most of `text`; a line of tofu tells nothing.
fn covers(font: &FontRef, text: &str) -> bool {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let missing = chars.iter().filter(|&&c| font.glyph_id(c) == GlyphId(0)).count();
    !chars.is_empty() && missing * 4 <= chars.len()
}

/// Ink coverage on a white page, cropped to the text on `finish`.
struct Sheet {
    w: u32,
    h: u32,
    ink: Vec<f32>,
}

impl Sheet {
    fn new(w: u32, h: u32) -> Self {
        Self { w, h, ink: vec![0.0; w as usize * h as usize] }
    }

    fn draw(&mut self, font: &FontRef, px: f32, text: &str, x0: f32, baseline: f32) {
        let sf = font.as_scaled(PxScale::from(px));
        let mut x = x0;
        let mut prev: Option<GlyphId> = None;
        for c in text.chars() {
            if x > self.w as f32 {
                break;
            }
            let id = sf.glyph_id(c);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            prev = Some(id);
            let glyph = id.with_scale_and_position(px, point(x, baseline));
            x += sf.h_advance(id);
            if let Some(outline) = font.outline_glyph(glyph) {
                self.blit(&outline);
            }
        }
    }

    /// Glyphs in id order on a grid of square cells, from `top` down to the
    /// bottom of the sheet. Returns where the last row ends.
    fn grid(&mut self, font: &FontRef, px: f32, x0: f32, top: f32) -> f32 {
        let sf = font.as_scaled(PxScale::from(px));
        let cell = px * 1.5;
        let cols = ((self.w as f32 - 2.0 * x0) / cell).floor().max(1.0) as usize;
        let mut end = top;
        let mut n = 0;
        for id in (1..font.glyph_count().min(usize::from(u16::MAX))).map(|i| GlyphId(i as u16)) {
            let cy = top + (n / cols) as f32 * cell;
            if cy + cell > self.h as f32 {
                break;
            }
            let x = x0 + (n % cols) as f32 * cell + (cell - sf.h_advance(id)).max(0.0) / 2.0;
            let baseline = cy + (cell + sf.ascent() + sf.descent()) / 2.0;
            // Blank glyphs (space, control) don't take a cell.
            let Some(outline) = font.outline_glyph(id.with_scale_and_position(px, point(x, baseline)))
            else {
                continue;
            };
            self.blit(&outline);
            n += 1;
            end = cy + cell;
        }
        end
    }

    fn blit(&mut self, outline: &ab_glyph::OutlinedGlyph) {
        let b = outline.px_bounds();
        outline.draw(|gx, gy, cov| {
            let (sx, sy) = (b.min.x as i64 + i64::from(gx), b.min.y as i64 + i64::from(gy));
            if sx >= 0 && sy >= 0 && sx < i64::from(self.w) && sy < i64::from(self.h) {
                let i = sy as usize * self.w as usize + sx as usize;
                self.ink[i] = (self.ink[i] + cov).min(1.0);
            }
        });
    }

    /// Black on white RGBA, cut to `height` rows.
    fn finish(self, height: u32) -> (u32, u32, Arc<Vec<u8>>) {
        let h = height.clamp(1, self.h);
        let n = self.w as usize * h as usize;
        let mut rgba = Vec::with_capacity(n * 4);
        for &c in &self.ink[..n] {
            let v = (255.0 * (1.0 - c)).round() as u8;
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
        (self.w, h, Arc::new(rgba))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_font(name: &str) -> Option<std::path::PathBuf> {
        let p = std::path::PathBuf::from(r"C:\Windows\Fonts").join(name);
        p.exists().then_some(p)
    }

    fn dark_pixels(p: &Payload) -> usize {
        let Payload::Image { rgba, .. } = p else { panic!("not an image: {p:?}") };
        rgba.chunks(4).filter(|px| px[0] < 128).count()
    }

    #[test]
    fn draws_a_truetype_font() {
        let Some(path) = system_font("consola.ttf") else { return };
        let p = render(&path, (1200, 1600)).unwrap();
        let Payload::Image { width, height, caption, .. } = &p else { unreachable!() };
        assert_eq!(*width, 1200);
        assert!(*height > 200 && *height <= 1600, "{height}");
        assert!(caption.starts_with("Consolas"), "{caption}");
        assert!(dark_pixels(&p) > 1000);
    }

    #[test]
    fn draws_the_first_face_of_a_collection() {
        let Some(path) = system_font("meiryo.ttc") else { return };
        let p = render(&path, (1200, 1600)).unwrap();
        let Payload::Image { caption, .. } = &p else { unreachable!() };
        assert!(caption.contains("face 1 of"), "{caption}");
        assert!(dark_pixels(&p) > 1000);
    }

    #[test]
    fn stops_at_the_bottom_of_a_short_box() {
        let Some(path) = system_font("consola.ttf") else { return };
        let Payload::Image { height, .. } = render(&path, (400, 200)).unwrap() else { unreachable!() };
        assert!(height <= 200, "{height}");
    }

    #[test]
    fn lays_out_a_symbol_font_as_a_grid() {
        let Some(path) = system_font("wingding.ttf") else { return };
        let p = render(&path, (1200, 1600)).unwrap();
        let Payload::Image { caption, .. } = &p else { unreachable!() };
        assert!(caption.starts_with("Wingdings"), "{caption}");
        assert!(dark_pixels(&p) > 1000);
    }

    #[test]
    fn rejects_garbage() {
        assert!(render_bytes(b"definitely not a font", "x.ttf", (400, 400)).is_err());
    }
}
