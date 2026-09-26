//! Spawning detached background workers.
//!
//! Claude Code cancels an in-flight status line script when a new trigger
//! fires, so anything slow must outlive the tick: the worker is the same
//! binary, started in its own process group with null stdio, and never waited
//! for. `GARNISH_NO_SPAWN=1` records the intended spawn instead (tests).

use std::ffi::OsString;
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
    /// The config file the tick loaded, if it loaded one: the worker must
    /// read the same options (`sync.fetch_interval`), and a `--config` on
    /// the status line command is not in the environment it inherits.
    pub config: Option<PathBuf>,
}

impl Job {
    /// Arguments for `garnish [--config C] refresh …`, the paths as they
    /// are: a path is any bytes, and one turned into text lossily names
    /// another file (review 2026-09-25).
    #[must_use]
    pub fn args(&self, lock_held: bool) -> Vec<OsString> {
        let mut v: Vec<OsString> = Vec::new();
        if let Some(config) = &self.config {
            v.push("--config".into());
            v.push(config.into());
        }
        v.extend([
            "refresh".into(),
            "--module".into(),
            (&self.module).into(),
            "--session".into(),
            (&self.session).into(),
            "--cwd".into(),
            (&self.cwd).into(),
        ]);
        if lock_held {
            v.push("--lock-held".into());
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
    if crate::claude_settings::env_flag(NO_SPAWN_ENV) == Some(true) {
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
    let args: Vec<String> =
        job.args(lock_held).iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let line = format!("{}\n", args.join(" "));
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
        let job = Job {
            module: "branch".into(),
            session: "s1".into(),
            cwd: PathBuf::from("/x y"),
            config: None,
        };
        assert_eq!(
            job.args(true),
            ["refresh", "--module", "branch", "--session", "s1", "--cwd", "/x y", "--lock-held"]
        );
        assert_eq!(job.args(false).len(), 7);
        // The tick's own config file, ahead of the subcommand (the flag is
        // global): a worker that re-resolved it read another file.
        let job = Job { config: Some(PathBuf::from("/c.toml")), ..job };
        assert_eq!(
            job.args(false),
            [
                "--config",
                "/c.toml",
                "refresh",
                "--module",
                "branch",
                "--session",
                "s1",
                "--cwd",
                "/x y"
            ]
        );
    }

    /// A config path is any bytes (`--config`, `GARNISH_CONFIG`), and the
    /// worker must read the file the tick read: a lossy conversion handed
    /// it a name with U+FFFD in it, which does not exist, so the worker
    /// read the defaults and an opted-in `fetch_interval` never fetched
    /// (review 2026-09-25).
    #[test]
    fn a_config_path_that_is_not_utf8_reaches_the_worker_intact() {
        use std::os::unix::ffi::OsStrExt as _;
        let config = std::ffi::OsStr::from_bytes(b"/tmp/caf\xe9.toml");
        let cwd = std::ffi::OsStr::from_bytes(b"/r\xff");
        let job = Job {
            module: "sync".into(),
            session: "s".into(),
            cwd: PathBuf::from(cwd),
            config: Some(PathBuf::from(config)),
        };
        let args = job.args(false);
        let arg = |i: usize| args.get(i).map(std::ffi::OsStr::new);
        assert_eq!((arg(0), arg(1)), (Some(std::ffi::OsStr::new("--config")), Some(config)));
        assert_eq!(arg(8), Some(cwd));
    }

    #[test]
    fn no_spawn_logs_the_job() {
        let dir = tempfile::tempdir().unwrap();
        let job = Job {
            module: "sync".into(),
            session: "s".into(),
            cwd: PathBuf::from("/r"),
            config: None,
        };
        assert_eq!(log_spawn(&job, dir.path(), false), Spawned::Logged);
        assert_eq!(log_spawn(&job, dir.path(), true), Spawned::Logged);
        let log = fs::read_to_string(dir.path().join("spawns.log")).unwrap();
        assert_eq!(log.lines().count(), 2);
        assert!(log.lines().nth(1).unwrap().ends_with("--lock-held"));
    }
}
