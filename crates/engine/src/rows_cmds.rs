//! The rows: grouped how (the tickets flat, or by worker), collapsed how; the rows
//! themselves, and those a window leaves out (#74); and the clock time at a position on the
//! timeline's axis; a bar by its place, and a gate run's bar by its id (#84).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::rows::{Bar, left_out};
use swarmception_domain::{Collapse, Grouping, Node, RowsSpec};
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::engine_cmds::NoParams;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::session::Session;
use crate::stats_cmds::At;

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
    /// `off`, `empty` or `window` (#74); `true` and `false` still read, as `empty` and `off`.
    pub collapse: Option<Collapse>,
}

/// `rows.set`: the tickets flat or by worker, collapsed how. Bars always carry their tickets'
/// titles: `titles` is no field of it (#34), so `{"titles":false}` is refused. Collapsed to the
/// window, the rows are those with events, as `empty`'s; `rows.window` says which of them a
/// window leaves out.
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

/// `rows.window`'s params: the window, and the rows it keeps whatever it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowParams {
    /// The window's start and end, each in seconds since the epoch or `YYYY-MM-DDTHH:MM:SSZ`;
    /// with neither, there is no window and nothing is left out.
    pub start: Option<At>,
    pub end: Option<At>,
    /// Rows kept however empty the window leaves them, by key: those holding the selection, so
    /// it isn't lost.
    #[serde(default)]
    pub keep: Vec<String>,
}

/// The rows a window shows and those it leaves out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WindowResult {
    pub collapse: Collapse,
    /// The window asked about, when there is one.
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
    /// Every row and group the window shows, by key, top to bottom (a group before its rows).
    pub shown: Vec<String>,
    /// The rows and groups it leaves out, by key: a group with nothing in the window in place
    /// of its rows. None unless the rows are collapsed to the window.
    pub left_out: Vec<String>,
}

/// `rows.window` (#74): which of the rows (`rows.list`'s) a window from `start` to `end`
/// leaves out, collapsed to the window: a row none of whose bars overlaps it, unless `keep`
/// names it, and a group with no row left. What the timeline pane hides once its window settles,
/// so its rows are `shown`. Any other collapse leaves out nothing.
pub struct WindowRows;

impl Command for WindowRows {
    const ID: &'static str = "rows.window";
    const LABEL: &'static str = "Rows in a Window";
    const READ_ONLY: bool = true;
    type Params = WindowParams;
    type Result = WindowResult;

    fn run(session: &mut Session, params: WindowParams) -> Result<WindowResult, EngineError> {
        let start = params.start.as_ref().map(At::time).transpose()?;
        let end = params.end.as_ref().map(At::time).transpose()?;
        let window = match (start, end) {
            (Some(start), Some(end)) if end < start => return Err(Refusal::BackwardWindow { start, end }.into()),
            (Some(start), Some(end)) => Some((start, end)),
            (None, None) => None,
            _ => return Err(EngineError::BadParams { message: "rows.window takes both start and end, or neither".into() }),
        };
        let keep: Vec<&str> = params.keep.iter().map(String::as_str).collect();
        let left_out = match window {
            Some((start, end)) if session.spec.collapse == Collapse::Window => left_out(&session.rows, start, end, &keep),
            _ => Vec::new(),
        };
        let mut shown = Vec::new();
        shown_keys(&session.rows, &left_out, &mut shown);
        Ok(WindowResult { collapse: session.spec.collapse, start, end, shown, left_out })
    }
}

/// The keys of `rows` that `left_out` doesn't name, top to bottom, a group's before its rows'.
fn shown_keys(rows: &[Node], left_out: &[String], shown: &mut Vec<String>) {
    for node in rows.iter().filter(|node| !left_out.iter().any(|key| key == node.key())) {
        shown.push(node.key().to_owned());
        if let Node::Group { children, .. } = node {
            shown_keys(children, left_out, shown);
        }
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

/// A bar by its row's key and its place among the row's bars (the widget's range).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarParams {
    pub track: String,
    pub range: usize,
}

/// A bar of the rows, as `rows.list` has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BarResult {
    pub bar: Bar,
}

/// `timeline.bar` (#84): the bar `range` of row `track` in the rows the timeline shows, with its
/// label, tooltip, strip and marks: what a front end reads of the bar under the pointer.
pub struct GetBar;

impl Command for GetBar {
    const ID: &'static str = "timeline.bar";
    const LABEL: &'static str = "Bar";
    const READ_ONLY: bool = true;
    type Params = BarParams;
    type Result = BarResult;

    fn run(session: &mut Session, params: BarParams) -> Result<BarResult, EngineError> {
        let found = session.rows.iter().flat_map(Node::leaves).find_map(|leaf| match leaf {
            Node::Ranges { key, bars, .. } if *key == params.track => bars.get(params.range),
            _ => None,
        });
        let what = || swarmception_domain::titles::short(&format!("bar {} of row {}", params.range, params.track), 120);
        found.cloned().map(|bar| BarResult { bar }).ok_or_else(|| Refusal::NotFound { what: what() }.into())
    }
}
