//! Compiler for the IEC 61131-3 (second edition, 2003) programming
//! languages. Source files are parsed, checked and lowered to bytecode for
//! the `structure-runtime` virtual machine; a configuration is linked into
//! an executable [`Image`].

pub mod codegen;
pub mod consteval;
pub mod ctx;
pub mod ir;
pub mod layout;
pub mod link;
pub mod lower;
pub mod prelude;
pub mod stdlib;
pub mod types;
pub mod visit;

pub use ctx::CompileOptions;
use structure_runtime::Image;
use structure_syntax::diag::{Diagnostics, Policy};
use structure_syntax::parser;
use structure_syntax::{FileId, SourceFile, SourceMap};

/// Result of a compilation.
pub struct Output {
    pub image: Option<Image>,
    pub diagnostics: Diagnostics,
}

/// What to produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Check all library elements; do not link.
    Check,
    /// Check and link a configuration into an image.
    Build,
}

/// Compiles the given files of `sources`.
pub fn compile(sources: &SourceMap, files: &[FileId], options: CompileOptions, mode: Mode, policy: Policy) -> Output {
    let mut diags = Diagnostics::new();
    let mut units = Vec::new();
    for f in files {
        let r = parser::parse(sources.get(*f));
        diags.extend(r.diagnostics);
        units.push(r.unit);
    }
    let prelude_src = prelude::source();
    let prelude_file = SourceFile::new(FileId(u32::MAX - 1), "<standard library>", prelude_src);
    let mut pr = parser::parse(&prelude_file);
    // Names starting with "__" are reserved for the implementation.
    pr.diagnostics.retain(|d| !d.message.starts_with("invalid identifier '__"));
    if !pr.diagnostics.is_empty() {
        let mut d = Diagnostics::new();
        d.extend(pr.diagnostics.clone());
        panic!("standard library does not parse: {:?}", d.items.iter().map(|x| x.message.clone()).collect::<Vec<_>>());
    }
    let mut cx = ctx::Ctx::new(sources, options);
    cx.in_std = true;
    cx.register(&pr.unit, true);
    cx.in_std = false;
    for u in &units {
        cx.register(u, false);
    }
    cx.resolve_all_types();
    cx.layout_all();
    if !cx.recursive_instances {
        cx.check_declarations();
        cx.lower_all();
        check_recursion(&mut cx);
    }
    let mut image = None;
    if mode == Mode::Build && !diags.has_errors() && !cx.diags.has_errors() {
        image = link::link(&mut cx);
    }
    // Diagnostics inside the standard library indicate compiler defects.
    let prelude_id = FileId(u32::MAX - 1);
    let std_errors: Vec<_> = cx.diags.items.iter().filter(|d| d.span.map(|s| s.file == prelude_id).unwrap_or(false)).cloned().collect();
    if !std_errors.is_empty() {
        let msgs: Vec<String> = std_errors.iter().map(|d| format!("{} @{:?}", d.message, d.span)).collect();
        panic!("internal error in the standard library: {:#?}", msgs);
    }
    diags.extend(cx.diags.items);
    diags.apply_policy(policy);
    diags.sort();
    if diags.has_errors() {
        image = None;
    }
    Output { image, diagnostics: diags }
}

/// Reports recursive invocations (2.5): cycles in the call graph of the
/// lowered routines.
fn check_recursion(cx: &mut ctx::Ctx) {
    let n = cx.routines.len();
    let frames: Vec<u64> = cx.routines.iter().map(|r| r.frame_bytes).collect();
    let frame_of = |r: usize| frames.get(r).copied().unwrap_or(0);
    let calls: Vec<Vec<usize>> = cx
        .routines
        .iter()
        .map(|r| codegen::generate(&r.body, &frame_of).calls.iter().map(|c| c.routine).filter(|&c| c < n).collect())
        .collect();
    let pou_of = |cx: &ctx::Ctx, r: usize| cx.pous.iter().position(|p| p.routine == Some(r));
    let mut state = vec![0u8; n];
    let mut reported = std::collections::HashSet::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0usize)];
        while let Some(top) = stack.last_mut() {
            let r = top.0;
            if top.1 == calls[r].len() {
                state[r] = 2;
                stack.pop();
                continue;
            }
            let c = calls[r][top.1];
            top.1 += 1;
            match state[c] {
                0 => {
                    state[c] = 1;
                    stack.push((c, 0));
                }
                1 if reported.insert(c) => {
                    // c is on the stack: report the cycle at its declaration
                    let from = stack.iter().position(|(x, _)| *x == c).unwrap_or(0);
                    let names: Vec<String> = stack[from..].iter().chain(std::iter::once(&(c, 0))).map(|(x, _)| cx.routines[*x].name.clone()).collect();
                    if let Some(p) = pou_of(cx, c) {
                        let (name, span) = (cx.pous[p].name.clone(), cx.pous[p].ast.name.span);
                        cx.error_clause("2.5", format!("{} is invoked recursively ({}); program organization units shall not be recursive", name, names.join(" -> ")), span);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Convenience: compiles source text held in memory.
pub fn compile_str(name: &str, text: &str, mode: Mode) -> (SourceMap, Output) {
    let mut sm = SourceMap::new();
    let id = sm.add(name, text);
    let out = compile(&sm, &[id], CompileOptions::default(), mode, Policy::default());
    (sm, out)
}

/// Source text of a configuration running `program` cyclically on a single
/// resource, used when no configuration is given.
pub fn default_configuration(program: &str, interval: &str) -> String {
    format!(
        "CONFIGURATION DEFAULT\n  RESOURCE MAIN ON VM\n    TASK CYCLIC(INTERVAL := {interval}, PRIORITY := 1);\n    PROGRAM {program}_INSTANCE WITH CYCLIC : {program};\n  END_RESOURCE\nEND_CONFIGURATION\n"
    )
}
