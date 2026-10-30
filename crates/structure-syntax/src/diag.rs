//! Diagnostics reported while preparing a program for execution.
//!
//! Subclause 1.5.1 requires the system to report violations of the standard,
//! errors listed in annex E, uses of extensions and uses of
//! implementation-dependent features. Each diagnostic carries a
//! [`Category`] so that a user-controlled switch (see [`Policy`]) can decide
//! how the latter two are reported.

use crate::source::{SourceMap, Span};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Note,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Syntax error (annex B).
    Syntax,
    /// Violation of a requirement of the standard which is not an annex E error.
    Violation,
    /// An error condition listed in table E.1.
    AnnexE,
    /// Use of a feature that is an extension to the standard (1.5.1 e, f).
    Extension,
    /// Use of an implementation-dependent feature (1.5.1 g).
    ImplementationDependent,
    /// Advisory diagnostics (for example 2.4.3.2 initial value count warnings).
    Advisory,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub span: Option<Span>,
    /// Subclause of IEC 61131-3 the diagnostic refers to, e.g. `"2.1.5"`.
    pub clause: Option<&'static str>,
    pub notes: Vec<(Option<Span>, String)>,
}

impl Diagnostic {
    pub fn new(severity: Severity, category: Category, message: impl Into<String>, span: Option<Span>) -> Self {
        Diagnostic { severity, category, message: message.into(), span, clause: None, notes: Vec::new() }
    }

    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Error, Category::Violation, message, Some(span))
    }

    pub fn syntax(message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Error, Category::Syntax, message, Some(span)).with_clause("B")
    }

    pub fn annex_e(clause: &'static str, message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Error, Category::AnnexE, message, Some(span)).with_clause(clause)
    }

    pub fn extension(message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Warning, Category::Extension, message, Some(span))
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Diagnostic::new(Severity::Warning, Category::Advisory, message, Some(span))
    }

    pub fn with_clause(mut self, clause: &'static str) -> Self {
        self.clause = Some(clause);
        self
    }

    pub fn with_note(mut self, span: Option<Span>, note: impl Into<String>) -> Self {
        self.notes.push((span, note.into()));
        self
    }
}

/// User-controlled reporting switches ("be able to" in 1.5.1).
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// Report use of extensions as errors instead of warnings.
    pub extensions_are_errors: bool,
    /// Report use of implementation-dependent features as warnings.
    pub report_implementation_dependent: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy { extensions_are_errors: false, report_implementation_dependent: false }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(other);
    }

    pub fn error(&mut self, message: impl Into<String>, span: Span) {
        self.push(Diagnostic::error(message, span));
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.items.iter().filter(|d| d.severity == Severity::Error).count()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Applies a reporting policy: may promote extensions to errors and drop
    /// implementation-dependent notes.
    pub fn apply_policy(&mut self, policy: Policy) {
        self.items.retain(|d| {
            policy.report_implementation_dependent || d.category != Category::ImplementationDependent
        });
        if policy.extensions_are_errors {
            for d in &mut self.items {
                if d.category == Category::Extension {
                    d.severity = Severity::Error;
                }
            }
        }
    }

    /// Sorts diagnostics by file and position.
    pub fn sort(&mut self) {
        self.items.sort_by_key(|d| d.span.map(|s| (s.file, s.start)).unwrap_or_default());
    }

    pub fn render(&self, sources: &SourceMap) -> String {
        let mut out = String::new();
        for d in &self.items {
            render_one(&mut out, d, sources);
        }
        out
    }
}

pub fn render_one(out: &mut String, d: &Diagnostic, sources: &SourceMap) {
    let sev = match d.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    };
    let cat = match d.category {
        Category::Extension => " [extension]",
        Category::AnnexE => " [annex E]",
        Category::ImplementationDependent => " [implementation-dependent]",
        _ => "",
    };
    let clause = d.clause.map(|c| format!(" (IEC 61131-3 {})", c)).unwrap_or_default();
    let _ = writeln!(out, "{}{}: {}{}", sev, cat, d.message, clause);
    if let Some(span) = d.span {
        write_location(out, span, sources);
    }
    for (span, note) in &d.notes {
        let _ = writeln!(out, "  note: {}", note);
        if let Some(span) = span {
            write_location(out, *span, sources);
        }
    }
}

fn write_location(out: &mut String, span: Span, sources: &SourceMap) {
    if (span.file.0 as usize) >= sources.len() {
        return;
    }
    let file = sources.get(span.file);
    let (line, col) = file.line_col(span.start);
    let _ = writeln!(out, "  --> {}:{}:{}", file.name, line, col);
    let text = file.line_text(line);
    let _ = writeln!(out, "   | {}", text);
    let (end_line, end_col) = file.line_col(span.end.max(span.start));
    let width = if end_line == line { (end_col.saturating_sub(col)).max(1) } else { 1 };
    let _ = writeln!(out, "   | {}{}", " ".repeat(col as usize - 1), "^".repeat(width as usize));
}
