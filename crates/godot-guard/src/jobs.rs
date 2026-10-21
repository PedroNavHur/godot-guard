//! `gate.jobs: auto`: how many gate steps this process can run at once.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::config::Jobs;
use crate::error::{Failure, Result};

const CGROUP_ROOT: &str = "/sys/fs/cgroup";
const PROC_CGROUP: &str = "/proc/self/cgroup";

/// The CPUs this process may run on: its affinity mask on Linux, the online CPUs elsewhere.
pub fn usable_cpus() -> usize {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: a zeroed cpu_set_t is a valid empty set, and sched_getaffinity writes at most its size.
        unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            if libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set) == 0 {
                return usize::try_from(libc::CPU_COUNT(&set)).unwrap_or(1);
            }
        }
    }
    // SAFETY: sysconf only reads a system value.
    let online = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    usize::try_from(online)
        .ok()
        .filter(|&count| count > 0)
        .unwrap_or(1)
}

/// This process's cgroup v2 path from the `0::/path` line; None where there is no v2 hierarchy.
fn own_cgroup(proc_cgroup: &Path) -> Option<String> {
    let text = fs::read_to_string(proc_cgroup).ok()?;
    crate::py::splitlines(&text)
        .into_iter()
        .find_map(|line| line.strip_prefix("0::").map(str::to_owned))
}

/// Whole CPUs a `cpu.max` file allows, rounded up; None when it is absent or says `max`.
fn cpu_max(file: &Path) -> Result<Option<usize>> {
    let text = match fs::read_to_string(file) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        other => other.map_err(|e| Failure::crash(file.display(), e))?,
    };
    let malformed = || Failure::crash(file.display(), "expected a quota and a period");
    let [quota, period] = text.split_whitespace().collect::<Vec<_>>()[..] else {
        return Err(malformed());
    };
    if quota == "max" {
        return Ok(None);
    }
    let (quota, period): (i64, i64) = (
        quota.parse().map_err(|_| malformed())?,
        period.parse().map_err(|_| malformed())?,
    );
    let (quota, period) = (quota.max(0), period.max(1));
    Ok(Some(
        usize::try_from((quota + period - 1) / period).unwrap_or(0),
    ))
}

/// The limits of the cgroup at `path` and each ancestor up to `root`.
fn cpu_limits(root: &Path, path: &str) -> Result<Vec<usize>> {
    let mut limits = Vec::new();
    let mut directory: PathBuf = root.join(path.trim_start_matches('/'));
    loop {
        limits.extend(cpu_max(&directory.join("cpu.max"))?);
        if directory == root {
            return Ok(limits);
        }
        match directory.parent() {
            Some(parent) => directory = parent.to_path_buf(),
            None => return Ok(limits),
        }
    }
}

/// The usable CPUs, or the tightest cgroup quota above this process if lower; never below 1.
pub fn auto_jobs(root: &Path, proc_cgroup: &Path, usable: usize) -> Result<usize> {
    let limits = match own_cgroup(proc_cgroup) {
        Some(path) => cpu_limits(root, &path)?,
        None => Vec::new(),
    };
    Ok(limits.into_iter().fold(usable, usize::min).max(1))
}

/// The worker count for a `gate.jobs` value: a whole number, or `auto` for this process's CPU budget.
pub fn worker_count(value: Jobs) -> Result<usize> {
    match value {
        Jobs::Count(count) => Ok(count),
        Jobs::Auto => auto_jobs(
            Path::new(CGROUP_ROOT),
            Path::new(PROC_CGROUP),
            usable_cpus(),
        ),
    }
}
