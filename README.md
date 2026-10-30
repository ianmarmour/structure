# structure

A fully open source IEC 61131-3 compliant PLC compiler and runtime.

`structure` implements the programming languages of IEC 61131-3 (second
edition, 2003): the common elements (data types, variables, functions,
function blocks, programs, configurations, resources, tasks), Structured Text
(ST), Instruction List (IL), Sequential Function Charts (SFC), and the
graphical languages Ladder Diagram (LD) and Function Block Diagram (FBD) in
their semigraphic (ASCII) representation. Programs are compiled to bytecode
for a virtual machine that executes configurations with resources, periodic
and event-triggered tasks (preemptive or non-preemptive scheduling), process
images (`%I`, `%Q`, `%M`), retentive variables and access paths.

## Usage

```
cargo build --release
target/release/structure run examples/blink.st --program BLINK --for T#2s \
    --every T#250ms --watch LAMP
```

With `--program`, a default configuration runs the program in a cyclic task
(`--interval`, 10 ms by default) as the instance `MAIN.<PROGRAM>_INSTANCE`;
`--watch` and `--set` accept the program's variable names directly. Without
it, the configurations of the source files are used and variables are named
by their full path (`RESOURCE.INSTANCE.VARIABLE`), an access path, or a
directly represented variable (`%IX1`, `RESOURCE.%QW4`).

`examples/agv.st` is the AGV program of annex F.8, drawn entirely in the
semigraphic form: the declaration of the program, its body as two
sequential function charts and the configuration that runs it:

```
target/release/structure run examples/agv.st --for T#200ms --set %IX3=TRUE \
    --set %IX1=TRUE --every T#50ms --watch %QX1 --watch %QX2
```

| Command | Purpose |
|---|---|
| `structure check FILES` | Check library elements for errors (1.5.1 c) |
| `structure build FILES -o app.json` | Compile and link a configuration into an image |
| `structure run FILES\|app.json` | Run a configuration (simulated clock by default, `--realtime` for wall clock) |
| `structure fmt FILES` | Write library elements in canonical textual form (1.5.1 j) |
| `structure compliance` | Compliance statement (1.5.1) |
| `structure params` | Implementation-dependent parameters (annex D) |
| `structure errors` | Treatment of the error conditions of annex E |
| `structure extensions` | Extensions to the standard (1.5.1 e) |
| `structure syntax` | Formal syntax of the textual elements (1.5.1 i) |

Use `--strict` to report extensions as errors and
`--report-implementation-dependent` to report uses of implementation-dependent
features. Without a CONFIGURATION, `--program NAME [--interval T#10ms]`
creates one with a single cyclic task.

## Layout

| Crate | Contents |
|---|---|
| `crates/structure-syntax` | Lexer, parser and syntax tree for all languages, semigraphic parser, source writer |
| `crates/structure-compiler` | Type system, semantic checks, lowering of every language to a typed IR, code generation, standard library, linker |
| `crates/structure-runtime` | Bytecode VM, native functions, scheduler, I/O, retentive data, variable access |
| `crates/structure-cli` | The `structure` command |

Documentation required by subclause 1.5.1 is in `docs/`.

## License

MIT
