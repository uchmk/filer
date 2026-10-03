//! SVG rendered with resvg, scaled to fill the preview box.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

use super::{image_preview, Payload};

const MAX_BYTES: u64 = 32 * 1024 * 1024;

pub fn render(path: &Path, box_size: (u32, u32)) -> Result<Payload, String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_BYTES {
        return Err(format!("SVG too large to preview ({})", crate::util::human_size(len)));
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    render_bytes(&data, path.parent(), box_size)
}

fn render_bytes(data: &[u8], dir: Option<&Path>, box_size: (u32, u32)) -> Result<Payload, String> {
    let mut opt = usvg::Options { resources_dir: dir.map(Path::to_path_buf), ..Default::default() };
    // Scanning the system's fonts takes a moment; only pay for it when there is text.
    if data.windows(5).any(|w| w == b"<text") {
        opt.fontdb = fonts_for(dir);
    }
    let tree = usvg::Tree::from_data(data, &opt).map_err(|e| format!("bad SVG: {e}"))?;

    // Vectors scale up for free, so small icons fill the box too.
    let size = tree.size();
    let (bw, bh) = (box_size.0.max(32) as f32, box_size.1.max(32) as f32);
    let scale = (bw / size.width()).min(bh / size.height());
    let w = (size.width() * scale).round().clamp(1.0, bw) as u32;
    let h = (size.height() * scale).round().clamp(1.0, bh) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("SVG has no area")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

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
    let source = vector_source(size.width(), size.height());
    let own = source.0 as f32 / size.width().max(1.0);
    Ok(Payload::Image { width, height, source, own, rgba, caption })
}

/// The size an SVG is laid out at, whatever box it was rendered into.
///
/// The picture's geometry -- its fit, its zoom, where a pan stops -- is worked
/// out from `source`, and a re-render for a zoom must not move it. This used
/// to be the size just rendered, so each re-render shrank `fit`, which asked
/// for a bigger box, which shrank `fit` again: one `<A-i>` ran on to the
/// decode cap (#219). A vector has no pixels of its own, so it is given as
/// many as the largest render can have: big enough that fitting it to the pane
/// fills the pane, as an icon should, and 1:1 is the sharpest render there is.
fn vector_source(w: f32, h: f32) -> (u32, u32) {
    let cap = super::MAX_DECODE as f32;
    let k = cap / w.max(h).max(1.0);
    (((w * k).round() as u32).max(1), ((h * k).round() as u32).max(1))
}

/// The system's fonts, and any font file in the SVG's own folder: a drawing
/// shipped with the font it was made in names that font, and `resources_dir`
/// serves only images, so the text fell back to Times (#219, 4.6). The folder
/// only, not below it, and only a few files: a preview must not read a whole
/// font collection to show one picture.
fn fonts_for(dir: Option<&Path>) -> Arc<usvg::fontdb::Database> {
    let system = system_fonts();
    let local: Vec<std::path::PathBuf> = dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["ttf", "otf", "ttc", "otc"].contains(&e.to_ascii_lowercase().as_str()))
        })
        .take(16)
        .collect();
    if local.is_empty() {
        return system;
    }
    let mut db = (*system).clone();
    for p in &local {
        let _ = db.load_font_file(p);
    }
    Arc::new(db)
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
        let Payload::Image { width, rgba, .. } = p else { panic!("not an image: {p:?}") };
        let i = (y * width + x) as usize * 4;
        rgba[i..i + 4].try_into().unwrap()
    }

    /// #219, 4.6: a font file in the SVG's folder is among the fonts its text
    /// is set in.
    #[test]
    fn a_font_beside_the_svg_is_used() {
        let dir = crate::util::test_dir("svg-font");
        let defs = egui::FontDefinitions::default();
        let hack = defs.font_data.get("Hack").expect("egui ships Hack");
        std::fs::write(dir.join("Hack-Regular.ttf"), &*hack.font).unwrap();
        let db = fonts_for(Some(&dir));
        let families: Vec<String> = db.faces().flat_map(|f| f.families.iter().map(|(n, _)| n.clone())).collect();
        assert!(families.iter().any(|f| f == "Hack"), "the folder's font is offered");

        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40">
            <text x="0" y="30" font-family="Hack" font-size="30">filer</text></svg>"#;
        assert!(render_bytes(svg, Some(&dir), (400, 80)).is_ok(), "and the picture renders");
        // A folder without one leaves the system's set as it is.
        assert!(Arc::ptr_eq(&fonts_for(Some(&crate::util::test_dir("svg-nofont"))), &system_fonts()));
    }

    /// #219: the size the geometry is worked out from does not follow the box
    /// a zoom re-rendered into, so a zoom cannot feed on itself.
    #[test]
    fn the_source_size_does_not_follow_the_render_box() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20">
            <rect width="10" height="20" fill="red"/></svg>"#;
        let source = |b| match render_bytes(svg, None, b).unwrap() {
            Payload::Image { source, width, height, .. } => (source, (width, height)),
            p => panic!("{p:?}"),
        };
        let (small, drawn_small) = source((400, 400));
        let (big, drawn_big) = source((1600, 1600));
        assert_ne!(drawn_small, drawn_big, "it was rendered twice as big");
        assert_eq!(small, big, "and is laid out the same");
        assert_eq!(small, (2048, 4096), "the icon's shape, at the decode cap");
    }

    #[test]
    fn scales_a_small_icon_up_to_the_box() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20">
            <rect width="10" height="20" fill="red"/></svg>"#;
        let p = render_bytes(svg, None, (400, 400)).unwrap();
        let Payload::Image { width, height, caption, .. } = &p else { unreachable!() };
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
