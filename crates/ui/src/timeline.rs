//! The timeline pane: swarm-timeline's widget over the engine's rows. When the engine's rows
//! move on (a read landed, the rows were set), the pane loads the engine's timeline into the
//! widget's session, keeping the window and rulers it showed; on the first load it shows the
//! whole swarm, under a Clock ruler (the time of day, UTC). The ruler's menu still offers the Time
//! ruler (seconds from the axis's origin).
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
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::{TimeAt, TimeAtParams};
use swarmception_model::Timestamp;
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::input_cmds::{ClickModeParams, ClickModeWire, SetClickMode};
use timeline_engine::minimap_cmds::{SetVisibility, VisibilityParams, VisibilityWire};
use timeline_engine::query_cmds::{Bars, BarsParams};
use timeline_engine::toolbar_cmds::{SetToolbar, ToolbarControl, ToolbarSetParams};
use timeline_engine::tracks_cmds::{Layout, LayoutParams};

use timeline_engine::highlight_cmds::HighlightParams;
use timeline_engine::ranges_cmds::{Clear as ClearRanges, RangeParams, Toggle as ToggleRange};
use timeline_engine::rulers_cmds::{Add as AddRuler, AddParams as AddRulerParams, Set as SetRuler, SetParams as SetRulerParams};
use timeline_engine::state_cmds::{FocusWire, Get, InstantRefWire, Set, SetParams, TimelineState};
use timeline_engine::view_cmds::{Fit, Show, ShowParams, Zoom, ZoomParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire, RulerWire, SpanWire};

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

/// Where the timeline is looking, as clock times on the `Time` scale (none on another scale).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Looking {
    /// The time under the pointer.
    pub hover: Option<Timestamp>,
    /// The start of the playhead's item.
    pub playhead: Option<Timestamp>,
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
    /// The bars of what the details pane shows (`details.get`'s `timeline`).
    Details,
}

/// What the widget was told to highlight: the hovered ticket, else the details' bars, on a
/// generation of rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Lighting {
    hovered: Option<u64>,
    details: Vec<BarRef>,
    generation: Option<u64>,
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
    /// Pages chosen from a bar's context menu (the widget's host items), for the shell to open.
    links: Vec<Link>,
    /// Where the widget was drawn last frame.
    rect: Option<egui::Rect>,
}

impl Default for TimelinePane {
    fn default() -> Self {
        let mut session = timeline_engine::Session::new();
        // The session keeps the toolbar's spec across loads: set once.
        let _ = session.call::<SetToolbar>(ToolbarSetParams { show: Some(TOOLBAR.to_vec()), hide: Vec::new() });
        Self { widget: timeline_egui::Timeline::new(session), loaded: None, has_data: false, lit: None, scroll_to: None, links: Vec::new(), rect: None }
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

    /// The pages chosen from bars' context menus since the last call (#17): a bar's Open Issue
    /// and Open PR, the widget's host items read back as [`Link`]s.
    pub fn take_links(&mut self) -> Vec<Link> {
        std::mem::take(&mut self.links)
    }

    /// The widget, for a host to drive.
    pub const fn widget(&mut self) -> &mut timeline_egui::Timeline {
        &mut self.widget
    }

    /// The scale, window and rulers the widget shows.
    fn view(&mut self) -> Option<(String, SpanWire, Vec<RulerWire>)> {
        let state = self.widget.session().call::<Get>(NoParams {}).ok()?.timeline?;
        Some((state.scale, state.window, state.rulers))
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
    pub fn sync(&mut self, engine: &Session) -> Result<(), String> {
        let (generation, _) = engine.revision();
        if self.loaded == Some(generation) {
            return Ok(());
        }
        self.loaded = Some(generation);
        let before = if self.has_data { self.view() } else { None };
        let Some(timeline) = engine.widget_timeline().map_err(|e| e.to_string())? else {
            *self = Self { loaded: self.loaded, ..Self::default() };
            return Ok(());
        };
        let session = self.widget.session();
        session.load(timeline);
        self.has_data = true;
        Self::fix_view(session);
        let restored = before.and_then(|(scale, window, rulers)| {
            let window = (scale == TIME).then_some(window);
            session.call::<Set>(SetParams { window, ..SetParams::default() }).ok().map(|_| (window.is_some(), rulers))
        });
        match restored {
            Some((kept, rulers)) => {
                if !kept {
                    let _ = session.call::<Fit>(NoParams {});
                }
                self.restore_rulers(&rulers);
            }
            None => {
                let _ = session.call::<Fit>(NoParams {});
                let _ = session.call::<SetRuler>(SetRulerParams { ruler: 0, domain: Some(RulerDomainWire::Clock), origin: None });
            }
        }
        Ok(())
    }

    /// What the owner fixed (#35), by command: the `Time` scale, the absolute origin, the minimap
    /// always, a click selecting the nearest bar.
    fn fix_view(session: &mut timeline_engine::Session) {
        let _ = session.call::<Set>(SetParams { scale: Some(TIME.to_owned()), origin: Some(OriginWire::Absolute), ..SetParams::default() });
        let _ = session.call::<SetVisibility>(VisibilityParams { visibility: VisibilityWire::Always });
        let _ = session.call::<SetClickMode>(ClickModeParams { mode: ClickModeWire::Nearest });
    }

    /// Shows the whole swarm.
    pub fn fit(&mut self) {
        let _ = self.widget.session().call::<Fit>(NoParams {});
    }

    /// Zooms about the window's centre: 2 shows half as much. (The widget's toolbar has the zoom
    /// buttons since #35; this is for a host or a test to drive it.)
    pub fn zoom(&mut self, factor: f64) {
        let _ = self.widget.session().call::<Zoom>(ZoomParams { factor, ..ZoomParams::default() });
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
        let session = self.widget.session();
        let _ = session.call::<ClearRanges>(NoParams {});
        let Some(Ok(found)) = issue.map(|issue| engine.call::<TicketBars>(TicketParams { issue })) else { return };
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
        let details = if hovered.is_some() { Vec::new() } else { details.to_vec() };
        let wanted = Lighting { hovered, details, generation: self.loaded.filter(|_| self.has_data) };
        if self.lit.as_ref() == Some(&wanted) {
            return false;
        }
        let bars = match hovered {
            Some(issue) => engine.call::<TicketBars>(TicketParams { issue }).map(|found| found.bars).unwrap_or_default(),
            None => wanted.details.clone(),
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
            Some(Lighting { hovered: Some(ticket), .. }) => Lit::Hovered { ticket: *ticket },
            Some(Lighting { details, .. }) if !details.is_empty() => Lit::Details,
            _ => Lit::Nothing,
        }
    }

    /// Brings `on`'s bars into view when none of them is (#55), never touching the selection:
    /// across, when none is drawn in the window, the window moves to them (`view.show`, kept in
    /// the zoom history): the same width centred on them when they fit in it, else just wide
    /// enough to hold them all; down, on the next frame, the tracks scroll to the first of their
    /// rows when none is on screen. Whether the window moved.
    pub fn reveal(&mut self, engine: &mut Session, on: &OnTimeline) -> bool {
        if !self.has_data || on.bars.is_empty() {
            return false;
        }
        self.scroll_to = Some(on.bars.iter().map(|bar| bar.track.clone()).collect());
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
            return Some(format!("{} scale: event order, idle time cut out", state.scale));
        }
        let mut at = |position: i64| engine.call::<TimeAt>(TimeAtParams { position }).ok().map(|t| t.text.trim_end_matches('Z').to_owned());
        let (start, end) = (at(state.window.start)?, at(state.window.end)?);
        let end = if start.get(..10) == end.get(..10) { end.get(11..).unwrap_or(&end).to_owned() } else { end };
        let mut text = format!("{} → {} UTC", start.replacen('T', " ", 1), end.replacen('T', " ", 1));
        if let Some(hover) = state.hover.and_then(&mut at) {
            text.push_str(&format!("  ·  pointer {}", hover.get(11..).unwrap_or(&hover)));
        }
        Some(text)
    }

    /// Draws the pane: the widget's own toolbar and the widget, or why there is nothing to show.
    pub fn ui(&mut self, ui: &mut egui::Ui, status: &StatusResult) {
        if !self.has_data {
            let text = match (&status.repo, &status.job, &status.error) {
                (None, _, _) => "No repository is open: start with --repo <path>.".to_owned(),
                (_, Some(_), _) => format!("{}  Reading the tracking branch…", icons::SPINNER),
                (_, None, Some(error)) => format!("{}  {error}", icons::WARNING),
                (Some(_), None, None) => "The tracking branch holds no events yet.".to_owned(),
            };
            ui.centered_and_justified(|ui| ui.label(text));
            return;
        }
        self.widget.toolbar(ui);
        let shown = egui::ScrollArea::vertical().auto_shrink(false).animated(false).show(ui, |ui| {
            let rect = ui.add(&mut self.widget).rect;
            let row = self.scroll_to.take().and_then(|tracks| hidden_row(&mut self.widget, rect, ui.clip_rect(), &tracks));
            if let Some(row) = row {
                ui.scroll_to_rect(row, Some(egui::Align::Center));
            }
            rect
        });
        self.rect = Some(shown.inner);
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
