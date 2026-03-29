//! loader.rs — Image decoding for all supported formats.
//!
//! Returns `DecodedImage` directly — no conversion needed in main.rs.
//!
//! | Format               | Crate                          |
//! |----------------------|--------------------------------|
//! | BMP/PNG/JPEG/        |                                |
//! |   WebP/TIFF/ICO      | `image`                        |
//! | GIF (animated)       | `image::codecs::gif`           |
//! | JPEG XL (.jxl)       | `jxl-oxide`                    |
//! | SVG                  | `resvg` + `usvg` + `tiny-skia` |

use std::path::Path;

pub type LoadResult = Result<DecodedImage, Box<dyn std::error::Error + Send + Sync>>;

// ── Public types ──────────────────────────────────────────────────────────────

/// One animation frame — straight RGBA8, row-major.
#[derive(Clone, Debug)]
pub struct Frame {
    pub pixels:   Vec<u8>,
    pub delay_ms: u32,   // 0 for non-animated formats
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

// ── Public interface ──────────────────────────────────────────────────────────

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "bmp", "png", "jpg", "jpeg", "gif", "webp",
    "tiff", "tif", "ico", "jxl", "svg",
];

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Decode any supported image file to straight RGBA8.
pub fn load_image(path: &Path) -> LoadResult {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => load_svg(path),
        Some("jxl") => load_jxl(path),
        Some("gif") => load_gif(path),
        _           => load_standard(path),
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

#[inline]
fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn pixels_from_dynamic(img: image::DynamicImage) -> (Vec<u8>, u32, u32) {
    let rgba   = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    (rgba.into_raw(), w, h)
}

// ── Decoders ──────────────────────────────────────────────────────────────────

fn load_standard(path: &Path) -> LoadResult {
    let (pixels, w, h) = pixels_from_dynamic(image::open(path)?);
    Ok(DecodedImage::single(pixels, w, h))
}

fn load_gif(path: &Path) -> LoadResult {
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
            let delay_ms = if d == 0 { 100 } else { (n / d).max(20) };
            Frame { pixels: f.into_buffer().into_raw(), delay_ms }
        })
        .collect();

    if frames.is_empty() { return Err("GIF has no frames".into()); }
    Ok(DecodedImage { frames, width, height })
}

fn load_jxl(path: &Path) -> LoadResult {
    use jxl_oxide::JxlImage;

    let file = std::fs::File::open(path)?;
    let mut image = JxlImage::builder().read(std::io::BufReader::new(file))?;
    image.request_color_encoding(jxl_oxide::EnumColourEncoding::srgb(
        jxl_oxide::RenderingIntent::Relative,
    ));

    let render     = image.render_frame(0)?;
    let mut stream = render.stream();
    let width      = stream.width()    as usize;
    let height     = stream.height()   as usize;
    let channels   = stream.channels() as usize;

    let mut samples = vec![0f32; width * height * channels];
    stream.write_to_buffer(&mut samples);

    let mut rgba = vec![255u8; width * height * 4];
    for i in 0..(width * height) {
        let s = i * channels;
        let d = i * 4;
        match channels {
            1 => { let v = f32_to_u8(samples[s]);
                   rgba[d]=v; rgba[d+1]=v; rgba[d+2]=v; }
            2 => { let v = f32_to_u8(samples[s]);
                   rgba[d]=v; rgba[d+1]=v; rgba[d+2]=v;
                   rgba[d+3]=f32_to_u8(samples[s+1]); }
            3 => { rgba[d]  =f32_to_u8(samples[s]);
                   rgba[d+1]=f32_to_u8(samples[s+1]);
                   rgba[d+2]=f32_to_u8(samples[s+2]); }
            _ => { rgba[d]  =f32_to_u8(samples[s]);
                   rgba[d+1]=f32_to_u8(samples[s+1]);
                   rgba[d+2]=f32_to_u8(samples[s+2]);
                   rgba[d+3]=f32_to_u8(samples[s+3]); }
        }
    }

    Ok(DecodedImage::single(rgba, width as u32, height as u32))
}

fn load_svg(path: &Path) -> LoadResult {
    let data = std::fs::read(path)?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default())?;

    let sz     = tree.size();
    let width  = sz.width()  as u32;
    let height = sz.height() as u32;
    if width == 0 || height == 0 { return Err("SVG has zero dimensions".into()); }

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or("Failed to allocate SVG pixmap")?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    // Unpremultiply: tiny-skia produces premultiplied RGBA, we need straight.
    let pixels: Vec<u8> = pixmap.pixels().iter().flat_map(|p| {
        let a = p.alpha();
        if a == 0 { [0u8, 0, 0, 0] } else {
            let inv = 255.0 / a as f32;
            [(p.red()   as f32 * inv).round() as u8,
             (p.green() as f32 * inv).round() as u8,
             (p.blue()  as f32 * inv).round() as u8, a]
        }
    }).collect();

    Ok(DecodedImage::single(pixels, width, height))
}
