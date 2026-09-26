//! Rebuild `assets/icon.ico` from `assets/icon.svg`.
//!
//! Windows wants an `.ico` compiled into the executable, and an `.ico` is a
//! handful of fixed-size bitmaps — so one has to be generated from the vector
//! and committed. This is how, so that nobody has to guess later:
//!
//! ```text
//! cargo run --example make-icon
//! ```
//!
//! Doing it here rather than in `build.rs` keeps `resvg` out of the build
//! graph twice over, and keeps an ordinary build from depending on anything
//! but the file it reads.
//!
//! The sizes are the ones Windows actually asks for: 16 for the title bar and
//! list views, 32 for the desktop and Alt+Tab, 48 for large icons, 64 and 128
//! for the in-between DPI scalings, and 256 for the extra-large view. Each is
//! rendered from the vector rather than downscaled from one bitmap, so the
//! small ones stay crisp.

use std::path::Path;

use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::ExtendedColorType;
use resvg::{tiny_skia, usvg};

const SIZES: [u32; 6] = [16, 32, 48, 64, 128, 256];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let svg = std::fs::read(root.join("icon.svg"))?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default())?;

    let mut frames = Vec::with_capacity(SIZES.len());
    for px in SIZES {
        let rgba = render(&tree, px).ok_or("could not rasterize")?;
        frames.push(IcoFrame::as_png(&rgba, px, px, ExtendedColorType::Rgba8)?);
    }

    let out = root.join("icon.ico");
    let file = std::fs::File::create(&out)?;
    IcoEncoder::new(std::io::BufWriter::new(file)).encode_images(&frames)?;
    println!(
        "wrote {} with {} sizes: {SIZES:?}",
        out.display(),
        SIZES.len()
    );
    Ok(())
}

/// One square of the icon, in straight-alpha RGBA. Kept in step with
/// `main.rs`'s `app_icon`: fitted and centred, never stretched.
fn render(tree: &usvg::Tree, px: u32) -> Option<Vec<u8>> {
    let size = tree.size();
    let scale = (px as f32 / size.width()).min(px as f32 / size.height());
    let dx = (px as f32 - size.width() * scale) / 2.0;
    let dy = (px as f32 - size.height() * scale) / 2.0;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    )
}
