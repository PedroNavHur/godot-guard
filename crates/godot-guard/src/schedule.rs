//! Gate steps: what each one is, how they run in order, and how a phase runs them on a worker pool.

use std::collections::HashSet;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::error::{Failure, Result};
use crate::tools::{self, Echo, Env};

/// Phases run in this order; steps within one phase may run concurrently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// format, lint, check, and the gate.steps commands
    Prepare,
    /// Godot's import, alone
    Import,
    /// the analyzer, every test process, and smoke
    Test,
}

pub const PHASES: [Phase; 3] = [Phase::Prepare, Phase::Import, Phase::Test];

/// A step's work: given an echo (None writes to the terminal) and None or a Godot process's environment,
/// its status.
pub type Action<'a> = Box<dyn Fn(Option<Echo>, &Env) -> Result<i32> + Sync + 'a>;

/// Where the gate's own lines go: stdout, or a test's buffer.
pub type Out<'a> = dyn Fn(&str) + Sync + 'a;

pub struct Step<'a> {
    pub name: String,
    pub phase: Phase,
    pub action: Action<'a>,
    pub godot: bool,
}

impl<'a> Step<'a> {
    pub fn new(name: impl Into<String>, phase: Phase, action: Action<'a>) -> Step<'a> {
        Step {
            name: name.into(),
            phase,
            action,
            godot: false,
        }
    }

    pub fn godot(name: impl Into<String>, phase: Phase, action: Action<'a>) -> Step<'a> {
        Step {
            name: name.into(),
            phase,
            action,
            godot: true,
        }
    }
}

struct Outcome {
    name: String,
    status: i32,
    output: String,
    elapsed: f64,
}

/// With one job, run the steps in order, streamed. With more, run each phase on a pool of `jobs` workers.
pub fn run_steps(steps: &[Step], jobs: usize, out: &Out) -> Result<i32> {
    let started = Instant::now();
    let failed = if jobs == 1 {
        run_serial(steps, out)?
    } else {
        run_phases(steps, jobs, out)?
    };
    if !failed.is_empty() {
        return Ok(1);
    }
    out(&format!(
        "gate passed in {:.1}s\n",
        started.elapsed().as_secs_f64()
    ));
    Ok(0)
}

fn run_serial(steps: &[Step], out: &Out) -> Result<Vec<String>> {
    for step in steps {
        out(&format!("== {}\n", step.name));
        let began = Instant::now();
        let status = run_step(step, None, &None, out)?;
        let elapsed = began.elapsed().as_secs_f64();
        if status != 0 {
            out(&format!("== {} failed after {elapsed:.1}s\n", step.name));
            return Ok(vec![step.name.clone()]);
        }
        out(&format!("== {} passed in {elapsed:.1}s\n", step.name));
    }
    Ok(Vec::new())
}

fn run_phases(steps: &[Step], jobs: usize, out: &Out) -> Result<Vec<String>> {
    for phase in PHASES {
        let group: Vec<&Step> = steps.iter().filter(|step| step.phase == phase).collect();
        let failed = if group.is_empty() {
            Vec::new()
        } else {
            run_concurrent(&group, jobs, out)?
        };
        if !failed.is_empty() {
            out(&format!("== failed: {}\n", failed.join(", ")));
            return Ok(failed);
        }
    }
    Ok(Vec::new())
}

/// At most `jobs` steps at once, started in order and printed as they finish. After the first failure no
/// new step starts; running ones finish and print. Returns the failed steps' names.
pub fn run_concurrent(steps: &[&Step], jobs: usize, out: &Out) -> Result<Vec<String>> {
    let stop = AtomicBool::new(false);
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    let mut failed = Vec::new();
    let mut error = None;
    thread::scope(|scope| {
        for _ in 0..jobs.min(steps.len()) {
            let sender = sender.clone();
            let (stop, next) = (&stop, &next);
            scope.spawn(move || {
                while let Some(step) = steps.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let _ = sender.send(run_captured(step, stop, out));
                }
            });
        }
        drop(sender);
        for outcome in receiver {
            match outcome {
                _ if error.is_some() => {}
                Ok(None) => {}
                Ok(Some(outcome)) => {
                    print_block(&outcome, out);
                    if outcome.status != 0 {
                        failed.push(outcome.name);
                    }
                }
                Err(raised) => {
                    // 0.4.0 re-raises it here; the steps still running finish unprinted.
                    stop.store(true, Ordering::SeqCst);
                    error = Some(raised);
                }
            }
        }
    });
    error.map_or(Ok(failed), Err)
}

/// The step with its output captured, or None when a failure has already stopped the phase.
fn run_captured(step: &Step, stop: &AtomicBool, out: &Out) -> Result<Option<Outcome>> {
    if stop.load(Ordering::SeqCst) {
        return Ok(None);
    }
    let mut output = String::new();
    let began = Instant::now();
    let status = {
        let isolated = Isolation::new(step)?;
        let mut echo = |text: &str| output.push_str(text);
        let status = run_step(step, Some(&mut echo), isolated.env(), out);
        drop(isolated);
        status?
    };
    if status != 0 {
        stop.store(true, Ordering::SeqCst);
    }
    let elapsed = began.elapsed().as_secs_f64();
    Ok(Some(Outcome {
        name: step.name.clone(),
        status,
        output,
        elapsed,
    }))
}

/// One step's output, framed by its name and how it ended.
fn print_block(outcome: &Outcome, out: &Out) {
    let mut output = outcome.output.clone();
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    let ending = if outcome.status != 0 {
        "failed after"
    } else {
        "passed in"
    };
    out(&format!(
        "== {}\n{output}== {} {ending} {:.1}s\n",
        outcome.name, outcome.name, outcome.elapsed
    ));
}

/// The step's status. A missing tool fails it, with the reason written to its output.
fn run_step(step: &Step, mut echo: Option<Echo>, env: &Env, out: &Out) -> Result<i32> {
    let echoed: Option<Echo> = echo.as_deref_mut().map(|echo| echo as Echo);
    match (step.action)(echoed, env) {
        Err(Failure::Tool(message)) => {
            let line = format!("godot-guard: {message}\n");
            match echo {
                Some(echo) => echo(&line),
                None => out(&line),
            }
            Ok(127)
        }
        status => status,
    }
}

/// Temporary user-data directories now in use, removed by a stop before it exits.
static TEMPS: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Mutex::default);

fn temps() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    TEMPS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// For a Godot test step, an environment with a fresh user-data dir, removed on drop. Other steps get none.
///
/// Only TEST steps run beside each other; IMPORT runs alone and keeps the inherited environment.
pub struct Isolation {
    temp: Option<tempfile::TempDir>,
    env: Env,
}

impl Isolation {
    pub fn new(step: &Step) -> Result<Isolation> {
        if !(step.godot && step.phase == Phase::Test) {
            return Ok(Isolation {
                temp: None,
                env: None,
            });
        }
        let temp = tempfile::Builder::new()
            .prefix("godot-guard-")
            .tempdir()
            .map_err(|e| Failure::crash("a temporary directory", e))?;
        temps().insert(temp.path().to_owned());
        let mut env: Vec<_> = env::vars_os()
            .filter(|(key, _)| key != "XDG_DATA_HOME")
            .collect();
        env.push(("XDG_DATA_HOME".into(), temp.path().into()));
        if cfg!(target_os = "macos") {
            // Godot on macOS ignores XDG_DATA_HOME; HOME moves its user:// directory.
            env.retain(|(key, _)| key != "HOME");
            env.push(("HOME".into(), temp.path().into()));
        }
        Ok(Isolation {
            temp: Some(temp),
            env: Some(env),
        })
    }

    pub fn env(&self) -> &Env {
        &self.env
    }
}

impl Drop for Isolation {
    fn drop(&mut self) {
        if let Some(temp) = self.temp.take() {
            temps().remove(temp.path());
        }
    }
}

/// On SIGINT or SIGTERM, kill every child this process started, remove the step directories, and exit with
/// 128 plus the signal number, printing nothing more.
///
/// 0.4.0 does this during a gate's steps only; this covers every command, so `run` leaves no Godot behind.
pub fn stop_on_signals() -> Result<()> {
    let mut signals =
        Signals::new([SIGINT, SIGTERM]).map_err(|e| Failure::crash("signal handlers", e))?;
    thread::spawn(move || {
        if let Some(signum) = signals.forever().next() {
            tools::stop_children();
            let mut stdout = io::stdout().lock();
            let _ = stdout.flush();
            for temp in temps().iter() {
                let _ = std::fs::remove_dir_all(temp);
            }
            std::process::exit(128 + signum);
        }
    });
    Ok(())
}
