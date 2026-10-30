//! Instruction List (3.2).

use super::*;
use structure_syntax::ast::IlInstr;

impl<'c, 's> Lowerer<'c, 's> {
    pub fn il_body(&mut self, _out: &mut Vec<St>, list: &[IlInstr]) {
        if let Some(i) = list.first() {
            self.cx.error("IL is not supported yet", i.span);
        }
    }
}
