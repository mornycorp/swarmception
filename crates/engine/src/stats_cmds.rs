//! The swarm's statistics over a window (`swarmception_domain::stats` defines each figure):
//! tickets landed per active hour first.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::{Stats, Window, stats, tickets};
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::session::Session;

/// A time: seconds since the epoch, or `YYYY-MM-DDTHH:MM:SSZ`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum At {
    Seconds(i64),
    Text(String),
}

impl At {
    fn time(&self) -> Result<Timestamp, EngineError> {
        match self {
            Self::Seconds(seconds) => Ok(Timestamp(*seconds)),
            Self::Text(text) => Timestamp::parse(text).ok_or_else(|| Refusal::BadTime { text: text.chars().take(64).collect() }.into()),
        }
    }
}

/// The window: an end left out is the run's (its first or last event).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetParams {
    pub start: Option<At>,
    pub end: Option<At>,
}

/// `stats.get`: the swarm's statistics over a window, the whole run by default.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "stats.get";
    const LABEL: &'static str = "Statistics";
    const READ_ONLY: bool = true;
    type Params = GetParams;
    type Result = Stats;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: GetParams) -> Result<Stats, EngineError> {
        let start = params.start.as_ref().map(At::time).transpose()?;
        let end = params.end.as_ref().map(At::time).transpose()?;
        if let (Some(start), Some(end)) = (start, end)
            && end < start
        {
            return Err(Refusal::BackwardWindow { start, end }.into());
        }
        let Some(loaded) = session.loaded.as_ref() else { return Err(EngineError::Disabled { command: Self::ID.into(), reason: Unavailable::NoRepo }) };
        let activity = &loaded.activity;
        Ok(stats(activity, &tickets(activity), &loaded.tracking.merges, Window { start, end }))
    }
}
