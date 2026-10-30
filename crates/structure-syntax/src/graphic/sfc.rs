//! Graphical SFC.

use super::grid::Grid;
use super::Network;
use crate::ast::Sfc;
use crate::diag::Diagnostic;
use crate::source::SourceFile;

pub fn looks_like_sfc(_grid: &Grid) -> bool {
    false
}

pub fn parse_sfc(_file: &SourceFile, _grid: &Grid, _diags: &mut Vec<Diagnostic>) -> (Sfc, Vec<Network>) {
    (Sfc::default(), Vec::new())
}
