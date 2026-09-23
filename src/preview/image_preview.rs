use std::path::Path;
use std::sync::Arc;

use super::Payload;

/// Refuse to decode anything that would balloon in memory; a 100 MP image is
/// 400 MB of RGBA.
const MAX_PIXELS: u64 = 80_000_000;

pub fn render(path: &Path, box_size: (u32, u32)) -> Result<Payload, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;

    let source = reader.into_dimensions().map_err(|e| e.to_string())?;
    if u64::from(source.0) * u64::from(source.1) > MAX_PIXELS {
        return Err(format!("image too large to preview ({}x{})", source.0, source.1));
    }

    let img = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;

    let (bw, bh) = (box_size.0.max(32), box_size.1.max(32));
    // Only ever shrink: upscaling is the renderer's job and keeps memory low.
    let img = if img.width() > bw || img.height() > bh {
        img.resize(bw, bh, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Ok(Payload::Image { width: w, height: h, rgba: Arc::new(rgba.into_raw()), source })
}
