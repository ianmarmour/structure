//! Graphical languages (clause 4) in the semigraphic representation built
//! from the characters of table 1 (table 57 features 1, 3, 5, 7, 9, 11, 13).
//!
//! A network is represented as a set of nodes (contacts, coils, blocks,
//! variables, connectors, jumps, ...) whose pins are joined by nets. A net
//! with several drivers is a "wired OR" (LD only).

use crate::ast::{Body, Edge, Expr, ExprKind, GraphicBody, GraphicLanguage, Qualifier, Sfc, TransitionCondition};
use crate::token::{Token, TokenKind};
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
mod net;
mod scan;
mod sfc;


/// Parses a semigraphic body. `origin` is the byte offset in `file` of the
/// first character of `text` (which always starts at the beginning of a
/// line).
///
/// The drawing ends at the first line starting with one of the keywords
/// `ACTION`, `TRANSITION`, `STEP` or `INITIAL_STEP` (see [`textual_tail`]);
/// textual SFC elements from that line on are ignored here, except that
/// the names of textual `TRANSITION name ...` elements are used to recognise
/// transition names written next to graphical transitions (table 41
/// feature 7).
pub fn parse_graphic(file: &SourceFile, origin: u32, text: &str) -> (GraphicBody, Vec<Diagnostic>) {
    let mut grid = grid::Grid::new(file.id, origin, text);
    let mut tail_names = Vec::new();
    if let Some(n) = textual_tail(text) {
        grid.truncate(n);
        tail_names = textual_transition_names(text, n);
    }
    let span = Span::new(file.id, origin, origin + text.len() as u32);
    let (mut body, diags) = parse_grid(file, &grid, text.to_string(), span);
    if let Some(sfc) = body.sfc.as_deref_mut() {
        resolve_named_with(sfc, &tail_names);
        resolve_transition_names(sfc);
    }
    (body, diags)
}

/// Parses an already built grid (also used for drawings inside action
/// blocks).
pub(crate) fn parse_grid(file: &SourceFile, grid: &grid::Grid, text: String, span: Span) -> (GraphicBody, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    if sfc::looks_like_sfc(grid) {
        let (sfc, nets) = sfc::parse_sfc(file, grid, &mut diags);
        return (GraphicBody { language: GraphicLanguage::Sfc, networks: nets, sfc: Some(Box::new(sfc)), text, span }, diags);
    }
    let language = if ladder::has_left_rail(grid) { GraphicLanguage::Ld } else { GraphicLanguage::Fbd };
    let networks = ladder::parse_networks(file, grid, language, &mut diags);
    (GraphicBody { language, networks, sfc: None, text, span }, diags)
}

/// Index of the first line of `text` that starts (after white space) with
/// one of the keywords `ACTION`, `TRANSITION`, `STEP` or `INITIAL_STEP`,
/// i.e. where the textual SFC elements following a graphical SFC drawing
/// begin.
pub fn textual_tail(text: &str) -> Option<usize> {
    for (i, line) in text.split('\n').enumerate() {
        let t = line.trim_start();
        let word: String = t.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let up = word.to_ascii_uppercase();
        if matches!(up.as_str(), "ACTION" | "TRANSITION" | "STEP" | "INITIAL_STEP") {
            let rest = &t[word.len()..];
            if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace() || c == '(') {
                return Some(i);
            }
        }
    }
    None
}

/// Names of the `TRANSITION name ...` elements in the lines of `text` from
/// line `from` on.
fn textual_transition_names(text: &str, from: usize) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.split('\n').skip(from) {
        let mut words = line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| !w.is_empty());
        if words.next().is_some_and(|w| w.eq_ignore_ascii_case("TRANSITION")) {
            if let Some(n) = words.next() {
                if !n.eq_ignore_ascii_case("FROM") {
                    names.push(n.to_ascii_uppercase());
                }
            }
        }
    }
    names
}

/// A transition written next to a graphical transition as a single
/// identifier can be either a Boolean variable (`+ DN1`) or a transition
/// name (`+ TRAN78`, table 41 feature 7). The graphical parser produces
/// `TransitionCondition::St(<name>)` with `name: None`; this function turns
/// every such transition into `name: Some(<name>)`,
/// `condition: TransitionCondition::Named` when another (textual)
/// transition of the chart carries that name. It is idempotent.
pub fn resolve_transition_names(sfc: &mut Sfc) {
    let names: Vec<String> = sfc.transitions.iter().filter_map(|t| t.name.as_ref().map(|n| n.key())).collect();
    resolve_named_with(sfc, &names);
}

fn resolve_named_with(sfc: &mut Sfc, names: &[String]) {
    if names.is_empty() {
        return;
    }
    for t in &mut sfc.transitions {
        if t.name.is_some() {
            continue;
        }
        if let TransitionCondition::St(Expr { kind: ExprKind::Name(id), .. }) = &t.condition {
            if names.contains(&id.key()) {
                let id = id.clone();
                t.name = Some(id);
                t.condition = TransitionCondition::Named;
            }
        }
    }
}

/// Parses the ST expression written in the bytes `start..end` of `file`
/// (a variable or literal in a drawing, a transition condition, ...).
pub(crate) fn parse_expr_range(file: &SourceFile, start: u32, end: u32) -> (Expr, Vec<Diagnostic>) {
    let text = file.text.get(start as usize..end as usize).unwrap_or("");
    parse_expr_fragment(file, text, start)
}

/// Lexes `text`, which reproduces the bytes of `file` from `start` on
/// (possibly with some characters replaced by blanks), shifting the token
/// spans to their place in `file`.
pub(crate) fn lex_fragment(file: &SourceFile, text: &str, start: u32) -> Vec<Token> {
    let lexed = crate::lexer::lex(file.id, text);
    lexed
        .tokens
        .into_iter()
        .map(|mut t| {
            t.span.start += start;
            t.span.end += start;
            t
        })
        .collect()
}

pub(crate) fn parse_expr_fragment(file: &SourceFile, text: &str, start: u32) -> (Expr, Vec<Diagnostic>) {
    let toks = lex_fragment(file, text, start);
    let mut p = crate::parser::Parser::new(file, toks);
    let e = p.expr();
    if !p.at(TokenKind::Eof) {
        p.error_here("unexpected text after the expression");
    }
    (e, p.diags)
}

/// Parses an action body written inside a box ("d" field of an action
/// block, table 43 feature 4). `grid` holds only the interior of the box.
pub(crate) fn parse_box_body(file: &SourceFile, grid: &grid::Grid) -> (Body, Vec<Diagnostic>) {
    // Blank everything but the interior cells, keeping byte offsets.
    let Some(start) = grid.offsets.iter().flat_map(|r| r.iter()).min().copied() else {
        return (Body::Empty, Vec::new());
    };
    let end = grid.row_end.iter().copied().max().unwrap_or(start).max(start);
    let mut bytes: Vec<u8> = file.text.as_bytes()[start as usize..end as usize].iter().map(|b| if *b == b'\n' || *b == b'\r' { *b } else { b' ' }).collect();
    let mut any = false;
    for (r, row) in grid.rows.iter().enumerate() {
        for (c, ch) in row.iter().enumerate() {
            if *ch == ' ' {
                continue;
            }
            let o = grid.offsets[r][c];
            let e = grid.cell_end(r, c);
            if o < start || e > end {
                continue;
            }
            bytes[(o - start) as usize..(e - start) as usize].copy_from_slice(&file.text.as_bytes()[o as usize..e as usize]);
            any = true;
        }
    }
    if !any {
        return (Body::Empty, Vec::new());
    }
    let text = String::from_utf8(bytes).unwrap_or_default();
    let toks = lex_fragment(file, &text, start);
    let mut p = crate::parser::Parser::new(file, toks);
    let n = p.toks.len() - 1;
    match p.classify(0, n) {
        crate::parser::BodyLang::Graphic => {
            let span = Span::new(file.id, start, end);
            let (g, diags) = parse_grid(file, grid, grid.to_text(), span);
            (Body::Graphic(g), diags)
        }
        _ => {
            let body = p.body_region(0, n, start, end);
            (body, p.diags)
        }
    }
}
