//! `swarmception-cli`: Swarmception's engine without a window (docs/architecture.md ›
//! Automation). It lists the engine's commands, or opens a repository and runs commands on one
//! session, printing each result as JSON: how an agent checks a change without a window. `serve`
//! speaks the app's control protocol headless (JSON lines on loopback with a token; no `ui.*`),
//! `mcp` serves the engine's commands as MCP tools over stdio, or with `--control <file>` every
//! command of a running window (the app's `--control-file`), and `send` sends a running window
//! (or `serve`) one request or a script of them (`swarmception_automation`).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use swarmception_automation::client::Remote;
use swarmception_automation::protocol::{Handler, ProtocolError};
use swarmception_automation::script::{self, Line, Step};
use swarmception_automation::session::SessionHandler;
use swarmception_automation::tcp::{Shared, bind, serve, write_listening};
use swarmception_engine::{EngineError, Session};

const USAGE: &str = "\
usage: swarmception-cli [--repo <path>] <command>

  commands                          list the engine's commands: ids, labels, params and result schemas
  run <id> [<params JSON>] [<id> [<params JSON>]]…
                                    run commands on one session, one result a line; with --repo,
                                    after repo.open of that path
  serve [--port <port>] [--control-file <path>]
                                    serve the control protocol on 127.0.0.1 (any free port by
                                    default), headless: prints the address and token as one JSON
                                    line (and writes it to the file), then answers JSON lines
  mcp [--control <file>]            serve MCP tools on stdin / stdout: the engine's commands on a
                                    session of its own; with --control, every command of the
                                    window (or server) whose control file that is, run there:
                                    the engine's, ui.* and the widget's timeline.<id>
  send --control <file> <id> [<params JSON>]
  send --control <file> --script <file.jsonl>
                                    send the window (or server) whose control file that is one
                                    request, or a script's ({\"command\": <id>, \"params\": {…}} a
                                    line; blank lines and lines starting # skipped), printing each
                                    result as a line; a script stops at its first error. A
                                    ui.screenshot's relative path is this directory's

A command's error prints its code and message to stderr and exits 1; bad usage, or a script
that doesn't read, exits 2; a window that can't be reached (no control file, nothing listening,
no answer in time) exits 3. --repo goes with commands, run, serve and mcp without --control.";

enum Failure {
    Usage(String),
    Engine(EngineError),
    /// The server answered with an error, or couldn't be reached.
    Protocol(ProtocolError),
}

/// What exit code says a server couldn't be reached.
const UNREACHABLE: u8 = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut out = std::io::stdout().lock();
    match run(&args, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("swarmception-cli: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Engine(error)) => {
            eprintln!("swarmception-cli: {}: {error}\nfix: {}", error.code(), error.remediation());
            ExitCode::FAILURE
        }
        Err(Failure::Protocol(error)) => {
            eprintln!("swarmception-cli: {}: {error}", error.code());
            if let Ok(json) = serde_json::to_string(&error) {
                eprintln!("{json}");
            }
            if unreachable(&error) { ExitCode::from(UNREACHABLE) } else { ExitCode::FAILURE }
        }
    }
}

/// Whether `error` says the server wasn't there to answer, rather than refusing.
const fn unreachable(error: &ProtocolError) -> bool {
    matches!(error, ProtocolError::Unavailable { .. } | ProtocolError::Busy { .. })
}

fn run(args: &[&str], out: &mut impl Write) -> Result<(), Failure> {
    let (repo, rest) = match args {
        ["--repo", path, rest @ ..] => (Some(*path), rest),
        rest => (None, rest),
    };
    // A running window's commands: no session here.
    match (rest, repo) {
        (["send", ..] | ["mcp", "--control", ..], Some(_)) => return Err(Failure::Usage("--repo opens a session here; a window has its own".into())),
        (["send", options @ ..], None) => return send(options, out),
        (["mcp", "--control", file], None) => return mcp(std::io::stdin().lock(), out, &mut remote(file)?),
        _ => {}
    }
    let mut session = Session::new();
    if let Some(path) = repo {
        let params = serde_json::json!({ "path": path }).to_string();
        session.execute("repo.open", &params).map_err(Failure::Engine)?;
    }
    match rest {
        ["commands"] => print(out, &session.execute("engine.commands", "").map_err(Failure::Engine)?),
        ["run", calls @ ..] if !calls.is_empty() => {
            let mut calls = calls.iter().peekable();
            while let Some(id) = calls.next() {
                let params = calls.next_if(|p| p.trim_start().starts_with(['{', '['])).copied().unwrap_or("");
                print(out, &session.execute(id, params).map_err(Failure::Engine)?)?;
            }
            Ok(())
        }
        ["serve", options @ ..] => serve_control(session, options, out),
        ["mcp"] => mcp(std::io::stdin().lock(), out, &mut SessionHandler::new(session)),
        [] => Err(Failure::Usage("no command".into())),
        other => Err(Failure::Usage(format!("unknown command `{}`", other.join(" ")))),
    }
}

fn mcp(input: impl std::io::BufRead, out: &mut impl Write, handler: &mut impl Handler) -> Result<(), Failure> {
    swarmception_automation::mcp::serve(input, out, handler).map_err(|e| Failure::Usage(format!("mcp: {e}")))
}

/// The window (or server) whose control file is `file`.
fn remote(file: &str) -> Result<Remote, Failure> {
    Remote::from_file(Path::new(file))
        .map_err(|e| Failure::Protocol(ProtocolError::Unavailable { message: format!("no window to reach: {e} (the app's --control-file, or serve's)") }))
}

/// `send`: one request, or a script's, to the window whose control file `--control` names.
fn send(options: &[&str], out: &mut impl Write) -> Result<(), Failure> {
    let (file, rest) = match options {
        ["--control", file, rest @ ..] => (*file, rest),
        _ => return Err(Failure::Usage("send takes --control <file> first".into())),
    };
    let steps = match rest {
        ["--script", path] => {
            let text = read_script(Path::new(path))?;
            script::parse(&text).map_err(|e| Failure::Usage(format!("{path}: {e}")))?
        }
        [command, params @ ..] if params.len() <= 1 && !command.starts_with('-') => {
            let params = params.first().map(|p| serde_json::from_str(p).map_err(|e| Failure::Usage(format!("params aren't JSON: {e}")))).transpose()?;
            vec![Line { number: 1, step: Step { command: (*command).to_owned(), params } }]
        }
        _ => return Err(Failure::Usage("send takes <id> [<params JSON>], or --script <file.jsonl>".into())),
    };
    let mut remote = remote(file)?;
    let mut written = Ok(());
    let ran = script::run(&steps, &mut remote, |_, result| {
        if written.is_ok() {
            written = print(out, result);
        }
    });
    written?;
    ran.map_err(|failed| {
        eprintln!("swarmception-cli: line {} ({}) failed; nothing after it was sent", failed.line, failed.command);
        Failure::Protocol(failed.error)
    })
}

fn read_script(path: &Path) -> Result<String, Failure> {
    use std::io::Read;
    let mut text = String::new();
    let limit = u64::try_from(script::MAX_SCRIPT).unwrap_or(u64::MAX).saturating_add(1);
    std::fs::File::open(path)
        .and_then(|file| file.take(limit).read_to_string(&mut text))
        .map_err(|e| Failure::Usage(format!("can't read {}: {e}", path.display())))?;
    Ok(text)
}

/// `serve`: the control protocol on loopback over `session`, until the process is stopped.
fn serve_control(session: Session, options: &[&str], out: &mut impl Write) -> Result<(), Failure> {
    let (mut port, mut file) = (0, None);
    let mut options = options.iter();
    while let Some(option) = options.next() {
        let mut value = || options.next().copied().ok_or_else(|| Failure::Usage(format!("{option} needs a value")));
        match *option {
            "--port" => {
                let text = value()?;
                port = text.parse().map_err(|_| Failure::Usage(format!("--port takes a port (0 to 65535), not {text}")))?;
            }
            "--control-file" => file = Some(std::path::PathBuf::from(value()?)),
            other => return Err(Failure::Usage(format!("serve takes --port and --control-file, not {other}"))),
        }
    }
    let (listener, listening) = bind(port, None).map_err(|e| Failure::Usage(format!("can't listen on 127.0.0.1:{port}: {e}")))?;
    // The file first: a caller that read the line finds the file written.
    if let Some(file) = file {
        write_listening(&file, &listening).map_err(|e| Failure::Usage(format!("can't write {}: {e}", file.display())))?;
    }
    print(out, &serde_json::to_string(&listening).map_err(|e| Failure::Usage(e.to_string()))?)?;
    out.flush().map_err(|e| Failure::Usage(format!("can't write: {e}")))?;
    serve(&listener, &listening.token, &Shared::new(SessionHandler::new(session)));
    Ok(())
}

fn print(out: &mut impl Write, line: &str) -> Result<(), Failure> {
    writeln!(out, "{line}").map_err(|e| Failure::Usage(format!("can't write: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(args: &[&str]) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        run(args, &mut out).map_err(|f| match f {
            Failure::Usage(m) => m,
            Failure::Engine(e) => e.code().to_owned(),
            Failure::Protocol(e) => e.code().to_owned(),
        })?;
        Ok(String::from_utf8(out).unwrap().lines().map(str::to_owned).collect())
    }

    #[test]
    fn runs_commands_on_one_session() {
        let out = lines(&["run", "rows.set", r#"{"grouping":"workers"}"#, "rows.get"]).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out[1].contains("\"workers\""));
        assert_eq!(lines(&["run", "repo.reload"]), Err("disabled".into()));
        assert!(lines(&[]).is_err());
        assert!(lines(&["commands"]).unwrap()[0].contains("repo.open"));
    }

    #[test]
    fn stats_get_is_a_command_here() {
        assert!(lines(&["commands"]).unwrap()[0].contains("\"stats.get\""));
        assert_eq!(lines(&["run", "stats.get", r#"{"start":"2026-10-31T00:00:00Z"}"#]), Err("disabled".into()), "no repository");
    }

    #[test]
    fn send_needs_a_control_file_and_a_request() {
        let usage = |args: &[&str]| matches!(run(args, &mut Vec::new()), Err(Failure::Usage(_)));
        assert!(usage(&["send", "ui.inspect"]), "no --control");
        assert!(usage(&["send", "--control", "c.json"]), "nothing to send");
        assert!(usage(&["send", "--control", "c.json", "ui.set", "{not json"]));
        assert!(usage(&["send", "--control", "c.json", "ui.set", "{}", "extra"]));
        assert!(usage(&["send", "--control", "c.json", "--script"]));
        assert!(usage(&["send", "--control", "c.json", "--script", "/no/such/script.jsonl"]));
        assert!(usage(&["--repo", ".", "send", "--control", "c.json", "ui.inspect"]));
        assert!(usage(&["--repo", ".", "mcp", "--control", "c.json"]));
        let missing = run(&["send", "--control", "/no/such/control.json", "ui.inspect"], &mut Vec::new());
        assert!(matches!(missing, Err(Failure::Protocol(ref e)) if unreachable(e)), "no window: unreachable");
    }
}
