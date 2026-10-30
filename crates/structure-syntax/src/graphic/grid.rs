//! Character grid of a semigraphic drawing.

use crate::source::{FileId, Span};

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
            }
            if in_comment {
                let end = c == ')' && i > 0 && chars[i - 1].1 == '*' && cur.len() > 0;
                cur.push(' ');
                cur_off.push(off);
                if end {
                    in_comment = false;
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

    pub fn get(&self, r: isize, c: isize) -> char {
        if r < 0 || c < 0 {
            return ' ';
        }
        self.rows.get(r as usize).and_then(|row| row.get(c as usize)).copied().unwrap_or(' ')
    }

    pub fn offset(&self, r: usize, c: usize) -> u32 {
        match self.offsets.get(r) {
            Some(row) => row.get(c).copied().unwrap_or_else(|| self.row_end[r]),
            None => *self.row_end.last().unwrap_or(&0),
        }
    }

    pub fn span(&self, r: usize, c0: usize, c1: usize) -> Span {
        let s = self.offset(r, c0);
        let e = if c1 > c0 { self.offset(r, c1 - 1) + 1 } else { s };
        Span::new(self.file, s, e.max(s))
    }
}
