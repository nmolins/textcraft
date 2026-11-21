//! Remembers everything between runs: open tabs (even unsaved text),
//! the open folder, the active tab and the view settings.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::doc::Doc;
use crate::highlight::Lang;

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Session {
    pub docs: Vec<SavedDoc>,
    pub active: usize,
    pub folder: Option<PathBuf>,
    pub dark: bool,
    pub font_size: f32,
    pub sidebar: bool,
    pub minimap: bool,
    pub ruled: bool,
    pub find_query: String,
    pub show_tabs: Option<bool>,
    pub show_status: Option<bool>,
    pub wrap: bool,
    pub ruler: u16,
    pub tab_size: u8,
    pub recent_files: Vec<PathBuf>,
    pub recent_folders: Vec<PathBuf>,
    pub md_auto: Option<bool>,
    pub share_enabled: bool,
    pub share_code: String,
    pub share_name: String,
    pub share_hint: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedDoc {
    pub path: Option<PathBuf>,
    pub title: String,
    /// The text in the editor when it differs from the file (or for untitled tabs).
    pub text: Option<String>,
    /// For untitled tabs: the text counted as "saved" (so the welcome tab isn't marked modified).
    pub saved: Option<String>,
    pub lang: String,
    pub scroll_y: f32,
    pub crlf: bool,
    pub bookmarks: Vec<usize>,
    pub preview: bool,
}

/// Where settings live: %APPDATA% on Windows, ~/.config (or $XDG_CONFIG_HOME) on Linux.
pub fn config_base() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn session_file() -> PathBuf {
    config_base().join("TextCraft").join("session.json")
}

/// The session kept by the app under its former name, picked up once after the rename.
fn legacy_session_file() -> PathBuf {
    config_base().join("AtelierTexte").join("session.json")
}

fn lang_from_debug(s: &str) -> Lang {
    Lang::ALL.into_iter().find(|l| format!("{l:?}") == s).unwrap_or(Lang::Plain)
}

impl App {
    pub fn session_json(&self) -> String {
        let docs = self
            .docs
            .iter()
            .filter(|d| d.remote.is_none() && !(d.path.is_none() && d.text.is_empty()))
            .map(|d| SavedDoc {
                path: d.path.clone(),
                title: d.title.clone(),
                text: if d.path.is_none() || d.dirty() { Some(d.text.clone()) } else { None },
                saved: if d.path.is_none() { Some(d.saved.clone()) } else { None },
                lang: format!("{:?}", d.lang),
                scroll_y: d.scroll_y,
                crlf: d.crlf,
                bookmarks: d.bookmarks.clone(),
                preview: d.preview,
            })
            .collect::<Vec<_>>();
        let active = self.docs[..self.active.min(self.docs.len())]
            .iter()
            .filter(|d| d.remote.is_none() && !(d.path.is_none() && d.text.is_empty()))
            .count()
            .min(docs.len().saturating_sub(1));
        let s = Session {
            docs,
            active,
            folder: self.folder.clone(),
            dark: self.dark,
            font_size: self.font_size,
            sidebar: self.sidebar,
            minimap: self.minimap,
            ruled: self.ruled,
            find_query: self.find_query.clone(),
            show_tabs: Some(self.show_tabs),
            show_status: Some(self.show_status),
            wrap: self.wrap,
            ruler: self.ruler,
            tab_size: self.tab_size,
            recent_files: self.recent_files.clone(),
            recent_folders: self.recent_folders.clone(),
            md_auto: Some(self.md_auto),
            share_enabled: self.share_enabled,
            share_code: self.share_code.clone(),
            share_name: self.share_name.clone(),
            share_hint: self.share_hint.clone(),
        };
        serde_json::to_string(&s).unwrap_or_default()
    }

    /// Writes the session if it changed since the last write.
    pub fn save_session(&mut self) {
        if self.shot_path.is_some() {
            return; // screenshot runs never touch the person's session
        }
        let json = self.session_json();
        if json == self.last_session {
            return;
        }
        let file = session_file();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = file.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() && std::fs::rename(&tmp, &file).is_ok() {
            self.last_session = json;
        }
    }

    /// Restores the previous session. Returns false when there was none.
    pub fn load_session(&mut self) -> bool {
        let Ok(json) = std::fs::read_to_string(session_file()).or_else(|_| std::fs::read_to_string(legacy_session_file()))
        else {
            return false;
        };
        let Ok(s) = serde_json::from_str::<Session>(&json) else { return false };
        self.dark = s.dark;
        if s.font_size >= 8.0 {
            self.font_size = s.font_size.min(40.0);
        }
        self.sidebar = s.sidebar;
        self.minimap = s.minimap;
        self.ruled = s.ruled;
        self.find_query = s.find_query;
        self.show_tabs = s.show_tabs.unwrap_or(true);
        self.show_status = s.show_status.unwrap_or(true);
        self.wrap = s.wrap;
        self.md_auto = s.md_auto.unwrap_or(true);
        self.share_enabled = s.share_enabled;
        self.share_code = s.share_code.clone();
        self.share_name = s.share_name.clone();
        self.share_hint = s.share_hint.clone();
        self.ruler = s.ruler;
        if (1..=16).contains(&s.tab_size) {
            self.tab_size = s.tab_size;
        }
        let recent_files = s.recent_files;
        let recent_folders = s.recent_folders;
        if let Some(f) = s.folder.filter(|f| f.is_dir()) {
            self.open_folder(f);
            self.sidebar = s.sidebar;
        }
        for sd in s.docs {
            let id = self.alloc_id();
            let mut doc = match &sd.path {
                Some(path) => {
                    let on_disk = std::fs::read(path).ok().map(|b| String::from_utf8_lossy(&b).replace("\r\n", "\n"));
                    let text = sd.text.clone().or_else(|| on_disk.clone());
                    let Some(text) = text else { continue }; // file gone and nothing unsaved: drop the tab
                    let mut d = Doc::new(id, sd.title.clone(), text, Some(path.clone()));
                    // Unsaved changes stay marked as unsaved; a deleted file counts as unsaved too.
                    d.saved = on_disk.unwrap_or_else(|| String::from("\u{0}"));
                    d
                }
                None => {
                    let text = sd.text.clone().unwrap_or_default();
                    let mut d = Doc::new(id, sd.title.clone(), text, None);
                    d.saved = sd.saved.clone().unwrap_or_default();
                    d
                }
            };
            doc.lang = lang_from_debug(&sd.lang);
            doc.crlf = sd.crlf;
            doc.bookmarks = sd.bookmarks.clone();
            doc.preview = sd.preview;
            if sd.scroll_y > 0.0 {
                doc.set_scroll = Some(sd.scroll_y);
            }
            self.docs.push(doc);
        }
        // Opening things above already touched the recent lists: put the saved ones back.
        self.recent_files = recent_files;
        self.recent_folders = recent_folders;
        if self.docs.is_empty() {
            return false;
        }
        self.active = s.active.min(self.docs.len() - 1);
        self.last_session = json;
        true
    }
}
