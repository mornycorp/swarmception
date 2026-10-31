//! `swarmception-cli`: Swarmception's engine without a window (docs/architecture.md ›
//! Automation). It lists the engine's commands, or opens a repository and runs commands on one
//! session, printing each result as JSON: how an agent checks a change without a window.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

use std::io::Write;
use std::process::ExitCode;

use swarmception_engine::{EngineError, Session};

const USAGE: &str = "\
usage: swarmception-cli [--repo <path>] <command>

  commands                          list the engine's commands: ids, labels, params and result schemas
  run <id> [<params JSON>] [<id> [<params JSON>]]…
                                    run commands on one session, one result a line; with --repo,
                                    after repo.open of that path

A command's error prints its code and message to stderr and exits 1; bad usage exits 2.";

enum Failure {
    Usage(String),
    Engine(EngineError),
}

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
            eprintln!("swarmception-cli: {}: {error}", error.code());
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[&str], out: &mut impl Write) -> Result<(), Failure> {
    let (repo, rest) = match args {
        ["--repo", path, rest @ ..] => (Some(*path), rest),
        rest => (None, rest),
    };
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
        [] => Err(Failure::Usage("no command".into())),
        other => Err(Failure::Usage(format!("unknown command `{}`", other.join(" ")))),
    }
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
        })?;
        Ok(String::from_utf8(out).unwrap().lines().map(str::to_owned).collect())
    }

    #[test]
    fn runs_commands_on_one_session() {
        let out = lines(&["run", "rows.set", r#"{"by":"tickets"}"#, "rows.get"]).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out[1].contains("\"tickets\""));
        assert_eq!(lines(&["run", "repo.reload"]), Err("disabled".into()));
        assert!(lines(&[]).is_err());
        assert!(lines(&["commands"]).unwrap()[0].contains("repo.open"));
    }
}
