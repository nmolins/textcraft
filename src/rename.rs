//! Renaming a tab: an unsaved tab just gets a new name (offered when saving),
//! a saved file is renamed on disk too.

use eframe::egui::{self, text::CCursor, text::CCursorRange, Key, RichText};

use crate::app::App;
use crate::highlight::Lang;

const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

impl App {
    pub fn start_rename(&mut self, idx: usize) {
        if let Some(d) = self.docs.get(idx) {
            if d.remote.is_none() {
                self.renaming = Some((d.id, d.title.clone(), true));
            }
        }
    }

    /// Gives the tab `doc_id` the name `name`. Returns false (with a message) when it can't.
    pub fn apply_rename(&mut self, ctx: &egui::Context, doc_id: u64, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            self.flash(ctx, "Le nom ne peut pas être vide");
            return false;
        }
        if name.chars().any(|c| FORBIDDEN.contains(&c) || c.is_control()) || name == "." || name == ".." {
            self.flash(ctx, "Un nom de fichier ne peut pas contenir / \\ : * ? \" < > |");
            return false;
        }
        let Some(i) = self.docs.iter().position(|d| d.id == doc_id) else { return false };
        let old_path = self.docs[i].path.clone();
        if let Some(old) = old_path {
            let new = old.with_file_name(name);
            if new != old {
                // Only a change of upper/lower case is allowed onto the "same" file on Windows.
                let same_file_other_case = new.to_string_lossy().to_lowercase() == old.to_string_lossy().to_lowercase();
                if new.exists() && !same_file_other_case {
                    self.flash(ctx, format!("Un fichier « {name} » existe déjà ici"));
                    return false;
                }
                if old.exists() {
                    if let Err(e) = std::fs::rename(&old, &new) {
                        self.flash(ctx, format!("Impossible de renommer le fichier : {e}"));
                        return false;
                    }
                }
                self.recent_files.retain(|p| p != &old);
                self.docs[i].path = Some(new);
                if let Some(f) = self.folder.clone() {
                    if self.docs[i].path.as_ref().is_some_and(|p| p.starts_with(&f)) {
                        self.run(ctx, crate::app::Cmd::RefreshFolder);
                    }
                }
            }
        }
        let d = &mut self.docs[i];
        d.title = name.to_string();
        let lang = Lang::from_name(name);
        if lang != Lang::Plain || name.contains('.') {
            d.lang = lang;
        }
        if d.lang == Lang::Markdown && self.md_auto {
            d.preview = true;
        }
        let msg = if d.path.is_some() { format!("Fichier renommé : {name}") } else { format!("Onglet renommé : {name}") };
        self.flash(ctx, msg);
        true
    }

    /// The small "Renommer" window.
    pub fn rename_window(&mut self, ctx: &egui::Context) {
        // A double-click in the side panel's list asks for a rename.
        if let Some(i) = ctx.data_mut(|d| d.remove_temp::<usize>(egui::Id::new("textcraft_rename"))) {
            self.start_rename(i);
        }
        let Some((doc_id, mut name, first)) = self.renaming.take() else { return };
        let Some(doc) = self.docs.iter().find(|d| d.id == doc_id) else { return };
        let on_disk = doc.path.is_some();
        let p = *self.pal();
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Renommer")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                let out = egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY).font(egui::FontId::proportional(16.0)).show(ui);
                if first {
                    // Select the name without its extension, ready to type over.
                    out.response.request_focus();
                    let stem = name.rfind('.').filter(|&k| k > 0).unwrap_or(name.len());
                    let stem_chars = name[..stem].chars().count();
                    let mut st = out.state.clone();
                    let mut range = CCursorRange::one(CCursor::new(stem_chars));
                    range.secondary = CCursor::new(0);
                    st.cursor.set_char_range(Some(range));
                    st.store(ui.ctx(), out.response.id);
                }
                if out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    confirm = true;
                }
                ui.add_space(4.0);
                let note = if on_disk {
                    "Le fichier sera aussi renommé sur le disque."
                } else {
                    "Pas encore enregistré : ce nom sera proposé à l'enregistrement. L'extension (.md, .py…) choisit les couleurs."
                };
                ui.label(RichText::new(note).size(12.0).color(p.faint));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Renommer").clicked() {
                        confirm = true;
                    }
                    if ui.button("Annuler").clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if cancel {
            self.focus_editor(ctx);
            return;
        }
        if confirm {
            if self.apply_rename(ctx, doc_id, &name) {
                self.focus_editor(ctx);
                return;
            }
        }
        self.renaming = Some((doc_id, name, false));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stem_is_the_name_before_the_last_dot() {
        let name = "notes.de.cours.md";
        let stem = name.rfind('.').filter(|&k| k > 0).unwrap_or(name.len());
        assert_eq!(&name[..stem], "notes.de.cours");
        let dotfile = ".gitignore";
        let stem = dotfile.rfind('.').filter(|&k| k > 0).unwrap_or(dotfile.len());
        assert_eq!(&dotfile[..stem], ".gitignore");
    }
}
