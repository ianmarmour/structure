//! Sequential Function Charts (2.6).

use super::*;
use structure_syntax::ast::Sfc;

impl<'c, 's> Lowerer<'c, 's> {
    pub fn sfc_body(&mut self, _out: &mut Vec<St>, sfc: &Sfc) {
        self.cx.error("SFC is not supported yet", sfc.span);
    }
}
