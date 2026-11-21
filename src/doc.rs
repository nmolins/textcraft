use std::path::{Path, PathBuf};

use eframe::egui::{self, text::CCursor, text::CCursorRange, text_edit::TextEditState, Id};

use crate::highlight::Lang;

// ---------------------------------------------------------------- documents

pub struct Doc {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub title: String,
    pub text: String,
    pub saved: String,
    pub lang: Lang,
    pub line: usize,
    pub col: usize,
    pub selected: usize,
    pub row_h: f32,
    pub scroll_y: f32,
    pub view_h: f32,
    pub content_h: f32,
    pub set_scroll: Option<f32>,
    /// Width of the longest line last time it was drawn (the page is made that wide).
    pub text_w: f32,
    pub scroll_x: f32,
    pub content_w: f32,
    pub view_w: f32,
    pub crlf: bool,
    pub bookmarks: Vec<usize>,
    pub preview: bool,
    /// (computer id, tab id) when this tab shows another computer's tab, read-only.
    pub remote: Option<(String, u64)>,
    pub remote_from: String,
    pub remote_rev: u64,
    pub remote_online: bool,
}

impl Doc {
    pub fn new(id: u64, title: String, text: String, path: Option<PathBuf>) -> Self {
        let lang = Lang::from_name(&title);
        Doc {
            id,
            path,
            title,
            saved: text.clone(),
            text,
            lang,
            line: 0,
            col: 0,
            selected: 0,
            row_h: 18.0,
            scroll_y: 0.0,
            view_h: 400.0,
            content_h: 400.0,
            set_scroll: None,
            text_w: 0.0,
            scroll_x: 0.0,
            content_w: 0.0,
            view_w: 0.0,
            crlf: false,
            bookmarks: Vec::new(),
            preview: false,
            remote: None,
            remote_from: String::new(),
            remote_rev: 0,
            remote_online: false,
        }
    }
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }
    pub fn line_count(&self) -> usize {
        self.text.split('\n').count()
    }
}

pub fn editor_id(doc_id: u64) -> Id {
    Id::new(("editor", doc_id))
}

// ---------------------------------------------------------------- folder tree

pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub children: Option<Vec<Node>>, // None = file
}

pub fn scan_dir(dir: &Path, depth: usize, budget: &mut usize) -> Vec<Node> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!is_dir, e.file_name().to_string_lossy().to_lowercase())
    });
    for e in entries {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if matches!(name.as_str(), ".git" | "target" | "node_modules" | "__pycache__") {
                continue;
            }
            let children = if depth < 8 { scan_dir(&path, depth + 1, budget) } else { Vec::new() };
            out.push(Node { name, path, children: Some(children) });
        } else {
            out.push(Node { name, path, children: None });
        }
    }
    out
}

pub fn flatten_files(nodes: &[Node], root: &Path, out: &mut Vec<(String, PathBuf)>) {
    for n in nodes {
        match &n.children {
            Some(c) => flatten_files(c, root, out),
            None => {
                let rel = n.path.strip_prefix(root).unwrap_or(&n.path).to_string_lossy().replace('\\', "/");
                out.push((rel, n.path.clone()));
            }
        }
    }
}

// ---------------------------------------------------------------- text helpers

pub fn char_to_byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}

pub fn line_of(s: &str, ci: usize) -> usize {
    s.chars().take(ci).filter(|c| *c == '\n').count()
}

pub fn col_of(s: &str, ci: usize) -> usize {
    let mut col = 0;
    for c in s.chars().take(ci) {
        if c == '\n' {
            col = 0;
        } else {
            col += 1;
        }
    }
    col
}

pub fn line_start_char(s: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut n = 0;
    for (i, c) in s.chars().enumerate() {
        if c == '\n' {
            n += 1;
            if n == line {
                return i + 1;
            }
        }
    }
    s.chars().count()
}

/// (anchor, cursor) of the editor's selection, in characters.
pub fn get_sel(ctx: &egui::Context, id: Id) -> Option<(usize, usize)> {
    TextEditState::load(ctx, id)
        .and_then(|s| s.cursor.char_range())
        .map(|r| (r.secondary.index.0, r.primary.index.0))
}

pub fn set_sel(ctx: &egui::Context, id: Id, a: usize, b: usize) {
    let mut st = TextEditState::load(ctx, id).unwrap_or_default();
    let mut range = CCursorRange::one(CCursor::new(b));
    range.secondary = CCursor::new(a);
    st.cursor.set_char_range(Some(range));
    st.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

pub fn select_lines(ctx: &egui::Context, doc: &Doc, l0: usize, l1: usize) {
    let s = line_start_char(&doc.text, l0);
    let e = line_start_char(&doc.text, l1) + doc.text.split('\n').nth(l1).map_or(0, |l| l.chars().count());
    set_sel(ctx, editor_id(doc.id), s, e);
}

pub fn find_exact(ctx: &egui::Context, doc: &mut Doc, q: &str, from: usize, backwards: bool) -> bool {
    let fb = char_to_byte(&doc.text, from);
    let found = if backwards {
        doc.text[..fb].rfind(q).or_else(|| doc.text.rfind(q))
    } else {
        doc.text[fb..].find(q).map(|x| x + fb).or_else(|| doc.text.find(q))
    };
    if let Some(pos) = found {
        let s = doc.text[..pos].chars().count();
        set_sel(ctx, editor_id(doc.id), s, s + q.chars().count());
        let line = line_of(&doc.text, s);
        doc.set_scroll = Some(line as f32 * doc.row_h - doc.view_h / 3.0);
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------- fuzzy matching

pub fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = target.to_lowercase().chars().collect();
    let mut score = 0;
    let mut ti = 0;
    let mut last = 0usize;
    for qc in query.to_lowercase().chars() {
        if qc == ' ' {
            continue;
        }
        let mut found = false;
        while ti < t.len() {
            if t[ti] == qc {
                score += if ti == last + 1 { 3 } else { 1 };
                if ti == 0 || !t[ti - 1].is_alphanumeric() {
                    score += 2;
                }
                last = ti;
                ti += 1;
                found = true;
                break;
            }
            ti += 1;
        }
        if !found {
            return None;
        }
    }
    Some(score * 10 - t.len() as i32 / 4)
}
