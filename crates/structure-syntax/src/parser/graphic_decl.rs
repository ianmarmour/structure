//! Graphical declaration of the external interface of a program
//! organization unit (figures 6 b, 11 and 12 a; table 20a features 2, 3;
//! table 33 features 4b, 5b, 6b, 9a, 9b):
//!
//! ```text
//!          +-----------+
//!          | STACK_INT |
//! BOOL---->PUSH  EMPTY|---BOOL     (rising edge input, output)
//! BOOL----|R1      OUT|---INT
//! INT-----|A---------A|---INT      (in-out variable)
//!          +-----------+
//! ```
//!
//! For a function, the output pin without a name gives the type of the
//! result. EN and ENO, which are declared implicitly, may be drawn.

use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::source::Span;

/// A line of text by column (tabs expanded to multiples of 8), with the
/// byte offset of each column.
struct Row {
    chars: Vec<char>,
    offs: Vec<u32>,
    end: u32,
}

impl Row {
    fn at(&self, c: usize) -> char {
        self.chars.get(c).copied().unwrap_or(' ')
    }

    fn off(&self, c: usize) -> u32 {
        self.offs.get(c).copied().unwrap_or(self.end)
    }

    /// Words (identifier characters) in columns `c0..c1`.
    fn words(&self, c0: usize, c1: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut c = c0;
        while c < c1 {
            if is_word(self.at(c)) {
                let s = c;
                while c < c1 && is_word(self.at(c)) {
                    c += 1;
                }
                out.push((s, c));
            } else {
                c += 1;
            }
        }
        out
    }

    fn text(&self, c0: usize, c1: usize) -> String {
        (c0..c1).map(|c| self.at(c)).collect()
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(crate) struct Interface {
    pub name: Ident,
    pub result: Option<TypeSpec>,
    pub blocks: Vec<VarBlock>,
}

impl<'a> Parser<'a> {
    fn grid_row(&self, line: u32) -> Row {
        let start = self.file.line_start(line as usize - 1);
        let text = self.file.line_text(line);
        let (mut chars, mut offs) = (Vec::new(), Vec::new());
        for (i, ch) in text.char_indices() {
            let off = start + i as u32;
            if ch == '\t' {
                for _ in 0..8 - chars.len() % 8 {
                    chars.push(' ');
                    offs.push(off);
                }
            } else {
                chars.push(ch);
                offs.push(off);
            }
        }
        Row { chars, offs, end: start + text.len() as u32 }
    }

    fn row_span(&self, row: &Row, c0: usize, c1: usize) -> Span {
        Span::new(self.file.id, row.off(c0), if c1 >= row.chars.len() { row.end } else { row.off(c1) })
    }

    /// Parses the type written at columns `c0..c1` of a row from the tokens
    /// of that text.
    fn type_at(&mut self, row: &Row, c0: usize, c1: usize) -> Option<TypeSpec> {
        let span = self.row_span(row, c0, c1);
        let first = self.toks.iter().position(|t| t.span.start >= span.start)?;
        let mut last = first;
        while last < self.toks.len() && self.toks[last].span.start < span.end {
            last += 1;
        }
        if first == last {
            return None;
        }
        let (saved_pos, saved_limit) = (self.pos, self.limit);
        self.pos = first;
        self.limit = last;
        let ty = self.type_spec();
        if self.pos < last {
            self.error_here("unexpected text in the type of a graphical declaration");
        }
        self.pos = saved_pos;
        self.limit = saved_limit;
        Some(ty)
    }

    fn box_error(&mut self, msg: &str, span: Span) {
        self.diags.push(Diagnostic::error(msg.to_string(), span));
    }

    /// Parses a graphical interface declaration whose upper left corner is
    /// the current `+` token, and moves past the lines of the box.
    pub(crate) fn graphic_interface(&mut self, function: bool) -> Option<Interface> {
        let top_off = self.span().start;
        let (line0, _) = self.file.line_col(top_off);
        let top = self.grid_row(line0);
        let l = top.offs.iter().position(|&o| o == top_off)?;
        let mut r = l + 1;
        while matches!(top.at(r), '-' | '=') {
            r += 1;
        }
        if top.at(r) != '+' || r == l + 1 {
            self.box_error("malformed graphical declaration: expected the upper side of a box", self.span());
            return None;
        }
        // sides and lower side of the box
        let mut rows = Vec::new();
        let mut line = line0 + 1;
        let bottom = loop {
            if line as usize > self.file.line_count() {
                self.box_error("the box of the graphical declaration is not closed", self.span());
                return None;
            }
            let row = self.grid_row(line);
            if row.at(l) == '+' && row.at(r) == '+' && (l + 1..r).all(|c| matches!(row.at(c), '-' | '=')) {
                break row;
            }
            if !matches!(row.at(l), '|' | '>' | '<') || row.at(r) != '|' {
                let span = self.row_span(&row, l, l + 1);
                self.box_error("malformed graphical declaration: expected the sides of the box", span);
                return None;
            }
            rows.push(row);
            line += 1;
        };
        let box_span = Span::new(self.file.id, top_off, bottom.off(r) + 1);

        let mut name: Option<Ident> = None;
        let mut result: Option<TypeSpec> = None;
        let (mut inputs, mut outputs, mut in_outs) = (Vec::new(), Vec::new(), Vec::new());
        for row in &rows {
            let words = row.words(l + 1, r);
            let left = l > 0 && row.at(l - 1) == '-';
            let right = row.at(r + 1) == '-';
            let edge = match row.at(l) {
                '>' => Some(Edge::Rising),
                '<' => Some(Edge::Falling),
                _ => None,
            };
            let ident = |p: &Self, (c0, c1): (usize, usize)| Ident::new(row.text(c0, c1), p.row_span(row, c0, c1));
            if !left && !right {
                // the name of the POU
                match (words.as_slice(), &name) {
                    ([], _) => {}
                    ([w], None) => name = Some(ident(self, *w)),
                    _ => {
                        let span = self.row_span(row, l + 1, r);
                        self.box_error("unexpected text inside the box of a graphical declaration", span);
                    }
                }
                continue;
            }
            // types outside the box
            let left_ty = if left {
                let mut c = l - 1;
                while c > 0 && row.at(c - 1) == '-' {
                    c -= 1;
                }
                let s = (0..c).find(|&i| row.at(i) != ' ').unwrap_or(c);
                let ty = self.type_at(row, s, c);
                if ty.is_none() {
                    let span = self.row_span(row, l - 1, l);
                    self.box_error("the type of an input shall be written at the left of the graphical declaration", span);
                }
                ty.map(|t| (t, row.text(s, c).trim().to_uppercase()))
            } else {
                None
            };
            let right_ty = if right {
                let mut c = r + 1;
                while row.at(c) == '-' {
                    c += 1;
                }
                let mut e = c;
                while e < row.chars.len() && !row.at(e).is_whitespace() {
                    e += 1;
                }
                let ty = self.type_at(row, c, e);
                if ty.is_none() {
                    let span = self.row_span(row, r, r + 1);
                    self.box_error("the type of an output shall be written at the right of the graphical declaration", span);
                }
                ty.map(|t| (t, row.text(c, e).to_uppercase()))
            } else {
                None
            };
            let in_out = left && right && words.len() == 2 && {
                let gap = row.text(words[0].1, words[1].0);
                gap.contains('-') && gap.chars().all(|c| c == '-')
            };
            let decl = |name: Ident, spec: TypeSpec, edge: Option<Edge>| {
                let span = name.span;
                VarDecl { names: vec![name], location: None, spec, edge, init: None, span }
            };
            if in_out {
                let (a, b) = (ident(self, words[0]), ident(self, words[1]));
                if a.key() != b.key() {
                    self.box_error("both ends of an in-out variable shall have the same name", b.span);
                }
                if let (Some((lt, ltext)), Some((_, rtext))) = (left_ty, right_ty) {
                    if ltext != rtext {
                        let span = self.row_span(row, r + 1, row.chars.len());
                        self.box_error("both ends of an in-out variable shall have the same type", span);
                    }
                    if edge.is_some() {
                        self.box_error("edge detection can only be declared for inputs", a.span);
                    }
                    in_outs.push(decl(a, lt, None));
                }
                continue;
            }
            let mut words = words.as_slice();
            if let Some((ty, text)) = left_ty {
                match words.first() {
                    Some(w) if words.len() <= 2 => {
                        let n = ident(self, *w);
                        words = &words[1..];
                        if n.key() == "EN" && text == "BOOL" && edge.is_none() {
                            // implicit (2.5.1.2)
                        } else {
                            inputs.push(decl(n, ty, edge));
                        }
                    }
                    _ => {
                        let span = self.row_span(row, l, r);
                        self.box_error("an input of a graphical declaration shall be named inside the box", span);
                    }
                }
            } else if edge.is_some() {
                let span = self.row_span(row, l, l + 1);
                self.box_error("edge detection can only be declared for inputs", span);
            }
            if let Some((ty, text)) = right_ty {
                match words {
                    [w] => {
                        let n = ident(self, *w);
                        if !(n.key() == "ENO" && text == "BOOL") {
                            outputs.push(decl(n, ty, None));
                        }
                    }
                    [] if function && result.is_none() => result = Some(ty),
                    _ => {
                        let span = self.row_span(row, l, r + 1);
                        self.box_error(
                            if function { "an output of a graphical declaration shall be named (only the result of the function is unnamed)" } else { "an output of a graphical declaration shall be named inside the box" },
                            span,
                        );
                    }
                }
            } else if !words.is_empty() {
                let span = self.row_span(row, l + 1, r);
                self.box_error("unexpected text inside the box of a graphical declaration", span);
            }
        }
        let Some(name) = name else {
            self.box_error("the name of the program organization unit shall be written inside the box", box_span);
            return None;
        };
        // move past the lines of the box
        let next = self.file.line_start(line as usize);
        while self.pos < self.limit && self.toks[self.pos].span.start < next {
            self.pos += 1;
        }
        let mut blocks = Vec::new();
        for (kind, decls) in [(VarKind::Input, inputs), (VarKind::InOut, in_outs), (VarKind::Output, outputs)] {
            if !decls.is_empty() {
                blocks.push(VarBlock { kind, qualifier: None, items: VarItems::Vars(decls), span: box_span });
            }
        }
        Some(Interface { name, result, blocks })
    }
}
