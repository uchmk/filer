//! SVG rendered with resvg, scaled to fill the preview box.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

use super::{image_preview, Payload};

const MAX_BYTES: u64 = 32 * 1024 * 1024;

pub fn render(path: &Path, box_size: (u32, u32)) -> Result<Payload, String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_BYTES {
        return Err(format!(
            "SVG too large to preview ({})",
            crate::util::human_size(len)
        ));
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    render_bytes(&data, path.parent(), box_size)
}

fn render_bytes(data: &[u8], dir: Option<&Path>, box_size: (u32, u32)) -> Result<Payload, String> {
    let mut opt = usvg::Options {
        resources_dir: dir.map(Path::to_path_buf),
        ..Default::default()
    };
    // Scanning the system's fonts takes a moment; only pay for it when there is text.
    if data.windows(5).any(|w| w == b"<text") {
        opt.fontdb = system_fonts();
    }
    let tree = usvg::Tree::from_data(data, &opt).map_err(|e| format!("bad SVG: {e}"))?;

    // Vectors scale up for free, so small icons fill the box too.
    let size = tree.size();
    let (bw, bh) = (box_size.0.max(32) as f32, box_size.1.max(32) as f32);
    let scale = (bw / size.width()).min(bh / size.height());
    let w = (size.width() * scale).round().clamp(1.0, bw) as u32;
    let h = (size.height() * scale).round().clamp(1.0, bh) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("SVG has no area")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let straight: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let img = image::RgbaImage::from_raw(w, h, straight).ok_or("SVG buffer size mismatch")?;
    let (width, height, rgba) = image_preview::finish(img);
    let caption = format!("SVG · {} × {}", size.width().round(), size.height().round());
    // Vector art is re-rendered into a bigger box as the zoom grows, so what
    // came back is the source for now.
    Ok(Payload::Image {
        width,
        height,
        source: (width, height),
        rgba,
        caption,
    })
}

fn system_fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(p: &Payload, x: u32, y: u32) -> [u8; 4] {
        let Payload::Image { width, rgba, .. } = p else {
            panic!("not an image: {p:?}")
        };
        let i = (y * width + x) as usize * 4;
        rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn scales_a_small_icon_up_to_the_box() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20">
            <rect width="10" height="20" fill="red"/></svg>"#;
        let p = render_bytes(svg, None, (400, 400)).unwrap();
        let Payload::Image {
            width,
            height,
            caption,
            ..
        } = &p
        else {
            unreachable!()
        };
        assert_eq!((*width, *height), (200, 400));
        assert_eq!(caption, "SVG · 10 × 20");
        assert_eq!(pixel(&p, 100, 200), [255, 0, 0, 255]);
    }

    #[test]
    fn shows_transparency_on_the_checkerboard() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="50" width="50" height="100" fill="blue"/></svg>"#;
        let p = render_bytes(svg, None, (100, 100)).unwrap();
        assert_eq!(pixel(&p, 75, 50), [0, 0, 255, 255]);
        assert_eq!(pixel(&p, 1, 1), [0xcc, 0xcc, 0xcc, 255]);
    }

    #[test]
    fn rejects_malformed_svg() {
        assert!(render_bytes(b"<svg", None, (100, 100)).is_err());
    }
}
