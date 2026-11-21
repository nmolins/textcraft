//! Text editing actions behind the menus and shortcuts.

use eframe::egui::{self, text_edit::TextEditState};

use crate::app::{App, Cmd};
use crate::doc::*;
use crate::highlight::Lang;

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl App {
    // ------------------------------------------------ selection helpers

    /// (anchor, cursor) in characters.
    pub fn sel(&self, ctx: &egui::Context) -> (usize, usize) {
        let d = self.doc();
        let n = d.text.chars().count();
        let (a, b) = get_sel(ctx, editor_id(d.id)).unwrap_or((0, 0));
        (a.min(n), b.min(n))
    }

    pub fn selection_text(&self, ctx: &egui::Context) -> Option<String> {
        let (a, b) = self.sel(ctx);
        if a == b {
            return None;
        }
        let (s, e) = (a.min(b), a.max(b));
        Some(self.doc().text.chars().skip(s).take(e - s).collect())
    }

    fn word_range_at(&self, pos: usize) -> Option<(usize, usize)> {
        let chars: Vec<char> = self.doc().text.chars().collect();
        let mut s = pos.min(chars.len());
        if (s == chars.len() || !is_word(chars[s])) && s > 0 && is_word(chars[s - 1]) {
            s -= 1;
        }
        if s >= chars.len() || !is_word(chars[s]) {
            return None;
        }
        let mut e = s;
        while s > 0 && is_word(chars[s - 1]) {
            s -= 1;
        }
        while e < chars.len() && is_word(chars[e]) {
            e += 1;
        }
        Some((s, e))
    }

    pub fn selection_or_word(&self, ctx: &egui::Context) -> Option<String> {
        if let Some(t) = self.selection_text(ctx) {
            return Some(t);
        }
        let (_, b) = self.sel(ctx);
        let (s, e) = self.word_range_at(b)?;
        Some(self.doc().text.chars().skip(s).take(e - s).collect())
    }

    pub fn selected_lines(&self, ctx: &egui::Context) -> (usize, usize, usize, usize) {
        let doc = self.doc();
        let (a, b) = self.sel(ctx);
        let (s, e) = (a.min(b), a.max(b));
        let l0 = line_of(&doc.text, s);
        let mut l1 = line_of(&doc.text, e);
        // A selection ending at column 0 doesn't include that line.
        if e > s && l1 > l0 && col_of(&doc.text, e) == 0 {
            l1 -= 1;
        }
        (l0, l1, a, b)
    }

    fn replace_chars(&mut self, ctx: &egui::Context, s: usize, e: usize, with: &str, select: bool) {
        let doc = &mut self.docs[self.active];
        let bs = char_to_byte(&doc.text, s);
        let be = char_to_byte(&doc.text, e);
        doc.text.replace_range(bs..be, with);
        let ne = s + with.chars().count();
        if select {
            set_sel(ctx, editor_id(doc.id), s, ne);
        } else {
            set_sel(ctx, editor_id(doc.id), ne, ne);
        }
    }

    fn scroll_to_line(&mut self, line: usize) {
        let d = &mut self.docs[self.active];
        d.set_scroll = Some(line as f32 * d.row_h - d.view_h / 3.0);
    }

    // ------------------------------------------------ jump history

    pub fn remember_position(&mut self, ctx: &egui::Context) {
        let (_, b) = self.sel(ctx);
        let id = self.doc().id;
        self.back.push((id, b));
        if self.back.len() > 100 {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    pub fn jump_history(&mut self, ctx: &egui::Context, back: bool) {
        let target = if back { self.back.pop() } else { self.forward.pop() };
        let Some((id, pos)) = target else { return };
        let here = (self.doc().id, self.sel(ctx).1);
        if back {
            self.forward.push(here);
        } else {
            self.back.push(here);
        }
        if let Some(i) = self.docs.iter().position(|d| d.id == id) {
            self.active = i;
            let d = &self.docs[i];
            set_sel(ctx, editor_id(d.id), pos, pos);
            let line = line_of(&d.text, pos);
            self.scroll_to_line(line);
        }
    }

    // ------------------------------------------------ undo / clipboard

    pub fn can_undo_redo(&self, ctx: &egui::Context, redo: bool) -> bool {
        let d = self.doc();
        let id = editor_id(d.id);
        let Some(st) = TextEditState::load(ctx, id) else { return false };
        let range = st.cursor.char_range().unwrap_or_default();
        let cur = (range, d.text.clone());
        let u = st.undoer();
        if redo {
            u.has_redo(&cur)
        } else {
            u.has_undo(&cur)
        }
    }

    pub fn undo_redo(&mut self, ctx: &egui::Context, redo: bool) {
        let d = &mut self.docs[self.active];
        let id = editor_id(d.id);
        let Some(mut st) = TextEditState::load(ctx, id) else { return };
        let range = st.cursor.char_range().unwrap_or_default();
        let cur = (range, d.text.clone());
        let mut u = st.undoer();
        let next = if redo { u.redo(&cur).cloned() } else { u.undo(&cur).cloned() };
        if let Some((r, t)) = next {
            d.text = t;
            st.cursor.set_char_range(Some(r));
        }
        st.set_undoer(u);
        st.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    pub fn cut_copy(&mut self, ctx: &egui::Context, cut: bool) {
        let Some(t) = self.selection_text(ctx) else { return };
        ctx.copy_text(t);
        if cut {
            let (a, b) = self.sel(ctx);
            self.replace_chars(ctx, a.min(b), a.max(b), "", false);
        }
    }

    pub fn paste(&mut self, ctx: &egui::Context) {
        let text = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
        if text.is_empty() {
            return;
        }
        let text = text.replace("\r\n", "\n");
        let (a, b) = self.sel(ctx);
        self.replace_chars(ctx, a.min(b), a.max(b), &text, false);
    }

    // ------------------------------------------------ lines

    fn lines(&self) -> Vec<String> {
        self.doc().text.split('\n').map(String::from).collect()
    }

    fn set_lines(&mut self, ctx: &egui::Context, lines: Vec<String>, l0: usize, l1: usize) {
        let d = &mut self.docs[self.active];
        d.text = lines.join("\n");
        select_lines(ctx, d, l0, l1);
    }

    pub fn toggle_comment(&mut self, ctx: &egui::Context) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let Some(tok) = self.doc().lang.comment_token() else { return };
        let mut lines = self.lines();
        let filled = || lines[l0..=l1].iter().filter(|l| !l.trim().is_empty());
        let all_commented = filled().all(|l| l.trim_start().starts_with(tok));
        let indent = filled().map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
        for l in &mut lines[l0..=l1] {
            if l.trim().is_empty() {
                continue;
            }
            if all_commented {
                let pos = l.find(tok).unwrap();
                let mut end = pos + tok.len();
                if l[end..].starts_with(' ') {
                    end += 1;
                }
                l.replace_range(pos..end, "");
            } else {
                l.insert_str(indent, &format!("{tok} "));
            }
        }
        self.set_lines(ctx, lines, l0, l1);
    }

    pub fn duplicate_line(&mut self, ctx: &egui::Context) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let mut lines = self.lines();
        let block: Vec<String> = lines[l0..=l1].to_vec();
        let n = block.len();
        for (k, l) in block.into_iter().enumerate() {
            lines.insert(l1 + 1 + k, l);
        }
        self.set_lines(ctx, lines, l0 + n, l1 + n);
    }

    pub fn delete_line(&mut self, ctx: &egui::Context) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let mut lines = self.lines();
        lines.drain(l0..=l1);
        if lines.is_empty() {
            lines.push(String::new());
        }
        let l = l0.min(lines.len() - 1);
        let d = &mut self.docs[self.active];
        d.text = lines.join("\n");
        let c = line_start_char(&d.text, l);
        set_sel(ctx, editor_id(d.id), c, c);
    }

    pub fn move_lines(&mut self, ctx: &egui::Context, up: bool) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let mut lines = self.lines();
        if up && l0 > 0 {
            let l = lines.remove(l0 - 1);
            lines.insert(l1, l);
            self.set_lines(ctx, lines, l0 - 1, l1 - 1);
        } else if !up && l1 + 1 < lines.len() {
            let l = lines.remove(l1 + 1);
            lines.insert(l0, l);
            self.set_lines(ctx, lines, l0 + 1, l1 + 1);
        }
    }

    pub fn join_lines(&mut self, ctx: &egui::Context) {
        let (l0, mut l1, _, _) = self.selected_lines(ctx);
        let mut lines = self.lines();
        if l1 == l0 {
            l1 = (l0 + 1).min(lines.len() - 1);
        }
        if l1 == l0 {
            return;
        }
        let mut joined = lines[l0].trim_end().to_string();
        for l in &lines[l0 + 1..=l1] {
            let t = l.trim();
            if !t.is_empty() {
                if !joined.is_empty() {
                    joined.push(' ');
                }
                joined.push_str(t);
            }
        }
        lines.splice(l0..=l1, [joined]);
        self.set_lines(ctx, lines, l0, l0);
    }

    pub fn insert_line(&mut self, ctx: &egui::Context, before: bool) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let mut lines = self.lines();
        let at = if before { l0 } else { l1 };
        let ws: String = lines[at].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let target = if before { l0 } else { l1 + 1 };
        lines.insert(target, ws.clone());
        let d = &mut self.docs[self.active];
        d.text = lines.join("\n");
        let c = line_start_char(&d.text, target) + ws.chars().count();
        set_sel(ctx, editor_id(d.id), c, c);
    }

    pub fn indent_lines(&mut self, ctx: &egui::Context, out: bool) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let ts = self.tab_size as usize;
        let mut lines = self.lines();
        for l in &mut lines[l0..=l1] {
            if out {
                let n = if l.starts_with('\t') { 1 } else { (l.len() - l.trim_start_matches(' ').len()).min(ts) };
                l.replace_range(0..n, "");
            } else if !l.is_empty() {
                l.insert_str(0, &" ".repeat(ts));
            }
        }
        self.set_lines(ctx, lines, l0, l1);
    }

    /// Tab / Shift+Tab inside the editor.
    pub fn indent(&mut self, ctx: &egui::Context, out: bool) {
        let (l0, l1, a, b) = self.selected_lines(ctx);
        let ts = self.tab_size as usize;
        if !out && a == b {
            // Plain Tab: spaces up to the next tab stop.
            let d = &mut self.docs[self.active];
            let col = col_of(&d.text, a);
            let n = ts - col % ts;
            let bi = char_to_byte(&d.text, a);
            d.text.insert_str(bi, &" ".repeat(n));
            set_sel(ctx, editor_id(d.id), a + n, a + n);
            return;
        }
        if a == b && l0 == l1 {
            self.indent_lines(ctx, out);
            let d = &self.docs[self.active];
            let c = line_start_char(&d.text, l0);
            let len = d.text.split('\n').nth(l0).map_or(0, |s| s.len() - s.trim_start().len());
            set_sel(ctx, editor_id(d.id), c + len, c + len);
        } else {
            self.indent_lines(ctx, out);
        }
    }

    pub fn newline_indent(&mut self, ctx: &egui::Context) {
        let ts = self.tab_size as usize;
        let (a, b) = self.sel(ctx);
        let (s, e) = (a.min(b), a.max(b));
        let d = &self.docs[self.active];
        let line = line_of(&d.text, s);
        let cur = d.text.split('\n').nth(line).unwrap_or("");
        let mut ws: String = cur.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let bs = char_to_byte(&d.text, s);
        let trimmed = d.text[..bs].trim_end_matches(' ');
        if trimmed.ends_with('{')
            || trimmed.ends_with('[')
            || trimmed.ends_with('(')
            || (trimmed.ends_with(':') && d.lang == Lang::Python)
        {
            ws.push_str(&" ".repeat(ts));
        }
        self.replace_chars(ctx, s, e, &format!("\n{ws}"), false);
    }

    pub fn convert_indent(&mut self, ctx: &egui::Context) {
        let sp = " ".repeat(self.tab_size as usize);
        let mut lines = self.lines();
        let mut changed = 0;
        for l in &mut lines {
            let tabs = l.len() - l.trim_start_matches('\t').len();
            if tabs > 0 {
                l.replace_range(0..tabs, &sp.repeat(tabs));
                changed += 1;
            }
        }
        self.docs[self.active].text = lines.join("\n");
        self.flash(ctx, format!("{changed} ligne(s) converties en espaces"));
    }

    pub fn change_case(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let (a, b) = self.sel(ctx);
        let (s, e) = if a == b {
            match self.word_range_at(b) {
                Some(r) => r,
                None => return,
            }
        } else {
            (a.min(b), a.max(b))
        };
        let t: String = self.doc().text.chars().skip(s).take(e - s).collect();
        let out: String = match cmd {
            Cmd::Upper => t.to_uppercase(),
            Cmd::Lower => t.to_lowercase(),
            Cmd::SwapCase => t
                .chars()
                .flat_map(|c| if c.is_uppercase() { c.to_lowercase().collect::<Vec<_>>() } else { c.to_uppercase().collect() })
                .collect(),
            _ => {
                let mut start = true;
                t.chars()
                    .flat_map(|c| {
                        let r: Vec<char> = if start { c.to_uppercase().collect() } else { c.to_lowercase().collect() };
                        start = !c.is_alphanumeric();
                        r
                    })
                    .collect()
            }
        };
        self.replace_chars(ctx, s, e, &out, true);
    }

    pub fn permute_lines(&mut self, ctx: &egui::Context, cmd: Cmd) {
        let (mut l0, mut l1, a, b) = self.selected_lines(ctx);
        let mut lines = self.lines();
        if a == b || l0 == l1 {
            // No multi-line selection: work on the whole file.
            l0 = 0;
            l1 = lines.len() - 1;
            while l1 > 0 && lines[l1].is_empty() {
                l1 -= 1;
            }
        }
        let mut block: Vec<String> = lines[l0..=l1].to_vec();
        match cmd {
            Cmd::SortLines => block.sort_by_key(|l| l.to_lowercase()),
            Cmd::SortLinesCase => block.sort(),
            Cmd::ReverseLines => block.reverse(),
            _ => {
                let mut seen = std::collections::HashSet::new();
                block.retain(|l| seen.insert(l.clone()));
            }
        }
        let n = block.len();
        lines.splice(l0..=l1, block);
        self.set_lines(ctx, lines, l0, l0 + n.saturating_sub(1));
    }

    // ------------------------------------------------ expand selection

    pub fn expand_line(&mut self, ctx: &egui::Context) {
        let (l0, l1, a, b) = self.selected_lines(ctx);
        let d = self.doc();
        let total = d.line_count();
        let s = line_start_char(&d.text, l0);
        let mut e = if l1 + 1 < total { line_start_char(&d.text, l1 + 1) } else { d.text.chars().count() };
        if (a.min(b), a.max(b)) == (s, e) && l1 + 2 <= total {
            e = if l1 + 2 < total { line_start_char(&d.text, l1 + 2) } else { d.text.chars().count() };
        }
        set_sel(ctx, editor_id(d.id), s, e);
    }

    pub fn expand_word(&mut self, ctx: &egui::Context) {
        let (a, b) = self.sel(ctx);
        if a != b {
            // Already a selection: go to its next occurrence.
            if let Some(t) = self.selection_text(ctx) {
                self.find_query = t;
                self.find_next(ctx, false);
            }
            return;
        }
        if let Some((s, e)) = self.word_range_at(b) {
            set_sel(ctx, editor_id(self.doc().id), s, e);
        }
    }

    fn enclosing_brackets(&self, s: usize, e: usize) -> Option<(usize, usize)> {
        let chars: Vec<char> = self.doc().text.chars().collect();
        let mut depth = 0i32;
        let mut i = s;
        let open = loop {
            if i == 0 {
                return None;
            }
            i -= 1;
            match chars[i] {
                ')' | ']' | '}' => depth += 1,
                '(' | '[' | '{' => {
                    if depth == 0 {
                        break i;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        };
        let close_ch = match chars[open] {
            '(' => ')',
            '[' => ']',
            _ => '}',
        };
        let mut depth = 0i32;
        for (j, c) in chars.iter().enumerate().skip(e) {
            if *c == chars[open] {
                depth += 1;
            } else if *c == close_ch {
                if depth == 0 {
                    return Some((open, j));
                }
                depth -= 1;
            }
        }
        None
    }

    pub fn expand_brackets(&mut self, ctx: &egui::Context) {
        let (a, b) = self.sel(ctx);
        let (s, e) = (a.min(b), a.max(b));
        if let Some((o, c)) = self.enclosing_brackets(s, e) {
            let id = editor_id(self.doc().id);
            if (s, e) == (o + 1, c) {
                set_sel(ctx, id, o, c + 1);
            } else {
                set_sel(ctx, id, o + 1, c);
            }
        }
    }

    pub fn expand_paragraph(&mut self, ctx: &egui::Context) {
        let (l0, l1, _, _) = self.selected_lines(ctx);
        let lines = self.lines();
        let mut s = l0;
        while s > 0 && !lines[s - 1].trim().is_empty() {
            s -= 1;
        }
        let mut e = l1;
        while e + 1 < lines.len() && !lines[e + 1].trim().is_empty() {
            e += 1;
        }
        select_lines(ctx, self.doc(), s, e);
    }

    pub fn matching_bracket(&mut self, ctx: &egui::Context) {
        let (_, b) = self.sel(ctx);
        let chars: Vec<char> = self.doc().text.chars().collect();
        let pairs = [('(', ')'), ('[', ']'), ('{', '}')];
        for at in [b, b.wrapping_sub(1)] {
            let Some(&c) = chars.get(at) else { continue };
            if let Some(&(o, cl)) = pairs.iter().find(|(o, cl)| *o == c || *cl == c) {
                let forward = c == o;
                let mut depth = 0i32;
                let mut i = at;
                loop {
                    if forward {
                        i += 1;
                        if i >= chars.len() {
                            return;
                        }
                    } else {
                        if i == 0 {
                            return;
                        }
                        i -= 1;
                    }
                    if chars[i] == c {
                        depth += 1;
                    } else if chars[i] == if forward { cl } else { o } {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    }
                }
                self.remember_position(ctx);
                let id = editor_id(self.doc().id);
                set_sel(ctx, id, i, i);
                let line = line_of(&self.doc().text, i);
                self.scroll_to_line(line);
                return;
            }
        }
        if let Some((o, _)) = self.enclosing_brackets(b, b) {
            set_sel(ctx, editor_id(self.doc().id), o, o);
        }
    }

    // ------------------------------------------------ bookmarks, revert

    pub fn bookmark_jump(&mut self, ctx: &egui::Context, next: bool) {
        let d = self.doc();
        if d.bookmarks.is_empty() {
            return;
        }
        let cur = d.line;
        let target = if next {
            d.bookmarks.iter().copied().find(|l| *l > cur).unwrap_or(d.bookmarks[0])
        } else {
            d.bookmarks.iter().rev().copied().find(|l| *l < cur).unwrap_or(*d.bookmarks.last().unwrap())
        };
        self.goto_line(ctx, target + 1);
    }

    pub fn revert(&mut self, ctx: &egui::Context) {
        let d = &mut self.docs[self.active];
        let Some(p) = d.path.clone() else { return };
        if let Ok(bytes) = std::fs::read(&p) {
            let raw = String::from_utf8_lossy(&bytes).to_string();
            d.crlf = raw.contains("\r\n");
            d.text = raw.replace("\r\n", "\n");
            d.saved = d.text.clone();
            self.flash(ctx, "Fichier rechargé depuis le disque");
        }
    }

    // ------------------------------------------------ find / replace

    pub fn find_next(&mut self, ctx: &egui::Context, backwards: bool) -> bool {
        if self.find_query.is_empty() {
            return false;
        }
        let (a, b) = self.sel(ctx);
        let doc = &mut self.docs[self.active];
        let id = editor_id(doc.id);
        let hay = if self.match_case { doc.text.clone() } else { doc.text.to_lowercase() };
        let needle = if self.match_case { self.find_query.clone() } else { self.find_query.to_lowercase() };
        if hay.len() != doc.text.len() || needle.len() != self.find_query.len() {
            // Lowercasing changed byte lengths: fall back to exact search.
            return find_exact(ctx, doc, &self.find_query, a.max(b), backwards);
        }
        let start_b = char_to_byte(&doc.text, if backwards { a.min(b) } else { a.max(b) });
        let found = if backwards {
            hay[..start_b].rfind(&needle).or_else(|| hay.rfind(&needle))
        } else {
            hay[start_b..].find(&needle).map(|x| x + start_b).or_else(|| hay.find(&needle))
        };
        match found {
            Some(pos) => {
                let s = doc.text[..pos].chars().count();
                let e = s + doc.text[pos..pos + needle.len()].chars().count();
                set_sel(ctx, id, s, e);
                let line = line_of(&doc.text, s);
                doc.set_scroll = Some(line as f32 * doc.row_h - doc.view_h / 3.0);
                true
            }
            None => false,
        }
    }

    pub fn count_matches(&self) -> usize {
        if self.find_query.is_empty() {
            return 0;
        }
        let doc = self.doc();
        if self.match_case {
            doc.text.matches(&self.find_query).count()
        } else {
            doc.text.to_lowercase().matches(&self.find_query.to_lowercase()).count()
        }
    }

    pub fn replace_one(&mut self, ctx: &egui::Context) {
        if let Some(sel) = self.selection_text(ctx) {
            let same = if self.match_case {
                sel == self.find_query
            } else {
                sel.to_lowercase() == self.find_query.to_lowercase()
            };
            if same {
                let (a, b) = self.sel(ctx);
                let r = self.replace_with.clone();
                self.replace_chars(ctx, a.min(b), a.max(b), &r, false);
            }
        }
        self.find_next(ctx, false);
    }

    pub fn replace_all(&mut self, ctx: &egui::Context) {
        let n = self.count_matches();
        if n == 0 {
            return;
        }
        let q = self.find_query.clone();
        let r = self.replace_with.clone();
        let match_case = self.match_case;
        let doc = &mut self.docs[self.active];
        let lower = doc.text.to_lowercase();
        let ql = q.to_lowercase();
        if match_case || lower.len() != doc.text.len() || ql.len() != q.len() {
            doc.text = doc.text.replace(&q, &r);
        } else {
            let mut out = String::with_capacity(doc.text.len());
            let mut last = 0;
            for (pos, _) in lower.match_indices(&ql) {
                out.push_str(&doc.text[last..pos]);
                out.push_str(&r);
                last = pos + ql.len();
            }
            out.push_str(&doc.text[last..]);
            doc.text = out;
        }
        self.flash(ctx, format!("{n} remplacement(s)"));
    }

    pub fn goto_line(&mut self, ctx: &egui::Context, line: usize) {
        self.remember_position(ctx);
        let doc = &mut self.docs[self.active];
        let l = line.saturating_sub(1).min(doc.line_count() - 1);
        let c = line_start_char(&doc.text, l);
        set_sel(ctx, editor_id(doc.id), c, c);
        doc.set_scroll = Some(l as f32 * doc.row_h - doc.view_h / 3.0);
    }
}
