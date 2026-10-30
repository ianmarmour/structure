//! LD and FBD networks.

use super::grid::Grid;
use super::Network;
use crate::ast::GraphicLanguage;
use crate::diag::Diagnostic;
use crate::source::SourceFile;

pub fn has_left_rail(_grid: &Grid) -> bool {
    false
}

pub fn parse_networks(_file: &SourceFile, grid: &Grid, _lang: GraphicLanguage, diags: &mut Vec<Diagnostic>) -> Vec<Network> {
    diags.push(Diagnostic::error("graphical languages are not supported yet", grid.span(0, 0, 0)));
    Vec::new()
}
