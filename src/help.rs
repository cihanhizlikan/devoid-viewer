//! The key-bindings overlay that `H` toggles.
//!
//! Drawn into the softbuffer frame after the image: the whole window dimmed,
//! with the table centred on it in Segoe UI, rendered by Windows
//! (`win32::render_text`) and blended in.
//! Nothing is rendered until the overlay is first shown, and it is rendered
//! again only when the size it needs changes.

use crate::win32::{render_text, TextMask};

/// What each key does, in the order the overlay lists them. The README's key
/// table is the same list, and a test fails when a row is missing there.
pub const BINDINGS: &[(&str, &str)] = &[
    ("Right · Down · PgDn", "Next image"),
    ("Left · Up · PgUp", "Previous image"),
    ("Mouse wheel", "Next / previous image"),
    ("Delete", "Move to the Recycle Bin"),
    ("F or F11", "Fullscreen on / off"),
    ("Esc", "Close help, leave fullscreen"),
    ("Double-click", "Maximise / restore"),
    ("Drop a file", "Open it"),
    ("H", "Show / hide this help"),
    ("Q", "Quit"),
];

const TITLE: &str = "Keyboard shortcuts";
/// Em size of the body text at 100 % DPI, in pixels.
const BASE_PX: f64 = 15.0;
/// Smallest em the overlay shrinks to in a small window; below this it clips.
const MIN_PX: u32 = 9;
const KEY_COLOUR: u32 = 0x00FF_FFFF;
const ACTION_COLOUR: u32 = 0x00C0_C0C0;

/// The overlay's text, rendered at one em size and laid out.
struct Rendered {
    px: u32,
    title: TextMask,
    rows: Vec<(TextMask, TextMask)>,
    key_w: u32,
    width: u32,
    height: u32,
}

#[derive(Default)]
pub struct Help {
    rendered: Option<Rendered>,
}

impl Help {
    /// Overlay the key table on `buf`, centred, as large as `dpi_scale` asks
    /// for and smaller if the window cannot fit it. Drawing is clipped to the
    /// buffer, so a window too small even at `MIN_PX` shows less.
    pub fn draw(&mut self, buf: &mut [u32], win_w: u32, win_h: u32, dpi_scale: f64) {
        let base = ((BASE_PX * dpi_scale).round() as u32).max(MIN_PX);
        // One render to measure, one at the estimated fit, and since hinting
        // keeps text from shrinking exactly with the em, a few one-pixel
        // steps down if the estimate still overflows.
        for _ in 0..5 {
            let px = match &self.rendered {
                Some(r) if r.is_right_for(base, win_w, win_h) => break,
                Some(r) if !r.fits(win_w, win_h) => r.fitting_px(base, win_w, win_h).min(r.px - 1),
                Some(r) => r.fitting_px(base, win_w, win_h),
                None => base,
            };
            self.rendered = Rendered::new(px);
        }
        if let Some(r) = &self.rendered {
            r.draw(buf, win_w, win_h);
        }
    }
}

/// Margin kept clear around the text, column gap, space under the title and
/// line pitch, in proportion to the em so the table looks the same at every
/// size.
struct Spacing {
    pad: u32,
    gap: u32,
    title_gap: u32,
    line: u32,
}

impl Spacing {
    fn new(px: u32, row_h: u32) -> Self {
        Spacing {
            pad: px * 3 / 2,
            gap: px * 5 / 2,
            title_gap: px,
            line: row_h + px / 3,
        }
    }
}

impl Rendered {
    fn new(px: u32) -> Option<Self> {
        let title = render_text(TITLE, px * 4 / 3, true)?;
        let rows = BINDINGS
            .iter()
            .map(|(keys, action)| {
                Some((
                    render_text(keys, px, true)?,
                    render_text(action, px, false)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let key_w = rows.iter().map(|(k, _)| k.width).max()?;
        let action_w = rows.iter().map(|(_, a)| a.width).max()?;
        let row_h = rows[0].0.height;
        let s = Spacing::new(px, row_h);
        Some(Rendered {
            px,
            width: 2 * s.pad + title.width.max(key_w + s.gap + action_w),
            height: 2 * s.pad
                + title.height
                + s.title_gap
                + (rows.len() as u32 - 1) * s.line
                + row_h,
            key_w,
            title,
            rows,
        })
    }

    fn fits(&self, win_w: u32, win_h: u32) -> bool {
        self.width <= win_w && self.height <= win_h
    }

    /// The em at which the panel would just fit the window, estimated from
    /// this rendering: text extents scale with the em, near enough.
    fn fitting_px(&self, base: u32, win_w: u32, win_h: u32) -> u32 {
        let scale = (win_w as f64 / self.width as f64).min(win_h as f64 / self.height as f64);
        ((self.px as f64 * scale).floor() as u32).clamp(MIN_PX, base)
    }

    /// Keep this rendering if it fits (or cannot shrink further) and a new
    /// one would not be meaningfully larger. The one-pixel slack stops
    /// extents that do not scale exactly from re-rendering every frame.
    fn is_right_for(&self, base: u32, win_w: u32, win_h: u32) -> bool {
        let wanted = self.fitting_px(base, win_w, win_h);
        let fits = self.fits(win_w, win_h) || self.px == MIN_PX;
        let big_enough = wanted <= self.px + 1 || self.px == base;
        fits && big_enough && self.px <= base
    }

    fn draw(&self, buf: &mut [u32], win_w: u32, win_h: u32) {
        let x0 = win_w.saturating_sub(self.width) / 2;
        let y0 = win_h.saturating_sub(self.height) / 2;

        // The whole window at quarter brightness, with no panel: no edge to
        // catch the eye, the image still recognisable behind the text. Where
        // the image is pure white it dims to #3F3F3F, under which the grey
        // action text still has a WCAG contrast of 5.8:1 (white keys 10.5:1).
        for px in buf.iter_mut() {
            *px = (*px >> 2) & 0x003F_3F3F;
        }

        let s = Spacing::new(self.px, self.rows[0].0.height);
        let (x, mut y) = (x0 + s.pad, y0 + s.pad);
        blend_mask(buf, win_w, win_h, &self.title, x, y, KEY_COLOUR);
        y += self.title.height + s.title_gap;
        for (keys, action) in &self.rows {
            blend_mask(buf, win_w, win_h, keys, x, y, KEY_COLOUR);
            blend_mask(
                buf,
                win_w,
                win_h,
                action,
                x + self.key_w + s.gap,
                y,
                ACTION_COLOUR,
            );
            y += s.line;
        }
    }
}

/// Blend `colour` into `buf` through the mask's coverage, top-left at
/// (`x`, `y`), clipped to the buffer.
fn blend_mask(
    buf: &mut [u32],
    win_w: u32,
    win_h: u32,
    mask: &TextMask,
    x: u32,
    y: u32,
    colour: u32,
) {
    let rows = mask.coverage.chunks_exact(mask.width as usize);
    for (dy, coverage) in (y..win_h).zip(rows) {
        let row = (dy * win_w) as usize;
        for (dx, &a) in (x..win_w).zip(coverage) {
            if a != 0 {
                let px = &mut buf[row + dx as usize];
                *px = blend(*px, colour, a as u32);
            }
        }
    }
}

/// `src * a + dst * (255 - a)`, per channel of a `0x00RRGGBB` pixel.
fn blend(dst: u32, src: u32, a: u32) -> u32 {
    let channel = |shift: u32| {
        let (d, s) = ((dst >> shift) & 0xFF, (src >> shift) & 0xFF);
        ((s * a + d * (255 - a)) / 255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readme_key_table_lists_every_binding() {
        let readme = include_str!("../README.md");
        for (keys, action) in BINDINGS {
            assert!(
                readme.contains(&format!("| {keys} | {action} |")),
                "README.md has no row for {keys:?} / {action:?}"
            );
        }
    }

    #[test]
    fn blend_mixes_each_channel_by_coverage() {
        assert_eq!(blend(0x0000_0000, 0x00FF_FFFF, 255), 0x00FF_FFFF);
        assert_eq!(blend(0x0012_3456, 0x00FF_FFFF, 0), 0x0012_3456);
        assert_eq!(blend(0x0000_0000, 0x00FF_0000, 51), 0x0033_0000);
    }

    #[test]
    fn the_table_is_drawn_full_size_over_a_dimmed_window() {
        let (w, h) = (1600, 1000);
        let mut buf = vec![0x00FF_FFFF; (w * h) as usize];
        let mut help = Help::default();
        help.draw(&mut buf, w, h, 1.0);
        assert_eq!(help.rendered.as_ref().expect("rendered").px, 15);
        assert!(buf.contains(&KEY_COLOUR), "text");
        assert_eq!(buf[0], 0x003F_3F3F, "corners are dimmed too");
        assert_eq!(buf[buf.len() - 1], 0x003F_3F3F);
    }

    #[test]
    fn a_small_window_gets_a_smaller_panel_that_fits() {
        let full = Rendered::new(15).expect("rendered");
        let (w, h) = (full.width * 3 / 4, full.height * 3 / 4);
        let mut help = Help::default();
        help.draw(&mut vec![0; (w * h) as usize], w, h, 1.0);
        let r = help.rendered.as_ref().expect("rendered");
        assert!(r.px < 15 && r.fits(w, h), "px {} for {w}×{h}", r.px);

        // Drawing again at the same size keeps the rendering.
        let px = r.px;
        help.draw(&mut vec![0; (w * h) as usize], w, h, 1.0);
        assert_eq!(help.rendered.as_ref().unwrap().px, px);
    }

    #[test]
    fn a_tiny_window_clips_instead_of_panicking() {
        let mut help = Help::default();
        for (w, h) in [(1, 1), (7, 3), (120, 40)] {
            help.draw(&mut vec![0; (w * h) as usize], w, h, 3.0);
        }
    }
}
