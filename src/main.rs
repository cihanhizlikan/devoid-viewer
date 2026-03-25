//! Devoid Viewer — main.rs
//!
//! GUI layer: winit (window + events) + softbuffer (CPU pixel blit).
//! No GPU pipeline, no widget toolkit, no font renderer.
//! Threading: crossbeam-channel + std::thread per load; adjacent images preloaded.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod loader;
mod nav;

use std::{collections::HashMap, num::NonZeroU32, path::PathBuf, sync::Arc};

use crossbeam_channel::{bounded, Receiver, Sender};
use loader::load_image;
use nav::DirNavigator;
use softbuffer::Surface;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder},
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Background colour (dark gray) used for the blank state and behind
/// transparent image regions.
const BG: u8 = 42;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Decoded, ready-to-blit image — straight RGBA8, row-major.
#[derive(Clone, Debug)]
struct DecodedImage {
    pixels: Vec<u8>,
    width:  u32,
    height: u32,
}

impl From<loader::RawImage> for DecodedImage {
    fn from(r: loader::RawImage) -> Self {
        Self { pixels: r.pixels, width: r.width, height: r.height }
    }
}

/// Whether the window should resize to match a newly opened image.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenMode { FirstImage, Navigate }

/// Messages from background loader threads to the UI thread.
#[derive(Debug)]
enum LoadMsg {
    /// Primary load — display this image.
    Loaded { path: PathBuf, image: DecodedImage, resize: bool },
    /// Speculative preload — cache for instant navigation.
    Preloaded { path: PathBuf, image: DecodedImage },
    /// Decode failed.
    Error { path: PathBuf, msg: String },
}


/// Zero-size marker injected into the winit event loop by background threads
/// to trigger a repaint after posting to the channel.
#[derive(Debug)]
struct WakeUp;

type Proxy = winit::event_loop::EventLoopProxy<WakeUp>;

// ── App state ─────────────────────────────────────────────────────────────────

struct App {
    current_path:  Option<PathBuf>,
    current_image: Option<DecodedImage>,

    navigator:     DirNavigator,

    load_tx:       Sender<LoadMsg>,
    load_rx:       Receiver<LoadMsg>,
    loading_path:  Option<PathBuf>,
    preload_cache: HashMap<PathBuf, DecodedImage>,

    /// Set after a resize command; triggers centering on the next wake.
    pending_center: bool,
}

impl App {
    fn new(load_tx: Sender<LoadMsg>, load_rx: Receiver<LoadMsg>) -> Self {
        App {
            current_path:   None,
            current_image:  None,
            navigator:      DirNavigator::default(),
            load_tx,
            load_rx,
            loading_path:   None,
            preload_cache:  HashMap::new(),
            pending_center: false,
        }
    }

    // ── Loading ───────────────────────────────────────────────────────────────

    fn open_image(&mut self, path: PathBuf, mode: OpenMode, proxy: &Proxy) {
        if let Some(img) = self.preload_cache.remove(&path) {
            self.apply_image(path, img, mode, proxy);
            return;
        }

        self.loading_path = Some(path.clone());
        self.navigator.set_current(path.clone());

        let tx     = self.load_tx.clone();
        let proxy  = proxy.clone();
        let resize = mode == OpenMode::FirstImage;
        std::thread::spawn(move || {
            let msg = match load_image(&path) {
                Ok(raw) => LoadMsg::Loaded {
                    path,
                    image: raw.into(),
                    resize,
                },
                Err(e) => LoadMsg::Error { path, msg: e.to_string() },
            };
            let _ = tx.send(msg);
            let _ = proxy.send_event(WakeUp);
        });
    }

    fn apply_image(
        &mut self,
        path:  PathBuf,
        img:   DecodedImage,
        mode:  OpenMode,
        proxy: &Proxy,
    ) -> Option<(u32, u32)> {
        self.current_path  = Some(path.clone());
        self.current_image = Some(img.clone());
        self.loading_path  = None;
        self.navigator.set_current(path);
        self.preload_adjacent(proxy);

        if mode == OpenMode::FirstImage {
            Some((img.width, img.height))
        } else {
            None
        }
    }

    fn clear_image(&mut self) {
        self.current_path  = None;
        self.current_image = None;
    }

    fn preload_adjacent(&mut self, proxy: &Proxy) {
        for path in [self.navigator.peek_next(), self.navigator.peek_prev()]
            .into_iter()
            .flatten()
        {
            if self.preload_cache.contains_key(&path) { continue; }
            let tx    = self.load_tx.clone();
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                if let Ok(raw) = load_image(&path) {
                    let _ = tx.send(LoadMsg::Preloaded { path, image: raw.into() });
                    let _ = proxy.send_event(WakeUp);
                }
            });
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    fn navigate(&mut self, forward: bool, proxy: &Proxy) {
        let next = if forward { self.navigator.next() } else { self.navigator.prev() };
        if let Some(p) = next {
            self.open_image(p, OpenMode::Navigate, proxy);
        }
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    fn delete_current(&mut self, proxy: &Proxy) {
        let Some(path) = self.current_path.clone() else { return };
        match std::fs::remove_file(&path) {
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

    // ── Channel drain ─────────────────────────────────────────────────────────

    /// Returns Some((w, h)) if the first image was just loaded and the window
    /// should be resized, None otherwise.
    fn process_messages(&mut self, proxy: &Proxy) -> Option<(u32, u32)> {
        let mut resize_to = None;
        while let Ok(msg) = self.load_rx.try_recv() {
            match msg {
                LoadMsg::Preloaded { path, image } => {
                    self.preload_cache.insert(path, image);
                }
                LoadMsg::Loaded { path, image, resize } => {
                    if self.loading_path.as_deref() == Some(path.as_path()) {
                        let mode = if resize { OpenMode::FirstImage } else { OpenMode::Navigate };
                        if let Some(dims) = self.apply_image(path, image, mode, proxy) {
                            resize_to = Some(dims);
                        }
                    }
                }
                LoadMsg::Error { path, msg } => {
                    if self.loading_path.as_deref() == Some(path.as_path()) {
                        self.loading_path = None;
                        show_error_dialog(&format!("Failed to load image:\n{msg}"));
                    }
                }
            }
        }
        resize_to
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

/// Write pixels into `buf` (softbuffer's 0x00RRGGBB u32 format).
/// Scales the image to fill the window while preserving aspect ratio.
/// Background and transparent regions use BG gray.
fn render(buf: &mut [u32], win_w: u32, win_h: u32, img: Option<&DecodedImage>) {
    let bg = pack_rgb(BG, BG, BG);

    let Some(img) = img else {
        buf.fill(bg);
        return;
    };

    let scale = (win_h as f32 / img.height as f32)
        .min(win_w  as f32 / img.width  as f32);
    let dst_w = (img.width  as f32 * scale) as u32;
    let dst_h = (img.height as f32 * scale) as u32;
    let off_x = (win_w.saturating_sub(dst_w)) / 2;
    let off_y = (win_h.saturating_sub(dst_h)) / 2;

    for dy in 0..win_h {
        for dx in 0..win_w {
            let px = if dx >= off_x && dx < off_x + dst_w
                     && dy >= off_y && dy < off_y + dst_h
            {
                let sx = ((dx - off_x) as f32 / scale) as u32;
                let sy = ((dy - off_y) as f32 / scale) as u32;
                let i  = ((sy.min(img.height - 1) * img.width
                           + sx.min(img.width  - 1)) * 4) as usize;
                let [r, g, b, a] = [
                    img.pixels[i], img.pixels[i+1],
                    img.pixels[i+2], img.pixels[i+3],
                ];
                // Alpha-composite over the background colour.
                let af = a as u32;
                pack_rgb(
                    ((r as u32 * af + BG as u32 * (255 - af)) / 255) as u8,
                    ((g as u32 * af + BG as u32 * (255 - af)) / 255) as u8,
                    ((b as u32 * af + BG as u32 * (255 - af)) / 255) as u8,
                )
            } else {
                bg
            };
            buf[(dy * win_w + dx) as usize] = px;
        }
    }
}

#[inline]
fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

// ── Native dialogs ────────────────────────────────────────────────────────────

/// Shows a Yes/No confirmation dialog. Returns true if the user clicked Yes.
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

// ── Window sizing ─────────────────────────────────────────────────────────────

/// Returns logical usable dimensions: 90% of monitor width, 85% of height.
/// Falls back to 1152×648 if the monitor cannot be queried.
fn monitor_usable(window: &Window) -> (f64, f64) {
    window.current_monitor()
        .or_else(|| window.available_monitors().next())
        .map(|m| {
            let sf = m.scale_factor();
            let s  = m.size();
            (s.width  as f64 / sf * 0.90,
             s.height as f64 / sf * 0.85)
        })
        .unwrap_or((1152.0, 648.0))
}

/// Clamp (w, h) to the monitor's usable area, preserving aspect ratio.
fn clamp_to_monitor(w: u32, h: u32, window: &Window) -> (u32, u32) {
    let (max_w, max_h) = monitor_usable(window);
    let scale = (max_w / w as f64).min(max_h / h as f64).min(1.0);
    ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
}

/// Centre the window on its monitor, keeping the title bar reachable.
fn center_window(window: &Window) {
    let monitor = window.current_monitor()
        .or_else(|| window.available_monitors().next());
    if let Some(mon) = monitor {
        let mp  = mon.position();
        let ms  = mon.size();
        let ws  = window.outer_size();
        let sf  = mon.scale_factor();
        let x   = mp.x + (ms.width  as i32 - ws.width  as i32) / 2;
        let y   = mp.y + (ms.height as i32 - ws.height as i32) / 2;
        let y   = y.max(mp.y + (30.0 * sf) as i32); // never above titlebar height
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let initial_path = std::env::args().nth(1)
        .map(PathBuf::from)
        .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)));

    let (load_tx, load_rx) = bounded::<LoadMsg>(32);

    let event_loop: EventLoop<WakeUp> = EventLoopBuilder::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let proxy = event_loop.create_proxy();

    let (init_w, init_h) = initial_path.as_deref()
        .and_then(image_size_hint)
        .unwrap_or((800, 600));

    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Devoid Viewer")
            .with_inner_size(LogicalSize::new(init_w, init_h))
            .with_min_inner_size(LogicalSize::new(120u32, 80u32))
            .build(&event_loop)
            .expect("Failed to create window"),
    );

    let ctx     = softbuffer::Context::new(window.clone()).expect("softbuffer context");
    let mut surface = Surface::new(&ctx, window.clone()).expect("softbuffer surface");
    let mut app = App::new(load_tx, load_rx);

    if let Some(path) = initial_path {
        app.open_image(path, OpenMode::FirstImage, &proxy);
    }

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);

        match event {
            Event::UserEvent(WakeUp) => {
                if let Some((iw, ih)) = app.process_messages(&proxy) {
                    let (cw, ch) = clamp_to_monitor(iw, ih, &window);
                    let _ = window.request_inner_size(LogicalSize::new(cw, ch));
                    app.pending_center = true;
                } else if app.pending_center {
                    center_window(&window);
                    app.pending_center = false;
                }
                window.request_redraw();
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),

                WindowEvent::Resized(_) => window.request_redraw(),

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
                    KeyCode::Delete => {
                        if app.current_path.is_some() {
                            let fname = app.current_path.as_deref()
                                .and_then(|p| p.file_name())
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            if confirm_delete_dialog(&fname) {
                                app.delete_current(&proxy);
                                window.request_redraw();
                            }
                        }
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
                    render(&mut buf, win_w, win_h, app.current_image.as_ref());
                    buf.present().expect("present");
                }

                _ => {}
            },

            _ => {}
        }
    }).expect("event loop error");
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Read only the file header for dimensions — avoids a full decode.
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
