//! The gate's runs over a window (#61, `swarmception_domain::gates` defines each figure): what the
//! Gates pane draws.

use swarmception_domain::gates::{GatesList, gates};
use swarmception_domain::{Window, tickets};

use crate::command::Command;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::session::Session;
use crate::stats_cmds::GetParams;

/// `gates.list`: every gate run that went on in a window (the whole run by default), the
/// coordinator's and the workers' `ready` checks kept apart: start, end, duration, kind, result
/// (passed, failed with its cause, no result, running), the PRs and tickets it carried, its merges
/// and send backs, what it reran, its tooltip and its details' subject; each lane's figures over
/// the runs that started in it (pass rate, median and longest duration, busy, reruns, failures by
/// cause), per-hour figures, and the longest queue at the dock. Its lanes' counts are
/// `stats.get`'s `gates` and `ready` over the same window: one definition of a gate run (#75).
pub struct List;

impl Command for List {
    const ID: &'static str = "gates.list";
    const LABEL: &'static str = "Gate Runs";
    const READ_ONLY: bool = true;
    type Params = GetParams;
    type Result = GatesList;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: GetParams) -> Result<GatesList, EngineError> {
        let start = params.start.as_ref().map(crate::stats_cmds::At::time).transpose()?;
        let end = params.end.as_ref().map(crate::stats_cmds::At::time).transpose()?;
        if let (Some(start), Some(end)) = (start, end)
            && end < start
        {
            return Err(Refusal::BackwardWindow { start, end }.into());
        }
        let Some(loaded) = session.loaded.as_ref() else { return Err(EngineError::Disabled { command: Self::ID.into(), reason: Unavailable::NoRepo }) };
        let activity = &loaded.activity;
        Ok(gates(activity, &tickets(activity), &loaded.tracking.merges, Window { start, end }))
    }
}
