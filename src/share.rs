//! Screens for sharing tabs between computers: the settings window, the
//! "Autres ordinateurs" list in the side panel, and the read-only banner.

use std::sync::Arc;

use eframe::egui::{self, RichText};

use crate::app::{App, Cmd};
use crate::doc::Doc;
use crate::highlight::Lang;
use crate::net::{self, TabInfo};

fn text_rev(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn lang_by_name(name: &str) -> Lang {
    Lang::ALL.into_iter().find(|l| format!("{l:?}") == name).unwrap_or(Lang::Plain)
}

/// Which remote tab the person picked in the side panel.
pub struct RemotePick {
    pub peer: String,
    pub peer_name: String,
    pub tab: TabInfo,
}

impl App {
    /// Applies the sharing settings (derives the key from the code; takes a fraction of a second).
    pub fn apply_share(&mut self, ctx: &egui::Context) {
        let code = self.share_code.trim().to_string();
        let ok = code.chars().count() >= 6;
        let key = if self.share_enabled && ok { Some(net::derive_key(&code)) } else { None };
        self.net.configure(self.share_enabled && ok, key, &self.share_name, &self.share_hint);
        self.share_last_sync = -10.0;
        if self.share_enabled && !ok {
            self.flash(ctx, "Le code secret doit avoir au moins 6 caractères");
        }
    }

    /// Called every frame: publishes our tabs and refreshes the open remote tabs.
    pub fn share_tick(&mut self, ctx: &egui::Context) {
        if !self.net.enabled() {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now - self.share_last_sync >= 1.0 {
            self.share_last_sync = now;
            let tabs = self
                .docs
                .iter()
                .filter(|d| d.remote.is_none())
                .map(|d| {
                    let info = TabInfo {
                        id: d.id,
                        title: d.title.chars().take(120).collect(),
                        lang: format!("{:?}", d.lang),
                        rev: text_rev(&d.text),
                        dirty: d.dirty(),
                    };
                    (info, Arc::new(d.text.clone()))
                })
                .collect();
            self.net.set_tabs(tabs);
        }
        // Keep remote tabs up to date (the visible one first; others when looked at).
        ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        let peers = self.net.peers();
        let active = self.active;
        for (i, d) in self.docs.iter_mut().enumerate() {
            let Some((peer_id, tab_id)) = d.remote.clone() else { continue };
            let peer = peers.iter().find(|p| p.id == peer_id);
            d.remote_online = peer.is_some();
            let wanted_rev = peer.and_then(|p| p.tabs.iter().find(|t| t.id == tab_id)).map(|t| t.rev);
            if let Some(c) = self.net.fetched(&peer_id, tab_id) {
                if text_rev(&c.text) != text_rev(&d.saved) {
                    d.text = c.text.clone();
                    d.saved = c.text;
                }
                d.remote_rev = c.rev;
            }
            if i == active && wanted_rev.is_some() && wanted_rev != Some(d.remote_rev) {
                self.net.request(&peer_id, tab_id, ctx);
            }
        }
    }

    /// Opens (or shows again) a tab from another computer, read-only.
    pub fn open_remote(&mut self, ctx: &egui::Context, pick: RemotePick) {
        if let Some(i) = self.docs.iter().position(|d| d.remote.as_ref() == Some(&(pick.peer.clone(), pick.tab.id))) {
            self.active = i;
            return;
        }
        let id = self.alloc_id();
        let mut d = Doc::new(id, format!("{} — {}", pick.tab.title, pick.peer_name), String::new(), None);
        d.lang = lang_by_name(&pick.tab.lang);
        d.preview = d.lang == Lang::Markdown && self.md_auto;
        d.remote = Some((pick.peer.clone(), pick.tab.id));
        d.remote_from = pick.peer_name;
        d.remote_online = true;
        self.docs.push(d);
        self.active = self.docs.len() - 1;
        self.net.request(&pick.peer, pick.tab.id, ctx);
    }

    /// A local, editable copy of the remote tab on screen.
    pub fn remote_copy(&mut self) {
        let d = self.doc();
        let title = d.title.split(" — ").next().unwrap_or("copie").to_string();
        let (text, lang) = (d.text.clone(), d.lang);
        let id = self.alloc_id();
        let mut copy = Doc::new(id, title, text, None);
        copy.saved = String::new();
        copy.lang = lang;
        self.docs.push(copy);
        self.active = self.docs.len() - 1;
    }

    /// The "Autres ordinateurs" part of the side panel (drawn inside its scroll area).
    pub fn share_sidebar(&self, ui: &mut egui::Ui) -> Option<RemotePick> {
        let p = *self.pal();
        let mut pick = None;
        ui.add_space(14.0);
        ui.label(RichText::new("AUTRES ORDINATEURS").size(11.0).color(p.faint).strong());
        ui.add_space(2.0);
        if !self.net.enabled() {
            ui.label(RichText::new("Partage désactivé.").color(p.faint).italics());
            if ui.link(RichText::new("Voir les onglets de mes autres PC…").color(p.accent).size(12.5)).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("textcraft_open_share"), true));
            }
            return None;
        }
        if let Some(problem) = self.net.problem() {
            ui.label(RichText::new(problem).color(p.accent).size(12.0));
        }
        let peers = self.net.peers();
        if peers.is_empty() {
            ui.label(RichText::new("Recherche sur le réseau…").color(p.faint).italics());
            ui.label(RichText::new("Ouvrez TextCraft sur l'autre ordinateur, avec le même code.").color(p.faint).size(11.5));
        }
        for peer in peers {
            let os = match peer.os.as_str() {
                "windows" => "Windows",
                "linux" => "Linux",
                "macos" => "Mac",
                o => o,
            };
            egui::CollapsingHeader::new(RichText::new(format!("▣ {}", peer.name)).color(p.ink))
                .id_salt(("peer", &peer.id))
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("{os} · {}", peer.addr)).size(11.0).color(p.faint));
                    if peer.tabs.is_empty() {
                        ui.label(RichText::new("Aucun onglet ouvert").color(p.faint).italics());
                    }
                    for t in &peer.tabs {
                        let open_here = self.docs.iter().position(|d| d.remote.as_ref() == Some(&(peer.id.clone(), t.id)));
                        let selected = open_here == Some(self.active);
                        let label = if t.dirty { format!("● {}", t.title) } else { t.title.clone() };
                        let r = ui
                            .selectable_label(selected, RichText::new(label).size(13.0))
                            .on_hover_text("Voir cet onglet (lecture seule)");
                        if r.clicked() {
                            pick = Some(RemotePick { peer: peer.id.clone(), peer_name: peer.name.clone(), tab: t.clone() });
                        }
                    }
                });
        }
        pick
    }

    /// Strip above a remote tab: where it comes from, read-only, and what can be done with it.
    pub fn remote_banner(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let d = self.doc();
        let from = d.remote_from.clone();
        let online = d.remote_online;
        let loading = d.text.is_empty() && d.remote_rev == 0;
        let mut copy_all = false;
        let mut local_copy = false;
        egui::Frame::NONE.fill(p.tab_idle).inner_margin(egui::Margin::symmetric(12, 5)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Lecture seule").color(p.accent).strong());
                let state = if loading {
                    "chargement…"
                } else if online {
                    "en direct"
                } else {
                    "hors ligne (dernière version reçue)"
                };
                ui.label(RichText::new(format!("de {from} · {state}")).size(12.5).color(p.faint));
                ui.add_space(8.0);
                if ui.small_button("Tout copier").on_hover_text("Copier tout le texte, pour le coller ailleurs").clicked() {
                    copy_all = true;
                }
                if ui.small_button("Copie modifiable").on_hover_text("Ouvrir une copie de cet onglet sur cet ordinateur").clicked() {
                    local_copy = true;
                }
            });
        });
        let ctx = ui.ctx().clone();
        if copy_all {
            ctx.copy_text(self.doc().text.clone());
            self.flash(&ctx, "Texte copié : collez-le où vous voulez");
        }
        if local_copy {
            self.run(&ctx, Cmd::RemoteCopy);
        }
    }

    /// Outils > Partage entre ordinateurs…
    pub fn share_window(&mut self, ctx: &egui::Context) {
        let asked = ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("textcraft_open_share"))).unwrap_or(false);
        if asked {
            self.show_share = true;
        }
        if !self.show_share {
            return;
        }
        let p = *self.pal();
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Partage entre ordinateurs")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Voyez les onglets ouverts sur vos autres ordinateurs du réseau local, en lecture seule, pour copier-coller de l'un à l'autre.");
                ui.add_space(8.0);
                if ui.checkbox(&mut self.share_enabled, "Partager mes onglets avec mes autres ordinateurs").changed() {
                    apply = true;
                }
                ui.add_space(6.0);
                egui::Grid::new("share_grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Nom de cet ordinateur");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.share_name).desired_width(240.0).hint_text(net::computer_name()));
                    apply |= r.lost_focus();
                    ui.end_row();

                    ui.label("Code secret");
                    ui.horizontal(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.share_code)
                                .password(!self.share_show_code)
                                .desired_width(170.0)
                                .hint_text("au moins 6 caractères"),
                        );
                        apply |= r.lost_focus();
                        let eye = if self.share_show_code { "Masquer" } else { "Afficher" };
                        if ui.small_button(eye).clicked() {
                            self.share_show_code = !self.share_show_code;
                        }
                        if ui.small_button("Générer").on_hover_text("Proposer un code facile à recopier").clicked() {
                            self.share_code = net::suggest_code();
                            self.share_show_code = true;
                            apply = true;
                        }
                    });
                    ui.end_row();

                    ui.label("Adresse d'un autre ordinateur");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.share_hint)
                            .desired_width(240.0)
                            .hint_text("facultatif, ex. 192.168.1.20"),
                    );
                    apply |= r.lost_focus();
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Saisissez le même code sur chaque ordinateur. Tout ce qui passe sur le réseau est chiffré avec ce code : sans lui, rien n'est lisible.")
                        .size(12.0)
                        .color(p.faint),
                );
                ui.label(
                    RichText::new("L'adresse n'est utile que si les ordinateurs ne se trouvent pas tout seuls.")
                        .size(12.0)
                        .color(p.faint),
                );
                ui.label(
                    RichText::new("La première fois, Windows peut demander d'autoriser TextCraft sur le réseau privé : acceptez.")
                        .size(12.0)
                        .color(p.faint),
                );
                ui.separator();
                let peers = self.net.peers();
                let status = if !self.net.enabled() {
                    "Partage désactivé.".to_string()
                } else if peers.is_empty() {
                    "Partage actif. Aucun autre ordinateur trouvé pour l'instant.".to_string()
                } else {
                    let names: Vec<String> = peers.iter().map(|p| p.name.clone()).collect();
                    format!("Partage actif. Trouvé(s) : {}", names.join(", "))
                };
                ui.label(RichText::new(status).color(if self.net.enabled() { p.accent } else { p.faint }));
                if let Some(problem) = self.net.problem() {
                    ui.label(RichText::new(problem).color(p.accent));
                }
                ui.add_space(4.0);
                if ui.button("Appliquer").clicked() {
                    apply = true;
                }
            });
        if apply {
            self.apply_share(ctx);
        }
        if !open {
            self.show_share = false;
        }
    }
}
