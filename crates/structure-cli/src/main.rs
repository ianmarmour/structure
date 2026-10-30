//! `structure`: compiler and runtime for IEC 61131-3 programs.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use structure_compiler::{compile, default_configuration, CompileOptions, Mode};
use structure_runtime::value::parse_value;
use structure_runtime::{ClockMode, ErrorPolicy, Image, Options, Runtime};
use structure_syntax::diag::Policy;
use structure_syntax::{FileId, SourceMap};

const COMPLIANCE: &str = include_str!("../../../docs/compliance.md");
const PARAMETERS: &str = include_str!("../../../docs/implementation-parameters.md");
const SYNTAX: &str = include_str!("../../../docs/formal-syntax.md");
const ERRORS: &str = include_str!("../../../docs/error-handling.md");
const EXTENSIONS: &str = include_str!("../../../docs/extensions.md");

#[derive(Parser)]
#[command(name = "structure", version, about = "Compiler and runtime for IEC 61131-3 (2003) programmable controller languages")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Clone)]
struct Diag {
    /// Report the use of extensions as errors (1.5.1 e, f).
    #[arg(long)]
    strict: bool,
    /// Report uses of implementation-dependent features (1.5.1 g).
    #[arg(long)]
    report_implementation_dependent: bool,
}

#[derive(clap::Args, Clone)]
struct Target {
    /// Source files (.st, .il, ...) or a compiled image (.json).
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Configuration to build when several are declared.
    #[arg(long)]
    config: Option<String>,
    /// Build a default configuration running this program cyclically.
    #[arg(long)]
    program: Option<String>,
    /// Interval of the default configuration's task.
    #[arg(long, default_value = "T#10ms")]
    interval: String,
}

#[derive(Copy, Clone, ValueEnum)]
enum OnError {
    /// Stop the resource in which the error occurred.
    Stop,
    /// Abandon the faulting execution and continue.
    Continue,
}

#[derive(Subcommand)]
enum Command {
    /// Check source files for errors without linking.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[command(flatten)]
        diag: Diag,
    },
    /// Compile and link a configuration into an executable image.
    Build {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        diag: Diag,
        /// Output file for the image.
        #[arg(short, long, default_value = "a.json")]
        output: PathBuf,
    },
    /// Run a configuration.
    Run {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        diag: Diag,
        /// Duration to run (a TIME literal); runs forever if omitted with --realtime.
        #[arg(long = "for", default_value = "T#1s")]
        duration: String,
        /// Use the wall clock instead of simulated time.
        #[arg(long)]
        realtime: bool,
        /// Simulated execution time of one VM instruction, in nanoseconds.
        #[arg(long, default_value_t = 20)]
        ns_per_instruction: u64,
        /// Non-preemptive scheduling (table 50 feature 5a).
        #[arg(long)]
        non_preemptive: bool,
        /// Handling of run-time errors.
        #[arg(long, value_enum, default_value = "stop")]
        on_error: OnError,
        /// Set a variable before starting: PATH=VALUE (IEC literal syntax).
        #[arg(long = "set")]
        sets: Vec<String>,
        /// Print the value of a variable (or access path) at the end.
        #[arg(long = "watch")]
        watches: Vec<String>,
        /// Print watched values periodically (a TIME literal).
        #[arg(long)]
        every: Option<String>,
        /// File holding retentive data: loaded for a warm restart if present, saved on exit.
        #[arg(long)]
        retain: Option<PathBuf>,
        /// Print task scheduling events.
        #[arg(long)]
        trace: bool,
    },
    /// Write source files in canonical textual form.
    Fmt {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Rewrite the files in place.
        #[arg(long)]
        write: bool,
    },
    /// Print the compliance statement (1.5.1).
    Compliance,
    /// Print the implementation-dependent parameters (annex D).
    Params,
    /// Print the formal syntax of the supported textual elements (1.5.1 i).
    Syntax,
    /// Print the treatment of the error conditions of annex E (1.5.1 d).
    Errors,
    /// Print the list of extensions (1.5.1 e).
    Extensions,
}

fn policy(d: &Diag) -> Policy {
    Policy { extensions_are_errors: d.strict, report_implementation_dependent: d.report_implementation_dependent }
}

fn load_sources(files: &[PathBuf]) -> Result<(SourceMap, Vec<FileId>), String> {
    let mut sm = SourceMap::new();
    let mut ids = Vec::new();
    for f in files {
        let id = sm.load(f).map_err(|e| format!("{}: {}", f.display(), e))?;
        ids.push(id);
    }
    Ok((sm, ids))
}

fn build_image(target: &Target, diag: &Diag) -> Result<Image, String> {
    if target.files.len() == 1 && target.files[0].extension().map(|e| e == "json").unwrap_or(false) {
        let text = std::fs::read_to_string(&target.files[0]).map_err(|e| e.to_string())?;
        return Image::from_json(&text);
    }
    let (mut sm, mut ids) = load_sources(&target.files)?;
    if let Some(p) = &target.program {
        ids.push(sm.add("<default configuration>", default_configuration(&p.to_uppercase(), &target.interval)));
    }
    let options = CompileOptions { configuration: target.config.clone() };
    let out = compile(&sm, &ids, options, Mode::Build, policy(diag));
    eprint!("{}", out.diagnostics.render(&sm));
    out.image.ok_or_else(|| format!("compilation failed with {} error(s)", out.diagnostics.error_count().max(1)))
}

fn parse_time(s: &str) -> Result<i64, String> {
    structure_runtime::text::parse_duration(s).ok_or_else(|| format!("invalid duration '{}' (use a TIME literal such as T#500ms)", s))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Command::Check { files, diag } => {
            let (sm, ids) = load_sources(&files)?;
            let out = compile(&sm, &ids, CompileOptions::default(), Mode::Check, policy(&diag));
            eprint!("{}", out.diagnostics.render(&sm));
            if out.diagnostics.has_errors() {
                eprintln!("{} error(s)", out.diagnostics.error_count());
                return Ok(ExitCode::FAILURE);
            }
            eprintln!("no violations were detected");
            Ok(ExitCode::SUCCESS)
        }
        Command::Build { target, diag, output } => {
            let image = build_image(&target, &diag)?;
            std::fs::write(&output, image.to_json()).map_err(|e| e.to_string())?;
            eprintln!("wrote {}", output.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Run { target, diag, duration, realtime, ns_per_instruction, non_preemptive, on_error, sets, watches, every, retain, trace } => {
            let image = build_image(&target, &diag)?;
            let options = Options {
                preemptive: !non_preemptive,
                clock: if realtime { ClockMode::RealTime } else { ClockMode::Virtual },
                ns_per_instruction,
                on_error: match on_error {
                    OnError::Stop => ErrorPolicy::StopResource,
                    OnError::Continue => ErrorPolicy::AbortExecution,
                },
                ..Options::default()
            };
            let mut rt = Runtime::new(image, options);
            if trace {
                rt.trace = Some(Vec::new());
            }
            let retained = retain.as_ref().and_then(|p| std::fs::read(p).ok());
            let start = match &retained {
                Some(data) => rt.warm_start(data),
                None => rt.cold_start(),
            };
            start.map_err(|e| e.to_string())?;
            for s in &sets {
                let (path, value) = s.split_once('=').ok_or_else(|| format!("--set expects PATH=VALUE, got '{}'", s))?;
                let (_, ty) = rt.resolve(path)?;
                let v = parse_value(&ty, value)?;
                rt.write(path, &v)?;
            }
            let total = parse_time(&duration)?;
            let step = match &every {
                Some(e) => parse_time(e)?,
                None => total,
            }
            .max(1);
            let mut elapsed = 0;
            let mut printed_errors = 0;
            while elapsed < total {
                let d = step.min(total - elapsed);
                rt.run_for(d);
                elapsed += d;
                if every.is_some() {
                    print_watches(&rt, &watches, true);
                }
                for e in &rt.errors[printed_errors..] {
                    eprintln!("{}", e);
                }
                printed_errors = rt.errors.len();
            }
            if every.is_none() {
                print_watches(&rt, &watches, false);
            }
            if let Some(events) = &rt.trace {
                for e in events {
                    println!("{:?}", e);
                }
            }
            if let Some(p) = &retain {
                std::fs::write(p, rt.retain_snapshot()).map_err(|e| e.to_string())?;
            }
            Ok(if rt.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
        }
        Command::Fmt { files, write } => {
            let (sm, ids) = load_sources(&files)?;
            let mut failed = false;
            for (id, path) in ids.iter().zip(&files) {
                let r = structure_syntax::parser::parse(sm.get(*id));
                let mut d = structure_syntax::Diagnostics::new();
                d.extend(r.diagnostics);
                if d.has_errors() {
                    eprint!("{}", d.render(&sm));
                    failed = true;
                    continue;
                }
                let text = structure_syntax::writer::write_unit(&r.unit);
                if write {
                    std::fs::write(path, text).map_err(|e| e.to_string())?;
                } else {
                    print!("{}", text);
                }
            }
            Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
        }
        Command::Compliance => {
            print!("{}", COMPLIANCE);
            Ok(ExitCode::SUCCESS)
        }
        Command::Params => {
            print!("{}", PARAMETERS);
            Ok(ExitCode::SUCCESS)
        }
        Command::Syntax => {
            print!("{}", SYNTAX);
            Ok(ExitCode::SUCCESS)
        }
        Command::Errors => {
            print!("{}", ERRORS);
            Ok(ExitCode::SUCCESS)
        }
        Command::Extensions => {
            print!("{}", EXTENSIONS);
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn print_watches(rt: &Runtime, watches: &[String], with_time: bool) {
    let mut line = Vec::new();
    for w in watches {
        let v = rt.read(w).or_else(|_| rt.read_access(w));
        let s = match v {
            Ok(v) => format!("{} = {}", w, v),
            Err(e) => format!("{} = <{}>", w, e),
        };
        line.push(s);
    }
    if line.is_empty() {
        return;
    }
    if with_time {
        println!("{} {}", structure_runtime::text::format_time(rt.now()), line.join("  "));
    } else {
        for l in line {
            println!("{}", l);
        }
    }
}

#[allow(dead_code)]
fn exists(p: &Path) -> bool {
    p.exists()
}
