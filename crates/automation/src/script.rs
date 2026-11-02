//! A script of requests, as `swarmception-cli send --script <file.jsonl>` runs it: a [`Step`] a
//! line, `{"command": "<id>", "params": {…}}` (params left out for none), blank lines and lines
//! starting `#` skipped. [`run`] sends them in order and stops at the first error.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::protocol::{Handler, MAX_LINE, ProtocolError};

/// The longest script read, in bytes.
pub const MAX_SCRIPT: usize = 16 * 1024 * 1024;

/// One request of a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// The command's id: the engine's, `ui.*` or `timeline.<id>`.
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// A step on its line, counted from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub number: usize,
    pub step: Step,
}

/// Why a script doesn't read: the line, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    /// Counted from 1; 0 for the script as a whole.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            0 => write!(f, "{}", self.message),
            line => write!(f, "line {line}: {}", self.message),
        }
    }
}

impl std::error::Error for ScriptError {}

/// The steps of `text`, a script; nothing runs.
///
/// # Errors
///
/// The first line that isn't a step, or a script over [`MAX_SCRIPT`] bytes.
pub fn parse(text: &str) -> Result<Vec<Line>, ScriptError> {
    if text.len() > MAX_SCRIPT {
        return Err(ScriptError { line: 0, message: format!("a script over {MAX_SCRIPT} bytes") });
    }
    let mut steps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index.saturating_add(1);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.len() > MAX_LINE {
            return Err(ScriptError { line: number, message: format!("a line over {MAX_LINE} bytes") });
        }
        let step = serde_json::from_str(trimmed)
            .map_err(|e| ScriptError { line: number, message: format!("not a step ({{\"command\": …, \"params\": …}}): {e}") })?;
        steps.push(Line { number, step });
    }
    Ok(steps)
}

/// A step that failed: where, and the server's error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub line: usize,
    pub command: String,
    pub error: ProtocolError,
}

/// Runs `steps` in order on `handler`, handing each result's JSON text to `each`, until one
/// fails.
///
/// # Errors
///
/// The first step that failed; the steps after it aren't sent.
pub fn run(steps: &[Line], handler: &mut impl Handler, mut each: impl FnMut(&Line, &str)) -> Result<(), Failed> {
    for line in steps {
        let params = line.step.params.as_ref().map_or_else(String::new, ToString::to_string);
        match handler.handle(&line.step.command, &params) {
            Ok(result) => each(line, &result),
            Err(error) => return Err(Failed { line: line.number, command: line.step.command.clone(), error }),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo(Vec<String>);

    impl Handler for Echo {
        fn handle(&mut self, command: &str, params: &str) -> Result<String, ProtocolError> {
            self.0.push(command.to_owned());
            match command {
                "fail" => Err(ProtocolError::NoUi),
                _ => Ok(if params.is_empty() { "null".into() } else { params.to_owned() }),
            }
        }
    }

    #[test]
    fn a_script_runs_in_order_and_stops_at_the_first_error() {
        let script = "# the screenshots\n\n{\"command\":\"a\",\"params\":{\"x\":1}}\n  {\"command\":\"b\"}\n{\"command\":\"fail\"}\n{\"command\":\"never\"}\n";
        let steps = parse(script).unwrap();
        assert_eq!(steps.iter().map(|l| l.number).collect::<Vec<_>>(), [3, 4, 5, 6]);
        let (mut handler, mut results) = (Echo(Vec::new()), Vec::new());
        let failed = run(&steps, &mut handler, |line, result| results.push(format!("{} {result}", line.step.command))).unwrap_err();
        assert_eq!(failed, Failed { line: 5, command: "fail".into(), error: ProtocolError::NoUi });
        assert_eq!(results, ["a {\"x\":1}", "b null"]);
        assert_eq!(handler.0, ["a", "b", "fail"], "nothing after the error is sent");
    }

    #[test]
    fn a_line_that_isnt_a_step_is_named() {
        for (script, line) in [("{\"command\":\"a\"}\n{\"comand\":\"b\"}", 2), ("nope", 1), ("{\"command\":\"a\",\"params\":1,\"x\":2}", 1), ("[]", 1)] {
            assert_eq!(parse(script).unwrap_err().line, line, "{script}");
        }
        assert!(parse(&"#".repeat(MAX_SCRIPT + 1)).is_err());
        assert!(parse("").unwrap().is_empty());
        assert!(!ScriptError { line: 2, message: "m".into() }.to_string().is_empty());
    }
}
