//! `git`, run with a deadline and a cap on what it says. Its output is read on threads of their
//! own, so a full pipe never stalls it; past the deadline it is killed.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Why a git call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    /// It couldn't start (no git on the path, say).
    Spawn(String),
    /// It ran past its deadline and was killed.
    Deadline { args: String, seconds: u64 },
    /// It said more than the cap.
    TooLarge { args: String, cap: usize },
    /// It exited with an error.
    Failed { args: String, status: String, stderr: String },
    /// Its output wasn't what the call expected.
    Garbled(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(message) => write!(f, "can't run git: {message}"),
            Self::Deadline { args, seconds } => write!(f, "`git {args}` took over {seconds} s and was stopped"),
            Self::TooLarge { args, cap } => write!(f, "`git {args}` said over {cap} bytes"),
            Self::Failed { args, status, stderr } => write!(f, "`git {args}` failed ({status}): {stderr}"),
            Self::Garbled(message) => write!(f, "git's output didn't read: {message}"),
        }
    }
}

impl std::error::Error for GitError {}

/// Git in one repository, with a deadline and a cap for each call.
#[derive(Debug, Clone)]
pub struct Git {
    program: PathBuf,
    dir: PathBuf,
    deadline: Duration,
    cap: usize,
}

impl Git {
    #[must_use]
    pub fn new(dir: &Path, deadline: Duration, cap: usize) -> Self {
        Self::program(Path::new("git"), dir, deadline, cap)
    }

    /// Another program (`gh`) in place of git, run the same way: a deadline, a cap, no prompts.
    #[must_use]
    pub fn program(program: &Path, dir: &Path, deadline: Duration, cap: usize) -> Self {
        Self { program: program.to_path_buf(), dir: dir.to_path_buf(), deadline, cap }
    }

    /// Runs `git <args>`, feeding it `input`: what it printed.
    ///
    /// # Errors
    ///
    /// [`GitError`]: it couldn't start, ran past the deadline, said too much or failed.
    pub fn run(&self, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>, GitError> {
        let shown = args.join(" ");
        let mut child = Command::new(&self.program)
            .current_dir(&self.dir)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| GitError::Spawn(e.to_string()))?;
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
            std::thread::spawn(move || stdin.write_all(&input));
        }
        let cap = self.cap;
        let out = child.stdout.take().map(|s| std::thread::spawn(move || capped_read(s, cap)));
        let err = child.stderr.take().map(|s| std::thread::spawn(move || capped_read(s, 64 * 1024)));
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > self.deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GitError::Deadline { args: shown, seconds: self.deadline.as_secs() });
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => return Err(GitError::Spawn(e.to_string())),
            }
        };
        let joined = |h: Option<std::thread::JoinHandle<(Vec<u8>, bool)>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
        let (stdout, over) = joined(out);
        let (stderr, _) = joined(err);
        if over {
            return Err(GitError::TooLarge { args: shown, cap });
        }
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
            return Err(GitError::Failed { args: shown, status: status.to_string(), stderr });
        }
        Ok(stdout)
    }

    /// The contents of each of `paths` at `commit`, in order, through one `git cat-file --batch`.
    ///
    /// # Errors
    ///
    /// As [`Git::run`], or [`GitError::Garbled`] for output that doesn't read as the batch's.
    pub fn cat_files(&self, commit: &str, paths: &[String]) -> Result<Vec<String>, GitError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let input: String = paths.iter().map(|p| format!("{commit}:{p}\n")).collect();
        let out = self.run(&["cat-file", "--batch"], Some(input.into_bytes()))?;
        let mut rest = out.as_slice();
        let mut texts = Vec::with_capacity(paths.len());
        for path in paths {
            let header_end = rest.iter().position(|b| *b == b'\n').ok_or_else(|| GitError::Garbled(format!("no header for {path}")))?;
            let header = String::from_utf8_lossy(rest.get(..header_end).unwrap_or_default()).into_owned();
            rest = rest.get(header_end + 1..).unwrap_or_default();
            if header.ends_with(" missing") {
                texts.push(String::new());
                continue;
            }
            let size: usize = header.rsplit(' ').next().and_then(|s| s.parse().ok()).ok_or_else(|| GitError::Garbled(format!("header {header:?}")))?;
            let body = rest.get(..size).ok_or_else(|| GitError::Garbled(format!("{path} is cut short")))?;
            texts.push(String::from_utf8_lossy(body).into_owned());
            rest = rest.get(size + 1..).unwrap_or_default();
        }
        Ok(texts)
    }
}

/// Reads all of `source`, up to `cap` bytes: what it read, and whether there was more.
fn capped_read(source: impl Read, cap: usize) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let limit = u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1);
    let mut taken = source.take(limit);
    let _ = taken.read_to_end(&mut out);
    let over = out.len() > cap;
    // Drain the rest, so the process isn't left blocked on a full pipe.
    let _ = std::io::copy(&mut taken.into_inner(), &mut std::io::sink());
    out.truncate(cap);
    (out, over)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deadline_stops_it() {
        let git = Git::new(Path::new("."), Duration::from_millis(100), 1024);
        let started = Instant::now();
        let result = git.run(&["-c", "alias.nap=!sleep 5", "nap"], None);
        assert!(matches!(result, Err(GitError::Deadline { .. })), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn a_failure_says_why() {
        let git = Git::new(Path::new("."), Duration::from_secs(10), 1024);
        let Err(GitError::Failed { stderr, .. }) = git.run(&["no-such-command-at-all"], None) else { panic!("it ran") };
        assert!(!stderr.is_empty());
    }

    #[test]
    fn the_cap_holds() {
        let git = Git::new(Path::new("."), Duration::from_secs(10), 4);
        assert!(matches!(git.run(&["--version"], None), Err(GitError::TooLarge { .. })));
    }
}
