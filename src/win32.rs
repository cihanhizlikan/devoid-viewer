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

use windows_sys::Win32::Foundation::RECT;
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
    unsafe { MessageBoxW(0, text.as_ptr(), caption.as_ptr(), MB_YESNO | icon) == IDYES }
}

/// Modal error message box with an OK button and no owner window.
pub fn show_error(text: &str, caption: &str) {
    let (text, caption) = (to_wide(text), to_wide(caption));
    // SAFETY: as in `ask_yes_no`.
    unsafe {
        MessageBoxW(0, text.as_ptr(), caption.as_ptr(), MB_ICONERROR | MB_OK);
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
    #[should_panic(expected = "null-terminated")]
    fn compare_logical_rejects_unterminated_input() {
        compare_logical(&[b'a' as u16], &wide("a"));
    }
}
