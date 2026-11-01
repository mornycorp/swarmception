//! The rows: grouped how (the tickets flat, or by worker), collapsed or not; the rows
//! themselves; and the clock time at a position on the timeline's axis.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::{Grouping, Node, RowsSpec};
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::engine_cmds::NoParams;
use crate::error::{EngineError, Unavailable};
use crate::session::Session;

/// `rows.get`: the rows asked for.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "rows.get";
    const LABEL: &'static str = "Rows";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = RowsSpec;

    fn run(session: &mut Session, _params: NoParams) -> Result<RowsSpec, EngineError> {
        Ok(session.spec)
    }
}

/// What `rows.set` changes; what it leaves out stays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetParams {
    pub grouping: Option<Grouping>,
    pub collapse: Option<bool>,
}

/// `rows.set`: the tickets flat or by worker, collapsed or not. Bars always carry their
/// tickets' titles: `titles` is no field of it (#34), so `{"titles":false}` is refused.
pub struct Set;

impl Command for Set {
    const ID: &'static str = "rows.set";
    const LABEL: &'static str = "Set Rows";
    type Params = SetParams;
    type Result = RowsSpec;

    fn run(session: &mut Session, params: SetParams) -> Result<RowsSpec, EngineError> {
        let spec = RowsSpec { grouping: params.grouping.unwrap_or(session.spec.grouping), collapse: params.collapse.unwrap_or(session.spec.collapse) };

        if spec != session.spec {
            session.spec = spec;
            session.rebuild();
        }
        Ok(session.spec)
    }
}

/// The rows, and what they were built for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListResult {
    pub spec: RowsSpec,
    pub rows: Vec<Node>,
}

/// `rows.list`: the rows the timeline shows, each bar and pin with its label and tooltip.
pub struct List;

impl Command for List {
    const ID: &'static str = "rows.list";
    const LABEL: &'static str = "List Rows";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = ListResult;

    fn run(session: &mut Session, _params: NoParams) -> Result<ListResult, EngineError> {
        Ok(ListResult { spec: session.spec, rows: session.rows.clone() })
    }
}

/// A position on the timeline's `Time` scale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeAtParams {
    /// Nanoseconds from the axis's origin.
    pub position: i64,
}

/// The clock time at a position, and the axis's origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeAtResult {
    pub time: Timestamp,
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    pub text: String,
    /// The time at position 0.
    pub origin: Timestamp,
}

/// `timeline.timeAt`: the UTC time at a position on the `Time` scale.
pub struct TimeAt;

impl Command for TimeAt {
    const ID: &'static str = "timeline.timeAt";
    const LABEL: &'static str = "Time At";
    const READ_ONLY: bool = true;
    type Params = TimeAtParams;
    type Result = TimeAtResult;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: TimeAtParams) -> Result<TimeAtResult, EngineError> {
        let origin = session.built.as_ref().map(|b| b.origin).unwrap_or_default();
        let time = session.built.as_ref().map(|b| b.time_at(params.position)).unwrap_or_default();
        Ok(TimeAtResult { time, text: time.to_string(), origin })
    }
}
