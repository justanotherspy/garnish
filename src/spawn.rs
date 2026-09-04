//! Spawning detached background workers.
//!
//! Claude Code cancels an in-flight status line script when a new trigger
//! fires, so anything slow must outlive the tick: the worker is the same
//! binary, started in its own process group with null stdio, and never waited
//! for. `GARNISH_NO_SPAWN=1` records the intended spawn instead (tests).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Environment variable that disables spawning and logs instead.
pub const NO_SPAWN_ENV: &str = "GARNISH_NO_SPAWN";

/// What a worker should refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// Module id.
    pub module: String,
    /// Session id.
    pub session: String,
    /// Working directory the payload reported.
    pub cwd: PathBuf,
}

impl Job {
    /// Arguments for `garnish refresh`.
    #[must_use]
    pub fn args(&self, lock_held: bool) -> Vec<String> {
        let mut v = vec![
            "refresh".to_owned(),
            "--module".to_owned(),
            self.module.clone(),
            "--session".to_owned(),
            self.session.clone(),
            "--cwd".to_owned(),
            self.cwd.to_string_lossy().into_owned(),
        ];
        if lock_held {
            v.push("--lock-held".to_owned());
        }
        v
    }
}

/// Outcome of a spawn attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum Spawned {
    /// A worker process was started.
    Process,
    /// `GARNISH_NO_SPAWN` was set; the job was logged to `spawns.log`.
    Logged,
    /// Spawning failed (the error text).
    Failed(String),
}

/// Spawn a detached worker for `job`. `lock_held` tells the worker the caller
/// already holds the module lock and hands it over.
#[must_use]
pub fn spawn(job: &Job, cache_root: &Path, lock_held: bool) -> Spawned {
    if std::env::var_os(NO_SPAWN_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        return log_spawn(job, cache_root, lock_held);
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return Spawned::Failed(format!("current_exe: {e}")),
    };
    let mut cmd = Command::new(exe);
    cmd.args(job.args(lock_held)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    match cmd.spawn() {
        Ok(_child) => Spawned::Process,
        Err(e) => Spawned::Failed(e.to_string()),
    }
}

fn log_spawn(job: &Job, cache_root: &Path, lock_held: bool) -> Spawned {
    let path = cache_root.join("spawns.log");
    let line = format!("{}\n", job.args(lock_held).join(" "));
    let result = fs::create_dir_all(cache_root).and_then(|()| {
        use std::io::Write as _;
        fs::OpenOptions::new().append(true).create(true).open(&path)?.write_all(line.as_bytes())
    });
    match result {
        Ok(()) => Spawned::Logged,
        Err(e) => Spawned::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_args_are_stable() {
        let job = Job { module: "branch".into(), session: "s1".into(), cwd: PathBuf::from("/x y") };
        assert_eq!(
            job.args(true),
            ["refresh", "--module", "branch", "--session", "s1", "--cwd", "/x y", "--lock-held"]
        );
        assert_eq!(job.args(false).len(), 7);
    }

    #[test]
    fn no_spawn_logs_the_job() {
        let dir = tempfile::tempdir().unwrap();
        let job = Job { module: "sync".into(), session: "s".into(), cwd: PathBuf::from("/r") };
        assert_eq!(log_spawn(&job, dir.path(), false), Spawned::Logged);
        assert_eq!(log_spawn(&job, dir.path(), true), Spawned::Logged);
        let log = fs::read_to_string(dir.path().join("spawns.log")).unwrap();
        assert_eq!(log.lines().count(), 2);
        assert!(log.lines().nth(1).unwrap().ends_with("--lock-held"));
    }
}
