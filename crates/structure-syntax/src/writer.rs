//! Writes library elements back as text in the syntax of annex B
//! (1.5.1 j). Graphical bodies are written in their semigraphic form.

use crate::ast::*;
use std::fmt::Write;

pub fn write_unit(unit: &SourceUnit) -> String {
    let mut w = Writer::default();
    for (i, el) in unit.elements.iter().enumerate() {
        if i > 0 {
            w.out.push('\n');
        }
        w.element(el);
    }
    w.out
}

#[derive(Default)]
struct Writer {
    out: String,
    indent: usize,
}

fn lit_text(l: &Literal) -> String {
    let mut s = String::new();
    if let Some(p) = &l.type_prefix {
        s.push_str(&p.text.to_uppercase());
        s.push('#');
    }
    match &l.kind {
        LiteralKind::Int { value, negative, based } => {
            if *negative {
                s.push('-');
            }
            if *based && l.type_prefix.as_ref().map(|p| !p.is("BOOL")).unwrap_or(true) {
                let _ = write!(s, "16#{:X}", value);
            } else {
                let _ = write!(s, "{}", value);
            }
        }
        LiteralKind::Real { text, negative } => {
            if *negative {
                s.push('-');
            }
            s.push_str(&text.replace('e', "E"));
        }
        LiteralKind::Bool(b) => s.push_str(if *b { "TRUE" } else { "FALSE" }),
        LiteralKind::String { wide, chars } => {
            let wide_quotes = *wide && l.type_prefix.as_ref().map(|p| !p.is("WSTRING")).unwrap_or(true);
            let q = if wide_quotes { '"' } else { '\'' };
            s.push(q);
            for &c in chars {
                match c {
                    0x24 => s.push_str("$$"),
                    0x0A => s.push_str("$N"),
                    0x0D => s.push_str("$R"),
                    0x09 => s.push_str("$T"),
                    0x0C => s.push_str("$P"),
                    c if c == q as u32 => {
                        s.push('$');
                        s.push(q);
                    }
                    c if (0x20..0x7F).contains(&c) => s.push(char::from_u32(c).unwrap()),
                    c => {
                        if wide_quotes || *wide {
                            let _ = write!(s, "${:04X}", c);
                        } else {
                            let _ = write!(s, "${:02X}", c);
                        }
                    }
                }
            }
            s.push(q);
        }
        LiteralKind::Duration(d) => {
            s.push_str("T#");
            if d.negative {
                s.push('-');
            }
            s.push_str(&duration_text(d.nanos));
        }
        LiteralKind::TimeOfDay(ns) => {
            let _ = write!(s, "TOD#{}", daytime_text(*ns));
        }
        LiteralKind::Date(d) => {
            let _ = write!(s, "D#{:04}-{:02}-{:02}", d.year, d.month, d.day);
        }
        LiteralKind::DateAndTime(d, ns) => {
            let _ = write!(s, "DT#{:04}-{:02}-{:02}-{}", d.year, d.month, d.day, daytime_text(*ns));
        }
    }
    s
}

fn duration_text(ns: u128) -> String {
    if ns == 0 {
        return "0s".into();
    }
    let mut v = ns;
    let mut s = String::new();
    for (unit, size) in [("d", 86_400_000_000_000u128), ("h", 3_600_000_000_000), ("m", 60_000_000_000), ("s", 1_000_000_000)] {
        if v >= size {
            let _ = write!(s, "{}{}", v / size, unit);
            v %= size;
        }
    }
    if v > 0 {
        let ms = v / 1_000_000;
        let frac = v % 1_000_000;
        if frac == 0 {
            let _ = write!(s, "{}ms", ms);
        } else {
            let f = format!("{:06}", frac);
            let _ = write!(s, "{}.{}ms", ms, f.trim_end_matches('0'));
        }
    }
    s
}

fn daytime_text(ns: u64) -> String {
    let secs = ns / 1_000_000_000;
    let frac = ns % 1_000_000_000;
    let mut s = format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60);
    if frac != 0 {
        let f = format!("{:09}", frac);
        s.push('.');
        s.push_str(f.trim_end_matches('0'));
    }
    s
}

/// Text of an expression with minimal parentheses.
pub fn expr_text(e: &Expr) -> String {
    expr_prec(e, 0)
}

fn expr_prec(e: &Expr, min: u8) -> String {
    match &e.kind {
        ExprKind::Literal(l) => lit_text(l),
        ExprKind::Name(i) => i.text.clone(),
        ExprKind::TypedEnum { ty, value } => format!("{}#{}", ty.text, value.text),
        ExprKind::DirectVar(d) => d.to_text(),
        ExprKind::Field { base, field } => format!("{}.{}", expr_prec(base, 10), field.text),
        ExprKind::Index { base, indices } => {
            let ix: Vec<String> = indices.iter().map(expr_text).collect();
            format!("{}[{}]", expr_prec(base, 10), ix.join(", "))
        }
        ExprKind::Call(c) => call_text(c),
        ExprKind::Unary { op, operand } => {
            let s = match op {
                UnaryOp::Neg => format!("-{}", expr_prec(operand, 9)),
                UnaryOp::Not => format!("NOT {}", expr_prec(operand, 9)),
            };
            if min > 9 {
                format!("({})", s)
            } else {
                s
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let p = op.precedence();
            let s = format!("{} {} {}", expr_prec(lhs, p), op.symbol(), expr_prec(rhs, p + 1));
            if p < min {
                format!("({})", s)
            } else {
                s
            }
        }
        ExprKind::Paren(inner) => expr_prec(inner, min),
        ExprKind::Error => "(* error *)".into(),
    }
}

fn arg_text(a: &Arg) -> String {
    match a {
        Arg::Positional(e) => expr_text(e),
        Arg::Input { name, value } => format!("{} := {}", name.text, expr_text(value)),
        Arg::Output { name, target, negated, .. } => {
            format!("{}{} =>{}", if *negated { "NOT " } else { "" }, name.text, target.as_ref().map(|t| format!(" {}", expr_text(t))).unwrap_or_default())
        }
    }
}

fn call_text(c: &CallExpr) -> String {
    let args: Vec<String> = c.args.iter().map(arg_text).collect();
    format!("{}({})", expr_prec(&c.callee, 10), args.join(", "))
}

fn spec_text(s: &TypeSpec) -> String {
    match s {
        TypeSpec::Named(i) => i.text.to_uppercase(),
        TypeSpec::String { wide, len, .. } => {
            let base = if *wide { "WSTRING" } else { "STRING" };
            match len {
                Some(l) => format!("{}[{}]", base, expr_text(l)),
                None => base.into(),
            }
        }
        TypeSpec::Subrange { base, lo, hi, .. } => format!("{} ({}..{})", base.text.to_uppercase(), expr_text(lo), expr_text(hi)),
        TypeSpec::Enum { values, .. } => {
            let v: Vec<String> = values.iter().map(|x| x.text.clone()).collect();
            format!("({})", v.join(", "))
        }
        TypeSpec::Array { dims, elem, .. } => {
            let d: Vec<String> = dims.iter().map(|(l, h)| format!("{}..{}", expr_text(l), expr_text(h))).collect();
            format!("ARRAY [{}] OF {}", d.join(", "), spec_text(elem))
        }
        TypeSpec::Struct { .. } => "STRUCT".into(),
    }
}

fn init_text(i: &Initializer) -> String {
    match i {
        Initializer::Expr(e) => expr_text(e),
        Initializer::Array(elems, _) => {
            let v: Vec<String> = elems
                .iter()
                .map(|e| match e {
                    ArrayInitElem::Single(i) => init_text(i),
                    ArrayInitElem::Repeat { count, value, .. } => {
                        format!("{}({})", expr_text(count), value.as_ref().map(init_text).unwrap_or_default())
                    }
                })
                .collect();
            format!("[{}]", v.join(", "))
        }
        Initializer::Struct(elems, _) => {
            let v: Vec<String> = elems.iter().map(|(n, i)| format!("{} := {}", n.text, init_text(i))).collect();
            format!("({})", v.join(", "))
        }
    }
}

impl Writer {
    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn element(&mut self, el: &LibraryElement) {
        match el {
            LibraryElement::DataTypes(d) => {
                self.line("TYPE");
                self.indent += 1;
                for t in &d.types {
                    self.type_decl(t);
                }
                self.indent -= 1;
                self.line("END_TYPE");
            }
            LibraryElement::Pou(p) => self.pou(p),
            LibraryElement::Configuration(c) => self.configuration(c),
        }
    }

    fn type_decl(&mut self, t: &TypeDecl) {
        let init = t.init.as_ref().map(|i| format!(" := {}", init_text(i))).unwrap_or_default();
        if let TypeSpec::Struct { fields, .. } = &t.spec {
            self.line(&format!("{} :", t.name.text));
            self.struct_body(fields);
            let last = self.out.pop();
            debug_assert_eq!(last, Some('\n'));
            self.out.push_str(&format!("{};\n", init));
        } else {
            self.line(&format!("{} : {}{};", t.name.text, spec_text(&t.spec), init));
        }
    }

    fn struct_body(&mut self, fields: &[StructField]) {
        self.line("STRUCT");
        self.indent += 1;
        for f in fields {
            if let TypeSpec::Struct { fields: inner, .. } = &f.spec {
                self.line(&format!("{} :", f.name.text));
                self.struct_body(inner);
                let last = self.out.pop();
                debug_assert_eq!(last, Some('\n'));
                self.out.push_str(";\n");
                continue;
            }
            let init = f.init.as_ref().map(|i| format!(" := {}", init_text(i))).unwrap_or_default();
            self.line(&format!("{} : {}{};", f.name.text, spec_text(&f.spec), init));
        }
        self.indent -= 1;
        self.line("END_STRUCT");
    }

    fn var_block(&mut self, b: &VarBlock) {
        let q = match b.qualifier {
            Some(VarQualifier::Constant) => " CONSTANT",
            Some(VarQualifier::Retain) => " RETAIN",
            Some(VarQualifier::NonRetain) => " NON_RETAIN",
            None => "",
        };
        self.line(&format!("{}{}", b.kind.keyword(), q));
        self.indent += 1;
        match &b.items {
            VarItems::Vars(decls) => {
                for d in decls {
                    let names: Vec<String> = d.names.iter().map(|n| n.text.clone()).collect();
                    let mut s = names.join(", ");
                    if let Some(l) = &d.location {
                        if !s.is_empty() {
                            s.push(' ');
                        }
                        s.push_str(&format!("AT {}", l.to_text()));
                    }
                    s.push_str(" : ");
                    s.push_str(&spec_text(&d.spec));
                    match d.edge {
                        Some(Edge::Rising) => s.push_str(" R_EDGE"),
                        Some(Edge::Falling) => s.push_str(" F_EDGE"),
                        None => {}
                    }
                    if let Some(i) = &d.init {
                        s.push_str(&format!(" := {}", init_text(i)));
                    }
                    s.push(';');
                    self.line(&s);
                }
            }
            VarItems::Access(decls) => {
                for a in decls {
                    let dir = match a.direction {
                        Some(Direction::ReadWrite) => " READ_WRITE",
                        Some(Direction::ReadOnly) => " READ_ONLY",
                        None => "",
                    };
                    self.line(&format!("{} : {} : {}{};", a.name.text, access_path_text(&a.path), spec_text(&a.spec), dir));
                }
            }
            VarItems::Config(inits) => {
                for c in inits {
                    let path: Vec<String> = c.path.iter().map(|p| p.text.clone()).collect();
                    let loc = c.location.as_ref().map(|l| format!(" AT {}", l.to_text())).unwrap_or_default();
                    let init = c.init.as_ref().map(|i| format!(" := {}", init_text(i))).unwrap_or_default();
                    self.line(&format!("{}{} : {}{};", path.join("."), loc, spec_text(&c.spec), init));
                }
            }
        }
        self.indent -= 1;
        self.line("END_VAR");
    }

    fn pou(&mut self, p: &Pou) {
        let (kw, end) = match &p.kind {
            PouKind::Function { result } => {
                self.line(&format!("FUNCTION {} : {}", p.name.text, spec_text(result)));
                ("", "END_FUNCTION")
            }
            PouKind::FunctionBlock => {
                self.line(&format!("FUNCTION_BLOCK {}", p.name.text));
                ("", "END_FUNCTION_BLOCK")
            }
            PouKind::Program => {
                self.line(&format!("PROGRAM {}", p.name.text));
                ("", "END_PROGRAM")
            }
        };
        let _ = kw;
        self.indent += 1;
        for b in &p.var_blocks {
            self.var_block(b);
        }
        self.indent -= 1;
        self.body(&p.body);
        self.line(end);
    }

    fn body(&mut self, b: &Body) {
        match b {
            Body::Empty => {}
            Body::St(stmts) => {
                self.indent += 1;
                self.stmts(stmts);
                self.indent -= 1;
            }
            Body::Il(list) => {
                self.indent += 1;
                self.il(list);
                self.indent -= 1;
            }
            Body::Sfc(sfc) => self.sfc(sfc),
            Body::Graphic(g) => {
                for l in g.text.trim_matches('\n').lines() {
                    self.out.push_str(l.trim_end());
                    self.out.push('\n');
                }
            }
        }
    }

    fn stmts(&mut self, list: &[Stmt]) {
        for s in list {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Empty => self.line(";"),
            StmtKind::Assign { target, value } => self.line(&format!("{} := {};", expr_text(target), expr_text(value))),
            StmtKind::Call(c) => self.line(&format!("{};", call_text(c))),
            StmtKind::Return => self.line("RETURN;"),
            StmtKind::Exit => self.line("EXIT;"),
            StmtKind::If { branches, else_body } => {
                for (i, (c, b)) in branches.iter().enumerate() {
                    self.line(&format!("{} {} THEN", if i == 0 { "IF" } else { "ELSIF" }, expr_text(c)));
                    self.indent += 1;
                    self.stmts(b);
                    self.indent -= 1;
                }
                if let Some(b) = else_body {
                    self.line("ELSE");
                    self.indent += 1;
                    self.stmts(b);
                    self.indent -= 1;
                }
                self.line("END_IF;");
            }
            StmtKind::Case { selector, arms, else_body } => {
                self.line(&format!("CASE {} OF", expr_text(selector)));
                self.indent += 1;
                for a in arms {
                    let labels: Vec<String> = a
                        .labels
                        .iter()
                        .map(|l| match l {
                            CaseLabel::Value(e) => expr_text(e),
                            CaseLabel::Range(lo, hi) => format!("{}..{}", expr_text(lo), expr_text(hi)),
                        })
                        .collect();
                    self.line(&format!("{}:", labels.join(", ")));
                    self.indent += 1;
                    self.stmts(&a.body);
                    self.indent -= 1;
                }
                self.indent -= 1;
                if let Some(b) = else_body {
                    self.line("ELSE");
                    self.indent += 1;
                    self.stmts(b);
                    self.indent -= 1;
                }
                self.line("END_CASE;");
            }
            StmtKind::For { var, from, to, by, body } => {
                let by = by.as_ref().map(|b| format!(" BY {}", expr_text(b))).unwrap_or_default();
                self.line(&format!("FOR {} := {} TO {}{} DO", var.text, expr_text(from), expr_text(to), by));
                self.indent += 1;
                self.stmts(body);
                self.indent -= 1;
                self.line("END_FOR;");
            }
            StmtKind::While { cond, body } => {
                self.line(&format!("WHILE {} DO", expr_text(cond)));
                self.indent += 1;
                self.stmts(body);
                self.indent -= 1;
                self.line("END_WHILE;");
            }
            StmtKind::Repeat { body, until } => {
                self.line("REPEAT");
                self.indent += 1;
                self.stmts(body);
                self.indent -= 1;
                self.line(&format!("UNTIL {}", expr_text(until)));
                self.line("END_REPEAT;");
            }
        }
    }

    fn il(&mut self, list: &[IlInstr]) {
        for i in list {
            let label = i.label.as_ref().map(|l| format!("{}: ", l.text)).unwrap_or_default();
            match &i.op {
                IlOp::None => self.line(&label),
                IlOp::Simple { op, operand } => {
                    let o = operand.as_ref().map(|e| format!(" {}", expr_text(e))).unwrap_or_default();
                    self.line(&format!("{}{}{}", label, op.as_str(), o));
                }
                IlOp::Deferred { op, operand, body } => {
                    let o = operand.as_ref().map(|e| format!(" {}", expr_text(e))).unwrap_or_default();
                    self.line(&format!("{}{}({}", label, op.as_str(), o));
                    self.indent += 1;
                    self.il(body);
                    self.indent -= 1;
                    self.line(")");
                }
                IlOp::Function { name, operands } => {
                    let o: Vec<String> = operands.iter().map(expr_text).collect();
                    let o = if o.is_empty() { String::new() } else { format!(" {}", o.join(", ")) };
                    self.line(&format!("{}{}{}", label, name.text, o));
                }
                IlOp::FormalFunction { name, params } => {
                    self.line(&format!("{}{}(", label, name.text));
                    self.il_params(params);
                }
                IlOp::Jump { cond, label: target } => self.line(&format!("{}{} {}", label, cond_op("JMP", *cond), target.text)),
                IlOp::Return { cond } => self.line(&format!("{}{}", label, cond_op("RET", *cond))),
                IlOp::Call { cond, instance, args } => {
                    let head = format!("{}{} {}", label, cond_op("CAL", *cond), expr_text(instance));
                    match args {
                        IlCallArgs::None => self.line(&head),
                        IlCallArgs::NonFormal(ops) => {
                            let o: Vec<String> = ops.iter().map(expr_text).collect();
                            self.line(&format!("{}({})", head, o.join(", ")));
                        }
                        IlCallArgs::Formal(params) => {
                            self.line(&format!("{}(", head));
                            self.il_params(params);
                        }
                    }
                }
            }
        }
    }

    fn il_params(&mut self, params: &[IlParam]) {
        self.indent += 1;
        for (k, p) in params.iter().enumerate() {
            let sep = if k + 1 < params.len() { "," } else { "" };
            match p {
                IlParam::Input { name, value: IlParamValue::Operand(e), .. } => self.line(&format!("{} := {}{}", name.text, expr_text(e), sep)),
                IlParam::Input { name, value: IlParamValue::Nested(list), .. } => {
                    self.line(&format!("{} := (", name.text));
                    self.indent += 1;
                    self.il(list);
                    self.indent -= 1;
                    self.line(&format!("){}", sep));
                }
                IlParam::Output { name, target, negated, .. } => {
                    self.line(&format!("{}{} => {}{}", if *negated { "NOT " } else { "" }, name.text, expr_text(target), sep))
                }
            }
        }
        self.indent -= 1;
        self.line(")");
    }

    fn sfc(&mut self, sfc: &Sfc) {
        for s in &sfc.steps {
            let assocs: Vec<String> = s
                .associations
                .iter()
                .map(|a| {
                    let mut parts = Vec::new();
                    if let Some(q) = a.qualifier {
                        parts.push(q.as_str().to_string());
                    }
                    if let Some(t) = &a.time {
                        parts.push(expr_text(t));
                    }
                    for i in &a.indicators {
                        parts.push(i.text.clone());
                    }
                    format!(" {}({});", a.action.text, parts.join(", "))
                })
                .collect();
            self.line(&format!("{} {}:{} END_STEP", if s.initial { "INITIAL_STEP" } else { "STEP" }, s.name.text, assocs.join("")));
        }
        for t in &sfc.transitions {
            let list = |v: &[crate::ident::Ident]| {
                if v.len() == 1 {
                    v[0].text.clone()
                } else {
                    format!("({})", v.iter().map(|x| x.text.clone()).collect::<Vec<_>>().join(", "))
                }
            };
            let name = t.name.as_ref().map(|n| format!(" {}", n.text)).unwrap_or_default();
            let prio = t.priority.as_ref().map(|p| format!(" (PRIORITY := {})", expr_text(p))).unwrap_or_default();
            let head = format!("TRANSITION{}{} FROM {} TO {}", name, prio, list(&t.from), list(&t.to));
            match &t.condition {
                TransitionCondition::St(e) => self.line(&format!("{} := {}; END_TRANSITION", head, expr_text(e))),
                TransitionCondition::Il(l) => {
                    self.line(&format!("{} :", head));
                    self.indent += 1;
                    self.il(l);
                    self.indent -= 1;
                    self.line("END_TRANSITION");
                }
                TransitionCondition::Graphic(g) => {
                    self.line(&format!("{} :", head));
                    self.body(&Body::Graphic(g.clone()));
                    self.line("END_TRANSITION");
                }
                TransitionCondition::Named => {}
            }
        }
        for a in &sfc.actions {
            self.line(&format!("ACTION {}:", a.name.text));
            self.body(&a.body);
            self.line("END_ACTION");
        }
    }

    fn configuration(&mut self, c: &Configuration) {
        self.line(&format!("CONFIGURATION {}", c.name.text));
        self.indent += 1;
        let (globals, rest): (Vec<&VarBlock>, Vec<&VarBlock>) = c.var_blocks.iter().partition(|b| b.kind == VarKind::Global);
        for b in globals {
            self.var_block(b);
        }
        for r in &c.resources {
            if let Some(n) = &r.name {
                self.line(&format!("RESOURCE {} ON {}", n.text, r.on.as_ref().map(|o| o.text.clone()).unwrap_or_default()));
                self.indent += 1;
            }
            for b in &r.var_blocks {
                self.var_block(b);
            }
            for t in &r.tasks {
                let mut parts = Vec::new();
                if let Some(s) = &t.single {
                    parts.push(format!("SINGLE := {}", expr_text(s)));
                }
                if let Some(i) = &t.interval {
                    parts.push(format!("INTERVAL := {}", expr_text(i)));
                }
                if let Some(p) = &t.priority {
                    parts.push(format!("PRIORITY := {}", expr_text(p)));
                }
                self.line(&format!("TASK {}({});", t.name.text, parts.join(", ")));
            }
            for p in &r.programs {
                let retain = match p.retain {
                    Some(VarQualifier::Retain) => " RETAIN",
                    Some(VarQualifier::NonRetain) => " NON_RETAIN",
                    _ => "",
                };
                let task = p.task.as_ref().map(|t| format!(" WITH {}", t.text)).unwrap_or_default();
                let els: Vec<String> = p
                    .elements
                    .iter()
                    .map(|e| match e {
                        ProgConfElement::FbTask { fb, task, .. } => format!("{} WITH {}", expr_text(fb), task.text),
                        ProgConfElement::Input { var, source, .. } => format!("{} := {}", var.text, expr_text(source)),
                        ProgConfElement::Output { var, sink, .. } => format!("{} => {}", var.text, expr_text(sink)),
                    })
                    .collect();
                let args = if els.is_empty() { String::new() } else { format!("({})", els.join(", ")) };
                self.line(&format!("PROGRAM{} {}{} : {}{};", retain, p.name.text, task, p.type_name.text, args));
            }
            if r.name.is_some() {
                self.indent -= 1;
                self.line("END_RESOURCE");
            }
        }
        for b in rest {
            self.var_block(b);
        }
        self.indent -= 1;
        self.line("END_CONFIGURATION");
    }
}

fn cond_op(base: &str, c: IlCond) -> String {
    match c {
        IlCond::Always => base.into(),
        IlCond::IfTrue => format!("{}C", base),
        IlCond::IfFalse => format!("{}CN", base),
    }
}

fn access_path_text(p: &AccessPath) -> String {
    let mut s: Vec<String> = p.prefix.iter().map(|i| i.text.clone()).collect();
    match &p.target {
        AccessTarget::Direct(d) => s.push(d.to_text()),
        AccessTarget::Symbolic(e) => s.push(expr_text(e)),
    }
    s.join(".")
}
