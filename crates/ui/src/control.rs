//! The window's own commands for a control channel (docs/architecture.md › Automation):
//! `ui.commands` lists them with their schemas, and the widget's forwarded ones as
//! `timeline.<id>`; `ui.inspect` reads what the shell shows as data ([`Inspect`]: the panes, the
//! factory's time and what it drew, the timeline's view and selection, the gates pane's runs),
//! `ui.set` sets it
//! ([`UiSet`]), `ui.loaded` answers once the window has loaded ([`Loaded`]) and `ui.screenshot`
//! saves the window as PNG or PPM ([`Screenshot`]); the app answers those two: it owns the
//! frame. [`Shell::answer`] runs them, forwards `timeline.<id>`
//! to the timeline widget (`timeline.view.fit` is its `view.fit`; `timeline.ui.inspect` its own
//! state) and hands every other command to the engine, so a host forwards a request knowing no
//! id.
//!
//! A request runs between frames: what `ui.inspect` says was drawn is the last frame's, and what
//! `ui.set` asks is drawn on the next.

use std::collections::BTreeMap;

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use swarmception_domain::details::Subject;
use swarmception_domain::moments::Direction;
use swarmception_domain::rows::BarRef;
use swarmception_engine::EngineError;
use swarmception_engine::command::parse;
use swarmception_engine::engine_cmds::{CommandWire, CommandsResult, NoParams};
use swarmception_engine::error::{Refusal, Unavailable};
use swarmception_engine::stats_cmds::At;
use swarmception_model::Timestamp;

use crate::factory::{Jump, Source, Speed, Steps};
use crate::timeline::Lit;
use crate::{Panes, Shell, UiState};

/// [`Inspect`]'s shape. 2: where the state is kept (`saved`, #53); 3: the gates pane (#61); 4:
/// how the panes share the window (`layout`) and the timeline's rows in view (#68); 5: the
/// workers pane (#67); 6: opening a repository (`open`, #69); 7: the gates pane's hours, both
/// lanes (`gates.hours`, #75); 8: where the workers' window was drawn (`workers.rect`, #76); 9:
/// the rows collapsed to the window (`timeline.leftOut`, `timeline.collapseUs`, #74); 10: the gate
/// runs as rows of the timeline (#84: `timeline.hoveredRun`; the gates pane's `runs` gone, its
/// `hovered` an hour).
pub const INSPECT_VERSION: u32 = 10;

/// The prefix that forwards a command to the timeline widget: `timeline.<its id>`.
const WIDGET_PREFIX: &str = "timeline.";

/// A clock-time span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: Timestamp,
    pub end: Timestamp,
}

/// What a bench drew.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BenchDrawn {
    /// The bench's worker key.
    pub key: String,
    pub tickets: Vec<u64>,
}

/// What the factory drew last frame, station by station: the ticket numbers at each.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FactoryDrawn {
    pub upcoming: Vec<u64>,
    pub shelf: Vec<u64>,
    pub benches: Vec<BenchDrawn>,
    pub dock: Vec<u64>,
    pub gate: Vec<u64>,
    /// The gate's runs going on, then those before *t*, as their lines read.
    pub runs: Vec<String>,
    /// Main's bricks.
    pub blocks: usize,
    pub done: Vec<u64>,
    /// The crates a station had no room for.
    pub hidden: usize,
    /// The traced ticket's journey, its legs' labels in order.
    pub journey: Vec<String>,
}

/// The factory pane, as `ui.inspect` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FactoryInspect {
    /// The time the scene shows (`factory.at`'s *t*): what the clock reads. None before a scene.
    pub t: Option<Timestamp>,
    /// *t* as `YYYY-MM-DDTHH:MM:SSZ`.
    pub text: Option<String>,
    /// Where *t* came from last frame.
    pub source: Source,
    /// The ticket under the pointer.
    pub hovered: Option<u64>,
    /// The key moment a step landed on, named as the header names it.
    pub moment: Option<String>,
    /// Why there is no scene.
    pub error: Option<String>,
    pub drawn: FactoryDrawn,
}

/// The timeline pane, as `ui.inspect` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimelineInspect {
    pub has_data: bool,
    pub scale: Option<String>,
    /// The window, as clock times (on the `Time` scale).
    pub window: Option<Span>,
    /// The playhead's item's time, and the pointer's.
    pub playhead: Option<Timestamp>,
    pub hover: Option<Timestamp>,
    /// The selected ranges, by row key.
    pub selected: BTreeMap<String, Vec<usize>>,
    /// The ranges the widget highlights, by row key, apart from the selection: the bars of the
    /// ticket hovered in the factory (#48), else those of what the details pane shows (#55).
    #[serde(default)]
    pub highlighted: BTreeMap<String, Vec<usize>>,
    /// What the highlight is of: the hovered ticket, the details' subject, or nothing (#55).
    #[serde(default)]
    pub lit: Lit,
    /// The playhead's item.
    pub playhead_item: Option<u64>,
    /// How long the last load into the widget took, its view put back, in milliseconds (#54).
    #[serde(default)]
    pub load_ms: Option<u64>,
    /// The rows at least partly on screen last frame, top to bottom, by key (#68).
    #[serde(default)]
    pub in_view: Vec<String>,
    /// Collapsed to the window (#74): the rows and groups the settled window leaves out, by key,
    /// top to bottom (`rows.window`'s `leftOut`), which the pane hides.
    #[serde(default)]
    pub left_out: Vec<String>,
    /// How long the last collapse to the window took, `rows.window` and hiding the rows, in
    /// microseconds (#74).
    #[serde(default)]
    pub collapse_us: Option<u64>,
    /// The gate run whose bar on the Gate runs or Ready checks row is under the pointer (#84),
    /// `<sub>@<start>`: its tickets' bars are lit.
    #[serde(default)]
    pub hovered_run: Option<String>,
}

/// `ui.inspect`'s answer: the shell's state and what its panes show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Inspect {
    /// [`INSPECT_VERSION`].
    pub version: u32,
    /// The shell's own state: the panes shown, the factory's follow, play, speed, *t*, traced
    /// ticket and steps.
    pub state: UiState,
    pub repo: Option<String>,
    pub loading: bool,
    /// The last command's error, or the read's, as the status bar shows it.
    pub error: Option<String>,
    pub factory: FactoryInspect,
    pub timeline: TimelineInspect,
    #[serde(default)]
    pub details: DetailsInspect,
    /// Where the state is kept across a restart, and where it was read from (#53).
    #[serde(default)]
    pub saved: crate::saved::SavedInspect,
    /// The gates pane (#61).
    #[serde(default)]
    pub gates: GatesInspect,
    /// How the panes shared the window last frame (#68): the timeline's height and least, each
    /// band's height, what it asked for and whether it is folded, the details' width.
    #[serde(default)]
    pub layout: crate::layout::LayoutDrawn,
    /// The workers pane (#67).
    #[serde(default)]
    pub workers: WorkersInspect,
    /// Opening a repository (#69): a folder refused, the path being typed, Open Recent's items.
    #[serde(default)]
    pub open: crate::OpenInspect,
}

/// The workers pane, as `ui.inspect` reads it (#67): what it drew last frame from
/// `workers.compare`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkersInspect {
    /// The window it drew, as `workers.compare` returned it.
    pub window: Option<Span>,
    /// Each row drawn, in order, its key (a model × effort's label, a worker's instance, `all`)
    /// then its cells, ` | ` between.
    pub rows: Vec<String>,
    /// The row under the pointer, by its key.
    pub hovered: Option<String>,
    /// Why there is nothing to draw: `workers.compare`'s refusal.
    pub error: Option<String>,
    /// Where its window was drawn (#76), in whole points from the app window's top-left: none
    /// when closed. Its sort and its place as asked are the state's `workers`.
    #[serde(default)]
    pub rect: Option<RectDrawn>,
}

/// A rectangle as drawn, in whole points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RectDrawn {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl RectDrawn {
    /// `rect`, each side to the nearest point.
    #[must_use]
    pub fn of(rect: egui::Rect) -> Self {
        let round = |v: f32| v.round() as i32;
        Self { x: round(rect.left()), y: round(rect.top()), width: round(rect.width()), height: round(rect.height()) }
    }
}

/// What `ui.set` sets of the Workers pane (#76), as its window does: each part left out stays as
/// it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkersSet {
    /// Its window shown or closed, as View ▸ Workers and its × do (after `panes`).
    pub open: Option<bool>,
    /// The column it sorts by, as a first click on its header (`name`: as `workers.compare` lists
    /// them), the direction that click gives unless `ascending` says.
    pub sort: Option<crate::WorkersColumn>,
    /// Smallest first, not largest.
    pub ascending: Option<bool>,
    /// The whole run, not the timeline's window.
    pub whole_run: Option<bool>,
    /// Its window's top-left corner, in points from the app window's top-left, each within
    /// ±16384; moved there on the next frame, kept on screen.
    pub place: Option<crate::WorkersPlace>,
    /// Back to its default place, up from the bottom over the bands.
    #[serde(default)]
    pub default_place: bool,
}

/// The gates pane, as `ui.inspect` reads it (#61): what it drew last frame from `gates.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatesInspect {
    /// The window it drew, as `gates.list` returned it. Its runs are the timeline's Gate runs and
    /// Ready checks rows (#84).
    pub window: Option<Span>,
    /// The headline figures, `label value`.
    pub figures: Vec<String>,
    /// The causes table's rows, `cause gate ready`.
    pub causes: Vec<String>,
    /// Each hour's columns drawn (#75), `<start> gate <runs> ready <runs>`.
    #[serde(default)]
    pub hours: Vec<String>,
    /// The hour under the pointer, its start (#84: the runs are the timeline's).
    pub hovered: Option<Timestamp>,
    /// Why there is nothing to draw: `gates.list`'s refusal.
    pub error: Option<String>,
}

/// The details pane, as `ui.inspect` reads it (#55).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetailsInspect {
    /// What it tells the story of (the state's `details.subject`).
    pub subject: Option<Subject>,
    /// Its title, once `details.get` has answered.
    pub title: Option<String>,
    /// Why there are no details: `details.get`'s refusal.
    pub error: Option<String>,
    /// Its bars on the timeline (`details.get`'s `timeline`), which the timeline highlights
    /// while no factory hover has the highlight.
    pub bars: Vec<BarRef>,
}

/// Where `ui.set`'s step puts the factory's *t*, as ⏮ / ⏭, Home and End do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum StepTo {
    /// The next or previous key moment of the kinds `steps` names, or the first or last of them.
    Next,
    Previous,
    First,
    Last,
    /// The first event, or the last.
    FirstEvent,
    LastEvent,
}

/// `ui.set`'s params: each part left out stays as it is. They apply in this order: the rows'
/// collapse; the panes, their folds, the workers pane, speed and steps; `at`; follow and play;
/// the ticket; the step; the details; the window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiSet {
    pub panes: Option<Panes>,
    /// Folds or opens the panes named (#68), as a click on a title's chevron does: `{"gates":
    /// true}` folds the Gates band to its header, `{"details": false}` opens the Details; the
    /// panes left out stay as they are.
    pub folded: Option<crate::layout::Folded>,
    /// The Workers pane (#76): its window open or not and where, its sort, its window of time.
    pub workers: Option<WorkersSet>,
    pub speed: Option<Speed>,
    /// The key moments a step goes through.
    pub steps: Option<Steps>,
    /// The factory's *t* (seconds since the epoch or `YYYY-MM-DDTHH:MM:SSZ`), following and play
    /// turned off and the timeline's playhead put there.
    pub at: Option<At>,
    /// *t* follows the timeline (the pointer, else the playhead, else the window's end).
    pub follow: Option<bool>,
    /// *t* plays through the timeline's window.
    pub playing: Option<bool>,
    /// The ticket selected: its journey traced in the factory and its bars selected on the
    /// timeline.
    pub ticket: Option<u64>,
    /// Clears the traced ticket and the timeline's selection.
    #[serde(default)]
    pub clear_ticket: bool,
    /// A step of *t* to a key moment, on the next frame.
    pub step: Option<StepTo>,
    /// What the details pane shows (#55), as choosing it in the pane does (← goes back): its
    /// bars highlighted on the timeline and brought into view on the next frame, the selection
    /// untouched.
    pub details: Option<Subject>,
    /// The details pane shows nothing.
    #[serde(default)]
    pub clear_details: bool,
    /// The timeline's window, as clock times, as much of it as its extent holds. Collapsed to
    /// the window, the rows follow it on the next frame, not once it settles: it is a jump.
    pub window: Option<SpanAt>,
    /// Which rows Collapse leaves out (#74), as `rows.set` sets it: `off`, `empty` or `window`.
    pub collapse: Option<swarmception_domain::Collapse>,
}

/// A span of times, each in seconds since the epoch or `YYYY-MM-DDTHH:MM:SSZ`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpanAt {
    pub start: At,
    pub end: At,
}

/// `ui.screenshot`'s params: the file the window is saved to, PNG or a binary PPM by its
/// extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Screenshot {
    /// A path ending in `.png` or `.ppm`, in a directory that exists. A client on the same
    /// machine (`swarmception-cli send`, `mcp --control`) makes a relative one its own.
    pub path: String,
}

/// The longest `ui.loaded` waits, in seconds: under the control channel's answer's 20.
pub const LOADED_WITHIN_MOST: u32 = 19;

/// `ui.loaded`'s params: answered with `ui.inspect`'s answer once nothing is loading, the GitHub
/// issues are in and the timeline has its data, or the read has failed, or there is no
/// repository; an error after `within` seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Loaded {
    /// At most [`LOADED_WITHIN_MOST`] seconds; that when left out.
    pub within: Option<u32>,
}

impl Loaded {
    /// How long to wait.
    #[must_use]
    pub fn wait(self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.within.unwrap_or(LOADED_WITHIN_MOST).min(LOADED_WITHIN_MOST)))
    }
}

/// `ui.screenshot`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Saved {
    pub path: String,
    /// In pixels.
    pub width: usize,
    pub height: usize,
}

/// Why the window's answer is an error: the engine's (and a `ui.*` command's bad params), or the
/// widget's for a `timeline.<id>` command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellError {
    Engine(EngineError),
    Widget(timeline_engine::EngineError),
}

impl From<EngineError> for ShellError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

/// The shell's answer to a request: now, a screenshot for the host to take on a later frame, or
/// a wait for the window to load ([`Shell::settled`]) that the host answers with `ui.inspect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Now(Result<String, ShellError>),
    Screenshot(Screenshot),
    Loaded(Loaded),
}

/// The window's own commands, then the widget's forwarded as `timeline.<id>`, with their
/// schemas: what `ui.commands` answers. The widget's registry doesn't say which of its commands
/// change nothing, so none of them is `readOnly` here.
#[must_use]
pub fn ui_commands() -> CommandsResult {
    let wire = |id: &str, label: &str, read_only: bool, params: schemars::Schema, result: schemars::Schema| CommandWire {
        id: id.to_owned(),
        label: label.to_owned(),
        read_only,
        params: params.to_value(),
        result: result.to_value(),
    };
    CommandsResult {
        commands: vec![
            wire("ui.commands", "Window Commands", true, schema_for!(NoParams), schema_for!(CommandsResult)),
            wire("ui.inspect", "Window State", true, schema_for!(NoParams), schema_for!(Inspect)),
            wire("ui.set", "Set Window State", false, schema_for!(UiSet), schema_for!(Inspect)),
            wire("ui.loaded", "Wait Until Loaded", true, schema_for!(Loaded), schema_for!(Inspect)),
            wire("ui.screenshot", "Save a Screenshot", true, schema_for!(Screenshot), schema_for!(Saved)),
        ]
        .into_iter()
        .chain(timeline_engine::commands::all().iter().map(|entry| CommandWire {
            id: format!("{WIDGET_PREFIX}{}", entry.id),
            label: format!("Timeline: {}", entry.label),
            read_only: false,
            params: entry.params_schema().to_value(),
            result: entry.result_schema().to_value(),
        }))
        .collect(),
    }
}

fn reply(result: &impl Serialize) -> Result<String, ShellError> {
    serde_json::to_string(result).map_err(|e| EngineError::BadResult { message: e.to_string() }.into())
}

impl Shell {
    /// The answer to a control request for `command` with `params` (JSON text; empty for none):
    /// the window's own `ui.*` commands, the widget's as `timeline.<id>`, every other the
    /// engine's.
    pub fn answer(&mut self, command: &str, params: &str) -> Answer {
        match command {
            "ui.screenshot" => match parse::<Screenshot>(params).and_then(checked) {
                Ok(shot) => Answer::Screenshot(shot),
                Err(error) => Answer::Now(Err(error.into())),
            },
            "ui.loaded" => match parse::<Loaded>(params) {
                Ok(loaded) => Answer::Loaded(loaded),
                Err(error) => Answer::Now(Err(error.into())),
            },
            _ => Answer::Now(self.answer_now(command, params)),
        }
    }

    /// Whether the window has loaded, as `ui.loaded` waits for: nothing loading, the GitHub
    /// issues in and the timeline's data drawn; or the read failed; or there is no repository.
    pub fn settled(&mut self) -> bool {
        self.sync();
        let failed = self.error.is_some() || self.status.error.is_some();
        !self.loading() && !self.fetching_issues() && (self.timeline.has_data() || failed || self.status.repo.is_none())
    }

    fn answer_now(&mut self, command: &str, params: &str) -> Result<String, ShellError> {
        match command {
            "ui.commands" => {
                parse::<NoParams>(params)?;
                reply(&ui_commands())
            }
            "ui.inspect" => {
                parse::<NoParams>(params)?;
                reply(&self.inspect())
            }
            "ui.set" => {
                let set = parse::<UiSet>(params)?;
                self.apply(&set)?;
                reply(&self.inspect())
            }
            other if other.starts_with("ui.") => Err(EngineError::UnknownCommand { command: other.to_owned() }.into()),
            other => match other.strip_prefix(WIDGET_PREFIX).filter(|rest| rest.contains('.')) {
                Some(widget) => self.timeline.widget().answer(widget, params).map_err(ShellError::Widget),
                // Through the shell, as File ▸ Open… is (#69): the open repository's state saved
                // first, a refusal said in the window.
                None if other == "repo.open" => {
                    let open = parse::<swarmception_engine::repo_cmds::OpenParams>(params)?;
                    reply(&self.open_repo(&open.path)?)
                }
                None => self.engine.execute(other, params).map_err(ShellError::Engine),
            },
        }
    }

    /// What the shell shows now: its state, and what its panes drew last frame.
    pub fn inspect(&mut self) -> Inspect {
        self.sync();
        let looking = self.timeline.looking(&mut self.engine).unwrap_or_default();
        let selection = self.timeline.selection().unwrap_or_default();
        let drawn = self.factory.drawn();
        let scene = self.factory.scene();
        let factory = FactoryInspect {
            t: scene.map(|s| s.t),
            text: scene.map(|s| s.text.clone()),
            source: self.factory.source(),
            hovered: self.factory.hovered(),
            moment: self.factory.moment(&self.ui.factory).map(|step| step.text.clone()),
            error: self.factory.error().map(str::to_owned),
            drawn: FactoryDrawn {
                upcoming: drawn.upcoming.clone(),
                shelf: drawn.shelf.clone(),
                benches: drawn.benches.iter().map(|(key, tickets)| BenchDrawn { key: key.clone(), tickets: tickets.clone() }).collect(),
                dock: drawn.dock.clone(),
                gate: drawn.gate.clone(),
                runs: drawn.runs.clone(),
                blocks: drawn.blocks,
                done: drawn.done.clone(),
                hidden: drawn.hidden,
                journey: drawn.journey.clone(),
            },
        };
        let timeline = TimelineInspect {
            has_data: self.timeline.has_data(),
            scale: self.timeline.scale(),
            window: looking.window.map(|(start, end)| Span { start, end }),
            playhead: looking.playhead,
            hover: looking.hover,
            selected: selection.ranges,
            highlighted: self.timeline.highlighted(),
            lit: self.timeline.lit(),
            playhead_item: selection.playhead,
            load_ms: self.timeline.load_time().map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
            in_view: self.timeline.rows_in_view(),
            left_out: self.timeline.left_out().to_vec(),
            collapse_us: self.timeline.collapse_time().map(|d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX)),
            hovered_run: self.timeline.hovered(&mut self.engine).run(),
        };
        let details = self.details_inspect();
        let gates = self.gates_inspect();
        Inspect {
            version: INSPECT_VERSION,
            state: self.ui.clone(),
            repo: self.status.repo.clone(),
            loading: self.loading(),
            error: self.error.clone().or_else(|| self.status.error.clone()),
            factory,
            timeline,
            details,
            saved: self.saved(),
            gates,
            layout: self.layout().clone(),
            workers: self.workers_inspect(),
            open: self.open_inspect(),
        }
    }

    /// What the workers pane drew last frame (#67): each row its key, then its cells.
    fn workers_inspect(&self) -> WorkersInspect {
        let drawn = self.workers.drawn();
        WorkersInspect {
            window: drawn.window.map(|(start, end)| Span { start, end }),
            rows: drawn.rows.iter().map(|r| std::iter::once(r.key.clone()).chain(r.cells.iter().cloned()).collect::<Vec<_>>().join(" | ")).collect(),
            hovered: drawn.hovered.clone(),
            error: self.workers.error().map(str::to_owned),
            rect: drawn.rect.map(RectDrawn::of),
        }
    }

    /// What the details pane shows (#55).
    fn details_inspect(&self) -> DetailsInspect {
        let shown = self.details.details().filter(|_| self.details.on_timeline(&self.ui.details).is_some());
        DetailsInspect {
            subject: self.ui.details.subject.clone(),
            title: shown.map(|d| d.title.clone()),
            error: self.details.error().map(str::to_owned),
            bars: self.details.on_timeline(&self.ui.details).map(|on| on.bars.clone()).unwrap_or_default(),
        }
    }

    /// What the gates pane drew last frame (#61).
    fn gates_inspect(&self) -> GatesInspect {
        let drawn = self.gates.drawn();
        GatesInspect {
            window: drawn.window.map(|(start, end)| Span { start, end }),
            figures: drawn.figures.clone(),
            causes: drawn.causes.clone(),
            hours: drawn
                .hour_columns
                .chunks(2)
                .filter_map(|pair| match pair {
                    [gate, ready] => Some(format!("{} gate {} ready {}", gate.start, gate.runs, ready.runs)),
                    _ => None,
                })
                .collect(),
            hovered: drawn.hovered,
            error: self.gates.error().map(str::to_owned),
        }
    }

    /// Sets what `set` asks, in [`UiSet`]'s order; a bad time or window changes nothing.
    fn apply(&mut self, set: &UiSet) -> Result<(), EngineError> {
        let at = set.at.as_ref().map(At::time).transpose()?;
        let window = set.window.as_ref().map(|span| self.window(span)).transpose()?;
        if let Some(place) = set.workers.and_then(|w| w.place).filter(|p| !p.sane()) {
            return Err(EngineError::BadParams {
                message: format!("workers.place: ({}, {}) is over {} points from the window's corner", place.x, place.y, crate::WorkersPlace::MOST),
            });
        }
        if let Some(collapse) = set.collapse {
            self.engine.call::<swarmception_engine::rows_cmds::Set>(swarmception_engine::rows_cmds::SetParams { collapse: Some(collapse), grouping: None })?;
        }
        self.sync();
        self.apply_view(set, at);
        self.apply_selection(set);
        if set.clear_details {
            self.ui.details.subject = None;
        }
        if let Some(subject) = &set.details {
            self.select(subject.clone());
        }
        self.details.sync(&mut self.engine, &self.ui.details);
        if let Some((start, end)) = window {
            self.timeline.show_span(&mut self.engine, start, end);
            self.timeline.settle_now();
        }
        Ok(())
    }

    /// `span`'s times, when the timeline can show them.
    fn window(&self, span: &SpanAt) -> Result<(Timestamp, Timestamp), EngineError> {
        let (start, end) = (span.start.time()?, span.end.time()?);
        if end <= start {
            return Err(Refusal::BackwardWindow { start, end }.into());
        }
        if !self.timeline.has_data() {
            return Err(EngineError::Disabled { command: "ui.set".into(), reason: Unavailable::NoData });
        }
        Ok((start, end))
    }

    /// The panes, speed and steps; *t*; follow and play.
    fn apply_view(&mut self, set: &UiSet, at: Option<Timestamp>) {
        if let Some(panes) = set.panes {
            self.ui.panes = panes;
        }
        if let Some(folded) = set.folded {
            self.ui.folded = self.ui.folded.with(folded);
        }
        if let Some(workers) = set.workers {
            self.set_workers(workers);
        }
        if let Some(speed) = set.speed {
            self.ui.factory.speed = speed;
        }
        if let Some(steps) = set.steps {
            self.ui.factory.steps = steps;
        }
        if let Some(t) = at {
            self.set_factory_at(t);
        }
        if let Some(follow) = set.follow {
            self.ui.factory.follow = follow;
        }
        if let Some(playing) = set.playing {
            self.ui.factory.playing = playing;
        }
    }

    /// The Workers pane (#76): open or not, its sort, its window of time, its place.
    const fn set_workers(&mut self, set: WorkersSet) {
        if let Some(open) = set.open {
            self.ui.panes.workers = open;
        }
        let view = &mut self.ui.workers;
        if let Some(column) = set.sort {
            view.sort_by(column);
        }
        if let Some(ascending) = set.ascending {
            view.ascending = ascending;
        }
        if let Some(whole_run) = set.whole_run {
            view.whole_run = whole_run;
        }
        if set.default_place {
            view.place = None;
        }
        if let Some(place) = set.place {
            view.place = Some(place);
        }
    }

    /// The ticket selected, and the step.
    fn apply_selection(&mut self, set: &UiSet) {
        if set.clear_ticket {
            self.ui.factory.ticket = None;
            self.timeline.select_ticket(&mut self.engine, None);
        }
        if let Some(ticket) = set.ticket {
            self.ui.factory.ticket = Some(ticket);
            self.timeline.select_ticket(&mut self.engine, Some(ticket));
        }
        if let Some(step) = set.step {
            self.jump_factory(match step {
                StepTo::Next => Jump::Step(Direction::Next),
                StepTo::Previous => Jump::Step(Direction::Previous),
                StepTo::First => Jump::Step(Direction::First),
                StepTo::Last => Jump::Step(Direction::Last),
                StepTo::FirstEvent => Jump::First,
                StepTo::LastEvent => Jump::Last,
            });
        }
    }
}

/// `shot` if its path is one a screenshot can be saved to.
fn checked(shot: Screenshot) -> Result<Screenshot, EngineError> {
    let path = std::path::Path::new(&shot.path);
    let bad = |why: &str| Err(EngineError::BadParams { message: format!("path: {why}") });
    if crate::shot::Format::of(path).is_none() {
        return bad("a screenshot is a PNG or a binary PPM: the path ends in .png or .ppm");
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| std::path::Path::new("."));
    if !parent.is_dir() {
        return Err(Refusal::NotADirectory { path: parent.display().to_string() }.into());
    }
    Ok(shot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(answer: Answer) -> Result<String, ShellError> {
        match answer {
            Answer::Now(result) => result,
            other => panic!("not now: {other:?}"),
        }
    }

    #[test]
    fn the_windows_commands_list_themselves_with_their_schemas() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let list: CommandsResult = serde_json::from_str(&now(shell.answer("ui.commands", "")).unwrap()).unwrap();
        let ids: Vec<&str> = list.commands.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids[..5], ["ui.commands", "ui.inspect", "ui.set", "ui.loaded", "ui.screenshot"]);
        for (listed, own) in list.commands.iter().zip(ui_commands().commands) {
            assert_eq!((&listed.label, &listed.params, &listed.result), (&own.label, &own.params, &own.result));
        }
        // The widget's commands follow, each as `timeline.<id>` with its schemas, and each runs
        // through the window by the id listed.
        let widget = &list.commands[5..];
        assert_eq!(widget.len(), timeline_engine::commands::all().len());
        for (listed, entry) in widget.iter().zip(timeline_engine::commands::all()) {
            assert_eq!(listed.id, format!("timeline.{}", entry.id));
            assert_eq!((&listed.params, &listed.result), (&entry.params_schema().to_value(), &entry.result_schema().to_value()));
            assert!(!entry.id.contains('_'), "{} has a `_`, which an MCP tool's name can't tell from its `.`", entry.id);
        }
        assert!(widget.iter().any(|c| c.id == "timeline.view.fit"));
        for id in ["timeline.state.get", "timeline.engine.commands"] {
            assert!(widget.iter().any(|c| c.id == id), "{id}");
            assert!(now(shell.answer(id, "")).is_ok(), "{id} runs by the id listed");
        }
    }

    #[test]
    fn loaded_waits_for_the_host_and_a_window_with_no_repository_has_loaded() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        assert_eq!(shell.answer("ui.loaded", ""), Answer::Loaded(Loaded { within: None }));
        assert_eq!(shell.answer("ui.loaded", r#"{"within": 300}"#), Answer::Loaded(Loaded { within: Some(300) }));
        assert_eq!(Loaded { within: Some(300) }.wait(), std::time::Duration::from_secs(19), "capped under the channel's 20 s");
        assert_eq!(Loaded { within: Some(2) }.wait(), std::time::Duration::from_secs(2));
        assert!(matches!(shell.answer("ui.loaded", r#"{"within": "soon"}"#), Answer::Now(Err(ShellError::Engine(EngineError::BadParams { .. })))));
        assert!(shell.settled(), "nothing to load");
    }

    #[test]
    fn bad_requests_are_errors_and_change_nothing() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let before = shell.inspect();
        let bad = |shell: &mut Shell, command: &str, params: &str| now(shell.answer(command, params)).unwrap_err();
        assert!(matches!(bad(&mut shell, "ui.set", r#"{"pains": {}}"#), ShellError::Engine(EngineError::BadParams { .. })));
        assert!(matches!(bad(&mut shell, "ui.set", r#"{"at": "teatime"}"#), ShellError::Engine(EngineError::Invalid { .. })));
        assert!(matches!(bad(&mut shell, "ui.set", r#"{"at": 5, "window": {"start": 9, "end": 3}}"#), ShellError::Engine(EngineError::Invalid { .. })));
        assert!(matches!(bad(&mut shell, "ui.set", r#"{"window": {"start": 1, "end": 3}}"#), ShellError::Engine(EngineError::Disabled { .. })));
        assert!(matches!(bad(&mut shell, "ui.inspect", r#"{"x": 1}"#), ShellError::Engine(EngineError::BadParams { .. })));
        assert!(matches!(bad(&mut shell, "ui.nope", ""), ShellError::Engine(EngineError::UnknownCommand { .. })));
        assert!(matches!(bad(&mut shell, "timeline.view.nope", ""), ShellError::Widget(timeline_engine::EngineError::UnknownCommand { .. })));
        assert!(matches!(bad(&mut shell, "repo.reload", ""), ShellError::Engine(EngineError::Disabled { .. })));
        assert!(matches!(now(shell.answer("ui.screenshot", r#"{"path": "a.jpg"}"#)), Err(ShellError::Engine(EngineError::BadParams { .. }))));
        assert!(matches!(now(shell.answer("ui.screenshot", r#"{"path": "/no/such/dir/a.ppm"}"#)), Err(ShellError::Engine(EngineError::Invalid { .. }))));
        assert_eq!(shell.inspect().state, before.state);
        for name in ["worker18-shot.ppm", "worker24-shot.png"] {
            let path = std::env::temp_dir().join(name).display().to_string();
            assert_eq!(shell.answer("ui.screenshot", &serde_json::json!({ "path": path }).to_string()), Answer::Screenshot(Screenshot { path }));
        }
    }

    #[test]
    fn set_moves_the_state_inspect_reads() {
        let ctx = egui::Context::default();
        let mut shell = Shell::new(&ctx, None);
        let set = r#"{"panes": {"timeline": false, "factory": true}, "at": "2026-10-31T03:42:00Z", "ticket": 7, "speed": "hour", "steps": "merges"}"#;
        let inspect: Inspect = serde_json::from_str(&now(shell.answer("ui.set", set)).unwrap()).unwrap();
        assert_eq!(inspect.version, INSPECT_VERSION);
        assert_eq!(
            inspect.state.panes,
            Panes { timeline: false, factory: true, details: true, gates: true, workers: false },
            "details, gates and workers left out: their defaults"
        );
        assert_eq!(inspect.state.factory.t, Timestamp::parse("2026-10-31T03:42:00Z"));
        assert!(!inspect.state.factory.follow);
        assert_eq!((inspect.state.factory.ticket, inspect.state.factory.speed, inspect.state.factory.steps), (Some(7), Speed::Hour, Steps::Merges));
        assert_eq!(inspect, shell.inspect());
        let cleared: Inspect = serde_json::from_str(&now(shell.answer("ui.set", r#"{"clearTicket": true, "follow": true}"#)).unwrap()).unwrap();
        assert_eq!((cleared.state.factory.ticket, cleared.state.factory.follow), (None, true));
        // The Workers pane (#76): opened, sorted as a header's first click sorts, placed; then the
        // other way, and back to its default place, closed.
        let set = r#"{"workers": {"open": true, "sort": "claimToMerge", "wholeRun": true, "place": {"x": 30, "y": 40}}}"#;
        let workers: Inspect = serde_json::from_str(&now(shell.answer("ui.set", set)).unwrap()).unwrap();
        assert!(workers.state.panes.workers);
        assert_eq!(
            workers.state.workers,
            crate::WorkersView {
                whole_run: true,
                sort: Some(crate::WorkersColumn::ClaimToMerge),
                ascending: true,
                place: Some(crate::WorkersPlace { x: 30, y: 40 })
            }
        );
        let set = r#"{"workers": {"sort": "rate"}}"#;
        let rate: Inspect = serde_json::from_str(&now(shell.answer("ui.set", set)).unwrap()).unwrap();
        assert_eq!((rate.state.workers.sort, rate.state.workers.ascending), (Some(crate::WorkersColumn::Rate), false), "more is better: largest first");
        let set = r#"{"workers": {"ascending": true, "defaultPlace": true, "open": false}}"#;
        let back: Inspect = serde_json::from_str(&now(shell.answer("ui.set", set)).unwrap()).unwrap();
        assert_eq!((back.state.workers.ascending, back.state.workers.place, back.state.panes.workers), (true, None, false));
        let named: Inspect = serde_json::from_str(&now(shell.answer("ui.set", r#"{"workers": {"sort": "name"}}"#)).unwrap()).unwrap();
        assert_eq!(named.state.workers.sort, None, "name: as workers.compare lists them");
        // A place past the bound, or a part it doesn't know, changes nothing.
        let before = shell.inspect().state;
        for bad in [
            r#"{"workers": {"place": {"x": 99999, "y": 0}}, "follow": false}"#,
            r#"{"workers": {"sorted": "rate"}}"#,
            r#"{"workers": {"place": {"x": 1.5, "y": 0}}}"#,
        ] {
            assert!(matches!(now(shell.answer("ui.set", bad)), Err(ShellError::Engine(EngineError::BadParams { .. }))), "{bad}");
        }
        assert_eq!(shell.inspect().state, before);

        // The engine's commands and the widget's go through too.
        assert!(now(shell.answer("engine.commands", "")).unwrap().contains("factory.at"));
        assert!(now(shell.answer("timeline.engine.commands", "")).unwrap().contains("view.fit"));
    }
}
