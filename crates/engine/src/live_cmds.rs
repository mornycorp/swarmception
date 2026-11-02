//! Watching a live swarm (#54): `live.get` says how the watch is going, `live.set` turns it on
//! or off and sets its interval, `live.check` checks now; `timeline.carryBars` says where bars of
//! the rows before a reload are in the rows now, so a front end keeps its selection.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::engine_cmds::NoParams;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::factory_cmds::BarRef;
use crate::live::{LiveStatus, MAX_INTERVAL, MIN_INTERVAL};
use crate::repo_cmds::has_repo;
use crate::session::{Job, Session};

fn status(session: &Session) -> LiveStatus {
    session.live.status(Instant::now(), session.job() == Some(Job::Live))
}

/// `live.get`: whether the session watches the swarm, when it checks next, what the last check
/// found and cost, and why it failed.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "live.get";
    const LABEL: &'static str = "Live Status";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = LiveStatus;

    fn run(session: &mut Session, _params: NoParams) -> Result<LiveStatus, EngineError> {
        Ok(status(session))
    }
}

/// What `live.set` sets: each part left out stays as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetParams {
    /// Watch the swarm: fetch `tracking` and `main` on the interval, and read them again when
    /// either moved. Turned on, the first check is at once.
    pub on: Option<bool>,
    /// Seconds between checks, 5 to 3600 (30 by default).
    pub interval_seconds: Option<u64>,
}

/// `live.set`: turns the watch on or off, and sets its interval. The watch runs in a session
/// that reads in the background (the app's); elsewhere `live.check` checks.
pub struct Set;

impl Command for Set {
    const ID: &'static str = "live.set";
    const LABEL: &'static str = "Live";
    type Params = SetParams;
    type Result = LiveStatus;

    fn run(session: &mut Session, params: SetParams) -> Result<LiveStatus, EngineError> {
        if let Some(seconds) = params.interval_seconds.filter(|s| !(MIN_INTERVAL..=MAX_INTERVAL).contains(s)) {
            return Err(Refusal::BadInterval { seconds, min: MIN_INTERVAL, max: MAX_INTERVAL }.into());
        }
        session.live.choose(params.on, params.interval_seconds.map(Duration::from_secs), Instant::now());
        Ok(status(session))
    }
}

/// `live.check`: a live check now, as the watch makes on its interval: fetch `tracking` and
/// `main`, and read them again only when either moved. In the background in the app (it lands
/// on a later frame), now in the CLI; a failure is the status's `error`, the timeline left as
/// it was.
pub struct Check;

impl Command for Check {
    const ID: &'static str = "live.check";
    const LABEL: &'static str = "Check for New Events";
    type Params = NoParams;
    type Result = LiveStatus;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        has_repo(session)
    }

    fn run(session: &mut Session, _params: NoParams) -> Result<LiveStatus, EngineError> {
        if session.job().is_none() {
            session.start(Job::Live)?;
        }
        Ok(status(session))
    }
}

/// `timeline.carryBars`' params: bars of the rows of a generation (`Session::revision`'s first).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CarryParams {
    pub generation: u64,
    pub bars: Vec<BarRef>,
}

/// Where they are now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CarryResult {
    /// The rows' generation now.
    pub generation: u64,
    /// The bars found, in the order asked; one the rows no longer have is left out.
    pub bars: Vec<BarRef>,
}

/// `timeline.carryBars`: where bars of the rows of an earlier generation are in the rows now,
/// each found by its ticket, start and style (on its own row when that is still there), so a
/// front end keeps its selection across a reload (#54) or a regrouping. The session remembers
/// the last eight generations.
pub struct CarryBars;

impl Command for CarryBars {
    const ID: &'static str = "timeline.carryBars";
    const LABEL: &'static str = "Carry Bars";
    const READ_ONLY: bool = true;
    type Params = CarryParams;
    type Result = CarryResult;

    fn run(session: &mut Session, params: CarryParams) -> Result<CarryResult, EngineError> {
        let bars: Vec<(String, usize)> = params.bars.into_iter().map(|b| (b.track, b.range)).collect();
        let found = session.carry(params.generation, &bars)?;
        Ok(CarryResult { generation: session.revision().0, bars: found.into_iter().map(|(track, range)| BarRef { track, range }).collect() })
    }
}
