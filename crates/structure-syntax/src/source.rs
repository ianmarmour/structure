//! Source files, spans and line/column mapping.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// Identifies a source file inside a [`SourceMap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// A byte range inside a source file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Span {
        Span { file, start, end }
    }

    /// Returns the smallest span covering both `self` and `other`.
    pub fn to(self, other: Span) -> Span {
        if self.file != other.file {
            return self;
        }
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}..{}", self.file.0, self.start, self.end)
    }
}

/// A single source text.
pub struct SourceFile {
    pub id: FileId,
    pub name: String,
    pub text: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(id: FileId, name: impl Into<String>, text: impl Into<String>) -> SourceFile {
        let text = text.into();
        let mut line_starts = vec![0u32];
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => line_starts.push((i + 1) as u32),
                b'\r' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                        i += 1;
                    }
                    line_starts.push((i + 1) as u32);
                }
                // U+2029 PARAGRAPH SEPARATOR (E2 80 A9), the EOL of annex A.
                0xE2 if i + 2 < bytes.len() && bytes[i + 1] == 0x80 && bytes[i + 2] == 0xA9 => {
                    i += 2;
                    line_starts.push((i + 1) as u32);
                }
                _ => {}
            }
            i += 1;
        }
        SourceFile { id, name: name.into(), text, line_starts }
    }

    /// 1-based line and column (in characters) of a byte offset.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let start = self.line_starts[line] as usize;
        let end = (offset as usize).min(self.text.len());
        let col = self.text.get(start..end).map(|s| s.chars().count()).unwrap_or(0);
        (line as u32 + 1, col as u32 + 1)
    }

    /// Byte offset of the start of the given 0-based line.
    pub fn line_start(&self, line0: usize) -> u32 {
        self.line_starts.get(line0).copied().unwrap_or(self.text.len() as u32)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Text of the given 1-based line, without the line terminator.
    pub fn line_text(&self, line: u32) -> &str {
        let idx = (line as usize).saturating_sub(1);
        let start = self.line_starts.get(idx).copied().unwrap_or(0) as usize;
        let end = self
            .line_starts
            .get(idx + 1)
            .map(|e| *e as usize)
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r', '\u{2029}'])
    }

    pub fn slice(&self, span: Span) -> &str {
        &self.text[span.start as usize..span.end as usize]
    }
}

/// Collection of all source files of a compilation.
#[derive(Default, Clone)]
pub struct SourceMap {
    files: Vec<Arc<SourceFile>>,
}

impl SourceMap {
    pub fn new() -> SourceMap {
        SourceMap::default()
    }

    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(Arc::new(SourceFile::new(id, name, text)));
        id
    }

    /// Loads a file from disk. Text that is not valid UTF-8 is decoded as
    /// ISO 8859-1 (Latin-1), which maps byte-for-byte onto row 00 of
    /// ISO/IEC 10646-1 as required by 2.1.1.
    pub fn load(&mut self, path: &Path) -> std::io::Result<FileId> {
        let bytes = std::fs::read(path)?;
        let text = decode_source(&bytes);
        Ok(self.add(path.display().to_string(), text))
    }

    pub fn get(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn get_arc(&self, id: FileId) -> Arc<SourceFile> {
        self.files[id.0 as usize].clone()
    }

    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.iter().map(|f| f.as_ref())
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Decodes source bytes as UTF-8, falling back to Latin-1.
pub fn decode_source(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}
