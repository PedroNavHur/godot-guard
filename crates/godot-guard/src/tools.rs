//! Locate Godot, and run Godot with its diagnostics treated as failures.
//!
//! Every child `run_process` starts runs in its own session and is registered while it lives, so a stop
//! (SIGINT or SIGTERM) kills each one's whole process group and the gate leaves no Godot or step running.
//! The temporary directories made through `Scratch` are registered too, and the stop removes them.

use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{LazyLock, Mutex};

use crate::error::{Failure, Result};

pub const MAC_GODOT: &str = "/Applications/Godot.app/Contents/MacOS/Godot";

/// Output text as a child writes it, each piece ending in a newline.
pub type Echo<'a> = &'a mut (dyn FnMut(&str) + Send);

/// A child's environment: None inherits ours.
pub type Env = Option<Vec<(OsString, OsString)>>;

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| !m.is_dir() && m.permissions().mode() & 0o111 != 0)
        && std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0)
}

/// `shutil.which`: the first executable `name` on PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH").unwrap_or_else(|| ":/bin:/usr/bin".into());
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| executable(candidate))
}

/// $GODOT, then godot or godot4 on PATH, then the macOS app bundle.
pub fn godot() -> Result<PathBuf> {
    if let Some(godot) = env::var_os("GODOT").filter(|value| !value.is_empty()) {
        return Ok(godot.into());
    }
    if let Some(found) = ["godot", "godot4"].iter().find_map(|name| which(name)) {
        return Ok(found);
    }
    if executable(Path::new(MAC_GODOT)) {
        return Ok(MAC_GODOT.into());
    }
    Err(Failure::Tool(
        "Godot 4 not found: set $GODOT or put godot on PATH".into(),
    ))
}

pub fn is_diagnostic(line: &str) -> bool {
    line.contains("SCRIPT ERROR:") || line.starts_with("ERROR:") || line.starts_with("WARNING:")
}

/// Write to stdout and flush, as `print(..., flush=True)` does. A closed pipe drops the text, as it ends a
/// shell pipeline; any other write error is a crash, where 0.4.0's `print` raises.
pub fn write_stdout(text: &str) -> Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(Failure::crash("stdout", e)),
        _ => Ok(()),
    }
}

/// Directories made by `Scratch` and not yet dropped.
static SCRATCH: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Mutex::default);

fn scratch() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    SCRATCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A temporary directory removed on drop, and by a stop, since `process::exit` runs no destructors.
pub struct Scratch(tempfile::TempDir);

impl Scratch {
    pub fn new(prefix: &str) -> Result<Scratch> {
        let directory = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .map_err(|e| Failure::crash("a temporary directory", e))?;
        scratch().insert(directory.path().to_owned());
        Ok(Scratch(directory))
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        scratch().remove(self.0.path());
    }
}

/// Remove the directories still in use, for a stop.
pub fn remove_scratch() {
    for directory in scratch().iter() {
        // The stop exits next; a directory it cannot remove is left to the system's temp cleaning.
        let _ = std::fs::remove_dir_all(directory);
    }
}

/// Live children by pid, and whether a stop has refused new ones.
struct Registry {
    live: HashSet<i32>,
    stopped: bool,
}

static CHILDREN: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    Mutex::new(Registry {
        live: HashSet::new(),
        stopped: false,
    })
});

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    CHILDREN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Start `command` in its own session, registered; refused once a stop has begun.
fn start(command: &mut Command) -> Result<Child> {
    // Held across the spawn, so a stop waits until the child is registered and then kills it.
    let mut registry = registry();
    if registry.stopped {
        return Err(Failure::Tool(
            "interrupted; not starting another process".into(),
        ));
    }
    // SAFETY: setsid is async-signal-safe and touches no state shared with the parent.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let program = command.get_program().to_string_lossy().into_owned();
    let child = command
        .spawn()
        .map_err(|e| Failure::Crash(format!("{program}: {e}")))?;
    registry.live.insert(child.id() as i32);
    Ok(child)
}

/// Forget `child`; if it is still running (its reader or its wait failed), kill it and reap it.
fn finish(child: &mut Child) -> Result<()> {
    registry().live.remove(&(child.id() as i32));
    if matches!(child.try_wait(), Ok(None)) {
        // A child that cannot be killed fails the run rather than leaving it waiting on a live process.
        kill_session(child.id() as i32).map_err(|e| Failure::crash("killing a child", e))?;
        let _ = child.wait();
    }
    Ok(())
}

/// Refuse new children and kill the live ones, for a stop.
pub fn stop_children() {
    let mut registry = registry();
    registry.stopped = true;
    for pid in &registry.live {
        // The stop exits next; a child it cannot kill is left to the exit.
        let _ = kill_session(*pid);
    }
}

/// SIGKILL the session `pid` leads. A gone group is no error.
pub fn kill_session(pid: i32) -> io::Result<()> {
    // SAFETY: plain syscalls on a pid this process started and has not reaped.
    if unsafe { libc::killpg(pid, libc::SIGKILL) } == 0 {
        return Ok(());
    }
    match io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Ok(()),
        // macOS answers EPERM, not ESRCH, once the group's only member is a zombie. EPERM can also mean a
        // live group we may not signal, so kill the child itself: an unreaped child's pid is still ours.
        Some(libc::EPERM) if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

/// `Popen.wait()`'s status: the exit code, or minus the signal that killed the child.
pub fn status_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(0))
}

/// Move the bytes before the next `\n` or `\r` from `reader` to `line` and consume that byte; it, or None at
/// the end of the input.
fn take_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<Option<u8>> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(None);
        }
        let Some(end) = buffer.iter().position(|&b| b == b'\n' || b == b'\r') else {
            line.extend_from_slice(buffer);
            let used = buffer.len();
            reader.consume(used);
            continue;
        };
        line.extend_from_slice(&buffer[..end]);
        let terminator = buffer[end];
        reader.consume(end + 1);
        return Ok(Some(terminator));
    }
}

/// Read one line into `line`, ending in `\n` whether the child wrote `\n`, `\r\n`, or `\r`. False when the
/// input ended first, leaving the unterminated rest, if any, in `line`.
fn read_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<bool> {
    let Some(terminator) = take_line(reader, line)? else {
        return Ok(false);
    };
    if terminator == b'\r' && reader.fill_buf()?.first() == Some(&b'\n') {
        reader.consume(1);
    }
    line.push(b'\n');
    Ok(true)
}

/// Feed `reader`'s text to `echo` line by line, as Python's text mode reads it: invalid UTF-8 replaced and
/// `\r\n` and `\r` read as `\n`.
fn echo_lines(reader: impl Read, echo: Echo) -> io::Result<()> {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        let ended = read_line(&mut reader, &mut line)?;
        if !line.is_empty() {
            echo(&String::from_utf8_lossy(&line));
        }
        if !ended {
            return Ok(());
        }
        line.clear();
    }
}

/// Run `command` as a tracked child. With `echo`, its output (stderr merged in) goes to it line by line;
/// without, to the terminal.
pub fn run_process(
    command: &[OsString],
    cwd: Option<&Path>,
    env: &Env,
    echo: Option<Echo>,
) -> Result<i32> {
    let mut process = Command::new(&command[0]);
    process.args(&command[1..]);
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    if let Some(env) = env {
        process.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    }
    let Some(echo) = echo else {
        let mut child = start(&mut process)?;
        let status = child.wait();
        finish(&mut child)?;
        return Ok(status_code(status.map_err(|e| Failure::crash("wait", e))?));
    };
    let (reader, writer) = io::pipe().map_err(|e| Failure::crash("pipe", e))?;
    let stderr = writer.try_clone().map_err(|e| Failure::crash("pipe", e))?;
    process
        .stdin(Stdio::inherit())
        .stdout(writer)
        .stderr(stderr);
    let mut child = start(&mut process)?;
    // The parent's copies of the write end close here, so the read ends when the child's do.
    drop(process);
    if let Err(e) = echo_lines(reader, echo) {
        // Nothing reads the child's output now: kill it rather than wait for it to end.
        finish(&mut child)?;
        return Err(Failure::crash("reading a child's output", e));
    }
    let status = child.wait();
    finish(&mut child)?;
    Ok(status_code(status.map_err(|e| Failure::crash("wait", e))?))
}

/// Run Godot; fail on a non-zero exit or any diagnostic line. Output goes to `echo`, or to stdout.
pub fn run_godot(
    args: &[OsString],
    cwd: Option<&Path>,
    env: &Env,
    echo: Option<Echo>,
) -> Result<i32> {
    let mut diagnostics = false;
    let mut written = Ok(());
    let mut terminal = |text: &str| {
        if written.is_ok() {
            written = write_stdout(text);
        }
    };
    let echo: Echo = match echo {
        Some(echo) => echo,
        None => &mut terminal,
    };
    let mut watch = |line: &str| {
        diagnostics |= is_diagnostic(line);
        echo(line);
    };
    let mut command = vec![godot()?.into_os_string()];
    command.extend(args.iter().cloned());
    let status = run_process(&command, cwd, env, Some(&mut watch));
    written?;
    let status = status?;
    Ok(if status != 0 {
        status
    } else {
        i32::from(diagnostics)
    })
}

/// Lines of `reader` as a child's output becomes once read as text; captured in-process gdformat and
/// gdlint output is echoed through it.
pub fn read_lines(reader: impl Read) -> Vec<String> {
    let mut lines = Vec::new();
    echo_lines(reader, &mut |line: &str| lines.push(line.to_owned()))
        .expect("in-memory reads succeed");
    lines
}
