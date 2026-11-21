use eframe::egui::{
    self,
    text::{LayoutJob, TextFormat},
    Color32, FontId,
};

use crate::theme::{palette, Palette};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Lang {
    Plain,
    Rust,
    Python,
    JavaScript,
    C,
    Json,
    Markdown,
    Html,
    Css,
    Toml,
}

impl Lang {
    pub const ALL: [Lang; 10] = [
        Lang::Plain,
        Lang::Rust,
        Lang::Python,
        Lang::JavaScript,
        Lang::C,
        Lang::Json,
        Lang::Markdown,
        Lang::Html,
        Lang::Css,
        Lang::Toml,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Lang::Plain => "Texte brut",
            Lang::Rust => "Rust",
            Lang::Python => "Python",
            Lang::JavaScript => "JavaScript",
            Lang::C => "C / C++",
            Lang::Json => "JSON",
            Lang::Markdown => "Markdown",
            Lang::Html => "HTML",
            Lang::Css => "CSS",
            Lang::Toml => "TOML / INI",
        }
    }

    pub fn from_name(name: &str) -> Lang {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !name.contains('.') {
            return Lang::Plain;
        }
        match ext.as_str() {
            "rs" => Lang::Rust,
            "py" | "pyw" => Lang::Python,
            "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => Lang::JavaScript,
            "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "go" | "swift" | "kt" => Lang::C,
            "json" => Lang::Json,
            "md" | "markdown" => Lang::Markdown,
            "html" | "htm" | "xml" | "svg" | "vue" => Lang::Html,
            "css" | "scss" | "less" => Lang::Css,
            "toml" | "ini" | "cfg" | "conf" | "yaml" | "yml" => Lang::Toml,
            _ => Lang::Plain,
        }
    }

    pub fn line_comment(self) -> Option<&'static str> {
        match self {
            Lang::Rust | Lang::JavaScript | Lang::C => Some("//"),
            Lang::Python | Lang::Toml => Some("#"),
            _ => None,
        }
    }

    pub fn block_comment(self) -> Option<(&'static str, &'static str)> {
        match self {
            Lang::Rust | Lang::JavaScript | Lang::C | Lang::Css => Some(("/*", "*/")),
            Lang::Html | Lang::Markdown => Some(("<!--", "-->")),
            _ => None,
        }
    }

    /// What the "comment" command puts at the start of a line.
    pub fn comment_token(self) -> Option<&'static str> {
        match self {
            Lang::Html | Lang::Css | Lang::Json | Lang::Markdown | Lang::Plain => None,
            other => other.line_comment(),
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop",
                "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static",
                "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Lang::Python => &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
                "del", "elif", "else", "except", "False", "finally", "for", "from", "global",
                "if", "import", "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass",
                "raise", "return", "True", "try", "while", "with", "yield", "self",
            ],
            Lang::JavaScript => &[
                "async", "await", "break", "case", "catch", "class", "const", "continue",
                "default", "delete", "do", "else", "export", "extends", "false", "finally", "for",
                "from", "function", "if", "import", "in", "instanceof", "interface", "let", "new",
                "null", "of", "return", "static", "super", "switch", "this", "throw", "true",
                "try", "type", "typeof", "undefined", "var", "void", "while", "yield",
            ],
            Lang::C => &[
                "auto", "bool", "break", "case", "char", "class", "const", "continue", "default",
                "delete", "do", "double", "else", "enum", "extern", "false", "float", "for",
                "func", "if", "import", "include", "int", "long", "namespace", "new", "nullptr",
                "package", "private", "protected", "public", "return", "short", "signed",
                "sizeof", "static", "struct", "switch", "template", "this", "true", "typedef",
                "union", "unsigned", "using", "var", "virtual", "void", "while", "define",
            ],
            Lang::Json => &["true", "false", "null"],
            Lang::Css => &["important", "media", "import", "from", "to"],
            Lang::Toml => &["true", "false"],
            _ => &[],
        }
    }
}

fn fmt(font: &FontId, color: Color32, italics: bool) -> TextFormat {
    TextFormat {
        font_id: font.clone(),
        color,
        italics,
        ..Default::default()
    }
}

pub fn highlight(p: &Palette, code: &str, lang: Lang, size: f32) -> LayoutJob {
    let font = FontId::monospace(size);
    let mut job = LayoutJob::default();
    if lang == Lang::Plain {
        job.append(code, 0.0, fmt(&font, p.ink, false));
        return job;
    }
    let keywords = lang.keywords();
    let mut i = 0usize;
    let mut line_start = true;
    while i < code.len() {
        let rest = &code[i..];
        let c = rest.chars().next().unwrap();

        // Block comments
        if let Some((open, close)) = lang.block_comment() {
            if rest.starts_with(open) {
                let end = rest[open.len()..]
                    .find(close)
                    .map(|e| e + open.len() + close.len())
                    .unwrap_or(rest.len());
                job.append(&rest[..end], 0.0, fmt(&font, p.comment, true));
                i += end;
                line_start = false;
                continue;
            }
        }
        // Line comments
        if let Some(lc) = lang.line_comment() {
            if rest.starts_with(lc) {
                let end = rest.find('\n').unwrap_or(rest.len());
                job.append(&rest[..end], 0.0, fmt(&font, p.comment, true));
                i += end;
                continue;
            }
        }
        // Markdown line starts
        if lang == Lang::Markdown && line_start {
            let end = rest.find('\n').unwrap_or(rest.len());
            if c == '#' {
                job.append(&rest[..end], 0.0, fmt(&font, p.keyword, false));
                i += end;
                continue;
            }
            if c == '>' {
                job.append(&rest[..end], 0.0, fmt(&font, p.comment, true));
                i += end;
                continue;
            }
        }
        // Toml section headers
        if lang == Lang::Toml && line_start && c == '[' {
            let end = rest.find('\n').unwrap_or(rest.len());
            job.append(&rest[..end], 0.0, fmt(&font, p.kind, false));
            i += end;
            continue;
        }

        if c == '\n' {
            job.append("\n", 0.0, fmt(&font, p.ink, false));
            i += 1;
            line_start = true;
            continue;
        }
        if c == ' ' || c == '\t' {
            let n = rest.len() - rest.trim_start_matches([' ', '\t']).len();
            job.append(&rest[..n], 0.0, fmt(&font, p.ink, false));
            i += n;
            continue;
        }
        line_start = false;

        // Strings
        let quote = c == '"' || c == '\'' || (c == '`' && matches!(lang, Lang::JavaScript | Lang::Markdown));
        if quote {
            let q = c as u8;
            let b = rest.as_bytes();
            let mut j = 1;
            let mut closed = false;
            while j < b.len() {
                if b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == q {
                    j += 1;
                    closed = true;
                    break;
                }
                if b[j] == b'\n' && c != '`' {
                    break;
                }
                j += 1;
            }
            let mut j = j.min(rest.len());
            while !rest.is_char_boundary(j) {
                j += 1;
            }
            if !closed && (c == '\'' || lang == Lang::Markdown) {
                job.append(&rest[..1], 0.0, fmt(&font, p.punct, false));
                i += 1;
                continue;
            }
            job.append(&rest[..j], 0.0, fmt(&font, p.string, false));
            i += j;
            continue;
        }

        if lang == Lang::Markdown {
            // Emphasis markers and list bullets in accent, the rest as ink.
            if c == '*' || c == '_' || c == '-' {
                job.append(&rest[..1], 0.0, fmt(&font, p.accent, false));
                i += 1;
                continue;
            }
            let n = rest
                .find(|ch: char| matches!(ch, '\n' | '`' | '*' | '_' | '-'))
                .unwrap_or(rest.len())
                .max(c.len_utf8());
            job.append(&rest[..n], 0.0, fmt(&font, p.ink, false));
            i += n;
            continue;
        }

        if lang == Lang::Html && c == '<' {
            let n = 1 + rest[1..]
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '/' || ch == '!' || ch == '-'))
                .unwrap_or(rest.len() - 1);
            job.append(&rest[..n], 0.0, fmt(&font, p.keyword, false));
            i += n;
            continue;
        }

        // Numbers
        if c.is_ascii_digit() {
            let n = rest
                .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '_'))
                .unwrap_or(rest.len());
            job.append(&rest[..n], 0.0, fmt(&font, p.number, false));
            i += n;
            continue;
        }

        // Words
        if c.is_alphabetic() || c == '_' || (lang == Lang::Css && c == '-') {
            let n = rest
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || (lang == Lang::Css && ch == '-')))
                .unwrap_or(rest.len());
            let word = &rest[..n];
            let after = rest[n..].trim_start_matches(' ');
            let color = if keywords.contains(&word) {
                p.keyword
            } else if after.starts_with('(') || after.starts_with('!') && lang == Lang::Rust {
                p.func
            } else if lang == Lang::Css && after.starts_with(':') {
                p.kind
            } else if (lang == Lang::Toml || lang == Lang::Html) && after.starts_with('=') {
                p.func
            } else if word.chars().next().map_or(false, |f| f.is_uppercase()) {
                p.kind
            } else {
                p.ink
            };
            job.append(word, 0.0, fmt(&font, color, false));
            i += n;
            continue;
        }

        // Punctuation runs
        let n = rest
            .find(|ch: char| {
                ch.is_alphanumeric()
                    || ch.is_whitespace()
                    || matches!(ch, '_' | '"' | '\'' | '`' | '/' | '#' | '<')
            })
            .unwrap_or(rest.len())
            .max(c.len_utf8());
        job.append(&rest[..n], 0.0, fmt(&font, p.punct, false));
        i += n;
    }
    job
}

#[derive(Default)]
pub struct Highlighter;

impl egui::cache::ComputerMut<(bool, &str, Lang, u32), LayoutJob> for Highlighter {
    fn compute(&mut self, (dark, code, lang, size): (bool, &str, Lang, u32)) -> LayoutJob {
        highlight(palette(dark), code, lang, f32::from_bits(size))
    }
}

pub type HighlightCache = egui::cache::FrameCache<LayoutJob, Highlighter>;
