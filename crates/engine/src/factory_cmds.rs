//! The factory (#18): the swarm at a moment, as a factory building the app
//! (`swarmception_domain::factory`); the timeline's place for a time, so a front end can put the
//! timeline's playhead where the factory is; a ticket's bars on the timeline, so it can
//! highlight them; and the factory's key moments (#33: `swarmception_domain::moments`), to step
//! through them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::factory::{DEFAULT_RECENT, FactoryScene, journey, keep_run, scene, titled};
use swarmception_domain::moments::{Direction, Moment, MomentKind, Step as Landing, moments, step};
use swarmception_domain::rows::bars_where;
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::session::Session;
use crate::stats_cmds::At as When;

/// A gate run or ready check, by its sub-agent's id and a time in it (its start), to pick one of
/// a rerun's runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunAt {
    pub sub: String,
    pub at: Option<Timestamp>,
}

/// What `factory.at` shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtParams {
    /// The moment, in seconds since the Unix epoch (UTC); the last event's when left out.
    pub t: Option<Timestamp>,
    /// The seconds before `t` whose events count as just happened (1 s to a week; 600 when left
    /// out).
    pub recent: Option<i64>,
    /// A ticket whose journey through the stations by `t` the scene carries.
    pub ticket: Option<u64>,
    /// A gate run or ready check the gate's lists keep though older than their history (#95):
    /// what is selected, so it can be drawn selected; none begun after `t`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunAt>,
}

/// `factory.at`: the factory at a moment: up next, the shelf, the benches with their actors and
/// crates, the dock, the gate and its runs, what has shipped to main, and what just happened.
pub struct At;

impl Command for At {
    const ID: &'static str = "factory.at";
    const LABEL: &'static str = "Factory At";
    const READ_ONLY: bool = true;
    type Params = AtParams;
    type Result = FactoryScene;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: AtParams) -> Result<FactoryScene, EngineError> {
        let Some(loaded) = &session.loaded else { return Err(EngineError::Disabled { command: Self::ID.to_owned(), reason: Unavailable::NoData }) };
        let last = loaded.activity.extent.map_or(Timestamp::default(), |(_, last)| last);
        let t = params.t.unwrap_or(last);
        let mut scene = scene(&loaded.activity, &loaded.tracking.merges, t, params.recent.unwrap_or(DEFAULT_RECENT));
        scene.journey = params.ticket.and_then(|issue| journey(&loaded.activity, t, issue));
        if let Some(run) = &params.run {
            keep_run(&mut scene, &loaded.activity, &run.sub, run.at);
        }
        titled(&mut scene, &session.issues.issues);
        Ok(scene)
    }
}

/// A time to find on the timeline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceAtParams {
    /// Seconds since the Unix epoch (UTC).
    pub time: Timestamp,
}

/// Where a time falls on the timeline's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaceAtResult {
    /// Its position on the `Time` scale: nanoseconds from the axis's origin.
    pub position: i64,
    /// The item it falls in (the last whose time is at or before it); none before the first.
    pub item: Option<u64>,
}

/// `timeline.placeAt`: the position and item of a time, the other way from `timeline.timeAt`.
pub struct PlaceAt;

impl Command for PlaceAt {
    const ID: &'static str = "timeline.placeAt";
    const LABEL: &'static str = "Place At";
    const READ_ONLY: bool = true;
    type Params = PlaceAtParams;
    type Result = PlaceAtResult;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: PlaceAtParams) -> Result<PlaceAtResult, EngineError> {
        let Some(built) = &session.built else { return Ok(PlaceAtResult { position: 0, item: None }) };
        let position = params.time.0.saturating_sub(built.origin.0).saturating_mul(1_000_000_000);
        let after = built.times.partition_point(|t| *t <= params.time);
        let item = after.checked_sub(1).and_then(|i| u64::try_from(i).ok());
        Ok(PlaceAtResult { position, item })
    }
}

/// A ticket.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TicketParams {
    pub issue: u64,
}

/// A bar on the timeline: its row's key and its place among the row's bars (the widget's range).
pub use swarmception_domain::rows::BarRef;

/// A ticket's bars on the timeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TicketBarsResult {
    pub bars: Vec<BarRef>,
}

/// `timeline.ticketBars`: where a ticket's bars are in the rows the timeline shows (its
/// stretches and its merge: the bars whose `ticket` it is), for a front end to select or
/// highlight them.
pub struct TicketBars;

impl Command for TicketBars {
    const ID: &'static str = "timeline.ticketBars";
    const LABEL: &'static str = "Ticket Bars";
    const READ_ONLY: bool = true;
    type Params = TicketParams;
    type Result = TicketBarsResult;

    fn run(session: &mut Session, params: TicketParams) -> Result<TicketBarsResult, EngineError> {
        let bars = bars_where(&session.rows, |_, bar| bar.ticket == Some(params.issue)).into_iter().map(|(found, _)| found).collect();
        Ok(TicketBarsResult { bars })
    }
}

/// The most moments `factory.moments` lists.
pub const MAX_MOMENTS: usize = 4_096;

/// Which key moments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MomentsParams {
    /// From this time on (seconds since the epoch, or `YYYY-MM-DDTHH:MM:SSZ`); the first's when
    /// left out.
    pub start: Option<When>,
    /// Up to this time; the last's when left out.
    pub end: Option<When>,
    /// Only these kinds; every kind when left out or empty.
    #[serde(default)]
    pub kinds: Vec<MomentKind>,
}

/// The key moments in a window, in time order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MomentsResult {
    /// The first [`MAX_MOMENTS`] of them.
    pub moments: Vec<Moment>,
    /// How many there are in the window, all told.
    pub total: usize,
}

/// `factory.moments`: the factory's key moments (#33): claims, work beginning, submits, gate runs
/// passing or failing, send backs and merges onto main, each named as a person would say it.
pub struct Moments;

impl Command for Moments {
    const ID: &'static str = "factory.moments";
    const LABEL: &'static str = "Key Moments";
    const READ_ONLY: bool = true;
    type Params = MomentsParams;
    type Result = MomentsResult;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: MomentsParams) -> Result<MomentsResult, EngineError> {
        let start = params.start.as_ref().map(When::time).transpose()?;
        let end = params.end.as_ref().map(When::time).transpose()?;
        if let (Some(start), Some(end)) = (start, end)
            && end < start
        {
            return Err(Refusal::BackwardWindow { start, end }.into());
        }
        let Some(loaded) = &session.loaded else { return Err(EngineError::Disabled { command: Self::ID.to_owned(), reason: Unavailable::NoData }) };
        let within = |m: &Moment| start.is_none_or(|s| s <= m.t) && end.is_none_or(|e| m.t <= e);
        let kinds = &params.kinds;
        let mut all: Vec<Moment> =
            moments(&loaded.activity, &loaded.tracking.merges).into_iter().filter(|m| within(m) && (kinds.is_empty() || kinds.contains(&m.kind))).collect();
        let total = all.len();
        all.truncate(MAX_MOMENTS);
        Ok(MomentsResult { moments: all, total })
    }
}

/// A step from a time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepParams {
    /// From this moment, in seconds since the Unix epoch (UTC); before the first (for `next`) or
    /// after the last (for `previous`) when left out.
    pub t: Option<Timestamp>,
    /// `next` (the default), `previous`, `first` or `last`.
    #[serde(default)]
    pub direction: Direction,
    /// Only these kinds; every kind when left out or empty.
    #[serde(default)]
    pub kinds: Vec<MomentKind>,
}

/// `factory.step`: the factory's next (or previous, first, last) key moment from a time, of the
/// kinds asked for (#33): its time, the moments at that second, and what to call it.
pub struct StepTo;

impl Command for StepTo {
    const ID: &'static str = "factory.step";
    const LABEL: &'static str = "Step to a Key Moment";
    const READ_ONLY: bool = true;
    type Params = StepParams;
    type Result = Landing;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: StepParams) -> Result<Landing, EngineError> {
        let Some(loaded) = &session.loaded else { return Err(EngineError::Disabled { command: Self::ID.to_owned(), reason: Unavailable::NoData }) };
        let all = moments(&loaded.activity, &loaded.tracking.merges);
        Ok(step(&all, &params.kinds, params.t, params.direction))
    }
}
