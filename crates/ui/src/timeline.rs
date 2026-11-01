//! The timeline pane: swarm-timeline's widget over the engine's rows. When the engine's rows
//! move on (a read landed, the rows were set), the pane loads the engine's timeline into the
//! widget's session, keeping the scale, window and rulers it showed; on the first load it shows
//! the `Time` scale, the whole swarm, under a Clock ruler (the time of day, UTC). The ruler's
//! menu still offers the Time ruler (seconds from the axis's origin).

use swarmception_engine::Session;
use swarmception_engine::factory_cmds::{PlaceAt, PlaceAtParams, TicketBars, TicketParams};
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::{TimeAt, TimeAtParams};
use swarmception_model::Timestamp;
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::ranges_cmds::{Clear as ClearRanges, FocusParams, FocusTrack, InstantParams, RangeParams, SelectInstant, Toggle as ToggleRange};
use timeline_engine::rulers_cmds::{Add as AddRuler, AddParams as AddRulerParams, Set as SetRuler, SetParams as SetRulerParams};
use timeline_engine::state_cmds::{FocusWire, Get, InstantRefWire, Set, SetParams, TimelineState};
use timeline_engine::view_cmds::{Fit, Zoom, ZoomParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire, RulerWire, SpanWire};

use crate::icons;

/// The scale the pane starts on.
const TIME: &str = swarmception_domain::timeline::TIME;

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

/// The widget, and which of the engine's generations of rows it shows.
/// The person's selection on the timeline: what a hover in the factory puts aside while it shows
/// a ticket's bars, and puts back after.
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

pub struct TimelinePane {
    widget: timeline_egui::Timeline,
    loaded: Option<u64>,
    has_data: bool,
    /// The person's selection, put aside while a hovered ticket's bars are selected.
    saved: Option<Selection>,
}

impl Default for TimelinePane {
    fn default() -> Self {
        Self { widget: timeline_egui::Timeline::new(timeline_engine::Session::new()), loaded: None, has_data: false, saved: None }
    }
}

impl TimelinePane {
    /// Whether the widget shows data.
    #[must_use]
    pub const fn has_data(&self) -> bool {
        self.has_data
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
        let origin = Some(OriginWire::Absolute);
        let restored = before.and_then(|(scale, window, rulers)| {
            session.call::<Set>(SetParams { scale: Some(scale), window: Some(window), origin, ..SetParams::default() }).ok().map(|_| rulers)
        });
        match restored {
            Some(rulers) => self.restore_rulers(&rulers),
            None => {
                let _ = session.call::<Set>(SetParams { scale: Some(TIME.to_owned()), origin, ..SetParams::default() });
                let _ = session.call::<Fit>(NoParams {});
                let _ = session.call::<SetRuler>(SetRulerParams { ruler: 0, domain: Some(RulerDomainWire::Clock), origin: None });
            }
        }
        Ok(())
    }

    /// Shows the whole swarm.
    pub fn fit(&mut self) {
        let _ = self.widget.session().call::<Fit>(NoParams {});
    }

    /// Zooms about the window's centre: 2 shows half as much.
    pub fn zoom(&mut self, factor: f64) {
        let _ = self.widget.session().call::<Zoom>(ZoomParams { factor, ..ZoomParams::default() });
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

    /// Shows `issue`'s bars selected while it is hovered in the factory, and puts the person's
    /// own selection back, exactly, once nothing is: the first hover puts it aside, a hover
    /// moving from ticket to ticket keeps it aside (never saving what a hover selected), and
    /// leaving restores it. The bars come from `timeline.ticketBars` and are selected with the
    /// widget's `ranges.toggle`, which leaves the playhead where it is; a bar in a collapsed
    /// group isn't shown, so it isn't selected. (The widget has no highlight that leaves the
    /// selection alone: its overlays are the host's data, not a command's.)
    pub fn light_ticket(&mut self, engine: &mut Session, issue: Option<u64>) {
        if !self.has_data {
            return;
        }
        let Some(issue) = issue else {
            if let Some(saved) = self.saved.take() {
                self.restore(&saved);
            }
            return;
        };
        if self.saved.is_none() {
            self.saved = self.selection();
        }
        let session = self.widget.session();
        let _ = session.call::<ClearRanges>(NoParams {});
        let Ok(found) = engine.call::<TicketBars>(TicketParams { issue }) else { return };
        for bar in found.bars {
            let _ = session.call::<ToggleRange>(RangeParams { track: bar.track, range: bar.range });
        }
    }

    /// Puts `saved` back: its ranges (or its instant: the widget holds one or the other), then its
    /// focus and its playhead, which selecting moved.
    fn restore(&mut self, saved: &Selection) {
        let session = self.widget.session();
        let _ = session.call::<ClearRanges>(NoParams {});
        for (track, ranges) in &saved.ranges {
            for range in ranges {
                let _ = session.call::<ToggleRange>(RangeParams { track: track.clone(), range: *range });
            }
        }
        if let Some(instant) = &saved.instant {
            let _ = session.call::<SelectInstant>(InstantParams { track: instant.track.clone(), instant: instant.instant });
        }
        let focus = saved.focus.as_ref().map_or_else(FocusParams::default, |f| FocusParams { key: Some(f.track.clone()), range: f.range });
        let _ = session.call::<FocusTrack>(focus);
        let playhead = match saved.playhead {
            Some(item) => SetParams { playhead: Some(item), ..SetParams::default() },
            None => SetParams { clear_playhead: true, ..SetParams::default() },
        };
        let _ = session.call::<Set>(playhead);
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
        egui::ScrollArea::vertical().auto_shrink(false).animated(false).show(ui, |ui| {
            ui.add(&mut self.widget);
        });
    }
}
