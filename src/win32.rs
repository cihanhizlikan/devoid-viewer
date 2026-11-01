//! Every Win32 call the viewer makes, behind safe functions.
//!
//! This is the only module allowed `unsafe` (`unsafe_code = "deny"` in
//! Cargo.toml, allowed here). Each `unsafe` block is one FFI call with a
//! `SAFETY:` comment; no function here passes a raw pointer in or out, so
//! the rest of the crate cannot misuse one.

#![allow(unsafe_code)]

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject, GetDC,
    GetDIBits, GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    TextOutW, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, FW_NORMAL, FW_SEMIBOLD, OPAQUE,
    OUT_DEFAULT_PRECIS,
};
use windows_sys::Win32::UI::Shell::StrCmpLogicalW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, PostMessageW, SystemParametersInfoW, IDYES, MB_ICONERROR, MB_ICONQUESTION,
    MB_ICONWARNING, MB_OK, MB_YESNO, MSG, SC_CLOSE, SC_MAXIMIZE, SPI_GETWORKAREA, WM_CLOSE,
    WM_SYSCOMMAND,
};
use winit::event_loop::EventLoopBuilder;
use winit::platform::windows::EventLoopBuilderExtWindows;

/// Encode a `&str` as a null-terminated UTF-16 `Vec` for Win32 API calls.
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Icon of a yes/no question box.
pub enum Ask {
    Warning,
    Question,
}

/// Modal yes/no message box with no owner window. `true` on Yes.
pub fn ask_yes_no(text: &str, caption: &str, icon: Ask) -> bool {
    let icon = match icon {
        Ask::Warning => MB_ICONWARNING,
        Ask::Question => MB_ICONQUESTION,
    };
    let (text, caption) = (to_wide(text), to_wide(caption));
    // SAFETY: both buffers are null-terminated by `to_wide` and outlive the
    // call; a null owner window is allowed.
    unsafe { MessageBoxW(null_mut(), text.as_ptr(), caption.as_ptr(), MB_YESNO | icon) == IDYES }
}

/// Modal error message box with an OK button and no owner window.
pub fn show_error(text: &str, caption: &str) {
    let (text, caption) = (to_wide(text), to_wide(caption));
    // SAFETY: as in `ask_yes_no`.
    unsafe {
        MessageBoxW(
            null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_ICONERROR | MB_OK,
        );
    }
}

/// Work area rectangle in physical pixels: (left, top, right, bottom).
///
/// `SPI_GETWORKAREA` — the desktop area excluding the taskbar, regardless
/// of which edge it is on. `None` if the call fails or the area is empty.
pub fn work_area() -> Option<(i32, i32, i32, i32)> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: SPI_GETWORKAREA writes exactly one RECT through the pointer,
    // which points at a live, writable local.
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0) };
    (ok != 0 && (r.right - r.left) > 0 && (r.bottom - r.top) > 0)
        .then_some((r.left, r.top, r.right, r.bottom))
}

/// Compare two null-terminated UTF-16 strings the way Explorer sorts names:
/// numeric runs by value, case-insensitive, locale-aware, with hyphens and
/// apostrophes ignored.
///
/// # Panics
/// If either slice is not null-terminated. That is a bug in the caller, and
/// the check is what makes this function safe for any input.
pub fn compare_logical(a: &[u16], b: &[u16]) -> Ordering {
    assert!(
        a.last() == Some(&0) && b.last() == Some(&0),
        "compare_logical needs null-terminated strings"
    );
    // SAFETY: both slices end in a null (checked above) and outlive the
    // call, which only reads up to that null.
    let r = unsafe { StrCmpLogicalW(a.as_ptr(), b.as_ptr()) };
    r.cmp(&0)
}

/// Intercept two system commands before winit sees them:
///
/// - **SC_CLOSE** (the taskbar thumbnail's ×): winit 0.29 does not turn it
///   into `CloseRequested`, so it is re-posted as WM_CLOSE, which winit
///   does handle.
/// - **SC_MAXIMIZE** (title-bar double-click and the maximise button):
///   suppressed, so `toggle_maximize()` is the one code path for maximise,
///   shared with the client-area double-click.
pub fn install_system_command_hook<T: 'static>(builder: &mut EventLoopBuilder<T>) {
    builder.with_msg_hook(|msg| {
        // SAFETY: winit calls the hook with a pointer to the MSG it has
        // just retrieved from the message queue, valid for this call.
        let msg = unsafe { &*(msg as *const MSG) };
        if msg.message == WM_SYSCOMMAND {
            match (msg.wParam & 0xFFF0) as u32 {
                SC_CLOSE => {
                    // SAFETY: plain message post to a window handle the OS
                    // gave us; no pointers are passed.
                    unsafe { PostMessageW(msg.hwnd, WM_CLOSE, 0, 0) };
                }
                SC_MAXIMIZE => return true,
                _ => {}
            }
        }
        false
    });
}

/// Runs its closure when dropped: releases a GDI handle on every exit path.
/// Guards drop in reverse order, so a deselect declared after its object's
/// delete runs first, as GDI requires.
struct Release<F: FnMut()>(F);

impl<F: FnMut()> Drop for Release<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}

/// Text rendered by [`render_text`]: `coverage` holds one byte per pixel,
/// row by row, 0 for background and 255 for solid ink.
pub struct TextMask {
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

/// Render one line of `text` in Segoe UI with a `px`-pixel em, greyscale
/// antialiased, the way Windows draws its own interface text. `None` if any
/// GDI call fails or the text is empty.
pub fn render_text(text: &str, px: u32, semibold: bool) -> Option<TextMask> {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let len = i32::try_from(utf16.len()).ok().filter(|&n| n > 0)?;
    let face = to_wide("Segoe UI");

    // SAFETY: a null window asks for a DC of the whole screen.
    let screen = unsafe { GetDC(null_mut()) };
    if screen.is_null() {
        return None;
    }
    // SAFETY: `screen` came from `GetDC(null)` and is released once.
    let _screen = Release(move || unsafe {
        ReleaseDC(null_mut(), screen);
    });
    // SAFETY: `screen` is a valid DC.
    let dc = unsafe { CreateCompatibleDC(screen) };
    if dc.is_null() {
        return None;
    }
    // SAFETY: `dc` was created above and is deleted once.
    let _dc = Release(move || unsafe {
        DeleteDC(dc);
    });

    let weight = if semibold { FW_SEMIBOLD } else { FW_NORMAL };
    // SAFETY: `face` is null-terminated and outlives the call; the rest are
    // plain values. A negative height asks for that em size in pixels.
    let font = unsafe {
        CreateFontW(
            -(px as i32),
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            face.as_ptr(),
        )
    };
    if font.is_null() {
        return None;
    }
    // SAFETY: `font` was created above and is deleted once.
    let _font = Release(move || unsafe {
        DeleteObject(font);
    });
    // SAFETY: both handles are valid; the guard restores the old font
    // before `_font` deletes ours.
    let old_font = unsafe { SelectObject(dc, font) };
    let _old_font = Release(move || unsafe {
        SelectObject(dc, old_font);
    });

    let mut size = SIZE::default();
    // SAFETY: `utf16` holds `len` units and outlives the call; `size` is a
    // valid out-parameter.
    if unsafe { GetTextExtentPoint32W(dc, utf16.as_ptr(), len, &mut size) } == 0
        || size.cx <= 0
        || size.cy <= 0
    {
        return None;
    }
    let (width, height) = (size.cx as u32, size.cy as u32);

    // A bitmap compatible with the screen, not the memory DC, which would be
    // monochrome and lose the antialiasing.
    // SAFETY: `screen` is valid; the size is positive.
    let bitmap = unsafe { CreateCompatibleBitmap(screen, size.cx, size.cy) };
    if bitmap.is_null() {
        return None;
    }
    // SAFETY: `bitmap` was created above and is deleted once.
    let _bitmap = Release(move || unsafe {
        DeleteObject(bitmap);
    });
    {
        // SAFETY: both handles are valid; the guard deselects the bitmap at
        // the end of this block, before `GetDIBits`, which requires it.
        let old_bitmap = unsafe { SelectObject(dc, bitmap) };
        let _old_bitmap = Release(move || unsafe {
            SelectObject(dc, old_bitmap);
        });
        // SAFETY: plain state setters on a valid DC.
        unsafe {
            SetTextColor(dc, 0x00FF_FFFF);
        }
        // SAFETY: as above.
        unsafe {
            SetBkColor(dc, 0);
        }
        // SAFETY: as above.
        unsafe {
            SetBkMode(dc, OPAQUE as i32);
        }
        // SAFETY: `utf16` holds `len` units and outlives the call. In opaque
        // mode the text cell, which is the whole bitmap, is filled black.
        if unsafe { TextOutW(dc, 0, 0, utf16.as_ptr(), len) } == 0 {
            return None;
        }
    }

    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size.cx,
            biHeight: -size.cy, // negative: rows top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u32; width as usize * height as usize];
    // SAFETY: `pixels` holds exactly `height` rows of `width` 32-bit pixels,
    // the format `info` asks for; the bitmap is not selected into any DC.
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    if rows != size.cy {
        return None;
    }

    // White text on black in greyscale: every channel is the coverage.
    Some(TextMask {
        width,
        height,
        coverage: pixels.iter().map(|p| (p >> 8) as u8).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        to_wide(s)
    }

    #[test]
    fn to_wide_appends_one_null() {
        assert_eq!(to_wide("ab"), vec![b'a' as u16, b'b' as u16, 0]);
        assert_eq!(to_wide(""), vec![0]);
    }

    #[test]
    fn compare_logical_orders_numbers_by_value() {
        assert_eq!(compare_logical(&wide("2"), &wide("10")), Ordering::Less);
        assert_eq!(compare_logical(&wide("a"), &wide("A")), Ordering::Equal);
    }

    #[test]
    fn render_text_draws_antialiased_ink_inside_its_extent() {
        let mask = render_text("Help", 20, false).expect("GDI renders text");
        assert!(
            mask.width > 20 && mask.height >= 20,
            "{}×{}",
            mask.width,
            mask.height
        );
        assert_eq!(mask.coverage.len(), (mask.width * mask.height) as usize);
        assert!(mask.coverage.contains(&255), "solid ink");
        assert!(
            mask.coverage.iter().any(|&c| c > 0 && c < 255),
            "antialiased edges"
        );
        assert!(render_text("", 20, false).is_none());
    }

    #[test]
    #[should_panic(expected = "null-terminated")]
    fn compare_logical_rejects_unterminated_input() {
        compare_logical(&[b'a' as u16], &wide("a"));
    }
}
