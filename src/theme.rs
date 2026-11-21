use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Colours of one look. "Papier" is the cream-paper-and-ink ArtCraft look,
/// "Encre de nuit" is its dark twin.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub paper: Color32,      // editor background
    pub panel: Color32,      // menus, status bar
    pub sidebar: Color32,    // kraft side panel
    pub tab_idle: Color32,   // inactive tabs
    pub tab_bar: Color32,    // strip behind tabs
    pub ink: Color32,        // main text
    pub faint: Color32,      // secondary text
    pub ruled: Color32,      // notebook lines
    pub margin: Color32,     // red margin line
    pub accent: Color32,     // terracotta
    pub current_line: Color32,
    pub selection: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub comment: Color32,
    pub number: Color32,
    pub kind: Color32,
    pub func: Color32,
    pub punct: Color32,
    pub minimap_bg: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const PAPER: Palette = Palette {
    dark: false,
    paper: rgb(0xFB, 0xF6, 0xEA),
    panel: rgb(0xF1, 0xE6, 0xD0),
    sidebar: rgb(0xE6, 0xD4, 0xB3),
    tab_idle: rgb(0xDD, 0xC9, 0xA4),
    tab_bar: rgb(0xD3, 0xBD, 0x95),
    ink: rgb(0x2E, 0x2A, 0x24),
    faint: rgb(0x8E, 0x7E, 0x64),
    ruled: rgb(0xEC, 0xE2, 0xCD),
    margin: rgb(0xE8, 0xA5, 0x98),
    accent: rgb(0xC4, 0x64, 0x3A),
    current_line: Color32::from_rgba_premultiplied(0xF4, 0xE4, 0xC4, 0xFF),
    selection: rgb(0xF2, 0xD0, 0x96),
    keyword: rgb(0xB0, 0x41, 0x3E),
    string: rgb(0x4E, 0x7D, 0x3A),
    comment: rgb(0x9A, 0x8B, 0x73),
    number: rgb(0x7A, 0x4F, 0xA0),
    kind: rgb(0x2F, 0x66, 0x90),
    func: rgb(0xA8, 0x6A, 0x12),
    punct: rgb(0x6B, 0x5F, 0x4E),
    minimap_bg: rgb(0xF5, 0xEE, 0xDE),
};

pub const NIGHT: Palette = Palette {
    dark: true,
    paper: rgb(0x26, 0x22, 0x1E),
    panel: rgb(0x1E, 0x1B, 0x18),
    sidebar: rgb(0x1A, 0x17, 0x14),
    tab_idle: rgb(0x31, 0x2B, 0x25),
    tab_bar: rgb(0x16, 0x13, 0x11),
    ink: rgb(0xED, 0xE3, 0xD1),
    faint: rgb(0x8F, 0x82, 0x6E),
    ruled: rgb(0x2E, 0x29, 0x24),
    margin: rgb(0x6E, 0x3A, 0x32),
    accent: rgb(0xE8, 0x84, 0x5C),
    current_line: rgb(0x30, 0x2A, 0x24),
    selection: rgb(0x5A, 0x44, 0x30),
    keyword: rgb(0xE8, 0x84, 0x5C),
    string: rgb(0xA8, 0xC9, 0x7F),
    comment: rgb(0x8A, 0x7D, 0x6A),
    number: rgb(0xC9, 0xA0, 0xE6),
    kind: rgb(0x7F, 0xB8, 0xD9),
    func: rgb(0xE8, 0xC4, 0x68),
    punct: rgb(0xB8, 0xAC, 0x98),
    minimap_bg: rgb(0x22, 0x1E, 0x1A),
};

pub fn palette(dark: bool) -> &'static Palette {
    if dark {
        &NIGHT
    } else {
        &PAPER
    }
}

pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.ink);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.paper;
    v.faint_bg_color = p.current_line;
    v.code_bg_color = p.paper;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.ink);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, p.tab_bar);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);

    let r = CornerRadius::same(6);
    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.weak_bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.tab_idle);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.ink);
    v.widgets.noninteractive.corner_radius = r;

    v.widgets.inactive.bg_fill = p.tab_idle;
    v.widgets.inactive.weak_bg_fill = p.tab_idle;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.ink);
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.corner_radius = r;

    v.widgets.hovered.bg_fill = p.selection;
    v.widgets.hovered.weak_bg_fill = p.selection;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.ink);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.hovered.corner_radius = r;

    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.active.fg_stroke = Stroke::new(2.0, p.paper);
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.corner_radius = r;

    v.widgets.open.bg_fill = p.selection;
    v.widgets.open.weak_bg_fill = p.selection;
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.ink);
    v.widgets.open.corner_radius = r;

    ctx.set_visuals(v);
}
