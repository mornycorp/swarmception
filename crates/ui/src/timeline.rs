//! The timeline pane: swarm-timeline's widget over the engine's rows. When the engine's rows
//! move on (a read landed, the rows were set), the pane loads the engine's timeline into the
//! widget's session, keeping the window and rulers it showed; on the first load it shows the
//! whole swarm, under a Clock ruler (the time of day, UTC). The ruler's menu still offers the Time
//! ruler (seconds from the axis's origin).
//!
//! A reload (a live watch's, #54) keeps the person's view: a window at the timeline's end
//! follows the new end (the whole swarm shown stays the whole swarm), any other stays put; the
//! selection and focus are carried to the same bars in the new rows (`timeline.carryBars`), the
//! playhead to the same time, and the highlight (the hovered ticket's, else the details'
//! subject's) is lit again on the new rows ([`TimelinePane::light`]).
//!
//! Collapsed to the window (#74), the pane hides the rows `rows.window` says the window leaves
//! out (the widget's `tracks.set` `hidden`), once the window settles ([`SETTLE`] after it last
//! moved, with no button held): the rows follow a pan and a zoom without flickering while they
//! go on. Nothing is loaded again, so the selection, focus, highlight and the rows' own states
//! stay as they were; a row holding the selection is kept, so it isn't lost.
//!
//! The gate's runs are no rows of it (#117, reversing #84): a run naming a ticket is the
//! ticket's *in the gate* stretch on its bar's strip, and a failure its send-back.
//!
//! Folding quiet time (#86): when on, the pane has the widget fold every stretch where no row has
//! anything for longer than the threshold (the widget's `view.fold`), again after every load and
//! restore; the bands drawn under it line their times up through the folds ([`TimeAxis`], the
//! widget's own mapping from time to x).
//!
//! Each row's lanes are sized over the window (#97, the widget's `view.lanes`): a row is as tall
//! as the most of its bars that overlap in the window, not anywhere in the swarm, packed again
//! once the view holds still after a pan or zoom (the widget's own 250 ms settle), and after
//! every load and restore.
//!
//! What the owner fixed (#35) the pane sets by command on every load, and the widget's toolbar
//! leaves out: the `Time` scale, the absolute origin, the minimap always and a click selecting
//! the nearest bar. The toolbar shows only [`TOOLBAR`]: Fit, Zoom to Selection, Undo Zoom, the
//! zoom in / out / fit buttons and the measurement (Δ).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::details::OnTimeline;
use swarmception_domain::rows::BarRef;
use swarmception_domain::timeline::Link;
use swarmception_engine::Session;

use swarmception_engine::factory_cmds::{PlaceAt, PlaceAtParams, TicketBars, TicketParams};
use swarmception_engine::live_cmds::{CarryBars, CarryParams};
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::{TimeAt, TimeAtParams};
use swarmception_engine::rows_cmds::{WindowParams, WindowRows};
use swarmception_engine::stats_cmds::At;
use swarmception_model::Timestamp;
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::input_cmds::{ClickModeParams, ClickModeWire, SetClickMode};
use timeline_engine::lanes_cmds::{LanesOverWire, LanesParams, SetLanes};
use timeline_engine::minimap_cmds::{SetStrips, SetVisibility, StripsParams, VisibilityParams, VisibilityWire};
use timeline_engine::query_cmds::{Bars, BarsParams};
use timeline_engine::toolbar_cmds::{SetToolbar, ToolbarControl, ToolbarSetParams};
use timeline_engine::tracks_cmds::{Expand, GroupParams, Layout, LayoutParams, SetTrack, SetTrackParams};

use timeline_engine::fold_cmds::{FoldParams, SetFold};
use timeline_engine::highlight_cmds::HighlightParams;
use timeline_engine::ranges_cmds::{Clear as ClearRanges, FocusParams, FocusTrack, RangeParams, Toggle as ToggleRange};
use timeline_engine::rulers_cmds::{Add as AddRuler, AddParams as AddRulerParams, Set as SetRuler, SetParams as SetRulerParams};
use timeline_engine::saved::SavedState;
use timeline_engine::state_cmds::{FocusWire, Get, InstantRefWire, Restore, RestoreParams, Save, Set, SetParams, TimelineState};
use timeline_engine::view_cmds::{Fit, Show, ShowParams, Zoom, ZoomParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire, RulerWire, SpanWire};

use crate::catalog;
use crate::icons;

/// The scale the pane shows, always (#35).
const TIME: &str = swarmception_domain::timeline::TIME;

/// The widget's toolbar controls the pane shows (#35, the owner's): the navigate items, the zoom
/// buttons and the measurement. The scale, origin, minimap and click mode are fixed by command
/// on each load instead; the tracks checklist, the window's fields and the overlays aren't
/// wanted.
pub const TOOLBAR: [ToolbarControl; 7] = [
    ToolbarControl::Fit,
    ToolbarControl::ZoomToSelection,
    ToolbarControl::UndoZoom,
    ToolbarControl::ZoomIn,
    ToolbarControl::ZoomOut,
    ToolbarControl::ZoomFit,
    ToolbarControl::Measurement,
];

/// How long the window holds still, with no button held, before the rows collapsed to it follow
/// (#74): a drag, a wheel's zoom or the zoom buttons' clicks move the rows once, after.
pub const SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// What the rows were last collapsed to (#74): the rows' generation, the window as clock times
/// (none off the `Time` scale) and the rows kept for the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Collapsed {
    generation: Option<u64>,
    window: Option<(Timestamp, Timestamp)>,
    keep: Vec<String>,
}

/// The rows collapsed to the window (#74), as the pane keeps them.
#[derive(Debug, Clone, Default)]
struct WindowRowsState {
    /// The rows the pane hid, by key: never one the person hid themselves.
    hidden: std::collections::BTreeSet<String>,
    /// The rows and groups `rows.window` last left out, top to bottom.
    left_out: Vec<String>,
    /// The window last seen (the widget's positions) and since when it has been there; none
    /// before the first look, or after a jump that settles at once (`ui.set`'s window).
    seen: Option<(SpanWire, std::time::Instant)>,
    /// What they were last collapsed to.
    applied: Option<Collapsed>,
    /// How long the last collapse took: `rows.window` and the widget's `tracks.set`s.
    cost: Option<std::time::Duration>,
}

/// Where the timeline is looking, as clock times on the `Time` scale (none on another scale).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Looking {
    /// The time under the pointer.
    pub hover: Option<Timestamp>,
    /// The start of the playhead's item.
    pub playhead: Option<Timestamp>,
    /// The playhead's item (#85: a time put on the playhead holds while it stays on that item).
    pub playhead_item: Option<u64>,
    /// The window's start and end.
    pub window: Option<(Timestamp, Timestamp)>,
}

/// The person's selection on the timeline, which a hover in the factory never touches: it
/// highlights the hovered ticket's bars instead (#48).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The selected ranges, by row key.
    pub ranges: std::collections::BTreeMap<String, Vec<usize>>,
    pub instant: Option<InstantRefWire>,
    pub focus: Option<FocusWire>,
    pub playhead: Option<u64>,
}

impl Selection {
    fn of(state: TimelineState) -> Self {
        Self { ranges: state.selected_ranges, instant: state.selected_instant, focus: state.focus, playhead: state.playhead }
    }
}

/// What the timeline's highlight is of (#55): one owner, so a hover in the factory wins while it
/// lasts and the details pane's subject has it back on leave.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Lit {
    /// Nothing is highlighted.
    #[default]
    Nothing,
    /// The bars of the ticket hovered in the factory (#48).
    Hovered { ticket: u64 },
    /// The bars of the tickets of a worker (or a model × effort) hovered in the Workers pane (#67).
    HoveredWorker,
    /// The bars of what the details pane shows (`details.get`'s `timeline`).
    Details,
}

/// What a hover asks the timeline to highlight, over the details pane's bars: a ticket hovered in
/// the factory (#48), or a worker's in the Workers pane (#67).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Hover {
    #[default]
    Nothing,
    Ticket(u64),
    /// A worker in the Workers pane (#67), by its instance (a model × effort's label), and the
    /// tickets it claimed, worked on, landed or abandoned in the pane's window.
    Worker {
        key: String,
        tickets: Vec<u64>,
    },
}

impl Hover {
    /// The tickets whose bars it lights.
    fn tickets(&self) -> &[u64] {
        match self {
            Self::Nothing => &[],
            Self::Ticket(issue) => std::slice::from_ref(issue),
            Self::Worker { tickets, .. } => tickets,
        }
    }
}

/// What the widget was told to highlight: the hover's, else the details' bars, on a generation
/// of rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Lighting {
    hovered: Hover,
    details: Vec<BarRef>,
    generation: Option<u64>,
}

/// What a load keeps of the view (#54), read before it.
struct Kept {
    scale: String,
    extent: SpanWire,
    window: SpanWire,
    rulers: Vec<RulerWire>,
    selection: Selection,
    /// The start of the playhead's item, a position on the scale.
    playhead: Option<i64>,
}

/// The window to show after a load whose extent went from `before` to `after`: at the end, it
/// follows the new end at its width (at both ends, it shows the whole new extent); elsewhere it
/// stays. Positions are the `Time` scale's, which a load keeps (nanoseconds from the first
/// event's hour). A window within a hundredth of its width of the end is at the end.
fn follow(window: SpanWire, before: SpanWire, after: SpanWire) -> Option<SpanWire> {
    let width = window.end.checked_sub(window.start)?;
    let slack = (width / 100).max(1);
    let at_end = window.end >= before.end.saturating_sub(slack);
    let at_start = window.start <= before.start.saturating_add(slack);
    let window = match (at_start, at_end) {
        (true, true) => after,
        (false, true) => SpanWire { start: after.end.saturating_sub(width).max(after.start), end: after.end },
        _ => window,
    };
    (window.end > window.start).then_some(window)
}

/// The timeline's view as the app keeps it across a restart (#53): the widget's own saved state
/// (`state.save`: its rulers, label column, rows' states and toggles; its playhead left out, an
/// item's index moving as events come) and the window as clock times, so it comes back where it
/// was however the axis's items moved; or at the end, as wide, when it showed the last event;
/// or the whole swarm, when it showed all of it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimelineView {
    pub widget: Option<SavedState>,
    /// The window's start and end.
    pub window: Option<crate::control::Span>,
    /// The window showed the whole swarm: it shows the whole of it again.
    #[serde(default)]
    pub fitted: bool,
    /// The window ended at the last event: it ends at the last event again, as wide.
    #[serde(default)]
    pub at_end: bool,
    /// The playhead's item's time.
    pub playhead: Option<Timestamp>,
}

pub struct TimelinePane {
    widget: timeline_egui::Timeline,
    loaded: Option<u64>,
    has_data: bool,
    /// What the widget highlights: told again when the hover, the details' bars or the rows'
    /// generation move.
    lit: Option<Lighting>,
    /// Rows to bring into view vertically on the next frame, when none of them is (#55).
    scroll_to: Option<Vec<String>>,
    /// A view to restore once the timeline has data (#53).
    pending: Option<TimelineView>,
    /// Pages chosen from a bar's context menu (the widget's host items), for the shell to open.
    links: Vec<Link>,
    /// Where the widget was drawn last frame.
    rect: Option<egui::Rect>,
    /// What of it was on screen last frame: the scroll area's viewport (#68).
    visible: Option<egui::Rect>,
    /// The widget's toolbar's height last frame, for the pane's least height (#68).
    toolbar_height: f32,
    /// Where its window's start and end were across the screen last frame: its rows' area, right
    /// of the label column (#61: the Gates pane's runs line up under it).
    axis: Option<(f32, f32)>,
    /// How long the last load took, its view put back.
    load_time: Option<std::time::Duration>,
    /// The rows collapsed to the window (#74).
    window_rows: WindowRowsState,
    /// Folding quiet time (#86): the threshold in seconds when on, as the shell wants it.
    fold: Option<i64>,
    /// What the widget was last told to fold: none since a load or a restore, which reset it.
    folded: Option<Option<i64>>,
    /// The time at position 0 of the `Time` scale: the axis's origin, read at each load.
    origin: Option<Timestamp>,
    /// Where times fell across the screen last frame, through the folds (#86).
    time_axis: Option<TimeAxis>,
}

/// Where times fall across the screen, as the timeline drew them last frame (#86): its window's
/// start at `x0` and end at `x1`, through the widget's own mapping, folds and all, so a band drawn
/// under the timeline puts a time under the timeline's x for it.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeAxis {
    x0: f32,
    x1: f32,
    /// The time at position 0.
    origin: Timestamp,
    /// The window's start and end, as clock times.
    start: Timestamp,
    end: Timestamp,
    /// The widget's window across its width, through its folds.
    map: timeline_core::axis::PixelMap,
}

impl TimeAxis {
    /// The axis over the widget's `state` from `x0` to `x1` on screen, position 0 at `origin`;
    /// none for a width that isn't positive.
    #[must_use]
    pub fn new(state: &TimelineState, origin: Timestamp, (x0, x1): (f32, f32)) -> Option<Self> {
        let span = |s: SpanWire| timeline_core::axis::Span::new(s.start, s.end).ok();
        let axis = timeline_core::axis::Axis::new(timeline_core::axis::Domain::Time, span(state.extent)?);
        let window = axis.window(state.window.start, state.window.end).ok()?;
        let folds = timeline_core::fold::Folds::new(state.folds.iter().filter_map(|&f| span(f)), axis.extent());
        let map = timeline_core::axis::PixelMap::folded(window, state.width, &folds).ok()?;
        let at = |position: i64| Timestamp(origin.0.saturating_add(position.div_euclid(NANOS)));
        (x1 > x0).then(|| Self { x0, x1, origin, start: at(state.window.start), end: at(state.window.end), map })
    }

    /// The window's start and end across the screen.
    #[must_use]
    pub const fn ends(&self) -> (f32, f32) {
        (self.x0, self.x1)
    }

    /// The window, as clock times.
    #[must_use]
    pub const fn window(&self) -> (Timestamp, Timestamp) {
        (self.start, self.end)
    }

    /// Where `t` falls across the screen, through the folds: inside a fold's marker for a time
    /// it folded; outside `x0` to `x1` for a time outside the window.
    #[must_use]
    pub fn x(&self, t: Timestamp) -> f32 {
        let position = t.0.saturating_sub(self.origin.0).saturating_mul(NANOS);
        let width = self.map.width();
        let across = if width > 0.0 { (self.x1 - self.x0) / width } else { 0.0 };
        self.x0 + self.map.x(position) * across
    }

    /// Whether the window folds anything.
    #[must_use]
    pub fn folded(&self) -> bool {
        self.map.fold_map().is_some()
    }
}

/// Nanoseconds a second: the `Time` scale's positions.
const NANOS: i64 = 1_000_000_000;

impl Default for TimelinePane {
    fn default() -> Self {
        let mut session = timeline_engine::Session::new();
        // The session keeps the toolbar's spec across loads: set once.
        let _ = session.call::<SetToolbar>(ToolbarSetParams { show: Some(TOOLBAR.to_vec()), hide: Vec::new() });
        Self {
            widget: timeline_egui::Timeline::new(session),
            loaded: None,
            has_data: false,
            pending: None,
            lit: None,
            scroll_to: None,
            links: Vec::new(),
            rect: None,
            visible: None,
            toolbar_height: 0.0,
            axis: None,
            load_time: None,
            window_rows: WindowRowsState::default(),
            fold: None,
            folded: None,
            origin: None,
            time_axis: None,
        }
    }
}

impl TimelinePane {
    /// Whether the widget shows data.
    #[must_use]
    pub const fn has_data(&self) -> bool {
        self.has_data
    }

    /// Where on screen ticket `issue`'s first bar drawn last frame is (a stretch before its
    /// merge), its middle: for a host to put the pointer on it (the app's `--hover-bar`, to see
    /// its tooltip). None when none of its bars is drawn.
    pub fn bar_point(&mut self, engine: &mut Session, issue: u64) -> Option<egui::Pos2> {
        let rect = self.rect.filter(|_| self.has_data)?;
        let found = engine.call::<TicketBars>(TicketParams { issue }).ok()?;
        let session = self.widget.session();
        let layout = session.call::<Layout>(LayoutParams { pane: rect.height(), scroll: 0.0 }).ok()?;
        let merged = swarmception_domain::rows::MERGED_KEY;
        let stretches_first = found.bars.iter().filter(|b| b.track != merged).chain(found.bars.iter().filter(|b| b.track == merged));
        stretches_first.into_iter().find_map(|bar| {
            let row = layout.rows.iter().find(|r| r.key == bar.track)?;
            let drawn = session.call::<Bars>(BarsParams { tracks: Some(vec![bar.track.clone()]), label_char_width: None }).ok()?;
            let range = drawn.tracks.first()?.ranges.iter().find(|r| r.range == bar.range)?;
            let x = rect.left() + layout.label_width + range.x + range.width.min(400.0) / 2.0;
            Some(egui::pos2(x, rect.top() + row.y + row.height / 2.0)).filter(|p| rect.contains(*p))
        })
    }

    /// Where on screen the `n`th fold's marker (from 0, left to right) drawn last frame is
    /// (#86): its middle, a few rows down: for a host to put the pointer on it (the app's
    /// `--hover-fold`, to see its tooltip). None when there is no such fold on screen.
    pub fn fold_point(&mut self, n: usize) -> Option<egui::Pos2> {
        let rect = self.rect.filter(|_| self.has_data)?;
        let session = self.widget.session();
        let layout = session.call::<Layout>(LayoutParams { pane: rect.height(), scroll: 0.0 }).ok()?;
        let folds = session.call::<timeline_engine::fold_cmds::QueryFolds>(NoParams {}).ok()?;
        let fold = folds.folds.get(n)?;
        let x = rect.left() + layout.label_width + f32::midpoint(fold.x0, fold.x1);
        let row = layout.rows.get(4).or_else(|| layout.rows.last())?;
        Some(egui::pos2(x, rect.top() + row.y + row.height / 2.0)).filter(|p| rect.contains(*p))
    }

    /// Where the window's start and end were across the screen last frame (its rows' area, right of
    /// the label column); none before the pane has drawn data.
    #[must_use]
    pub fn axis(&self) -> Option<(f32, f32)> {
        self.axis.filter(|_| self.has_data)
    }

    /// Where times fell across the screen last frame, through the folds (#86): what a band drawn
    /// under the timeline lines its times up by. None before the pane has drawn data, or on a
    /// scale other than `Time`.
    #[must_use]
    pub fn time_axis(&self) -> Option<&TimeAxis> {
        self.time_axis.as_ref().filter(|_| self.has_data)
    }

    /// Folds the timeline's quiet time (#86): `Some(threshold)` in seconds folds every stretch
    /// where no row has anything for longer than it, `None` unfolds. Told to the widget now when
    /// it has data, and again after every load or restore.
    pub fn fold_quiet(&mut self, threshold: Option<i64>) {
        self.fold = threshold;
        self.apply_fold();
    }

    /// What the pane folds: the threshold in seconds when on.
    #[must_use]
    pub const fn folding(&self) -> Option<i64> {
        self.fold
    }

    /// Tells the widget what to fold, when it has data and was told otherwise (or nothing since
    /// a load or a restore).
    fn apply_fold(&mut self) {
        if !self.has_data || self.folded == Some(self.fold) {
            return;
        }
        let params = match self.fold {
            Some(seconds) => FoldParams {
                on: Some(true),
                threshold: Some(u64::try_from(seconds.max(1)).unwrap_or(1).saturating_mul(NANOS.unsigned_abs())),
                ranges: Some(Vec::new()),
            },
            None => FoldParams { on: Some(false), ..FoldParams::default() },
        };
        if self.widget.session().call::<SetFold>(params).is_ok() {
            self.folded = Some(self.fold);
        }
    }

    /// The stretches the timeline folds now, as clock times (#86): none while off.
    pub fn folds(&mut self) -> Vec<(Timestamp, Timestamp)> {
        let Some(origin) = self.origin.filter(|_| self.has_data) else { return Vec::new() };
        let Some(state) = self.widget.session().call::<Get>(NoParams {}).ok().and_then(|s| s.timeline) else { return Vec::new() };
        let at = |position: i64| Timestamp(origin.0.saturating_add(position.div_euclid(NANOS)));
        state.folds.iter().map(|f| (at(f.start), at(f.end))).collect()
    }

    /// The window the rows' lanes were last packed over (#97), as clock times: they are sized over
    /// the window, packed again once it holds still. None before data, or with them over the
    /// whole swarm.
    pub fn lanes_window(&mut self) -> Option<(Timestamp, Timestamp)> {
        let origin = self.origin.filter(|_| self.has_data)?;
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        let window = state.lanes_window.filter(|_| state.lanes_over == LanesOverWire::Window)?;
        let at = |position: i64| Timestamp(origin.0.saturating_add(position.div_euclid(NANOS)));
        Some((at(window.start), at(window.end)))
    }

    /// The pages chosen from bars' context menus since the last call (#17): a bar's Open Issue
    /// and Open PR, the widget's host items read back as [`Link`]s.
    pub fn take_links(&mut self) -> Vec<Link> {
        std::mem::take(&mut self.links)
    }

    /// The widget, for a host to drive.
    pub const fn widget(&mut self) -> &mut timeline_egui::Timeline {
        &mut self.widget
    }

    /// What a load keeps of the view: the scale, extent, window and rulers, the selection and
    /// focus, and where the playhead is.
    fn view(&mut self) -> Option<Kept> {
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        Some(Kept {
            playhead: state.playhead_span.map(|s| s.start),
            scale: state.scale.clone(),
            extent: state.extent,
            window: state.window,
            rulers: state.rulers.clone(),
            selection: Selection::of(state),
        })
    }

    /// How long the last load into the widget took, and its view put back (#54's cost).
    #[must_use]
    pub const fn load_time(&self) -> Option<std::time::Duration> {
        self.load_time
    }

    /// Puts back the ruler stack a load reset: the scrub ruler's domain, then the rulers under it.
    fn restore_rulers(&mut self, rulers: &[RulerWire]) {
        let session = self.widget.session();
        for (place, ruler) in rulers.iter().enumerate() {
            let _ = if place == 0 {
                session.call::<SetRuler>(SetRulerParams { ruler: 0, domain: Some(ruler.domain), origin: None }).map(drop)
            } else {
                session.call::<AddRuler>(AddRulerParams { domain: ruler.domain, origin: Some(ruler.origin) }).map(drop)
            };
        }
    }

    /// Loads the engine's timeline when its rows moved on.
    ///
    /// # Errors
    ///
    /// The engine's refusal of the rows, as text to show.
    pub fn sync(&mut self, engine: &mut Session) -> Result<(), String> {
        let (generation, _) = engine.revision();
        if self.loaded == Some(generation) {
            return Ok(());
        }
        let started = std::time::Instant::now();
        let previous = self.loaded.replace(generation);
        let before = if self.has_data { self.view() } else { None };
        let Some(timeline) = engine.widget_timeline().map_err(|e| e.to_string())? else {
            *self = Self { loaded: self.loaded, pending: self.pending.take(), fold: self.fold, ..Self::default() };
            return Ok(());
        };
        self.origin = engine.call::<TimeAt>(TimeAtParams { position: 0 }).ok().map(|t| t.time);
        let session = self.widget.session();
        session.load(timeline);
        self.has_data = true;
        Self::fix_view(session);
        // A load folds nothing: told again (#86).
        self.folded = None;
        self.apply_fold();
        let session = self.widget.session();
        match before {
            Some(kept) => {
                self.restore_window(&kept);
                self.restore_rulers(&kept.rulers);
                self.restore_selection(engine, &kept, previous);
            }
            None => {
                let _ = session.call::<Fit>(NoParams {});
                let _ = session.call::<SetRuler>(SetRulerParams { ruler: 0, domain: Some(RulerDomainWire::Clock), origin: None });
            }
        }
        Self::pack_lanes(self.widget.session());
        self.load_time = Some(started.elapsed());
        Ok(())
    }

    /// The window after a load: on the `Time` scale, one at the timeline's end follows the new
    /// end at its width (the whole swarm shown stays the whole swarm), any other stays where it
    /// was; on another scale, or one the new extent can't hold, the whole swarm.
    fn restore_window(&mut self, kept: &Kept) {
        let session = self.widget.session();
        let Some(extent) = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline).map(|s| s.extent) else { return };
        let window = (kept.scale == TIME).then(|| follow(kept.window, kept.extent, extent)).flatten();
        if window.is_none_or(|window| session.call::<Set>(SetParams { window: Some(window), ..SetParams::default() }).is_err()) {
            let _ = session.call::<Fit>(NoParams {});
        }
    }

    /// The selection and focus carried to the same bars in the new rows, and the playhead to the
    /// same time.
    fn restore_selection(&mut self, engine: &mut Session, kept: &Kept, previous: Option<u64>) {
        let mut carry = |bars: Vec<BarRef>| match previous {
            Some(generation) if !bars.is_empty() => engine.call::<CarryBars>(CarryParams { generation, bars }).map(|c| c.bars).unwrap_or_default(),
            _ => Vec::new(),
        };
        let selected =
            carry(kept.selection.ranges.iter().flat_map(|(track, ranges)| ranges.iter().map(|&range| BarRef { track: track.clone(), range })).collect());
        let focus = kept.selection.focus.as_ref();
        let focused = carry(focus.and_then(|f| f.range.map(|range| BarRef { track: f.track.clone(), range })).into_iter().collect());
        let playhead = kept.playhead.and_then(|position| engine.call::<TimeAt>(TimeAtParams { position }).ok()).map(|t| t.time);
        self.show_rows(selected.iter().map(|bar| bar.track.as_str()));
        let session = self.widget.session();
        for bar in selected {
            let _ = session.call::<ToggleRange>(RangeParams { track: bar.track, range: bar.range });
        }
        let focus_params = match (focused.into_iter().next(), focus) {
            (Some(bar), _) => Some(FocusParams { key: Some(bar.track), range: Some(bar.range) }),
            (None, Some(f)) if f.range.is_none() => Some(FocusParams { key: Some(f.track.clone()), range: None }),
            // Toggling took the focus: put back none.
            (None, None) => Some(FocusParams::default()),
            (None, Some(_)) => None,
        };
        if let Some(params) = focus_params {
            let _ = session.call::<FocusTrack>(params);
        }
        if let Some(t) = playhead {
            self.put_playhead(engine, t);
        }
    }

    /// Restores `view` once the timeline has data ([`Self::restore_pending`]), in place of the
    /// first load's whole swarm.
    pub fn restore_view(&mut self, view: TimelineView) {
        self.pending = Some(view);
    }

    /// Restores the view [`Self::restore_view`] was given, once the timeline has data: the widget's
    /// saved state (`state.restore`), what the owner fixed (#35) again, then the window (the whole
    /// swarm, the end as wide, or its clock times, as much of them as the extent holds) and the
    /// playhead. Whether it restored one.
    pub fn restore_pending(&mut self, engine: &mut Session) -> bool {
        if !self.has_data {
            return false;
        }
        let Some(view) = self.pending.take() else { return false };
        let session = self.widget.session();
        if let Some(mut saved) = view.widget {
            saved.playhead = None;
            let _ = session.call::<Restore>(RestoreParams { saved: Some(saved), state: None });
            // The rows' states are the saved ones now, which hold none of the window's (#74).
            self.window_rows.hidden.clear();
            self.window_rows.applied = None;
        }
        Self::fix_view(session);
        // The saved state's own folding gives way to the pane's (#86).
        self.folded = None;
        self.apply_fold();
        let session = self.widget.session();
        match view.window {
            _ if view.fitted => {
                let _ = session.call::<Fit>(NoParams {});
            }
            Some(span) if view.at_end => {
                let wide = span.end.0.saturating_sub(span.start.0).saturating_mul(1_000_000_000);
                if let Some(extent) = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline).map(|s| s.extent) {
                    let start = extent.end.saturating_sub(wide).max(extent.start);
                    let _ = session.call::<Set>(SetParams { window: Some(SpanWire { start, end: extent.end }), ..SetParams::default() });
                }
            }
            Some(span) => self.show_span(engine, span.start, span.end),
            None => {}
        }
        if let Some(t) = view.playhead {
            self.put_playhead(engine, t);
        }
        Self::pack_lanes(self.widget.session());
        true
    }

    /// The view as the app keeps it ([`TimelineView`]); before the timeline has data, the view
    /// still waiting to be restored, if any.
    pub fn view_state(&mut self, engine: &mut Session) -> Option<TimelineView> {
        if !self.has_data || self.pending.is_some() {
            return self.pending.clone();
        }
        let session = self.widget.session();
        let mut widget = session.call::<Save>(NoParams {}).ok()?;
        widget.playhead = None;
        // The rows the window left out (#74) aren't the person's: they come back with the window.
        for key in &self.window_rows.hidden {
            if let Some(state) = widget.tracks.get_mut(key) {
                state.hidden = false;
                if *state == timeline_engine::track_state::TrackState::default() {
                    widget.tracks.remove(key);
                }
            }
        }
        let state = session.call::<Get>(NoParams {}).ok()?.timeline?;
        let mut at = |position: i64| engine.call::<TimeAt>(TimeAtParams { position }).ok().map(|t| t.time);
        let window = if state.scale == TIME { at(state.window.start).zip(at(state.window.end)) } else { None };
        Some(TimelineView {
            widget: Some(widget),
            window: window.map(|(start, end)| crate::control::Span { start, end }),
            fitted: state.window == state.extent,
            at_end: state.window.end >= state.extent.end,
            playhead: state.playhead_span.and_then(|s| at(s.start)),
        })
    }

    /// What the owner fixed (#35), by command: the `Time` scale, the absolute origin, the minimap
    /// always, drawing the tickets' strips (#134, swarm-timeline#222: where work waited or was
    /// sent back), a click selecting the nearest bar; and each row's lanes sized over the window
    /// (#97), so a row is as tall as what the window shows.
    fn fix_view(session: &mut timeline_engine::Session) {
        let _ = session.call::<Set>(SetParams { scale: Some(TIME.to_owned()), origin: Some(OriginWire::Absolute), ..SetParams::default() });
        let _ = session.call::<SetVisibility>(VisibilityParams { visibility: VisibilityWire::Always });
        let _ = session.call::<SetStrips>(StripsParams { on: true });
        let _ = session.call::<SetClickMode>(ClickModeParams { mode: ClickModeWire::Nearest });
        Self::pack_lanes(session);
    }

    /// The rows' lanes packed over the window now (the widget's `view.lanes`, #97): after a load or
    /// a restore has put the window where it stays. The widget packs them again itself once the
    /// view holds still after a pan or zoom.
    fn pack_lanes(session: &mut timeline_engine::Session) {
        let _ = session.call::<SetLanes>(LanesParams { over: LanesOverWire::Window });
    }

    /// Shows the whole swarm.
    pub fn fit(&mut self) {
        let session = self.widget.session();
        let _ = session.call::<Fit>(NoParams {});
        Self::pack_lanes(session);
    }

    /// Zooms about the window's centre: 2 shows half as much. (The widget's toolbar has the zoom
    /// buttons since #35; this is for a host or a test to drive it.)
    pub fn zoom(&mut self, factor: f64) {
        let session = self.widget.session();
        let _ = session.call::<Zoom>(ZoomParams { factor, ..ZoomParams::default() });
        Self::pack_lanes(session);
    }

    /// Shows `start` to `end` (clock times, `timeline.placeAt`'s positions) in the window, on the
    /// `Time` scale, as much of it as the timeline's extent holds; nothing before the widget has
    /// data, or for a span outside the extent.
    pub fn show_span(&mut self, engine: &mut Session, start: Timestamp, end: Timestamp) {
        if !self.has_data || end <= start {
            return;
        }
        let mut at = |time| engine.call::<PlaceAt>(PlaceAtParams { time }).ok().map(|p| p.position);
        let Some((start, end)) = at(start).zip(at(end)) else { return };
        let session = self.widget.session();
        let Some(state) = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline) else { return };
        if state.scale != TIME {
            let _ = session.call::<Set>(SetParams { scale: Some(TIME.to_owned()), ..SetParams::default() });
        }
        let Some(extent) = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline).map(|s| s.extent) else { return };
        let (start, end) = (start.max(extent.start), end.min(extent.end));
        if end > start {
            let _ = session.call::<Set>(SetParams { window: Some(SpanWire { start, end }), ..SetParams::default() });
            // The host moved the view, not the pointer: the lanes follow at once (#97).
            Self::pack_lanes(session);
        }
    }

    /// Where the timeline is looking: the pointer, the playhead and the window, as clock times;
    /// none before it has data, and nothing in it on a scale other than `Time`.
    pub fn looking(&mut self, engine: &mut Session) -> Option<Looking> {
        if !self.has_data {
            return None;
        }
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        if state.scale != TIME {
            return Some(Looking::default());
        }
        let mut at = |position: i64| engine.call::<TimeAt>(TimeAtParams { position }).ok().map(|t| t.time);
        Some(Looking {
            hover: state.hover.and_then(&mut at),
            playhead: state.playhead_span.and_then(|s| at(s.start)),
            playhead_item: state.playhead,
            window: at(state.window.start).zip(at(state.window.end)),
        })
    }

    /// Puts the playhead on the item `t` falls in (`timeline.placeAt`), or clears it before the
    /// first; nothing when it is there already.
    pub fn put_playhead(&mut self, engine: &mut Session, t: Timestamp) {
        if !self.has_data {
            return;
        }
        let Ok(place) = engine.call::<PlaceAt>(PlaceAtParams { time: t }) else { return };
        let session = self.widget.session();
        let now = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline).and_then(|s| s.playhead);
        if now == place.item {
            return;
        }
        let params = match place.item {
            Some(item) => SetParams { playhead: Some(item), ..SetParams::default() },
            None => SetParams { clear_playhead: true, ..SetParams::default() },
        };
        let _ = session.call::<Set>(params);
    }

    /// The selection on the timeline now: its ranges or instant, its focus and its playhead.
    pub fn selection(&mut self) -> Option<Selection> {
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        Some(Selection::of(state))
    }

    /// The scale the widget shows; none before it has data.
    pub fn scale(&mut self) -> Option<String> {
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        Some(state.scale)
    }

    /// Selects ticket `issue`'s bars (`timeline.ticketBars`), and nothing else; none clears the
    /// selection. What a control channel's `ui.set` with a ticket does.
    pub fn select_ticket(&mut self, engine: &mut Session, issue: Option<u64>) {
        if !self.has_data {
            return;
        }
        let _ = self.widget.session().call::<ClearRanges>(NoParams {});
        let Some(Ok(found)) = issue.map(|issue| engine.call::<TicketBars>(TicketParams { issue })) else { return };
        // Only a shown row's bars can be selected: those the window left out come back (#74).
        self.show_rows(found.bars.iter().map(|bar| bar.track.as_str()));
        let session = self.widget.session();
        for bar in found.bars {
            let _ = session.call::<ToggleRange>(RangeParams { track: bar.track, range: bar.range });
        }
    }

    /// The bar the person chose on the timeline (#47): the focused one when it is a bar, else the
    /// first selected. Their own selection, which a hover in the factory never touches (it
    /// highlights instead, #48). None when no bar is selected.
    pub fn chosen_bar(&mut self) -> Option<(String, usize)> {
        if !self.has_data {
            return None;
        }
        let selection = self.selection()?;
        let focused = selection.focus.as_ref().and_then(|f| f.range.map(|r| (f.track.clone(), r)));
        let selected = |(track, range): &(String, usize)| selection.ranges.get(track).is_some_and(|r| r.contains(range));
        focused.filter(selected).or_else(|| selection.ranges.iter().find_map(|(track, ranges)| ranges.first().map(|r| (track.clone(), *r))))
    }

    /// Highlights ticket `issue`'s bars while it is hovered in the factory, and clears the
    /// highlight once nothing is (#48): [`Self::light`] with no details' bars.
    pub fn light_ticket(&mut self, engine: &mut Session, issue: Option<u64>) -> bool {
        self.light(engine, issue, &[])
    }

    /// The highlight's one owner (#48, #55): the bars of the ticket `hovered` in the factory
    /// while there is one (`timeline.ticketBars`), else `details`, the bars of what the details
    /// pane shows; nothing when neither has any. It is the widget's highlight (`highlight.set`,
    /// drawn as rings apart from the selection), so the person's selection, focus and playhead
    /// are never touched and there is nothing to put back: a hover's leave gives the details'
    /// bars their highlight again. The widget is told only when what is wanted or the rows'
    /// generation moves (a load clears its highlight, so a reload highlights again); whether it
    /// was told, so a host can draw again.
    pub fn light(&mut self, engine: &mut Session, hovered: Option<u64>, details: &[BarRef]) -> bool {
        self.light_hover(engine, hovered.map_or(Hover::Nothing, Hover::Ticket), details)
    }

    /// [`Self::light`] for any [`Hover`]: a worker's lights the bars of every ticket it held
    /// (`timeline.ticketBars` of each), over the details' bars while it lasts (#67).
    pub fn light_hover(&mut self, engine: &mut Session, hovered: Hover, details: &[BarRef]) -> bool {
        let details = if hovered == Hover::Nothing { details.to_vec() } else { Vec::new() };
        let wanted = Lighting { hovered, details, generation: self.loaded.filter(|_| self.has_data) };
        if self.lit.as_ref() == Some(&wanted) {
            return false;
        }
        let bars = match &wanted.hovered {
            Hover::Nothing => wanted.details.clone(),
            hover => hover
                .tickets()
                .iter()
                .flat_map(|&issue| engine.call::<TicketBars>(TicketParams { issue }).map(|found| found.bars).unwrap_or_default())
                .collect(),
        };
        self.lit = Some(wanted);
        if !self.has_data {
            return false;
        }
        let ranges: Vec<RangeParams> = bars.into_iter().map(|bar| RangeParams { track: bar.track, range: bar.range }).collect();
        if ranges.is_empty() || self.widget.set_highlight(HighlightParams { ranges, instants: Vec::new() }).is_err() {
            self.widget.clear_highlight();
        }
        true
    }

    /// What the highlight is of, as the pane last told the widget.
    #[must_use]
    pub fn lit(&self) -> Lit {
        match &self.lit {
            Some(Lighting { hovered: Hover::Ticket(ticket), .. }) => Lit::Hovered { ticket: *ticket },
            Some(Lighting { hovered: Hover::Worker { .. }, .. }) => Lit::HoveredWorker,
            Some(Lighting { details, .. }) if !details.is_empty() => Lit::Details,
            _ => Lit::Nothing,
        }
    }

    /// Brings `on`'s bars into view when none of them is (#55), never touching the selection:
    /// across, when none is drawn in the window, the window moves to them (`view.show`, kept in
    /// the zoom history): the same width centred on them when they fit in it, else just wide
    /// enough to hold them all; down, the groups holding their rows opened (#68), and on the next
    /// frame the tracks scroll to the first of their rows when none is on screen: of their rows
    /// under the merges rows when they have any (#68: a merged ticket is always on the merges
    /// rows at the top, which say nothing of where its own rows are). Whether the window moved.
    pub fn reveal(&mut self, engine: &mut Session, on: &OnTimeline) -> bool {
        if !self.has_data || on.bars.is_empty() {
            return false;
        }
        self.open_groups(&on.groups);
        let top = swarmception_domain::rows::TOP_KEYS;
        let mut rows: Vec<String> = on.bars.iter().map(|bar| bar.track.clone()).filter(|track| !top.contains(&track.as_str())).collect();
        if rows.is_empty() {
            rows = on.bars.iter().map(|bar| bar.track.clone()).collect();
        }
        self.scroll_to = Some(rows);
        let (Some(start), Some(end)) = (on.start, on.end) else { return false };
        if self.drawn_any(&on.bars) {
            return false;
        }
        let mut at = |time| engine.call::<PlaceAt>(PlaceAtParams { time }).ok().map(|p| p.position);
        let Some((start, end)) = at(start).zip(at(end)) else { return false };
        let session = self.widget.session();
        let Some(state) = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline) else { return false };
        if state.scale != TIME {
            return false;
        }
        let width = state.window.end.saturating_sub(state.window.start).max(1);
        let span = end.saturating_sub(start).max(0);
        let (from, to) = if span <= width {
            let from = start.saturating_add(span / 2).saturating_sub(width / 2);
            (from, from.saturating_add(width))
        } else {
            let pad = span / 20;
            (start.saturating_sub(pad), end.saturating_add(pad))
        };
        session.call::<Show>(ShowParams { start: from, end: to }).is_ok()
    }

    /// Opens the groups of `groups` that are collapsed (#68, by worker: a subject's bars in a
    /// collapsed group are drawn only once it opens); the others left as they are.
    fn open_groups(&mut self, groups: &[String]) {
        let session = self.widget.session();
        let Some(collapsed) = session.call::<Get>(NoParams {}).ok().and_then(|s| s.timeline).map(|t| t.collapsed) else { return };
        for key in groups.iter().filter(|key| collapsed.contains(key)) {
            let _ = session.call::<Expand>(GroupParams { key: key.clone() });
        }
    }

    /// The least height the pane keeps (#68, [`crate::layout::timeline_least`]): its toolbar, the
    /// rulers, the merges rows and six rows under them (all when there are fewer), and the
    /// scroller; [`crate::layout::TIMELINE_LEAST_UNMEASURED`] before it has data.
    pub fn least(&mut self) -> f32 {
        if !self.has_data {
            return crate::layout::TIMELINE_LEAST_UNMEASURED;
        }
        let Ok(layout) = self.widget.session().call::<Layout>(LayoutParams { pane: 100_000.0, scroll: 0.0 }) else {
            return crate::layout::TIMELINE_LEAST_UNMEASURED;
        };
        let first = layout.rows.first().map_or(layout.top, |row| row.y);
        let trough = (layout.bands.trough.bottom - layout.bands.trough.top).max(0.0);
        let heights: Vec<f32> = layout.rows.iter().map(|row| row.height).collect();
        crate::layout::timeline_least(self.toolbar_height + first + trough, &heights)
    }

    /// The rows at least partly on screen last frame, top to bottom (`ui.inspect`'s
    /// `timeline.inView`, #68).
    pub fn rows_in_view(&mut self) -> Vec<String> {
        let (Some(rect), Some(visible)) = (self.rect.filter(|_| self.has_data), self.visible) else { return Vec::new() };
        let Ok(layout) = self.widget.session().call::<Layout>(LayoutParams { pane: rect.height(), scroll: 0.0 }) else { return Vec::new() };
        layout
            .rows
            .into_iter()
            .filter(|row| row.y + row.height > visible.top() - rect.top() && row.y < visible.bottom() - rect.top())
            .map(|row| row.key)
            .collect()
    }

    /// Whether any of `bars` is drawn in the window (`query.bars`, whose ranges stand for `count`
    /// bars from `range` where they are packed).
    fn drawn_any(&mut self, bars: &[BarRef]) -> bool {
        let mut tracks: Vec<String> = bars.iter().map(|bar| bar.track.clone()).collect();
        tracks.dedup();
        let Ok(drawn) = self.widget.session().call::<Bars>(BarsParams { tracks: Some(tracks), label_char_width: None }) else { return false };
        bars.iter().any(|bar| {
            drawn
                .tracks
                .iter()
                .filter(|t| t.key == bar.track)
                .flat_map(|t| &t.ranges)
                .any(|r| r.range <= bar.range && bar.range < r.range.saturating_add(r.count.max(1)))
        })
    }

    /// Collapsed to the window (#74), hides the rows `rows.window` says the window leaves out
    /// and shows again those it no longer does, once the window has held still for [`SETTLE`]
    /// with no button `held` (a drag, a wheel's zoom), or at once when the rows' generation or
    /// the selection moved under a still window. A row holding the selection is kept. Collapsed otherwise, the rows the pane hid come back. How long until it should
    /// look again, while a window waits to settle.
    pub fn collapse_to_window(&mut self, engine: &mut Session, now: std::time::Instant, held: bool) -> Option<std::time::Duration> {
        if !self.has_data {
            return None;
        }
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        let rows = &mut self.window_rows;
        let since = match rows.seen {
            Some((window, since)) if window == state.window => since,
            Some(_) => {
                rows.seen = Some((state.window, now));
                now
            }
            // The first look, or a jump: settled at once.
            None => {
                let settled = now.checked_sub(SETTLE).unwrap_or(now);
                rows.seen = Some((state.window, settled));
                settled
            }
        };
        let keep: Vec<String> = state.selected_ranges.iter().filter(|(_, ranges)| !ranges.is_empty()).map(|(key, _)| key.clone()).collect();
        let mut at = |position: i64| engine.call::<TimeAt>(TimeAtParams { position }).ok().map(|t| t.time);
        let window = if state.scale == TIME { at(state.window.start).zip(at(state.window.end)) } else { None };
        let wanted = Collapsed { generation: self.loaded, window, keep };
        if self.window_rows.applied.as_ref() == Some(&wanted) {
            return None;
        }
        let waited = now.saturating_duration_since(since);
        if held || waited < SETTLE {
            return Some(SETTLE.saturating_sub(waited).max(std::time::Duration::from_millis(16)));
        }
        let started = std::time::Instant::now();
        let params = WindowParams {
            start: wanted.window.map(|(start, _)| At::Seconds(start.0)),
            end: wanted.window.map(|(_, end)| At::Seconds(end.0)),
            keep: wanted.keep.clone(),
        };
        let left_out = engine.call::<WindowRows>(params).map(|found| found.left_out).unwrap_or_default();
        self.hide_rows(&state.tracks, &left_out);
        let rows = &mut self.window_rows;
        rows.left_out = left_out;
        rows.applied = Some(wanted);
        rows.cost = Some(started.elapsed());
        None
    }

    /// Hides the rows `left_out` names that the person hasn't hidden themselves (`tracks`, the
    /// widget's rows' states), and shows again those the pane hid that it no longer names.
    fn hide_rows(&mut self, tracks: &std::collections::BTreeMap<String, timeline_engine::track_state::TrackState>, left_out: &[String]) {
        let hidden = &self.window_rows.hidden;
        let wanted: std::collections::BTreeSet<String> =
            left_out.iter().filter(|key| hidden.contains(*key) || !tracks.get(*key).is_some_and(|s| s.hidden)).cloned().collect();
        let session = self.widget.session();
        for key in hidden.difference(&wanted) {
            let _ = session.call::<SetTrack>(SetTrackParams { key: key.clone(), hidden: Some(false), ..SetTrackParams::default() });
        }
        for key in wanted.difference(hidden) {
            let _ = session.call::<SetTrack>(SetTrackParams { key: key.clone(), hidden: Some(true), ..SetTrackParams::default() });
        }
        self.window_rows.hidden = wanted;
    }

    /// Shows again the rows of `tracks` the window left out (#74), and the groups holding them
    /// (a row's key is its groups' keys and its own, `/` between: `w/<machine>/<worker>/<n>`),
    /// so their bars can be selected (the widget selects only a shown row's); the window looks
    /// again on the next frame, keeping them while they hold the selection.
    fn show_rows<'a>(&mut self, tracks: impl Iterator<Item = &'a str>) {
        let tracks: Vec<&str> = tracks.collect();
        let holds =
            |hidden: &String| tracks.iter().any(|track| track.strip_prefix(hidden.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('/')));
        let shown: Vec<String> = self.window_rows.hidden.iter().filter(|key| holds(key)).cloned().collect();
        if shown.is_empty() {
            return;
        }
        let session = self.widget.session();
        for key in shown {
            let _ = session.call::<SetTrack>(SetTrackParams { key: key.clone(), hidden: Some(false), ..SetTrackParams::default() });
            self.window_rows.hidden.remove(&key);
        }
        self.window_rows.applied = None;
    }

    /// Collapses the rows to the window on the next look, without waiting for it to settle:
    /// after a jump (`ui.set`'s window), which no drag goes on from.
    pub fn settle_now(&mut self) {
        self.window_rows.seen = None;
    }

    /// The rows and groups the window leaves out, collapsed to it (#74), by key, top to bottom;
    /// those the pane hides (`ui.inspect`'s `timeline.leftOut`).
    #[must_use]
    pub fn left_out(&self) -> &[String] {
        &self.window_rows.left_out
    }

    /// How long the last collapse to the window took (#74): `rows.window` and hiding the rows.
    #[must_use]
    pub const fn collapse_time(&self) -> Option<std::time::Duration> {
        self.window_rows.cost
    }

    /// What the widget highlights: by row key, the ranges (`state.get`'s `highlight`).
    pub fn highlighted(&mut self) -> std::collections::BTreeMap<String, Vec<usize>> {
        self.widget.highlight().ranges
    }

    /// The clock times the window spans and the pointer is at, on the `Time` scale, in UTC as
    /// the Clock ruler reads them.
    pub fn clock_text(&mut self, engine: &mut Session) -> Option<String> {
        if !self.has_data {
            return None;
        }
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        if state.scale != TIME {
            return Some(catalog::scale_event_order_idle_time(&state.scale));
        }
        let mut at = |position: i64| engine.call::<TimeAt>(TimeAtParams { position }).ok().map(|t| t.text.trim_end_matches('Z').to_owned());
        let (start, end) = (at(state.window.start)?, at(state.window.end)?);
        let end = if start.get(..10) == end.get(..10) { end.get(11..).unwrap_or(&end).to_owned() } else { end };
        let mut text = catalog::utc(&(start.replacen('T', " ", 1)), &(end.replacen('T', " ", 1)));
        if let Some(hover) = state.hover.and_then(&mut at) {
            text.push_str(&catalog::pointer(&(hover.get(11..).unwrap_or(&hover))));
        }
        Some(text)
    }

    /// Draws the pane: the widget's own toolbar and the widget, or why there is nothing to show.
    pub fn ui(&mut self, ui: &mut egui::Ui, status: &StatusResult) {
        if !self.has_data {
            let text = match (&status.repo, &status.job, &status.error) {
                (None, _, _) => catalog::NO_REPOSITORY_IS_OPEN_START_WITH.to_owned(),
                (_, Some(_), _) => catalog::reading_the_tracking_branch_2(&icons::SPINNER),
                (_, None, Some(error)) => format!("{}  {error}", icons::TRIANGLE_EXCLAMATION),
                (Some(_), None, None) => catalog::THE_TRACKING_BRANCH_HOLDS_NO_EVENTS.to_owned(),
            };
            ui.centered_and_justified(|ui| ui.label(icons::text(ui.ctx(), &text)));
            return;
        }
        let top = ui.cursor().top();
        self.widget.toolbar(ui);
        self.toolbar_height = (ui.cursor().top() - top).max(0.0);
        let shown = egui::ScrollArea::vertical().auto_shrink(false).animated(false).show(ui, |ui| {
            let response = ui.add(&mut self.widget);
            let rect = response.rect;
            let clip = ui.clip_rect();
            let row = self.scroll_to.take().and_then(|tracks| hidden_row(&mut self.widget, rect, clip, &tracks));
            if let Some(row) = row {
                ui.scroll_to_rect(row, Some(egui::Align::Center));
            }
            rect
        });
        self.rect = Some(shown.inner);
        self.visible = Some(shown.inner_rect);
        let state = self.widget.session().call::<Get>(NoParams {}).ok().and_then(|s| s.timeline);
        let label_width = state.as_ref().map(|s| s.label_width);
        self.axis = label_width.filter(|w| w.is_finite()).map(|w| (shown.inner.left() + w.clamp(0.0, shown.inner.width()), shown.inner.right()));
        // Through the widget's own mapping, folds and all (#86).
        self.time_axis = state.filter(|s| s.scale == TIME).zip(self.origin).zip(self.axis).and_then(|((s, origin), ends)| TimeAxis::new(&s, origin, ends));
        self.links.extend(self.widget.take_host_choices().iter().filter_map(|id| Link::parse(id)));
    }
}

/// Where the first of `tracks`' rows is, when none of them is on screen (`clip`) in the widget
/// drawn at `rect`; none when one is, or none is laid out.
fn hidden_row(widget: &mut timeline_egui::Timeline, rect: egui::Rect, clip: egui::Rect, tracks: &[String]) -> Option<egui::Rect> {
    let layout = widget.session().call::<Layout>(LayoutParams { pane: rect.height(), scroll: 0.0 }).ok()?;
    let rows: Vec<egui::Rect> = layout
        .rows
        .iter()
        .filter(|row| tracks.contains(&row.key))
        .map(|row| egui::Rect::from_min_size(egui::pos2(rect.left(), rect.top() + row.y), egui::vec2(rect.width(), row.height)))
        .collect();
    (!rows.iter().any(|row| row.intersects(clip))).then(|| rows.first().copied()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn span(start: i64, end: i64) -> SpanWire {
        SpanWire { start, end }
    }

    #[test]
    fn a_window_at_the_end_follows_it_and_any_other_stays() {
        let (before, after) = (span(0, 1_000), span(0, 1_500));
        assert_eq!(follow(span(800, 1_000), before, after), Some(span(1_300, 1_500)), "at the end: the new end, the same width");
        assert_eq!(follow(span(500, 996), before, after), Some(span(1_004, 1_500)), "within a hundredth of its width");
        assert_eq!(follow(span(500, 995), before, after), Some(span(500, 995)), "further: put");
        assert_eq!(follow(span(0, 1_000), before, after), Some(after), "the whole swarm: the whole swarm again");
        assert_eq!(follow(span(200, 600), before, after), Some(span(200, 600)), "elsewhere: put");
        assert_eq!(follow(span(0, 600), before, after), Some(span(0, 600)), "at the start only: put");
        // A window wider than what is left of a shrunk extent starts at its start.
        assert_eq!(follow(span(500, 1_000), before, span(800, 900)), Some(span(800, 900)));
        assert_eq!(follow(span(5, 5), before, after), None, "no width: nothing to keep");
    }
}
