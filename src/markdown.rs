//! Live Markdown preview: parses the text with pulldown-cmark into a small tree,
//! then draws it with egui in the paper-and-ink style.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, CornerRadius, FontId, RichText, Stroke, Vec2};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

use crate::highlight::{highlight, Lang};
use crate::theme::Palette;

// ---------------------------------------------------------------- tree

#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    sup: bool,
    link: Option<String>,
}

enum SpanKind {
    Text,
    Code,
    Break,
    Task(bool),
    Image(String),
}

struct Span {
    text: String,
    st: Style,
    kind: SpanKind,
}

pub enum Block {
    Heading(u8, Vec<Span>),
    Para(Vec<Span>),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Vec<Block>>),
    Code(String, String),
    Rule,
    Table(Vec<Vec<Vec<Span>>>, usize),
    Raw(String),
}

struct P<'a> {
    ev: Vec<Event<'a>>,
    i: usize,
}

fn is_inline_tag(t: &Tag) -> bool {
    matches!(
        t,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript | Tag::Subscript | Tag::Link { .. } | Tag::Image { .. }
    )
}

impl<'a> P<'a> {
    fn blocks(&mut self) -> Vec<Block> {
        let mut out = Vec::new();
        while self.i < self.ev.len() {
            match &self.ev[self.i] {
                Event::End(_) => {
                    self.i += 1;
                    break;
                }
                Event::Start(Tag::Paragraph) => {
                    self.i += 1;
                    out.push(Block::Para(self.inlines(false)));
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    let l = *level as u8;
                    self.i += 1;
                    out.push(Block::Heading(l, self.inlines(false)));
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    self.i += 1;
                    out.push(Block::Quote(self.blocks()));
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) => l.split([' ', ',', '{']).next().unwrap_or("").to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    self.i += 1;
                    let mut text = String::new();
                    while self.i < self.ev.len() {
                        match &self.ev[self.i] {
                            Event::End(_) => {
                                self.i += 1;
                                break;
                            }
                            Event::Text(t) => text.push_str(t),
                            _ => {}
                        }
                        self.i += 1;
                    }
                    out.push(Block::Code(lang, text.trim_end_matches('\n').to_string()));
                }
                Event::Start(Tag::List(start)) => {
                    let start = *start;
                    self.i += 1;
                    let mut items = Vec::new();
                    while self.i < self.ev.len() {
                        match &self.ev[self.i] {
                            Event::Start(Tag::Item) => {
                                self.i += 1;
                                items.push(self.blocks());
                            }
                            Event::End(_) => {
                                self.i += 1;
                                break;
                            }
                            _ => self.i += 1,
                        }
                    }
                    out.push(Block::List(start, items));
                }
                Event::Start(Tag::Table(_)) => {
                    self.i += 1;
                    let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
                    let mut head = 0;
                    let mut row: Vec<Vec<Span>> = Vec::new();
                    let mut in_head = false;
                    while self.i < self.ev.len() {
                        match &self.ev[self.i] {
                            Event::Start(Tag::TableHead) => {
                                in_head = true;
                                self.i += 1;
                            }
                            Event::Start(Tag::TableRow) => self.i += 1,
                            Event::Start(Tag::TableCell) => {
                                self.i += 1;
                                row.push(self.inlines(false));
                            }
                            Event::End(pulldown_cmark::TagEnd::TableHead) | Event::End(pulldown_cmark::TagEnd::TableRow) => {
                                rows.push(std::mem::take(&mut row));
                                if in_head {
                                    head = rows.len();
                                    in_head = false;
                                }
                                self.i += 1;
                            }
                            Event::End(_) => {
                                self.i += 1;
                                break;
                            }
                            _ => self.i += 1,
                        }
                    }
                    out.push(Block::Table(rows, head));
                }
                Event::Start(Tag::HtmlBlock) => {
                    self.i += 1;
                    let mut text = String::new();
                    while self.i < self.ev.len() {
                        match &self.ev[self.i] {
                            Event::End(_) => {
                                self.i += 1;
                                break;
                            }
                            Event::Html(t) | Event::Text(t) => text.push_str(t),
                            _ => {}
                        }
                        self.i += 1;
                    }
                    out.push(Block::Raw(text.trim_end().to_string()));
                }
                Event::Start(t) if !is_inline_tag(t) => {
                    // Footnotes, definition lists, metadata…: show their content plainly.
                    self.i += 1;
                    out.extend(self.blocks());
                }
                Event::Rule => {
                    self.i += 1;
                    out.push(Block::Rule);
                }
                Event::DisplayMath(t) => {
                    out.push(Block::Code(String::new(), t.to_string()));
                    self.i += 1;
                }
                Event::Html(t) => {
                    out.push(Block::Raw(t.trim_end().to_string()));
                    self.i += 1;
                }
                _ => {
                    // Inline content straight inside a list item (tight lists).
                    let spans = self.inlines(true);
                    if spans.is_empty() {
                        self.i += 1;
                    } else {
                        out.push(Block::Para(spans));
                    }
                }
            }
        }
        out
    }

    /// Reads inline content. `loose`: stop (without consuming) at the first block-level event.
    fn inlines(&mut self, loose: bool) -> Vec<Span> {
        let mut out = Vec::new();
        let mut stack: Vec<Style> = vec![Style::default()];
        let mut image: Option<(String, String)> = None;
        let mut depth = 0;
        while self.i < self.ev.len() {
            let st = stack.last().cloned().unwrap_or_default();
            match &self.ev[self.i] {
                Event::Start(t) if is_inline_tag(t) => {
                    let mut s = st.clone();
                    match t {
                        Tag::Emphasis => s.italic = true,
                        Tag::Strong => s.bold = true,
                        Tag::Strikethrough => s.strike = true,
                        Tag::Superscript | Tag::Subscript => s.sup = true,
                        Tag::Link { dest_url, .. } => s.link = Some(dest_url.to_string()),
                        Tag::Image { dest_url, .. } => image = Some((dest_url.to_string(), String::new())),
                        _ => {}
                    }
                    stack.push(s);
                    depth += 1;
                }
                Event::End(pulldown_cmark::TagEnd::Image) if depth > 0 => {
                    stack.pop();
                    depth -= 1;
                    if let Some((url, alt)) = image.take() {
                        out.push(Span { text: alt, st, kind: SpanKind::Image(url) });
                    }
                }
                Event::End(_) if depth > 0 => {
                    stack.pop();
                    depth -= 1;
                }
                Event::End(_) => {
                    if !loose {
                        self.i += 1;
                    }
                    return out;
                }
                Event::Start(_) | Event::Rule | Event::DisplayMath(_) | Event::Html(_) if loose => return out,
                Event::Text(t) => match image.as_mut() {
                    Some((_, alt)) => alt.push_str(t),
                    None => out.push(Span { text: t.to_string(), st, kind: SpanKind::Text }),
                },
                Event::Code(t) | Event::InlineMath(t) => out.push(Span { text: t.to_string(), st, kind: SpanKind::Code }),
                Event::InlineHtml(t) | Event::Html(t) => out.push(Span { text: t.to_string(), st, kind: SpanKind::Code }),
                Event::SoftBreak => out.push(Span { text: " ".into(), st, kind: SpanKind::Text }),
                Event::HardBreak => out.push(Span { text: String::new(), st, kind: SpanKind::Break }),
                Event::TaskListMarker(b) => out.push(Span { text: String::new(), st, kind: SpanKind::Task(*b) }),
                Event::FootnoteReference(r) => {
                    let mut s = st.clone();
                    s.sup = true;
                    out.push(Span { text: format!("[{r}]"), st: s, kind: SpanKind::Text });
                }
                _ => {}
            }
            self.i += 1;
        }
        out
    }
}

pub fn parse(text: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut p = P { ev: Parser::new_ext(text, opts).collect(), i: 0 };
    p.blocks()
}

// ---------------------------------------------------------------- fonts

/// Reading fonts for the preview, with a real bold and italic. Uses the system's
/// Segoe UI when it is there (Windows); otherwise the app's own font is used.
pub fn install_fonts(ctx: &egui::Context) -> bool {
    // A complete family (regular, bold, italic, bold italic) that the system already has.
    let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:/Windows")).join("Fonts");
    let candidates: Vec<[PathBuf; 4]> = vec![
        ["segoeui.ttf", "segoeuib.ttf", "segoeuii.ttf", "segoeuiz.ttf"].map(|f| win.join(f)),
        // Linux: the families most distributions ship
        [
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Italic.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-BoldItalic.ttf",
        ]
        .map(PathBuf::from),
        [
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/noto/NotoSans-Italic.ttf",
            "/usr/share/fonts/noto/NotoSans-BoldItalic.ttf",
        ]
        .map(PathBuf::from),
        [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf",
        ]
        .map(PathBuf::from),
        [
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Oblique.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-BoldOblique.ttf",
        ]
        .map(PathBuf::from),
        [
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Oblique.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-BoldOblique.ttf",
        ]
        .map(PathBuf::from),
    ];
    let Some(set) = candidates.into_iter().find(|set| set.iter().all(|f| f.exists())) else { return false };
    let names = ["prose", "prose-b", "prose-i", "prose-bi"];
    let mut defs = egui::FontDefinitions::default();
    let fallbacks = defs.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, file) in names.into_iter().zip(set.iter()) {
        let Ok(bytes) = std::fs::read(file) else { return false };
        defs.font_data.insert(name.to_string(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        let mut fam = vec![name.to_string()];
        fam.extend(fallbacks.iter().cloned());
        defs.families.insert(egui::FontFamily::Name(name.into()), fam);
    }
    ctx.set_fonts(defs);
    true
}

fn prose_font(c: &Ctx, size: f32, bold: bool, italic: bool) -> FontId {
    if !c.fonts {
        return FontId::proportional(size);
    }
    let name = match (bold, italic) {
        (false, false) => "prose",
        (true, false) => "prose-b",
        (false, true) => "prose-i",
        (true, true) => "prose-bi",
    };
    FontId::new(size, egui::FontFamily::Name(name.into()))
}

fn code_lang(name: &str) -> Lang {
    match name.to_ascii_lowercase().as_str() {
        "" => Lang::Plain,
        "rust" | "rs" => Lang::Rust,
        "python" | "py" | "python3" => Lang::Python,
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" | "node" => Lang::JavaScript,
        "c" | "cpp" | "c++" | "h" | "java" | "csharp" | "cs" | "go" | "kotlin" | "swift" => Lang::C,
        "json" => Lang::Json,
        "markdown" | "md" => Lang::Markdown,
        "html" | "xml" | "svg" => Lang::Html,
        "css" | "scss" => Lang::Css,
        "toml" | "ini" | "yaml" | "yml" | "sh" | "bash" | "shell" | "zsh" | "powershell" | "ps1" => Lang::Toml,
        other => Lang::from_name(&format!("x.{other}")),
    }
}

// ---------------------------------------------------------------- drawing

/// Pictures used in the preview, loaded once.
#[derive(Default)]
pub struct Images {
    map: HashMap<PathBuf, Option<egui::TextureHandle>>,
}

pub enum PreviewAction {
    OpenFile(PathBuf),
}

pub struct Ctx<'a> {
    pub p: &'a Palette,
    pub base: Option<&'a Path>,
    pub size: f32,
    pub images: &'a mut Images,
    pub action: Option<PreviewAction>,
    pub quote: bool,
    pub code_id: usize,
    pub fonts: bool,
}

fn resolve(base: Option<&Path>, url: &str) -> Option<PathBuf> {
    if url.contains("://") || url.starts_with("mailto:") || url.starts_with('#') {
        return None;
    }
    let clean = url.split('#').next().unwrap_or("").replace("%20", " ");
    let p = PathBuf::from(&clean);
    let full = if p.is_absolute() { p } else { base?.join(clean.trim_start_matches("./")) };
    Some(full)
}

fn load_image(ctx: &egui::Context, images: &mut Images, path: &Path) -> Option<egui::TextureHandle> {
    images
        .map
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            let img = image::open(path).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
            Some(ctx.load_texture(path.display().to_string(), ci, Default::default()))
        })
        .clone()
}

fn spans_ui(ui: &mut egui::Ui, spans: &[Span], c: &mut Ctx, size: f32, heading: bool) {
    spans_ui_opt(ui, spans, c, size, heading, true)
}

fn spans_ui_opt(ui: &mut egui::Ui, spans: &[Span], c: &mut Ctx, size: f32, heading: bool, wrap: bool) {
    let p = c.p;
    let mut body = |ui: &mut egui::Ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
        for s in spans {
            let sz = if s.st.sup { size * 0.75 } else { size };
            let mut rt = match s.kind {
                SpanKind::Code => RichText::new(&s.text)
                    .font(FontId::monospace(sz * 0.9))
                    .background_color(p.current_line)
                    .color(p.keyword),
                _ => {
                    let bold = s.st.bold || heading;
                    let italic = s.st.italic || c.quote;
                    let mut r = RichText::new(&s.text)
                        .font(prose_font(c, sz, bold, italic))
                        .color(if c.quote { p.faint } else { p.ink });
                    if italic && !c.fonts {
                        r = r.italics();
                    }
                    r
                }
            };
            if s.st.strike {
                rt = rt.strikethrough();
            }
            match &s.kind {
                SpanKind::Break => ui.end_row(),
                SpanKind::Task(done) => {
                    let mut d = *done;
                    ui.add_enabled(false, egui::Checkbox::without_text(&mut d));
                    ui.add_space(6.0);
                }
                SpanKind::Image(url) => {
                    let local = resolve(c.base, url).filter(|p| p.exists());
                    let tex = local.as_ref().and_then(|lp| load_image(ui.ctx(), c.images, lp));
                    match tex {
                        Some(t) => {
                            let s0 = t.size_vec2();
                            let max_w = ui.available_width().max(60.0);
                            let scale = (max_w / s0.x).min(1.0);
                            ui.add(egui::Image::new(&t).fit_to_exact_size(s0 * scale).corner_radius(CornerRadius::same(6)))
                                .on_hover_text(&s.text);
                        }
                        None => {
                            let label = if s.text.is_empty() { "image".to_string() } else { s.text.clone() };
                            ui.label(RichText::new(format!("🖼 {label}")).size(sz).color(p.faint).italics())
                                .on_hover_text(url);
                        }
                    }
                }
                _ => {
                    if let Some(url) = &s.st.link {
                        let rt = rt.color(p.accent).underline();
                        let r = ui.add(egui::Link::new(rt)).on_hover_text(url);
                        if r.clicked() {
                            match resolve(c.base, url) {
                                Some(path) => c.action = Some(PreviewAction::OpenFile(path)),
                                None if url.starts_with('#') => {}
                                None => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
                            }
                        }
                    } else {
                        ui.label(rt);
                    }
                }
            }
        }
    };
    if wrap {
        ui.horizontal_wrapped(body);
    } else {
        ui.horizontal(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            body(ui)
        });
    }
}

fn blocks_ui(ui: &mut egui::Ui, blocks: &[Block], c: &mut Ctx, depth: usize) {
    let p = c.p;
    let size = c.size;
    for (k, b) in blocks.iter().enumerate() {
        if k > 0 {
            ui.add_space(size * 0.55);
        }
        match b {
            Block::Heading(l, spans) => {
                let hs = match l {
                    1 => size * 1.9,
                    2 => size * 1.5,
                    3 => size * 1.25,
                    4 => size * 1.1,
                    _ => size,
                };
                if k > 0 {
                    ui.add_space(size * 0.5);
                }
                spans_ui(ui, spans, c, hs, true);
                if *l <= 2 {
                    let r = ui.available_rect_before_wrap();
                    let y = r.top() + 3.0;
                    let w = if *l == 1 { 2.0 } else { 1.0 };
                    ui.painter().line_segment(
                        [egui::pos2(r.left(), y), egui::pos2(r.right(), y)],
                        Stroke::new(w, if *l == 1 { p.accent.gamma_multiply(0.7) } else { p.ruled }),
                    );
                    ui.add_space(8.0);
                }
            }
            Block::Para(spans) => spans_ui(ui, spans, c, size, false),
            Block::Quote(inner) => {
                let resp = egui::Frame::NONE
                    .inner_margin(egui::Margin { left: 16, right: 4, top: 2, bottom: 2 })
                    .show(ui, |ui| {
                        let was = c.quote;
                        c.quote = true;
                        blocks_ui(ui, inner, c, depth);
                        c.quote = was;
                    });
                let r = resp.response.rect;
                ui.painter().line_segment(
                    [egui::pos2(r.left() + 4.0, r.top()), egui::pos2(r.left() + 4.0, r.bottom())],
                    Stroke::new(3.0, p.accent.gamma_multiply(0.55)),
                );
            }
            Block::List(start, items) => {
                for (n, item) in items.iter().enumerate() {
                    if n > 0 {
                        ui.add_space(3.0);
                    }
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let is_task = matches!(item.first(), Some(Block::Para(s)) if matches!(s.first().map(|x| &x.kind), Some(SpanKind::Task(_))));
                        let marker = match start {
                            Some(s0) => format!("{}.", s0 + n as u64),
                            None if is_task => String::new(),
                            None => ["•", "◦", "▪"][depth % 3].to_string(),
                        };
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(22.0, size * 1.3), egui::Sense::hover());
                        ui.painter().text(
                            rect.right_center() - Vec2::new(4.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            marker,
                            FontId::proportional(size),
                            p.accent,
                        );
                        ui.vertical(|ui| blocks_ui(ui, item, c, depth + 1));
                    });
                }
            }
            Block::Code(lang, text) => {
                let l = code_lang(lang);
                c.code_id += 1;
                egui::Frame::NONE
                    .fill(p.current_line)
                    .stroke(Stroke::new(1.0, p.ruled))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::horizontal().id_salt(("md_code", c.code_id)).show(ui, |ui| {
                            let job = highlight(p, text, l, size * 0.88);
                            ui.add(egui::Label::new(job).extend());
                        });
                    });
            }
            Block::Rule => {
                ui.add_space(4.0);
                let r = ui.available_rect_before_wrap();
                let y = r.top() + 4.0;
                let mid = r.center().x;
                ui.painter().line_segment([egui::pos2(r.left(), y), egui::pos2(mid - 14.0, y)], Stroke::new(1.0, p.ruled));
                ui.painter().line_segment([egui::pos2(mid + 14.0, y), egui::pos2(r.right(), y)], Stroke::new(1.0, p.ruled));
                ui.painter().text(egui::pos2(mid, y), egui::Align2::CENTER_CENTER, "✎", FontId::proportional(size * 0.8), p.accent.gamma_multiply(0.7));
                ui.add_space(10.0);
            }
            Block::Table(rows, head) => {
                c.code_id += 1;
                egui::ScrollArea::horizontal().id_salt(("md_table", c.code_id)).show(ui, |ui| {
                    egui::Frame::NONE.stroke(Stroke::new(1.0, p.ruled)).corner_radius(CornerRadius::same(6)).inner_margin(6).show(ui, |ui| {
                        egui::Grid::new(("md_grid", c.code_id)).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
                            for (ri, row) in rows.iter().enumerate() {
                                for cell in row {
                                    spans_ui_opt(ui, cell, c, size * 0.95, ri < *head, false);
                                }
                                ui.end_row();
                            }
                        });
                    });
                });
            }
            Block::Raw(t) => {
                ui.label(RichText::new(t).font(FontId::monospace(size * 0.85)).color(p.faint));
            }
        }
    }
}

/// Draws the whole preview page. Returns a link the person clicked, if any.
pub fn show(ui: &mut egui::Ui, blocks: &[Block], c: &mut Ctx) {
    if blocks.is_empty() {
        ui.add_space(20.0);
        ui.label(RichText::new("La page est encore blanche… écrivez du Markdown à gauche.").italics().color(c.p.faint));
        return;
    }
    blocks_ui(ui, blocks, c, 0);
}

