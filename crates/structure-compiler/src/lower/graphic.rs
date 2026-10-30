//! LD and FBD networks (clause 4).

use super::*;
use structure_syntax::ast::GraphicBody;

impl<'c, 's> Lowerer<'c, 's> {
    pub fn graphic_body(&mut self, _out: &mut Vec<St>, g: &GraphicBody) {
        self.cx.error("graphical languages are not supported yet", g.span);
    }
}

impl<'c, 's> Lowerer<'c, 's> {
    pub fn graphic_condition(&mut self, _out: &mut Vec<St>, g: &GraphicBody, _name: Option<&structure_syntax::Ident>) -> Ex {
        self.cx.error("graphical transition conditions are not supported yet", g.span);
        Ex::bool(false)
    }
}
