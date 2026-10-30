//! Graphical representation of tasks and of their association with programs
//! and function blocks inside a RESOURCE (table 50 features 2a, 2b, 4a, 4b):
//!
//! ```text
//!            SLOW_1                       P1                  P2
//!          +---------+                +-------+     +--------------------+
//!          |  TASK   |                |   F   |     |         G          |
//!          |SINGLE   |                |       |     |   FB1              |
//! t#20ms---|INTERVAL |                +-------+     |  +-----+           |
//!      2---|PRIORITY |                |SLOW_1 |     |  |  A  |           |
//!          +---------+                +-------+     |  +-----+           |
//!                                                   |  |FAST_1|          |
//!                                                   |  +-----+           |
//!                                                   +--------------------+
//!                                                   |       SLOW_1       |
//!                                                   +--------------------+
//! ```
//!
//! A TASK box gives a task declaration whose inputs are the values connected
//! to SINGLE, INTERVAL and PRIORITY. Any other box is a program (at the top
//! level) or a function block instance (inside a program): its instance name
//! is written above it, its type in its first compartment and the task it is
//! associated with in its last compartment. Values connected to the left
//! side of a program box and variables connected to its right side are the
//! connections of the program configuration.

use super::graphic_decl::Row;
use super::Parser;
use crate::ast::*;
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::source::Span;
use crate::token::{Keyword, TokenKind};

struct DrawnBox {
    top: usize,
    bottom: usize,
    l: usize,
    r: usize,
    /// Rows of the compartment dividers (`+---+` across the box).
    dividers: Vec<usize>,
}

impl DrawnBox {
    fn contains(&self, other: &DrawnBox) -> bool {
        other.top > self.top && other.bottom < self.bottom && other.l > self.l && other.r < self.r
    }

    /// Interior rows of each compartment.
    fn compartments(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = self.top + 1;
        for &d in self.dividers.iter().chain(std::iter::once(&self.bottom)) {
            out.push((start, d));
            start = d + 1;
        }
        out
    }

    fn on_border(&self, r: usize, c: usize) -> bool {
        ((r == self.top || r == self.bottom || self.dividers.contains(&r)) && c >= self.l && c <= self.r)
            || ((c == self.l || c == self.r) && r >= self.top && r <= self.bottom)
    }
}

impl<'a> Parser<'a> {
    /// Whether the current token starts a drawing inside a resource.
    pub(crate) fn at_resource_drawing(&self) -> bool {
        matches!(self.kind(0), TokenKind::Ident | TokenKind::Plus | TokenKind::Pipe | TokenKind::Minus)
    }

    fn expr_at(&mut self, row: &Row, c0: usize, c1: usize) -> Option<Expr> {
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
        let e = self.expr();
        if self.pos < last {
            self.error_here("unexpected text in a graphical task declaration");
        }
        self.pos = saved_pos;
        self.limit = saved_limit;
        Some(e)
    }

    fn drawing_error(&mut self, msg: &str, span: Span) {
        self.diags.push(Diagnostic::error(msg.to_string(), span).with_clause("2.7.2"));
    }

    /// Parses the drawing that starts at the current token, up to the next
    /// line beginning with TASK, PROGRAM, a VAR keyword or END_RESOURCE.
    pub(crate) fn resource_drawing(&mut self, tasks: &mut Vec<TaskDecl>, programs: &mut Vec<ProgramConfig>) {
        let (first_line, _) = self.file.line_col(self.span().start);
        let mut end = self.pos;
        while end < self.limit {
            let t = &self.toks[end];
            let starts_line = {
                let (l, _) = self.file.line_col(t.span.start);
                let ls = self.file.line_start(l as usize - 1) as usize;
                self.file.text[ls..t.span.start as usize].trim().is_empty()
            };
            let item = matches!(
                t.kind,
                TokenKind::Kw(
                    Keyword::Task
                        | Keyword::Program
                        | Keyword::VarGlobal
                        | Keyword::VarExternal
                        | Keyword::Var
                        | Keyword::VarAccess
                        | Keyword::VarConfig
                        | Keyword::EndResource
                        | Keyword::EndConfiguration
                ) | TokenKind::Eof
            );
            if item && starts_line {
                break;
            }
            end += 1;
        }
        let last_line = if end < self.toks.len() { self.file.line_col(self.toks[end].span.start).0 } else { self.file.line_count() as u32 + 1 };
        let rows: Vec<Row> = (first_line..last_line).map(|l| self.grid_row(l)).collect();
        let saved_end = end;
        self.drawing(&rows, tasks, programs);
        self.pos = saved_end;
    }

    fn drawing(&mut self, rows: &[Row], tasks: &mut Vec<TaskDecl>, programs: &mut Vec<ProgramConfig>) {
        let at = |r: usize, c: usize| rows.get(r).map(|row| row.at(c)).unwrap_or(' ');
        // find the boxes
        let mut boxes: Vec<DrawnBox> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            for c in 0..row.chars.len() {
                if at(r, c) != '+' || at(r, c + 1) != '-' || at(r + 1, c) != '|' || boxes.iter().any(|b| b.on_border(r, c)) {
                    continue;
                }
                let mut rc = c + 1;
                while at(r, rc) == '-' {
                    rc += 1;
                }
                if at(r, rc) != '+' || at(r + 1, rc) != '|' {
                    continue;
                }
                // down the left side to the bottom; full-width `+---+` rows
                // followed by more sides are compartment dividers
                let mut dividers = Vec::new();
                let mut rr = r + 1;
                let bottom = loop {
                    match (at(rr, c), at(rr, rc)) {
                        ('|' | '>' | '<', '|') => rr += 1,
                        ('+', '+') if (c + 1..rc).all(|k| at(rr, k) == '-') => {
                            if at(rr + 1, c) == '|' && at(rr + 1, rc) == '|' {
                                dividers.push(rr);
                                rr += 1;
                            } else {
                                break Some(rr);
                            }
                        }
                        _ => break None,
                    }
                };
                match bottom {
                    Some(bottom) => boxes.push(DrawnBox { top: r, bottom, l: c, r: rc, dividers }),
                    None => {
                        let span = self.row_span(&rows[r], c, c + 1);
                        self.drawing_error("malformed box in the graphical representation of a resource", span);
                    }
                }
            }
        }
        // top-level boxes: tasks and programs
        for bi in 0..boxes.len() {
            if boxes.iter().any(|o| o.contains(&boxes[bi])) {
                continue;
            }
            let b = &boxes[bi];
            let inner: Vec<&DrawnBox> = boxes.iter().filter(|o| b.contains(o)).collect();
            let compartments = b.compartments();
            let (first_text, _) = self.first_text(rows, b, compartments[0], &inner);
            let is_task = first_text.as_ref().map(|i| i.text.eq_ignore_ascii_case("TASK")).unwrap_or(false);
            let name = self.name_above(rows, b);
            let span = Span::new(self.file.id, rows[b.top].off(b.l), rows[b.bottom].off(b.r) + 1);
            let Some(name) = name else {
                self.drawing_error("the name of the task or program instance shall be written above its box", span);
                continue;
            };
            if is_task {
                let mut task = TaskDecl { name, single: None, interval: None, priority: None, span };
                for r in compartments[0].0..compartments[0].1 {
                    let words = rows[r].words(b.l + 1, b.r);
                    let Some(&(w0, w1)) = words.first() else { continue };
                    let pin = rows[r].text(w0, w1).to_uppercase();
                    if pin == "TASK" {
                        continue;
                    }
                    let value = self.left_value(rows, r, b.l);
                    match (pin.as_str(), value) {
                        (_, None) => {}
                        ("SINGLE", Some(v)) => task.single = Some(v),
                        ("INTERVAL", Some(v)) => task.interval = Some(v),
                        ("PRIORITY", Some(v)) => task.priority = Some(v),
                        (_, Some(_)) => {
                            let span = self.row_span(&rows[r], w0, w1);
                            self.drawing_error("a TASK has the inputs SINGLE, INTERVAL and PRIORITY", span);
                        }
                    }
                }
                if task.priority.is_none() {
                    self.drawing_error("the PRIORITY of a task shall be given", span);
                }
                tasks.push(task);
                continue;
            }
            let Some(type_name) = first_text else {
                self.drawing_error("the type of the program shall be written in its box", span);
                continue;
            };
            let task = if compartments.len() > 1 { self.compartment_text(rows, b, *compartments.last().unwrap()) } else { None };
            let mut elements = Vec::new();
            // connections of the program
            for r in compartments[0].0..compartments[0].1 {
                let words = rows[r].words(b.l + 1, b.r);
                if let (Some(v), Some(&(w0, w1))) = (self.left_value(rows, r, b.l), words.first()) {
                    if w0 == b.l + 1 && !inner.iter().any(|i| i.top <= r && r <= i.bottom && i.l <= w0) {
                        let var = Ident::new(rows[r].text(w0, w1), self.row_span(&rows[r], w0, w1));
                        elements.push(ProgConfElement::Input { span: var.span.to(v.span), var, source: v });
                    }
                }
                if let (Some(v), Some(&(w0, w1))) = (self.right_value(rows, r, b.r), words.last()) {
                    if w1 == b.r {
                        let var = Ident::new(rows[r].text(w0, w1), self.row_span(&rows[r], w0, w1));
                        elements.push(ProgConfElement::Output { span: var.span.to(v.span), var, sink: v });
                    }
                }
            }
            // function block instances associated with tasks (4b)
            for fb in inner {
                let fc = fb.compartments();
                let Some(fb_name) = self.name_above(rows, fb) else {
                    let span = self.row_span(&rows[fb.top], fb.l, fb.r + 1);
                    self.drawing_error("the name of the function block instance shall be written above its box", span);
                    continue;
                };
                let fb_task = if fc.len() > 1 { self.compartment_text(rows, fb, *fc.last().unwrap()) } else { None };
                match fb_task {
                    Some(t) => {
                        let span = fb_name.span.to(t.span);
                        let fb = Expr { kind: ExprKind::Name(fb_name.clone()), span: fb_name.span };
                        elements.push(ProgConfElement::FbTask { fb, task: t, span });
                    }
                    None => {
                        let span = self.row_span(&rows[fb.top], fb.l, fb.r + 1);
                        self.drawing_error("the task of a function block instance shall be written in the last compartment of its box", span);
                    }
                }
            }
            programs.push(ProgramConfig { name, retain: None, task, type_name, elements, span });
        }
    }

    /// The first word inside a compartment, outside nested boxes.
    fn first_text(&self, rows: &[Row], b: &DrawnBox, (r0, r1): (usize, usize), inner: &[&DrawnBox]) -> (Option<Ident>, usize) {
        for (r, row) in rows.iter().enumerate().take(r1).skip(r0) {
            for (w0, w1) in row.words(b.l + 1, b.r) {
                if inner.iter().any(|i| r >= i.top.saturating_sub(1) && r <= i.bottom && w1 > i.l && w0 <= i.r) {
                    continue;
                }
                // pins with connections are not the type name
                if (w0 == b.l + 1 && row.at(b.l.wrapping_sub(1)) == '-') || (w1 == b.r && row.at(b.r + 1) == '-') {
                    continue;
                }
                return (Some(Ident::new(row.text(w0, w1), self.row_span(row, w0, w1))), r);
            }
        }
        (None, r0)
    }

    fn compartment_text(&self, rows: &[Row], b: &DrawnBox, (r0, r1): (usize, usize)) -> Option<Ident> {
        (r0..r1).find_map(|r| rows[r].words(b.l + 1, b.r).first().map(|&(w0, w1)| Ident::new(rows[r].text(w0, w1), self.row_span(&rows[r], w0, w1))))
    }

    /// The instance name written above a box.
    fn name_above(&self, rows: &[Row], b: &DrawnBox) -> Option<Ident> {
        let r = b.top.checked_sub(1)?;
        let words = rows[r].words(b.l.saturating_sub(2), b.r + 3);
        let &(w0, w1) = words.first()?;
        Some(Ident::new(rows[r].text(w0, w1), self.row_span(&rows[r], w0, w1)))
    }

    /// The value connected to the left side of a box at row `r`.
    fn left_value(&mut self, rows: &[Row], r: usize, l: usize) -> Option<Expr> {
        let row = &rows[r];
        if l == 0 || row.at(l - 1) != '-' {
            return None;
        }
        let mut c = l - 1;
        while c > 0 && row.at(c - 1) == '-' {
            c -= 1;
        }
        let mut s = c;
        while s > 0 && !row.at(s - 1).is_whitespace() && !matches!(row.at(s - 1), '|' | '+') {
            s -= 1;
        }
        if s == c {
            return None;
        }
        self.expr_at(row, s, c)
    }

    /// The variable connected to the right side of a box at row `r`.
    fn right_value(&mut self, rows: &[Row], r: usize, rc: usize) -> Option<Expr> {
        let row = &rows[r];
        if row.at(rc + 1) != '-' {
            return None;
        }
        let mut c = rc + 1;
        while row.at(c) == '-' {
            c += 1;
        }
        let mut e = c;
        while e < row.chars.len() && !row.at(e).is_whitespace() && !matches!(row.at(e), '|' | '+') {
            e += 1;
        }
        if e == c {
            return None;
        }
        self.expr_at(row, c, e)
    }
}
