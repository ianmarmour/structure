//! Graphical languages (clause 4) in the semigraphic representation built
//! from the characters of table 1 (table 57 features 1, 3, 5, 7, 9, 11, 13).
//!
//! A network is represented as a set of nodes (contacts, coils, blocks,
//! variables, connectors, jumps, ...) whose pins are joined by nets. A net
//! with several drivers is a "wired OR" (LD only).

use crate::ast::{Edge, Expr, GraphicBody, GraphicLanguage, Qualifier};
use crate::diag::Diagnostic;
use crate::ident::Ident;
use crate::source::{SourceFile, Span};

#[derive(Clone, Debug)]
pub struct Network {
    pub label: Option<Ident>,
    pub nodes: Vec<Node>,
    pub nets: Vec<Net>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
    /// Position (row, column) of the node in the drawing, used to order the
    /// evaluation of otherwise independent elements (top to bottom, left to
    /// right).
    pub pos: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContactKind {
    /// `--| |--` normally open.
    NormallyOpen,
    /// `--|/|--` normally closed.
    NormallyClosed,
    /// `--|P|--` positive transition-sensing.
    Positive,
    /// `--|N|--` negative transition-sensing.
    Negative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CoilKind {
    /// `--( )--`
    Normal,
    /// `--(/)--`
    Negated,
    /// `--(S)--`
    Set,
    /// `--(R)--`
    Reset,
    /// `--(P)--`
    Positive,
    /// `--(N)--`
    Negative,
}

#[derive(Clone, Debug)]
pub struct BlockPin {
    /// Formal parameter name shown inside the block, if any.
    pub name: Option<Ident>,
    /// Graphical negation `O` (table 19).
    pub negated: bool,
    /// Edge marker `>` / `<` (table 33 features 9a/9b).
    pub edge: Option<Edge>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum NodeKind {
    /// LD left power rail: every pin is a driver of the ON state.
    LeftRail,
    /// LD right power rail: every pin is a sink.
    RightRail,
    Contact { var: Expr, kind: ContactKind },
    Coil { var: Expr, kind: CoilKind },
    /// Function, function block or operator block.
    Block { type_name: Ident, instance: Option<Ident>, inputs: Vec<BlockPin>, outputs: Vec<BlockPin> },
    /// A variable or literal whose value drives a line (left side).
    Source(Expr),
    /// A variable assigned from a line (right side).
    Sink(Expr),
    /// `--->>LABEL`
    Jump(Ident),
    /// `---<RETURN>`
    Return,
    /// `--->NAME>`: end of a line continued elsewhere.
    ConnectorOut(Ident),
    /// `>NAME>---`: continuation of a connected line.
    ConnectorIn(Ident),
    /// Action block used inside a network (table 44 features 8, 9).
    ActionBlock { qualifier: Option<Qualifier>, time: Option<Expr>, action: Ident, indicator: Option<Expr> },
}

/// Pin reference: (node index, pin index). For blocks, input pins are
/// numbered from 0 and output pins likewise in a separate numbering. Contacts
/// and coils have input 0 (left) and output 0 (right). Sources have output 0,
/// sinks and jumps input 0. Rails use the row number as pin index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinRef {
    pub node: usize,
    pub pin: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Net {
    pub drivers: Vec<PinRef>,
    pub sinks: Vec<PinRef>,
}

mod grid;
mod ladder;
mod sfc;

/// Parses a semigraphic body. `origin` is the byte offset in `file` of the
/// first character of `text` (which always starts at the beginning of a
/// line).
pub fn parse_graphic(file: &SourceFile, origin: u32, text: &str) -> (GraphicBody, Vec<Diagnostic>) {
    let grid = grid::Grid::new(file.id, origin, text);
    let mut diags = Vec::new();
    let span = Span::new(file.id, origin, origin + text.len() as u32);
    if sfc::looks_like_sfc(&grid) {
        let (sfc, nets) = sfc::parse_sfc(file, &grid, &mut diags);
        return (
            GraphicBody { language: GraphicLanguage::Sfc, networks: nets, sfc: Some(Box::new(sfc)), text: text.to_string(), span },
            diags,
        );
    }
    let language = if ladder::has_left_rail(&grid) { GraphicLanguage::Ld } else { GraphicLanguage::Fbd };
    let networks = ladder::parse_networks(file, &grid, language, &mut diags);
    (GraphicBody { language, networks, sfc: None, text: text.to_string(), span }, diags)
}
