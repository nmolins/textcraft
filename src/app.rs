use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::doc::*;
use crate::highlight::Lang;
use crate::theme::{self, palette, Palette};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    // Fichier
    NewFile,
    OpenFile,
    OpenFolder,
    OpenRecent(usize),
    Save,
    SaveAs,
    SaveAll,
    CloseTab,
    RevertFile,
    CloseAll,
    Quit,
    // Édition
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Indent,
    Unindent,
    DuplicateLine,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    JoinLines,
    ToggleComment,
    InsertLineBefore,
    InsertLineAfter,
    Upper,
    Lower,
    TitleCase,
    SwapCase,
    SortLines,
    SortLinesCase,
    ReverseLines,
    UniqueLines,
    // Sélection
    SelectAll,
    ExpandLine,
    ExpandWord,
    ExpandBrackets,
    ExpandParagraph,
    // Rechercher
    Find,
    FindNext,
    FindPrev,
    Replace,
    ReplaceNext,
    QuickFind,
    UseSelFind,
    UseSelReplace,
    FindInFiles,
    // Affichage
    ToggleSidebar,
    ToggleMinimap,
    ToggleTabs,
    ShareSettings,
    RenameTab,
    RemoteCopy,
    TogglePreview,
    ToggleAutoPreview,
    ToggleStatus,
    FullScreen,
    DistractionFree,
    SetLang(Lang),
    TabSize(u8),
    ConvertIndent,
    LineEndings(bool),
    WordWrap,
    Ruler(u16),
    // Aller
    QuickOpen,
    GotoSymbol,
    GotoLine,
    JumpBack,
    JumpForward,
    NextTab,
    PrevTab,
    SwitchTo(usize),
    ToggleBookmark,
    NextBookmark,
    PrevBookmark,
    ClearBookmarks,
    MatchingBracket,
    // Outils / Projet / Préférences / Aide
    Palette,
    RecentFolder(usize),
    RefreshFolder,
    CloseFolder,
    ThemePaper,
    ThemeNight,
    FontBigger,
    FontSmaller,
    FontReset,
    ToggleRuled,
    Shortcuts,
    About,
}

impl Cmd {
    /// Commands that change the text (refused on another computer's tab).
    pub fn edits(self) -> bool {
        matches!(
            self,
            Cmd::Save
                | Cmd::SaveAs
                | Cmd::RevertFile
                | Cmd::Undo
                | Cmd::Redo
                | Cmd::Cut
                | Cmd::Paste
                | Cmd::Indent
                | Cmd::Unindent
                | Cmd::DuplicateLine
                | Cmd::DeleteLine
                | Cmd::MoveLineUp
                | Cmd::MoveLineDown
                | Cmd::JoinLines
                | Cmd::ToggleComment
                | Cmd::InsertLineBefore
                | Cmd::InsertLineAfter
                | Cmd::Upper
                | Cmd::Lower
                | Cmd::TitleCase
                | Cmd::SwapCase
                | Cmd::SortLines
                | Cmd::SortLinesCase
                | Cmd::ReverseLines
                | Cmd::UniqueLines
                | Cmd::Replace
                | Cmd::ReplaceNext
                | Cmd::UseSelReplace
                | Cmd::ConvertIndent
                | Cmd::RenameTab
        )
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum PaletteMode {
    Commands,
    Files,
    Line,
    Symbols,
    InFiles,
}

/// One line of a "find in files" result.
pub struct Hit {
    pub label: String,
    pub hint: String,
    pub path: PathBuf,
    pub line: usize,
}

pub struct PaletteState {
    pub mode: PaletteMode,
    pub query: String,
    pub selected: usize,
    pub just_opened: bool,
    pub hits: Vec<Hit>,
    pub searched: Option<String>,
    pub typed_at: f64,
}

pub enum Pending {
    CloseTab(usize),
    Quit,
}

pub struct App {
    pub docs: Vec<Doc>,
    pub active: usize,
    pub next_id: u64,
    pub dark: bool,
    pub font_size: f32,
    pub sidebar: bool,
    pub minimap: bool,
    pub ruled: bool,
    pub show_tabs: bool,
    pub md_auto: bool,
    pub net: crate::net::Net,
    pub share_enabled: bool,
    pub share_code: String,
    pub share_name: String,
    pub share_hint: String,
    pub share_show_code: bool,
    pub show_share: bool,
    /// Tab being renamed: (doc id, new name being typed, focus the field next frame).
    pub renaming: Option<(u64, String, bool)>,
    pub share_last_sync: f64,
    pub md_fonts: bool,
    pub md_images: crate::markdown::Images,
    pub preview_last_scroll: f32,
    pub preview_max: f32,
    pub show_status: bool,
    pub wrap: bool,
    pub ruler: u16,
    pub tab_size: u8,
    pub fullscreen: bool,
    pub distraction: bool,
    pub show_keys: bool,
    pub show_about: bool,
    pub recent_files: Vec<PathBuf>,
    pub recent_folders: Vec<PathBuf>,
    pub back: Vec<(u64, usize)>,
    pub forward: Vec<(u64, usize)>,
    pub folder: Option<PathBuf>,
    pub tree: Vec<Node>,
    pub palette: Option<PaletteState>,
    pub find_open: bool,
    pub replace_open: bool,
    pub find_query: String,
    pub replace_with: String,
    pub match_case: bool,
    pub find_focus: bool,
    pub message: Option<(String, f64)>,
    pub pending: Option<Pending>,
    pub allow_quit: bool,
    pub shot_path: Option<PathBuf>,
    pub frame: u32,
    pub shot_requested: bool,
    pub last_session: String,
    pub last_session_time: f64,
    pub shot_slot: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>,
}

pub const WELCOME: &str = include_str!("welcome.rs.txt");

fn push_recent(list: &mut Vec<PathBuf>, p: &Path) {
    list.retain(|x| x != p);
    list.insert(0, p.to_path_buf());
    list.truncate(12);
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>, shot_path: Option<PathBuf>) -> Self {
        let mut app = App {
            docs: Vec::new(),
            active: 0,
            next_id: 1,
            dark: false,
            font_size: 14.0,
            sidebar: true,
            minimap: true,
            ruled: true,
            show_tabs: true,
            md_auto: true,
            net: crate::net::Net::new(),
            share_enabled: false,
            share_code: String::new(),
            share_name: String::new(),
            share_hint: String::new(),
            share_show_code: false,
            show_share: false,
            renaming: None,
            share_last_sync: -10.0,
            md_fonts: false,
            md_images: Default::default(),
            preview_last_scroll: -1.0,
            preview_max: 0.0,
            show_status: true,
            wrap: false,
            ruler: 0,
            tab_size: 4,
            fullscreen: false,
            distraction: false,
            show_keys: false,
            show_about: false,
            recent_files: Vec::new(),
            recent_folders: Vec::new(),
            back: Vec::new(),
            forward: Vec::new(),
            folder: None,
            tree: Vec::new(),
            palette: None,
            find_open: false,
            replace_open: false,
            find_query: String::new(),
            replace_with: String::new(),
            match_case: false,
            find_focus: false,
            message: None,
            pending: None,
            allow_quit: false,
            shot_path,
            frame: 0,
            shot_requested: false,
            last_session: String::new(),
            last_session_time: 0.0,
            shot_slot: Default::default(),
        };
        // Pick up where the person left off (never in screenshot runs).
        if app.shot_path.is_none() {
            app.load_session();
        }
        for f in files {
            if f.is_dir() {
                app.open_folder(f);
            } else {
                app.open_path(&f);
            }
        }
        if app.docs.is_empty() {
            let id = app.alloc_id();
            app.docs.push(Doc::new(id, "bienvenue.rs".into(), WELCOME.replace("\r\n", "\n"), None));
        }
        app.md_fonts = crate::markdown::install_fonts(&cc.egui_ctx);
        // Test aid: TEXTCRAFT_TEST_SHARE="code" turns sharing on with that code.
        if let Ok(code) = std::env::var("TEXTCRAFT_TEST_SHARE") {
            app.share_enabled = true;
            app.share_code = code;
            app.share_name = std::env::var("TEXTCRAFT_TEST_NAME").unwrap_or_default();
        }
        if app.share_enabled {
            app.apply_share(&cc.egui_ctx);
        }
        theme::apply(&cc.egui_ctx, app.pal());
        app
    }

    pub fn pal(&self) -> &'static Palette {
        palette(self.dark)
    }

    pub fn alloc_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn flash(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.message = Some((msg.into(), ctx.input(|i| i.time) + 3.0));
    }

    pub fn doc(&self) -> &Doc {
        &self.docs[self.active]
    }

    pub fn open_folder(&mut self, dir: PathBuf) {
        let mut budget = 5000;
        self.tree = scan_dir(&dir, 0, &mut budget);
        push_recent(&mut self.recent_folders, &dir);
        self.folder = Some(dir);
        self.sidebar = true;
    }

    pub fn open_path(&mut self, path: &Path) {
        push_recent(&mut self.recent_files, path);
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(path)) {
            self.active = i;
            return;
        }
        let raw = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };
        let crlf = raw.contains("\r\n");
        let text = raw.replace("\r\n", "\n");
        let title = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let id = self.alloc_id();
        let mut doc = Doc::new(id, title, text, Some(path.to_path_buf()));
        doc.crlf = crlf;
        doc.preview = self.md_auto && doc.lang == Lang::Markdown;
        if !path.exists() {
            doc.saved = String::from("\u{0}"); // a new file named on the command line counts as unsaved
        }
        // Replace an untouched welcome/scratch tab
        if self.docs.len() == 1 && self.docs[0].path.is_none() && !self.docs[0].dirty() {
            self.docs.clear();
        }
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
    }

    pub fn save(&mut self, ctx: &egui::Context, idx: usize, ask: bool) -> bool {
        let Some(doc) = self.docs.get_mut(idx) else { return false };
        let path = if ask || doc.path.is_none() {
            let mut dlg = rfd::FileDialog::new().set_file_name(&doc.title);
            if let Some(dir) = doc.path.as_ref().and_then(|p| p.parent()) {
                dlg = dlg.set_directory(dir);
            }
            match dlg.save_file() {
                Some(p) => p,
                None => return false,
            }
        } else {
            doc.path.clone().unwrap()
        };
        let out = if doc.crlf { doc.text.replace('\n', "\r\n") } else { doc.text.clone() };
        match std::fs::write(&path, out) {
            Ok(()) => {
                doc.saved = doc.text.clone();
                doc.title = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if doc.lang == Lang::Plain {
                    doc.lang = Lang::from_name(&doc.title);
                }
                doc.path = Some(path.clone());
                let t = doc.title.clone();
                push_recent(&mut self.recent_files, &path);
                self.flash(ctx, format!("Enregistré : {t}"));
                true
            }
            Err(e) => {
                self.flash(ctx, format!("Impossible d'enregistrer : {e}"));
                false
            }
        }
    }

    pub fn close_tab(&mut self, idx: usize) {
        if idx >= self.docs.len() {
            return;
        }
        self.docs.remove(idx);
        if self.docs.is_empty() {
            let id = self.alloc_id();
            self.docs.push(Doc::new(id, "sans titre".into(), String::new(), None));
        }
        if self.active >= self.docs.len() {
            self.active = self.docs.len() - 1;
        } else if self.active > idx {
            self.active -= 1;
        }
    }

    pub fn request_close(&mut self, idx: usize) {
        if self.docs.get(idx).map_or(false, |d| d.dirty()) {
            self.pending = Some(Pending::CloseTab(idx));
        } else {
            self.close_tab(idx);
        }
    }

    pub fn focus_editor(&self, ctx: &egui::Context) {
        if let Some(d) = self.docs.get(self.active) {
            ctx.memory_mut(|m| m.request_focus(editor_id(d.id)));
        }
    }

    pub fn run(&mut self, ctx: &egui::Context, cmd: Cmd) {
        if self.doc().remote.is_some() && cmd.edits() {
            if matches!(cmd, Cmd::Save | Cmd::SaveAs) {
                // Saving a remote tab saves a local copy of it.
                self.remote_copy();
                self.save(ctx, self.active, true);
            } else {
                self.flash(ctx, "Onglet d'un autre ordinateur : lecture seule (« Ouvrir une copie modifiable » pour le changer)");
            }
            return;
        }
        match cmd {
            Cmd::NewFile => {
                let id = self.alloc_id();
                self.docs.push(Doc::new(id, "sans titre".into(), String::new(), None));
                self.active = self.docs.len() - 1;
                self.focus_editor(ctx);
            }
            Cmd::OpenFile => {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    for f in files {
                        self.open_path(&f);
                    }
                }
            }
            Cmd::OpenFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(dir);
                }
            }
            Cmd::OpenRecent(i) => {
                if let Some(p) = self.recent_files.get(i).cloned() {
                    if p.exists() {
                        self.open_path(&p);
                        self.focus_editor(ctx);
                    } else {
                        self.recent_files.remove(i);
                        self.flash(ctx, "Ce fichier n'existe plus");
                    }
                }
            }
            Cmd::RecentFolder(i) => {
                if let Some(p) = self.recent_folders.get(i).cloned() {
                    if p.is_dir() {
                        self.open_folder(p);
                    } else {
                        self.recent_folders.remove(i);
                        self.flash(ctx, "Ce dossier n'existe plus");
                    }
                }
            }
            Cmd::Save => {
                self.save(ctx, self.active, false);
            }
            Cmd::SaveAs => {
                self.save(ctx, self.active, true);
            }
            Cmd::SaveAll => {
                for i in 0..self.docs.len() {
                    if self.docs[i].dirty() {
                        self.save(ctx, i, false);
                    }
                }
            }
            Cmd::CloseTab => self.request_close(self.active),
            Cmd::RevertFile => self.revert(ctx),
            Cmd::CloseAll => {
                let kept = self.docs.iter().filter(|d| d.dirty()).count();
                let mut i = self.docs.len();
                while i > 0 {
                    i -= 1;
                    if !self.docs[i].dirty() {
                        self.close_tab(i);
                    }
                }
                if kept > 0 {
                    self.flash(ctx, format!("{kept} fichier(s) non enregistré(s) gardé(s) ouvert(s)"));
                }
            }
            Cmd::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Cmd::Undo => self.undo_redo(ctx, false),
            Cmd::Redo => self.undo_redo(ctx, true),
            Cmd::Cut => self.cut_copy(ctx, true),
            Cmd::Copy => self.cut_copy(ctx, false),
            Cmd::Paste => self.paste(ctx),
            Cmd::Indent => self.indent_lines(ctx, false),
            Cmd::Unindent => self.indent_lines(ctx, true),
            Cmd::DuplicateLine => self.duplicate_line(ctx),
            Cmd::DeleteLine => self.delete_line(ctx),
            Cmd::MoveLineUp => self.move_lines(ctx, true),
            Cmd::MoveLineDown => self.move_lines(ctx, false),
            Cmd::JoinLines => self.join_lines(ctx),
            Cmd::ToggleComment => self.toggle_comment(ctx),
            Cmd::InsertLineBefore => self.insert_line(ctx, true),
            Cmd::InsertLineAfter => self.insert_line(ctx, false),
            Cmd::Upper | Cmd::Lower | Cmd::TitleCase | Cmd::SwapCase => self.change_case(ctx, cmd),
            Cmd::SortLines | Cmd::SortLinesCase | Cmd::ReverseLines | Cmd::UniqueLines => self.permute_lines(ctx, cmd),
            Cmd::SelectAll => {
                let d = self.doc();
                set_sel(ctx, editor_id(d.id), 0, d.text.chars().count());
            }
            Cmd::ExpandLine => self.expand_line(ctx),
            Cmd::ExpandWord => self.expand_word(ctx),
            Cmd::ExpandBrackets => self.expand_brackets(ctx),
            Cmd::ExpandParagraph => self.expand_paragraph(ctx),
            Cmd::Find | Cmd::Replace => {
                self.find_open = true;
                self.replace_open = cmd == Cmd::Replace;
                self.find_focus = true;
                self.prefill_find(ctx);
            }
            Cmd::FindNext | Cmd::FindPrev => {
                if self.find_query.is_empty() {
                    self.run(ctx, Cmd::Find);
                } else if !self.find_next(ctx, cmd == Cmd::FindPrev) {
                    self.flash(ctx, "Introuvable");
                }
            }
            Cmd::ReplaceNext => {
                if self.find_query.is_empty() {
                    self.run(ctx, Cmd::Replace);
                } else {
                    self.replace_one(ctx);
                }
            }
            Cmd::QuickFind => {
                if let Some(w) = self.selection_or_word(ctx) {
                    self.find_query = w;
                    self.find_next(ctx, false);
                }
            }
            Cmd::UseSelFind => {
                if let Some(w) = self.selection_or_word(ctx) {
                    self.find_query = w;
                    self.flash(ctx, format!("Recherche : « {} »", self.find_query));
                }
            }
            Cmd::UseSelReplace => {
                if let Some(w) = self.selection_or_word(ctx) {
                    self.replace_with = w;
                    self.flash(ctx, format!("Remplacer par : « {} »", self.replace_with));
                }
            }
            Cmd::FindInFiles => self.open_palette(PaletteMode::InFiles),
            Cmd::ToggleSidebar => self.sidebar = !self.sidebar,
            Cmd::ToggleMinimap => self.minimap = !self.minimap,
            Cmd::ToggleTabs => self.show_tabs = !self.show_tabs,
            Cmd::ShareSettings => self.show_share = true,
            Cmd::RenameTab => self.start_rename(self.active),
            Cmd::RemoteCopy => {
                if self.doc().remote.is_some() {
                    self.remote_copy();
                }
            }
            Cmd::TogglePreview => {
                let d = &mut self.docs[self.active];
                if d.lang != Lang::Markdown {
                    d.lang = Lang::Markdown;
                    d.preview = true;
                } else {
                    d.preview = !d.preview;
                }
                self.preview_last_scroll = -1.0;
            }
            Cmd::ToggleAutoPreview => self.md_auto = !self.md_auto,
            Cmd::ToggleStatus => self.show_status = !self.show_status,
            Cmd::FullScreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            Cmd::DistractionFree => {
                self.distraction = !self.distraction;
                self.fullscreen = self.distraction;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                if self.distraction {
                    self.flash(ctx, "Maj+F11 pour quitter le mode sans distraction");
                }
            }
            Cmd::SetLang(l) => self.docs[self.active].lang = l,
            Cmd::TabSize(n) => self.tab_size = n,
            Cmd::ConvertIndent => self.convert_indent(ctx),
            Cmd::LineEndings(crlf) => self.docs[self.active].crlf = crlf,
            Cmd::WordWrap => self.wrap = !self.wrap,
            Cmd::Ruler(c) => self.ruler = c,
            Cmd::QuickOpen => self.open_palette(PaletteMode::Files),
            Cmd::GotoSymbol => self.open_palette(PaletteMode::Symbols),
            Cmd::GotoLine => self.open_palette(PaletteMode::Line),
            Cmd::JumpBack => self.jump_history(ctx, true),
            Cmd::JumpForward => self.jump_history(ctx, false),
            Cmd::NextTab => self.active = (self.active + 1) % self.docs.len(),
            Cmd::PrevTab => self.active = (self.active + self.docs.len() - 1) % self.docs.len(),
            Cmd::SwitchTo(i) => {
                if i < self.docs.len() {
                    self.active = i;
                    self.focus_editor(ctx);
                }
            }
            Cmd::ToggleBookmark => {
                let d = &mut self.docs[self.active];
                let l = d.line;
                if let Some(pos) = d.bookmarks.iter().position(|b| *b == l) {
                    d.bookmarks.remove(pos);
                } else {
                    d.bookmarks.push(l);
                    d.bookmarks.sort_unstable();
                }
            }
            Cmd::NextBookmark => self.bookmark_jump(ctx, true),
            Cmd::PrevBookmark => self.bookmark_jump(ctx, false),
            Cmd::ClearBookmarks => self.docs[self.active].bookmarks.clear(),
            Cmd::MatchingBracket => self.matching_bracket(ctx),
            Cmd::Palette => self.open_palette(PaletteMode::Commands),
            Cmd::RefreshFolder => {
                if let Some(f) = self.folder.clone() {
                    self.open_folder(f);
                }
            }
            Cmd::CloseFolder => {
                self.folder = None;
                self.tree.clear();
            }
            Cmd::ThemePaper | Cmd::ThemeNight => {
                self.dark = cmd == Cmd::ThemeNight;
                theme::apply(ctx, self.pal());
            }
            Cmd::FontBigger => self.font_size = (self.font_size + 1.0).min(40.0),
            Cmd::FontSmaller => self.font_size = (self.font_size - 1.0).max(8.0),
            Cmd::FontReset => self.font_size = 14.0,
            Cmd::ToggleRuled => self.ruled = !self.ruled,
            Cmd::Shortcuts => self.show_keys = true,
            Cmd::About => self.show_about = true,
        }
    }

    /// Whether a menu entry shows a check mark (None = not a toggle).
    pub fn checked(&self, cmd: Cmd) -> Option<bool> {
        let d = self.doc();
        Some(match cmd {
            Cmd::ToggleSidebar => self.sidebar,
            Cmd::ToggleMinimap => self.minimap,
            Cmd::ToggleTabs => self.show_tabs,
            Cmd::TogglePreview => d.preview && d.lang == Lang::Markdown,
            Cmd::ToggleAutoPreview => self.md_auto,
            Cmd::ToggleStatus => self.show_status,
            Cmd::FullScreen => self.fullscreen,
            Cmd::DistractionFree => self.distraction,
            Cmd::WordWrap => self.wrap,
            Cmd::ToggleRuled => self.ruled,
            Cmd::SetLang(l) => d.lang == l,
            Cmd::TabSize(n) => self.tab_size == n,
            Cmd::Ruler(c) => self.ruler == c,
            Cmd::LineEndings(c) => d.crlf == c,
            Cmd::ThemePaper => !self.dark,
            Cmd::ThemeNight => self.dark,
            Cmd::SwitchTo(i) => i == self.active,
            _ => return None,
        })
    }

    /// Whether a menu entry can be used right now.
    pub fn enabled(&self, ctx: &egui::Context, cmd: Cmd) -> bool {
        let d = self.doc();
        match cmd {
            Cmd::Undo | Cmd::Redo => self.can_undo_redo(ctx, cmd == Cmd::Redo),
            Cmd::Cut | Cmd::Copy => get_sel(ctx, editor_id(d.id)).map_or(false, |(a, b)| a != b),
            Cmd::RevertFile => d.path.is_some() && d.dirty(),
            Cmd::JumpBack => !self.back.is_empty(),
            Cmd::JumpForward => !self.forward.is_empty(),
            Cmd::NextBookmark | Cmd::PrevBookmark | Cmd::ClearBookmarks => !d.bookmarks.is_empty(),
            Cmd::CloseFolder | Cmd::RefreshFolder => self.folder.is_some(),
            Cmd::ToggleComment => d.lang.comment_token().is_some(),
            _ => true,
        }
    }

    pub fn open_palette(&mut self, mode: PaletteMode) {
        let query = if mode == PaletteMode::InFiles { self.find_query.clone() } else { String::new() };
        self.palette = Some(PaletteState {
            mode,
            query,
            selected: 0,
            just_opened: true,
            hits: Vec::new(),
            searched: None,
            typed_at: 0.0,
        });
    }

    fn prefill_find(&mut self, ctx: &egui::Context) {
        if let Some(sel) = self.selection_text(ctx) {
            if !sel.is_empty() && !sel.contains('\n') {
                self.find_query = sel;
            }
        }
    }

    // ------------------------------------------------ keyboard

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        let c = Modifiers::COMMAND;
        let cs = Modifiers::COMMAND | Modifiers::SHIFT;
        let ca = Modifiers::COMMAND | Modifiers::ALT;
        let s = Modifiers::SHIFT;
        let a = Modifiers::ALT;
        let as_ = Modifiers::ALT | Modifiers::SHIFT;
        let n = Modifiers::NONE;
        let mut table: Vec<(Modifiers, Key, Cmd)> = vec![
            (cs, Key::P, Cmd::Palette),
            (Modifiers::ALT, Key::M, Cmd::TogglePreview),
            (cs, Key::S, Cmd::SaveAs),
            (cs, Key::O, Cmd::OpenFolder),
            (cs, Key::D, Cmd::DuplicateLine),
            (cs, Key::K, Cmd::DeleteLine),
            (cs, Key::Tab, Cmd::PrevTab),
            (cs, Key::ArrowUp, Cmd::MoveLineUp),
            (cs, Key::ArrowDown, Cmd::MoveLineDown),
            (cs, Key::Enter, Cmd::InsertLineBefore),
            (cs, Key::M, Cmd::ExpandBrackets),
            (cs, Key::H, Cmd::ReplaceNext),
            (cs, Key::E, Cmd::UseSelReplace),
            (cs, Key::F, Cmd::FindInFiles),
            (cs, Key::R, Cmd::GotoSymbol),
            (cs, Key::W, Cmd::Quit),
            (cs, Key::F2, Cmd::ClearBookmarks),
            (cs, Key::U, Cmd::Lower),
            (ca, Key::U, Cmd::Upper),
            (c, Key::Tab, Cmd::NextTab),
            (c, Key::P, Cmd::QuickOpen),
            (c, Key::N, Cmd::NewFile),
            (c, Key::O, Cmd::OpenFile),
            (c, Key::S, Cmd::Save),
            (c, Key::W, Cmd::CloseTab),
            (c, Key::F, Cmd::Find),
            (c, Key::H, Cmd::Replace),
            (c, Key::G, Cmd::GotoLine),
            (c, Key::R, Cmd::GotoSymbol),
            (c, Key::B, Cmd::ToggleSidebar),
            (c, Key::Slash, Cmd::ToggleComment),
            (c, Key::Equals, Cmd::FontBigger),
            (c, Key::Plus, Cmd::FontBigger),
            (c, Key::Minus, Cmd::FontSmaller),
            (c, Key::Num0, Cmd::FontReset),
            (c, Key::CloseBracket, Cmd::Indent),
            (c, Key::OpenBracket, Cmd::Unindent),
            (c, Key::J, Cmd::JoinLines),
            (c, Key::Enter, Cmd::InsertLineAfter),
            (c, Key::L, Cmd::ExpandLine),
            (c, Key::D, Cmd::ExpandWord),
            (c, Key::E, Cmd::UseSelFind),
            (c, Key::M, Cmd::MatchingBracket),
            (c, Key::F3, Cmd::QuickFind),
            (c, Key::F2, Cmd::ToggleBookmark),
            (c, Key::F9, Cmd::SortLinesCase),
            (s, Key::F3, Cmd::FindPrev),
            (s, Key::F2, Cmd::PrevBookmark),
            (s, Key::F11, Cmd::DistractionFree),
            (as_, Key::Minus, Cmd::JumpForward),
            (a, Key::Minus, Cmd::JumpBack),
            (n, Key::F3, Cmd::FindNext),
            (n, Key::F2, Cmd::NextBookmark),
            (n, Key::F9, Cmd::SortLines),
            (n, Key::F11, Cmd::FullScreen),
        ];
        let digits = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        for (i, k) in digits.into_iter().enumerate() {
            table.push((a, k, Cmd::SwitchTo(i)));
        }
        // Most specific shortcuts first, so Ctrl+Shift+F isn't taken by Ctrl+F.
        let weight = |m: &Modifiers| m.ctrl as u8 + m.command as u8 + m.shift as u8 + m.alt as u8;
        table.sort_by_key(|(m, _, _)| std::cmp::Reverse(weight(m)));
        let mut fired = Vec::new();
        ctx.input_mut(|i| {
            for (m, k, cmd) in &table {
                if i.consume_shortcut(&KeyboardShortcut::new(*m, *k)) {
                    fired.push(*cmd);
                }
            }
        });
        for cmd in fired {
            self.run(ctx, cmd);
        }

        // Editor-only keys
        if self.palette.is_some() || self.pending.is_some() {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else { return };
        if doc.remote.is_some() {
            return; // read-only
        }
        let id = editor_id(doc.id);
        if !ctx.memory(|m| m.has_focus(id)) {
            return;
        }
        let (shift_tab, tab, enter) = ctx.input_mut(|i| {
            let st = i.consume_key(Modifiers::SHIFT, Key::Tab);
            let t = !st && i.consume_key(Modifiers::NONE, Key::Tab);
            let e = i.consume_key(Modifiers::NONE, Key::Enter);
            (st, t, e)
        });
        if shift_tab {
            self.indent(ctx, true);
        } else if tab {
            self.indent(ctx, false);
        }
        if enter {
            self.newline_indent(ctx);
        }
    }
}
