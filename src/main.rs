//! Devoid Viewer — main.rs
//!
//! GUI: winit + softbuffer (CPU pixel blit, no GPU pipeline).
//!
//! Key features
//! ============
//! - Animated GIF playback
//! - Async image deletion
//! - Double-click to maximise / restore (unified with title-bar button)
//! - F / F11 to toggle borderless fullscreen; Escape to exit fullscreen
//! - Mouse-wheel / trackpad navigation between images
//! - Graceful extension-mismatch recovery with optional rename
//! - Archiver-safe path handling (no canonicalize / no \\?\ prefix)
//! - Exact taskbar-excluded work-area clamping on Windows
//! - Filename shown in the title bar

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod loader;
mod nav;

use std::{
    collections::HashMap,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam_channel::{bounded, Receiver, Sender};
use loader::{load_image, DecodedImage, LoadOutcome};
use nav::DirNavigator;
use softbuffer::Surface;
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{
        ElementState, Event, KeyEvent, MouseButton,
        MouseScrollDelta, TouchPhase, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoopBuilder},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowBuilder},
};
#[cfg(windows)]
use winit::platform::windows::EventLoopBuilderExtWindows;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Background colour (grey) for the blank state and transparent image regions.
const BG: u8 = 42;

/// Application name shown in the title bar when no image is open.
const APP_NAME: &str = "Devoid Viewer";

/// Maximum gap between two left-clicks that counts as a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

// ── Message types ─────────────────────────────────────────────────────────────

/// Messages posted from background threads to the UI thread.
#[derive(Debug)]
enum Msg {
    /// Primary decode finished successfully.
    Loaded {
        path:  PathBuf,
        image: DecodedImage,
        mode:  OpenMode,
    },
    /// Decoded after header sniffing — the file's extension was wrong.
    /// Carries the correct extension so the UI can offer to rename.
    WrongExtension {
        path:        PathBuf,
        image:       DecodedImage,
        mode:        OpenMode,
        correct_ext: String,
    },
    /// Speculative decode for a neighbour file (preload cache).
    Preloaded {
        path:  PathBuf,
        image: DecodedImage,
    },
    /// File is not a recognised image even after header sniffing.
    Unsupported {
        path:   PathBuf,
        reason: String,
    },
    /// Async deletion finished.
    Deleted {
        path:   PathBuf,
        result: Result<(), String>,
    },
}

/// Zero-size token injected by background threads to wake the sleeping
/// event loop after posting a `Msg`.
#[derive(Debug)]
struct WakeUp;

type Proxy = winit::event_loop::EventLoopProxy<WakeUp>;

// ── View snapshot ─────────────────────────────────────────────────────────────

/// Immutable borrow of the current display state, passed to `render()`.
struct View<'a> {
    image: &'a DecodedImage,
    frame: usize,
}

// ── DrainResult ───────────────────────────────────────────────────────────────

/// Everything the event loop needs to know after one drain of the message
/// channel. A named struct keeps call-sites readable.
#[derive(Default)]
struct DrainResult {
    /// Non-None when the first image loaded — resize the window to these dims.
    resize_to:    Option<(u32, u32)>,
    /// Non-None when any error occurred — reveal the window then show this.
    error_msg:    Option<String>,
    /// Non-None when a file decoded under the wrong extension — offer a rename
    /// after the drain loop so the dialog does not block the drain.
    rename_offer: Option<(PathBuf, String)>,
}

// ── App state ─────────────────────────────────────────────────────────────────

/// Controls whether loading an image triggers a window resize.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OpenMode {
    /// First image opened — resize the window to fit.
    FirstImage,
    /// Navigation or drag-and-drop — keep the current window size.
    Navigate,
}

struct App {
    // ── Display ───────────────────────────────────────────────────────────────
    current_path:  Option<PathBuf>,
    current_image: Option<DecodedImage>,

    // ── Animation ─────────────────────────────────────────────────────────────
    frame_index:   usize,
    next_frame_at: Option<Instant>,

    // ── Navigation ────────────────────────────────────────────────────────────
    navigator: DirNavigator,

    // ── Async I/O ─────────────────────────────────────────────────────────────
    msg_tx:        Sender<Msg>,
    msg_rx:        Receiver<Msg>,
    /// Path currently being decoded on a background thread.
    /// Used to discard stale messages when the user navigates quickly.
    loading_path:  Option<PathBuf>,
    preload_cache: HashMap<PathBuf, DecodedImage>,
    /// True while a deletion is in flight — prevents a second Delete keypress.
    deleting:      bool,

    // ── Window state ──────────────────────────────────────────────────────────
    /// True while waiting for a resize to land before revealing the window.
    /// The event loop sends itself a WakeUp after requesting the resize;
    /// on the following WakeUp it centres and reveals the window.
    pending_center: bool,
    /// Physical inner size saved before entering managed-maximise so we can
    /// restore it exactly. The position is not saved — we always centre on
    /// restore, which is simpler and avoids off-screen placement when the
    /// user has moved the window to a secondary monitor.
    pre_maximize:   Option<PhysicalSize<u32>>,
    /// True when the window is in our managed-maximise state.
    maximized:      bool,
    /// True when the window is in borderless fullscreen.
    fullscreen:     bool,
    /// Timestamp of the last left-click, for double-click detection.
    last_click:     Option<Instant>,
}

impl App {
    fn new(msg_tx: Sender<Msg>, msg_rx: Receiver<Msg>) -> Self {
        App {
            current_path:   None,
            current_image:  None,
            frame_index:    0,
            next_frame_at:  None,
            navigator:      DirNavigator::default(),
            msg_tx,
            msg_rx,
            loading_path:   None,
            preload_cache:  HashMap::new(),
            deleting:       false,
            pending_center: false,
            pre_maximize:   None,
            maximized:      false,
            fullscreen:     false,
            last_click:     None,
        }
    }

    /// Borrow the current frame for the renderer.
    fn view(&self) -> Option<View<'_>> {
        self.current_image.as_ref().map(|image| View {
            image,
            frame: self.frame_index,
        })
    }

    /// The filename component of the current path, for the title bar and dialogs.
    fn current_filename(&self) -> Option<String> {
        self.current_path.as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    }

    // ── Window mode ───────────────────────────────────────────────────────────

    /// Record a left-click and return `true` when it completes a double-click.
    fn register_click(&mut self) -> bool {
        let now       = Instant::now();
        let is_double = self.last_click
            .map(|t| now.duration_since(t) <= DOUBLE_CLICK)
            .unwrap_or(false);
        self.last_click = Some(now);
        is_double
    }

    /// Toggle between our managed-maximise and the saved window size.
    ///
    /// Saves the physical inner size before maximising and restores it
    /// exactly on the next toggle. Always centres on restore. Independent
    /// of fullscreen — double-click in fullscreen is a no-op.
    fn toggle_maximize(&mut self, window: &Window) {
        if self.fullscreen { return; }

        if self.maximized {
            window.set_maximized(false);
            if let Some(size) = self.pre_maximize.take() {
                // request_inner_size is asynchronous — pending_center ensures
                // center_window() fires after the resize has actually landed.
                let _ = window.request_inner_size(size);
                self.pending_center = true;
            }
            self.maximized = false;
        } else {
            self.pre_maximize = Some(window.inner_size());
            window.set_maximized(true);
            self.maximized = true;
        }
    }

    /// Toggle borderless fullscreen on the current monitor.
    /// Independent of the managed-maximise state.
    fn toggle_fullscreen(&mut self, window: &Window) {
        let target = if self.fullscreen { None }
                     else { Some(Fullscreen::Borderless(None)) };
        window.set_fullscreen(target);
        self.fullscreen = !self.fullscreen;
    }

    /// Exit fullscreen if active. Returns `true` when a change was made.
    fn exit_fullscreen(&mut self, window: &Window) -> bool {
        if self.fullscreen {
            window.set_fullscreen(None);
            self.fullscreen = false;
            true
        } else {
            false
        }
    }

    // ── Image loading ─────────────────────────────────────────────────────────

    fn open_image(&mut self, path: PathBuf, mode: OpenMode, proxy: &Proxy) {
        // Cache hit: install immediately without spawning a thread.
        if let Some(img) = self.preload_cache.remove(&path) {
            self.apply_image(path, img, mode, proxy);
            return;
        }

        // Seed the navigator immediately so peek_next / peek_prev are accurate
        // for preloading even before the decode thread finishes.
        self.loading_path = Some(path.clone());
        self.navigator.set_current(path.clone());

        let tx    = self.msg_tx.clone();
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let msg = match load_image(&path) {
                LoadOutcome::Ok(img) =>
                    Msg::Loaded { path, image: img, mode },
                LoadOutcome::WrongExtension(img, ext) =>
                    Msg::WrongExtension { path, image: img, mode, correct_ext: ext },
                LoadOutcome::Unsupported(reason) =>
                    Msg::Unsupported { path, reason },
            };
            let _ = tx.send(msg);
            let _ = proxy.send_event(WakeUp);
        });
    }

    /// Install `img` as the current display, kick off neighbour preloads,
    /// and reset animation. Returns `Some((w, h))` when a resize is needed.
    fn apply_image(
        &mut self,
        path: PathBuf,
        img:  DecodedImage,
        mode: OpenMode,
        proxy: &Proxy,
    ) -> Option<(u32, u32)> {
        let resize = (mode == OpenMode::FirstImage).then_some((img.width, img.height));
        self.current_image = Some(img);
        self.current_path  = Some(path.clone());
        self.loading_path  = None;
        self.navigator.set_current(path);
        self.preload_adjacent(proxy);
        self.reset_animation();
        resize
    }

    fn clear_image(&mut self) {
        self.current_image = None;
        self.current_path  = None;
        self.reset_animation();
    }

    /// Speculatively decode both neighbours so navigation feels instant.
    /// Only caches `Ok` results — wrong-extension files would prompt a rename
    /// dialog and must not be silently preloaded. Does nothing for
    /// single-image directories (`peek_*` return `None`).
    fn preload_adjacent(&mut self, proxy: &Proxy) {
        for path in [self.navigator.peek_next(), self.navigator.peek_prev()]
            .into_iter()
            .flatten()
        {
            if self.preload_cache.contains_key(&path) { continue; }
            let tx    = self.msg_tx.clone();
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                if let LoadOutcome::Ok(img) = load_image(&path) {
                    let _ = tx.send(Msg::Preloaded { path, image: img });
                    let _ = proxy.send_event(WakeUp);
                }
            });
        }
    }

    // ── Animation ─────────────────────────────────────────────────────────────

    fn reset_animation(&mut self) {
        self.frame_index   = 0;
        self.next_frame_at = self.current_image.as_ref()
            .filter(|i| i.is_animated())
            .map(|i| Instant::now() + Duration::from_millis(i.frame(0).delay_ms as u64));
    }

    /// Advance one GIF frame if the deadline has passed.
    /// Returns `true` when a redraw is needed.
    fn tick_animation(&mut self) -> bool {
        let Some(deadline) = self.next_frame_at else { return false; };
        if Instant::now() < deadline { return false; }

        if let Some(img) = &self.current_image {
            self.frame_index = (self.frame_index + 1) % img.frames.len();
            let delay = img.frame(self.frame_index).delay_ms;
            self.next_frame_at =
                Some(Instant::now() + Duration::from_millis(delay as u64));
            true
        } else {
            self.next_frame_at = None;
            false
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    fn navigate(&mut self, forward: bool, proxy: &Proxy) {
        let next = if forward { self.navigator.next() } else { self.navigator.prev() };
        if let Some(path) = next {
            self.open_image(path, OpenMode::Navigate, proxy);
        }
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    fn request_delete(&mut self, proxy: &Proxy) {
        let Some(path) = self.current_path.clone() else { return };
        self.deleting = true;
        let tx    = self.msg_tx.clone();
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let result = std::fs::remove_file(&path).map_err(|e| e.to_string());
            let _ = tx.send(Msg::Deleted { path, result });
            let _ = proxy.send_event(WakeUp);
        });
    }

    fn on_deleted(
        &mut self,
        path:   PathBuf,
        result: Result<(), String>,
        proxy:  &Proxy,
    ) -> Option<String> {
        self.deleting = false;
        match result {
            Ok(()) => {
                self.navigator.remove(&path);
                self.preload_cache.clear();
                match self.navigator.current().cloned() {
                    Some(next) => self.open_image(next, OpenMode::Navigate, proxy),
                    None       => self.clear_image(),
                }
                None
            }
            Err(e) => Some(format!("Delete failed:\n{e}")),
        }
    }

    // ── Message drain ─────────────────────────────────────────────────────────

    /// Non-blockingly drain all pending messages from the channel.
    ///
    /// All blocking side-effects (dialogs, rename) are returned in `DrainResult`
    /// rather than performed inside the loop, so the drain itself never blocks.
    fn drain_messages(&mut self, proxy: &Proxy) -> DrainResult {
        let mut dr = DrainResult::default();

        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                Msg::Preloaded { path, image } => {
                    self.preload_cache.insert(path, image);
                }
                Msg::Loaded { path, image, mode } => {
                    if self.is_active_load(&path) {
                        dr.resize_to = self.apply_image(path, image, mode, proxy);
                    }
                }
                Msg::WrongExtension { path, image, mode, correct_ext } => {
                    if self.is_active_load(&path) {
                        dr.resize_to = self.apply_image(path.clone(), image, mode, proxy);
                        dr.rename_offer = Some((path, correct_ext));
                    }
                }
                Msg::Unsupported { path, reason } => {
                    if self.is_active_load(&path) {
                        self.loading_path = None;
                        dr.error_msg = Some(format!("Cannot open image:\n{reason}"));
                    }
                }
                Msg::Deleted { path, result } => {
                    if let Some(e) = self.on_deleted(path, result, proxy) {
                        dr.error_msg = Some(e);
                    }
                }
            }
        }

        dr
    }

    /// Whether `path` is the file currently being decoded.
    #[inline]
    fn is_active_load(&self, path: &Path) -> bool {
        self.loading_path.as_deref() == Some(path)
    }

    /// Update `current_path` and the navigator after a successful rename.
    fn apply_rename(&mut self, old_path: &Path, new_path: PathBuf) {
        if self.current_path.as_deref() == Some(old_path) {
            self.current_path = Some(new_path.clone());
        }
        self.navigator.rename(old_path, new_path);
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Largest uniform scale that fits the image inside the window without upscaling.
#[inline]
fn fit_scale(img_w: u32, img_h: u32, win_w: u32, win_h: u32) -> f32 {
    (win_h as f32 / img_h as f32).min(win_w as f32 / img_w as f32)
}

/// Point-sample an RGBA8 buffer at float source coordinates.
#[inline]
fn nearest(pixels: &[u8], w: u32, h: u32, sx: f32, sy: f32) -> [u8; 4] {
    let x = (sx as u32).min(w - 1);
    let y = (sy as u32).min(h - 1);
    let i = (y * w + x) as usize * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// Alpha-composite a straight-RGBA pixel over the background colour.
#[inline]
fn composite(r: u8, g: u8, b: u8, a: u8) -> u32 {
    #[inline]
    fn blend(src: u8, a: u32) -> u8 {
        ((src as u32 * a + BG as u32 * (255 - a)) / 255) as u8
    }
    let a = a as u32;
    pack_rgb(blend(r, a), blend(g, a), blend(b, a))
}

/// Pack RGB bytes into softbuffer's `0x00RRGGBB` u32.
#[inline]
fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Write the current frame into `buf`, centred and scaled to fit.
fn render(buf: &mut [u32], win_w: u32, win_h: u32, view: Option<View<'_>>) {
    let bg = pack_rgb(BG, BG, BG);

    let Some(v) = view else {
        buf.fill(bg);
        return;
    };

    let scale = fit_scale(v.image.width, v.image.height, win_w, win_h);
    let dst_w = (v.image.width  as f32 * scale) as i32;
    let dst_h = (v.image.height as f32 * scale) as i32;
    let off_x = ((win_w as f32 - dst_w as f32) / 2.0) as i32;
    let off_y = ((win_h as f32 - dst_h as f32) / 2.0) as i32;

    let pixels   = v.image.frame(v.frame).pixels.as_slice();
    let (iw, ih) = (v.image.width, v.image.height);

    // dx/dy are plain loop counters — no per-pixel division or modulo.
    for dy in 0..win_h as i32 {
        for dx in 0..win_w as i32 {
            buf[dy as usize * win_w as usize + dx as usize] =
                if dx >= off_x && dx < off_x + dst_w
                && dy >= off_y && dy < off_y + dst_h
                {
                    let [r, g, b, a] = nearest(pixels, iw, ih,
                        (dx - off_x) as f32 / scale,
                        (dy - off_y) as f32 / scale);
                    composite(r, g, b, a)
                } else {
                    bg
                };
        }
    }
}

// ── Native dialogs (Windows) ──────────────────────────────────────────────────

/// Encode a `&str` as a null-terminated UTF-16 `Vec` for Win32 API calls.
#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn confirm_delete_dialog(filename: &str) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONWARNING, MB_YESNO, IDYES,
        };
        let text = to_wide(&format!("Permanently delete\n\n{filename}?"));
        let cap  = to_wide("Delete Image");
        unsafe {
            MessageBoxW(0, text.as_ptr(), cap.as_ptr(), MB_YESNO | MB_ICONWARNING) == IDYES
        }
    }
    #[cfg(not(windows))]
    { let _ = filename; true }
}

fn show_error_dialog(msg: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONERROR, MB_OK,
        };
        let text = to_wide(msg);
        let cap  = to_wide(APP_NAME);
        unsafe { MessageBoxW(0, text.as_ptr(), cap.as_ptr(), MB_ICONERROR | MB_OK); }
    }
    #[cfg(not(windows))]
    { eprintln!("Error: {msg}"); }
}

/// Offer to rename `path` to use `correct_ext`.
///
/// Skips the prompt silently when the parent directory is confirmed read-only.
/// When the target path already exists (e.g. renaming `5.png` → `5.jpg` but
/// `5.jpg` already exists), a numbered suffix is appended: `5 (1).jpg`,
/// `5 (2).jpg`, etc.
/// Returns `Some(new_path)` when the rename succeeded, `None` otherwise.
fn offer_rename(path: &Path, correct_ext: &str) -> Option<PathBuf> {
    // Skip silently when the parent is read-only. If we cannot determine
    // write access we offer the rename optimistically — the filesystem will
    // reject it and we show an error.
    let parent_readonly = path.parent()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.permissions().readonly())
        .unwrap_or(false);
    if parent_readonly { return None; }

    // Resolve the target path, appending a counter if it already exists.
    let new_path = unique_rename_target(path, correct_ext);

    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONQUESTION, MB_YESNO, IDYES,
        };
        let current_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let prompt = format!(
            "This file is a .{correct_ext} image saved with a .{current_ext} extension.\n\n\
             Rename to:\n{}\n\nRename?",
            new_path.display()
        );
        let text   = to_wide(&prompt);
        let cap    = to_wide("Wrong Extension");
        let answer = unsafe {
            MessageBoxW(0, text.as_ptr(), cap.as_ptr(), MB_YESNO | MB_ICONQUESTION)
        };
        if answer == IDYES {
            match std::fs::rename(path, &new_path) {
                Ok(())  => return Some(new_path),
                Err(e)  => show_error_dialog(&format!("Could not rename file:\n{e}")),
            }
        }
    }
    #[cfg(not(windows))]
    { let _ = new_path; }

    None
}

/// Return a path that does not yet exist by appending ` (n)` before the
/// extension when necessary.
///
/// Examples:
/// - `5.png` + `"jpg"` → `5.jpg`          (if `5.jpg` does not exist)
/// - `5.png` + `"jpg"` → `5 (1).jpg`      (if `5.jpg` already exists)
/// - `5.png` + `"jpg"` → `5 (2).jpg`      (if `5.jpg` and `5 (1).jpg` exist)
fn unique_rename_target(path: &Path, ext: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new(""));

    let candidate = parent.join(format!("{stem}.{ext}"));
    if !candidate.exists() {
        return candidate;
    }

    for n in 1u32.. {
        let candidate = parent.join(format!("{stem} ({n}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    // Unreachable in practice — u32::MAX files cannot exist in one directory.
    unreachable!("could not find a unique rename target")
}

// ── Window helpers ────────────────────────────────────────────────────────────

/// Usable desktop dimensions in logical pixels.
/// On Windows uses `SPI_GETWORKAREA` for exact taskbar exclusion.
/// Elsewhere uses 90 % × 85 % of the monitor size as a conservative heuristic.
fn work_area(window: &Window) -> (f64, f64) {
    #[cfg(windows)]
    if let Some(wa) = windows_work_area(window) {
        return wa;
    }

    window.current_monitor()
        .or_else(|| window.available_monitors().next())
        .map(|m| {
            let sf = m.scale_factor();
            let s  = m.size();
            (s.width as f64 / sf * 0.90, s.height as f64 / sf * 0.85)
        })
        .unwrap_or((1152.0, 648.0))
}

#[cfg(windows)]
fn windows_work_area(window: &Window) -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0);
    }
    let sf = window.current_monitor()
        .or_else(|| window.available_monitors().next())
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    let w = (rect.right  - rect.left) as f64 / sf;
    let h = (rect.bottom - rect.top)  as f64 / sf;
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// Scale `(w, h)` down to fit inside the work area; never scale up.
fn clamp_to_work_area(w: u32, h: u32, window: &Window) -> (u32, u32) {
    let (max_w, max_h) = work_area(window);
    let scale = (max_w / w as f64).min(max_h / h as f64).min(1.0);
    ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
}

/// Centre the window on its current monitor with a minimum top margin so the
/// title bar always remains reachable.
fn center_window(window: &Window) {
    let Some(mon) = window.current_monitor()
        .or_else(|| window.available_monitors().next()) else { return };
    let mp = mon.position();
    let ms = mon.size();
    let ws = window.outer_size();
    let sf = mon.scale_factor();
    let x  = mp.x + (ms.width  as i32 - ws.width  as i32) / 2;
    let y  = (mp.y + (ms.height as i32 - ws.height as i32) / 2)
             .max(mp.y + (30.0 * sf) as i32);
    window.set_outer_position(PhysicalPosition::new(x, y));
}

/// Set the window title to `"filename — Devoid Viewer"`, or just
/// `"Devoid Viewer"` when `filename` is `None`.
fn update_title(window: &Window, filename: Option<&str>) {
    match filename {
        Some(name) => window.set_title(&format!("{name} \u{2014} {APP_NAME}")),
        None       => window.set_title(APP_NAME),
    }
}

/// Interpret a scroll delta as a navigation direction.
/// Returns `true` for forward (next image) and `false` for backward (previous).
/// Convention: scroll down or right → next, scroll up or left → previous.
#[inline]
fn scroll_is_forward(delta: &MouseScrollDelta) -> bool {
    match delta {
        MouseScrollDelta::LineDelta(x, y) =>
            if *y != 0.0 { *y < 0.0 } else { *x > 0.0 },
        MouseScrollDelta::PixelDelta(pos) =>
            if pos.y != 0.0 { pos.y < 0.0 } else { pos.x > 0.0 },
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Decode the application icon embedded at compile time and return a winit Icon.
fn load_window_icon() -> Option<winit::window::Icon> {
    let bytes  = include_bytes!("../devoid.ico");
    let img    = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico).ok()?;
    let rgba   = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), w, h).ok()
}

/// Read only enough of the file to determine its pixel dimensions.
/// For most formats this is just the header; SVG requires a full parse.
fn image_size_hint(path: &Path) -> Option<(u32, u32)> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)?;

    if ext == "svg" {
        let data = std::fs::read(path).ok()?;
        let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
        let sz   = tree.size();
        return Some((sz.width() as u32, sz.height() as u32));
    }
    let reader = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    image::ImageReader::new(reader)
        .with_guessed_format().ok()?
        .into_dimensions().ok()
}

/// Fill the softbuffer surface with the background colour.
/// Called once before the event loop to overwrite the DWM white fill that
/// appears on every new Win32 window before the application first paints.
fn paint_background(surface: &mut Surface<Arc<Window>, Arc<Window>>, window: &Window) {
    let size  = window.inner_size();
    let win_w = NonZeroU32::new(size.width.max(1)).unwrap();
    let win_h = NonZeroU32::new(size.height.max(1)).unwrap();
    if surface.resize(win_w, win_h).is_ok() {
        if let Ok(mut buf) = surface.buffer_mut() {
            buf.fill(pack_rgb(BG, BG, BG));
            let _ = buf.present();
        }
    }
}

/// Make the window visible if it isn't already. Returns `true`.
#[inline]
fn ensure_visible(window: &Window, shown: bool) -> bool {
    if !shown { window.set_visible(true); }
    true
}

/// Terminate the process immediately.
///
/// Background threads hold channel senders that would delay a graceful exit.
/// `process::exit` bypasses all cleanup — correct for a stateless viewer.
#[inline]
fn quit() -> ! {
    std::process::exit(0)
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    // Use the raw argument path — never canonicalize.
    // `std::fs::canonicalize` prepends `\\?\` on Windows, which breaks the
    // image crate and causes silent failures for archiver temp paths.
    let initial_path = std::env::args().nth(1).map(PathBuf::from);

    // Header-only dimension probe lets the window appear at the correct size
    // immediately, while the full decode runs on a background thread.
    let (init_w, init_h) = initial_path.as_deref()
        .and_then(image_size_hint)
        .unwrap_or((800, 600));

    let (msg_tx, msg_rx) = bounded::<Msg>(32);

    let mut el_builder: EventLoopBuilder<WakeUp> = EventLoopBuilder::with_user_event();

    // Install a Win32 message hook to intercept two system commands before
    // winit sees them:
    //
    // SC_CLOSE (taskbar thumbnail × button)
    //   winit 0.29 does not translate this into `CloseRequested`.
    //   Re-post as WM_CLOSE, which winit does handle.
    //
    // SC_MAXIMIZE (title-bar double-click and the maximise button)
    //   Suppress OS handling so `toggle_maximize()` is the single code path
    //   for maximise — giving unified behaviour with the client-area
    //   double-click. The title-bar double-click also fires `MouseInput`
    //   in winit, so suppressing SC_MAXIMIZE here is sufficient.
    #[cfg(windows)]
    el_builder.with_msg_hook(|msg| {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            PostMessageW, WM_CLOSE, WM_SYSCOMMAND, SC_CLOSE, SC_MAXIMIZE,
        };
        let msg = msg as *const windows_sys::Win32::UI::WindowsAndMessaging::MSG;
        unsafe {
            if (*msg).message == WM_SYSCOMMAND {
                match ((*msg).wParam & 0xFFF0) as u32 {
                    SC_CLOSE    => { PostMessageW((*msg).hwnd, WM_CLOSE, 0, 0); }
                    SC_MAXIMIZE => { return true; }  // suppress; MouseInput fires instead
                    _           => {}
                }
            }
        }
        false
    });

    let event_loop = el_builder.build().expect("event loop");
    let proxy      = event_loop.create_proxy();

    // Hide the window when an image argument is provided so the user sees it
    // appear at the correct size and position rather than watching it resize.
    // Without an argument show immediately as a drag-and-drop target.
    let start_hidden = initial_path.is_some();

    let window = Arc::new(
        WindowBuilder::new()
            .with_title(APP_NAME)
            .with_window_icon(load_window_icon())
            .with_inner_size(LogicalSize::new(init_w, init_h))
            .with_min_inner_size(LogicalSize::new(120u32, 80u32))
            .with_visible(!start_hidden)
            .build(&event_loop)
            .expect("window"),
    );

    if start_hidden {
        let (cw, ch) = clamp_to_work_area(init_w, init_h, &window);
        let _ = window.request_inner_size(LogicalSize::new(cw, ch));
        center_window(&window);
    }

    let ctx         = softbuffer::Context::new(window.clone()).expect("softbuffer context");
    let mut surface = Surface::new(&ctx, window.clone()).expect("softbuffer surface");
    paint_background(&mut surface, &window);

    let mut app   = App::new(msg_tx, msg_rx);
    let mut shown = !start_hidden;

    if let Some(path) = initial_path {
        app.open_image(path, OpenMode::FirstImage, &proxy);
    }

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(match app.next_frame_at {
            Some(t) => ControlFlow::WaitUntil(t),
            None    => ControlFlow::Wait,
        });

        match event {
            // ── Background thread results ─────────────────────────────────────
            Event::UserEvent(WakeUp) => {
                let dr = app.drain_messages(&proxy);

                if let Some((iw, ih)) = dr.resize_to {
                    // Issue the resize, then send a self-WakeUp. The window
                    // size change is asynchronous; `pending_center` ensures we
                    // wait for it to land before centering and revealing.
                    let (cw, ch) = clamp_to_work_area(iw, ih, &window);
                    let _ = window.request_inner_size(LogicalSize::new(cw, ch));
                    app.pending_center = true;
                    let _ = proxy.send_event(WakeUp);
                } else if app.pending_center {
                    center_window(&window);
                    app.pending_center = false;
                    shown = ensure_visible(&window, shown);
                }

                // Update the title bar whenever the displayed image may have changed.
                update_title(&window, app.current_filename().as_deref());

                // Reveal before showing a blocking error dialog so we never
                // leave an invisible process alive in the background.
                if let Some(msg) = dr.error_msg {
                    shown = ensure_visible(&window, shown);
                    show_error_dialog(&msg);
                }

                // Rename offer shown after the drain so the dialog is never
                // blocking inside the tight drain loop.
                if let Some((old_path, correct_ext)) = dr.rename_offer {
                    if let Some(new_path) = offer_rename(&old_path, &correct_ext) {
                        app.apply_rename(&old_path, new_path);
                        update_title(&window, app.current_filename().as_deref());
                    }
                }

                window.request_redraw();
            }

            // ── Animation tick ────────────────────────────────────────────────
            Event::NewEvents(_) => {
                if app.tick_animation() {
                    window.request_redraw();
                }
            }

            // ── Window events ─────────────────────────────────────────────────
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => quit(),

                WindowEvent::Resized(_) => {
                    // Sync our flag when the OS maximises the window externally
                    // (Win+Up, or the maximise button while we suppressed SC_MAXIMIZE).
                    if window.is_maximized() && !app.maximized {
                        app.maximized = true;
                    }
                    window.request_redraw();
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left, ..
                } => {
                    if app.register_click() {
                        app.toggle_maximize(&window);
                        // If toggle_maximize requested a resize (restore path),
                        // send a self-WakeUp so centering fires after it lands.
                        if app.pending_center {
                            let _ = proxy.send_event(WakeUp);
                        }
                        window.request_redraw();
                    }
                }

                // Scroll wheel / trackpad: down or right → next, up or left → previous.
                // LineDelta (physical wheels) and PixelDelta (trackpads) are both handled.
                // TouchPhase::Cancelled is ignored; momentum/inertia phases are not filtered
                // because winit reports standard mouse-wheel events as TouchPhase::Moved
                // regardless of device type.
                WindowEvent::MouseWheel { delta, phase, .. }
                    if phase != TouchPhase::Cancelled =>
                {
                    app.navigate(scroll_is_forward(&delta), &proxy);
                    window.request_redraw();
                }

                WindowEvent::DroppedFile(path) => {
                    if loader::is_supported(&path) {
                        app.open_image(path, OpenMode::Navigate, &proxy);
                        window.request_redraw();
                    }
                }

                WindowEvent::KeyboardInput {
                    event: KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: ElementState::Pressed, ..
                    }, ..
                } => match code {
                    // Navigation
                    KeyCode::ArrowRight | KeyCode::ArrowDown  | KeyCode::PageDown => {
                        app.navigate(true, &proxy);
                        window.request_redraw();
                    }
                    KeyCode::ArrowLeft  | KeyCode::ArrowUp    | KeyCode::PageUp   => {
                        app.navigate(false, &proxy);
                        window.request_redraw();
                    }
                    // Deletion
                    KeyCode::Delete if app.current_path.is_some() && !app.deleting => {
                        if let Some(fname) = app.current_filename() {
                            if confirm_delete_dialog(&fname) {
                                app.request_delete(&proxy);
                            }
                        }
                    }
                    // Fullscreen
                    KeyCode::KeyF | KeyCode::F11 => {
                        app.toggle_fullscreen(&window);
                        window.request_redraw();
                    }
                    // Exit fullscreen; no-op if not fullscreen
                    KeyCode::Escape => {
                        if app.exit_fullscreen(&window) {
                            window.request_redraw();
                        }
                    }
                    // Quit
                    KeyCode::KeyQ => quit(),
                    _ => {}
                },

                WindowEvent::RedrawRequested => {
                    let size  = window.inner_size();
                    let win_w = size.width;
                    let win_h = size.height;
                    if win_w == 0 || win_h == 0 { return; }

                    surface
                        .resize(
                            NonZeroU32::new(win_w).unwrap(),
                            NonZeroU32::new(win_h).unwrap(),
                        )
                        .expect("surface resize");
                    let mut buf = surface.buffer_mut().expect("surface buffer");
                    render(&mut buf, win_w, win_h, app.view());
                    buf.present().expect("present");
                }

                _ => {}
            },

            _ => {}
        }
    }).expect("event loop error");
}
