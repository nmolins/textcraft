#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod menus;
mod net;
mod rename;
mod share;
mod doc;
mod highlight;
mod icon;
mod markdown;
mod session;
mod theme;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontId, TextStyle};

use app::App;

const APP_NAME: &str = "TextCraft";

impl App {
    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot_path.clone() else { return };
        self.frame += 1;
        ctx.request_repaint();
        let frames: u32 = std::env::var("TEXTCRAFT_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        if self.frame >= frames && !self.shot_requested {
            self.shot_requested = true;
            let slot = self.shot_slot.clone();
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                *slot.lock().unwrap() = Some(img);
                c2.request_repaint();
            });
        }
        let shot: Option<Arc<egui::ColorImage>> = self.shot_slot.lock().unwrap().take();
        if let Some(img) = shot {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(dir) = path.parent() {
                if !dir.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(dir);
                }
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                if let Err(e) = buf.save(&path) {
                    eprintln!("screenshot failed: {e}");
                }
            }
            self.allow_quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    /// Test aid for screenshot runs only: TEXTCRAFT_TEST_CLICKS="x,y;x,y" clicks those
    /// points (one every 20 frames from frame 30), e.g. to photograph an open menu.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.shot_path.is_none() {
            return;
        }
        let Ok(spec) = std::env::var("TEXTCRAFT_TEST_CLICKS") else { return };
        for (k, pt) in spec.split(';').enumerate() {
            let at = 30 + 20 * k as u32;
            // "type:some text" types it; "key:enter" presses Enter.
            if let Some(t) = pt.strip_prefix("type:") {
                if self.frame == at {
                    raw.events.push(egui::Event::Text(t.to_string()));
                }
                continue;
            }
            // "press:x,y" / "move:x,y" / "release:x,y": a mouse drag in steps (the button
            // stays down between press and release, so the pointer can be held at an edge).
            for (prefix, pressed) in [("press:", Some(true)), ("move:", None), ("release:", Some(false))] {
                if let Some(rest) = pt.strip_prefix(prefix) {
                    let mut it = rest.split(',').filter_map(|v| v.trim().parse::<f32>().ok());
                    if let (Some(x), Some(y)) = (it.next(), it.next()) {
                        let pos = egui::pos2(x, y);
                        if self.frame >= at && self.frame < at + 20 {
                            raw.events.push(egui::Event::PointerMoved(pos));
                        }
                        if let (true, Some(pressed)) = (self.frame == at, pressed) {
                            raw.events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            });
                        }
                    }
                }
            }
            if pt.starts_with("press:") || pt.starts_with("move:") || pt.starts_with("release:") || pt == "wait" {
                continue;
            }
            if pt == "key:enter" {
                if self.frame == at || self.frame == at + 1 {
                    raw.events.push(egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: self.frame == at,
                        repeat: false,
                        modifiers: Default::default(),
                    });
                }
                continue;
            }
            let mut it = pt.split(',').filter_map(|v| v.trim().parse::<f32>().ok());
            let (Some(x), Some(y)) = (it.next(), it.next()) else { continue };
            let pos = egui::pos2(x, y);
            if self.frame == at || self.frame == at + 1 {
                raw.events.push(egui::Event::PointerMoved(pos));
                raw.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: self.frame == at,
                    modifiers: Default::default(),
                });
            } else if self.frame > at && self.frame < at + 20 {
                raw.events.push(egui::Event::PointerMoved(pos));
            }
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        // Closing keeps everything (even unsaved text) for the next start.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_session();
        }
        // ...and a regular safety save in case the computer stops suddenly.
        let now = ctx.input(|i| i.time);
        if now - self.last_session_time > 3.0 {
            self.last_session_time = now;
            self.save_session();
        }

        let title = {
            let d = &self.docs[self.active];
            format!("{}{} — {APP_NAME}", if d.dirty() { "• " } else { "" }, d.title)
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Files and folders dropped onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for f in dropped {
            if f.is_dir() {
                self.open_folder(f);
            } else {
                self.open_path(&f);
            }
        }

        let size = self.font_size;
        ctx.all_styles_mut(|s| {
            s.text_styles.insert(TextStyle::Monospace, FontId::monospace(size));
        });

        self.shortcuts(ctx);
        self.share_tick(ctx);
        let p = *self.pal();

        let calm = self.distraction; // distraction-free: only the page
        if !calm {
            egui::Panel::top("menu")
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(egui::Margin::symmetric(8, 4)))
                .show(root, |ui| self.menu_bar(ui));
        }

        if self.show_status && !calm {
            egui::Panel::bottom("status")
                .exact_size(26.0)
                .frame(egui::Frame::NONE.fill(p.tab_bar))
                .show(root, |ui| self.status_bar(ui));
        }

        if self.find_open {
            egui::Panel::bottom("find")
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(egui::Margin::symmetric(10, 4)))
                .show(root, |ui| self.find_bar(ui));
        }

        if self.sidebar && !calm {
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(220.0)
                .size_range(140.0..=480.0)
                .frame(egui::Frame::NONE.fill(p.sidebar).inner_margin(egui::Margin::symmetric(10, 0)))
                .show(root, |ui| self.sidebar_ui(ui));
        }

        if self.show_tabs && !calm {
            egui::Panel::top("tabs")
                .exact_size(32.0)
                .frame(egui::Frame::NONE.fill(p.tab_bar))
                .show(root, |ui| self.tab_bar(ui));
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(p.paper)).show(root, |ui| {
            let d = self.doc();
            if d.preview && d.lang == highlight::Lang::Markdown {
                let full = ui.available_width();
                egui::Panel::right("md_preview")
                    .resizable(true)
                    .default_size(full * 0.48)
                    .size_range(220.0..=(full - 220.0).max(240.0))
                    .show_separator_line(false)
                    .frame(egui::Frame::NONE.fill(p.minimap_bg))
                    .show(ui, |ui| self.preview_ui(ui));
            }
            if self.minimap && !calm {
                egui::Panel::right("minimap")
                    .exact_size(96.0)
                    .resizable(false)
                    .show_separator_line(false)
                    .frame(egui::Frame::NONE.fill(p.minimap_bg))
                    .show(ui, |ui| self.minimap_ui(ui));
            }
            if self.doc().remote.is_some() {
                egui::Panel::top("remote_banner").frame(egui::Frame::NONE).show(ui, |ui| self.remote_banner(ui));
            }
            self.editor_ui(ui);
        });

        if calm {
            // A quiet reminder of how to come back, in the corner.
            if let Some((m, until)) = &self.message {
                if now < *until {
                    egui::Area::new(egui::Id::new("calm_hint"))
                        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0))
                        .show(ctx, |ui| ui.label(egui::RichText::new(m).color(p.faint)));
                }
            }
        }
        self.help_windows(ctx);
        self.share_window(ctx);
        self.rename_window(ctx);
        self.palette_ui(ctx);
        self.pending_ui(ctx);
        // Safety net: another computer's tab never changes here.
        for d in self.docs.iter_mut().filter(|d| d.remote.is_some()) {
            if d.text != d.saved {
                d.text = d.saved.clone();
            }
        }
        self.screenshot_step(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let mut shot: Option<PathBuf> = None;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--screenshot" {
            shot = args.next().map(PathBuf::from);
        } else if a == "--write-icon" {
            // Used by the Linux install script: writes the app icon as a PNG and quits.
            let path = args.next().unwrap_or_else(|| "textcraft.png".into());
            let img = image::RgbaImage::from_raw(256, 256, icon::icon_rgba(256)).expect("icon size");
            if let Err(e) = img.save(&path) {
                eprintln!("icône non écrite : {e}");
                std::process::exit(1);
            }
            return Ok(());
        } else if !a.starts_with("--") {
            files.push(PathBuf::from(a));
        }
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size([1200.0, 760.0])
        .with_min_inner_size([520.0, 320.0])
        .with_drag_and_drop(true)
        .with_app_id("textcraft")
        .with_icon(std::sync::Arc::new(egui::IconData { rgba: icon::icon_rgba(64), width: 64, height: 64 }));
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        options.event_loop_builder = Some(Box::new(|b| {
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native(APP_NAME, options, Box::new(move |cc| Ok(Box::new(App::new(cc, files, shot)))))
}
