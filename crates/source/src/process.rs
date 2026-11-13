//! The one place a program is started (#164, swarm-core's docs/app-architecture.md › Layers and
//! Never hang on another program): `git`, `gh` and the host's folder picker all run through
//! [`Program::run`], with a deadline past which it is stopped and a cap on what it may print, its
//! output read on threads of their own so a full pipe never stalls it. `cargo xtask writes` fails on
//! a program started anywhere else.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The most of a program's stderr kept.
const STDERR_CAP: usize = 64 * 1024;

/// A program to run: what, where, with what, and how long it may take.
#[derive(Debug, Clone)]
pub struct Program {
    program: PathBuf,
    args: Vec<String>,
    dir: Option<PathBuf>,
    env: Vec<(String, String)>,
    input: Option<Vec<u8>>,
    deadline: Duration,
    cap: usize,
}

/// How a program ended: its exit code (none when a signal stopped it), whether that is success,
/// and what it printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub code: Option<i32>,
    pub success: bool,
    /// The exit status as the system says it, for a message.
    pub status: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why a program didn't run to its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessError {
    /// There is no such program.
    NotFound(String),
    /// It couldn't start, or be waited on.
    Spawn(String),
    /// It ran past its deadline and was stopped.
    Deadline { seconds: u64 },
    /// It printed more than the cap.
    TooLarge { cap: usize },
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(program) => write!(f, "{program} isn't installed"),
            Self::Spawn(message) => write!(f, "{message}"),
            Self::Deadline { seconds } => write!(f, "it took over {seconds} s and was stopped"),
            Self::TooLarge { cap } => write!(f, "it printed over {cap} bytes"),
        }
    }
}

impl std::error::Error for ProcessError {}

impl Program {
    /// `program`, stopped after `deadline`, printing at most `cap` bytes.
    #[must_use]
    pub fn new(program: &Path, deadline: Duration, cap: usize) -> Self {
        Self { program: program.to_path_buf(), args: Vec::new(), dir: None, env: Vec::new(), input: None, deadline, cap }
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Run in `dir`.
    #[must_use]
    pub fn dir(mut self, dir: &Path) -> Self {
        self.dir = Some(dir.to_path_buf());
        self
    }

    #[must_use]
    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    /// Fed `input` on its stdin; else its stdin is empty.
    #[must_use]
    pub fn input(mut self, input: Vec<u8>) -> Self {
        self.input = Some(input);
        self
    }

    /// Runs it to its end, or stops it at its deadline.
    ///
    /// # Errors
    ///
    /// [`ProcessError`]: not installed, couldn't start, past the deadline, or printed too much.
    pub fn run(self) -> Result<Ran, ProcessError> {
        let mut command = Command::new(&self.program);
        command.args(&self.args).envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        if let Some(dir) = &self.dir {
            command.current_dir(dir);
        }
        let mut child =
            command.stdin(if self.input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(
                |e| match e.kind() {
                    std::io::ErrorKind::NotFound => ProcessError::NotFound(self.program.display().to_string()),
                    _ => ProcessError::Spawn(e.to_string()),
                },
            )?;
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), self.input) {
            std::thread::spawn(move || stdin.write_all(&input));
        }
        let cap = self.cap;
        let out = child.stdout.take().map(|s| std::thread::spawn(move || capped_read(s, cap)));
        let err = child.stderr.take().map(|s| std::thread::spawn(move || capped_read(s, STDERR_CAP)));
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > self.deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProcessError::Deadline { seconds: self.deadline.as_secs() });
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => {
                    let _ = child.kill();
                    return Err(ProcessError::Spawn(e.to_string()));
                }
            }
        };
        let joined = |h: Option<std::thread::JoinHandle<(Vec<u8>, bool)>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
        let (stdout, over) = joined(out);
        let (stderr, _) = joined(err);
        if over {
            return Err(ProcessError::TooLarge { cap });
        }
        Ok(Ran { code: status.code(), success: status.success(), status: status.to_string(), stdout, stderr })
    }
}

/// Reads `source` to its end, keeping at most `cap` bytes: what it kept, and whether there was
/// more (the rest drained, so the program isn't left blocked on a full pipe).
fn capped_read(source: impl Read, cap: usize) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let limit = u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1);
    let mut taken = source.take(limit);
    let _ = taken.read_to_end(&mut out);
    let over = out.len() > cap;
    let _ = std::io::copy(&mut taken.into_inner(), &mut std::io::sink());
    out.truncate(cap);
    (out, over)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Program {
        Program::new(Path::new("sh"), Duration::from_secs(30), 1024).args(["-c", script])
    }

    #[test]
    fn a_program_runs_to_its_end_with_its_output_and_code() {
        let ran = sh("echo out; echo err >&2; exit 3").run().unwrap();
        assert_eq!((ran.code, ran.success, ran.stdout.as_slice(), ran.stderr.as_slice()), (Some(3), false, &b"out\n"[..], &b"err\n"[..]));
        let fed = sh("cat").input(b"fed".to_vec()).run().unwrap();
        assert_eq!((fed.success, fed.stdout.as_slice()), (true, &b"fed"[..]));
        let there = sh("pwd").dir(&std::env::temp_dir()).env("X", "y").run().unwrap();
        assert!(!there.stdout.is_empty());
    }

    #[test]
    fn a_deadline_a_cap_and_a_missing_program_are_errors() {
        let slow = Program::new(Path::new("sh"), Duration::from_millis(200), 1024).args(["-c", "sleep 30"]);
        assert_eq!(slow.run(), Err(ProcessError::Deadline { seconds: 0 }));
        assert_eq!(sh("yes | head -c 5000").run(), Err(ProcessError::TooLarge { cap: 1024 }));
        assert!(matches!(Program::new(Path::new("/no/such/program"), Duration::from_secs(1), 1).run(), Err(ProcessError::NotFound(_))));
    }
}
