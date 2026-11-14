//! The engine's errors, each with a stable code for transport replies, a remediation (what to do
//! about it, [`EngineError::remediation`]), and the why as data: a command that can't run now is
//! [`EngineError::Disabled`] with an [`Unavailable`], a refused value [`EngineError::Invalid`] with
//! a [`Refusal`]. A panic the dispatch caught is [`EngineError::Internal`] whose message starts
//! `panicked: ` and whose remediation is [`PANICKED`] (#180, swarm-core's docs/app-architecture.md
//! › One dispatch); a call cancelled before it ran is [`EngineError::Cancelled`].

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The remediation of a panic the dispatch caught: swarm-core's testkit's `conformance::PANICKED`,
/// word for word, so a caught panic is told from a plain `internal` on every surface.
pub const PANICKED: &str =
    "the command panicked, a bug in the app and not in the call: report it with what the app's stderr says; the app goes on, and other calls still work";

/// Why a command can't run now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Unavailable {
    /// No repository is open.
    NoRepo,
    /// A read is still going.
    Loading,
    /// The repository read, but holds no events.
    NoData,
    /// The repository's `origin` isn't on GitHub (or it has none).
    NoGithub,
    /// No repository was opened before (#160: Clear Recent has nothing to clear).
    NothingRecent,
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRepo => write!(f, "no repository is open"),
            Self::Loading => write!(f, "the tracking branch is still being read"),
            Self::NoData => write!(f, "the tracking branch holds no events"),
            Self::NoGithub => write!(f, "the repository's origin remote isn't on GitHub"),
            Self::NothingRecent => write!(f, "no repository was opened before"),
        }
    }
}

/// Why a value was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Refusal {
    /// The path is not a directory.
    NotADirectory { path: String },
    /// Nothing is at the path (#69: a folder since moved or deleted, from Open Recent).
    NoSuchFolder { path: String },
    /// The folder is not in a git repository (#69): the repository open stays open.
    NotARepository { path: String, message: String },
    /// The repository has no tracking branch, so no swarm's events (#69).
    NoTrackingBranch { path: String },
    /// The source couldn't be read: not a repository, no tracking branch, git failing.
    Source { message: String },
    /// The timeline widget refused the data built from the rows.
    Timeline { message: String },
    /// A time that isn't `YYYY-MM-DDTHH:MM:SSZ`.
    BadTime { text: String },
    /// A window that ends before it starts.
    BackwardWindow { start: swarmception_model::Timestamp, end: swarmception_model::Timestamp },
    /// What was asked for isn't in the swarm: no such ticket, gate run, session or merge.
    NotFound { what: String },
    /// A live watch's interval outside the seconds it takes (#54).
    BadInterval { seconds: u64, min: u64, max: u64 },
    /// Bars of rows the session no longer remembers (`timeline.carryBars`, #54).
    OldRows { generation: u64 },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADirectory { path } => write!(f, "{path} is not a directory"),
            Self::NoSuchFolder { path } => write!(f, "{path} isn't there: no such folder"),
            Self::NotARepository { path, .. } => write!(f, "{path} is not in a git repository: open a swarm's clone"),
            Self::NoTrackingBranch { path } => {
                write!(f, "{path} has no tracking branch (origin/tracking or tracking): not a swarm's clone, or it hasn't fetched one")
            }
            Self::Source { message } | Self::Timeline { message } => write!(f, "{message}"),
            Self::BadTime { text } => write!(f, "{text:?} is not a UTC time (YYYY-MM-DDTHH:MM:SSZ) or seconds since the epoch"),
            Self::BackwardWindow { start, end } => write!(f, "the window ends ({end}) before it starts ({start})"),
            Self::NotFound { what } => write!(f, "the swarm has no {what}"),
            Self::BadInterval { seconds, min, max } => write!(f, "a live interval is {min} to {max} seconds, not {seconds}"),
            Self::OldRows { generation } => write!(f, "the rows of generation {generation} are no longer remembered"),
        }
    }
}

/// What went wrong running a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum EngineError {
    UnknownCommand {
        command: String,
    },
    BadParams {
        message: String,
    },
    ParamsTooLarge {
        bytes: usize,
        limit: usize,
    },
    Disabled {
        command: String,
        reason: Unavailable,
    },
    Invalid {
        refusal: Refusal,
    },
    /// Something went wrong inside: a panic the dispatch caught (its message `panicked: ` and the
    /// command, its remediation [`PANICKED`]; what the panic said is on stderr, never here).
    Internal {
        message: String,
        remediation: String,
    },
    /// The call was cancelled before it ran, so it never ran (#180).
    Cancelled {
        command: String,
    },
    BadResult {
        message: String,
    },
}

impl EngineError {
    /// A panic the dispatch caught in `command` (its check or its run).
    #[must_use]
    pub fn panicked(command: &str) -> Self {
        Self::Internal { message: format!("panicked: `{command}` failed inside (a bug); the session is still usable"), remediation: PANICKED.to_owned() }
    }

    /// What to do about it: every error has one (swarm-core's contract, error-shape).
    #[must_use]
    pub fn remediation(&self) -> String {
        match self {
            Self::UnknownCommand { .. } => "ask engine.commands for the commands there are".to_owned(),
            Self::BadParams { .. } => "give the params the command's schema asks for (engine.commands lists it)".to_owned(),
            Self::ParamsTooLarge { limit, .. } => format!("send params under {limit} bytes"),
            Self::Disabled { reason, .. } => match reason {
                Unavailable::NoRepo => "open a repository first (repo.open)".to_owned(),
                Unavailable::Loading => "wait for the read to finish (repo.status), then ask again".to_owned(),
                Unavailable::NoData => "open a swarm's clone whose tracking branch has events".to_owned(),
                Unavailable::NoGithub => "open a clone whose origin is on GitHub".to_owned(),
                Unavailable::NothingRecent => "open a repository first: it is then recent".to_owned(),
            },
            Self::Invalid { .. } => "change what was asked for, as the message says, and ask again".to_owned(),
            Self::Internal { remediation, .. } => remediation.clone(),
            Self::Cancelled { .. } => "ask again if it is still wanted".to_owned(),
            Self::BadResult { .. } => PANICKED.to_owned(),
        }
    }
}

impl EngineError {
    /// The stable code a transport reply carries.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownCommand { .. } => "unknown_command",
            Self::BadParams { .. } => "bad_params",
            Self::ParamsTooLarge { .. } => "params_too_large",
            Self::Disabled { .. } => "disabled",
            Self::Invalid { .. } => "invalid",
            Self::Internal { .. } => "internal",
            Self::Cancelled { .. } => "cancelled",
            Self::BadResult { .. } => "bad_result",
        }
    }
}

impl From<Refusal> for EngineError {
    fn from(refusal: Refusal) -> Self {
        Self::Invalid { refusal }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand { command } => write!(f, "no command `{command}`"),
            Self::BadParams { message } => write!(f, "bad params: {message}"),
            Self::ParamsTooLarge { bytes, limit } => write!(f, "params of {bytes} bytes, over {limit}"),
            Self::Disabled { command, reason } => write!(f, "`{command}` can't run: {reason}"),
            Self::Invalid { refusal } => write!(f, "{refusal}"),
            Self::Internal { message, .. } => write!(f, "{message}"),
            Self::Cancelled { command } => write!(f, "`{command}` was cancelled before it ran"),
            Self::BadResult { message } => write!(f, "the result didn't serialize: {message}"),
        }
    }
}

impl std::error::Error for EngineError {}
