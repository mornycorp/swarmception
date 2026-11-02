//! The tracking format (Swarm Core's docs/tracking-format.md, version 3): the events each agent
//! instance appends to `tracking/<instance>/work.jsonl` on a swarm's `tracking` branch, parsed
//! from hostile JSON lines. Pure: no I/O, no clock. A line that doesn't parse is a [`LineError`]
//! naming its line, never a panic; an event this reader doesn't know is kept as
//! [`Kind::Other`], an instant (the format's rule for a newer writer).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

pub mod issues;
pub mod merges;
pub mod time;

use serde::{Deserialize, Serialize};

pub use issues::{Issue, IssueCache, IssueKind, IssueState, Issues, PrRef, Validator, parse_issues, parse_pulls};
pub use merges::{MainMerge, parse_merge};
pub use time::Timestamp;

/// The newest version of the format this reader knows.
pub const FORMAT: u32 = 3;
/// The longest line read; a longer one is an error, not an allocation.
pub const MAX_LINE: usize = 64 * 1024;
/// The longest instance id, machine, name or note kept; longer text is cut.
pub const MAX_TEXT: usize = 2 * 1024;

/// An agent instance's id: the machine name and a random suffix (`mxv9wfm7rr-2021a6`), letters,
/// digits, `.`, `_` and `-`.
#[must_use]
pub fn is_instance_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// A session's role.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Worker,
    Coordinator,
    /// A role this reader doesn't know, by name.
    #[serde(untagged)]
    Other(String),
}

/// Why an agent stopped working on an issue (`stop`'s `reason`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StopReason {
    Handoff,
    Blocked,
    Paused,
    Done,
    #[serde(untagged)]
    Other(String),
}

impl StopReason {
    /// The reason `text` names.
    #[must_use]
    pub fn of(text: String) -> Self {
        match text.as_str() {
            "handoff" => Self::Handoff,
            "blocked" => Self::Blocked,
            "paused" => Self::Paused,
            "done" => Self::Done,
            _ => Self::Other(capped(text)),
        }
    }
}

/// What an event says, by its `event`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    SessionStart {
        name: String,
        role: Role,
        machine: String,
        parent: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        format: Option<u32>,
    },
    SessionPause,
    SessionResume,
    /// `of`: the dead instance whose session the coordinator ends.
    SessionStop {
        reason: Option<String>,
        of: Option<String>,
    },
    Claim {
        issue: u64,
        mr: Option<u64>,
    },
    Work {
        issue: u64,
    },
    Submit {
        issue: u64,
        mr: Option<u64>,
    },
    Stop {
        issue: u64,
        reason: Option<StopReason>,
    },
    /// `of`: the instance whose claim or work it ends, when not the logger's own.
    Release {
        issue: u64,
        reason: Option<String>,
        of: Option<String>,
    },
    Merge {
        issue: u64,
        mr: Option<u64>,
    },
    Sendback {
        issue: u64,
        mr: Option<u64>,
    },
    SubStart {
        sub: String,
        name: Option<String>,
        model: Option<String>,
        effort: Option<String>,
    },
    SubStop {
        sub: String,
    },
    /// An event this reader doesn't know: an instant.
    Other {
        event: String,
    },
}

impl Kind {
    /// The issue an issue event names.
    #[must_use]
    pub const fn issue(&self) -> Option<u64> {
        match self {
            Self::Claim { issue, .. }
            | Self::Work { issue }
            | Self::Submit { issue, .. }
            | Self::Stop { issue, .. }
            | Self::Release { issue, .. }
            | Self::Merge { issue, .. }
            | Self::Sendback { issue, .. } => Some(*issue),
            _ => None,
        }
    }

    /// The pull request an event names.
    #[must_use]
    pub const fn mr(&self) -> Option<u64> {
        match self {
            Self::Claim { mr, .. } | Self::Submit { mr, .. } | Self::Merge { mr, .. } | Self::Sendback { mr, .. } => *mr,
            _ => None,
        }
    }

    /// The `event` name as the format writes it.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::SessionStart { .. } => "session.start",
            Self::SessionPause => "session.pause",
            Self::SessionResume => "session.resume",
            Self::SessionStop { .. } => "session.stop",
            Self::Claim { .. } => "claim",
            Self::Work { .. } => "work",
            Self::Submit { .. } => "submit",
            Self::Stop { .. } => "stop",
            Self::Release { .. } => "release",
            Self::Merge { .. } => "merge",
            Self::Sendback { .. } => "sendback",
            Self::SubStart { .. } => "sub.start",
            Self::SubStop { .. } => "sub.stop",
            Self::Other { event } => event,
        }
    }
}

/// One line of an agent's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// When, by the agent's own clock.
    pub t: Timestamp,
    pub kind: Kind,
    /// Its one-line note.
    pub note: Option<String>,
    /// The helper sub-agent that did it, on an issue event.
    pub sub: Option<String>,
}

/// Why a line didn't read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct LineError {
    /// The line's number, from 1.
    pub line: usize,
    pub message: String,
}

/// One agent instance's file, parsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFile {
    pub instance: String,
    /// Its events, in the file's order (oldest first).
    pub events: Vec<Event>,
    /// The lines that didn't read.
    pub errors: Vec<LineError>,
}

/// The fields any event may carry, as the format writes them: unknown fields are a newer
/// writer's and are skipped.
#[derive(Deserialize)]
struct Raw {
    t: String,
    event: String,
    name: Option<String>,
    role: Option<Role>,
    machine: Option<String>,
    parent: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    format: Option<u32>,
    reason: Option<String>,
    of: Option<String>,
    issue: Option<u64>,
    mr: Option<u64>,
    sub: Option<String>,
    note: Option<String>,
}

/// `text` cut to [`MAX_TEXT`] bytes at a character boundary.
pub(crate) fn capped(text: String) -> String {
    if text.len() <= MAX_TEXT {
        return text;
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Parses one line.
///
/// # Errors
///
/// The reason it doesn't read: too long, not JSON, a field of the wrong type, a bad time, or an
/// event missing a field it needs.
pub fn parse_line(line: &str) -> Result<Event, String> {
    if line.len() > MAX_LINE {
        return Err(format!("the line is {} bytes, over {MAX_LINE}", line.len()));
    }
    let raw: Raw = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let t = Timestamp::parse(&raw.t).ok_or_else(|| format!("`t` is not a UTC time: {:?}", capped(raw.t.clone())))?;
    let issue = |raw: &Raw| raw.issue.ok_or_else(|| format!("`{}` needs an `issue`", raw.event));
    let text = |v: Option<String>| v.map(capped);
    let kind = match raw.event.as_str() {
        "session.start" => Kind::SessionStart {
            name: capped(raw.name.clone().unwrap_or_default()),
            role: raw.role.clone().unwrap_or(Role::Other(String::new())),
            machine: capped(raw.machine.clone().unwrap_or_default()),
            parent: text(raw.parent.clone()),
            model: text(raw.model.clone()),
            effort: text(raw.effort.clone()),
            format: raw.format,
        },
        "session.pause" => Kind::SessionPause,
        "session.resume" => Kind::SessionResume,
        "session.stop" => Kind::SessionStop { reason: text(raw.reason.clone()), of: text(raw.of.clone()) },
        "claim" => Kind::Claim { issue: issue(&raw)?, mr: raw.mr },
        "work" => Kind::Work { issue: issue(&raw)? },
        "submit" => Kind::Submit { issue: issue(&raw)?, mr: raw.mr },
        "stop" => Kind::Stop { issue: issue(&raw)?, reason: raw.reason.clone().map(StopReason::of) },
        "release" => Kind::Release { issue: issue(&raw)?, reason: text(raw.reason.clone()), of: text(raw.of.clone()) },
        "merge" => Kind::Merge { issue: issue(&raw)?, mr: raw.mr },
        "sendback" => Kind::Sendback { issue: issue(&raw)?, mr: raw.mr },
        "sub.start" => Kind::SubStart {
            sub: capped(raw.sub.clone().ok_or("`sub.start` needs a `sub`")?),
            name: text(raw.name.clone()),
            model: text(raw.model.clone()),
            effort: text(raw.effort.clone()),
        },
        "sub.stop" => Kind::SubStop { sub: capped(raw.sub.clone().ok_or("`sub.stop` needs a `sub`")?) },
        other => Kind::Other { event: capped(other.to_owned()) },
    };
    let sub = if matches!(kind, Kind::SubStart { .. } | Kind::SubStop { .. }) { None } else { text(raw.sub) };
    Ok(Event { t, kind, note: text(raw.note), sub })
}

/// Parses `instance`'s file: every line that reads, and why each other didn't. Blank lines are
/// skipped.
#[must_use]
pub fn parse_file(instance: &str, text: &str) -> AgentFile {
    let mut file = AgentFile { instance: capped(instance.to_owned()), ..AgentFile::default() };
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(line) {
            Ok(event) => file.events.push(event),
            Err(message) => file.errors.push(LineError { line: number + 1, message }),
        }
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_line_reads() {
        let lines = [
            r#"{"effort":"high","event":"session.start","format":3,"machine":"m","model":"claude-opus-5-5","name":"worker-2","parent":"m-1","role":"worker","t":"2026-10-31T03:53:06Z"}"#,
            r#"{"event":"claim","issue":170,"mr":173,"t":"2026-10-31T03:53:22Z"}"#,
            r#"{"event":"work","issue":170,"t":"2026-10-31T03:53:25Z"}"#,
            r#"{"event":"sub.start","name":"ready","note":"ci --changed before submitting","sub":"ready-4bb1de4","t":"2026-10-31T04:06:00Z"}"#,
            r#"{"event":"sub.stop","note":"rc=0","sub":"ready-4bb1de4","t":"2026-10-31T04:06:37Z"}"#,
            r#"{"event":"submit","issue":170,"mr":173,"t":"2026-10-31T04:06:43Z"}"#,
            r#"{"event":"stop","issue":170,"reason":"done","t":"2026-10-31T04:07:00Z"}"#,
            r#"{"event":"release","issue":218,"of":"mxv9wfm7rr-2021a6","reason":"duplicate","note":"worker-seek has it","t":"2026-10-17T15:00:00Z"}"#,
            r#"{"event":"merge","issue":141,"mr":154,"sub":"gate-mr154-rerun","t":"2026-10-20T10:52:17Z"}"#,
            r#"{"event":"sendback","issue":141,"mr":154,"t":"2026-10-20T10:52:17Z"}"#,
            r#"{"event":"session.pause","t":"2026-10-20T10:52:17Z"}"#,
            r#"{"event":"session.resume","t":"2026-10-20T10:52:17Z"}"#,
            r#"{"event":"session.stop","of":"mxv9wfm7rr-2021a6","note":"rebooted","t":"2026-10-18T03:00:00Z"}"#,
            r#"{"event":"brand.new","t":"2026-10-18T03:00:00Z","extra":1}"#,
        ];
        let file = parse_file("m-2", &lines.join("\n"));
        assert!(file.errors.is_empty(), "{:?}", file.errors);
        let names: Vec<&str> = file.events.iter().map(|e| e.kind.name()).collect();
        assert_eq!(
            names,
            [
                "session.start",
                "claim",
                "work",
                "sub.start",
                "sub.stop",
                "submit",
                "stop",
                "release",
                "merge",
                "sendback",
                "session.pause",
                "session.resume",
                "session.stop",
                "brand.new"
            ]
        );
        assert_eq!(file.events[1].kind, Kind::Claim { issue: 170, mr: Some(173) });
        assert_eq!(file.events[6].kind, Kind::Stop { issue: 170, reason: Some(StopReason::Done) });
        assert_eq!(file.events[8].sub.as_deref(), Some("gate-mr154-rerun"));
        let Kind::SessionStart { role, format, .. } = &file.events[0].kind else { panic!("not a start") };
        assert_eq!((role, *format), (&Role::Worker, Some(3)));
    }

    #[test]
    fn hostile_lines_are_errors_with_their_numbers() {
        let text = "not json\n\n{\"event\":\"claim\",\"t\":\"2026-10-31T03:53:22Z\"}\n{\"event\":\"work\",\"issue\":\"x\",\"t\":\"2026-10-31T03:53:22Z\"}\n{\"event\":\"work\",\"issue\":1,\"t\":\"yesterday\"}\n{\"event\":\"sub.stop\",\"t\":\"2026-10-31T03:53:22Z\"}";
        let file = parse_file("m-2", text);
        assert!(file.events.is_empty());
        let lines: Vec<usize> = file.errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 3, 4, 5, 6]);
        let long = format!("{{\"event\":\"work\",\"note\":\"{}\",\"t\":\"2026-10-31T03:53:22Z\"}}", "x".repeat(MAX_LINE));
        assert!(parse_line(&long).is_err());
    }

    #[test]
    fn notes_are_capped_at_a_character_boundary() {
        let note = "é".repeat(MAX_TEXT);
        let line = format!("{{\"event\":\"session.pause\",\"note\":\"{note}\",\"t\":\"2026-10-31T03:53:22Z\"}}");
        let event = parse_line(&line).unwrap();
        assert!(event.note.unwrap().len() <= MAX_TEXT);
    }

    #[test]
    fn instance_ids() {
        assert!(is_instance_id("christohersmini-8d44f3"));
        assert!(is_instance_id("a.b_c-1"));
        assert!(!is_instance_id(""));
        assert!(!is_instance_id("../etc"));
        assert!(!is_instance_id("a b"));
    }
}
