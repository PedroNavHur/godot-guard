//! Port of tests/test_jobs.py: `gate.jobs: auto` under cgroup v2 CPU quotas, with 12 usable CPUs.

use std::fs;
use std::path::{Path, PathBuf};

use godot_guard::jobs::auto_jobs;

const USABLE: usize = 12;

struct Cgroups {
    _temp: tempfile::TempDir,
    root: PathBuf,
    proc: PathBuf,
}

impl Cgroups {
    fn new() -> Cgroups {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        Cgroups {
            proc: root.join("proc-cgroup"),
            root,
            _temp: temp,
        }
    }

    fn cgroup(&self, path: &str, limits: &[(&str, &str)]) {
        for (directory, text) in limits {
            fs::create_dir_all(self.root.join(directory)).unwrap();
            fs::write(self.root.join(directory).join("cpu.max"), text).unwrap();
        }
        fs::write(&self.proc, format!("0::{path}\n")).unwrap();
    }

    fn jobs(&self) -> usize {
        self.jobs_from(&self.proc)
    }

    fn jobs_from(&self, proc: &Path) -> usize {
        auto_jobs(&self.root, proc, USABLE).unwrap()
    }
}

#[test]
fn quota_rounds_up_to_whole_cpus() {
    let cgroups = Cgroups::new();
    cgroups.cgroup("/a/b", &[("a/b", "900000 100000\n")]);
    assert_eq!(cgroups.jobs(), 9);
    cgroups.cgroup("/a/b", &[("a/b", "250000 100000\n")]);
    assert_eq!(cgroups.jobs(), 3);
    cgroups.cgroup("/a/b", &[("a/b", "1000 100000\n")]);
    assert_eq!(cgroups.jobs(), 1);
}

#[test]
fn tightest_ancestor_wins() {
    let cgroups = Cgroups::new();
    cgroups.cgroup("/a/b", &[("a", "150000 100000\n"), ("a/b", "max 100000\n")]);
    assert_eq!(cgroups.jobs(), 2);
    let cgroups = Cgroups::new();
    cgroups.cgroup("/a/b", &[("", "max 100000\n"), ("a", "1100000 100000\n")]);
    assert_eq!(cgroups.jobs(), 11);
}

#[test]
fn limit_above_usable_cpus_is_ignored() {
    let cgroups = Cgroups::new();
    cgroups.cgroup("/a", &[("a", "4000000 100000\n")]);
    assert_eq!(cgroups.jobs(), 12);
}

#[test]
fn unlimited_or_unknown_cgroups_use_every_cpu() {
    let cgroups = Cgroups::new();
    cgroups.cgroup("/a/b", &[("a/b", "max 100000\n")]);
    assert_eq!(cgroups.jobs(), 12);
    fs::write(&cgroups.proc, "1:cpu,cpuacct:/a\n").unwrap();
    assert_eq!(cgroups.jobs(), 12);
    assert_eq!(cgroups.jobs_from(&cgroups.root.join("missing")), 12);
}

#[test]
fn never_below_one() {
    let cgroups = Cgroups::new();
    cgroups.cgroup("/", &[("", "0 100000\n")]);
    assert_eq!(cgroups.jobs(), 1);
}
