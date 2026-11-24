//! The menu bar, described once and used for both the menus and the command palette.

use eframe::egui::{self, RichText};

use crate::app::{App, Cmd};
use crate::highlight::Lang;

pub enum M {
    /// label, shortcut, command
    I(&'static str, &'static str, Cmd),
    /// label built at run time (languages, sizes)
    D(String, &'static str, Cmd),
    Sep,
    Sub(&'static str, Vec<M>),
    RecentFiles,
    RecentFolders,
    OpenTabs,
}

use M::*;

pub fn menus() -> Vec<(&'static str, Vec<M>)> {
    let langs: Vec<M> = Lang::ALL.iter().map(|l| D(l.name().to_string(), "", Cmd::SetLang(*l))).collect();
    let sizes: Vec<M> = [2u8, 3, 4, 8].iter().map(|n| D(format!("Largeur de tabulation : {n}"), "", Cmd::TabSize(*n))).collect();
    let rulers: Vec<M> = [0u16, 72, 80, 100, 120]
        .iter()
        .map(|c| D(if *c == 0 { "Aucune".to_string() } else { format!("{c} colonnes") }, "", Cmd::Ruler(*c)))
        .collect();
    let mut indentation = sizes;
    indentation.push(Sep);
    indentation.push(I("Convertir l'indentation en espaces", "", Cmd::ConvertIndent));

    vec![
        (
            "Fichier",
            vec![
                I("Nouveau fichier", "Ctrl+N", Cmd::NewFile),
                I("Ouvrir un fichier…", "Ctrl+O", Cmd::OpenFile),
                I("Ouvrir un dossier…", "Ctrl+Maj+O", Cmd::OpenFolder),
                Sub("Ouvrir un fichier récent", vec![RecentFiles]),
                Sep,
                I("Enregistrer", "Ctrl+S", Cmd::Save),
                I("Enregistrer sous…", "Ctrl+Maj+S", Cmd::SaveAs),
                I("Renommer…", "", Cmd::RenameTab),
                I("Tout enregistrer", "", Cmd::SaveAll),
                Sep,
                I("Fermer la fenêtre", "Ctrl+Maj+W", Cmd::Quit),
                Sep,
                I("Fermer le fichier", "Ctrl+W", Cmd::CloseTab),
                I("Recharger le fichier", "", Cmd::RevertFile),
                I("Fermer tous les fichiers", "", Cmd::CloseAll),
                Sep,
                I("Quitter", "", Cmd::Quit),
            ],
        ),
        (
            "Édition",
            vec![
                I("Annuler", "Ctrl+Z", Cmd::Undo),
                I("Rétablir", "Ctrl+Y", Cmd::Redo),
                Sep,
                I("Couper", "Ctrl+X", Cmd::Cut),
                I("Copier", "Ctrl+C", Cmd::Copy),
                I("Coller", "Ctrl+V", Cmd::Paste),
                Sep,
                Sub(
                    "Ligne",
                    vec![
                        I("Indenter", "Ctrl+]", Cmd::Indent),
                        I("Désindenter", "Ctrl+[", Cmd::Unindent),
                        I("Monter la ligne", "Ctrl+Maj+Haut", Cmd::MoveLineUp),
                        I("Descendre la ligne", "Ctrl+Maj+Bas", Cmd::MoveLineDown),
                        I("Dupliquer la ligne", "Ctrl+Maj+D", Cmd::DuplicateLine),
                        I("Supprimer la ligne", "Ctrl+Maj+K", Cmd::DeleteLine),
                        I("Joindre les lignes", "Ctrl+J", Cmd::JoinLines),
                    ],
                ),
                Sub("Commentaire", vec![I("Commenter / décommenter", "Ctrl+/", Cmd::ToggleComment)]),
                Sub(
                    "Texte",
                    vec![
                        I("Insérer une ligne avant", "Ctrl+Maj+Entrée", Cmd::InsertLineBefore),
                        I("Insérer une ligne après", "Ctrl+Entrée", Cmd::InsertLineAfter),
                    ],
                ),
                Sub(
                    "Convertir la casse",
                    vec![
                        I("MAJUSCULES", "Ctrl+Alt+U", Cmd::Upper),
                        I("minuscules", "Ctrl+Maj+U", Cmd::Lower),
                        I("Première Lettre En Majuscule", "", Cmd::TitleCase),
                        I("Inverser la casse", "", Cmd::SwapCase),
                    ],
                ),
                Sep,
                I("Trier les lignes", "F9", Cmd::SortLines),
                I("Trier les lignes (respecter la casse)", "Ctrl+F9", Cmd::SortLinesCase),
                Sub(
                    "Permuter les lignes",
                    vec![
                        I("Inverser l'ordre", "", Cmd::ReverseLines),
                        I("Garder une seule fois chaque ligne", "", Cmd::UniqueLines),
                    ],
                ),
            ],
        ),
        (
            "Sélection",
            vec![
                I("Tout sélectionner", "Ctrl+A", Cmd::SelectAll),
                Sep,
                I("Étendre à la ligne", "Ctrl+L", Cmd::ExpandLine),
                I("Étendre au mot", "Ctrl+D", Cmd::ExpandWord),
                I("Étendre au paragraphe", "", Cmd::ExpandParagraph),
                I("Étendre aux parenthèses", "Ctrl+Maj+M", Cmd::ExpandBrackets),
            ],
        ),
        (
            "Rechercher",
            vec![
                I("Chercher…", "Ctrl+F", Cmd::Find),
                I("Suivant", "F3", Cmd::FindNext),
                I("Précédent", "Maj+F3", Cmd::FindPrev),
                Sep,
                I("Remplacer…", "Ctrl+H", Cmd::Replace),
                I("Remplacer le suivant", "Ctrl+Maj+H", Cmd::ReplaceNext),
                Sep,
                I("Recherche rapide", "Ctrl+F3", Cmd::QuickFind),
                Sep,
                I("Chercher la sélection", "Ctrl+E", Cmd::UseSelFind),
                I("Remplacer par la sélection", "Ctrl+Maj+E", Cmd::UseSelReplace),
                Sep,
                I("Chercher dans les fichiers…", "Ctrl+Maj+F", Cmd::FindInFiles),
            ],
        ),
        (
            "Affichage",
            vec![
                I("Panneau latéral", "Ctrl+B", Cmd::ToggleSidebar),
                I("Mini-carte", "", Cmd::ToggleMinimap),
                I("Onglets", "", Cmd::ToggleTabs),
                I("Barre d'état", "", Cmd::ToggleStatus),
                Sep,
                I("Aperçu Markdown en direct", "Alt+M", Cmd::TogglePreview),
                I("Ouvrir l'aperçu avec les fichiers Markdown", "", Cmd::ToggleAutoPreview),
                Sep,
                I("Plein écran", "F11", Cmd::FullScreen),
                I("Mode sans distraction", "Maj+F11", Cmd::DistractionFree),
                Sep,
                Sub("Syntaxe", langs),
                Sub("Indentation", indentation),
                Sub(
                    "Fins de ligne",
                    vec![
                        I("Windows (CRLF)", "", Cmd::LineEndings(true)),
                        I("Unix (LF)", "", Cmd::LineEndings(false)),
                    ],
                ),
                Sep,
                I("Retour à la ligne automatique", "", Cmd::WordWrap),
                Sub("Règle", rulers),
            ],
        ),
        (
            "Aller",
            vec![
                I("Aller à n'importe quel fichier…", "Ctrl+P", Cmd::QuickOpen),
                Sep,
                I("Aller au symbole…", "Ctrl+R", Cmd::GotoSymbol),
                I("Aller à la ligne…", "Ctrl+G", Cmd::GotoLine),
                Sep,
                I("Revenir en arrière", "Alt+-", Cmd::JumpBack),
                I("Aller en avant", "Alt+Maj+-", Cmd::JumpForward),
                Sep,
                Sub(
                    "Changer de fichier",
                    vec![
                        I("Onglet suivant", "Ctrl+Tab", Cmd::NextTab),
                        I("Onglet précédent", "Ctrl+Maj+Tab", Cmd::PrevTab),
                        Sep,
                        OpenTabs,
                    ],
                ),
                Sub(
                    "Signets",
                    vec![
                        I("Poser / enlever un signet", "Ctrl+F2", Cmd::ToggleBookmark),
                        I("Signet suivant", "F2", Cmd::NextBookmark),
                        I("Signet précédent", "Maj+F2", Cmd::PrevBookmark),
                        I("Effacer les signets", "Ctrl+Maj+F2", Cmd::ClearBookmarks),
                    ],
                ),
                Sep,
                I("Aller à la parenthèse correspondante", "Ctrl+M", Cmd::MatchingBracket),
            ],
        ),
        (
            "Outils",
            vec![
                I("Palette de commandes…", "Ctrl+Maj+P", Cmd::Palette),
                I("Chercher dans les fichiers…", "Ctrl+Maj+F", Cmd::FindInFiles),
                Sep,
                I("Partage entre ordinateurs…", "", Cmd::ShareSettings),
            ],
        ),
        (
            "Projet",
            vec![
                I("Ouvrir un dossier…", "Ctrl+Maj+O", Cmd::OpenFolder),
                Sub("Dossiers récents", vec![RecentFolders]),
                Sep,
                I("Actualiser le dossier", "", Cmd::RefreshFolder),
                I("Fermer le dossier", "", Cmd::CloseFolder),
            ],
        ),
        (
            "Préférences",
            vec![
                I("Thème papier crème", "", Cmd::ThemePaper),
                I("Thème encre de nuit", "", Cmd::ThemeNight),
                I("Lignes de cahier", "", Cmd::ToggleRuled),
                Sep,
                Sub(
                    "Police",
                    vec![
                        I("Agrandir", "Ctrl+=", Cmd::FontBigger),
                        I("Réduire", "Ctrl+-", Cmd::FontSmaller),
                        I("Taille normale", "Ctrl+0", Cmd::FontReset),
                    ],
                ),
            ],
        ),
        (
            "Aide",
            vec![
                I("Raccourcis clavier", "", Cmd::Shortcuts),
                Sep,
                I("Rechercher des mises à jour…", "", Cmd::CheckUpdates),
                I("Vérifier les mises à jour au démarrage", "", Cmd::ToggleAutoUpdate),
                Sep,
                I("À propos de TextCraft", "", Cmd::About),
            ],
        ),
    ]
}

/// Every fixed command with its full name ("Menu : entrée") and shortcut, for the palette.
pub fn palette_entries() -> Vec<(Cmd, String, &'static str)> {
    fn walk(prefix: &str, items: &[M], out: &mut Vec<(Cmd, String, &'static str)>) {
        for it in items {
            match it {
                I(l, s, c) => out.push((*c, format!("{prefix} : {l}"), *s)),
                D(l, s, c) => out.push((*c, format!("{prefix} : {l}"), *s)),
                Sub(l, sub) => walk(&format!("{prefix} : {l}"), sub, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for (title, items) in menus() {
        walk(title, &items, &mut out);
    }
    // The same command can sit in two menus: keep the first.
    let mut seen: Vec<Cmd> = Vec::new();
    out.retain(|(c, _, _)| {
        if seen.contains(c) {
            false
        } else {
            seen.push(*c);
            true
        }
    });
    out
}

fn file_label(p: &std::path::Path) -> String {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let dir = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
    format!("{name}   —   {dir}")
}

impl App {
    fn menu_entry(&self, ui: &mut egui::Ui, label: &str, short: &str, cmd: Cmd, chosen: &mut Option<Cmd>) {
        let enabled = self.enabled(ui.ctx(), cmd);
        let p = self.pal();
        let font = egui::TextStyle::Button.resolve(ui.style());
        let fmt = |color| egui::text::TextFormat { font_id: font.clone(), color, ..Default::default() };
        let mut job = egui::text::LayoutJob::default();
        match self.checked(cmd) {
            Some(on) => {
                // A hand-drawn-looking tick in terracotta ink; empty space keeps labels aligned.
                job.append(if on { "✓" } else { " " }, 0.0, fmt(if on { p.accent } else { p.faint }));
                job.append(label, 10.0, fmt(if enabled { p.ink } else { p.faint }));
            }
            None => job.append(label, 0.0, fmt(if enabled { p.ink } else { p.faint })),
        }
        let b = egui::Button::new(job).shortcut_text(short);
        if ui.add_enabled(enabled, b).clicked() {
            *chosen = Some(cmd);
            ui.close();
        }
    }

    fn menu_items(&self, ui: &mut egui::Ui, items: &[M], chosen: &mut Option<Cmd>) {
        for it in items {
            match it {
                I(l, s, c) => self.menu_entry(ui, l, s, *c, chosen),
                D(l, s, c) => self.menu_entry(ui, l, s, *c, chosen),
                Sep => {
                    ui.separator();
                }
                Sub(l, sub) => {
                    ui.menu_button(*l, |ui| self.menu_items(ui, sub, chosen));
                }
                RecentFiles => {
                    if self.recent_files.is_empty() {
                        ui.label(RichText::new("Aucun fichier récent").italics().weak());
                    }
                    for (i, p) in self.recent_files.iter().enumerate() {
                        if ui.button(file_label(p)).clicked() {
                            *chosen = Some(Cmd::OpenRecent(i));
                            ui.close();
                        }
                    }
                }
                RecentFolders => {
                    if self.recent_folders.is_empty() {
                        ui.label(RichText::new("Aucun dossier récent").italics().weak());
                    }
                    for (i, p) in self.recent_folders.iter().enumerate() {
                        if ui.button(p.display().to_string()).clicked() {
                            *chosen = Some(Cmd::RecentFolder(i));
                            ui.close();
                        }
                    }
                }
                OpenTabs => {
                    for (i, d) in self.docs.iter().enumerate() {
                        let short = if i < 9 { ["Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9"][i] } else { "" };
                        let label = if d.dirty() { format!("{} •", d.title) } else { d.title.clone() };
                        self.menu_entry(ui, &label, short, Cmd::SwitchTo(i), chosen);
                    }
                }
            }
        }
    }

    /// "Raccourcis clavier" and "À propos" windows from the Aide menu.
    pub fn help_windows(&mut self, ctx: &egui::Context) {
        let p = *self.pal();
        if self.show_keys {
            let mut open = true;
            egui::Window::new("Raccourcis clavier")
                .open(&mut open)
                .collapsible(false)
                .default_size([460.0, 520.0])
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("keys").striped(true).spacing([24.0, 6.0]).show(ui, |ui| {
                            for (_, label, short) in palette_entries() {
                                if short.is_empty() {
                                    continue;
                                }
                                ui.label(label);
                                ui.label(RichText::new(short).monospace().color(p.accent));
                                ui.end_row();
                            }
                        });
                    });
                });
            self.show_keys = open;
        }
        if self.show_about {
            let mut open = true;
            egui::Window::new("À propos")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("✎").size(40.0).color(p.accent));
                        ui.label(RichText::new("TextCraft").size(22.0).strong());
                        ui.label(RichText::new(format!("version {}", env!("CARGO_PKG_VERSION"))).color(p.faint));
                        if let Some(backend) = crate::display::backend() {
                            ui.label(RichText::new(format!("Affichage : {backend}")).size(12.0).color(p.faint));
                        }
                        ui.add_space(8.0);
                        ui.label("Un carnet de code rapide et simple,\nsur papier crème et à l'encre.");
                        ui.add_space(4.0);
                        ui.label(RichText::new("Tout est gardé à la fermeture, même ce qui n'est pas enregistré.").italics().color(p.faint));
                    });
                });
            self.show_about = open;
        }
    }

    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut chosen: Option<Cmd> = None;
        let p = *self.pal();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("✎ TextCraft").color(p.accent).strong());
            ui.add_space(6.0);
            for (title, items) in menus() {
                ui.menu_button(title, |ui| {
                    ui.set_min_width(260.0);
                    self.menu_items(ui, &items, &mut chosen);
                });
            }
        });
        if let Some(c) = chosen {
            self.run(&ctx, c);
        }
    }
}
