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
//! - Live directory watching for external file additions/deletions

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod help;
mod loader;
mod nav;
#[cfg(windows)]
mod win32;

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
use notify_debouncer_mini::{new_debouncer, Debouncer};
use softbuffer::Surface;
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{
        ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
    },
    event_loop::{ControlFlow, EventLoopBuilder},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen, Window, WindowBuilder},
};

// ── Type aliases ─────────────────────────────────────────────────────────────

/// Opaque handle that keeps the filesystem watcher alive.
/// Dropping it unregisters the watch.
type DirWatcher = Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>;

type Proxy = winit::event_loop::EventLoopProxy<WakeUp>;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Background colour (grey) for the blank state and transparent image regions.
const BG: u8 = 42;

/// Application name shown in the title bar when no image is open.
const APP_NAME: &str = "Devoid Viewer";

/// Maximum gap between two left-clicks that counts as a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// Debounce window for the directory watcher, coalescing rapid bursts
/// (e.g. a batch copy) into a single rebuild.
const DIR_WATCH_DEBOUNCE: Duration = Duration::from_millis(50);

// ── Message types ─────────────────────────────────────────────────────────────

/// Messages posted from background threads to the UI thread.
#[derive(Debug)]
enum Msg {
    /// Primary decode finished successfully.
    Loaded {
        path: PathBuf,
        image: DecodedImage,
        mode: OpenMode,
    },
    /// Decoded after header sniffing — the file's extension was wrong.
    /// Carries the correct extension so the UI can offer to rename.
    WrongExtension {
        path: PathBuf,
        image: DecodedImage,
        mode: OpenMode,
        correct_ext: String,
    },
    /// Speculative decode for a neighbour file (preload cache).
    Preloaded { path: PathBuf, image: DecodedImage },
    /// File is not a recognised image even after header sniffing.
    Unsupported { path: PathBuf, reason: String },
    /// Async deletion finished.
    Deleted {
        path: PathBuf,
        result: Result<(), String>,
    },
    /// The user answered No to the delete confirmation.
    DeleteCancelled,
    /// The user accepted the wrong-extension rename and it succeeded.
    Renamed {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    /// The watched directory changed (file added or removed).
    /// Triggers a navigator rebuild on the UI thread.
    DirectoryChanged,
}

/// Zero-size token injected by background threads to wake the sleeping
/// event loop after posting a `Msg`.
#[derive(Debug)]
struct WakeUp;

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
    resize_to: Option<(u32, u32)>,
    /// Non-None when any error occurred — reveal the window then show this.
    error_msg: Option<String>,
    /// Non-None when a file decoded under the wrong extension — offer a rename
    /// once the drain is done.
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
    current_path: Option<PathBuf>,
    current_image: Option<DecodedImage>,

    // ── Animation ─────────────────────────────────────────────────────────────
    frame_index: usize,
    next_frame_at: Option<Instant>,

    // ── Navigation ────────────────────────────────────────────────────────────
    navigator: DirNavigator,

    // ── Async I/O ─────────────────────────────────────────────────────────────
    msg_tx: Sender<Msg>,
    msg_rx: Receiver<Msg>,
    /// Path currently being decoded on a background thread.
    /// Used to discard stale messages when the user navigates quickly.
    loading_path: Option<PathBuf>,
    preload_cache: HashMap<PathBuf, DecodedImage>,
    /// True while a deletion is in flight — prevents a second Delete keypress.
    deleting: bool,

    // ── Window state ──────────────────────────────────────────────────────────
    /// True while waiting for a resize to land before revealing the window.
    /// The event loop sends itself a WakeUp after requesting the resize;
    /// on the following WakeUp it centres and reveals the window.
    pending_center: bool,
    /// Physical inner size saved before entering managed-maximise so we can
    /// restore it exactly. The position is not saved — we always centre on
    /// restore, which is simpler and avoids off-screen placement when the
    /// user has moved the window to a secondary monitor.
    pre_maximize: Option<PhysicalSize<u32>>,
    /// True when the window is in our managed-maximise state.
    maximized: bool,
    /// True when the window is in borderless fullscreen.
    fullscreen: bool,
    /// Timestamp of the last left-click, for double-click detection.
    last_click: Option<Instant>,
    /// True while the key-bindings overlay (`H`) is drawn over the image.
    show_help: bool,
    /// The overlay's rendered text, kept between frames.
    #[cfg(windows)]
    help: help::Help,

    // ── Directory watching ────────────────────────────────────────────────────
    /// Active filesystem watcher for the current directory.
    /// Replaced whenever the watched directory changes; dropping it
    /// unregisters the OS watch handle.
    dir_watcher: Option<DirWatcher>,
    /// The directory currently being watched. Tracked separately from the
    /// navigator because `set_current()` updates the navigator before
    /// `apply_image()` runs, making the navigator an unreliable signal
    /// for whether the watcher needs to be (re)installed.
    watched_dir: Option<PathBuf>,
}

impl App {
    fn new(msg_tx: Sender<Msg>, msg_rx: Receiver<Msg>) -> Self {
        App {
            current_path: None,
            current_image: None,
            frame_index: 0,
            next_frame_at: None,
            navigator: DirNavigator::default(),
            msg_tx,
            msg_rx,
            loading_path: None,
            preload_cache: HashMap::new(),
            deleting: false,
            pending_center: false,
            pre_maximize: None,
            maximized: false,
            fullscreen: false,
            last_click: None,
            show_help: false,
            #[cfg(windows)]
            help: help::Help::default(),
            dir_watcher: None,
            watched_dir: None,
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
        self.current_path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    }

    // ── Window mode ───────────────────────────────────────────────────────────

    /// Record a left-click and return `true` when it completes a double-click.
    fn register_click(&mut self) -> bool {
        let now = Instant::now();
        let is_double = self
            .last_click
            .is_some_and(|t| now.duration_since(t) <= DOUBLE_CLICK);
        self.last_click = Some(now);
        is_double
    }

    /// Toggle between our managed-maximise and the saved window size.
    ///
    /// Saves the physical inner size before maximising and restores it
    /// exactly on the next toggle. Always centres on restore. Independent
    /// of fullscreen — double-click in fullscreen is a no-op.
    fn toggle_maximize(&mut self, window: &Window) {
        if self.fullscreen {
            return;
        }

        if self.maximized {
            debug_assert!(
                self.pre_maximize.is_some(),
                "maximized=true but pre_maximize=None — state desync"
            );
            window.set_maximized(false);
            if let Some(size) = self.pre_maximize.take() {
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
        let target = if self.fullscreen {
            None
        } else {
            Some(Fullscreen::Borderless(None))
        };
        window.set_fullscreen(target);
        self.fullscreen = !self.fullscreen;
    }

    /// Exit fullscreen if active. Returns `true` when a change was made.
    fn exit_fullscreen(&mut self, window: &Window) -> bool {
        if !self.fullscreen {
            return false;
        }
        window.set_fullscreen(None);
        self.fullscreen = false;
        true
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

        let tx = self.msg_tx.clone();
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let msg = match load_image(&path) {
                LoadOutcome::Ok(img) => Msg::Loaded {
                    path,
                    image: img,
                    mode,
                },
                LoadOutcome::WrongExtension(img, ext) => Msg::WrongExtension {
                    path,
                    image: img,
                    mode,
                    correct_ext: ext,
                },
                LoadOutcome::Unsupported(reason) => Msg::Unsupported { path, reason },
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
        img: DecodedImage,
        mode: OpenMode,
        proxy: &Proxy,
    ) -> Option<(u32, u32)> {
        let resize = (mode == OpenMode::FirstImage).then_some((img.width, img.height));
        self.current_image = Some(img);
        self.current_path = Some(path.clone());
        self.loading_path = None;

        // (Re)install the directory watcher when the parent directory changes.
        // We compare against `watched_dir` rather than the navigator's
        // `current_dir` because `open_image` calls `set_current` (updating
        // the navigator) before the decode thread finishes and this runs.
        let new_dir = path.parent().map(PathBuf::from);
        if new_dir.as_deref() != self.watched_dir.as_deref() {
            if let Some(ref dir) = new_dir {
                self.watch_dir(dir, proxy);
            }
        }
        self.navigator.set_current(path);

        self.preload_adjacent(proxy);
        self.reset_animation();
        resize
    }

    fn clear_image(&mut self) {
        self.current_image = None;
        self.current_path = None;
        self.reset_animation();
    }

    // ── Directory watching ────────────────────────────────────────────────────

    /// Start watching `dir` for file additions and deletions.
    ///
    /// Replaces any existing watcher (dropping it unregisters the old watch).
    /// On any failure the watcher is simply absent — navigation still works,
    /// the sibling list is just static until the next directory change.
    fn watch_dir(&mut self, dir: &Path, proxy: &Proxy) {
        let tx = self.msg_tx.clone();
        let proxy = proxy.clone();

        let ok = (|| {
            let mut debouncer = new_debouncer(
                DIR_WATCH_DEBOUNCE,
                move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, _>| {
                    if events.is_ok() {
                        let _ = tx.send(Msg::DirectoryChanged);
                        let _ = proxy.send_event(WakeUp);
                    }
                },
            )
            .ok()?;

            use notify_debouncer_mini::notify::RecursiveMode;
            debouncer
                .watcher()
                .watch(dir, RecursiveMode::NonRecursive)
                .ok()?;
            Some(debouncer)
        })();

        self.dir_watcher = ok;
        self.watched_dir = self.dir_watcher.as_ref().map(|_| dir.to_path_buf());
    }

    /// Called when the OS reports a change in the watched directory.
    ///
    /// Rebuilds the navigator's sibling list. If the currently displayed
    /// file is no longer present (externally deleted), navigates to the
    /// next available sibling or clears the display.
    fn on_directory_changed(&mut self, proxy: &Proxy) {
        self.preload_cache.clear();
        let current = self.current_path.clone();
        self.navigator.rebuild_current(current.as_deref());

        let still_present = current
            .as_deref()
            .is_some_and(|p| self.navigator.contains(p));

        if !still_present {
            match self.navigator.current().cloned() {
                Some(next) => self.open_image(next, OpenMode::Navigate, proxy),
                None => self.clear_image(),
            }
        }
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
            if self.preload_cache.contains_key(&path) {
                continue;
            }
            let tx = self.msg_tx.clone();
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
        self.frame_index = 0;
        self.next_frame_at = self
            .current_image
            .as_ref()
            .filter(|i| i.is_animated())
            .map(|i| Instant::now() + Duration::from_millis(i.frame(0).delay_ms as u64));
    }

    /// Advance one GIF frame if the deadline has passed.
    /// Returns `true` when a redraw is needed.
    fn tick_animation(&mut self) -> bool {
        let Some(deadline) = self.next_frame_at else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        let Some(img) = &self.current_image else {
            self.next_frame_at = None;
            return false;
        };

        self.frame_index = (self.frame_index + 1) % img.frames.len();
        let delay = img.frame(self.frame_index).delay_ms;
        self.next_frame_at = Some(Instant::now() + Duration::from_millis(delay as u64));
        true
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Navigate to the next or previous image, skipping siblings that no
    /// longer exist on disk (externally deleted since the sibling list was
    /// last built). The loop bound prevents infinite cycling if every file
    /// disappears simultaneously.
    fn navigate(&mut self, forward: bool, proxy: &Proxy) {
        for _ in 0..self.navigator.len() {
            let next = if forward {
                self.navigator.next()
            } else {
                self.navigator.prev()
            };
            match next {
                None => return,
                Some(path) if path.exists() => {
                    self.open_image(path, OpenMode::Navigate, proxy);
                    return;
                }
                Some(gone) => {
                    self.navigator.remove(&gone);
                    self.preload_cache.remove(&gone);
                }
            }
        }
        // Every sibling is gone.
        self.preload_cache.clear();
        self.clear_image();
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    /// Confirm, then move the current file to the Recycle Bin, all on a
    /// background thread (see `spawn_dialog`).
    ///
    /// Recycling, not deleting, so a mistaken Delete is undoable. Where the
    /// volume has no Recycle Bin (some network shares and removable drives)
    /// the shell itself warns before deleting permanently.
    fn request_delete(&mut self, proxy: &Proxy) {
        let (Some(path), Some(filename)) = (self.current_path.clone(), self.current_filename())
        else {
            return;
        };
        self.deleting = true;
        self.spawn_dialog(proxy, move || {
            if !confirm_delete_dialog(&filename) {
                return Some(Msg::DeleteCancelled);
            }
            let result = recycle(&path);
            Some(Msg::Deleted { path, result })
        });
    }

    fn on_deleted(
        &mut self,
        path: PathBuf,
        result: Result<(), String>,
        proxy: &Proxy,
    ) -> Option<String> {
        self.deleting = false;
        match result {
            Ok(()) => {
                let was_current = self.current_path.as_deref() == Some(path.as_path());
                self.navigator.remove(&path);
                self.preload_cache.remove(&path);
                // The viewer stays usable while the confirmation is open, so
                // the user may have moved on; only replace the view if the
                // deleted file is the one on screen.
                if was_current {
                    self.preload_cache.clear();
                    match self.navigator.current().cloned() {
                        Some(next) => self.open_image(next, OpenMode::Navigate, proxy),
                        None => self.clear_image(),
                    }
                }
                None
            }
            Err(e) => Some(format!("Delete failed:\n{e}")),
        }
    }

    /// Offer to rename a file whose extension does not match its contents,
    /// on a background thread (see `spawn_dialog`).
    fn request_rename(&self, path: PathBuf, correct_ext: String, proxy: &Proxy) {
        self.spawn_dialog(proxy, move || {
            offer_rename(&path, &correct_ext).map(|new_path| Msg::Renamed {
                old_path: path,
                new_path,
            })
        });
    }

    /// Run a blocking dialog flow on its own thread and post its outcome.
    ///
    /// A message box runs a modal message loop. Run inside a winit callback,
    /// that loop receives the viewer's `WM_PAINT`, which winit 0.29 cannot
    /// deliver mid-callback and re-posts with `RedrawWindow` — a paint storm
    /// that pins a core and keeps the dialog from ever becoming visible
    /// (it shows only once its queue is idle). On its own thread the dialog
    /// has its own queue, and the event loop keeps painting.
    fn spawn_dialog(&self, proxy: &Proxy, flow: impl FnOnce() -> Option<Msg> + Send + 'static) {
        let tx = self.msg_tx.clone();
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            if let Some(msg) = flow() {
                let _ = tx.send(msg);
                let _ = proxy.send_event(WakeUp);
            }
        });
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
                Msg::WrongExtension {
                    path,
                    image,
                    mode,
                    correct_ext,
                } => {
                    if self.is_active_load(&path) {
                        dr.resize_to = self.apply_image(path.clone(), image, mode, proxy);
                        debug_assert!(
                            dr.rename_offer.is_none(),
                            "two WrongExtension messages in one drain — second rename dropped"
                        );
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
                Msg::DeleteCancelled => {
                    self.deleting = false;
                }
                Msg::Renamed { old_path, new_path } => {
                    self.apply_rename(&old_path, new_path);
                }
                Msg::DirectoryChanged => {
                    self.on_directory_changed(proxy);
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

/// Largest uniform scale that fits the image inside the window. Small images
/// are enlarged to fill it too — intended; see docs/settled-decisions.md.
#[inline]
fn fit_scale(img_w: u32, img_h: u32, win_w: u32, win_h: u32) -> f32 {
    (win_h as f32 / img_h as f32).min(win_w as f32 / img_w as f32)
}

/// Point-sample an RGBA8 buffer at float source coordinates.
#[inline]
fn nearest(pixels: &[u8], w: u32, sx: f32, sy: f32) -> [u8; 4] {
    let x = (sx as u32).min(w - 1);
    let y = sy as u32; // caller guarantees sy ∈ [0, height)
    let i = (y as usize * w as usize + x as usize) * 4;
    pixels[i..i + 4].try_into().unwrap()
}

/// Alpha-blend a single channel: `(src * alpha + bg * (255 - alpha)) / 255`.
#[inline]
fn blend_channel(src: u8, alpha: u32) -> u8 {
    ((src as u32 * alpha + BG as u32 * (255 - alpha)) / 255) as u8
}

/// Alpha-composite a straight-RGBA pixel over the background colour.
#[inline]
fn composite(r: u8, g: u8, b: u8, a: u8) -> u32 {
    if a == 255 {
        return pack_rgb(r, g, b);
    }
    if a == 0 {
        return pack_rgb(BG, BG, BG);
    }
    let a32 = a as u32;
    pack_rgb(
        blend_channel(r, a32),
        blend_channel(g, a32),
        blend_channel(b, a32),
    )
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
    let dst_w = (v.image.width as f32 * scale) as i32;
    let dst_h = (v.image.height as f32 * scale) as i32;
    let off_x = ((win_w as f32 - dst_w as f32) / 2.0) as i32;
    let off_y = ((win_h as f32 - dst_h as f32) / 2.0) as i32;

    let pixels = v.image.frame(v.frame).pixels.as_slice();
    let iw = v.image.width;
    let inv_scale = 1.0 / scale;

    for dy in 0..win_h as i32 {
        let row_start = dy as usize * win_w as usize;
        let row = &mut buf[row_start..row_start + win_w as usize];

        if dy < off_y || dy >= off_y + dst_h {
            row.fill(bg);
            continue;
        }

        let sy = (dy - off_y) as f32 * inv_scale;

        let img_x0 = off_x.max(0) as usize;
        let img_x1 = (off_x + dst_w).min(win_w as i32) as usize;
        row[..img_x0].fill(bg);

        for (dx, px) in (img_x0..).zip(&mut row[img_x0..img_x1]) {
            let [r, g, b, a] = nearest(pixels, iw, (dx as i32 - off_x) as f32 * inv_scale, sy);
            *px = composite(r, g, b, a);
        }

        row[img_x1..].fill(bg);
    }
}

// ── Native dialogs (Windows) ──────────────────────────────────────────────────
//
// The blocking ones below run off the event-loop thread only: see
// `App::spawn_dialog` for why a message box must never run inside a winit
// callback.

fn confirm_delete_dialog(filename: &str) -> bool {
    #[cfg(windows)]
    {
        win32::ask_yes_no(
            &format!("Move to the Recycle Bin?\n\n{filename}"),
            "Delete Image",
            win32::Ask::Warning,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = filename;
        true
    }
}

/// Show an error box on its own thread and return at once; there is no
/// answer to wait for.
fn show_error_dialog(msg: String) {
    #[cfg(windows)]
    std::thread::spawn(move || win32::show_error(&msg, APP_NAME));
    #[cfg(not(windows))]
    {
        eprintln!("Error: {msg}");
    }
}

/// Offer to rename `path` to use `correct_ext`.
///
/// Skips the prompt silently when the file is not writable (e.g. on a
/// read-only volume or a CD-ROM). We probe by opening the file with write
/// access rather than checking the parent directory's read-only attribute,
/// because on Windows all shell folders (Desktop, Downloads, Pictures,
/// etc.) have that attribute set by convention for Explorer's custom-view
/// mechanism, even though they are fully writable.
///
/// When the target path already exists (e.g. renaming `5.png` → `5.jpg` but
/// `5.jpg` already exists), a numbered suffix is appended: `5 (1).jpg`,
/// `5 (2).jpg`, etc.
/// Returns `Some(new_path)` when the rename succeeded, `None` otherwise.
fn offer_rename(path: &Path, correct_ext: &str) -> Option<PathBuf> {
    let writable = std::fs::OpenOptions::new().write(true).open(path).is_ok();
    if !writable {
        return None;
    }

    let new_path = unique_rename_target(path, correct_ext);

    #[cfg(windows)]
    {
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
        if win32::ask_yes_no(&prompt, "Wrong Extension", win32::Ask::Question) {
            match rename_to_free_name(path, correct_ext) {
                Ok(renamed) => return Some(renamed),
                Err(e) => win32::show_error(&format!("Could not rename file:\n{e}"), APP_NAME),
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = new_path;
    }

    None
}

/// Rename `path` to the first free name with extension `ext`, and return it.
///
/// The name is picked again here, after the confirmation box has closed,
/// rather than reused from the prompt: `std::fs::rename` replaces an existing
/// target on Windows, so a file given the prompted name while the box was
/// open (by a sync client, a download) would otherwise be overwritten. This
/// narrows that window to the gap between the check and the rename; closing
/// it needs a no-replace move, which std does not offer.
fn rename_to_free_name(path: &Path, ext: &str) -> std::io::Result<PathBuf> {
    let target = unique_rename_target(path, ext);
    std::fs::rename(path, &target)?;
    Ok(target)
}

/// Move `path` to the Recycle Bin, so a mistaken Delete is undoable.
///
/// `trash` unwraps a failed COM instantiation internally. Under
/// `panic = "abort"` that ends the process, recorded in the Event Log, rather
/// than leaving the viewer's `deleting` flag stuck at `true`.
fn recycle(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

/// Return a path that does not yet exist by appending ` (n)` before the
/// extension when necessary.
///
/// Examples:
/// - `5.png` + `"jpg"` → `5.jpg`          (if `5.jpg` does not exist)
/// - `5.png` + `"jpg"` → `5 (1).jpg`      (if `5.jpg` already exists)
/// - `5.png` + `"jpg"` → `5 (2).jpg`      (if `5.jpg` and `5 (1).jpg` exist)
///
/// The stem stays an `OsStr`: a Windows name need not be valid Unicode, and a
/// lossy conversion would rename the file to something else.
fn unique_rename_target(path: &Path, ext: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new(""));
    let named = |suffix: &str| {
        let mut name = stem.to_os_string();
        name.push(suffix);
        name.push(".");
        name.push(ext);
        parent.join(name)
    };

    let candidate = named("");
    if !candidate.exists() {
        return candidate;
    }

    for n in 1u32.. {
        let candidate = named(&format!(" ({n})"));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("could not find a unique rename target")
}

// ── Window helpers ────────────────────────────────────────────────────────────

/// Return the best available monitor, preferring the one the window is on.
fn current_or_any_monitor(window: &Window) -> Option<winit::monitor::MonitorHandle> {
    window
        .current_monitor()
        .or_else(|| window.available_monitors().next())
}

/// Heuristic work area as a physical-pixel rect when no OS API is available.
/// Uses 90% × 85% of the monitor size, anchored at the monitor origin.
fn heuristic_work_area(monitor: &winit::monitor::MonitorHandle) -> (i32, i32, i32, i32) {
    let pos = monitor.position();
    let size = monitor.size();
    let w = (size.width as f64 * 0.90) as i32;
    let h = (size.height as f64 * 0.85) as i32;
    (pos.x, pos.y, pos.x + w, pos.y + h)
}

/// Usable desktop dimensions in logical pixels.
/// On Windows uses `SPI_GETWORKAREA` for exact taskbar exclusion.
/// Elsewhere uses 90% × 85% of the monitor size as a conservative heuristic.
fn work_area(window: &Window) -> (f64, f64) {
    #[cfg(windows)]
    if let Some((l, t, r, b)) = win32::work_area() {
        let sf = current_or_any_monitor(window)
            .map(|m| m.scale_factor())
            .unwrap_or(1.0);
        return ((r - l) as f64 / sf, (b - t) as f64 / sf);
    }

    current_or_any_monitor(window)
        .map(|m| {
            let sf = m.scale_factor();
            let s = m.size();
            (s.width as f64 / sf * 0.90, s.height as f64 / sf * 0.85)
        })
        .unwrap_or((1152.0, 648.0))
}

/// Scale `(w, h)` down so the outer window fits inside the work area.
///
/// The title bar height is measured exactly from the live window frame
/// (outer minus inner size) so no hardcoded pixel value is assumed.
fn clamp_to_work_area(w: u32, h: u32, window: &Window) -> (u32, u32) {
    let (max_w, max_h) = work_area(window);
    let title_bar_h = window
        .outer_size()
        .height
        .saturating_sub(window.inner_size().height) as f64;
    let available_h = (max_h - title_bar_h).max(1.0);
    let scale = (max_w / w as f64).min(available_h / h as f64).min(1.0);
    ((w as f64 * scale) as u32, (h as f64 * scale) as u32)
}

/// Centre the window within the usable work area.
///
/// On Windows the work area origin and bounds come from `SPI_GETWORKAREA`,
/// so the window is always placed clear of the taskbar regardless of which
/// edge it is on.
fn center_window(window: &Window) {
    let Some(mon) = current_or_any_monitor(window) else {
        return;
    };
    let ws = window.outer_size();

    #[cfg(windows)]
    let (wa_left, wa_top, wa_right, wa_bottom) =
        win32::work_area().unwrap_or_else(|| heuristic_work_area(&mon));

    #[cfg(not(windows))]
    let (wa_left, wa_top, wa_right, wa_bottom) = heuristic_work_area(&mon);

    let wa_w = wa_right - wa_left;
    let wa_h = wa_bottom - wa_top;

    let x = wa_left + (wa_w - ws.width as i32) / 2;

    // Pin to the top if taller than the work area so the title bar stays
    // accessible; never let the bottom exceed wa_bottom.
    let max_y = wa_bottom - ws.height as i32;
    let y = (wa_top + (wa_h - ws.height as i32) / 2)
        .max(wa_top)
        .min(max_y.max(wa_top));

    window.set_outer_position(PhysicalPosition::new(x, y));
}

/// Set the window title to `"filename — Devoid Viewer"`, or just
/// `"Devoid Viewer"` when `filename` is `None`.
fn update_title(window: &Window, filename: Option<&str>) {
    match filename {
        Some(name) => window.set_title(&format!("{name} \u{2014} {APP_NAME}")),
        None => window.set_title(APP_NAME),
    }
}

/// Interpret a scroll delta as a navigation direction.
/// Returns `true` for forward (next image) and `false` for backward (previous).
#[inline]
fn scroll_is_forward(delta: &MouseScrollDelta) -> bool {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            if *y != 0.0 {
                *y < 0.0
            } else {
                *x > 0.0
            }
        }
        MouseScrollDelta::PixelDelta(pos) => {
            if pos.y != 0.0 {
                pos.y < 0.0
            } else {
                pos.x > 0.0
            }
        }
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Decode the application icon embedded at compile time and return a winit Icon.
fn load_window_icon() -> Option<winit::window::Icon> {
    let bytes = include_bytes!("../devoid.ico");
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico).ok()?;
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), w, h).ok()
}

/// Read only enough of the file to determine its pixel dimensions.
/// For all supported formats this is just the header — no full decode needed.
fn image_size_hint(path: &Path) -> Option<(u32, u32)> {
    let reader = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    image::ImageReader::new(reader)
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Fill the softbuffer surface with the background colour.
/// Called once before the event loop to overwrite the DWM white fill that
/// appears on every new Win32 window before the application first paints.
///
/// Errors are intentionally swallowed: this is a cosmetic pre-paint.
fn paint_background(surface: &mut Surface<Arc<Window>, Arc<Window>>, window: &Window) {
    let size = window.inner_size();
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
    if !shown {
        window.set_visible(true);
    }
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
    let initial_path = std::env::args().nth(1).map(PathBuf::from);

    let (init_w, init_h) = initial_path
        .as_deref()
        .and_then(image_size_hint)
        .unwrap_or((800, 600));

    let (msg_tx, msg_rx) = bounded::<Msg>(32);

    let mut el_builder: EventLoopBuilder<WakeUp> = EventLoopBuilder::with_user_event();

    // SC_CLOSE re-posted as WM_CLOSE, SC_MAXIMIZE routed to toggle_maximize.
    #[cfg(windows)]
    win32::install_system_command_hook(&mut el_builder);

    let event_loop = el_builder.build().expect("event loop");
    let proxy = event_loop.create_proxy();

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

    let ctx = softbuffer::Context::new(window.clone()).expect("softbuffer context");
    let mut surface = Surface::new(&ctx, window.clone()).expect("softbuffer surface");
    paint_background(&mut surface, &window);

    let mut app = App::new(msg_tx, msg_rx);
    let mut shown = !start_hidden;

    if let Some(path) = initial_path {
        app.open_image(path, OpenMode::FirstImage, &proxy);
    }

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(match app.next_frame_at {
                Some(t) => ControlFlow::WaitUntil(t),
                None => ControlFlow::Wait,
            });

            match event {
                Event::UserEvent(WakeUp) => {
                    let dr = app.drain_messages(&proxy);

                    if let Some((iw, ih)) = dr.resize_to {
                        let (cw, ch) = clamp_to_work_area(iw, ih, &window);
                        let _ = window.request_inner_size(LogicalSize::new(cw, ch));
                        app.pending_center = true;
                        let _ = proxy.send_event(WakeUp);
                    } else if app.pending_center {
                        center_window(&window);
                        app.pending_center = false;
                        shown = ensure_visible(&window, shown);
                    }

                    update_title(&window, app.current_filename().as_deref());

                    if let Some(msg) = dr.error_msg {
                        shown = ensure_visible(&window, shown);
                        show_error_dialog(msg);
                    }

                    if let Some((path, correct_ext)) = dr.rename_offer {
                        app.request_rename(path, correct_ext, &proxy);
                    }

                    window.request_redraw();
                }

                Event::NewEvents(_) => {
                    if app.tick_animation() {
                        window.request_redraw();
                    }
                }

                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => quit(),

                    WindowEvent::Resized(_) => {
                        if window.is_maximized() && !app.maximized {
                            app.maximized = true;
                        }
                        window.request_redraw();
                    }

                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        if app.register_click() {
                            app.toggle_maximize(&window);
                            if app.pending_center {
                                let _ = proxy.send_event(WakeUp);
                            }
                            window.request_redraw();
                        }
                    }

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
                        event:
                            KeyEvent {
                                physical_key: PhysicalKey::Code(code),
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    } => match code {
                        KeyCode::ArrowRight | KeyCode::ArrowDown | KeyCode::PageDown => {
                            app.navigate(true, &proxy);
                            window.request_redraw();
                        }
                        KeyCode::ArrowLeft | KeyCode::ArrowUp | KeyCode::PageUp => {
                            app.navigate(false, &proxy);
                            window.request_redraw();
                        }
                        KeyCode::Delete if !app.deleting => app.request_delete(&proxy),
                        KeyCode::KeyF | KeyCode::F11 => {
                            app.toggle_fullscreen(&window);
                            window.request_redraw();
                        }
                        KeyCode::KeyH => {
                            app.show_help = !app.show_help;
                            window.request_redraw();
                        }
                        // One Esc closes the help first, a second leaves fullscreen.
                        KeyCode::Escape => {
                            if app.show_help {
                                app.show_help = false;
                                window.request_redraw();
                            } else if app.exit_fullscreen(&window) {
                                window.request_redraw();
                            }
                        }
                        KeyCode::KeyQ => quit(),
                        _ => {}
                    },

                    WindowEvent::RedrawRequested => {
                        let size = window.inner_size();
                        let win_w = size.width;
                        let win_h = size.height;
                        if win_w == 0 || win_h == 0 {
                            return;
                        }

                        let (Some(nzw), Some(nzh)) =
                            (NonZeroU32::new(win_w), NonZeroU32::new(win_h))
                        else {
                            return;
                        };
                        if surface.resize(nzw, nzh).is_err() {
                            return;
                        }
                        let Ok(mut buf) = surface.buffer_mut() else {
                            return;
                        };
                        render(&mut buf, win_w, win_h, app.view());
                        if app.show_help {
                            #[cfg(windows)]
                            app.help.draw(&mut buf, win_w, win_h, window.scale_factor());
                        }
                        // Nothing to recover: the next RedrawRequested repaints.
                        let _ = buf.present();
                    }

                    _ => {}
                },

                _ => {}
            }
        })
        .expect("event loop error");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use loader::Frame;

    /// The background grey as a softbuffer pixel, computed independently of
    /// `pack_rgb` so the tests follow `BG` rather than pin its value.
    const BG_PIXEL: u32 = BG as u32 * 0x0001_0101;

    fn single_frame(width: u32, height: u32, pixels: Vec<u8>) -> DecodedImage {
        DecodedImage {
            frames: vec![Frame {
                pixels,
                delay_ms: 0,
            }],
            width,
            height,
        }
    }

    // ── Pixels ────────────────────────────────────────────────────────────────

    #[test]
    fn pack_rgb_is_softbuffer_xrgb() {
        assert_eq!(pack_rgb(0x12, 0x34, 0x56), 0x0012_3456);
        assert_eq!(pack_rgb(BG, BG, BG), BG_PIXEL);
    }

    #[test]
    fn composite_handles_opaque_transparent_and_partial_alpha() {
        assert_eq!(composite(255, 0, 0, 255), 0x00FF_0000);
        assert_eq!(composite(255, 0, 0, 0), BG_PIXEL);
        // Half alpha lands halfway between the colour and the background.
        let half = composite(255, 0, 0, 128);
        let (r, g, b) = (half >> 16, (half >> 8) & 0xFF, half & 0xFF);
        let bg = u32::from(BG);
        assert!(r.abs_diff((255 + bg) / 2) <= 1, "red {r}");
        assert!(g.abs_diff(bg / 2) <= 1 && g == b, "green {g}, blue {b}");
        // A fifth of the way, so a swapped weight cannot pass as halfway does.
        let fifth = composite(255, 0, 0, 51) >> 16;
        assert!(fifth.abs_diff(bg + (255 - bg) / 5) <= 1, "red {fifth}");
    }

    #[test]
    fn nearest_clamps_x_to_the_last_column() {
        let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(nearest(&pixels, 2, 0.0, 0.0), [1, 2, 3, 4]);
        assert_eq!(nearest(&pixels, 2, 9.0, 0.0), [5, 6, 7, 8]);
    }

    #[test]
    fn fit_scale_shrinks_to_the_tighter_dimension() {
        assert_eq!(fit_scale(200, 100, 100, 100), 0.5);
        assert_eq!(fit_scale(100, 400, 100, 100), 0.25);
        assert_eq!(fit_scale(100, 100, 100, 100), 1.0);
    }

    #[test]
    fn fit_scale_enlarges_a_small_image_to_fill_the_window() {
        assert_eq!(fit_scale(50, 25, 200, 200), 4.0);
    }

    // ── Rendering ─────────────────────────────────────────────────────────────

    #[test]
    fn render_without_an_image_fills_the_background() {
        let mut buf = [0u32; 6];
        render(&mut buf, 3, 2, None);
        assert_eq!(buf, [BG_PIXEL; 6]);
    }

    #[test]
    fn render_centres_the_image_with_background_around_it() {
        let image = single_frame(1, 1, vec![255, 0, 0, 255]);
        let mut buf = [0u32; 3];
        // Window 3×1: scale = min(1/1, 3/1) = 1, so the pixel lands in the middle.
        render(
            &mut buf,
            3,
            1,
            Some(View {
                image: &image,
                frame: 0,
            }),
        );
        assert_eq!(buf, [BG_PIXEL, 0x00FF_0000, BG_PIXEL]);
    }

    // ── Input ─────────────────────────────────────────────────────────────────

    #[test]
    fn scrolling_down_or_right_goes_forward() {
        use MouseScrollDelta::*;
        assert!(scroll_is_forward(&LineDelta(0.0, -1.0)));
        assert!(!scroll_is_forward(&LineDelta(0.0, 1.0)));
        assert!(scroll_is_forward(&LineDelta(1.0, 0.0)));
        assert!(!scroll_is_forward(&LineDelta(-1.0, 0.0)));
        assert!(scroll_is_forward(&PixelDelta(PhysicalPosition::new(
            0.0, -3.0
        ))));
        assert!(!scroll_is_forward(&PixelDelta(PhysicalPosition::new(
            0.0, 3.0
        ))));
    }

    // ── Rename target ─────────────────────────────────────────────────────────

    #[test]
    fn rename_target_appends_a_counter_until_the_name_is_free() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("5.png");
        assert_eq!(unique_rename_target(&file, "jpg"), dir.path().join("5.jpg"));

        std::fs::write(dir.path().join("5.jpg"), b"").unwrap();
        assert_eq!(
            unique_rename_target(&file, "jpg"),
            dir.path().join("5 (1).jpg")
        );

        std::fs::write(dir.path().join("5 (1).jpg"), b"").unwrap();
        assert_eq!(
            unique_rename_target(&file, "jpg"),
            dir.path().join("5 (2).jpg")
        );
    }

    /// What is on disk after the wrong-extension rename: the file under its
    /// new name with the same bytes, nothing left under the old one, and a
    /// file that took the prompted name meanwhile still intact.
    #[test]
    fn rename_moves_the_file_and_never_replaces_another() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("5.png");
        std::fs::write(&file, b"jpeg bytes").unwrap();
        // Created after the prompt picked `5.jpg`, before the user said yes.
        let late = dir.path().join("5.jpg");
        std::fs::write(&late, b"someone else's").unwrap();

        let renamed = rename_to_free_name(&file, "jpg").unwrap();

        assert_eq!(renamed, dir.path().join("5 (1).jpg"));
        assert!(!file.exists(), "old name still on disk");
        assert_eq!(std::fs::read(&renamed).unwrap(), b"jpeg bytes");
        assert_eq!(std::fs::read(&late).unwrap(), b"someone else's");
    }

    /// Sends a scratch file to the real Recycle Bin of whoever runs it, so it
    /// is not part of the default run: `cargo test -- --ignored`.
    #[test]
    #[ignore = "writes to the Recycle Bin"]
    fn recycle_removes_the_file_from_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("devoid-viewer recycle test.png");
        std::fs::write(&file, b"x").unwrap();
        recycle(&file).unwrap();
        assert!(!file.exists());
    }

    /// A lone surrogate is a legal Windows file name and not valid Unicode.
    #[cfg(windows)]
    #[test]
    fn rename_target_keeps_a_name_that_is_not_unicode() {
        use std::os::windows::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let stem = std::ffi::OsString::from_wide(&[0xD800, u16::from(b'a')]);
        let mut name = stem.clone();
        name.push(".png");
        let target = unique_rename_target(&dir.path().join(name), "jpg");
        assert_eq!(target.file_stem(), Some(stem.as_os_str()));
        assert_eq!(target.extension(), Some("jpg".as_ref()));
    }

    // ── Conventions ───────────────────────────────────────────────────────────

    /// The installer carries its own copy of the version; it must match Cargo's.
    #[test]
    fn installer_version_matches_cargo() {
        let iss = include_str!("../devoid_viewer_setup.iss");
        let version = iss
            .lines()
            .find_map(|l| l.strip_prefix("#define AppVersion"))
            .map(|rest| rest.trim().trim_matches('"'))
            .expect("#define AppVersion in devoid_viewer_setup.iss");
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }
}
