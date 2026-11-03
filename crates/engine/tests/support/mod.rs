//! What the engine's tests share: a session that reads in the background with a waker the test
//! can block on, so a test waits for its reads to land, not for a clock (#96); and fakes of `gh`
//! and `ssh` that are scripts the system's `sh` runs, never executables of their own (#106,
//! #113). Each test binary takes in this module and uses what it needs of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use swarmception_engine::Session;

/// How long a background read may take before a test fails: far longer than any read takes,
/// even on a machine loaded with builds, and reached only when one never lands (a read stuck, or
/// a wake missed). Under nextest's slow-timeout kill.
pub const LAND_WITHIN: Duration = Duration::from_secs(120);

/// The waker of a session that reads in the background: how many times it was called, which a
/// test blocks on.
#[derive(Default)]
pub struct Wakes {
    count: Mutex<usize>,
    woken: Condvar,
}

impl Wakes {
    /// A session that reads in the background, and its waker.
    pub fn session() -> (Session, Arc<Self>) {
        let wakes = Arc::new(Self::default());
        let waker = Arc::clone(&wakes);
        let session = Session::with_waker(Arc::new(move || waker.wake()));
        (session, wakes)
    }

    fn wake(&self) {
        *self.count.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.woken.notify_all();
    }

    /// How many times the session woke its front end.
    pub fn count(&self) -> usize {
        *self.count.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Polls `session` until `done` holds, as a front end that draws only when woken would: each
    /// poll after a wake. No sleeps, no clock: only a read that never lands, or a wake the
    /// session never gives, reaches [`LAND_WITHIN`], and the test fails saying `what`.
    pub fn poll_until(&self, session: &mut Session, what: &str, done: impl Fn(&Session) -> bool) {
        let deadline = Instant::now() + LAND_WITHIN;
        loop {
            let seen = self.count();
            session.poll();
            if done(session) {
                return;
            }
            let count = self.count.lock().unwrap_or_else(PoisonError::into_inner);
            let left = deadline.saturating_duration_since(Instant::now());
            let (count, _) = self.woken.wait_timeout_while(count, left, |count| *count == seen).unwrap_or_else(PoisonError::into_inner);
            assert!(*count != seen, "{what}: not within {} s (job {:?}, reading issues {})", LAND_WITHIN.as_secs(), session.job(), session.reading_issues());
        }
    }

    /// Polls `session` until its reads (the branch's and the issues') have all landed.
    pub fn settle(&self, session: &mut Session) {
        self.poll_until(session, "the reads landed", |s| s.job().is_none() && !s.reading_issues());
    }
}

/// A `gh` for a session reading the issues of `repo`, that runs `script` (shell, no `#!` line)
/// for each `gh api …`, gh's arguments in `$@`.
///
/// It is the system's `sh`, which runs `<repo>/api`, the script: `gh` runs in the repository,
/// `api` its first argument (put back in `$@`). So no file a test writes is ever exec'd itself (#106, #113): macOS
/// checks a new executable on its first exec, which took 0.3 s idle and 10–34 s under load, past
/// a 30 s deadline once. `cargo xtask fakes` fails on a test that writes one to exec. A second
/// fake for `repo` replaces the first.
pub fn fake_gh(repo: &Path, script: &str) -> PathBuf {
    std::fs::write(repo.join("api"), format!("set -- api \"$@\"\n{script}")).unwrap();
    PathBuf::from("/bin/sh")
}

/// Points `clone`'s fetches over ssh at the local bare repository `remote`, through an `ssh`
/// written in `dir`: a script `sh` runs, never exec'd itself (as [`fake_gh`]), and a simple ssh,
/// so git doesn't first run it with `-G` to ask what it is.
pub fn fake_ssh(clone: &Path, dir: &Path, remote: &Path) {
    let ssh = dir.join("fake-ssh");
    std::fs::write(&ssh, format!("exec git-upload-pack '{}'\n", remote.display())).unwrap();
    for (key, value) in [("core.sshCommand", format!("sh '{}'", ssh.display())), ("ssh.variant", "simple".to_owned())] {
        let out = std::process::Command::new("git").current_dir(clone).args(["config", key, &value]).output().unwrap();
        assert!(out.status.success(), "git config {key}: {}", String::from_utf8_lossy(&out.stderr));
    }
}
