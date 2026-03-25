//! Devoid Viewer — main.rs

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod loader;
mod nav;

use std::{collections::HashMap, path::PathBuf};

use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use loader::load_image;
use nav::DirNavigator;

// ── Message types ─────────────────────────────────────────────────────────────

#[derive(Debug)]
enum LoadMsg {
    Ready { path: PathBuf, image: ColorImage, is_preload: bool },
    Error { path: PathBuf, msg: String },
}

// ── App state ─────────────────────────────────────────────────────────────────

struct DevoidApp {
    current_path:       Option<PathBuf>,
    texture:            Option<TextureHandle>,
    image_size:         Option<(u32, u32)>,
    error_msg:          Option<String>,

    navigator:          DirNavigator,

    load_tx:            Sender<LoadMsg>,
    load_rx:            Receiver<LoadMsg>,
    preload_cache:      HashMap<PathBuf, ColorImage>,

    show_delete_dialog: bool,
    loading:            bool,
    loading_path:       Option<PathBuf>,

    /// Some((w,h)) → we need to emit a resize command this frame.
    pending_resize:     Option<(u32, u32)>,
    /// True on the frame after a resize, when we send the centre command.
    pending_center:     bool,
}

impl DevoidApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        let (load_tx, load_rx) = bounded::<LoadMsg>(32);

        let mut app = DevoidApp {
            current_path:       None,
            texture:            None,
            image_size:         None,
            error_msg:          None,
            navigator:          DirNavigator::default(),
            load_tx,
            load_rx,
            preload_cache:      HashMap::new(),
            show_delete_dialog: false,
            loading:            false,
            loading_path:       None,
            pending_resize:     None,
            pending_center:      false,
        };

        if let Some(path) = initial_path {
            app.open_image(path, true, &cc.egui_ctx);
        }

        app
    }

    // ── Image loading ─────────────────────────────────────────────────────────

    /// Begin loading `path`.  `resize_window` = true only for the very first image.
    fn open_image(&mut self, path: PathBuf, resize_window: bool, ctx: &egui::Context) {
        if let Some(img) = self.preload_cache.remove(&path) {
            self.apply_image(path, img, resize_window, ctx);
            self.preload_adjacent(ctx);
            return;
        }

        self.loading      = true;
        self.loading_path = Some(path.clone());
        self.error_msg    = None;
        self.navigator.set_current(path.clone());

        let tx   = self.load_tx.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let msg = match load_image(&path) {
                Ok(img) => LoadMsg::Ready { path, image: img, is_preload: false },
                Err(e)  => LoadMsg::Error { path, msg: e.to_string() },
            };
            let _ = tx.send(msg);
            ctx2.request_repaint();
        });
    }

    /// Upload a decoded image as a GPU texture and update all related state.
    fn apply_image(
        &mut self,
        path: PathBuf,
        img: ColorImage,
        resize_window: bool,
        ctx: &egui::Context,
    ) {
        let w = img.width()  as u32;
        let h = img.height() as u32;

        self.texture      = Some(ctx.load_texture(
            path.to_string_lossy().as_ref(),
            img,
            TextureOptions::LINEAR,
        ));
        self.image_size   = Some((w, h));
        self.current_path = Some(path.clone());
        self.loading      = false;
        self.loading_path = None;
        self.error_msg    = None;
        self.navigator.set_current(path);

        if resize_window {
            self.pending_resize = Some((w, h));
        }
    }

    fn clear_image(&mut self) {
        self.current_path = None;
        self.texture      = None;
        self.image_size   = None;
        self.error_msg    = None;
    }

    fn preload_adjacent(&mut self, ctx: &egui::Context) {
        for path in [self.navigator.peek_next(), self.navigator.peek_prev()]
            .into_iter()
            .flatten()
        {
            if self.preload_cache.contains_key(&path) { continue; }
            let tx   = self.load_tx.clone();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                if let Ok(img) = load_image(&path) {
                    let _ = tx.send(LoadMsg::Ready { path, image: img, is_preload: true });
                    ctx2.request_repaint();
                }
            });
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    fn navigate_next(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.navigator.next() { self.open_image(p, false, ctx); }
    }

    fn navigate_prev(&mut self, ctx: &egui::Context) {
        if let Some(p) = self.navigator.prev() { self.open_image(p, false, ctx); }
    }

    // ── Deletion ──────────────────────────────────────────────────────────────

    fn confirm_delete(&mut self, ctx: &egui::Context) {
        let Some(path) = self.current_path.clone() else { return };
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.navigator.remove(&path);
                self.preload_cache.clear();
                match self.navigator.current().cloned() {
                    Some(next) => self.open_image(next, false, ctx),
                    None       => self.clear_image(),
                }
            }
            Err(e) => self.error_msg = Some(format!("Delete failed: {e}")),
        }
    }

    // ── Drawing helpers ───────────────────────────────────────────────────────

    fn draw_blank(ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter_at(rect);
        let cell    = 32.0_f32;
        let cols    = (rect.width()  / cell).ceil() as i32;
        let rows    = (rect.height() / cell).ceil() as i32;
        let dark    = egui::Color32::from_gray(36);
        let light   = egui::Color32::from_gray(48);
        for row in 0..rows {
            for col in 0..cols {
                let color = if (row + col) % 2 == 0 { light } else { dark };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.left() + col as f32 * cell,
                                   rect.top()  + row as f32 * cell),
                        egui::vec2(cell, cell),
                    ),
                    0.0, color,
                );
            }
        }
        painter.text(
            rect.center(), egui::Align2::CENTER_CENTER,
            "Drop an image here",
            egui::FontId::proportional(18.0),
            egui::Color32::from_gray(100),
        );
    }

    /// Scale the image to fill `avail` as much as possible while preserving
    /// aspect ratio.  Strategy: try fitting by height first; if the resulting
    /// width still exceeds the panel, fit by width instead.  This means images
    /// always fill the window in the dominant dimension.
    fn draw_image(ui: &mut egui::Ui, tex: &TextureHandle, size: (u32, u32), avail: egui::Rect) {
        let (orig_w, orig_h) = (size.0 as f32, size.1 as f32);
        let panel_w = avail.width();
        let panel_h = avail.height();

        // Scale so the image fills the panel in both axes (fit, not crop).
        let scale_h = panel_h / orig_h;               // height-fit scale
        let scale_w = panel_w / orig_w;               // width-fit scale
        let scale   = scale_h.min(scale_w);           // whichever fits both

        let display = egui::vec2(orig_w * scale, orig_h * scale);
        let offset  = egui::vec2(
            ((panel_w - display.x) * 0.5).max(0.0),
            ((panel_h - display.y) * 0.5).max(0.0),
        );
        let img_rect = egui::Rect::from_min_size(avail.min + offset, display);
        let uv       = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(tex.id(), img_rect, uv, egui::Color32::WHITE);
    }

    fn draw_status_bar(
        ui:    &mut egui::Ui,
        path:  &std::path::Path,
        index: usize,
        total: usize,
        size:  (u32, u32),
        avail: egui::Rect,
    ) {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = format!("{}  [{}/{}]  {}×{}", name, index + 1, total, size.0, size.1);
        ui.painter().text(
            egui::pos2(avail.left() + 8.0, avail.bottom() - 18.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            egui::FontId::proportional(12.0),
            egui::Color32::from_gray(180),
        );
    }

    fn draw_delete_dialog(&mut self, ctx: &egui::Context) {
        let file_name = self.current_path.as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mut do_delete = false;
        let mut do_cancel = false;

        egui::Window::new("Delete Image?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Permanently delete \"{file_name}\"?"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() { do_delete = true; }
                    if ui.button("Cancel").clicked() { do_cancel = true; }
                });
            });

        if do_delete { self.show_delete_dialog = false; self.confirm_delete(ctx); }
        if do_cancel { self.show_delete_dialog = false; }
    }
}

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct InputActions {
    nav_next:    bool,
    nav_prev:    bool,
    open_delete: bool,
    quit:        bool,
    drop_path:   Option<PathBuf>,
}

impl DevoidApp {
    fn read_input(&self, ctx: &egui::Context) -> InputActions {
        let mut a = InputActions::default();
        ctx.input(|i| {
            use egui::Key;
            a.nav_next    = i.key_pressed(Key::ArrowRight)
                          || i.key_pressed(Key::ArrowDown)
                          || i.key_pressed(Key::PageDown);
            a.nav_prev    = i.key_pressed(Key::ArrowLeft)
                          || i.key_pressed(Key::ArrowUp)
                          || i.key_pressed(Key::PageUp);
            a.open_delete = i.key_pressed(Key::Delete);
            a.quit        = i.key_pressed(Key::Q) || i.key_pressed(Key::Escape);
            a.drop_path   = i.raw.dropped_files.iter()
                .find_map(|f| f.path.as_ref().filter(|p| loader::is_supported(p)).cloned());
        });
        a
    }

    fn handle_input(&mut self, a: InputActions, ctx: &egui::Context) {
        if let Some(path) = a.drop_path {
            self.open_image(path, false, ctx);
            return;
        }
        if a.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if !self.show_delete_dialog {
            if a.nav_next { self.navigate_next(ctx); }
            if a.nav_prev { self.navigate_prev(ctx); }
        }
        if a.open_delete && self.current_path.is_some() {
            self.show_delete_dialog = true;
        }
    }

    fn process_load_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.load_rx.try_recv() {
            match msg {
                LoadMsg::Ready { path, image, is_preload: true } => {
                    self.preload_cache.insert(path, image);
                }
                LoadMsg::Ready { path, image, is_preload: false } => {
                    if self.loading_path.as_deref() == Some(path.as_path()) {
                        let is_first = self.texture.is_none();
                        self.apply_image(path, image, is_first, ctx);
                        self.preload_adjacent(ctx);
                    }
                }
                LoadMsg::Error { path, msg } => {
                    if self.loading_path.as_deref() == Some(path.as_path()) {
                        self.loading      = false;
                        self.loading_path = None;
                        self.error_msg    = Some(msg);
                    }
                }
            }
        }
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────

impl eframe::App for DevoidApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_load_messages(ctx);

        // Frame N: send the resize; frame N+1: centre once the new size has landed.
        if let Some((img_w, img_h)) = self.pending_resize.take() {
            let (max_w, max_h) = monitor_work_area(ctx);
            // Reserve room for the taskbar (bottom) and title bar (top) so the
            // window chrome is always reachable: 90% of width, 85% of height.
            let usable_w = max_w * 0.90;
            let usable_h = max_h * 0.85;
            let scale = (usable_w / img_w as f32).min(usable_h / img_h as f32).min(1.0);
            let win_w = (img_w as f32 * scale).floor();
            let win_h = (img_h as f32 * scale).floor();
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(win_w, win_h)));
            self.pending_center = true;   // centre on the next frame
            ctx.request_repaint();
        } else if self.pending_center {
            // The resize has been applied; now we can centre accurately.
            if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                ctx.send_viewport_cmd(cmd);
            }
            self.pending_center = false;
        }

        let input = self.read_input(ctx);
        self.handle_input(input, ctx);

        if self.show_delete_dialog {
            self.draw_delete_dialog(ctx);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_gray(24)))
            .show(ctx, |ui| {
                let avail = ui.available_rect_before_wrap();

                if self.loading {
                    ui.centered_and_justified(|ui| { ui.spinner(); });
                    return;
                }

                if let Some(err) = self.error_msg.as_deref() {
                    let err = err.to_owned();
                    ui.centered_and_justified(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), &err);
                    });
                    return;
                }

                if let (Some(tex), Some(size), Some(path)) = (
                    &self.texture,
                    self.image_size,
                    self.current_path.as_deref(),
                ) {
                    Self::draw_image(ui, tex, size, avail);
                    Self::draw_status_bar(
                        ui, path,
                        self.navigator.current_index(),
                        self.navigator.total(),
                        size, avail,
                    );
                } else {
                    Self::draw_blank(ui, avail);
                }
            });

        if self.loading {
            ctx.request_repaint();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> eframe::Result<()> {
    // Canonicalize the path so "Open With" short/relative paths resolve correctly.
    let initial_path = std::env::args().nth(1)
        .map(PathBuf::from)
        .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)));

    // Best-effort size hint for the initial window; clamping to the work area
    // happens inside `update` once egui knows the monitor geometry.
    let (w, h) = initial_path.as_deref()
        .and_then(image_size_hint)
        .unwrap_or((800, 600));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([w as f32, h as f32])
            .with_min_inner_size([120.0, 80.0])
            .with_title("Devoid Viewer")
            .with_drag_and_drop(true),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Devoid Viewer",
        native_options,
        Box::new(move |cc| Box::new(DevoidApp::new(cc, initial_path)) as Box<dyn eframe::App>),
    )
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Return the usable screen dimensions (work area, excluding taskbar) in
/// logical pixels.  Falls back to a conservative 1280×720 if unavailable.
fn monitor_work_area(ctx: &egui::Context) -> (f32, f32) {
    ctx.input(|i| {
        i.viewport()
            .monitor_size
            .map(|s| (s.x, s.y))
            .unwrap_or((1280.0, 720.0))
    })
}

/// Fast image-dimension probe — reads only the file header, no full decode.
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
