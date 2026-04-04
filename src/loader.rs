//! loader.rs — Image decoding for all supported formats.
//!
//! | Format          | Crate                           |
//! |-----------------|---------------------------------|
//! | BMP/PNG/JPEG/   |                                 |
//! |   WebP/TIFF/ICO | `image`                         |
//! | GIF (animated)  | `image::codecs::gif`            |
//! | JPEG XL (.jxl)  | `jxl-oxide`                     |
//! | SVG             | `resvg` + `usvg` + `tiny-skia`  |

use std::path::Path;

// ── Error type ────────────────────────────────────────────────────────────────

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// ── Public types ──────────────────────────────────────────────────────────────

/// One animation frame — straight RGBA8, row-major.
#[derive(Clone, Debug)]
pub struct Frame {
    pub pixels:   Vec<u8>,
    /// Frame duration in milliseconds. 0 for non-animated formats.
    pub delay_ms: u32,
}

/// A decoded image: one or more frames sharing the same canvas dimensions.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub frames: Vec<Frame>,
    pub width:  u32,
    pub height: u32,
}

impl DecodedImage {
    pub fn is_animated(&self) -> bool { self.frames.len() > 1 }
    pub fn frame(&self, idx: usize) -> &Frame { &self.frames[idx] }

    fn single(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self { frames: vec![Frame { pixels, delay_ms: 0 }], width, height }
    }
}

/// The outcome of a `load_image` call.
pub enum LoadOutcome {
    /// Decoded successfully using the file's declared extension.
    Ok(DecodedImage),
    /// The declared extension was wrong; decoded by sniffing the file header.
    /// Carries the image and the correct extension (e.g. `"jpg"`).
    WrongExtension(DecodedImage, String),
    /// Not a recognised image format even after header sniffing.
    Unsupported(String),
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// File extensions Devoid Viewer can open.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "bmp", "png", "jpg", "jpeg", "gif", "webp",
    "tiff", "tif", "ico", "jxl", "svg",
];

/// Returns `true` when the path has a supported image extension.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Decode an image file to straight RGBA8.
///
/// First tries the declared extension (fast path). On failure, reads the file
/// header to detect the actual format. Returns `WrongExtension` when the file
/// decodes successfully under a different format, or `Unsupported` when the
/// content is not a recognised image.
pub fn load_image(path: &Path) -> LoadOutcome {
    let ext = file_ext(path);

    if let Ok(img) = load_by_ext(&ext, path) {
        return LoadOutcome::Ok(img);
    }

    match sniff_format(path) {
        Some(actual) if actual != ext => {
            match load_by_ext(&actual, path) {
                Ok(img) => LoadOutcome::WrongExtension(img, actual),
                Err(e)  => LoadOutcome::Unsupported(e.to_string()),
            }
        }
        Some(_) => {
            // Sniffed the same extension but it still failed — the file is
            // corrupt or truncated.
            LoadOutcome::Unsupported(format!("Failed to decode {ext} file"))
        }
        None => LoadOutcome::Unsupported("Not a recognised image format".into()),
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Return the lowercased file extension, or an empty string.
fn file_ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Dispatch to the correct decoder based on `ext`.
/// BMP / PNG / JPEG / WebP / TIFF / ICO all fall through to `load_standard`.
fn load_by_ext(ext: &str, path: &Path) -> Result<DecodedImage, BoxError> {
    match ext {
        "svg" => load_svg(path),
        "jxl" => load_jxl(path),
        "gif" => load_gif(path),
        _     => load_standard(path),
    }
}

/// Read the first 32 bytes of the file to identify its format.
///
/// Returns a lowercase extension string, or `None` if unrecognised.
/// Reading happens once and the bytes are passed to both the image-crate
/// guesser and the manual signature checks for JXL and SVG.
fn sniff_format(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = [0u8; 32];
    let n = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let buf = &buf[..n];

    // Use the image crate's format guesser for common raster formats.
    if let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(buf))
        .with_guessed_format()
    {
        let ext = match reader.format() {
            Some(image::ImageFormat::Png)  => Some("png"),
            Some(image::ImageFormat::Jpeg) => Some("jpg"),
            Some(image::ImageFormat::Bmp)  => Some("bmp"),
            Some(image::ImageFormat::Gif)  => Some("gif"),
            Some(image::ImageFormat::WebP) => Some("webp"),
            Some(image::ImageFormat::Tiff) => Some("tiff"),
            Some(image::ImageFormat::Ico)  => Some("ico"),
            _                              => None,
        };
        if let Some(ext) = ext {
            return Some(ext.to_owned());
        }
    }

    // JXL codestream signature: FF 0A
    // JXL ISO BMFF container:   00 00 00 0C 4A 58 4C 20 0D 0A 87 0A
    if buf.starts_with(&[0xFF, 0x0A])
        || buf.starts_with(&[0x00,0x00,0x00,0x0C,0x4A,0x58,0x4C,0x20,
                              0x0D,0x0A,0x87,0x0A])
    {
        return Some("jxl".to_owned());
    }

    // SVG: XML begins with '<' (possibly after a UTF-8 BOM).
    // We inspect up to 32 bytes which is sufficient for any practical SVG.
    let text = std::str::from_utf8(buf).unwrap_or("").trim_start_matches('\u{FEFF}').trim_start();
    if text.starts_with('<') {
        return Some("svg".to_owned());
    }

    None
}

#[inline]
fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

// ── Decoders ──────────────────────────────────────────────────────────────────

fn load_standard(path: &Path) -> Result<DecodedImage, BoxError> {
    // Use with_guessed_format() so decoding is driven by the file header,
    // not the extension. This is the correct path for both normal files and
    // the wrong-extension fallback triggered by sniff_format().
    let file   = std::fs::File::open(path)?;
    let reader = image::ImageReader::new(std::io::BufReader::new(file))
        .with_guessed_format()?;
    let img    = reader.decode()?;
    let rgba   = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(DecodedImage::single(rgba.into_raw(), w, h))
}

fn load_gif(path: &Path) -> Result<DecodedImage, BoxError> {
    use image::{AnimationDecoder, ImageDecoder};
    use image::codecs::gif::GifDecoder;

    let file    = std::fs::File::open(path)?;
    let decoder = GifDecoder::new(std::io::BufReader::new(file))?;
    let (width, height) = decoder.dimensions();

    let frames: Vec<Frame> = decoder
        .into_frames()
        .filter_map(|f| f.ok())
        .map(|f| {
            let (n, d) = f.delay().numer_denom_ms();
            // Clamp to ≥ 20 ms to prevent zero-delay GIFs from locking the event loop.
            let delay_ms = if d == 0 { 100 } else { (n / d).max(20) };
            Frame { pixels: f.into_buffer().into_raw(), delay_ms }
        })
        .collect();

    if frames.is_empty() { return Err("GIF has no frames".into()); }
    Ok(DecodedImage { frames, width, height })
}

fn load_jxl(path: &Path) -> Result<DecodedImage, BoxError> {
    use jxl_oxide::JxlImage;

    let file      = std::fs::File::open(path)?;
    let mut image = JxlImage::builder().read(std::io::BufReader::new(file))?;
    image.request_color_encoding(jxl_oxide::EnumColourEncoding::srgb(
        jxl_oxide::RenderingIntent::Relative,
    ));

    let render      = image.render_frame(0)?;
    let mut stream  = render.stream();
    let width       = stream.width()    as usize;
    let height      = stream.height()   as usize;
    let channels    = stream.channels() as usize;
    let pixel_count = width * height;

    let mut samples = vec![0f32; pixel_count * channels];
    stream.write_to_buffer(&mut samples);

    // Convert f32 samples to RGBA8. Pre-fill alpha to 255 so the opaque
    // cases (1 or 3 channels) don't need to touch it.
    let mut rgba = vec![255u8; pixel_count * 4];
    for i in 0..pixel_count {
        let s = i * channels;
        let d = i * 4;
        let (r, g, b, a) = match channels {
            1 => { let v = samples[s]; (v, v, v, 1.0) }
            2 => { let v = samples[s]; (v, v, v, samples[s + 1]) }
            3 => (samples[s], samples[s+1], samples[s+2], 1.0),
            _ => (samples[s], samples[s+1], samples[s+2], samples[s+3]),
        };
        rgba[d]   = f32_to_u8(r);
        rgba[d+1] = f32_to_u8(g);
        rgba[d+2] = f32_to_u8(b);
        rgba[d+3] = f32_to_u8(a);
    }

    Ok(DecodedImage::single(rgba, width as u32, height as u32))
}

fn load_svg(path: &Path) -> Result<DecodedImage, BoxError> {
    let data = std::fs::read(path)?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default())?;

    let sz     = tree.size();
    let width  = sz.width()  as u32;
    let height = sz.height() as u32;
    if width == 0 || height == 0 {
        return Err("SVG has zero dimensions".into());
    }

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or("failed to allocate SVG pixmap")?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    // tiny-skia produces premultiplied RGBA; convert to straight RGBA.
    let pixels: Vec<u8> = pixmap.pixels().iter().flat_map(|p| {
        let a = p.alpha();
        if a == 0 {
            [0u8, 0, 0, 0]
        } else {
            let inv = 255.0 / a as f32;
            [
                (p.red()   as f32 * inv).round() as u8,
                (p.green() as f32 * inv).round() as u8,
                (p.blue()  as f32 * inv).round() as u8,
                a,
            ]
        }
    }).collect();

    Ok(DecodedImage::single(pixels, width, height))
}
