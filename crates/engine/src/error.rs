//! The engine's errors, each with a stable code for transport replies, and the why as data: a
//! command that can't run now is [`EngineError::Disabled`] with an [`Unavailable`], a refused
//! value [`EngineError::Invalid`] with a [`Refusal`].

use std::fmt;

use schemars::JsonSchema;
use serde::Serialize;

/// Why a command can't run now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Unavailable {
    /// No repository is open.
    NoRepo,
    /// A read is still going.
    Loading,
    /// The repository read, but holds no events.
    NoData,
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRepo => write!(f, "no repository is open"),
            Self::Loading => write!(f, "the tracking branch is still being read"),
            Self::NoData => write!(f, "the tracking branch holds no events"),
        }
    }
}

/// Why a value was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum Refusal {
    /// The path is not a directory.
    NotADirectory { path: String },
    /// The source couldn't be read: not a repository, no tracking branch, git failing.
    Source { message: String },
    /// The timeline widget refused the data built from the rows.
    Timeline { message: String },
    /// A time that isn't `YYYY-MM-DDTHH:MM:SSZ`.
    BadTime { text: String },
    /// A window that ends before it starts.
    BackwardWindow { start: swarmception_model::Timestamp, end: swarmception_model::Timestamp },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADirectory { path } => write!(f, "{path} is not a directory"),
            Self::Source { message } | Self::Timeline { message } => write!(f, "{message}"),
            Self::BadTime { text } => write!(f, "{text:?} is not a UTC time (YYYY-MM-DDTHH:MM:SSZ) or seconds since the epoch"),
            Self::BackwardWindow { start, end } => write!(f, "the window ends ({end}) before it starts ({start})"),
        }
    }
}

/// What went wrong running a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum EngineError {
    UnknownCommand { command: String },
    BadParams { message: String },
    ParamsTooLarge { bytes: usize, limit: usize },
    Disabled { command: String, reason: Unavailable },
    Invalid { refusal: Refusal },
    Panicked { command: String },
    BadResult { message: String },
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
            Self::Panicked { .. } => "panicked",
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
            Self::Panicked { command } => write!(f, "`{command}` failed inside (a bug); the session is still usable"),
            Self::BadResult { message } => write!(f, "the result didn't serialize: {message}"),
        }
    }
}

impl std::error::Error for EngineError {}
