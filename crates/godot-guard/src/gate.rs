//! `godot-guard gate`: the fast pre-commit gate and the full pre-push and CI gate.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::check::{self, Request, select};
use crate::config::{Config, TestScript};
use crate::error::{Failure, Result};
use crate::jobs::worker_count;
use crate::rules::Surfaces;
use crate::schedule::{Out, Phase, Step, run_steps};
use crate::scripts::Scripts;
use crate::snapshot::{indexed, snapshot, staged};
use crate::toolkit;
use crate::tools::{self, Echo, run_godot, run_process};

pub const ANALYZER: &str = include_str!("../../../src/godot_guard/data/analyze_scripts.gd");

/// `gdformat --check` or `gdlint` over `files`, run as 0.4.0 runs the commands, in the sources' root.
fn gdtool(
    tool: Tool,
    args: &[&str],
    files: &[String],
    root: &Path,
    echo: Option<Echo>,
) -> Result<i32> {
    if files.is_empty() {
        return Ok(0);
    }
    let argv: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string())
        .chain(files.iter().cloned())
        .collect();
    tool(&argv, root, echo)
}

type Tool = fn(&[String], &Path, Option<Echo>) -> Result<i32>;

pub fn res_path(path: &str) -> String {
    if path == "." {
        "res://".into()
    } else {
        format!("res://{path}")
    }
}

/// Godot steps in order, as (name, args after --headless --path ROOT).
pub fn godot_steps(config: &Config, analyzer: &Path) -> Vec<(String, Vec<String>)> {
    let gate = &config.gate;
    let mut steps = Vec::new();
    if gate.import {
        steps.push(("import".into(), vec!["--editor".into(), "--quit".into()]));
    }
    if gate.analyzer {
        let mut args = vec![
            "--script".into(),
            analyzer.to_string_lossy().into_owned(),
            "--".into(),
        ];
        for path in &config.paths {
            args.extend(["--root".into(), res_path(path)]);
        }
        for pattern in &config.exclude {
            args.extend(["--exclude".into(), pattern.text.clone()]);
        }
        steps.push(("analyzer".into(), args));
    }
    steps.extend(gate.tests.iter().flat_map(script_steps));
    if !gate.smoke.is_empty() {
        steps.push(("smoke".into(), gate.smoke.clone()));
    }
    steps
}

/// One step for a plain script; a script with `shards`, even 1, gets one per shard, each passed `-- --shard=I/N`.
pub fn script_steps(test: &TestScript) -> Vec<(String, Vec<String>)> {
    let script = &test.script;
    let Some(shards) = test.shards else {
        return vec![(
            format!("tests {script}"),
            vec!["--script".into(), script.clone()],
        )];
    };
    (1..=shards)
        .map(|shard| {
            let args = vec![
                "--script".into(),
                script.clone(),
                "--".into(),
                format!("--shard={shard}/{shards}"),
            ];
            (format!("tests {script} [{shard}/{shards}]"), args)
        })
        .collect()
}

/// What the steps share: the config, the copy the scripts are read from, and the run's script cache.
pub struct Plan<'a> {
    pub config: &'a Config,
    /// The selected scripts, or None for every scanned one.
    pub chosen: Option<&'a [String]>,
    /// The snapshot's config, or `config` itself.
    pub sources: &'a Config,
    pub scripts: &'a Scripts,
    /// The analyzer script, written out for Godot.
    pub analyzer: &'a Path,
}

pub fn steps_for<'a>(plan: &'a Plan<'a>, files: &'a [String], fast: bool) -> Vec<Step<'a>> {
    let (gate, root) = (&plan.config.gate, &plan.sources.root);
    let mut steps = Vec::new();
    if gate.format {
        steps.push(Step::new(
            "format",
            Phase::Prepare,
            Box::new(move |echo, _| gdtool(toolkit::gdformat, &["--check"], files, root, echo)),
        ));
    }
    if gate.lint {
        steps.push(Step::new(
            "lint",
            Phase::Prepare,
            Box::new(move |echo, _| gdtool(toolkit::gdlint, &[], files, root, echo)),
        ));
    }
    steps.push(Step::new(
        "check",
        Phase::Prepare,
        Box::new(move |echo, _| check_step(plan, echo)),
    ));
    if fast {
        return steps;
    }
    for command in &gate.steps {
        let shell = move |echo: Option<Echo>, _: &_| {
            let argv = ["/bin/sh".into(), "-c".into(), OsString::from(command)];
            run_process(&argv, Some(&plan.config.root), &None, echo)
        };
        steps.push(Step::new(command.clone(), Phase::Prepare, Box::new(shell)));
    }
    for (name, args) in godot_steps(plan.config, plan.analyzer) {
        let phase = if name == "import" {
            Phase::Import
        } else {
            Phase::Test
        };
        let mut argv: Vec<OsString> = vec![
            "--headless".into(),
            "--path".into(),
            plan.config.root.clone().into(),
        ];
        argv.extend(args.into_iter().map(OsString::from));
        steps.push(Step::godot(
            name,
            phase,
            Box::new(move |echo, env| run_godot(&argv, None, env, echo)),
        ));
    }
    steps
}

fn check_step(plan: &Plan, echo: Option<Echo>) -> Result<i32> {
    let files = plan.chosen.map(|chosen| {
        chosen
            .iter()
            .map(|path| absolute(plan.sources, path))
            .collect()
    });
    let request = Request {
        files,
        surfaces: resource_map(plan)?,
        as_json: false,
    };
    let (status, text) = check::check(plan.sources, request, plan.scripts)?;
    match echo {
        Some(echo) => echo(&text),
        None => tools::write_stdout(&text),
    }
    Ok(status)
}

/// For a snapshot: the index's other scripts as in the working tree, overlaid with the snapshot's.
fn resource_map(plan: &Plan) -> Result<Option<Surfaces>> {
    let Some(chosen) = plan
        .chosen
        .filter(|_| !std::ptr::eq(plan.sources, plan.config))
    else {
        return Ok(None);
    };
    let mut surfaces = Surfaces::new(
        plan.scripts,
        &plan.config.root,
        Some(indexed(plan.config, chosen)?),
    );
    surfaces.overlay(plan.scripts, &plan.sources.root, chosen.to_vec());
    Ok(Some(surfaces))
}

fn absolute(config: &Config, path: &str) -> String {
    config.root.join(path).to_string_lossy().into_owned()
}

/// The analyzer script in a temporary directory, for Godot's `--script`.
fn write_analyzer() -> Result<(tempfile::TempDir, PathBuf)> {
    let temp = tempfile::Builder::new()
        .prefix("godot-guard-")
        .tempdir()
        .map_err(|e| Failure::crash("a temporary directory", e))?;
    let path = temp.path().join("analyze_scripts.gd");
    fs::write(&path, ANALYZER).map_err(|e| Failure::crash(path.display(), e))?;
    Ok((temp, path))
}

fn run_gate(
    config: &Config,
    chosen: Option<&[String]>,
    fast: bool,
    sources: Option<&Config>,
) -> Result<i32> {
    if chosen.is_some_and(<[String]>::is_empty) && fast {
        tools::write_stdout("godot-guard gate: no GDScript files selected\n");
        return Ok(0);
    }
    let jobs = worker_count(config.gate.jobs)?;
    let sources = sources.unwrap_or(config);
    let files = chosen.map_or_else(|| sources.scanned_files(), <[String]>::to_vec);
    let written = (!fast && config.gate.analyzer)
        .then(write_analyzer)
        .transpose()?;
    let analyzer = written
        .as_ref()
        .map(|(_, path)| path.as_path())
        .unwrap_or(Path::new(""));
    let scripts = Scripts::new();
    let plan = Plan {
        config,
        chosen,
        sources,
        scripts: &scripts,
        analyzer,
    };
    let out: &Out = &tools::write_stdout;
    run_steps(&steps_for(&plan, &files, fast), jobs, out)
}

/// `--staged` checks the index's content of the staged scripts, not the working tree's.
pub fn gate(config: &Config, fast: bool, use_staged: bool, files: &[String]) -> Result<i32> {
    if !use_staged {
        let chosen = (!files.is_empty()).then(|| select(config, files));
        return run_gate(config, chosen.as_deref(), fast, None);
    }
    let (chosen, links) = staged(config)?;
    for link in &links {
        tools::write_stdout(&format!(
            "{link}: symlinked scripts are not supported; commit the real file\n"
        ));
    }
    if !links.is_empty() {
        return Ok(1);
    }
    if chosen.is_empty() {
        return run_gate(config, Some(&chosen), fast, None);
    }
    let (_copy, sources) = snapshot(config, &chosen)?;
    run_gate(config, Some(&chosen), fast, Some(&sources))
}
