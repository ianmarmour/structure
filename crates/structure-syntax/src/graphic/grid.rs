//! Character grid of a semigraphic drawing.

use crate::source::{FileId, Span};

#[derive(Clone)]
pub struct Grid {
    pub file: FileId,
    /// Rows of characters (comments are blanked out).
    pub rows: Vec<Vec<char>>,
    /// Byte offset in the file of each character of each row.
    pub offsets: Vec<Vec<u32>>,
    /// Byte offset of the end of each row.
    pub row_end: Vec<u32>,
}

impl Grid {
    pub fn new(file: FileId, origin: u32, text: &str) -> Grid {
        let mut rows = Vec::new();
        let mut offsets = Vec::new();
        let mut row_end = Vec::new();
        let mut cur = Vec::new();
        let mut cur_off = Vec::new();
        let mut in_comment = false;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut i = 0;
        while i < chars.len() {
            let (off, c) = chars[i];
            let off = origin + off as u32;
            if c == '\n' || c == '\u{2029}' || c == '\r' {
                if c == '\r' && i + 1 < chars.len() && chars[i + 1].1 == '\n' {
                    i += 1;
                }
                rows.push(std::mem::take(&mut cur));
                offsets.push(std::mem::take(&mut cur_off));
                row_end.push(off);
                i += 1;
                continue;
            }
            if !in_comment && c == '(' && i + 1 < chars.len() && chars[i + 1].1 == '*' {
                in_comment = true;
                // blank both characters of the opening delimiter
                cur.push(' ');
                cur_off.push(off);
                cur.push(' ');
                cur_off.push(origin + chars[i + 1].0 as u32);
                i += 2;
                continue;
            }
            if in_comment {
                let end = c == '*' && i + 1 < chars.len() && chars[i + 1].1 == ')';
                cur.push(' ');
                cur_off.push(off);
                if end {
                    cur.push(' ');
                    cur_off.push(origin + chars[i + 1].0 as u32);
                    in_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if c == '\t' {
                // tabs expand to the next multiple of 8 columns
                let n = 8 - cur.len() % 8;
                for _ in 0..n {
                    cur.push(' ');
                    cur_off.push(off);
                }
            } else {
                // Normalise look-alike characters produced by some editors
                // and document converters (broken bar, dashes).
                let c = match c {
                    '\u{a6}' => '|',
                    '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
                    _ => c,
                };
                cur.push(c);
                cur_off.push(off);
            }
            i += 1;
        }
        rows.push(cur);
        offsets.push(cur_off);
        row_end.push(origin + text.len() as u32);
        Grid { file, rows, offsets, row_end }
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// Length of the longest row.
    pub fn width(&self) -> usize {
        self.rows.iter().map(|r| r.len()).max().unwrap_or(0)
    }

    /// Character at (row, column), a space outside the drawing.
    pub fn get(&self, r: isize, c: isize) -> char {
        if r < 0 || c < 0 {
            return ' ';
        }
        self.rows.get(r as usize).and_then(|row| row.get(c as usize)).copied().unwrap_or(' ')
    }

    /// Character at (row, column) with unsigned coordinates.
    pub fn at(&self, r: usize, c: usize) -> char {
        self.rows.get(r).and_then(|row| row.get(c)).copied().unwrap_or(' ')
    }

    pub fn offset(&self, r: usize, c: usize) -> u32 {
        match self.offsets.get(r) {
            Some(row) => row.get(c).copied().unwrap_or_else(|| self.row_end[r]),
            None => *self.row_end.last().unwrap_or(&0),
        }
    }

    /// Byte offset just after the character at (row, column).
    pub fn cell_end(&self, r: usize, c: usize) -> u32 {
        let Some(row) = self.offsets.get(r) else {
            return *self.row_end.last().unwrap_or(&0);
        };
        let Some(&start) = row.get(c) else {
            return self.row_end[r];
        };
        for &o in &row[c + 1..] {
            if o > start {
                return o;
            }
        }
        self.row_end[r].max(start)
    }

    /// Span of the columns `c0..c1` (exclusive) of row `r`.
    pub fn span(&self, r: usize, c0: usize, c1: usize) -> Span {
        let s = self.offset(r, c0);
        let e = if c1 > c0 { self.cell_end(r, c1 - 1) } else { s };
        Span::new(self.file, s, e.max(s))
    }

    /// Span from cell (r0, c0) to cell (r1, c1), both inclusive.
    pub fn rect_span(&self, r0: usize, c0: usize, r1: usize, c1: usize) -> Span {
        let s = self.offset(r0, c0);
        let e = self.cell_end(r1, c1);
        Span::new(self.file, s.min(e), e.max(s))
    }

    /// Span covering whole rows `r0..=r1`.
    pub fn rows_span(&self, r0: usize, r1: usize) -> Span {
        let r1 = r1.min(self.height().saturating_sub(1));
        let s = self.offsets.get(r0).and_then(|r| r.first().copied()).unwrap_or_else(|| self.row_end.get(r0).copied().unwrap_or(0));
        let e = self.row_end.get(r1).copied().unwrap_or(s);
        Span::new(self.file, s.min(e), e.max(s))
    }

    /// Text of the cells `c0..=c1` of row `r`.
    pub fn text(&self, r: usize, c0: usize, c1: usize) -> String {
        (c0..=c1).map(|c| self.at(r, c)).collect()
    }

    /// Drops the rows from `n` on.
    pub fn truncate(&mut self, n: usize) {
        if n < self.rows.len() {
            self.rows.truncate(n);
            self.offsets.truncate(n);
            self.row_end.truncate(n);
        }
    }

    /// Copy of the rectangle `r0..=r1` x `c0..=c1` of the drawing (rows are
    /// renumbered from 0, columns are kept; everything outside the columns
    /// is blanked). Offsets still refer to the original file.
    pub fn subgrid(&self, r0: usize, r1: usize, c0: usize, c1: usize) -> Grid {
        let mut rows = Vec::new();
        let mut offsets = Vec::new();
        let mut row_end = Vec::new();
        for r in r0..=r1.min(self.height().saturating_sub(1)) {
            let mut row = Vec::new();
            let mut off = Vec::new();
            for c in 0..=c1 {
                let ch = if c >= c0 { self.at(r, c) } else { ' ' };
                row.push(ch);
                off.push(self.offset(r, c));
            }
            while row.last() == Some(&' ') {
                row.pop();
                off.pop();
            }
            rows.push(row);
            offsets.push(off);
            row_end.push(self.cell_end(r, c1));
        }
        if rows.is_empty() {
            rows.push(Vec::new());
            offsets.push(Vec::new());
            row_end.push(self.offset(r0, c0));
        }
        Grid { file: self.file, rows, offsets, row_end }
    }

    /// The drawing as text (one line per row).
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for (i, r) in self.rows.iter().enumerate() {
            if i > 0 {
                s.push('\n');
            }
            let line: String = r.iter().collect();
            s.push_str(line.trim_end());
        }
        s
    }
}
