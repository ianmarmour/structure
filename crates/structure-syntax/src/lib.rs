//! Syntax of the IEC 61131-3 (second edition, 2003) programming languages:
//! common elements, IL, ST, SFC, configurations (textual form, annex B) and
//! the semigraphic form of LD, FBD and SFC (clause 4).

pub mod ast;
pub mod diag;
pub mod graphic;
pub mod ident;
pub mod lexer;
pub mod literal;
pub mod parser;
pub mod source;
pub mod token;

pub use diag::{Diagnostic, Diagnostics, Severity};
pub use ident::Ident;
pub use source::{FileId, SourceFile, SourceMap, Span};
