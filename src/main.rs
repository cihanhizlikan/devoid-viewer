//! Devoid Viewer — main.rs
//!
//! GUI: winit + softbuffer (CPU pixel blit, no GPU pipeline).
//! Features: animated GIF, pan, async deletion,
//!           exact work-area clamping on Windows.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod loader;
mod nav;

use std::{
    collections::HashMap,
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam_channel::{bounded, Receiver, Sender};
use loader::{load_image, DecodedImage};
use nav::DirNavigator;
use softbuffer::Surface;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder},
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Background grey level — blank state and transparent image regions.
const BG: u8 = 42;

// ── Message & event types ─────────────────────────────────────────────────────

/// Messages from background threads to the UI thread.
#[derive(Debug)]
enum Msg {
    Loaded   { path: PathBuf, image: DecodedImage, resize: bool },
    Preloaded{ path: PathBuf, image: DecodedImage },
    Error    { path: PathBuf, msg:   String },
    Deleted  { path: PathBuf, result: Result<(), String> },
}

/// Zero-size token injected by background threads to wake the event loop.
#[derive(Debug)]
struct WakeUp;

type Proxy = winit::event_loop::EventLoopProxy<WakeUp>;

// ── View state (everything the renderer needs) ────────────────────────────────

/// Snapshot passed to `render()` each frame.
struct View<'a> {
    image: &'a DecodedImage,
    frame: usize,
    pan:   (f32, f32),
}

// ── App state ─────────────────────────────────────────────────────────────────

/// Whether the window should resize to match a newly opened image.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenMode { FirstImage, Navigate }

struct App {
    // Display
    current_path:  Option<PathBuf>,
    current_image: Option<DecodedImage>,

    // Animation
    frame_index:   usize,
    next_frame_at: Option<Instant>,

    // Pan
    pan:        (f32, f32),           // screen-pixel offset from centre
    drag_start: Option<(f32, f32)>,   // cursor pos when left-button went down
    pan_origin: (f32, f32),           // pan value at drag start

    // Navigation
    navigator: DirNavigator,

    // Async I/O
    msg_tx:        Sender<Msg>,
    msg_rx:        Receiver<Msg>,
    loading_path:  Option<PathBuf>,
    preload_cache: HashMap<PathBuf, DecodedImage>,
    deleting:      bool,

    // Window
    pending_center: bool,
}

impl App {
    fn new(msg_tx: Sender<Msg>, msg_rx: Receiver<Msg>) -> Self {
        App {
            current_path:   None,
            current_image:  None,
            frame_index:    0,
            next_frame_at:  None,
            pan:            (0.0, 0.0),
            drag_start:     None,
            pan_origin:     (0.0, 0.0),
            navigator:      DirNavigator::default(),
            msg_tx,
            msg_rx,
            loading_path:   None,
            preload_cache:  HashMap::new(),
            deleting:       false,
            pending_center: false,
        }
    }

    /// Snapshot of current view state for the renderer.
    fn view(&self) -> Option<View<'_>> {
        self.current_image.as_ref().map(|image| View {
            image,
            frame: self.frame_index,
            pan:   self.pan,
        })
    }

    // ── Loading ───────────────────────────────────────────────────────────────

    fn open_image(&mut self, path: PathBuf, mode: OpenMode, proxy: &Proxy) {
        if let Some(img) = self.preload_cache.remove(&path) {
            self.apply_image(path, img, mode, proxy);
            return;
        }
        self.loading_path = Some(path.clone());
        self.navigator.set_current(path.clone());

        let tx     = self.msg_tx.clone();
        let proxy  = proxy.clone();
        let resize = mode == OpenMode::FirstImage;
        std::thread::spawn(move || {
            let msg = match load_image(&path) {
                Ok(img) => Msg::Loaded { path, image: img, resize },
                Err(e)  => Msg::Error  { path, msg: e.to_string() },
            };
            let _ = tx.send(msg);
            let _ = proxy.send_event(WakeUp);
        });
    }

    /// Store the image, reset pan, start animation.
    /// Returns Some((w,h)) when the caller should resize the window.
    fn apply_image(
        &mut self,
        path:  PathBuf,
        img:   DecodedImage,
        mode:  OpenMode,
        proxy: &Proxy,
    ) -> Option<(u32, u32)> {
        let resize_dims = (mode == OpenMode::FirstImage)
            .then_some((img.width, img.height));

        self.current_image = Some(img);
        self.current_path  = Some(path.clone());
        self.loading_path  = None;
        self.reset_pan();
        self.navigator.set_current(path);
        self.preload_adjacent(proxy);
        self.reset_animation();

        resize_dims
    }

    fn clear_image(&mut self) {
        self.current_image = None;
        self.current_path  = None;
        self.reset_pan();
        self.reset_animation();
    }

    fn preload_adjacent(&mut self, proxy: &Proxy) {
        for path in [self.navigator.peek_next(), self.navigator.peek_prev()]
            .into_iter().flatten()
        {
            if self.preload_cache.contains_key(&path) { continue; }
            let tx    = self.msg_tx.clone();
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                if let Ok(img) = load_image(&path) {
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

    /// Advance to the next frame if its deadline has passed.
    /// Returns true when a redraw is needed.
    fn tick_animation(&mut self) -> bool {
        let Some(deadline) = self.next_frame_at else { return false; };
        if Instant::now() < deadline { return false; }

        if let Some(img) = &self.current_image {
            self.frame_index = (self.frame_index + 1) % img.frames.len();
            let delay = img.frame(self.frame_index).delay_ms;
            self.next_frame_at = Some(Instant::now() + Duration::from_millis(delay as u64));
            true
        } else {
            self.next_frame_at = None;
            false
        }
    }

    // ── Pan ───────────────────────────────────────────────────────────────────

    fn reset_pan(&mut self) {
        self.pan        = (0.0, 0.0);
        self.drag_start = None;
    }

    fn start_drag(&mut self, cursor: (f32, f32)) {
        self.drag_start = Some(cursor);
        self.pan_origin = self.pan;
    }

    /// Returns true if a drag is active (caller should request redraw).
    fn update_drag(&mut self, cursor: (f32, f32)) -> bool {
        if let Some(start) = self.drag_start {
            self.pan = (
                self.pan_origin.0 + cursor.0 - start.0,
                self.pan_origin.1 + cursor.1 - start.1,
            );
            true
        } else {
            false
        }
    }

    fn end_drag(&mut self) { self.drag_start = None; }

    // ── Navigation ────────────────────────────────────────────────────────────

    fn navigate(&mut self, forward: bool, proxy: &Proxy) {
        let p = if forward { self.navigator.next() } else { self.navigator.prev() };
        if let Some(p) = p { self.open_image(p, OpenMode::Navigate, proxy); }
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

    fn on_deleted(&mut self, path: PathBuf, result: Result<(), String>, proxy: &Proxy) {
        self.deleting = false;
        match result {
            Ok(()) => {
                self.navigator.remove(&path);
                self.preload_cache.clear();
                match self.navigator.current().cloned() {
                    Some(next) => self.open_image(next, OpenMode::Navigate, proxy),
                    None       => self.clear_image(),
                }
            }
            Err(e) => show_error_dialog(&format!("Delete failed:\n{e}")),
        }
    }

    // ── Message drain ─────────────────────────────────────────────────────────

    /// Drains the message channel.
    /// Returns Some((w,h)) when the window should be resized for the first image.
    fn drain_messages(&mut self, proxy: &Proxy) -> Option<(u32, u32)> {
        let mut resize_to = None;
        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                Msg::Preloaded { path, image } => {
                    self.preload_cache.insert(path, image);
                }
                Msg::Loaded { path, image, resize } => {
                    if self.loading_path.as_deref() == Some(path.as_path()) {
                        let mode = if resize { OpenMode::FirstImage } else { OpenMode::Navigate };
                        if let Some(dims) = self.apply_image(path, image, mode, proxy) {
                            resize_to = Some(dims);
                        }
                    }
                }
                Msg::Error { path, msg } => {
                    if self.loading_path.as_deref() == Some(path.as_path()) {
                        self.loading_path = None;
                        show_error_dialog(&format!("Failed to load image:\n{msg}"));
                    }
                }
                Msg::Deleted { path, result } => {
                    self.on_deleted(path, result, proxy);
                }
            }
        }
        resize_to
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Fit-to-window scale: the largest uniform scale that fits the image in the panel.
#[inline]
fn fit_scale(img_w: u32, img_h: u32, win_w: f32, win_h: f32) -> f32 {
    (win_h / img_h as f32).min(win_w / img_w as f32)
}

/// Nearest-neighbour sample from an RGBA8 pixel buffer.
#[inline]
fn nearest(pixels: &[u8], w: u32, h: u32, sx: f32, sy: f32) -> [u8; 4] {
    let x = (sx as u32).min(w - 1);
    let y = (sy as u32).min(h - 1);
    let i = (y * w + x) as usize * 4;
    [pixels[i], pixels[i+1], pixels[i+2], pixels[i+3]]
}

/// Alpha-composite a straight-RGBA pixel over the background colour.
#[inline]
fn composite(r: u8, g: u8, b: u8, a: u8) -> u32 {
    #[inline] fn blend(src: u8, a: u32) -> u8 {
        ((src as u32 * a + BG as u32 * (255 - a)) / 255) as u8
    }
    let a = a as u32;
    pack_rgb(blend(r, a), blend(g, a), blend(b, a))
}

/// Pack RGB into softbuffer's 0x00RRGGBB u32.
#[inline]
fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Write pixels into `buf` (softbuffer 0x00RRGGBB format).
fn render(buf: &mut [u32], win_w: u32, win_h: u32, view: Option<View<'_>>) {
    let bg = pack_rgb(BG, BG, BG);

    let Some(v) = view else { buf.fill(bg); return; };

    let scale = fit_scale(v.image.width, v.image.height, win_w as f32, win_h as f32);
    let dst_w = (v.image.width  as f32 * scale) as i32;
    let dst_h = (v.image.height as f32 * scale) as i32;
    let off_x = ((win_w as f32 - dst_w as f32) / 2.0 + v.pan.0) as i32;
    let off_y = ((win_h as f32 - dst_h as f32) / 2.0 + v.pan.1) as i32;

    let pixels   = &v.image.frame(v.frame).pixels;
    let (iw, ih) = (v.image.width, v.image.height);

    for dy in 0..win_h as i32 {
        for dx in 0..win_w as i32 {
            let px = if dx >= off_x && dx < off_x + dst_w
                     && dy >= off_y && dy < off_y + dst_h
            {
                let sx = (dx - off_x) as f32 / scale;
                let sy = (dy - off_y) as f32 / scale;
                let [r, g, b, a] = nearest(pixels, iw, ih, sx, sy);
                composite(r, g, b, a)
            } else {
                bg
            };
            buf[(dy as u32 * win_w + dx as u32) as usize] = px;
        }
    }
}

// ── Native dialogs ────────────────────────────────────────────────────────────

fn confirm_delete_dialog(filename: &str) -> bool {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONWARNING, MB_YESNO, IDYES,
        };
        let text: Vec<u16> = OsStr::new(&format!("Permanently delete\n\n{filename}?"))
            .encode_wide().chain(Some(0)).collect();
        let cap: Vec<u16> = OsStr::new("Delete Image")
            .encode_wide().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(0, text.as_ptr(), cap.as_ptr(), MB_YESNO | MB_ICONWARNING) == IDYES
        }
    }
    #[cfg(not(windows))]
    { true }
}

fn show_error_dialog(msg: &str) {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let text: Vec<u16> = OsStr::new(msg).encode_wide().chain(Some(0)).collect();
        let cap:  Vec<u16> = OsStr::new("Devoid Viewer").encode_wide().chain(Some(0)).collect();
        unsafe { MessageBoxW(0, text.as_ptr(), cap.as_ptr(), MB_ICONERROR | MB_OK); }
    }
    #[cfg(not(windows))]
    { eprintln!("Error: {msg}"); }
}

// ── Window helpers ────────────────────────────────────────────────────────────

/// Returns the usable desktop area in logical pixels.
/// On Windows uses SPI_GETWORKAREA (precise taskbar exclusion).
/// Elsewhere uses 90% × 85% of the monitor size as a heuristic.
fn work_area(window: &Window) -> (f64, f64) {
    #[cfg(windows)]
    if let Some(wa) = windows_work_area(window) { return wa; }

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

fn clamp_to_work_area(w: u32, h: u32, window: &Window) -> (u32, u32) {
    let (max_w, max_h) = work_area(window);
    let scale = (max_w / w as f64).min(max_h / h as f64).min(1.0);
    ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
}

fn center_window(window: &Window) {
    if let Some(mon) = window.current_monitor().or_else(|| window.available_monitors().next()) {
        let mp = mon.position();
        let ms = mon.size();
        let ws = window.outer_size();
        let sf = mon.scale_factor();
        let x  = mp.x + (ms.width  as i32 - ws.width  as i32) / 2;
        let y  = (mp.y + (ms.height as i32 - ws.height as i32) / 2)
                 .max(mp.y + (30.0 * sf) as i32);
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let initial_path = std::env::args().nth(1)
        .map(PathBuf::from)
        .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)));

    // Probe the image header for dimensions — fast, no full decode.
    // This lets us size the window correctly and show intentional gray
    // immediately, while the full decode happens on a background thread.
    // The user sees a correctly-sized gray window at once rather than a
    // white rectangle for the duration of a potentially slow decode.
    let (init_w, init_h) = initial_path.as_deref()
        .and_then(image_size_hint)
        .unwrap_or((800, 600));

    let (msg_tx, msg_rx) = bounded::<Msg>(32);

    let event_loop: EventLoop<WakeUp> = EventLoopBuilder::with_user_event()
        .build().expect("event loop");
    let proxy = event_loop.create_proxy();

    // Hide the window when an image is provided so the user never sees the
    // intermediate unsized/gray state. We reveal it once the image is ready.
    // When started without an image, show immediately as a drag-and-drop target.
    let start_hidden = initial_path.is_some();

    // Load the application icon from the exe's embedded Win32 resources
    // (placed there by winres/build.rs) and pass it to winit so the
    // title bar and taskbar show devoid.ico instead of the generic icon.
    let window_icon = load_window_icon();

    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Devoid Viewer")
            .with_window_icon(window_icon)
            .with_inner_size(LogicalSize::new(init_w, init_h))
            .with_min_inner_size(LogicalSize::new(120u32, 80u32))
            .with_visible(!start_hidden)
            .build(&event_loop).expect("window"),
    );

    // Clamp to work area and centre before the event loop begins.
    if initial_path.is_some() {
        let (cw, ch) = clamp_to_work_area(init_w, init_h, &window);
        let _ = window.request_inner_size(LogicalSize::new(cw, ch));
        center_window(&window);
    }

    let ctx         = softbuffer::Context::new(window.clone()).expect("softbuffer context");
    let mut surface = Surface::new(&ctx, window.clone()).expect("softbuffer surface");

    // Paint the surface gray synchronously before the event loop starts.
    // This overwrites the DWM white background immediately — the user never
    // sees white because we fill the buffer before yielding to the OS.
    {
        let size  = window.inner_size();
        let win_w = size.width.max(1);
        let win_h = size.height.max(1);
        if surface.resize(
            NonZeroU32::new(win_w).unwrap(),
            NonZeroU32::new(win_h).unwrap(),
        ).is_ok() {
            if let Ok(mut buf) = surface.buffer_mut() {
                let bg = ((BG as u32) << 16) | ((BG as u32) << 8) | BG as u32;
                buf.fill(bg);
                let _ = buf.present();
            }
        }
    }

    let mut app     = App::new(msg_tx, msg_rx);
    let mut cursor  = (0.0f32, 0.0f32);
    let mut shown   = !start_hidden;  // already visible when no image arg

    // Kick off the first image load on a background thread.
    // The window shows gray until it arrives via the normal WakeUp path.
    if let Some(path) = initial_path {
        app.open_image(path, OpenMode::FirstImage, &proxy);
    }

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(match app.next_frame_at {
            Some(t) => ControlFlow::WaitUntil(t),
            None    => ControlFlow::Wait,
        });

        match event {
            Event::UserEvent(WakeUp) => {
                if let Some((iw, ih)) = app.drain_messages(&proxy) {
                    let (cw, ch) = clamp_to_work_area(iw, ih, &window);
                    let _ = window.request_inner_size(LogicalSize::new(cw, ch));
                    app.pending_center = true;
                } else if app.pending_center {
                    center_window(&window);
                    app.pending_center = false;
                    // Reveal the window now: it is the correct size, centred,
                    // and the next RedrawRequested will paint the image.
                    if !shown {
                        shown = true;
                        window.set_visible(true);
                    }
                }
                window.request_redraw();
            }

            Event::NewEvents(_) => {
                if app.tick_animation() {
                    window.request_redraw();
                }
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),

                WindowEvent::Resized(_) => window.request_redraw(),

                WindowEvent::CursorMoved { position, .. } => {
                    cursor = (position.x as f32, position.y as f32);
                    if app.update_drag(cursor) {
                        window.request_redraw();
                    }
                }

                WindowEvent::MouseInput { button: MouseButton::Left, state, .. } => {
                    match state {
                        ElementState::Pressed  => app.start_drag(cursor),
                        ElementState::Released => app.end_drag(),
                    }
                }

                WindowEvent::MouseInput {
                    button: MouseButton::Middle,
                    state: ElementState::Pressed, ..
                } => {
                    app.reset_pan();
                    window.request_redraw();
                }

                WindowEvent::DroppedFile(path) => {
                    if loader::is_supported(&path) {
                        let path = std::fs::canonicalize(&path).unwrap_or(path);
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
                    KeyCode::ArrowRight | KeyCode::ArrowDown | KeyCode::PageDown => {
                        app.navigate(true, &proxy);
                        window.request_redraw();
                    }
                    KeyCode::ArrowLeft | KeyCode::ArrowUp | KeyCode::PageUp => {
                        app.navigate(false, &proxy);
                        window.request_redraw();
                    }
                    KeyCode::Delete if app.current_path.is_some() && !app.deleting => {
                        let fname = app.current_path.as_deref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if confirm_delete_dialog(&fname) {
                            app.request_delete(&proxy);
                        }
                    }
                    KeyCode::Backspace | KeyCode::Home => {
                        app.reset_pan();
                        window.request_redraw();
                    }
                    KeyCode::KeyQ | KeyCode::Escape => elwt.exit(),
                    _ => {}
                },

                WindowEvent::RedrawRequested => {
                    let size  = window.inner_size();
                    let win_w = size.width;
                    let win_h = size.height;
                    if win_w == 0 || win_h == 0 { return; }

                    surface
                        .resize(NonZeroU32::new(win_w).unwrap(),
                                NonZeroU32::new(win_h).unwrap())
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

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Load the application icon from the executable's embedded Win32 resources.
/// winres (build.rs) embeds devoid.ico as resource ID 1 (MAKEINTRESOURCE(1)).
/// Returns None on non-Windows or if loading fails — winit uses its default.
fn load_window_icon() -> Option<winit::window::Icon> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            LoadImageW, GetIconInfo, ICONINFO, DestroyIcon, IMAGE_ICON,
        };
        use windows_sys::Win32::Graphics::Gdi::{
            GetDC, ReleaseDC, GetDIBits, DeleteObject,
            BITMAPINFOHEADER, BITMAPINFO, BI_RGB, DIB_RGB_COLORS, RGBQUAD,
        };
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXSMICON, SM_CYSMICON,
        };

        unsafe {
            // Load the small icon size (typically 16×16) used by the title bar.
            let icon_w = GetSystemMetrics(SM_CXSMICON);
            let icon_h = GetSystemMetrics(SM_CYSMICON);
            let hicon = LoadImageW(
                GetModuleHandleW(std::ptr::null()),
                1 as _,   // resource ID 1 — embedded by winres/build.rs
                IMAGE_ICON,
                icon_w, icon_h,
                0,
            );
            if hicon == 0 { return None; }

            // GetIconInfo gives us the colour bitmap handle.
            let mut info = ICONINFO { fIcon: 0, xHotspot: 0, yHotspot: 0,
                                      hbmMask: 0, hbmColor: 0 };
            if GetIconInfo(hicon, &mut info) == 0 {
                DestroyIcon(hicon);
                return None;
            }

            // BITMAPINFO with 32-bit depth; remaining fields zeroed.
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize     = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biPlanes   = 1;
            bmi.bmiHeader.biCompression = BI_RGB;
            bmi.bmiColors = [RGBQUAD { rgbBlue: 0, rgbGreen: 0,
                                       rgbRed: 0, rgbReserved: 0 }];

            // First GetDIBits call: populate biWidth/biHeight.
            let hdc = GetDC(0);
            GetDIBits(hdc, info.hbmColor, 0, 0,
                      std::ptr::null_mut(), &mut bmi, DIB_RGB_COLORS);
            let w = bmi.bmiHeader.biWidth.unsigned_abs();
            let h = bmi.bmiHeader.biHeight.unsigned_abs();

            let result = if w > 0 && h > 0 {
                // Negative biHeight → top-down row order.
                bmi.bmiHeader.biHeight = -(h as i32);
                let mut pixels = vec![0u8; (w * h * 4) as usize];
                GetDIBits(hdc, info.hbmColor, 0, h,
                          pixels.as_mut_ptr() as *mut _,
                          &mut bmi, DIB_RGB_COLORS);
                // GDI returns BGRA; winit expects RGBA — swap B and R.
                for px in pixels.chunks_exact_mut(4) { px.swap(0, 2); }
                winit::window::Icon::from_rgba(pixels, w, h).ok()
            } else {
                None
            };

            // Clean up all GDI handles regardless of outcome.
            ReleaseDC(0, hdc);
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
            DestroyIcon(hicon);

            result
        }
    }
    #[cfg(not(windows))]
    { None }
}

/// Read only the file header for dimensions — no full decode.
fn image_size_hint(path: &std::path::Path) -> Option<(u32, u32)> {
    let ext = path.extension()?.to_ascii_lowercase();
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
