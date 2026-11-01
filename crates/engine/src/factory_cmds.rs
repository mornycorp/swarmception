//! The factory (#18): the swarm at a moment, as a factory building the app
//! (`swarmception_domain::factory`); the timeline's place for a time, so a front end can put the
//! timeline's playhead where the factory is; and a ticket's bars on the timeline, so it can
//! highlight them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::Node;
use swarmception_domain::factory::{DEFAULT_RECENT, FactoryScene, journey, scene};
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::error::{EngineError, Unavailable};
use crate::session::Session;

/// What `factory.at` shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AtParams {
    /// The moment, in seconds since the Unix epoch (UTC); the last event's when left out.
    pub t: Option<Timestamp>,
    /// The seconds before `t` whose events count as just happened (1 s to a week; 600 when left
    /// out).
    pub recent: Option<i64>,
    /// A ticket whose journey through the stations by `t` the scene carries.
    pub ticket: Option<u64>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BarRef {
    pub track: String,
    pub range: usize,
}

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
        let bars = session
            .rows
            .iter()
            .flat_map(Node::leaves)
            .filter_map(|leaf| match leaf {
                Node::Ranges { key, bars, .. } => Some((key, bars)),
                _ => None,
            })
            .flat_map(|(key, bars)| {
                bars.iter().enumerate().filter(|(_, bar)| bar.ticket == Some(params.issue)).map(|(range, _)| BarRef { track: key.clone(), range })
            })
            .collect();
        Ok(TicketBarsResult { bars })
    }
}
