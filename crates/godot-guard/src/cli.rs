//! Command line: godot-guard check | baseline | format | run | gate | init.
//!
//! Help texts are 0.4.0's at 80 columns; argparse rewraps them to the terminal's width, this does not.

use std::env;
use std::io::{self, Write};

use crate::argparse::{Exit, Kind, Namespace, Nargs, Opt, Parser};
use crate::check::{self, Request};
use crate::config::{self, AUTO, Config, Jobs};
use crate::error::{Failure, Result};
use crate::scaffold;
use crate::scripts::Scripts;

pub const VERSION: &str = concat!("godot-guard ", env!("CARGO_PKG_VERSION"));

const HELP_OPTION: Opt = Opt {
    strings: &["-h", "--help"],
    dest: "help",
    kind: Kind::Help,
};

/// `--jobs`: a whole number of at least 1, or `auto`.
pub fn jobs_argument(value: &str) -> std::result::Result<String, String> {
    if value == AUTO {
        return Ok(AUTO.to_owned());
    }
    match value.parse::<usize>() {
        Ok(count) if count >= 1 && value.bytes().all(|b| b.is_ascii_digit()) => {
            Ok(count.to_string())
        }
        _ => Err(format!("must be a whole number of at least 1, or {AUTO}")),
    }
}

fn flag(string: &'static [&'static str], dest: &'static str) -> Opt {
    Opt {
        strings: string,
        dest,
        kind: Kind::Flag,
    }
}

fn command(prog: &'static str, usage: &'static str, help: &'static str) -> Parser {
    Parser {
        prog,
        usage,
        help,
        options: vec![HELP_OPTION],
        positional: None,
        commands: Vec::new(),
    }
}

pub fn build_parser() -> Parser {
    let mut check = command(
        "godot-guard check",
        "usage: godot-guard check [-h] [--json] [files ...]",
        include_str!("help/check.txt"),
    );
    check.options.push(flag(&["--json"], "json"));
    check.positional = Some(("files", Nargs::Star));
    let mut baseline = command(
        "godot-guard baseline",
        "usage: godot-guard baseline [-h] [--init] [--force]",
        include_str!("help/baseline.txt"),
    );
    baseline
        .options
        .extend([flag(&["--init"], "init"), flag(&["--force"], "force")]);
    let mut format = command(
        "godot-guard format",
        "usage: godot-guard format [-h] [files ...]",
        include_str!("help/format.txt"),
    );
    format.positional = Some(("files", Nargs::Star));
    let mut run = command(
        "godot-guard run",
        "usage: godot-guard run [-h] ...",
        include_str!("help/run.txt"),
    );
    run.positional = Some(("args", Nargs::Remainder));
    let gate_usage = "usage: godot-guard gate [-h] [--fast] [--staged] [--jobs N|auto] [files ...]";
    let mut gate = command(
        "godot-guard gate",
        gate_usage,
        include_str!("help/gate.txt"),
    );
    gate.options.extend([
        flag(&["--fast"], "fast"),
        flag(&["--staged"], "staged"),
        Opt {
            strings: &["--jobs"],
            dest: "jobs",
            kind: Kind::Value(jobs_argument),
        },
    ]);
    gate.positional = Some(("files", Nargs::Star));
    let init = command(
        "godot-guard init",
        "usage: godot-guard init [-h]",
        include_str!("help/init.txt"),
    );
    Parser {
        prog: "godot-guard",
        usage: "usage: godot-guard [-h] [--version] {check,baseline,format,run,gate,init} ...",
        help: include_str!("help/godot-guard.txt"),
        options: vec![
            HELP_OPTION,
            Opt {
                strings: &["--version"],
                dest: "version",
                kind: Kind::Version,
            },
        ],
        positional: Some(("command", Nargs::Parser)),
        commands: vec![
            ("check", check),
            ("baseline", baseline),
            ("format", format),
            ("run", run),
            ("gate", gate),
            ("init", init),
        ],
    }
}

/// Write to stdout now; a closed pipe drops the rest, as it would end a shell pipeline.
pub fn emit(text: &str) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
}

fn load() -> Result<Config> {
    let cwd = env::current_dir().map_err(|e| Failure::crash("the working directory", e))?;
    config::load(&cwd)
}

/// The jobs `--jobs` asks for, else the config's.
pub fn gate_jobs(config: &Config, args: &Namespace) -> Jobs {
    match args.values.get("jobs").map(String::as_str) {
        None => config.gate.jobs,
        Some(AUTO) => Jobs::Auto,
        Some(count) => Jobs::Count(count.parse().expect("jobs_argument accepted a count")),
    }
}

fn run_command(args: &Namespace) -> Result<i32> {
    let scripts = Scripts::new();
    let (status, text) = match args
        .command
        .as_deref()
        .expect("argparse requires a command")
    {
        "check" => {
            let files = (!args.positionals.is_empty()).then(|| args.positionals.clone());
            check::check(
                &load()?,
                Request {
                    files,
                    surfaces: None,
                    as_json: args.flag("json"),
                },
                &scripts,
            )?
        }
        "baseline" => {
            check::update_baseline(&load()?, args.flag("init"), args.flag("force"), &scripts)?
        }
        "init" => {
            let cwd = env::current_dir().map_err(|e| Failure::crash("the working directory", e))?;
            let (root, _) = config::find_root(&cwd)?;
            (scaffold::init(&root)?, String::new())
        }
        // ceiling: format, run, and gate arrive with the tools and schedule ports.
        other => return Err(Failure::Tool(format!("{other} is not ported yet"))),
    };
    emit(&text);
    Ok(status)
}

/// Run the command line; the process's exit status before masking to a byte.
pub fn main(argv: &[String]) -> i32 {
    let args = match build_parser().parse(argv, VERSION) {
        Ok(args) => args,
        Err(Exit {
            status,
            stdout,
            stderr,
        }) => {
            emit(&stdout);
            eprint!("{stderr}");
            return status;
        }
    };
    match run_command(&args) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("godot-guard: {error}");
            match error {
                Failure::Config(_) | Failure::Tool(_) => 2,
                Failure::Crash(_) => 1,
            }
        }
    }
}
