//! Model and effort compared per worker (#67, `swarmception_domain::workers` defines each
//! figure): what the Workers pane draws.

use swarmception_domain::workers::{Comparison, compare};
use swarmception_domain::{Window, tickets};

use crate::command::Command;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::session::Session;
use crate::stats_cmds::{At, GetParams};

/// `workers.compare`: every worker session over a window (the whole run by default), rolled up
/// per model × effort: tickets claimed and landed, landed per active hour, the median claim →
/// merge and working time, send backs per landed and their causes, its `ready` checks' pass rate,
/// tickets abandoned, and its time on shift and active. Each rate and median says how many
/// samples it rests on, and is marked thin below three.
pub struct Compare;

impl Command for Compare {
    const ID: &'static str = "workers.compare";
    const LABEL: &'static str = "Compare Workers";
    const READ_ONLY: bool = true;
    type Params = GetParams;
    type Result = Comparison;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: GetParams) -> Result<Comparison, EngineError> {
        let start = params.start.as_ref().map(At::time).transpose()?;
        let end = params.end.as_ref().map(At::time).transpose()?;
        if let (Some(start), Some(end)) = (start, end)
            && end < start
        {
            return Err(Refusal::BackwardWindow { start, end }.into());
        }
        let Some(loaded) = session.loaded.as_ref() else { return Err(EngineError::Disabled { command: Self::ID.into(), reason: Unavailable::NoRepo }) };
        let activity = &loaded.activity;
        Ok(compare(activity, &tickets(activity), Window { start, end }))
    }
}
