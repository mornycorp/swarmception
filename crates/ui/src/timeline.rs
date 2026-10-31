//! The timeline pane: swarm-timeline's widget over the engine's rows. When the engine's rows
//! move on (a read landed, the rows were set), the pane loads the engine's timeline into the
//! widget's session, keeping the scale, window and rulers it showed; on the first load it shows
//! the `Time` scale, the whole swarm, under a Clock ruler (the time of day, UTC). The ruler's
//! menu still offers the Time ruler (seconds from the axis's origin).

use swarmception_engine::Session;
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_engine::rows_cmds::{TimeAt, TimeAtParams};
use timeline_engine::engine_cmds::NoParams;
use timeline_engine::rulers_cmds::{Add as AddRuler, AddParams as AddRulerParams, Set as SetRuler, SetParams as SetRulerParams};
use timeline_engine::state_cmds::{Get, Set, SetParams};
use timeline_engine::view_cmds::{Fit, Zoom, ZoomParams};
use timeline_engine::wire::{OriginWire, RulerDomainWire, RulerWire, SpanWire};

use crate::icons;

/// The scale the pane starts on.
const TIME: &str = swarmception_domain::timeline::TIME;

/// The widget, and which of the engine's generations of rows it shows.
pub struct TimelinePane {
    widget: timeline_egui::Timeline,
    loaded: Option<u64>,
    has_data: bool,
}

impl Default for TimelinePane {
    fn default() -> Self {
        Self { widget: timeline_egui::Timeline::new(timeline_engine::Session::new()), loaded: None, has_data: false }
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
