use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, CornerRadius, FontId, Id, Key, Layout, Modifiers, RichText, ScrollArea, Sense, Shape,
    Stroke, TextEdit, TextStyle, Vec2,
};

use crate::app::*;
use crate::menus::palette_entries;
use crate::doc::*;
use crate::highlight::{HighlightCache, Lang};
use crate::theme::Palette;

const MAX_HITS: usize = 500;

#[derive(Clone)]
enum Act {
    Run(Cmd),
    Open(PathBuf),
    OpenAt(PathBuf, usize),
    Line(usize),
}

fn rank(q: &str, cands: Vec<(String, String, Act)>, key: impl Fn(&(String, String, Act)) -> String) -> Vec<(String, String, Act)> {
    let mut scored: Vec<(i32, usize)> =
        cands.iter().enumerate().filter_map(|(i, c)| fuzzy_score(q, &key(c)).map(|s| (s, i))).collect();
    if !q.is_empty() {
        scored.sort_by(|a, b| b.0.cmp(&a.0));
    }
    let mut cands: Vec<Option<(String, String, Act)>> = cands.into_iter().map(Some).collect();
    scored.into_iter().take(80).filter_map(|(_, i)| cands[i].take()).collect()
}

/// Definitions worth jumping to: functions, types, headings.
fn symbols(d: &Doc) -> Vec<(usize, String)> {
    const STARTS: [&str; 22] = [
        "fn ", "pub fn ", "pub(crate) fn ", "async fn ", "pub async fn ", "struct ", "pub struct ", "enum ",
        "pub enum ", "trait ", "pub trait ", "impl", "mod ", "pub mod ", "def ", "async def ", "class ",
        "function ", "export function ", "export default function ", "export class ", "interface ",
    ];
    let mut out = Vec::new();
    for (i, l) in d.text.split('\n').enumerate() {
        let t = l.trim();
        let hit = match d.lang {
            Lang::Markdown => t.starts_with('#'),
            Lang::Css => !l.starts_with(' ') && !l.starts_with('\t') && t.ends_with('{'),
            Lang::Toml => t.starts_with('['),
            _ => STARTS.iter().any(|s| t.starts_with(s)),
        };
        if hit {
            let label = t.trim_end_matches('{').trim_end_matches(':').trim().to_string();
            out.push((i, label));
        }
    }
    out
}

fn section_title(ui: &mut egui::Ui, p: &Palette, t: &str) {
    ui.label(RichText::new(t).size(11.0).color(p.faint).strong());
    ui.add_space(2.0);
}

fn tree_ui(ui: &mut egui::Ui, nodes: &[Node], open: &mut Option<PathBuf>, current: Option<&Path>) {
    for n in nodes {
        match &n.children {
            Some(children) => {
                egui::CollapsingHeader::new(format!("📁 {}", n.name))
                    .id_salt(&n.path)
                    .default_open(false)
                    .show(ui, |ui| tree_ui(ui, children, open, current));
            }
            None => {
                let sel = current == Some(n.path.as_path());
                if ui.selectable_label(sel, RichText::new(format!("📄 {}", n.name)).size(13.0)).clicked() {
                    *open = Some(n.path.clone());
                }
            }
        }
    }
}

impl App {
    /// Live Markdown preview, beside the text (like the MarkdownLivePreview add-on, built in).
    pub fn preview_ui(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let mut close = false;
        // header strip
        egui::Frame::NONE.fill(p.tab_bar).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Aperçu — {}", self.doc().title)).size(13.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Fermer l'aperçu (Alt+M)").clicked() {
                        close = true;
                    }
                });
            });
        });
        // left edge of the page
        let r = ui.available_rect_before_wrap();
        ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, p.ruled));

        let d = &self.docs[self.active];
        let blocks = crate::markdown::parse(&d.text);
        let base = d.path.as_ref().and_then(|x| x.parent().map(|x| x.to_path_buf()));
        let editor_max = (d.content_h - d.view_h).max(1.0);
        let frac = (d.scroll_y / editor_max).clamp(0.0, 1.0);
        let editor_moved = (d.scroll_y - self.preview_last_scroll).abs() > 0.5;
        self.preview_last_scroll = d.scroll_y;

        let mut sa = ScrollArea::vertical().id_salt(("md_preview", d.id)).auto_shrink([false, false]);
        if editor_moved {
            // follow the text: same place, proportionally
            sa = sa.vertical_scroll_offset(frac * self.preview_max);
        }
        let mut c = crate::markdown::Ctx {
            p: &p,
            base: base.as_deref(),
            size: 15.0 * self.font_size / 14.0,
            images: &mut self.md_images,
            action: None,
            quote: false,
            code_id: 0,
            fonts: self.md_fonts,
        };
        let page_h = ui.available_height();
        let out = sa.show(ui, |ui| {
            egui::Frame::NONE
                .fill(p.paper)
                .inner_margin(egui::Margin { left: 28, right: 28, top: 22, bottom: 40 })
                .show(ui, |ui| {
                    ui.set_min_height((page_h - 62.0).max(0.0));
                    ui.set_width(ui.available_width().min(820.0));
                    crate::markdown::show(ui, &blocks, &mut c);
                    ui.allocate_space(Vec2::new(ui.available_width(), 0.0));
                });
        });
        self.preview_max = (out.content_size.y - out.inner_rect.height()).max(0.0);
        let action = c.action.take();
        if close {
            self.docs[self.active].preview = false;
        }
        if let Some(crate::markdown::PreviewAction::OpenFile(path)) = action {
            if path.exists() {
                self.open_path(&path);
            } else {
                let ctx = ui.ctx().clone();
                self.flash(&ctx, "Ce lien ne mène à aucun fichier");
            }
        }
    }

    pub fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let mut activate = None;
        let mut close = None;
        let mut new_tab = false;
        let mut rename = None;
        ScrollArea::horizontal()
            .id_salt("tabs")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    ui.add_space(6.0);
                    for (i, d) in self.docs.iter().enumerate() {
                        let active = i == self.active;
                        let color = if active { p.ink } else { p.faint };
                        let galley = ui.painter().layout_no_wrap(d.title.clone(), FontId::proportional(13.5), color);
                        let w = (galley.size().x + 44.0).max(90.0);
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
                        let fill = if active {
                            p.paper
                        } else if resp.hovered() {
                            p.selection
                        } else {
                            p.tab_idle
                        };
                        let painter = ui.painter();
                        let body = rect.with_min_y(rect.min.y + 4.0);
                        painter.rect_filled(body, CornerRadius { nw: 9, ne: 9, sw: 0, se: 0 }, fill);
                        if active {
                            painter.line_segment(
                                [body.left_top() + Vec2::new(8.0, 1.0), body.right_top() + Vec2::new(-8.0, 1.0)],
                                Stroke::new(2.5, p.accent),
                            );
                        }
                        let gh = galley.size().y;
                        painter.galley(body.left_center() + Vec2::new(14.0, -gh / 2.0), galley, p.ink);
                        let xc = egui::pos2(body.right() - 16.0, body.center().y);
                        let close_rect = egui::Rect::from_center_size(xc, Vec2::splat(16.0));
                        let over_close = resp.hover_pos().map_or(false, |pos| close_rect.contains(pos));
                        if d.dirty() && !over_close {
                            painter.circle_filled(xc, 4.0, p.accent);
                        } else if resp.hovered() || active {
                            if over_close {
                                painter.circle_filled(xc, 8.0, p.tab_idle);
                            }
                            let s = Stroke::new(1.4, if over_close { p.accent } else { p.faint });
                            painter.line_segment([xc + Vec2::new(-3.5, -3.5), xc + Vec2::new(3.5, 3.5)], s);
                            painter.line_segment([xc + Vec2::new(3.5, -3.5), xc + Vec2::new(-3.5, 3.5)], s);
                        }
                        if resp.clicked() {
                            if over_close {
                                close = Some(i);
                            } else {
                                activate = Some(i);
                            }
                        }
                        if resp.middle_clicked() {
                            close = Some(i);
                        }
                        if resp.double_clicked() && !over_close {
                            rename = Some(i);
                        }
                        resp.context_menu(|ui| {
                            if ui.add_enabled(d.remote.is_none(), egui::Button::new("Renommer…")).clicked() {
                                rename = Some(i);
                                ui.close();
                            }
                            if ui.button("Fermer").clicked() {
                                close = Some(i);
                                ui.close();
                            }
                        });
                        let hint = d
                            .path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "Pas encore enregistré".into());
                        resp.on_hover_text(hint);
                    }
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(28.0, 32.0), Sense::click());
                    let col = if resp.hovered() { p.accent } else { p.faint };
                    ui.painter().text(r.center() + Vec2::new(0.0, 2.0), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), col);
                    if resp.on_hover_text("Nouveau fichier").clicked() {
                        new_tab = true;
                    }
                });
            });
        let ctx = ui.ctx().clone();
        if new_tab {
            self.run(&ctx, Cmd::NewFile);
        }
        if let Some(i) = rename {
            self.active = i;
            self.run(&ctx, Cmd::RenameTab);
        }
        if let Some(i) = activate {
            self.active = i;
            self.focus_editor(&ctx);
        }
        if let Some(i) = close {
            self.request_close(i);
        }
    }

    pub fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let mut open: Option<PathBuf> = None;
        let mut want_folder = false;
        let mut remote_pick = None;
        let mut activate = None;
        let mut close = None;
        ui.add_space(10.0);
        ScrollArea::vertical().id_salt("sidebar").auto_shrink([false, false]).show(ui, |ui| {
            section_title(ui, &p, "FICHIERS OUVERTS");
            for (i, d) in self.docs.iter().enumerate() {
                ui.horizontal(|ui| {
                    let label = if d.dirty() { format!("● {}", d.title) } else { d.title.clone() };
                    let resp = ui
                        .selectable_label(i == self.active, RichText::new(label).size(13.0))
                        .on_hover_text("Double-clic pour renommer");
                    if resp.clicked() {
                        activate = Some(i);
                    }
                    if resp.double_clicked() && d.remote.is_none() {
                        ui.ctx().data_mut(|m| m.insert_temp(egui::Id::new("textcraft_rename"), i));
                    }
                    if resp.hovered() || i == self.active {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Fermer").clicked() {
                                close = Some(i);
                            }
                        });
                    }
                });
            }
            ui.add_space(14.0);
            section_title(ui, &p, "DOSSIER");
            if let Some(folder) = &self.folder {
                let name = folder
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| folder.display().to_string());
                let current = self.docs.get(self.active).and_then(|d| d.path.as_deref());
                egui::CollapsingHeader::new(RichText::new(format!("📁 {name}")).strong())
                    .id_salt("root")
                    .default_open(true)
                    .show(ui, |ui| tree_ui(ui, &self.tree, &mut open, current));
            } else {
                ui.label(RichText::new("Aucun dossier ouvert.").color(p.faint).italics());
                ui.add_space(4.0);
                if ui.button("Ouvrir un dossier…").clicked() {
                    want_folder = true;
                }
            }
            remote_pick = self.share_sidebar(ui);
        });
        let ctx = ui.ctx().clone();
        if let Some(pick) = remote_pick {
            self.open_remote(&ctx, pick);
        }
        if let Some(i) = activate {
            self.active = i;
            self.focus_editor(&ctx);
        }
        if let Some(i) = close {
            self.request_close(i);
        }
        if want_folder {
            self.run(&ctx, Cmd::OpenFolder);
        }
        if let Some(path) = open {
            self.open_path(&path);
            self.focus_editor(&ctx);
        }
    }

    pub fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let now = ui.input(|i| i.time);
        let doc = &self.docs[self.active];
        let mut new_lang = None;
        let mut chosen: Option<Cmd> = None;
        let tab_size = self.tab_size;
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.add_space(6.0);
            let mut txt = format!("Ligne {}, Colonne {}", doc.line + 1, doc.col + 1);
            if doc.selected > 0 {
                txt.push_str(&format!("  ({} sélectionné(s))", doc.selected));
            }
            ui.label(RichText::new(txt).size(12.5));
            if let Some((m, until)) = &self.message {
                if now < *until {
                    ui.label(RichText::new(m).size(12.5).color(p.accent));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(6.0);
                ui.menu_button(RichText::new(doc.lang.name()).size(12.5), |ui| {
                    for l in Lang::ALL {
                        if ui.selectable_label(l == doc.lang, l.name()).clicked() {
                            new_lang = Some(l);
                            ui.close();
                        }
                    }
                });
                if doc.lang == Lang::Markdown {
                    let on = doc.preview;
                    let txt = RichText::new("Aperçu").size(12.5).color(if on { p.accent } else { p.faint });
                    if ui.selectable_label(on, txt).on_hover_text("Aperçu Markdown en direct (Alt+M)").clicked() {
                        chosen = Some(Cmd::TogglePreview);
                    }
                }
                let endings = if doc.crlf { "Windows (CRLF)" } else { "Unix (LF)" };
                ui.menu_button(RichText::new(endings).size(12.5).color(p.faint), |ui| {
                    if ui.selectable_label(doc.crlf, "Windows (CRLF)").clicked() {
                        chosen = Some(Cmd::LineEndings(true));
                        ui.close();
                    }
                    if ui.selectable_label(!doc.crlf, "Unix (LF)").clicked() {
                        chosen = Some(Cmd::LineEndings(false));
                        ui.close();
                    }
                });
                ui.label(RichText::new("UTF-8").size(12.5).color(p.faint));
                ui.menu_button(RichText::new(format!("Tabulation : {tab_size}")).size(12.5).color(p.faint), |ui| {
                    for n in [2u8, 3, 4, 8] {
                        if ui.selectable_label(n == tab_size, format!("Largeur de tabulation : {n}")).clicked() {
                            chosen = Some(Cmd::TabSize(n));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Convertir l'indentation en espaces").clicked() {
                        chosen = Some(Cmd::ConvertIndent);
                        ui.close();
                    }
                });
                ui.label(RichText::new(format!("{} lignes", doc.line_count())).size(12.5).color(p.faint));
            });
        });
        if let Some(l) = new_lang {
            self.docs[self.active].lang = l;
        }
        if let Some(c) = chosen {
            let ctx = ui.ctx().clone();
            self.run(&ctx, c);
        }
    }

    pub fn find_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let p = *self.pal();
        let mut close = false;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Chercher").color(p.faint));
            let resp = ui.add(TextEdit::singleline(&mut self.find_query).desired_width(260.0).hint_text("texte à chercher"));
            if self.find_focus {
                resp.request_focus();
                self.find_focus = false;
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let back = ui.input(|i| i.modifiers.shift);
                if !self.find_next(&ctx, back) {
                    self.flash(&ctx, "Introuvable");
                }
                resp.request_focus();
            }
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
            ui.toggle_value(&mut self.match_case, "Aa").on_hover_text("Respecter la casse");
            if ui.button("◀").on_hover_text("Précédent (Maj+Entrée)").clicked() {
                self.find_next(&ctx, true);
            }
            if ui.button("▶").on_hover_text("Suivant (Entrée)").clicked() {
                self.find_next(&ctx, false);
            }
            let n = self.count_matches();
            let txt = if self.find_query.is_empty() { String::new() } else { format!("{n} trouvé(s)") };
            ui.label(RichText::new(txt).color(if n == 0 { p.accent } else { p.faint }));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("✕").on_hover_text("Fermer (Échap)").clicked() {
                    close = true;
                }
                if !self.replace_open && ui.button("Remplacer…").clicked() {
                    self.replace_open = true;
                }
            });
        });
        if self.replace_open {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Par       ").color(p.faint));
                ui.add(TextEdit::singleline(&mut self.replace_with).desired_width(260.0).hint_text("remplacement"));
                if ui.button("Remplacer").clicked() {
                    self.replace_one(&ctx);
                }
                if ui.button("Tout remplacer").clicked() {
                    self.replace_all(&ctx);
                }
            });
        }
        ui.add_space(4.0);
        if close {
            self.find_open = false;
            self.focus_editor(&ctx);
        }
    }

    pub fn minimap_ui(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let doc = &mut self.docs[self.active];
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, p.minimap_bg);
        painter.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, p.ruled));
        let lh = 2.5;
        let total = doc.line_count() as f32 * lh;
        let max_scroll = (doc.content_h - doc.view_h).max(1.0);
        let frac = (doc.scroll_y / max_scroll).clamp(0.0, 1.0);
        let off = if total > rect.height() - 12.0 { frac * (total - rect.height() + 12.0) } else { 0.0 };
        let first = (off / lh) as usize;
        let count = (rect.height() / lh) as usize + 2;
        let tok = doc.lang.line_comment().unwrap_or("\u{1}");
        for (k, line) in doc.text.split('\n').skip(first).take(count).enumerate() {
            let t = line.trim_start();
            if t.is_empty() {
                continue;
            }
            let indent = (line.len() - t.len()) as f32;
            let y = rect.top() + 6.0 + (first + k) as f32 * lh - off;
            let x0 = rect.left() + 8.0 + indent * 1.1;
            let x1 = (x0 + t.chars().count() as f32 * 1.1).min(rect.right() - 6.0);
            let heading = ["fn ", "def ", "function", "class ", "struct ", "impl ", "pub fn "].iter().any(|k| t.starts_with(k))
                || (t.starts_with('#') && doc.lang == Lang::Markdown);
            let color = if t.starts_with(tok) || t.starts_with("/*") || t.starts_with('*') {
                p.comment.gamma_multiply(0.7)
            } else if heading {
                p.keyword.gamma_multiply(0.8)
            } else {
                p.ink.gamma_multiply(0.45)
            };
            if x1 > x0 {
                painter.line_segment([egui::pos2(x0, y + 1.0), egui::pos2(x1, y + 1.0)], Stroke::new(1.6, color));
            }
        }
        let row_h = doc.row_h.max(1.0);
        let vtop = rect.top() + 6.0 + doc.scroll_y / row_h * lh - off;
        let vh = (doc.view_h / row_h * lh).max(12.0);
        let view = egui::Rect::from_min_size(egui::pos2(rect.left() + 3.0, vtop - 2.0), Vec2::new(rect.width() - 6.0, vh));
        painter.rect(
            view,
            CornerRadius::same(4),
            p.accent.gamma_multiply(if resp.hovered() { 0.16 } else { 0.10 }),
            Stroke::new(1.0, p.accent.gamma_multiply(0.5)),
            egui::StrokeKind::Inside,
        );
        if resp.clicked() || resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let line = (pos.y - rect.top() - 6.0 + off) / lh;
                doc.set_scroll = Some((line * row_h - doc.view_h / 2.0).clamp(0.0, max_scroll));
            }
        }
    }

    pub fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let dark = self.dark;
        let size = self.font_size;
        let ruled = self.ruled;
        let wrap = self.wrap;
        let ruler = self.ruler;
        let doc = &mut self.docs[self.active];
        let id = editor_id(doc.id);
        let lang = doc.lang;
        let n_lines = doc.line_count();
        let digits = n_lines.to_string().len().max(3) as f32;
        let gutter_w = digits * size * 0.62 + 26.0;

        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = ui
                .ctx()
                .memory_mut(|m| m.caches.cache::<HighlightCache>().get((dark, buf.as_str(), lang, size.to_bits())).clone());
            job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
            ui.painter().layout_job(job)
        };

        let mut sa = if wrap { ScrollArea::vertical() } else { ScrollArea::both() }
            .id_salt(("scroll", doc.id))
            .auto_shrink([false, false]);
        if let Some(y) = doc.set_scroll.take() {
            sa = sa.vertical_scroll_offset(y.max(0.0));
        }
        let view_h = ui.available_height();
        let out = sa.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let gutter_h = view_h;
                let (gutter, _) = ui.allocate_exact_size(Vec2::new(gutter_w, gutter_h), Sense::hover());
                let bg = ui.painter().add(Shape::Noop);
                // Another computer's tab: selectable and copyable, never editable.
                let ro_text: Option<String> = doc.remote.as_ref().map(|_| doc.text.clone());
                let mut ro_ref: &str = ro_text.as_deref().unwrap_or("");
                let buffer: &mut dyn egui::TextBuffer = if ro_text.is_some() { &mut ro_ref } else { &mut doc.text };
                let output = TextEdit::multiline(buffer)
                    .id(id)
                    .font(TextStyle::Monospace)
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .min_size(Vec2::new(0.0, view_h))
                    .margin(egui::Margin::symmetric(10, 4))
                    .layouter(&mut layouter)
                    .show(ui);

                // Draw our own ink-coloured text pointer: the system one can be
                // white and disappears on the cream paper.
                let over_text = output.response.contains_pointer() || output.response.dragged();
                if let (true, Some(pos)) = (over_text, ui.input(|i| i.pointer.hover_pos())) {
                    if ui.clip_rect().contains(pos) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                        let painter = ui
                            .ctx()
                            .layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("ink_ibeam")));
                        let h = (doc.row_h * 0.6).max(9.0);
                        let w = 4.0;
                        let pos = egui::pos2(pos.x.round() + 0.5, pos.y.round());
                        let segs = [
                            [pos + Vec2::new(0.0, -h), pos + Vec2::new(0.0, h)],
                            [pos + Vec2::new(-w, -h), pos + Vec2::new(w, -h)],
                            [pos + Vec2::new(-w, h), pos + Vec2::new(w, h)],
                        ];
                        let (ink, halo) = if p.dark { (p.ink, p.tab_bar) } else { (p.ink, p.paper) };
                        for s in segs {
                            painter.line_segment(s, Stroke::new(4.0, halo));
                        }
                        for s in segs {
                            painter.line_segment(s, Stroke::new(2.0, ink));
                        }
                    }
                }

                if let Some(r) = output.galley.rows.first() {
                    let rh = r.rect().height();
                    if rh > 4.0 {
                        doc.row_h = rh;
                    }
                }
                if let Some(cr) = output.cursor_range {
                    let pi = cr.primary.index.0;
                    let si = cr.secondary.index.0;
                    doc.line = line_of(&doc.text, pi);
                    doc.col = col_of(&doc.text, pi);
                    doc.selected = pi.abs_diff(si);
                }

                let clip = ui.clip_rect();
                let top = output.galley_pos.y;
                let rh = doc.row_h;
                let first = ((clip.top() - top) / rh).floor().max(0.0) as usize;
                let last = ((clip.bottom() - top) / rh).ceil().max(0.0) as usize + 1;
                let right = clip.right().max(output.response.rect.right());
                let mut shapes = Vec::new();
                // Where each logical line starts (rows differ from lines when wrapping).
                let mut starts: Vec<(usize, f32)> = Vec::new(); // (line, y) of visible line starts
                let (mut cur_top, mut cur_bot) = (f32::MAX, f32::MIN);
                let mut line = 0usize;
                let mut new_line = true;
                for row in output.galley.rows.iter() {
                    let y0 = top + row.pos.y;
                    if line == doc.line {
                        cur_top = cur_top.min(y0);
                        cur_bot = cur_bot.max(y0 + rh);
                    }
                    if new_line && y0 + rh >= clip.top() && y0 <= clip.bottom() {
                        starts.push((line, y0));
                    }
                    new_line = row.ends_with_newline;
                    if row.ends_with_newline {
                        line += 1;
                    }
                    if y0 > clip.bottom() && line > doc.line {
                        break;
                    }
                }
                // current line band
                let has_focus = output.response.has_focus();
                if cur_top < cur_bot {
                    let band = if has_focus { p.current_line } else { p.current_line.gamma_multiply(0.6) };
                    shapes.push(Shape::rect_filled(egui::Rect::from_x_y_ranges(gutter.left()..=right, cur_top..=cur_bot), 0.0, band));
                }
                // ruler at a fixed column
                if ruler > 0 {
                    let cw = ui.painter().layout_no_wrap("0".repeat(10), FontId::monospace(size), p.ink).size().x / 10.0;
                    let x = output.galley_pos.x + ruler as f32 * cw;
                    shapes.push(Shape::line_segment(
                        [egui::pos2(x, clip.top()), egui::pos2(x, clip.bottom())],
                        Stroke::new(1.0, p.accent.gamma_multiply(0.35)),
                    ));
                }
                if ruled {
                    for i in first..last {
                        let y = (top + (i + 1) as f32 * rh).round() - 0.5;
                        shapes.push(Shape::line_segment(
                            [egui::pos2(gutter.right(), y), egui::pos2(right, y)],
                            Stroke::new(1.0, p.ruled),
                        ));
                    }
                }
                // red margin line like a school notebook
                let mx = gutter.right() - 3.0;
                shapes.push(Shape::line_segment(
                    [egui::pos2(mx, clip.top()), egui::pos2(mx, clip.bottom())],
                    Stroke::new(1.2, p.margin),
                ));
                ui.painter().set(bg, Shape::Vec(shapes));

                let nfont = FontId::monospace(size * 0.86);
                for (i, y0) in starts {
                    let y = y0 + rh / 2.0;
                    let color = if i == doc.line { p.accent } else { p.faint };
                    let pos = egui::pos2(gutter.right() - 12.0, y);
                    ui.painter().text(pos, Align2::RIGHT_CENTER, (i + 1).to_string(), nfont.clone(), color);
                    if doc.bookmarks.contains(&i) {
                        // a little terracotta ribbon, like a bookmark in a notebook
                        let x = gutter.left() + 6.0;
                        let r = egui::Rect::from_min_max(egui::pos2(x, y0 + 2.0), egui::pos2(x + 7.0, y0 + rh - 2.0));
                        let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), egui::pos2(r.center().x, r.bottom() - 3.0), r.left_bottom()];
                        ui.painter().add(Shape::convex_polygon(pts, p.accent, Stroke::NONE));
                    }
                }
            });
        });
        doc.scroll_y = out.state.offset().y;
        doc.view_h = out.inner_rect.height();
        doc.content_h = out.content_size.y;
    }

    pub fn palette_ui(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.palette.take() else { return };
        let p = *self.pal();
        let mut action: Option<Act> = None;
        let mut close = false;
        let now = ctx.input(|i| i.time);

        // Find in files: search once typing pauses.
        if st.mode == PaletteMode::InFiles {
            let q = st.query.trim().to_string();
            if q.chars().count() >= 2 && st.searched.as_deref() != Some(q.as_str()) {
                if now - st.typed_at > 0.3 {
                    st.hits = self.search_files(&q);
                    st.searched = Some(q);
                    st.selected = 0;
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(320));
                }
            }
        }

        // label, hint, what choosing it does
        let mut items: Vec<(String, String, Act)> = Vec::new();
        match st.mode {
            PaletteMode::Commands => {
                let cands = palette_entries()
                    .into_iter()
                    .filter(|(c, _, _)| self.enabled(ctx, *c))
                    .map(|(c, l, s)| (l, s.to_string(), Act::Run(c)))
                    .collect();
                items = rank(&st.query, cands, |c| c.0.clone());
            }
            PaletteMode::Files => {
                let mut files: Vec<(String, PathBuf)> = Vec::new();
                if let Some(root) = &self.folder {
                    flatten_files(&self.tree, root, &mut files);
                }
                for d in &self.docs {
                    if let Some(pth) = &d.path {
                        if !files.iter().any(|f| &f.1 == pth) {
                            files.push((pth.display().to_string(), pth.clone()));
                        }
                    }
                }
                let cands = files
                    .into_iter()
                    .map(|(rel, path)| {
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        (name, rel, Act::Open(path))
                    })
                    .collect();
                items = rank(&st.query, cands, |c| c.1.clone());
            }
            PaletteMode::Symbols => {
                let cands = symbols(self.doc())
                    .into_iter()
                    .map(|(line, label)| (label, format!("ligne {}", line + 1), Act::Line(line + 1)))
                    .collect();
                items = rank(&st.query, cands, |c| c.0.clone());
            }
            PaletteMode::InFiles => {
                for h in &st.hits {
                    items.push((h.label.clone(), h.hint.clone(), Act::OpenAt(h.path.clone(), h.line)));
                }
            }
            PaletteMode::Line => {}
        }
        if st.selected >= items.len().max(1) {
            st.selected = items.len().saturating_sub(1);
        }

        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if up {
            st.selected = st.selected.saturating_sub(1);
        }
        if down && st.selected + 1 < items.len() {
            st.selected += 1;
        }
        if esc {
            close = true;
        }

        let screen = ctx.content_rect();
        let width = (screen.width() * 0.55).clamp(380.0, 680.0);
        let line_count = self.doc().line_count();
        let no_folder = self.folder.is_none();
        let two_lines = matches!(st.mode, PaletteMode::Files | PaletteMode::InFiles);
        egui::Area::new(Id::new("palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 46.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(p.panel)
                    .stroke(Stroke::new(1.5, p.accent.gamma_multiply(0.6)))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let hint = match st.mode {
                            PaletteMode::Commands => "Tapez une commande…",
                            PaletteMode::Files => "Nom de fichier…",
                            PaletteMode::Line => "Numéro de ligne…",
                            PaletteMode::Symbols => "Nom de fonction, de titre…",
                            PaletteMode::InFiles => "Texte à chercher dans tous les fichiers…",
                        };
                        let resp = ui.add(
                            TextEdit::singleline(&mut st.query)
                                .hint_text(hint)
                                .desired_width(f32::INFINITY)
                                .font(FontId::proportional(16.0)),
                        );
                        if st.just_opened {
                            resp.request_focus();
                            st.just_opened = false;
                        }
                        if resp.changed() {
                            st.selected = 0;
                            st.typed_at = now;
                        }
                        ui.add_space(6.0);
                        if st.mode == PaletteMode::Line {
                            ui.label(RichText::new(format!("Aller à la ligne (1 – {line_count})")).color(p.faint));
                            if enter {
                                if let Ok(n) = st.query.trim().parse::<usize>() {
                                    action = Some(Act::Line(n));
                                }
                                close = true;
                            }
                            return;
                        }
                        if items.is_empty() {
                            let msg = match st.mode {
                                PaletteMode::Files if no_folder => "Ouvrez un dossier pour chercher ses fichiers.",
                                PaletteMode::InFiles if st.query.trim().chars().count() < 2 => {
                                    if no_folder {
                                        "Cherche dans les fichiers ouverts (ouvrez un dossier pour chercher partout)."
                                    } else {
                                        "Tapez au moins deux lettres."
                                    }
                                }
                                PaletteMode::Symbols if st.query.is_empty() => "Aucun symbole dans ce fichier.",
                                _ => "Rien ne correspond.",
                            };
                            ui.label(RichText::new(msg).color(p.faint).italics());
                        } else if st.mode == PaletteMode::InFiles {
                            let n = items.len();
                            let more = if n >= MAX_HITS { "+" } else { "" };
                            ui.label(RichText::new(format!("{n}{more} résultat(s)")).size(12.0).color(p.faint));
                        }
                        ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                            for (k, (label, hint, act)) in items.iter().enumerate() {
                                let sel = k == st.selected;
                                let h = if two_lines { 38.0 } else { 26.0 };
                                let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
                                if sel {
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), p.selection);
                                    if up || down {
                                        ui.scroll_to_rect(rect, None);
                                    }
                                } else if r.hovered() {
                                    ui.painter().rect_filled(rect, CornerRadius::same(6), p.selection.gamma_multiply(0.5));
                                }
                                let pt = ui.painter_at(rect);
                                if two_lines {
                                    let f = if st.mode == PaletteMode::InFiles { FontId::monospace(13.0) } else { FontId::proportional(14.0) };
                                    pt.text(rect.left_top() + Vec2::new(10.0, 4.0), Align2::LEFT_TOP, label, f, p.ink);
                                    pt.text(rect.left_top() + Vec2::new(10.0, 21.0), Align2::LEFT_TOP, hint, FontId::proportional(11.5), p.faint);
                                } else {
                                    pt.text(rect.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.0), p.ink);
                                    pt.text(rect.right_center() - Vec2::new(10.0, 0.0), Align2::RIGHT_CENTER, hint, FontId::monospace(11.5), p.faint);
                                }
                                if r.clicked() || (enter && sel) {
                                    action = Some(act.clone());
                                    close = true;
                                }
                            }
                        });
                    });
            });

        if !close {
            self.palette = Some(st);
        } else {
            self.focus_editor(ctx);
        }
        match action {
            Some(Act::Run(c)) => self.run(ctx, c),
            Some(Act::Open(pth)) => {
                self.open_path(&pth);
                self.focus_editor(ctx);
            }
            Some(Act::OpenAt(pth, line)) => {
                self.remember_position(ctx);
                if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(pth.as_path())) {
                    self.active = i;
                } else {
                    self.open_path(&pth);
                }
                self.goto_line(ctx, line + 1);
            }
            Some(Act::Line(n)) => self.goto_line(ctx, n),
            None => {}
        }
    }

    /// Every line containing `q` (ignoring case) in the folder's files and the open tabs.
    fn search_files(&self, q: &str) -> Vec<Hit> {
        let ql = q.to_lowercase();
        let mut hits = Vec::new();
        let mut files: Vec<(String, PathBuf)> = Vec::new();
        if let Some(root) = &self.folder {
            flatten_files(&self.tree, root, &mut files);
        }
        for d in &self.docs {
            if let Some(pth) = &d.path {
                if !files.iter().any(|f| &f.1 == pth) {
                    files.push((d.title.clone(), pth.clone()));
                }
            }
        }
        let scan = |name: &str, path: &PathBuf, text: &str, hits: &mut Vec<Hit>| {
            for (i, l) in text.split('\n').enumerate() {
                if hits.len() >= MAX_HITS {
                    return;
                }
                if l.to_lowercase().contains(&ql) {
                    let t: String = l.trim().chars().take(140).collect();
                    hits.push(Hit { label: t, hint: format!("{name} : ligne {}", i + 1), path: path.clone(), line: i });
                }
            }
        };
        for (rel, path) in files.into_iter().take(4000) {
            if hits.len() >= MAX_HITS {
                break;
            }
            // Open tabs: search what's on screen, even unsaved.
            if let Some(d) = self.docs.iter().find(|d| d.path.as_deref() == Some(path.as_path())) {
                scan(&rel, &path, &d.text, &mut hits);
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if meta.len() > 2_000_000 {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if bytes.iter().take(4096).any(|b| *b == 0) {
                continue; // binary file
            }
            let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
            scan(&rel, &path, &text, &mut hits);
        }
        hits
    }

    pub fn pending_ui(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else { return };
        let (title, body) = match pending {
            Pending::CloseTab(i) => (
                "Fermer l'onglet",
                format!(
                    "« {} » a des modifications non enregistrées.",
                    self.docs.get(*i).map_or("", |d| d.title.as_str())
                ),
            ),
            Pending::Quit => ("Quitter", "Certains fichiers ont des modifications non enregistrées.".to_string()),
        };
        let mut choice = 0; // 1 save, 2 discard, 3 cancel
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(body);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Enregistrer").clicked() {
                        choice = 1;
                    }
                    if ui.button("Ne pas enregistrer").clicked() {
                        choice = 2;
                    }
                    if ui.button("Annuler").clicked() {
                        choice = 3;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            choice = 3;
        }
        if choice == 0 {
            return;
        }
        let pending = self.pending.take().unwrap();
        match (pending, choice) {
            (Pending::CloseTab(i), 1) => {
                if self.save(ctx, i, false) {
                    self.close_tab(i);
                }
            }
            (Pending::CloseTab(i), 2) => self.close_tab(i),
            (Pending::Quit, 1) => {
                let mut ok = true;
                for i in 0..self.docs.len() {
                    if self.docs[i].dirty() {
                        ok &= self.save(ctx, i, false);
                    }
                }
                if ok {
                    self.allow_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            (Pending::Quit, 2) => {
                self.allow_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
    }
}
