//! Thumbnails from the Windows shell, the same ones Explorer shows. Whatever
//! the system has a handler for (HEIC with the HEIF extension, video frames,
//! PDF pages with a PDF handler, album art) previews without external tools.

use std::path::Path;

use super::{image_preview, Payload};

/// Shell extensions expect a single-threaded apartment on the calling thread.
#[cfg(windows)]
pub fn init_thread() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    // Already initialized is fine; the thread keeps COM until it exits.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
}

#[cfg(not(windows))]
pub fn init_thread() {}

pub fn render(path: &Path, box_size: (u32, u32)) -> Result<Payload, String> {
    let img = imp::thumbnail(path, box_size)?;
    let (width, height, rgba) = image_preview::finish(image_preview::fit(img.into(), box_size).to_rgba8());
    Ok(Payload::Image { width, height, source: (width, height), rgba, caption: "thumbnail".into() })
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use image::RgbaImage;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY,
    };

    pub fn thumbnail(path: &Path, box_size: (u32, u32)) -> Result<RgbaImage, String> {
        let name = HSTRING::from(plain(path));
        let factory: IShellItemImageFactory =
            unsafe { SHCreateItemFromParsingName(&name, None) }.map_err(|e| e.message())?;
        let size = SIZE { cx: box_size.0.max(32) as i32, cy: box_size.1.max(32) as i32 };
        // THUMBNAILONLY: a file-type icon says nothing the file list doesn't.
        let bitmap = unsafe { factory.GetImage(size, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK) }
            .map_err(|e| e.message())?;
        let bitmap = Owned(bitmap);
        read(bitmap.0)
    }

    /// The shell doesn't parse `\\?\` paths, which `canonicalize` hands out.
    fn plain(path: &Path) -> &std::ffi::OsStr {
        let s = path.as_os_str();
        match s.to_str() {
            Some(t) if t.starts_with(r"\\?\") && !t.starts_with(r"\\?\UNC\") => t[4..].as_ref(),
            _ => s,
        }
    }

    struct Owned(HBITMAP);

    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(self.0.into());
            }
        }
    }

    fn read(bitmap: HBITMAP) -> Result<RgbaImage, String> {
        let mut bm = BITMAP::default();
        let got = unsafe {
            GetObjectW(bitmap.into(), size_of::<BITMAP>() as i32, Some((&raw mut bm).cast()))
        };
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight == 0 {
            return Err("the shell returned an empty thumbnail".into());
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight.unsigned_abs());

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; w as usize * h as usize * 4];
        let rows = unsafe {
            let dc = GetDC(None);
            let rows = GetDIBits(dc, bitmap, 0, h, Some(px.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            rows
        };
        if rows != h as i32 {
            return Err("could not read the thumbnail bitmap".into());
        }
        to_rgba(&mut px);
        RgbaImage::from_raw(w, h, px).ok_or_else(|| "thumbnail buffer size mismatch".into())
    }

    /// BGRA with premultiplied alpha (what the shell hands out) to straight
    /// RGBA. Handlers that ignore alpha leave it all zero: that means opaque.
    pub(super) fn to_rgba(px: &mut [u8]) {
        // `as_chunks` rather than `chunks_exact`, so a pixel is a `[u8; 4]`
        // and the indexing below is checked once by the type instead of on
        // every access.
        let opaque = px.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
            if opaque {
                p[3] = 255;
            } else if p[3] > 0 && p[3] < 255 {
                let a = u32::from(p[3]);
                for c in &mut p[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn thumbnail(_: &std::path::Path, _: (u32, u32)) -> Result<image::RgbaImage, String> {
        Err("thumbnails come from the Windows shell".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn reads_a_jpeg_thumbnail() {
        let dir = Path::new(r"C:\Windows\Web\Wallpaper\Windows");
        let Some(jpg) = std::fs::read_dir(dir).ok().and_then(|mut d| {
            d.find_map(|e| e.ok().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")))
        }) else {
            return;
        };
        init_thread();
        let p = render(&jpg, (256, 256)).unwrap();
        let Payload::Image { width, height, rgba, .. } = &p else { panic!("not an image: {p:?}") };
        assert!(*width <= 256 && *height <= 256 && *width > 0 && *height > 0, "{width}x{height}");
        // A photo, not a blank: some variety in the pixels.
        let first = &rgba[..4];
        assert!(rgba.chunks(4).any(|px| px != first));
    }

    #[test]
    fn no_thumbnail_for_a_plain_file() {
        init_thread();
        let path = std::env::temp_dir().join("filer-shell-thumb-test.bin");
        std::fs::write(&path, b"nothing to see").unwrap();
        let r = render(&path, (256, 256));
        let _ = std::fs::remove_file(&path);
        assert!(r.is_err());
    }

    #[test]
    fn unpremultiplies_and_swaps_channels() {
        // Half-transparent pure red, premultiplied, in BGRA; and an opaque blue.
        let mut px = vec![0, 0, 128, 128, 255, 0, 0, 255];
        imp::to_rgba(&mut px);
        assert_eq!(px, [255, 0, 0, 128, 0, 0, 255, 255]);

        let mut no_alpha = vec![1, 2, 3, 0];
        imp::to_rgba(&mut no_alpha);
        assert_eq!(no_alpha, [3, 2, 1, 255]);
    }
}
