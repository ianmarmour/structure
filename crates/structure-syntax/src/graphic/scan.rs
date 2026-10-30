//! Decomposition of a drawing into graphical primitives: boxes (blocks,
//! steps, action blocks), symbols (contacts, coils, jumps, returns,
//! connectors), text runs and line cells, plus the connectivity of the line
//! cells (union-find).

use super::grid::Grid;
use super::{CoilKind, ContactKind};
use crate::source::Span;

/// Classification of a grid cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cell {
    /// Not (yet) classified: blank or unprocessed character.
    Free,
    /// Border or interior of box `i`.
    Box(u32),
    /// Part of symbol `i`.
    Elem(u32),
    /// Part of text run `i`.
    Tok(u32),
    /// Line character taking part in connectivity.
    Line,
    /// Part of power rail `i` (LD).
    Rail(u32),
    /// SFC transition cross `i`.
    Trans(u32),
    /// Consumed by something else (labels, negation circles, transition
    /// condition text, ...).
    Used,
}

/// A rectangle drawn with `+` corners, `-`/`=` horizontal borders and `|`
/// vertical borders.
#[derive(Clone, Debug)]
pub(crate) struct BoxRect {
    pub r0: usize,
    pub c0: usize,
    pub r1: usize,
    pub c1: usize,
    /// Double horizontal borders (`+===+`), i.e. an initial step.
    pub double: bool,
    /// Double vertical borders (`||`).
    pub double_bar: bool,
    /// Columns of internal vertical dividers (action blocks).
    pub dividers: Vec<usize>,
}

impl BoxRect {
    /// First and last interior column.
    pub fn inner_cols(&self) -> (usize, usize) {
        if self.double_bar && self.c1 >= self.c0 + 4 {
            (self.c0 + 2, self.c1 - 2)
        } else {
            (self.c0 + 1, self.c1 - 1)
        }
    }

    pub fn contains(&self, r: usize, c: usize) -> bool {
        r >= self.r0 && r <= self.r1 && c >= self.c0 && c <= self.c1
    }

    /// True if `other` lies strictly inside the interior of `self`.
    pub fn encloses(&self, other: &BoxRect) -> bool {
        other.r0 > self.r0 && other.r1 < self.r1 && other.c0 > self.c0 && other.c1 < self.c1
    }

    /// Words of the interior row `r` between columns `a..=b`: (start, end, text).
    pub fn words(g: &Grid, r: usize, a: usize, b: usize) -> Vec<(usize, usize, String)> {
        let mut out = Vec::new();
        let mut c = a;
        while c <= b {
            if g.at(r, c) == ' ' {
                c += 1;
                continue;
            }
            let s = c;
            while c <= b && g.at(r, c) != ' ' {
                c += 1;
            }
            out.push((s, c - 1, g.text(r, s, c - 1)));
        }
        out
    }
}

/// Graphical symbols recognised outside boxes.
#[derive(Clone, Debug)]
pub(crate) enum ElemKind {
    Contact(ContactKind),
    Coil(CoilKind),
    Jump,
    Return,
    ConnOut,
    ConnIn,
}

#[derive(Clone, Debug)]
pub(crate) struct Elem {
    pub kind: ElemKind,
    pub row: usize,
    pub c0: usize,
    /// Last column (inclusive).
    pub c1: usize,
    /// Columns of the name for jumps and connectors.
    pub name_cols: (usize, usize),
}

/// A run of text (variable, literal, name, ...).
#[derive(Clone, Debug)]
pub(crate) struct Tok {
    pub row: usize,
    pub c0: usize,
    pub c1: usize,
    pub text: String,
    /// A line ends at the left of the text (it is assigned from the line).
    pub left: Option<(usize, usize)>,
    /// A line starts at the right of the text (its value drives the line).
    pub right: Option<(usize, usize)>,
    pub used: bool,
}

impl Tok {
    pub fn center2(&self) -> usize {
        self.c0 + self.c1
    }
    pub fn is_free(&self) -> bool {
        self.left.is_none() && self.right.is_none() && !self.used
    }
}

/// A vertical power rail segment (LD).
#[derive(Clone, Debug)]
pub(crate) struct RailSeg {
    pub col: usize,
    pub r0: usize,
    pub r1: usize,
    pub left: bool,
}

/// Union-find over grid cells and additional pin elements.
pub(crate) struct Uf {
    parent: Vec<u32>,
}

impl Uf {
    pub fn new(n: usize) -> Uf {
        Uf { parent: (0..n as u32).collect() }
    }
    pub fn add(&mut self) -> usize {
        let i = self.parent.len();
        self.parent.push(i as u32);
        i
    }
    pub fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] as usize != x {
            let p = self.parent[x] as usize;
            self.parent[x] = self.parent[p];
            x = p;
        }
        x
    }
    pub fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            if a < b {
                self.parent[b] = a as u32;
            } else {
                self.parent[a] = b as u32;
            }
        }
    }
}

pub(crate) fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || (!c.is_ascii() && c.is_alphabetic())
}

pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (!c.is_ascii() && c.is_alphanumeric())
}

/// Characters that can form lines.
pub(crate) fn is_line_char(c: char) -> bool {
    matches!(c, '-' | '|' | '+' | '=' | '*')
}

/// The drawing being decomposed.
pub(crate) struct Scene<'g> {
    pub g: &'g Grid,
    pub h: usize,
    pub w: usize,
    pub cells: Vec<Cell>,
    pub boxes: Vec<BoxRect>,
    pub elems: Vec<Elem>,
    pub toks: Vec<Tok>,
    pub rails: Vec<RailSeg>,
    /// Network labels: (row, text, span).
    pub labels: Vec<(usize, String, Span)>,
}

impl<'g> Scene<'g> {
    pub fn new(g: &'g Grid) -> Scene<'g> {
        let h = g.height();
        let w = g.width() + 2;
        Scene { g, h, w, cells: vec![Cell::Free; h * w], boxes: Vec::new(), elems: Vec::new(), toks: Vec::new(), rails: Vec::new(), labels: Vec::new() }
    }

    pub fn idx(&self, r: usize, c: usize) -> usize {
        r * self.w + c
    }

    pub fn cell(&self, r: isize, c: isize) -> Cell {
        if r < 0 || c < 0 || r as usize >= self.h || c as usize >= self.w {
            return Cell::Free;
        }
        self.cells[r as usize * self.w + c as usize]
    }

    pub fn cellu(&self, r: usize, c: usize) -> Cell {
        if r >= self.h || c >= self.w {
            return Cell::Free;
        }
        self.cells[r * self.w + c]
    }

    pub fn set(&mut self, r: usize, c: usize, v: Cell) {
        if r < self.h && c < self.w {
            let i = r * self.w + c;
            self.cells[i] = v;
        }
    }

    pub fn ch(&self, r: isize, c: isize) -> char {
        self.g.get(r, c)
    }

    pub fn chu(&self, r: usize, c: usize) -> char {
        self.g.at(r, c)
    }

    pub fn is_free(&self, r: usize, c: usize) -> bool {
        self.cellu(r, c) == Cell::Free
    }

    pub fn is_line(&self, r: isize, c: isize) -> bool {
        self.cell(r, c) == Cell::Line
    }

    // ------------------------------------------------------------ labels

    /// Finds network label lines (`NAME:` alone on a line) and blanks them.
    pub fn find_labels(&mut self) {
        for r in 0..self.h {
            let row = &self.g.rows[r];
            let first = row.iter().position(|c| *c != ' ');
            let last = row.iter().rposition(|c| *c != ' ');
            let (Some(a), Some(b)) = (first, last) else { continue };
            if b <= a || row[b] != ':' {
                continue;
            }
            let name: String = row[a..b].iter().collect();
            let name = name.trim_end().to_string();
            let ok = if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                name.chars().all(|c| c.is_ascii_digit())
            } else {
                name.chars().next().is_some_and(is_ident_start) && name.chars().all(is_ident_char)
            };
            if !ok {
                continue;
            }
            let span = self.g.span(r, a, a + name.chars().count());
            for c in a..=b {
                self.set(r, c, Cell::Used);
            }
            self.labels.push((r, name, span));
        }
    }

    // ------------------------------------------------------------ boxes

    /// Finds all rectangles. Boxes nested inside other boxes (drawings in
    /// action bodies) are dropped: they are parsed with the enclosing body.
    pub fn find_boxes(&mut self) {
        let g = self.g;
        let h = self.h;
        let mut out: Vec<BoxRect> = Vec::new();
        for r in 0..h {
            let len = g.rows[r].len();
            for c in 0..len {
                if self.cellu(r, c) != Cell::Free || g.at(r, c) != '+' {
                    continue;
                }
                let right = g.at(r, c + 1);
                if right != '-' && right != '=' {
                    continue;
                }
                if !matches!(g.at(r + 1, c), '|' | '>' | '<') {
                    continue;
                }
                // left side down to the bottom-left corner
                let mut r1 = r + 1;
                let found = loop {
                    if r1 >= h {
                        break false;
                    }
                    match g.at(r1, c) {
                        '|' | '>' | '<' => r1 += 1,
                        '+' => {
                            let rr = g.at(r1, c + 1);
                            if rr == '-' || rr == '=' {
                                break true;
                            }
                            if c > 0 && g.at(r1, c - 1) == '-' {
                                r1 += 1;
                            } else {
                                break false;
                            }
                        }
                        _ => break false,
                    }
                };
                if !found || r1 < r + 2 {
                    continue;
                }
                let mut cands = Vec::new();
                let mut x = c + 1;
                while matches!(g.at(r, x), '-' | '=' | '+') {
                    if g.at(r, x) == '+' {
                        cands.push(x);
                    }
                    x += 1;
                }
                let side_ok = |x: usize| {
                    (r + 1..r1).all(|rr| {
                        let ch = g.at(rr, x);
                        ch == '|' || (ch == '+' && g.at(rr, x + 1) == '-')
                    })
                };
                let bottom_ok = |x: usize| g.at(r1, x) == '+' && (c..=x).all(|xx| matches!(g.at(r1, xx), '-' | '=' | '+'));
                let mut c1 = None;
                for &x in cands.iter().rev() {
                    if side_ok(x) && bottom_ok(x) {
                        c1 = Some(x);
                        break;
                    }
                }
                let Some(c1) = c1 else { continue };
                if c1 < c + 2 {
                    continue;
                }
                let dividers: Vec<usize> =
                    cands.iter().copied().filter(|&x| x < c1 && g.at(r1, x) == '+' && (r + 1..r1).all(|rr| g.at(rr, x) == '|')).collect();
                let double = (c..=c1).any(|xx| g.at(r, xx) == '=') || (c..=c1).any(|xx| g.at(r1, xx) == '=');
                let double_bar = c1 >= c + 4 && (r + 1..r1).all(|rr| g.at(rr, c + 1) == '|' && g.at(rr, c1 - 1) == '|');
                // the inner bars of a double box are not dividers
                let dividers = dividers.into_iter().filter(|&x| !(double_bar && (x == c + 1 || x + 1 == c1))).collect();
                out.push(BoxRect { r0: r, c0: c, r1, c1, double, double_bar, dividers });
            }
        }
        // drop boxes drawn inside other boxes, and the parts of action
        // blocks starting at a divider
        let keep: Vec<bool> = (0..out.len())
            .map(|i| {
                let b = &out[i];
                !out.iter().enumerate().any(|(j, o)| {
                    j != i && (o.encloses(b) || (o.r0 == b.r0 && o.r1 == b.r1 && o.c0 < b.c0 && b.c1 <= o.c1))
                })
            })
            .collect();
        let mut boxes = Vec::new();
        for (b, k) in out.into_iter().zip(keep) {
            if k {
                boxes.push(b);
            }
        }
        for (i, b) in boxes.iter().enumerate() {
            for r in b.r0..=b.r1 {
                for c in b.c0..=b.c1 {
                    if self.cellu(r, c) == Cell::Free {
                        self.set(r, c, Cell::Box(i as u32));
                    }
                }
            }
        }
        self.boxes = boxes;
    }

    /// Box whose region contains the cell, if any.
    pub fn box_at(&self, r: usize, c: usize) -> Option<usize> {
        match self.cellu(r, c) {
            Cell::Box(i) => Some(i as usize),
            _ => self.boxes.iter().position(|b| b.contains(r, c)),
        }
    }

    // ------------------------------------------------------------ symbols

    fn lineish(&self, r: usize, c: isize) -> bool {
        c >= 0 && self.cell(r as isize, c) == Cell::Free && matches!(self.ch(r as isize, c), '-' | '+' | '=')
    }

    fn vertical_neighbour(&self, r: usize, c: usize) -> bool {
        let up = if r > 0 { self.chu(r - 1, c) } else { ' ' };
        let down = self.chu(r + 1, c);
        matches!(up, '|' | '+') && (r == 0 || self.cellu(r - 1, c) == Cell::Free) || matches!(down, '|' | '+') && self.cellu(r + 1, c) == Cell::Free
    }

    fn read_name(&self, r: usize, c: usize) -> usize {
        let mut e = c;
        while is_ident_char(self.chu(r, e)) && self.is_free(r, e) {
            e += 1;
        }
        e
    }

    /// Recognises contacts, coils, jumps, returns and connectors.
    pub fn find_symbols(&mut self) {
        for r in 0..self.h {
            let len = self.g.rows[r].len();
            let mut c = 0;
            while c < len {
                if !self.is_free(r, c) {
                    c += 1;
                    continue;
                }
                let ch = self.chu(r, c);
                let ci = c as isize;
                let mut found: Option<Elem> = None;
                if (ch == '|' || ch == '!') && self.lineish(r, ci - 1) {
                    let m = self.chu(r, c + 1);
                    let e = self.chu(r, c + 2);
                    let kind = match m {
                        ' ' => Some(ContactKind::NormallyOpen),
                        '/' => Some(ContactKind::NormallyClosed),
                        'P' | 'p' => Some(ContactKind::Positive),
                        'N' | 'n' => Some(ContactKind::Negative),
                        _ => None,
                    };
                    if let Some(k) = kind {
                        if e == ch
                            && self.is_free(r, c + 1)
                            && self.is_free(r, c + 2)
                            && self.lineish(r, ci + 3)
                            && !self.vertical_neighbour(r, c)
                            && !self.vertical_neighbour(r, c + 2)
                        {
                            found = Some(Elem { kind: ElemKind::Contact(k), row: r, c0: c, c1: c + 2, name_cols: (0, 0) });
                        }
                    } else if m == ch
                        && self.is_free(r, c + 1)
                        && self.lineish(r, ci + 2)
                        && !self.vertical_neighbour(r, c)
                        && !self.vertical_neighbour(r, c + 1)
                    {
                        found = Some(Elem { kind: ElemKind::Contact(ContactKind::NormallyOpen), row: r, c0: c, c1: c + 1, name_cols: (0, 0) });
                    }
                } else if ch == '(' && self.lineish(r, ci - 1) {
                    let m = self.chu(r, c + 1);
                    let kind = match m {
                        ' ' => Some(CoilKind::Normal),
                        '/' => Some(CoilKind::Negated),
                        'S' | 's' => Some(CoilKind::Set),
                        'R' | 'r' => Some(CoilKind::Reset),
                        'P' | 'p' => Some(CoilKind::Positive),
                        'N' | 'n' => Some(CoilKind::Negative),
                        _ => None,
                    };
                    if let (Some(k), ')') = (kind, self.chu(r, c + 2)) {
                        found = Some(Elem { kind: ElemKind::Coil(k), row: r, c0: c, c1: c + 2, name_cols: (0, 0) });
                    } else if m == ')' {
                        found = Some(Elem { kind: ElemKind::Coil(CoilKind::Normal), row: r, c0: c, c1: c + 1, name_cols: (0, 0) });
                    }
                } else if ch == '>' && self.chu(r, c + 1) == '>' && self.lineish(r, ci - 1) {
                    let e = self.read_name(r, c + 2);
                    if e > c + 2 {
                        found = Some(Elem { kind: ElemKind::Jump, row: r, c0: c, c1: e - 1, name_cols: (c + 2, e - 1) });
                    }
                } else if ch == '<' && self.lineish(r, ci - 1) {
                    let e = self.read_name(r, c + 1);
                    let name = if e > c + 1 { self.g.text(r, c + 1, e - 1) } else { String::new() };
                    if name.eq_ignore_ascii_case("RETURN") && self.chu(r, e) == '>' {
                        found = Some(Elem { kind: ElemKind::Return, row: r, c0: c, c1: e, name_cols: (c + 1, e - 1) });
                    }
                } else if ch == '>' && is_ident_start(self.chu(r, c + 1)) {
                    let e = self.read_name(r, c + 1);
                    if self.chu(r, e) == '>' && self.is_free(r, e) {
                        if self.lineish(r, ci - 1) {
                            found = Some(Elem { kind: ElemKind::ConnOut, row: r, c0: c, c1: e, name_cols: (c + 1, e - 1) });
                        } else if self.lineish(r, e as isize + 1) {
                            found = Some(Elem { kind: ElemKind::ConnIn, row: r, c0: c, c1: e, name_cols: (c + 1, e - 1) });
                        }
                    }
                }
                if let Some(el) = found {
                    let i = self.elems.len() as u32;
                    for x in el.c0..=el.c1 {
                        self.set(r, x, Cell::Elem(i));
                    }
                    c = el.c1 + 1;
                    self.elems.push(el);
                } else {
                    c += 1;
                }
            }
        }
    }

    // ------------------------------------------------------------ text

    /// Turns every remaining run of non-line characters into a text run.
    pub fn find_text(&mut self) {
        for r in 0..self.h {
            let len = self.g.rows[r].len();
            let mut c = 0;
            while c < len {
                if !self.is_free(r, c) || self.chu(r, c) == ' ' {
                    c += 1;
                    continue;
                }
                let ch = self.chu(r, c);
                let prev = if c > 0 { self.chu(r, c - 1) } else { ' ' };
                let prev_line = c > 0 && is_line_char(prev) && self.is_free(r, c - 1);
                let signed = (ch == '-' || ch == '+') && self.chu(r, c + 1).is_ascii_digit() && !prev_line;
                if (is_line_char(ch) || self.is_arrow(r, c)) && !signed {
                    c += 1;
                    continue;
                }
                let c0 = c;
                let mut sq = 0i32;
                let mut par = 0i32;
                loop {
                    if c >= len || !self.is_free(r, c) {
                        break;
                    }
                    let ch = self.chu(r, c);
                    if ch == '\'' || ch == '"' {
                        let q = ch;
                        c += 1;
                        while c < len && self.chu(r, c) != q {
                            if self.chu(r, c) == '$' {
                                c += 1;
                            }
                            c += 1;
                        }
                        c = (c + 1).min(len);
                        continue;
                    }
                    match ch {
                        '[' => {
                            sq += 1;
                            c += 1;
                            continue;
                        }
                        ']' => {
                            sq -= 1;
                            c += 1;
                            continue;
                        }
                        '(' => {
                            par += 1;
                            c += 1;
                            continue;
                        }
                        ')' => {
                            par -= 1;
                            c += 1;
                            continue;
                        }
                        _ => {}
                    }
                    if sq > 0 || par > 0 {
                        c += 1;
                        continue;
                    }
                    if ch == ' ' {
                        break;
                    }
                    if ch == '-' || ch == '+' {
                        if c == c0 {
                            c += 1;
                            continue;
                        }
                        if self.chu(r, c + 1).is_ascii_digit() && self.is_free(r, c + 1) && self.chu(r, c - 1) != ' ' {
                            c += 1;
                            continue;
                        }
                        break;
                    }
                    if matches!(ch, '|' | '=' | '*' | '<' | '>' | '!') {
                        if c == c0 {
                            c += 1;
                        }
                        break;
                    }
                    c += 1;
                }
                let c1 = c.max(c0 + 1) - 1;
                let text = self.g.text(r, c0, c1);
                let i = self.toks.len() as u32;
                for x in c0..=c1 {
                    self.set(r, x, Cell::Tok(i));
                }
                self.toks.push(Tok { row: r, c0, c1, text, left: None, right: None, used: false });
                c = c1 + 1;
            }
        }
    }

    /// `>` or `<` between two `-` (direction arrows of table 46 feature 7).
    pub fn is_arrow(&self, r: usize, c: usize) -> bool {
        matches!(self.chu(r, c), '<' | '>') && c > 0 && self.chu(r, c - 1) == '-' && self.chu(r, c + 1) == '-'
    }

    // ------------------------------------------------------------ lines

    /// Marks all remaining line characters as line cells.
    pub fn mark_lines(&mut self) {
        for r in 0..self.h {
            let len = self.g.rows[r].len();
            for c in 0..len {
                if self.is_free(r, c) && (is_line_char(self.chu(r, c)) || self.is_arrow(r, c)) {
                    self.set(r, c, Cell::Line);
                }
            }
        }
    }

    /// Horizontal line character of a line cell (`-`, `=`, arrows).
    pub fn h_line(&self, r: isize, c: isize) -> bool {
        self.is_line(r, c) && matches!(self.ch(r, c), '-' | '=' | '<' | '>')
    }

    /// Junction character of a line cell.
    pub fn j_line(&self, r: isize, c: isize) -> bool {
        self.is_line(r, c) && matches!(self.ch(r, c), '+' | '*')
    }

    /// Vertical line character of a line cell.
    pub fn v_line(&self, r: isize, c: isize) -> bool {
        self.is_line(r, c) && self.ch(r, c) == '|'
    }

    /// Text runs attached to lines: a line ending at the left of the text
    /// assigns the text, a line starting at its right is driven by it. A
    /// single blank between the text and a free line end is tolerated.
    pub fn attach_text(&mut self) {
        for i in 0..self.toks.len() {
            let (r, c0, c1) = (self.toks[i].row as isize, self.toks[i].c0 as isize, self.toks[i].c1 as isize);
            if self.h_line(r, c0 - 1) && self.ch(r, c0 - 1) == '-' {
                self.toks[i].left = Some((r as usize, (c0 - 1) as usize));
            }
            if self.h_line(r, c1 + 1) && self.ch(r, c1 + 1) == '-' {
                self.toks[i].right = Some((r as usize, (c1 + 1) as usize));
            }
        }
        // tolerate a single blank between text and line
        for i in 0..self.toks.len() {
            let (r, c0, c1) = (self.toks[i].row as isize, self.toks[i].c0 as isize, self.toks[i].c1 as isize);
            if self.toks[i].right.is_none() && self.ch(r, c1 + 1) == ' ' && self.cell(r, c1 + 1) == Cell::Free && self.h_line(r, c1 + 2) && self.ch(r, c1 + 2) == '-' {
                self.toks[i].right = Some((r as usize, (c1 + 2) as usize));
            }
            if self.toks[i].left.is_none()
                && c0 >= 2
                && self.ch(r, c0 - 1) == ' '
                && self.cell(r, c0 - 1) == Cell::Free
                && self.h_line(r, c0 - 2)
                && self.ch(r, c0 - 2) == '-'
                && self.toks[i].right.is_some()
            {
                // only a value that also drives a line is read this way
                // (`--OUT -1 --|`): leave it as a source
            } else if self.toks[i].left.is_none()
                && c0 >= 2
                && self.ch(r, c0 - 1) == ' '
                && self.cell(r, c0 - 1) == Cell::Free
                && self.h_line(r, c0 - 2)
                && self.ch(r, c0 - 2) == '-'
            {
                self.toks[i].left = Some((r as usize, (c0 - 2) as usize));
            }
        }
    }

    /// Vertical runs of `|`/`+` line cells, passing through crossings
    /// drawn with `-`: (column, first row, last row).
    pub fn vertical_segments(&self) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for c in 0..self.w {
            let mut r = 0;
            while r < self.h {
                let vert = |s: &Self, r: usize| {
                    s.cellu(r, c) == Cell::Line
                        && (matches!(s.chu(r, c), '|' | '+')
                            || (s.chu(r, c) == '-'
                                && r > 0
                                && s.cellu(r - 1, c) == Cell::Line
                                && matches!(s.chu(r - 1, c), '|' | '+')
                                && s.cellu(r + 1, c) == Cell::Line
                                && matches!(s.chu(r + 1, c), '|' | '+')))
                };
                if vert(self, r) {
                    let r0 = r;
                    while r < self.h && vert(self, r) {
                        r += 1;
                    }
                    out.push((c, r0, r - 1));
                } else {
                    r += 1;
                }
            }
        }
        out
    }

    /// Horizontal attachments of a vertical segment: (rows attached on the
    /// left, rows attached on the right). Crossings are not attachments.
    pub fn seg_attachments(&self, c: usize, r0: usize, r1: usize) -> (Vec<usize>, Vec<usize>) {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for r in r0..=r1 {
            let ri = r as isize;
            let ci = c as isize;
            let l = self.h_line(ri, ci - 1) || self.j_line(ri, ci - 1) || matches!(self.cell(ri, ci - 1), Cell::Elem(_));
            let rt = self.h_line(ri, ci + 1) || self.j_line(ri, ci + 1) || matches!(self.cell(ri, ci + 1), Cell::Elem(_));
            if (self.chu(r, c) == '|' && l && rt) || self.chu(r, c) == '-' {
                continue; // crossing without connection
            }
            if l {
                left.push(r);
            }
            if rt {
                right.push(r);
            }
        }
        (left, right)
    }

    /// Turns the given vertical segment into a power rail.
    pub fn make_rail(&mut self, c: usize, r0: usize, r1: usize, left: bool) -> usize {
        let i = self.rails.len();
        for r in r0..=r1 {
            if self.chu(r, c) != '-' {
                self.set(r, c, Cell::Rail(i as u32));
            }
        }
        self.rails.push(RailSeg { col: c, r0, r1, left });
        i
    }

    /// Connectivity of line cells (table 57): `-`/`=` connect horizontally,
    /// `|` vertically, `+`/`*` in all directions; a line crossing another
    /// one through a `|` (or `-`) does not connect, a line ending at another
    /// one connects.
    pub fn connect_lines(&self, uf: &mut Uf, skip: &dyn Fn(usize, usize) -> bool) {
        for r in 0..self.h {
            for c in 0..self.w {
                if self.cellu(r, c) != Cell::Line || skip(r, c) {
                    continue;
                }
                let (ri, ci) = (r as isize, c as isize);
                let me = self.chu(r, c);
                let here = self.idx(r, c);
                let ok = |rr: isize, cc: isize| self.is_line(rr, cc) && !skip(rr as usize, cc as usize);
                // horizontal neighbour on the right
                if ok(ri, ci + 1) {
                    let other = self.chu(r, c + 1);
                    let me_h = me != '|';
                    let other_h = other != '|';
                    if me_h && other_h {
                        uf.union(here, self.idx(r, c + 1));
                    } else if me_h && !other_h {
                        // `-|`: crossing if the line continues on the far side
                        let far = ok(ri, ci + 2) && self.chu(r, c + 2) != '|';
                        if far {
                            uf.union(here, self.idx(r, c + 2));
                            if !self.v_line(ri - 1, ci + 1) && !self.v_line(ri + 1, ci + 1) && !self.j_line(ri - 1, ci + 1) && !self.j_line(ri + 1, ci + 1) {
                                uf.union(here, self.idx(r, c + 1));
                            }
                        } else if !matches!(me, '+' | '*') {
                            // a line ending at a vertical line joins it
                            uf.union(here, self.idx(r, c + 1));
                        }
                    } else if !me_h && other_h {
                        let near = ok(ri, ci - 1) && self.chu(r, c - 1) != '|';
                        if !near && !matches!(other, '+' | '*') {
                            uf.union(here, self.idx(r, c + 1));
                        }
                    }
                }
                // vertical neighbour below
                if ok(ri + 1, ci) {
                    let other = self.chu(r + 1, c);
                    let me_v = matches!(me, '|' | '+' | '*');
                    let other_v = matches!(other, '|' | '+' | '*');
                    if me_v && other_v {
                        uf.union(here, self.idx(r + 1, c));
                    } else if me_v && other == '-' {
                        let far = ok(ri + 2, ci) && matches!(self.chu(r + 2, c), '|' | '+' | '*');
                        if far {
                            uf.union(here, self.idx(r + 2, c));
                        } else if me == '|' {
                            // a vertical line ending at a horizontal one joins it
                            uf.union(here, self.idx(r + 1, c));
                        }
                    } else if me == '-' && other_v {
                        let near = ok(ri - 1, ci) && matches!(self.chu(r - 1, c), '|' | '+' | '*');
                        if !near && other == '|' {
                            uf.union(here, self.idx(r + 1, c));
                        }
                    }
                }
            }
        }
    }
}
