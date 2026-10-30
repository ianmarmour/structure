//! Identifiers (2.1.2). The case of letters is not significant, so every
//! identifier carries a normalised upper-case key used for comparison.

use crate::source::Span;
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub struct Ident {
    /// Spelling as written in the source.
    pub text: String,
    pub span: Span,
}

impl Ident {
    pub fn new(text: impl Into<String>, span: Span) -> Ident {
        Ident { text: text.into(), span }
    }

    /// Normalised (upper-case) form used for all name lookups.
    pub fn key(&self) -> String {
        normalize(&self.text)
    }

    pub fn is(&self, upper: &str) -> bool {
        self.text.eq_ignore_ascii_case(upper)
    }
}

impl PartialEq for Ident {
    fn eq(&self, other: &Self) -> bool {
        self.text.eq_ignore_ascii_case(&other.text)
    }
}
impl Eq for Ident {}

impl Hash for Ident {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state)
    }
}

impl fmt::Debug for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

/// Normalises an identifier for comparison.
pub fn normalize(s: &str) -> String {
    s.to_uppercase()
}

/// Checks the identifier rules of 2.1.2 and returns a description of the
/// problem, if any. Leading underlines are allowed (table 2, feature 3);
/// multiple leading or embedded underlines and trailing underlines are not.
pub fn check_identifier(s: &str) -> Option<&'static str> {
    if s.ends_with('_') {
        return Some("trailing underline characters are not allowed in identifiers");
    }
    if s.contains("__") {
        return Some("multiple consecutive underline characters are not allowed in identifiers");
    }
    None
}
