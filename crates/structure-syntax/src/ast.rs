//! Abstract syntax tree for library elements (B.0) of all languages.

use crate::ident::Ident;
use crate::literal::{Date, Duration};
use crate::source::Span;

/// The contents of one source file.
#[derive(Clone, Debug, Default)]
pub struct SourceUnit {
    pub elements: Vec<LibraryElement>,
}

/// `library_element_declaration` (B.0).
#[derive(Clone, Debug)]
pub enum LibraryElement {
    DataTypes(DataTypeDecl),
    Pou(Pou),
    Configuration(Configuration),
}

// ---------------------------------------------------------------- data types

/// `TYPE ... END_TYPE`.
#[derive(Clone, Debug)]
pub struct DataTypeDecl {
    pub types: Vec<TypeDecl>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TypeDecl {
    pub name: Ident,
    pub spec: TypeSpec,
    pub init: Option<Initializer>,
    pub span: Span,
}

/// A data type specification as it may appear in a type or variable
/// declaration.
#[derive(Clone, Debug)]
pub enum TypeSpec {
    /// An elementary, derived, generic or function block type name.
    Named(Ident),
    /// `STRING`, `STRING[n]`, `WSTRING[n]`.
    String { wide: bool, len: Option<Box<Expr>>, span: Span },
    /// `INT (-4095..4095)`.
    Subrange { base: Ident, lo: Box<Expr>, hi: Box<Expr>, span: Span },
    /// `(A, B, C)`.
    Enum { values: Vec<Ident>, span: Span },
    /// `ARRAY [l..h, ...] OF elem`.
    Array { dims: Vec<(Expr, Expr)>, elem: Box<TypeSpec>, span: Span },
    /// `STRUCT ... END_STRUCT`.
    Struct { fields: Vec<StructField>, span: Span },
}

impl TypeSpec {
    pub fn span(&self) -> Span {
        match self {
            TypeSpec::Named(i) => i.span,
            TypeSpec::String { span, .. }
            | TypeSpec::Subrange { span, .. }
            | TypeSpec::Enum { span, .. }
            | TypeSpec::Array { span, .. }
            | TypeSpec::Struct { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StructField {
    pub name: Ident,
    pub spec: TypeSpec,
    pub init: Option<Initializer>,
    pub span: Span,
}

/// Initial value of a variable, type or structure element (2.4.2, 2.3.3.2).
#[derive(Clone, Debug)]
pub enum Initializer {
    Expr(Expr),
    /// `[ elem, n(elem), ... ]`
    Array(Vec<ArrayInitElem>, Span),
    /// `( name := init, ... )`
    Struct(Vec<(Ident, Initializer)>, Span),
}

impl Initializer {
    pub fn span(&self) -> Span {
        match self {
            Initializer::Expr(e) => e.span,
            Initializer::Array(_, s) | Initializer::Struct(_, s) => *s,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ArrayInitElem {
    Single(Initializer),
    /// `count(value)`; the value may be omitted (`count()`).
    Repeat { count: Expr, value: Option<Initializer>, span: Span },
}

// ---------------------------------------------------------------- variables

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarKind {
    Var,
    Input,
    Output,
    InOut,
    External,
    Global,
    Temp,
    Access,
    Config,
}

impl VarKind {
    pub fn keyword(self) -> &'static str {
        match self {
            VarKind::Var => "VAR",
            VarKind::Input => "VAR_INPUT",
            VarKind::Output => "VAR_OUTPUT",
            VarKind::InOut => "VAR_IN_OUT",
            VarKind::External => "VAR_EXTERNAL",
            VarKind::Global => "VAR_GLOBAL",
            VarKind::Temp => "VAR_TEMP",
            VarKind::Access => "VAR_ACCESS",
            VarKind::Config => "VAR_CONFIG",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarQualifier {
    Constant,
    Retain,
    NonRetain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Rising,
    Falling,
}

#[derive(Clone, Debug)]
pub struct VarBlock {
    pub kind: VarKind,
    pub qualifier: Option<VarQualifier>,
    pub items: VarItems,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum VarItems {
    Vars(Vec<VarDecl>),
    Access(Vec<AccessDecl>),
    Config(Vec<ConfigInit>),
}

/// One declaration line: `A, B AT %IX1 : BOOL R_EDGE := TRUE`.
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub names: Vec<Ident>,
    pub location: Option<DirectVar>,
    pub spec: TypeSpec,
    pub edge: Option<Edge>,
    pub init: Option<Initializer>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    ReadWrite,
    ReadOnly,
}

/// `name : path : type [direction]` in VAR_ACCESS (2.7.1).
#[derive(Clone, Debug)]
pub struct AccessDecl {
    pub name: Ident,
    pub path: AccessPath,
    pub spec: TypeSpec,
    pub direction: Option<Direction>,
    pub span: Span,
}

/// Hierarchical access path, e.g. `STATION_1.P1.x2` or `STATION_1.%IX1.1`.
#[derive(Clone, Debug)]
pub struct AccessPath {
    /// Leading resource/program/function block instance names.
    pub prefix: Vec<Ident>,
    pub target: AccessTarget,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum AccessTarget {
    Direct(DirectVar),
    /// A symbolic variable (possibly with structure/array selectors).
    Symbolic(Expr),
}

/// Instance-specific initialisation in VAR_CONFIG (2.7.1).
#[derive(Clone, Debug)]
pub struct ConfigInit {
    /// `resource.program.{fb.}variable`.
    pub path: Vec<Ident>,
    pub location: Option<DirectVar>,
    pub spec: TypeSpec,
    pub init: Option<Initializer>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocationPrefix {
    Input,
    Output,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizePrefix {
    /// No size prefix: single bit.
    None,
    X,
    B,
    W,
    D,
    L,
}

/// A directly represented variable (2.4.1.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DirectVar {
    pub location: LocationPrefix,
    pub size: SizePrefix,
    /// Empty for the asterisk notation (`%Q*`).
    pub address: Vec<u32>,
    pub incomplete: bool,
    pub span: Span,
}

impl DirectVar {
    pub fn to_text(&self) -> String {
        let mut s = String::from("%");
        s.push(match self.location {
            LocationPrefix::Input => 'I',
            LocationPrefix::Output => 'Q',
            LocationPrefix::Memory => 'M',
        });
        match self.size {
            SizePrefix::None => {}
            SizePrefix::X => s.push('X'),
            SizePrefix::B => s.push('B'),
            SizePrefix::W => s.push('W'),
            SizePrefix::D => s.push('D'),
            SizePrefix::L => s.push('L'),
        }
        if self.incomplete {
            s.push('*');
        } else {
            let parts: Vec<String> = self.address.iter().map(|a| a.to_string()).collect();
            s.push_str(&parts.join("."));
        }
        s
    }
}

// ---------------------------------------------------------------- POUs

#[derive(Clone, Debug)]
pub enum PouKind {
    Function { result: TypeSpec },
    FunctionBlock,
    Program,
}

/// Function, function block or program declaration (2.5).
#[derive(Clone, Debug)]
pub struct Pou {
    pub kind: PouKind,
    pub name: Ident,
    pub var_blocks: Vec<VarBlock>,
    pub body: Body,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Body {
    /// No body statements at all.
    Empty,
    St(Vec<Stmt>),
    Il(Vec<IlInstr>),
    Sfc(Sfc),
    Graphic(GraphicBody),
}

// ---------------------------------------------------------------- ST

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Empty,
    Assign { target: Expr, value: Expr },
    /// Function block (or, as an extension, function) invocation statement.
    Call(CallExpr),
    Return,
    Exit,
    If { branches: Vec<(Expr, Vec<Stmt>)>, else_body: Option<Vec<Stmt>> },
    Case { selector: Expr, arms: Vec<CaseArm>, else_body: Option<Vec<Stmt>> },
    For { var: Ident, from: Expr, to: Expr, by: Option<Expr>, body: Vec<Stmt> },
    While { cond: Expr, body: Vec<Stmt> },
    Repeat { body: Vec<Stmt>, until: Expr },
}

#[derive(Clone, Debug)]
pub struct CaseArm {
    pub labels: Vec<CaseLabel>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum CaseLabel {
    Value(Expr),
    Range(Expr, Expr),
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Or,
    Xor,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Or => "OR",
            BinaryOp::Xor => "XOR",
            BinaryOp::And => "AND",
            BinaryOp::Eq => "=",
            BinaryOp::Ne => "<>",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "MOD",
            BinaryOp::Pow => "**",
        }
    }

    /// Binding strength (higher binds tighter), per table 55 / B.3.1.
    pub fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::Xor => 2,
            BinaryOp::And => 3,
            BinaryOp::Eq | BinaryOp::Ne => 4,
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => 5,
            BinaryOp::Add | BinaryOp::Sub => 6,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 7,
            BinaryOp::Pow => 8,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Literal(Literal),
    /// A plain identifier: variable, enumerated value, step name, ...
    Name(Ident),
    /// `TYPE#VALUE` enumerated value.
    TypedEnum { ty: Ident, value: Ident },
    DirectVar(DirectVar),
    Field { base: Box<Expr>, field: Ident },
    Index { base: Box<Expr>, indices: Vec<Expr> },
    Call(CallExpr),
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Paren(Box<Expr>),
    /// Placeholder produced after a syntax error.
    Error,
}

#[derive(Clone, Debug)]
pub struct CallExpr {
    /// Function name or function block instance (possibly a field path).
    pub callee: Box<Expr>,
    pub args: Vec<Arg>,
    pub span: Span,
}

impl CallExpr {
    pub fn callee_name(&self) -> Option<&Ident> {
        match &self.callee.kind {
            ExprKind::Name(i) => Some(i),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Arg {
    /// Non-formal argument.
    Positional(Expr),
    /// `name := value`
    Input { name: Ident, value: Expr },
    /// `[NOT] name => target`; the target may be absent (empty assignment).
    Output { name: Ident, target: Option<Expr>, negated: bool, span: Span },
}

#[derive(Clone, Debug)]
pub struct Literal {
    pub kind: LiteralKind,
    /// Explicit type prefix (`INT#5`, `STRING#'OK'`).
    pub type_prefix: Option<Ident>,
}

#[derive(Clone, Debug)]
pub enum LiteralKind {
    /// Integer magnitude; `negative` for signed typed literals like `INT#-5`.
    Int { value: u128, negative: bool, based: bool },
    /// Normalised decimal text of a real literal.
    Real { text: String, negative: bool },
    Bool(bool),
    String { wide: bool, chars: Vec<u32> },
    Duration(Duration),
    TimeOfDay(u64),
    Date(Date),
    DateAndTime(Date, u64),
}

// ---------------------------------------------------------------- IL

#[derive(Clone, Debug)]
pub struct IlInstr {
    pub label: Option<Ident>,
    pub op: IlOp,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IlOperator {
    Ld,
    Ldn,
    St,
    Stn,
    Not,
    S,
    R,
    S1,
    R1,
    Clk,
    Cu,
    Cd,
    Pv,
    In,
    Pt,
    And,
    Andn,
    Or,
    Orn,
    Xor,
    Xorn,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Gt,
    Ge,
    Eq,
    Lt,
    Le,
    Ne,
}

impl IlOperator {
    pub fn from_upper(s: &str) -> Option<IlOperator> {
        use IlOperator::*;
        Some(match s {
            "LD" => Ld,
            "LDN" => Ldn,
            "ST" => St,
            "STN" => Stn,
            "NOT" => Not,
            "S" => S,
            "R" => R,
            "S1" => S1,
            "R1" => R1,
            "CLK" => Clk,
            "CU" => Cu,
            "CD" => Cd,
            "PV" => Pv,
            "IN" => In,
            "PT" => Pt,
            "AND" | "&" => And,
            "ANDN" | "&N" => Andn,
            "OR" => Or,
            "ORN" => Orn,
            "XOR" => Xor,
            "XORN" => Xorn,
            "ADD" => Add,
            "SUB" => Sub,
            "MUL" => Mul,
            "DIV" => Div,
            "MOD" => Mod,
            "GT" => Gt,
            "GE" => Ge,
            "EQ" => Eq,
            "LT" => Lt,
            "LE" => Le,
            "NE" => Ne,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        use IlOperator::*;
        match self {
            Ld => "LD",
            Ldn => "LDN",
            St => "ST",
            Stn => "STN",
            Not => "NOT",
            S => "S",
            R => "R",
            S1 => "S1",
            R1 => "R1",
            Clk => "CLK",
            Cu => "CU",
            Cd => "CD",
            Pv => "PV",
            In => "IN",
            Pt => "PT",
            And => "AND",
            Andn => "ANDN",
            Or => "OR",
            Orn => "ORN",
            Xor => "XOR",
            Xorn => "XORN",
            Add => "ADD",
            Sub => "SUB",
            Mul => "MUL",
            Div => "DIV",
            Mod => "MOD",
            Gt => "GT",
            Ge => "GE",
            Eq => "EQ",
            Lt => "LT",
            Le => "LE",
            Ne => "NE",
        }
    }

    /// Operators that can take a deferred (parenthesised) operand.
    pub fn is_expr_operator(self) -> bool {
        use IlOperator::*;
        matches!(self, And | Andn | Or | Orn | Xor | Xorn | Add | Sub | Mul | Div | Mod | Gt | Ge | Eq | Lt | Le | Ne)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IlCond {
    Always,
    IfTrue,
    IfFalse,
}

#[derive(Clone, Debug)]
pub enum IlOp {
    /// Label only (or empty line with a label).
    None,
    Simple { op: IlOperator, operand: Option<Expr> },
    /// `OP( [operand] EOL instructions )`.
    Deferred { op: IlOperator, operand: Option<Expr>, body: Vec<IlInstr> },
    /// Non-formal function invocation: current result is the first argument.
    Function { name: Ident, operands: Vec<Expr> },
    /// Formal function invocation `name( EOL params )`.
    FormalFunction { name: Ident, params: Vec<IlParam> },
    Jump { cond: IlCond, label: Ident },
    Call { cond: IlCond, instance: Expr, args: IlCallArgs },
    Return { cond: IlCond },
}

#[derive(Clone, Debug)]
pub enum IlCallArgs {
    None,
    Formal(Vec<IlParam>),
    NonFormal(Vec<Expr>),
}

#[derive(Clone, Debug)]
pub enum IlParam {
    Input { name: Ident, value: IlParamValue, span: Span },
    Output { name: Ident, target: Expr, negated: bool, span: Span },
}

#[derive(Clone, Debug)]
pub enum IlParamValue {
    Operand(Expr),
    Nested(Vec<IlInstr>),
}

// ---------------------------------------------------------------- SFC

#[derive(Clone, Debug, Default)]
pub struct Sfc {
    pub steps: Vec<Step>,
    pub transitions: Vec<Transition>,
    pub actions: Vec<Action>,
    /// Actions declared inline in graphical action blocks ("d" field).
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub name: Ident,
    pub initial: bool,
    pub associations: Vec<ActionAssociation>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Qualifier {
    N,
    R,
    S,
    L,
    D,
    P,
    SD,
    DS,
    SL,
    P1,
    P0,
}

impl Qualifier {
    pub fn from_upper(s: &str) -> Option<Qualifier> {
        Some(match s {
            "N" => Qualifier::N,
            "R" => Qualifier::R,
            "S" => Qualifier::S,
            "L" => Qualifier::L,
            "D" => Qualifier::D,
            "P" => Qualifier::P,
            "SD" => Qualifier::SD,
            "DS" => Qualifier::DS,
            "SL" => Qualifier::SL,
            "P1" => Qualifier::P1,
            "P0" => Qualifier::P0,
            _ => return None,
        })
    }

    pub fn is_timed(self) -> bool {
        matches!(self, Qualifier::L | Qualifier::D | Qualifier::SD | Qualifier::DS | Qualifier::SL)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Qualifier::N => "N",
            Qualifier::R => "R",
            Qualifier::S => "S",
            Qualifier::L => "L",
            Qualifier::D => "D",
            Qualifier::P => "P",
            Qualifier::SD => "SD",
            Qualifier::DS => "DS",
            Qualifier::SL => "SL",
            Qualifier::P1 => "P1",
            Qualifier::P0 => "P0",
        }
    }
}

/// `action_name ( [qualifier [, time]] {, indicator} )`.
#[derive(Clone, Debug)]
pub struct ActionAssociation {
    pub action: Ident,
    pub qualifier: Option<Qualifier>,
    pub time: Option<Expr>,
    pub indicators: Vec<Ident>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Transition {
    pub name: Option<Ident>,
    pub priority: Option<Expr>,
    pub from: Vec<Ident>,
    pub to: Vec<Ident>,
    pub condition: TransitionCondition,
    /// Explicit evaluation priority among the transitions of a selection
    /// divergence (graphical feature 2b of table 46). Lower is higher priority.
    pub branch_priority: Option<u32>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TransitionCondition {
    St(Expr),
    Il(Vec<IlInstr>),
    Graphic(GraphicBody),
    /// Condition is given by a named transition (graphical feature 7);
    /// resolved by the semantic analysis.
    Named,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub name: Ident,
    pub body: Body,
    pub span: Span,
}

// ---------------------------------------------------------------- graphics

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraphicLanguage {
    Ld,
    Fbd,
    Sfc,
}

/// A body written in a graphical language using the semigraphic
/// representation of clause 4 (characters of table 1).
#[derive(Clone, Debug)]
pub struct GraphicBody {
    pub language: GraphicLanguage,
    pub networks: Vec<crate::graphic::Network>,
    /// For graphical SFC bodies, the textual SFC equivalent.
    pub sfc: Option<Box<Sfc>>,
    /// Original text, retained for writing the element back out.
    pub text: String,
    pub span: Span,
}

// ---------------------------------------------------------------- configuration

#[derive(Clone, Debug)]
pub struct Configuration {
    pub name: Ident,
    pub var_blocks: Vec<VarBlock>,
    pub resources: Vec<Resource>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Resource {
    /// `None` for the single resource declaration form.
    pub name: Option<Ident>,
    pub on: Option<Ident>,
    pub var_blocks: Vec<VarBlock>,
    pub tasks: Vec<TaskDecl>,
    pub programs: Vec<ProgramConfig>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TaskDecl {
    pub name: Ident,
    pub single: Option<Expr>,
    pub interval: Option<Expr>,
    pub priority: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ProgramConfig {
    pub name: Ident,
    pub retain: Option<VarQualifier>,
    pub task: Option<Ident>,
    pub type_name: Ident,
    pub elements: Vec<ProgConfElement>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ProgConfElement {
    FbTask { fb: Expr, task: Ident, span: Span },
    Input { var: Ident, source: Expr, span: Span },
    Output { var: Ident, sink: Expr, span: Span },
}
