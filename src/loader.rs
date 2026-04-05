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

    /// Return the frame at `idx`. Callers must ensure `idx < frames.len()`;
    /// the index is always computed via modulo in the animation ticker so
    /// this is guaranteed in practice.
    pub fn frame(&self, idx: usize) -> &Frame {
        debug_assert!(idx < self.frames.len(), "frame index {idx} out of range ({})", self.frames.len());
        &self.frames[idx]
    }

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

    if let Ok(img) = load_by_ext(&ext, path, DecodeStrategy::ByExtension) {
        return LoadOutcome::Ok(img);
    }

    match sniff_format(path) {
        Some(actual) if actual != ext => {
            match load_by_ext(&actual, path, DecodeStrategy::ByContent) {
                Ok(img) => LoadOutcome::WrongExtension(img, actual),
                Err(e)  => LoadOutcome::Unsupported(e.to_string()),
            }
        }
        Some(_) => {
            // Sniffed the same extension but it still failed — the file is
            // corrupt or truncated.
            let desc = if ext.is_empty() { "file".into() } else { format!(".{ext} file") };
            LoadOutcome::Unsupported(format!("Failed to decode {desc}"))
        }
        None => LoadOutcome::Unsupported("Not a recognised image format".into()),
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Return the lowercased file extension, or an empty string.
fn file_ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// Whether the standard raster decoder should trust the file extension or
/// sniff the header to pick the format.
#[derive(Clone, Copy)]
enum DecodeStrategy {
    /// Trust the file extension — the fast, normal path.
    ByExtension,
    /// Ignore the extension; read magic bytes to determine the format.
    /// Used when the extension was found to be wrong.
    ByContent,
}

/// Dispatch to the correct decoder based on `ext`.
/// BMP / PNG / JPEG / WebP / TIFF / ICO all fall through to `load_raster`.
fn load_by_ext(
    ext:      &str,
    path:     &Path,
    strategy: DecodeStrategy,
) -> Result<DecodedImage, BoxError> {
    match ext {
        "svg" => load_svg(path),
        "jxl" => load_jxl(path),
        "gif" => load_gif(path),
        _     => load_raster(path, strategy),
    }
}

/// Read the first 32 bytes of the file to identify its format.
///
/// Returns a lowercase extension string, or `None` if unrecognised.
/// Reading happens once; the bytes are passed to both the image-crate
/// guesser and the manual signature checks for JXL and SVG.
fn sniff_format(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = [0u8; 32];
    let n   = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
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

    // SVG: look for XML's opening '<' after an optional UTF-8 BOM.
    // This is intentionally broad — any XML-like file matches — but false
    // positives are harmless: load_svg() will fail cleanly on non-SVG XML,
    // and this path is only reached after the declared extension already
    // failed to decode.
    let text = std::str::from_utf8(buf)
        .unwrap_or("")
        .trim_start_matches('\u{FEFF}')
        .trim_start();
    if text.starts_with('<') {
        return Some("svg".to_owned());
    }

    None
}

fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

// ── Decoders ──────────────────────────────────────────────────────────────────

/// Decode a standard raster image (BMP, PNG, JPEG, WebP, TIFF, ICO).
///
/// `ByExtension` trusts the file extension — the fast, normal path.
/// `ByContent` reads the file header to detect the format, ignoring the
/// extension — used when the extension was found to be wrong.
fn load_raster(path: &Path, strategy: DecodeStrategy) -> Result<DecodedImage, BoxError> {
    let img = match strategy {
        DecodeStrategy::ByExtension => image::open(path)?,
        DecodeStrategy::ByContent   => {
            let file = std::fs::File::open(path)?;
            image::ImageReader::new(std::io::BufReader::new(file))
                .with_guessed_format()?
                .decode()?
        }
    };
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

    // Corrupt individual frames are skipped rather than aborting the
    // whole decode — a partially broken GIF is more useful than nothing.
    // The is_empty() guard below catches the case where all frames fail.
    let frames: Vec<Frame> = decoder
        .into_frames()
        .filter_map(|f| f.ok())
        .map(|f| {
            let (n, d) = f.delay().numer_denom_ms();
            // d==0 means a malformed delay ratio in the GIF stream;
            // fall back to 100 ms (10 fps) rather than dividing by zero.
            // Also clamp to ≥ 20 ms: some encoders emit 0-delay frames
            // as a hack for "as fast as possible", which would busy-loop
            // the event loop.
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

    // Only frame 0 is rendered. Animated JXL is not yet supported;
    // the file opens as a still image without error.
    let render      = image.render_frame(0)?;
    let mut stream  = render.stream();
    let width       = stream.width()    as usize;
    let height      = stream.height()   as usize;
    let channels    = stream.channels() as usize;

    // Guard against absurdly large images before allocating potentially
    // gigabytes of memory. 32 768 px per side is the cap used by most
    // mainstream viewers and matches common GPU texture limits.
    const MAX_DIM: usize = 32_768;
    if width > MAX_DIM || height > MAX_DIM {
        return Err(format!(
            "JXL image too large ({width}×{height}); maximum dimension is {MAX_DIM}"
        ).into());
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or("JXL image dimensions overflow")?;

    let mut samples = vec![0f32; pixel_count * channels];
    stream.write_to_buffer(&mut samples);

    // Convert f32 samples to RGBA8. Alpha is pre-filled to 255 (opaque) so the
    // 1- and 3-channel cases only need to write RGB.
    let mut rgba = vec![255u8; pixel_count * 4];
    for i in 0..pixel_count {
        let s = i * channels;
        let d = i * 4;
        let (r, g, b, a) = match channels {
            1 => { let v = samples[s]; (v, v, v, 1.0) }
            2 => { let v = samples[s]; (v, v, v, samples[s + 1]) }
            3 => (samples[s], samples[s+1], samples[s+2], 1.0),
            // We requested sRGB output, so 4-channel RGBA is the only
            // remaining valid case. 5+ channels cannot occur here.
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
        .ok_or_else(|| format!("failed to allocate SVG pixmap ({width}×{height})"))?;
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
