//! Off macOS: File ▸ Open…'s folder picker (#76), the desktop's own, run as a child process with
//! a deadline (no new crate): on Linux and the BSDs `zenity --file-selection --directory`
//! (GNOME's) or `kdialog --getexistingdirectory` (KDE's), the desktop's own first; on Windows
//! PowerShell's folder browser (`powershell`, else `pwsh`). When none runs (none installed, no
//! display), the window asks for a path, as it did before (#69).
//!
//! Which pickers to try ([`candidates`]) and what one's exit and output mean ([`outcome`]) are
//! pure functions, tested on every platform; the process call ([`run`]) and the trying of each
//! in turn ([`pick_from`]) are tested with `sh` where there is one. The pickers themselves run
//! only on the platforms they are for.
//!
//! The picker is modal, as AppKit's open panel is on macOS: the window waits for it, at most
//! [`DEADLINE`], after which it is stopped and taken as cancelled.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use swarmception_ui::Picked;

/// The longest a picker stays open: past it, it is stopped and taken as cancelled.
pub const DEADLINE: Duration = Duration::from_secs(10 * 60);

/// The most a picker may print: a path, not a book.
pub const OUTPUT_MOST: usize = 64 * 1024;

/// The prompt each picker shows.
const TITLE: &str = "Open a swarm's clone: a git repository with a tracking branch";

/// The environment variable that hands PowerShell the folder to start in, so no path is ever
/// spliced into its script.
const START_VAR: &str = "SWARMCEPTION_PICK_START";

/// PowerShell's folder browser: the folder on stdout and exit 0, exit 3 when cancelled, 4 when it
/// fails.
const POWERSHELL: &str = "try { \
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
Add-Type -AssemblyName System.Windows.Forms; \
$d = New-Object System.Windows.Forms.FolderBrowserDialog; \
$d.Description = $env:SWARMCEPTION_PICK_TITLE; \
$d.ShowNewFolderButton = $false; \
if ($env:SWARMCEPTION_PICK_START) { $d.SelectedPath = $env:SWARMCEPTION_PICK_START }; \
if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($d.SelectedPath); exit 0 }; \
exit 3 } catch { exit 4 }";

/// The platform, as the choice of picker reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// AppKit's open panel (`folder.rs`): no child process.
    MacOs,
    Windows,
    /// Linux and the BSDs: zenity or kdialog.
    Unix,
}

impl Platform {
    /// This build's.
    #[must_use]
    pub const fn this() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// A picker to run: its program and arguments, its environment, and the exit code that says the
/// person cancelled it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cancelled: i32,
}

/// What a picker's run said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Chosen(PathBuf),
    Cancelled,
    /// It ran but didn't work (no display, an error, output that isn't a folder): try the next.
    Failed(String),
}

/// Why a picker couldn't be run to its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// Not installed: try the next.
    NotFound,
    /// It couldn't start for another reason.
    Spawn(String),
    /// Left open past the deadline: stopped.
    Deadline,
    /// It printed more than the cap.
    TooLarge,
}

/// `start` as a picker may be handed it: an absolute folder whose path is text with no control
/// characters; else none, and the picker starts where it likes.
fn start_text(start: Option<&Path>) -> Option<String> {
    let text = start?.to_str()?;
    (Path::new(text).is_absolute() && !text.chars().any(char::is_control)).then(|| text.to_owned())
}

/// The pickers to try on `platform`, in order, starting in `start` (the open repository's
/// parent): on Unix the one of the desktop `XDG_CURRENT_DESKTOP` names (`desktop`) first, KDE's
/// kdialog, else GNOME's zenity; on Windows PowerShell's folder browser, Windows PowerShell's then
/// PowerShell 7's; none on macOS, whose open panel is AppKit's.
#[must_use]
pub fn candidates(platform: Platform, desktop: Option<&str>, start: Option<&Path>) -> Vec<Invocation> {
    let start = start_text(start);
    let owned = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    match platform {
        Platform::MacOs => Vec::new(),
        Platform::Unix => {
            let mut zenity = owned(&["--file-selection", "--directory", "--title", TITLE]);
            // A trailing separator opens the folder itself, not its parent with it selected.
            zenity.extend(start.as_ref().map(|s| format!("--filename={}/", s.trim_end_matches('/'))));
            let zenity = Invocation { program: "zenity".into(), args: zenity, env: Vec::new(), cancelled: 1 };
            // kdialog reads its start folder as a positional argument: an absolute path, so never
            // an option.
            let mut kdialog = owned(&["--title", TITLE, "--getexistingdirectory"]);
            kdialog.extend(start);
            let kdialog = Invocation { program: "kdialog".into(), args: kdialog, env: Vec::new(), cancelled: 1 };
            let kde = desktop.is_some_and(|d| d.split(':').any(|part| part.eq_ignore_ascii_case("kde")));
            if kde { vec![kdialog, zenity] } else { vec![zenity, kdialog] }
        }
        Platform::Windows => {
            let mut env = vec![("SWARMCEPTION_PICK_TITLE".to_owned(), TITLE.to_owned())];
            env.extend(start.map(|s| (START_VAR.to_owned(), s)));
            let args = owned(&["-NoProfile", "-STA", "-Command", POWERSHELL]);
            ["powershell", "pwsh"]
                .into_iter()
                .map(|program| Invocation { program: program.into(), args: args.clone(), env: env.clone(), cancelled: 3 })
                .collect()
        }
    }
}

/// Whether `text` is an absolute path on `platform`: `/…` on Unix, `C:\…` or `\\server\…` on
/// Windows (read the same on any platform, so the tests run everywhere).
fn absolute(platform: Platform, text: &str) -> bool {
    match platform {
        Platform::Windows => {
            let bytes = text.as_bytes();
            text.starts_with(r"\\") || (bytes.len() >= 3 && bytes.first().is_some_and(u8::is_ascii_alphabetic) && bytes.get(1..3) == Some(b":\\"))
        }
        Platform::MacOs | Platform::Unix => text.starts_with('/'),
    }
}

/// What `picker` on `platform` meant by exiting with `code` (none: killed by a signal) having
/// printed `stdout`: the folder chosen (exit 0 and one line, an absolute path), cancelled (its
/// cancel code), or failed.
#[must_use]
pub fn outcome(platform: Platform, picker: &Invocation, code: Option<i32>, stdout: &[u8]) -> Outcome {
    match code {
        Some(0) => {
            let Ok(text) = std::str::from_utf8(stdout) else { return Outcome::Failed("printed a path that isn't UTF-8".into()) };
            let line = text.strip_suffix('\n').map_or(text, |t| t.strip_suffix('\r').unwrap_or(t));
            if line.is_empty() {
                Outcome::Failed("chose nothing".into())
            } else if line.chars().any(char::is_control) {
                Outcome::Failed("printed more than a path".into())
            } else if !absolute(platform, line) {
                Outcome::Failed(format!("printed {line:?}, not an absolute path"))
            } else {
                Outcome::Chosen(PathBuf::from(line))
            }
        }
        Some(code) if code == picker.cancelled => Outcome::Cancelled,
        Some(code) => Outcome::Failed(format!("exited with {code}")),
        None => Outcome::Failed("was stopped by a signal".into()),
    }
}

/// Runs `picker` until it exits or `deadline` passes (then it is stopped): its exit code and what
/// it printed, at most `most` bytes. Its stderr is left out; stdin is empty.
///
/// # Errors
///
/// [`RunError`]: not installed, couldn't start, past the deadline, or printed too much.
pub fn run(picker: &Invocation, deadline: Duration, most: usize) -> Result<(Option<i32>, Vec<u8>), RunError> {
    let mut child = Command::new(&picker.program)
        .args(&picker.args)
        .envs(picker.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { RunError::NotFound } else { RunError::Spawn(e.to_string()) })?;
    // Read on a thread of its own, so a full pipe never stalls it.
    let reader = child.stdout.take().map(|out| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = out.take(u64::try_from(most).unwrap_or(u64::MAX).saturating_add(1)).read_to_end(&mut bytes);
            bytes
        })
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Deadline);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                return Err(RunError::Spawn(e.to_string()));
            }
        }
    };
    let stdout = reader.and_then(|r| r.join().ok()).unwrap_or_default();
    if stdout.len() > most {
        return Err(RunError::TooLarge);
    }
    Ok((status.code(), stdout))
}

/// Tries each of `pickers` in turn on `platform`: the first that runs says what was chosen or
/// that it was cancelled (or left open past `deadline`); one not installed or failing gives way
/// to the next; none at all is [`Picked::NoPicker`].
pub fn pick_from(platform: Platform, pickers: &[Invocation], deadline: Duration) -> Picked {
    for picker in pickers {
        match run(picker, deadline, OUTPUT_MOST) {
            Ok((code, stdout)) => match outcome(platform, picker, code, &stdout) {
                Outcome::Chosen(folder) => return Picked::Folder(folder),
                Outcome::Cancelled => return Picked::Cancelled,
                Outcome::Failed(why) => eprintln!("swarmception: the folder picker {} {why}", picker.program),
            },
            Err(RunError::NotFound) => {}
            Err(RunError::Deadline) => {
                eprintln!("swarmception: the folder picker {} was open over {} min: stopped", picker.program, deadline.as_secs() / 60);
                return Picked::Cancelled;
            }
            Err(RunError::Spawn(why)) => eprintln!("swarmception: can't run the folder picker {}: {why}", picker.program),
            Err(RunError::TooLarge) => eprintln!("swarmception: the folder picker {} printed over {OUTPUT_MOST} bytes", picker.program),
        }
    }
    Picked::NoPicker
}

/// File ▸ Open…'s picker off macOS: this platform's pickers from `start`, within [`DEADLINE`].
pub fn pick(start: Option<&Path>) -> Picked {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    pick_from(Platform::this(), &candidates(Platform::this(), desktop.as_deref(), start), DEADLINE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs(pickers: &[Invocation]) -> Vec<&str> {
        pickers.iter().map(|p| p.program.as_str()).collect()
    }

    #[test]
    fn each_platform_tries_its_own_pickers_the_desktops_first() {
        let start = Path::new("/home/a/swarms");
        let gnome = candidates(Platform::Unix, Some("ubuntu:GNOME"), Some(start));
        assert_eq!(programs(&gnome), ["zenity", "kdialog"]);
        assert_eq!(gnome[0].args, ["--file-selection", "--directory", "--title", TITLE, "--filename=/home/a/swarms/"]);
        assert_eq!(gnome[1].args, ["--title", TITLE, "--getexistingdirectory", "/home/a/swarms"]);
        assert!(gnome.iter().all(|p| p.cancelled == 1 && p.env.is_empty()));
        assert_eq!(programs(&candidates(Platform::Unix, Some("KDE"), Some(start))), ["kdialog", "zenity"]);
        assert_eq!(programs(&candidates(Platform::Unix, None, None)), ["zenity", "kdialog"], "no desktop named: GNOME's first");
        assert!(candidates(Platform::MacOs, None, Some(start)).is_empty(), "AppKit's panel, not a child");

        // Windows: PowerShell, the start folder handed over in the environment, never the script.
        let windows = candidates(Platform::Windows, None, Some(Path::new("/c/swarms")));
        assert_eq!(programs(&windows), ["powershell", "pwsh"]);
        assert!(windows.iter().all(|p| p.cancelled == 3 && p.args.last().map(String::as_str) == Some(POWERSHELL)));
        assert!(windows[0].env.contains(&(START_VAR.to_owned(), "/c/swarms".to_owned())));
    }

    #[test]
    fn a_start_folder_a_picker_could_misread_is_left_out() {
        for start in ["relative/swarms", "-rf", "/home/a\nb", "/home/a\u{1b}[31m"] {
            let unix = candidates(Platform::Unix, None, Some(Path::new(start)));
            assert!(unix.iter().all(|p| !p.args.iter().any(|a| a.contains(start))), "{start:?}: {unix:?}");
            assert!(candidates(Platform::Windows, None, Some(Path::new(start))).iter().all(|p| p.env.iter().all(|(k, _)| k != START_VAR)));
        }
        let zenity = &candidates(Platform::Unix, None, Some(Path::new("/")))[0];
        assert_eq!(zenity.args.last().map(String::as_str), Some("--filename=/"), "the root's trailing separator not doubled");
    }

    #[test]
    fn a_pickers_exit_and_output_say_what_was_chosen() {
        let [zenity, _] = <[Invocation; 2]>::try_from(candidates(Platform::Unix, None, None)).unwrap();
        let unix = |code, out: &[u8]| outcome(Platform::Unix, &zenity, code, out);
        assert_eq!(unix(Some(0), b"/home/a/swarm\n"), Outcome::Chosen(PathBuf::from("/home/a/swarm")));
        assert_eq!(unix(Some(0), b"/home/a/my swarm"), Outcome::Chosen(PathBuf::from("/home/a/my swarm")), "spaces kept, no newline needed");
        assert_eq!(unix(Some(0), "/home/ä/swärm\n".as_bytes()), Outcome::Chosen(PathBuf::from("/home/ä/swärm")));
        assert_eq!(unix(Some(1), b""), Outcome::Cancelled);
        let failures: [(Option<i32>, &[u8]); 8] = [
            (Some(0), b""),
            (Some(0), b"\n"),
            (Some(0), b"swarm\n"),
            (Some(0), b"/a\n/b\n"),
            (Some(0), b"/a\xff\n"),
            (Some(255), b""),
            (Some(5), b""),
            (None, b"/a\n"),
        ];
        for (code, out) in failures {
            assert!(matches!(unix(code, out), Outcome::Failed(_)), "{code:?} {out:?}");
        }
        let powershell = &candidates(Platform::Windows, None, None)[0];
        let windows = |code, out: &[u8]| outcome(Platform::Windows, powershell, code, out);
        assert_eq!(windows(Some(0), b"C:\\Users\\a\\swarm"), Outcome::Chosen(PathBuf::from(r"C:\Users\a\swarm")));
        assert_eq!(windows(Some(0), b"\\\\server\\share\\swarm\r\n"), Outcome::Chosen(PathBuf::from(r"\\server\share\swarm")));
        assert_eq!(windows(Some(3), b""), Outcome::Cancelled);
        assert!(matches!(windows(Some(1), b""), Outcome::Failed(_)), "PowerShell's own failure isn't a cancel");
        assert!(matches!(windows(Some(0), b"/c/swarm"), Outcome::Failed(_)));
        assert!(matches!(windows(Some(0), b"C:swarm"), Outcome::Failed(_)));
    }

    fn sh(script: &str, cancelled: i32) -> Invocation {
        Invocation { program: "sh".into(), args: vec!["-c".into(), script.into()], env: vec![("PICKED".into(), "/tmp/picked".into())], cancelled }
    }

    #[cfg(unix)]
    #[test]
    fn a_picker_runs_as_a_child_within_its_deadline_and_cap() {
        let second = Duration::from_secs(5);
        assert_eq!(run(&sh("printf '%s\\n' \"$PICKED\"", 1), second, 1024), Ok((Some(0), b"/tmp/picked\n".to_vec())), "its environment handed over");
        assert_eq!(run(&sh("exit 1", 1), second, 1024), Ok((Some(1), Vec::new())));
        let started = Instant::now();
        assert_eq!(run(&sh("sleep 30", 1), Duration::from_millis(200), 1024), Err(RunError::Deadline));
        assert!(started.elapsed() < Duration::from_secs(10), "stopped at its deadline");
        assert_eq!(run(&sh("head -c 5000 /dev/zero", 1), second, 1000), Err(RunError::TooLarge));
        let missing = Invocation { program: "worker32-no-such-picker".into(), args: Vec::new(), env: Vec::new(), cancelled: 1 };
        assert_eq!(run(&missing, second, 1024), Err(RunError::NotFound));
    }

    #[cfg(unix)]
    #[test]
    fn the_first_picker_that_runs_answers_and_none_falls_back_to_the_typed_path() {
        let second = Duration::from_secs(5);
        let missing = Invocation { program: "worker32-no-such-picker".into(), args: Vec::new(), env: Vec::new(), cancelled: 1 };
        let chooses = sh("printf '/home/a/swarm\\n'", 1);
        assert_eq!(pick_from(Platform::Unix, &[missing.clone(), chooses.clone()], second), Picked::Folder(PathBuf::from("/home/a/swarm")));
        // One that fails (no display, say) gives way; a cancel is the answer.
        assert_eq!(pick_from(Platform::Unix, &[sh("exit 255", 1), sh("exit 1", 1), chooses.clone()], second), Picked::Cancelled);
        assert_eq!(pick_from(Platform::Unix, &[sh("printf 'not/absolute'", 1), chooses], second), Picked::Folder(PathBuf::from("/home/a/swarm")));
        // None runs: the window asks for a path.
        assert_eq!(pick_from(Platform::Unix, &[missing.clone(), sh("exit 255", 1)], second), Picked::NoPicker);
        assert_eq!(pick_from(Platform::Unix, &[], second), Picked::NoPicker);
        // Left open past the deadline: stopped, taken as cancelled, the next not tried.
        assert_eq!(pick_from(Platform::Unix, &[sh("sleep 30", 1), missing], Duration::from_millis(200)), Picked::Cancelled);
    }
}
