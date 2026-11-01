//! loader.rs — Image decoding for all supported formats.
//!
//! | Format          | Crate                |
//! |-----------------|----------------------|
//! | BMP/PNG/JPEG/   |                      |
//! |   WebP/TIFF/ICO | `image`              |
//! | GIF (animated)  | `image::codecs::gif` |
//! | JPEG XL (.jxl)  | `jxl-oxide`          |

use std::path::Path;

// ── Error type ────────────────────────────────────────────────────────────────

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// ── Public types ──────────────────────────────────────────────────────────────

/// One animation frame — straight RGBA8, row-major.
#[derive(Clone, Debug)]
pub struct Frame {
    pub pixels: Vec<u8>,
    /// Frame duration in milliseconds. 0 for non-animated formats.
    pub delay_ms: u32,
}

/// A decoded image: one or more frames sharing the same canvas dimensions.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub frames: Vec<Frame>,
    pub width: u32,
    pub height: u32,
}

impl DecodedImage {
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Return the frame at `idx`. Callers must ensure `idx < frames.len()`;
    /// the index is always computed via modulo in the animation ticker so
    /// this is guaranteed in practice.
    pub fn frame(&self, idx: usize) -> &Frame {
        debug_assert!(
            idx < self.frames.len(),
            "frame index {idx} out of range ({})",
            self.frames.len()
        );
        &self.frames[idx]
    }

    fn single(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            frames: vec![Frame {
                pixels,
                delay_ms: 0,
            }],
            width,
            height,
        }
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
    "bmp", "png", "jpg", "jpeg", "gif", "webp", "tiff", "tif", "ico", "jxl",
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

    if let Ok(img) = decode_by_ext(&ext, path, DecodeStrategy::ByExtension) {
        return LoadOutcome::Ok(img);
    }

    match sniff_format(path) {
        Some(actual) if actual != ext => {
            match decode_by_ext(&actual, path, DecodeStrategy::ByContent) {
                Ok(img) => LoadOutcome::WrongExtension(img, actual),
                Err(e) => LoadOutcome::Unsupported(e.to_string()),
            }
        }
        Some(_) => {
            // Sniffed the same extension but it still failed — corrupt or truncated.
            let desc = if ext.is_empty() {
                "file".into()
            } else {
                format!(".{ext} file")
            };
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
/// BMP / PNG / JPEG / WebP / TIFF / ICO all fall through to `decode_raster`.
fn decode_by_ext(
    ext: &str,
    path: &Path,
    strategy: DecodeStrategy,
) -> Result<DecodedImage, BoxError> {
    match ext {
        "jxl" => decode_jxl(path),
        "gif" => decode_gif(path),
        _ => decode_raster(path, strategy),
    }
}

// ── Format detection ──────────────────────────────────────────────────────────

/// JXL codestream signature: `FF 0A`.
const JXL_CODESTREAM_MAGIC: [u8; 2] = [0xFF, 0x0A];

/// JXL ISO BMFF container signature.
const JXL_CONTAINER_MAGIC: [u8; 12] = [
    0x00, 0x00, 0x00, 0x0C, 0x4A, 0x58, 0x4C, 0x20, 0x0D, 0x0A, 0x87, 0x0A,
];

/// Identify a raster image format from its first few bytes.
///
/// This is the single source of truth for magic-byte detection of all formats
/// the `image` crate handles. Both `sniff_format` (header sniffing for
/// wrong-extension recovery) and `decode_raster` (extension-trust validation)
/// delegate here.
fn magic_to_format(buf: &[u8]) -> Option<image::ImageFormat> {
    use image::ImageFormat;

    if buf.starts_with(&[0xFF, 0xD8]) {
        return Some(ImageFormat::Jpeg);
    }
    if buf.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some(ImageFormat::Png);
    }
    if buf.starts_with(&[0x42, 0x4D]) {
        return Some(ImageFormat::Bmp);
    }
    if buf.starts_with(b"GIF8") {
        return Some(ImageFormat::Gif);
    }
    if buf.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || buf.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some(ImageFormat::Tiff);
    }
    if buf.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some(ImageFormat::Ico);
    }
    if buf.len() >= 12 && buf.starts_with(b"RIFF") && &buf[8..12] == b"WEBP" {
        return Some(ImageFormat::WebP);
    }
    None
}

/// Map an `ImageFormat` to the canonical extension string used throughout this
/// module (e.g. `ImageFormat::Jpeg` → `"jpg"`).
fn format_to_ext(fmt: image::ImageFormat) -> Option<&'static str> {
    use image::ImageFormat;
    match fmt {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Bmp => Some("bmp"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::WebP => Some("webp"),
        ImageFormat::Tiff => Some("tiff"),
        ImageFormat::Ico => Some("ico"),
        _ => None,
    }
}

/// Read the first 32 bytes of the file to identify its format.
///
/// Returns a lowercase extension string, or `None` if unrecognised.
fn sniff_format(path: &Path) -> Option<String> {
    use std::io::Read;

    let mut buf = [0u8; 32];
    let n = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let buf = &buf[..n];

    // Common raster formats via our single magic-byte table.
    if let Some(ext) = magic_to_format(buf).and_then(format_to_ext) {
        return Some(ext.to_owned());
    }

    // JXL has its own signatures, not handled by the image crate.
    if buf.starts_with(&JXL_CODESTREAM_MAGIC) || buf.starts_with(&JXL_CONTAINER_MAGIC) {
        return Some("jxl".to_owned());
    }

    None
}

/// Read a small header from `path` and return the detected `ImageFormat`.
fn read_magic(path: &Path) -> Result<Option<image::ImageFormat>, BoxError> {
    use std::io::Read;
    let mut buf = [0u8; 12];
    let n = std::fs::File::open(path)?.read(&mut buf)?;
    Ok(magic_to_format(&buf[..n]))
}

// ── Decoders ──────────────────────────────────────────────────────────────────

/// Decode a standard raster image (BMP, PNG, JPEG, WebP, TIFF, ICO).
///
/// `ByExtension` reads the first 12 bytes and verifies that the magic bytes
/// agree with the declared extension. If they disagree the function returns
/// an error immediately — no decoding is attempted — so `load_image` falls
/// through to `sniff_format` and returns `WrongExtension`, triggering the
/// rename dialog. This is the only reliable approach: every image-crate API
/// that accepts a path (`image::open`, `with_guessed_format`) sniffs the
/// header internally and succeeds silently on misnamed files.
///
/// `ByContent` decodes purely from the file header, ignoring the extension
/// entirely — used when the extension was already confirmed to be wrong.
fn decode_raster(path: &Path, strategy: DecodeStrategy) -> Result<DecodedImage, BoxError> {
    let mut reader = image::ImageReader::open(path)?;
    match strategy {
        DecodeStrategy::ByExtension => {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let declared = image::ImageFormat::from_extension(ext)
                .ok_or_else(|| format!("unsupported extension: {ext}"))?;

            let actual = read_magic(path)?;
            if actual != Some(declared) {
                return Err(format!("magic bytes do not match extension .{ext}").into());
            }
            reader.set_format(declared);
        }
        DecodeStrategy::ByContent => reader = reader.with_guessed_format()?,
    }
    decode_to_rgba8(reader.into_decoder()?)
}

/// Decode straight into an RGBA8 buffer, so that only one full-size buffer
/// ever exists.
///
/// `DynamicImage::into_rgba8` would allocate the RGBA copy while the decoded
/// buffer is still alive: for a 12 MP JPEG (RGB8), 36 MB + 48 MB at once,
/// 33 MB above where the process settles (`docs/benchmarks.md`). Instead,
/// 8-bit RGB, grey and grey-alpha are decoded into the front of a buffer
/// already sized for RGBA and widened in place. Anything else (16-bit or
/// float samples, which only some PNG and TIFF files use) goes through
/// `DynamicImage`, where the conversion shrinks the data rather than grows it.
fn decode_to_rgba8(decoder: impl image::ImageDecoder) -> Result<DecodedImage, BoxError> {
    use image::ColorType;

    let (w, h) = decoder.dimensions();
    let channels = match decoder.color_type() {
        ColorType::Rgba8 => 4,
        ColorType::Rgb8 => 3,
        ColorType::La8 => 2,
        ColorType::L8 => 1,
        _ => {
            let rgba = image::DynamicImage::from_decoder(decoder)?.into_rgba8();
            return Ok(DecodedImage::single(rgba.into_raw(), w, h));
        }
    };
    let len = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("image dimensions overflow")?;
    let mut pixels = vec![0u8; len];
    decoder.read_image(&mut pixels[..len / 4 * channels])?;
    widen_to_rgba(&mut pixels, channels);
    Ok(DecodedImage::single(pixels, w, h))
}

/// Expand pixels of `channels` bytes (1 = grey, 2 = grey + alpha, 3 = RGB),
/// packed at the front of `buf`, to RGBA filling all of it.
///
/// Works back to front: pixel `i` is read from `i·channels` and written to
/// `i·4`, never before its own source, and never over a pixel not yet read.
fn widen_to_rgba(buf: &mut [u8], channels: usize) {
    if channels == 4 {
        return;
    }
    for i in (0..buf.len() / 4).rev() {
        let s = i * channels;
        let rgba = match channels {
            3 => [buf[s], buf[s + 1], buf[s + 2], 255],
            2 => [buf[s], buf[s], buf[s], buf[s + 1]],
            _ => [buf[s], buf[s], buf[s], 255],
        };
        buf[i * 4..i * 4 + 4].copy_from_slice(&rgba);
    }
}

fn decode_gif(path: &Path) -> Result<DecodedImage, BoxError> {
    use image::codecs::gif::GifDecoder;
    use image::{AnimationDecoder, ImageDecoder};

    let file = std::fs::File::open(path)?;
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
            let delay_ms = n.checked_div(d).map_or(100, |ms| ms.max(20));
            Frame {
                pixels: f.into_buffer().into_raw(),
                delay_ms,
            }
        })
        .collect();

    if frames.is_empty() {
        return Err("GIF has no decodable frames".into());
    }
    Ok(DecodedImage {
        frames,
        width,
        height,
    })
}

fn decode_jxl(path: &Path) -> Result<DecodedImage, BoxError> {
    use jxl_oxide::JxlImage;

    let file = std::fs::File::open(path)?;
    let mut image = JxlImage::builder().read(std::io::BufReader::new(file))?;
    image.request_color_encoding(jxl_oxide::EnumColourEncoding::srgb(
        jxl_oxide::RenderingIntent::Relative,
    ));

    // Only frame 0 is rendered. Animated JXL is not yet supported;
    // the file opens as a still image without error.
    let render = image.render_frame(0)?;
    let mut stream = render.stream();
    let width = stream.width() as usize;
    let height = stream.height() as usize;
    let channels = stream.channels() as usize;

    // Guard against absurdly large images before allocating potentially
    // gigabytes of memory. 32 768 px per side is the cap used by most
    // mainstream viewers and matches common GPU texture limits.
    const MAX_DIM: usize = 32_768;
    if width > MAX_DIM || height > MAX_DIM {
        return Err(format!(
            "JXL image too large ({width}×{height}); maximum dimension is {MAX_DIM}"
        )
        .into());
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or("JXL image dimensions overflow")?;

    let mut samples = vec![0f32; pixel_count * channels];
    stream.write_to_buffer(&mut samples);

    let rgba = jxl_samples_to_rgba8(&samples, pixel_count, channels);
    Ok(DecodedImage::single(rgba, width as u32, height as u32))
}

/// Clamp a float sample to [0, 1] and scale to a `u8`.
fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Convert interleaved float samples (1–4 channels) to RGBA8.
///
/// Missing channels are synthesised: greyscale is broadcast to RGB,
/// and missing alpha defaults to fully opaque.
fn jxl_samples_to_rgba8(samples: &[f32], pixel_count: usize, channels: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; pixel_count * 4];
    for i in 0..pixel_count {
        let s = &samples[i * channels..][..channels];
        let d = &mut rgba[i * 4..][..4];
        let (r, g, b, a) = match channels {
            1 => (s[0], s[0], s[0], 1.0),
            2 => (s[0], s[0], s[0], s[1]),
            3 => (s[0], s[1], s[2], 1.0),
            _ => (s[0], s[1], s[2], s[3]),
        };
        d[0] = f32_to_u8(r);
        d[1] = f32_to_u8(g);
        d[2] = f32_to_u8(b);
        d[3] = f32_to_u8(a);
    }
    rgba
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use std::io::Cursor;

    /// A 2×1 image: opaque red, half-transparent blue.
    fn sample_rgba() -> RgbaImage {
        RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 128])
            }
        })
    }

    /// Encode the sample in `format`. JPEG has no alpha channel, so it
    /// gets an RGB image instead.
    fn encode(format: ImageFormat) -> Vec<u8> {
        let img = match format {
            ImageFormat::Jpeg => {
                DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 1, Rgb([255, 0, 0])))
            }
            _ => DynamicImage::ImageRgba8(sample_rgba()),
        };
        let mut bytes = Cursor::new(Vec::new());
        img.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn expect_ok(outcome: LoadOutcome) -> DecodedImage {
        match outcome {
            LoadOutcome::Ok(img) => img,
            LoadOutcome::WrongExtension(_, ext) => panic!("expected Ok, got WrongExtension({ext})"),
            LoadOutcome::Unsupported(reason) => panic!("expected Ok, got Unsupported({reason})"),
        }
    }

    // ── Extension filter ──────────────────────────────────────────────────────

    #[test]
    fn supported_extensions_are_matched_case_insensitively() {
        assert!(is_supported(Path::new("a.PNG")));
        assert!(is_supported(Path::new("dir/b.jxl")));
        assert!(is_supported(Path::new("c.Tif")));
        assert!(!is_supported(Path::new("d.svg")));
        assert!(!is_supported(Path::new("e.txt")));
        assert!(!is_supported(Path::new("png")));
    }

    /// The installer registers file associations by hand; every format we
    /// open must be offered there, and nothing we cannot open.
    #[test]
    fn installer_associates_exactly_the_supported_extensions() {
        let iss = include_str!("../devoid_viewer_setup.iss");
        let mut associated: Vec<&str> = iss
            .lines()
            .filter(|l| l.contains("\\OpenWithProgIds"))
            .filter_map(|l| l.split("Classes\\.").nth(1)?.split('\\').next())
            .collect();
        associated.sort_unstable();
        associated.dedup();
        let mut supported = SUPPORTED_EXTENSIONS.to_vec();
        supported.sort_unstable();
        assert_eq!(associated, supported);
    }

    // ── Magic bytes ───────────────────────────────────────────────────────────

    #[test]
    fn magic_bytes_identify_each_raster_format() {
        use ImageFormat::*;
        let cases: &[(&[u8], ImageFormat)] = &[
            (&[0xFF, 0xD8, 0xFF], Jpeg),
            (&[0x89, b'P', b'N', b'G'], Png),
            (b"BM", Bmp),
            (b"GIF89a", Gif),
            (&[0x49, 0x49, 0x2A, 0x00], Tiff),
            (&[0x4D, 0x4D, 0x00, 0x2A], Tiff),
            (&[0x00, 0x00, 0x01, 0x00], Ico),
            (b"RIFF\0\0\0\0WEBP", WebP),
        ];
        for (bytes, format) in cases {
            assert_eq!(magic_to_format(bytes), Some(*format), "{bytes:02X?}");
        }
    }

    #[test]
    fn riff_without_webp_and_short_buffers_are_unknown() {
        assert_eq!(magic_to_format(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(magic_to_format(b"RIFF"), None);
        assert_eq!(magic_to_format(&[]), None);
        assert_eq!(magic_to_format(&[0xFF]), None);
    }

    #[test]
    fn both_jxl_signatures_are_sniffed() {
        let dir = tempfile::tempdir().unwrap();
        let stream = write(&dir, "a.bin", &[0xFF, 0x0A, 0, 0]);
        let container = write(&dir, "b.bin", &JXL_CONTAINER_MAGIC);
        assert_eq!(sniff_format(&stream).as_deref(), Some("jxl"));
        assert_eq!(sniff_format(&container).as_deref(), Some("jxl"));
    }

    // ── Decoding ──────────────────────────────────────────────────────────────

    #[test]
    fn every_encodable_format_decodes_under_its_own_extension() {
        let dir = tempfile::tempdir().unwrap();
        for (format, ext) in [
            (ImageFormat::Png, "png"),
            (ImageFormat::Jpeg, "jpg"),
            (ImageFormat::Jpeg, "jpeg"),
            (ImageFormat::Bmp, "bmp"),
            (ImageFormat::Gif, "gif"),
            (ImageFormat::WebP, "webp"),
            (ImageFormat::Tiff, "tiff"),
            (ImageFormat::Tiff, "tif"),
            (ImageFormat::Ico, "ico"),
        ] {
            let path = write(&dir, &format!("img.{ext}"), &encode(format));
            let img = expect_ok(load_image(&path));
            assert_eq!((img.width, img.height), (2, 1), ".{ext}");
            assert_eq!(img.frames.len(), 1, ".{ext}");
            assert_eq!(img.frames[0].pixels.len(), 2 * 4, ".{ext}");
        }
    }

    #[test]
    fn png_pixels_round_trip_as_straight_rgba() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "a.png", &encode(ImageFormat::Png));
        let img = expect_ok(load_image(&path));
        assert_eq!(img.frames[0].pixels, [255, 0, 0, 255, 0, 0, 255, 128]);
    }

    #[test]
    fn widening_in_place_expands_every_pixel() {
        let mut rgb = [1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        widen_to_rgba(&mut rgb[..8], 3);
        assert_eq!(rgb[..8], [1, 2, 3, 255, 4, 5, 6, 255]);

        let mut grey = [7, 9, 0, 0, 0, 0, 0, 0];
        widen_to_rgba(&mut grey, 1);
        assert_eq!(grey, [7, 7, 7, 255, 9, 9, 9, 255]);

        let mut grey_alpha = [7, 100, 9, 200, 0, 0, 0, 0];
        widen_to_rgba(&mut grey_alpha, 2);
        assert_eq!(grey_alpha, [7, 7, 7, 100, 9, 9, 9, 200]);
    }

    /// Every colour type a decoder can report lands as the same straight RGBA,
    /// whether it is widened in place or converted through `DynamicImage`.
    #[test]
    fn every_png_colour_type_decodes_to_the_same_rgba() {
        use image::{GrayAlphaImage, GrayImage, ImageBuffer, Luma, LumaA};

        let dir = tempfile::tempdir().unwrap();
        let cases: [(&str, DynamicImage, [u8; 8]); 4] = [
            (
                "rgb8",
                DynamicImage::ImageRgb8(RgbImage::from_fn(2, 1, |x, _| {
                    Rgb([10, 20, 30 + x as u8])
                })),
                [10, 20, 30, 255, 10, 20, 31, 255],
            ),
            (
                "l8",
                DynamicImage::ImageLuma8(GrayImage::from_fn(2, 1, |x, _| Luma([40 + x as u8]))),
                [40, 40, 40, 255, 41, 41, 41, 255],
            ),
            (
                "la8",
                DynamicImage::ImageLumaA8(GrayAlphaImage::from_fn(2, 1, |x, _| {
                    LumaA([50, 60 + x as u8])
                })),
                [50, 50, 50, 60, 50, 50, 50, 61],
            ),
            (
                "rgb16",
                DynamicImage::ImageRgb16(ImageBuffer::from_pixel(2, 1, Rgb([0xFFFF, 0, 0x8080]))),
                [255, 0, 128, 255, 255, 0, 128, 255],
            ),
        ];
        for (name, image, expected) in cases {
            let mut bytes = Vec::new();
            image
                .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
                .unwrap();
            let path = write(&dir, &format!("{name}.png"), &bytes);
            assert_eq!(
                expect_ok(load_image(&path)).frames[0].pixels,
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn a_misnamed_file_reports_its_real_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "photo.jpg", &encode(ImageFormat::Png));
        match load_image(&path) {
            LoadOutcome::WrongExtension(img, ext) => {
                assert_eq!(ext, "png");
                assert_eq!((img.width, img.height), (2, 1));
            }
            _ => panic!("expected WrongExtension"),
        }
    }

    #[test]
    fn a_file_without_extension_is_decoded_by_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "photo", &encode(ImageFormat::Png));
        assert!(matches!(load_image(&path), LoadOutcome::WrongExtension(_, ext) if ext == "png"));
    }

    #[test]
    fn a_truncated_file_is_unsupported_not_misnamed() {
        let dir = tempfile::tempdir().unwrap();
        let png = encode(ImageFormat::Png);
        let path = write(&dir, "cut.png", &png[..png.len() / 2]);
        match load_image(&path) {
            LoadOutcome::Unsupported(reason) => assert_eq!(reason, "Failed to decode .png file"),
            _ => panic!("expected Unsupported"),
        }
    }

    #[test]
    fn non_image_content_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "fake.png", b"definitely not an image");
        match load_image(&path) {
            LoadOutcome::Unsupported(reason) => assert_eq!(reason, "Not a recognised image format"),
            _ => panic!("expected Unsupported"),
        }
    }

    #[test]
    fn animated_gif_keeps_every_frame_and_clamps_zero_delays() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame as GifFrame};

        let mut bytes = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut bytes);
            for ms in [0, 50] {
                let frame =
                    GifFrame::from_parts(sample_rgba(), 0, 0, Delay::from_numer_denom_ms(ms, 1));
                enc.encode_frame(frame).unwrap();
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "anim.gif", &bytes);
        let img = expect_ok(load_image(&path));
        assert!(img.is_animated());
        let delays: Vec<u32> = img.frames.iter().map(|f| f.delay_ms).collect();
        assert_eq!(delays, [20, 50]);
    }

    // ── JXL sample conversion ─────────────────────────────────────────────────

    #[test]
    fn jxl_samples_fill_missing_channels() {
        assert_eq!(jxl_samples_to_rgba8(&[0.5], 1, 1), [128, 128, 128, 255]);
        assert_eq!(jxl_samples_to_rgba8(&[1.0, 0.0], 1, 2), [255, 255, 255, 0]);
        assert_eq!(
            jxl_samples_to_rgba8(&[1.0, 0.0, 0.0], 1, 3),
            [255, 0, 0, 255]
        );
        assert_eq!(
            jxl_samples_to_rgba8(&[0.0, 1.0, 0.0, 0.5], 1, 4),
            [0, 255, 0, 128]
        );
    }

    #[test]
    fn jxl_samples_are_clamped() {
        assert_eq!(
            jxl_samples_to_rgba8(&[-0.5, 2.0, f32::NAN], 1, 3)[..3],
            [0, 255, 0]
        );
    }
}
