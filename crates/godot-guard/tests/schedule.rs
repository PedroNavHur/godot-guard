//! Port of tests/test_schedule.py: phase order, the worker pool, fail-fast, isolation, and stops.

mod common;

use std::ffi::OsString;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use common::Repo;
use common::shim::{Shim, git};
use godot_guard::error::Failure;
use godot_guard::schedule::{Isolation, Phase, Step, run_concurrent, run_steps};
use godot_guard::tools::{self, Echo};
use regex::Regex;

/// Run `steps` on `jobs` workers; the status and everything the gate printed, step output included.
fn captured(steps: &[Step], jobs: usize) -> (i32, String) {
    let printed = Mutex::new(String::new());
    let out = |text: &str| {
        printed.lock().unwrap().push_str(text);
        Ok(())
    };
    let status = run_steps(steps, jobs, &out).unwrap();
    (status, printed.into_inner().unwrap())
}

/// A step that logs its name, writes `lines` through its echo, and returns `status`.
fn fake<'a>(
    name: &'a str,
    phase: Phase,
    status: i32,
    log: &'a Mutex<Vec<String>>,
    lines: &'a [&str],
) -> Step<'a> {
    Step::new(
        name,
        phase,
        Box::new(move |echo: Option<Echo>, _| {
            log.lock().unwrap().push(name.to_owned());
            if let Some(echo) = echo {
                for line in lines {
                    echo(&format!("{line}\n"));
                }
            }
            Ok(status)
        }),
    )
}

fn logged(log: &Mutex<Vec<String>>) -> Vec<String> {
    log.lock().unwrap().clone()
}

#[test]
fn phases_run_in_order_whatever_the_config_order() {
    let log = Mutex::new(Vec::new());
    let steps = [
        fake("t", Phase::Test, 0, &log, &[]),
        fake("p", Phase::Prepare, 0, &log, &[]),
        fake("i", Phase::Import, 0, &log, &[]),
    ];
    assert_eq!(captured(&steps, 2).0, 0);
    assert_eq!(logged(&log), ["p", "i", "t"]);
}

#[test]
fn a_failed_phase_skips_the_later_ones() {
    let log = Mutex::new(Vec::new());
    let steps = [
        fake("bad", Phase::Prepare, 1, &log, &[]),
        fake("i", Phase::Import, 0, &log, &[]),
        fake("t", Phase::Test, 0, &log, &[]),
    ];
    let (status, output) = captured(&steps, 2);
    assert_eq!(status, 1);
    assert_eq!(logged(&log), ["bad"]);
    assert!(output.contains("== failed: bad\n"), "{output}");
}

#[test]
fn a_single_job_runs_in_config_order_and_stops_at_the_first_failure() {
    let log = Mutex::new(Vec::new());
    let steps = [
        fake("a", Phase::Prepare, 0, &log, &[]),
        fake("bad", Phase::Prepare, 1, &log, &[]),
        fake("c", Phase::Prepare, 0, &log, &[]),
    ];
    let (status, output) = captured(&steps, 1);
    assert_eq!(status, 1);
    assert_eq!(logged(&log), ["a", "bad"]);
    assert!(output.contains("== bad failed after"), "{output}");
}

#[test]
fn a_phase_runs_at_most_jobs_steps_at_once() {
    // A barrier of two only passes when two steps overlap; four steps on two workers fill it twice.
    let barrier = Barrier::new(2);
    let (active, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let work = || {
        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        barrier.wait();
        active.fetch_sub(1, Ordering::SeqCst);
        Ok(0)
    };
    let steps: Vec<Step> = (0..4)
        .map(|i| {
            Step::new(
                format!("s{i}"),
                Phase::Prepare,
                Box::new(move |_, _| work()),
            )
        })
        .collect();
    assert_eq!(captured(&steps, 2).0, 0);
    assert_eq!(peak.load(Ordering::SeqCst), 2);
}

#[test]
fn each_block_is_framed_and_holds_only_its_own_output() {
    let log = Mutex::new(Vec::new());
    let steps = [
        fake("a", Phase::Prepare, 0, &log, &["a1", "a2"]),
        fake("b", Phase::Prepare, 0, &log, &["b1"]),
    ];
    let (status, output) = captured(&steps, 2);
    assert_eq!(status, 0);
    for (name, lines) in [("a", "a1\na2\n"), ("b", "b1\n")] {
        let block = Regex::new(&format!(
            r"== {name}\n{lines}== {name} passed in \d+\.\ds\n"
        ))
        .unwrap();
        assert!(block.is_match(&output), "{output}");
    }
}

#[test]
fn a_failed_block_is_framed_as_failed() {
    let log = Mutex::new(Vec::new());
    let (status, output) = captured(&[fake("bad", Phase::Prepare, 3, &log, &["oops"])], 2);
    assert_eq!(status, 1);
    let framed =
        Regex::new(r"== bad\noops\n== bad failed after \d+\.\ds\n== failed: bad\n").unwrap();
    assert!(framed.is_match(&output), "{output}");
}

#[test]
fn no_new_step_starts_after_a_failure() {
    let log = Mutex::new(Vec::new());
    let mut steps = vec![fake("bad", Phase::Prepare, 1, &log, &[])];
    steps.extend(
        ["later0", "later1", "later2"].map(|name| fake(name, Phase::Prepare, 0, &log, &[])),
    );
    let refs: Vec<&Step> = steps.iter().collect();
    let failed = run_concurrent(&refs, 1, &|_: &str| Ok(())).unwrap();
    assert_eq!(failed, ["bad"]);
    assert_eq!(logged(&log), ["bad"]);
}

#[test]
fn running_steps_finish_and_print_after_a_failure() {
    // `bad` fails only once `slow` is running, so `slow` is in flight when the failure lands.
    let started = AtomicBool::new(false);
    let slow = Step::new(
        "slow",
        Phase::Prepare,
        Box::new(|echo: Option<Echo>, _| {
            started.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(200));
            echo.expect("a captured step")("done\n");
            Ok(0)
        }),
    );
    let bad = Step::new(
        "bad",
        Phase::Prepare,
        Box::new(|_, _| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !started.load(Ordering::SeqCst) && Instant::now() < deadline {
                thread::yield_now();
            }
            Ok(1)
        }),
    );
    let (status, output) = captured(&[slow, bad], 2);
    assert_eq!(status, 1);
    assert!(
        Regex::new(r"== slow\ndone\n== slow passed in")
            .unwrap()
            .is_match(&output),
        "{output}"
    );
}

#[test]
fn a_missing_tool_fails_its_step_with_127_and_the_reason() {
    let missing = Step::new(
        "tool",
        Phase::Prepare,
        Box::new(|_, _| Err(Failure::Tool("gone".into()))),
    );
    let (status, output) = captured(&[missing], 2);
    assert_eq!(status, 1);
    assert!(
        output.contains("== tool\ngodot-guard: gone\n== tool failed after"),
        "{output}"
    );
}

fn idle_step(name: &str, phase: Phase) -> Step<'_> {
    Step::new(name, phase, Box::new(|_, _| Ok(0)))
}

fn value(env: &[(OsString, OsString)], key: &str) -> Option<OsString> {
    env.iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
}

#[test]
fn test_steps_get_a_fresh_data_dir_that_is_removed_afterwards() {
    let step = idle_step("tests", Phase::Test);
    let isolated = Isolation::new(&step).unwrap();
    let env = isolated.env().clone().expect("an environment");
    let data = std::path::PathBuf::from(value(&env, "XDG_DATA_HOME").unwrap());
    assert!(data.is_dir());
    assert!(
        data.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("godot-guard-")
    );
    assert_eq!(
        env.iter()
            .filter(|(name, _)| name == "XDG_DATA_HOME")
            .count(),
        1
    );
    if cfg!(target_os = "macos") {
        // Godot on macOS ignores XDG_DATA_HOME; HOME moves its user:// directory.
        assert_eq!(value(&env, "HOME"), value(&env, "XDG_DATA_HOME"));
    } else {
        assert_eq!(value(&env, "HOME"), std::env::var_os("HOME"));
    }
    drop(isolated);
    assert!(!data.exists());
}

#[test]
fn other_steps_and_import_get_no_environment() {
    let log = Mutex::new(Vec::new());
    assert!(
        Isolation::new(&fake("lint", Phase::Prepare, 0, &log, &[]))
            .unwrap()
            .env()
            .is_none()
    );
    assert!(
        Isolation::new(&idle_step("import", Phase::Import))
            .unwrap()
            .env()
            .is_none()
    );
}

#[test]
fn test_steps_in_a_phase_get_their_own_dirs() {
    let seen = Mutex::new(Vec::new());
    let record = |env: &Option<Vec<(OsString, OsString)>>| {
        seen.lock()
            .unwrap()
            .push(value(env.as_ref().unwrap(), "XDG_DATA_HOME").unwrap());
        Ok(0)
    };
    let steps = [
        Step::new("one", Phase::Test, Box::new(move |_, env| record(env))),
        Step::new("two", Phase::Test, Box::new(move |_, env| record(env))),
    ];
    assert_eq!(captured(&steps, 2).0, 0);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_ne!(seen[0], seen[1]);
}

#[test]
fn a_childs_output_is_read_as_python_reads_text() {
    let lines = tools::read_lines(&b"a\r\nb\rc\nbad \xff\nend"[..]);
    assert_eq!(lines, ["a\n", "b\n", "c\n", "bad \u{fffd}\n", "end"]);
}

#[test]
fn a_refused_group_kill_of_a_gone_child_is_no_error() {
    let mut child = Command::new("true").spawn().unwrap();
    let pid = child.id() as i32;
    child.wait().unwrap();
    tools::kill_session(pid).unwrap();
}

/// Run a gate whose one step is mid-`sleep 30`, send it `signum`, and check it exits 128 plus the signal
/// and its step's group is gone.
fn stop_gate_mid_step(signum: i32, jobs: &str) {
    let repo = Repo::new(
        &[],
        Some(
            "paths: [.]\ngate:\n  format: false\n  lint: false\n  import: false\n  analyzer: false\n  smoke: false\n  steps: ['echo $$ > pid; exec sleep 30']\n",
        ),
    );
    let mut gate = Command::new(env!("CARGO_BIN_EXE_godot-guard"))
        .args(["gate", "--jobs", jobs])
        .current_dir(&repo.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = wait_for_pid(&repo.root.join("pid"));
    // SAFETY: plain signal syscalls on processes this test started.
    unsafe { libc::kill(gate.id() as i32, signum) };
    let status = gate.wait().unwrap();
    assert_eq!(status.code(), Some(128 + signum));
    assert_group_gone(pid);
}

/// The pid a step wrote to `pidfile`, once it has.
fn wait_for_pid(pidfile: &std::path::Path) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = fs::read_to_string(pidfile)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "the gate never started its step");
        thread::sleep(Duration::from_millis(20));
    }
}

/// Check the process group `pid` led is gone, killing it if not.
fn assert_group_gone(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    // A killed group's leader can linger as a zombie of the exited gate until init reaps it.
    while unsafe { libc::killpg(pid, 0) } == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let alive = unsafe { libc::killpg(pid, 0) } == 0;
    unsafe { libc::killpg(pid, libc::SIGKILL) };
    assert!(!alive, "the step's process group outlived the gate");
}

#[test]
fn sigterm_reaches_a_nested_guards_godot_before_the_kill() {
    let shim = Shim::new();
    let guard = env!("CARGO_BIN_EXE_godot-guard");
    let root = shim.repo.root.display();
    // This Godot ignores SIGTERM, so only the nested guard's SIGKILL ends it.
    shim.godot(
        0,
        &format!("trap '' TERM; echo $$ > \"{root}/godot.pid\"; sleep 30 & wait"),
    );
    shim.repo.write(
        ".godot-guard.yml",
        &format!(
            "paths: [.]\ngate:\n  format: false\n  lint: false\n  import: false\n  analyzer: false\n  smoke: false\n  test-commands: ['\"{guard}\" run -- --headless']\n"
        ),
    );
    let mut gate = Command::new(guard)
        .args(["gate", "--jobs", "1"])
        .current_dir(&shim.repo.root)
        .env("GODOT", shim.repo.path(".bin/godot"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = wait_for_pid(&shim.repo.root.join("godot.pid"));
    // SAFETY: a plain signal syscall on the process this test started.
    unsafe { libc::kill(gate.id() as i32, libc::SIGTERM) };
    assert_eq!(gate.wait().unwrap().code(), Some(128 + libc::SIGTERM));
    assert_group_gone(pid);
    assert_process_gone(pid);
}

#[test]
fn sigterm_reaches_godot_through_two_nested_gates() {
    let shim = Shim::new();
    let guard = env!("CARGO_BIN_EXE_godot-guard");
    let root = shim.repo.root.display();
    shim.godot(
        0,
        &format!("trap '' TERM; echo $$ > \"{root}/godot.pid\"; sleep 30 & wait"),
    );
    let gate_config = |command: String| {
        format!(
            "paths: [.]\ngate:\n  format: false\n  lint: false\n  import: false\n  analyzer: false\n  smoke: false\n  test-commands: ['{command}']\n"
        )
    };
    let inner = Repo::new(
        &[],
        Some(&gate_config(format!("\"{guard}\" run -- --headless"))),
    );
    let inner_root = inner.root.display();
    shim.repo.write(
        ".godot-guard.yml",
        &gate_config(format!("cd \"{inner_root}\" && \"{guard}\" gate --jobs 1")),
    );
    let mut gate = Command::new(guard)
        .args(["gate", "--jobs", "1"])
        .current_dir(&shim.repo.root)
        .env("GODOT", shim.repo.path(".bin/godot"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = wait_for_pid(&shim.repo.root.join("godot.pid"));
    // SAFETY: a plain signal syscall on the process this test started.
    unsafe { libc::kill(gate.id() as i32, libc::SIGTERM) };
    assert_eq!(gate.wait().unwrap().code(), Some(128 + libc::SIGTERM));
    assert_process_gone(pid);
}

/// Check the process `pid` is gone, killing it if not.
fn assert_process_gone(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    // SAFETY: plain syscalls on a pid this test recorded; signal 0 only checks that it exists.
    while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    unsafe { libc::kill(pid, libc::SIGKILL) };
    assert!(!alive, "the nested guard's Godot outlived the gate");
}

#[test]
fn sigterm_kills_the_gate_children_and_exits_with_143() {
    stop_gate_mid_step(libc::SIGTERM, "2");
}

#[test]
fn sigint_mid_serial_step_exits_130_and_kills_the_step() {
    stop_gate_mid_step(libc::SIGINT, "1");
}

#[test]
fn sigterm_mid_serial_step_exits_143_and_kills_the_step() {
    stop_gate_mid_step(libc::SIGTERM, "1");
}

/// A staged full gate whose analyzer step is mid-`sleep 30`, with temporary files going to `temp`.
fn stopped_staged_gate(temp: &std::path::Path) -> (std::process::Child, Shim) {
    let shim = Shim::new();
    shim.repo.write(
        ".godot-guard.yml",
        "paths: [.]\ngate:\n  format: false\n  lint: false\n  import: false\n  smoke: false\n",
    );
    shim.godot(0, "sleep 30");
    shim.init();
    git(&shim.repo.root, &["add", "a.gd"]);
    let gate = Command::new(env!("CARGO_BIN_EXE_godot-guard"))
        .args(["gate", "--staged", "--jobs", "1"])
        .current_dir(&shim.repo.root)
        .env("GODOT", shim.repo.path(".bin/godot"))
        .env("TMPDIR", temp)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while shim.calls().is_empty() {
        assert!(Instant::now() < deadline, "the gate never started Godot");
        thread::sleep(Duration::from_millis(20));
    }
    (gate, shim)
}

#[test]
fn a_stop_removes_the_staged_snapshot_and_the_analyzer_copy() {
    let temp = tempfile::tempdir().unwrap();
    let (mut gate, _shim) = stopped_staged_gate(temp.path());
    let before = fs::read_dir(temp.path()).unwrap().count();
    assert_eq!(before, 2, "the snapshot and the analyzer copy");
    // SAFETY: a plain signal syscall on the process this test started.
    unsafe { libc::kill(gate.id() as i32, libc::SIGTERM) };
    assert_eq!(gate.wait().unwrap().code(), Some(128 + libc::SIGTERM));
    let left: Vec<_> = fs::read_dir(temp.path()).unwrap().collect();
    assert!(left.is_empty(), "the stop left {left:?}");
}
