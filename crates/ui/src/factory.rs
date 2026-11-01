//! The factory pane (#18): the swarm at one moment *t* as a factory building the app, drawn from
//! what `factory.at` returns (`swarmception_domain::factory`), stations left to right: up next
//! and the shelf, the workers' benches, the dock, the gate, and main.
//!
//! *t* comes from [`FactoryView`], plain data in the shell's state:
//!
//! - **following the timeline** (the default), *t* is the time under the pointer on the
//!   timeline, else its playhead, else its window's end; while the pointer is over the factory,
//!   *t* holds, so its tooltips can be read;
//! - **playing**, *t* runs on through the timeline's window at [`Speed`], looping, and the
//!   timeline's playhead follows it;
//! - otherwise *t* stays where it was put (`--at`, or where following or play left it).
//!
//! A ticket is the same chip at every station ([`crate::factory_chip`]), gliding and resizing
//! between them when *t* moves, and what happened in the last stretch of *t* glows, fading as it
//! gets older: the pane repaints only while a chip moves or play runs. Hovering a ticket rings it
//! everywhere it is drawn (and the shell selects its bars on the timeline); clicking it traces
//! its journey through the stations (`FactoryView::ticket`), clicking it again or the floor
//! stops.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::factory::FactoryScene;
use swarmception_engine::Session;
use swarmception_engine::factory_cmds::{At, AtParams};
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_model::Timestamp;

use crate::factory_chip::Motion;
use crate::factory_paint::{Drawn, Marks, WallView, paint};
use crate::factory_stats::StatsStrip;
use crate::icons;
use crate::timeline::Looking;

/// How fast play runs: seconds of the swarm a second.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Speed {
    TenSeconds,
    #[default]
    Minute,
    TenMinutes,
    Hour,
}

impl Speed {
    pub const ALL: [Self; 4] = [Self::TenSeconds, Self::Minute, Self::TenMinutes, Self::Hour];

    /// Seconds of the swarm each second.
    #[must_use]
    pub const fn per_second(self) -> i64 {
        match self {
            Self::TenSeconds => 10,
            Self::Minute => 60,
            Self::TenMinutes => 600,
            Self::Hour => 3_600,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TenSeconds => "10 s / s",
            Self::Minute => "1 min / s",
            Self::TenMinutes => "10 min / s",
            Self::Hour => "1 h / s",
        }
    }
}

/// The factory's time, as the shell keeps it: plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FactoryView {
    /// *t* follows the timeline: the pointer over it, else its playhead, else its window's end.
    pub follow: bool,
    /// *t* runs on through the timeline's window, looping; the timeline's playhead follows.
    pub playing: bool,
    pub speed: Speed,
    /// The factory's own moment; none for the last event.
    pub t: Option<Timestamp>,
    /// The ticket whose journey through the stations is traced.
    pub ticket: Option<u64>,
}

impl Default for FactoryView {
    fn default() -> Self {
        Self { follow: true, playing: false, speed: Speed::default(), t: None, ticket: None }
    }
}

/// Where this frame's *t* came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    /// The pointer over the timeline.
    Pointer,
    Playhead,
    /// The timeline window's end.
    WindowEnd,
    Playing,
    /// Held while the pointer is over the factory.
    Held,
    /// Put there (`--at`, or where following or play left it).
    #[default]
    Set,
    /// No *t*: the last event.
    Last,
}

impl Source {
    const fn text(self) -> &'static str {
        match self {
            Self::Pointer => "following the pointer on the timeline",
            Self::Playhead => "following the timeline's playhead",
            Self::WindowEnd => "following the timeline window's end",
            Self::Playing => "playing through the timeline's window",
            Self::Held => "held while the pointer is over the factory",
            Self::Set => "set here",
            Self::Last => "at the last event",
        }
    }
}

/// The scene the pane shows, and how it moves.
#[derive(Default)]
pub struct FactoryPane {
    scene: Option<FactoryScene>,
    /// What the scene was asked for: *t*, `recent`, the traced ticket, and the engine's revision.
    asked: Option<Asked>,
    error: Option<String>,
    source: Source,
    /// Play's position in fractional seconds, between frames.
    play_at: f64,
    /// The rectangle the floor was drawn in last, to tell the pointer is over it.
    rect: Option<egui::Rect>,
    motion: Motion,
    wall: WallView,
    drawn: Drawn,
    /// The ticket under the pointer last frame.
    hovered: Option<u64>,
    /// The statistics strip in the header, for the timeline's window.
    strip: StatsStrip,
}

/// What a scene was asked for: *t*, `recent`, the traced ticket, and the engine's revision.
type Asked = (Option<Timestamp>, i64, Option<u64>, (u64, u64));

/// What a play step does.
struct Play {
    dt: f64,
    window: Option<(Timestamp, Timestamp)>,
}

impl FactoryPane {
    /// The scene it shows: what `factory.at` returned for its *t*.
    #[must_use]
    pub const fn scene(&self) -> Option<&FactoryScene> {
        self.scene.as_ref()
    }

    /// What it drew last frame, station by station.
    #[must_use]
    pub const fn drawn(&self) -> &Drawn {
        &self.drawn
    }

    /// The statistics strip in the header.
    #[must_use]
    pub const fn strip(&self) -> &StatsStrip {
        &self.strip
    }

    /// Asks `stats.get` for the timeline's visible window (the whole run without one) when the
    /// window or the engine's data moved on.
    pub fn sync_stats(&mut self, engine: &mut Session, looking: Option<Looking>) {
        self.strip.sync(engine, looking.and_then(|l| l.window));
    }

    /// The ticket under the pointer, at any station.
    #[must_use]
    pub const fn hovered(&self) -> Option<u64> {
        self.hovered
    }

    /// Where this frame's *t* came from.
    #[must_use]
    pub const fn source(&self) -> Source {
        self.source
    }

    /// Whether the pointer is over the floor drawn last frame.
    fn pointer_over(&self, ctx: &egui::Context) -> bool {
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        self.rect.zip(pointer).is_some_and(|(rect, at)| rect.contains(at))
    }

    /// Moves `view.t` for this frame, from play, the timeline or neither: the time the timeline's
    /// playhead should be put at, while playing.
    pub fn step(&mut self, ctx: &egui::Context, view: &mut FactoryView, looking: Option<Looking>) -> Option<Timestamp> {
        if view.playing {
            let dt = f64::from(ctx.input(|i| i.stable_dt).clamp(0.0, 0.1));
            let t = self.advance(view, Play { dt, window: looking.and_then(|l| l.window) });
            ctx.request_repaint();
            self.source = Source::Playing;
            return Some(t);
        }
        if view.follow && self.pointer_over(ctx) {
            self.source = Source::Held;
        } else if let Some((t, source)) = view.follow.then(|| looking.and_then(followed)).flatten() {
            view.t = Some(t);
            self.source = source;
        } else {
            self.source = if view.t.is_some() { Source::Set } else { Source::Last };
        }
        None
    }

    /// Play's next *t*: on by `dt` at the view's speed through the window (else the swarm's
    /// first to last event), back to its start after its end.
    fn advance(&mut self, view: &mut FactoryView, play: Play) -> Timestamp {
        let scene = self.scene.as_ref();
        let whole = scene.and_then(|s| s.first.zip(s.last));
        let (start, end) = play.window.or(whole).unwrap_or_default();
        let (start, end) = (start.0 as f64, end.0.max(start.0) as f64);
        let at = view.t.map_or(start, |t| t.0 as f64);
        if (self.play_at.floor() - at).abs() >= 1.0 {
            self.play_at = at;
        }
        let mut next = self.play_at + play.dt * view.speed.per_second() as f64;
        if !(start..=end).contains(&next) {
            next = start;
        }
        self.play_at = next;
        let t = Timestamp(next.floor() as i64);
        view.t = Some(t);
        t
    }

    /// Asks `factory.at` again when *t*, `recent` or the engine moved on.
    pub fn sync(&mut self, engine: &mut Session, view: &FactoryView, recent: i64) {
        let (t, ticket) = (view.t, view.ticket);
        let asked = Some((t, recent, ticket, engine.revision()));
        if self.asked == asked {
            return;
        }
        self.asked = asked;
        match engine.call::<At>(AtParams { t, recent: Some(recent), ticket }) {
            Ok(scene) => {
                self.scene = Some(scene);
                self.error = None;
            }
            Err(error) => {
                self.scene = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// Draws the pane: its header (the clock, play, speed, follow) and the floor.
    pub fn ui(&mut self, ui: &mut egui::Ui, view: &mut FactoryView, status: &StatusResult) {
        ui.horizontal(|ui| self.header(ui, view));
        if let Some(t) = self.scene.as_ref().map(|s| s.t) {
            self.strip.ui(ui, t);
        }
        ui.separator();
        let Some(scene) = &self.scene else {
            let text = match (&status.repo, &status.job) {
                (None, _) => "No repository is open: start with --repo <path>.".to_owned(),
                (_, Some(_)) => format!("{}  Reading the tracking branch…", icons::SPINNER),
                _ => self.error.clone().unwrap_or_else(|| "The tracking branch holds no events yet.".to_owned()),
            };
            ui.centered_and_justified(|ui| ui.label(text));
            self.rect = None;
            return;
        };
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        self.rect = Some(rect);
        let marks = Marks { hovered: self.hovered, traced: view.ticket };
        self.drawn = paint(ui, rect, scene, (&mut self.motion, &mut self.wall), marks);
        if self.drawn.hovered != self.hovered {
            self.hovered = self.drawn.hovered;
            ui.ctx().request_repaint();
        }
        match self.drawn.clicked {
            Some(Some(issue)) => view.ticket = if view.ticket == Some(issue) { None } else { Some(issue) },
            Some(None) => view.ticket = None,
            None => {}
        }
    }

    fn header(&self, ui: &mut egui::Ui, view: &mut FactoryView) {
        ui.label(egui::RichText::new(format!("{}  Factory", icons::FACTORY)).strong());
        ui.separator();
        let clock = self.scene.as_ref().map_or_else(String::new, |s| s.text.replacen('T', " ", 1).replace('Z', " UTC"));
        ui.label(egui::RichText::new(format!("{}  {clock}", icons::CLOCK)).monospace().strong());
        ui.weak(self.source.text());
        ui.separator();
        let play = if view.playing { icons::PAUSE } else { icons::PLAY };
        if ui.button(play).on_hover_text("Play: run t through the timeline's window, looping; the timeline's playhead follows").clicked() {
            view.playing = !view.playing;
        }
        egui::ComboBox::from_id_salt("factory-speed").selected_text(view.speed.label()).width(84.0).show_ui(ui, |ui| {
            for speed in Speed::ALL {
                ui.selectable_value(&mut view.speed, speed, speed.label());
            }
        });
        let scene = self.scene.as_ref();
        if ui.button(icons::START).on_hover_text("To the first event").clicked() {
            view.t = scene.and_then(|s| s.first);
            view.follow = false;
        }
        if ui.button(icons::END).on_hover_text("To the last event").clicked() {
            view.t = scene.and_then(|s| s.last);
            view.follow = false;
        }
        ui.separator();
        ui.toggle_value(&mut view.follow, format!("{}  Follow the timeline", icons::FOLLOW))
            .on_hover_text("t follows the pointer over the timeline, else its playhead, else its window's end");
    }
}

/// The time the timeline is looking at: the pointer, else the playhead, else the window's end.
fn followed(looking: Looking) -> Option<(Timestamp, Source)> {
    looking
        .hover
        .map(|t| (t, Source::Pointer))
        .or_else(|| looking.playhead.map(|t| (t, Source::Playhead)))
        .or_else(|| looking.window.map(|(_, end)| (end, Source::WindowEnd)))
}

/// How many seconds before *t* count as just happened: a fortieth of the timeline's window
/// (a minute to half an hour), or three seconds of play.
#[must_use]
pub fn recent(view: &FactoryView, looking: Option<Looking>) -> i64 {
    if view.playing {
        return view.speed.per_second().saturating_mul(3).max(30);
    }
    looking.and_then(|l| l.window).map_or(swarmception_domain::factory::DEFAULT_RECENT, |(start, end)| (end.0.saturating_sub(start.0) / 40).clamp(60, 1_800))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looking(hover: Option<i64>, playhead: Option<i64>, window: Option<(i64, i64)>) -> Option<Looking> {
        Some(Looking { hover: hover.map(Timestamp), playhead: playhead.map(Timestamp), window: window.map(|(a, b)| (Timestamp(a), Timestamp(b))) })
    }

    #[test]
    fn following_takes_the_pointer_then_the_playhead_then_the_windows_end() {
        let ctx = egui::Context::default();
        let mut pane = FactoryPane::default();
        let mut view = FactoryView::default();
        let mut step = |l| {
            ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
            pane.step(&ctx, &mut view, l);
            (view.t, pane.source)
        };
        assert_eq!(step(looking(Some(5), Some(7), Some((0, 9)))), (Some(Timestamp(5)), Source::Pointer));
        assert_eq!(step(looking(None, Some(7), Some((0, 9)))), (Some(Timestamp(7)), Source::Playhead));
        assert_eq!(step(looking(None, None, Some((0, 9)))), (Some(Timestamp(9)), Source::WindowEnd));
        assert_eq!(step(None), (Some(Timestamp(9)), Source::Set), "nothing to follow: t stays");
    }

    #[test]
    fn play_runs_through_the_window_and_loops() {
        let mut pane = FactoryPane::default();
        let mut view = FactoryView { playing: true, speed: Speed::Minute, t: Some(Timestamp(100)), ..FactoryView::default() };
        let window = Some((Timestamp(100), Timestamp(200)));
        let t = pane.advance(&mut view, Play { dt: 0.5, window });
        assert_eq!(t, Timestamp(130));
        assert_eq!(pane.advance(&mut view, Play { dt: 0.01, window }), Timestamp(130), "fractions add up between frames");
        assert_eq!(pane.advance(&mut view, Play { dt: 1.0, window }), Timestamp(190));
        assert_eq!(pane.advance(&mut view, Play { dt: 1.0, window }), Timestamp(100), "past the end, back to the start");
        view.t = Some(Timestamp(150));
        assert_eq!(pane.advance(&mut view, Play { dt: 0.0, window }), Timestamp(150), "a t set elsewhere is taken up");
    }

    #[test]
    fn recent_is_a_fortieth_of_the_window_or_three_seconds_of_play() {
        let view = FactoryView::default();
        assert_eq!(recent(&view, looking(None, None, Some((0, 4_000)))), 100);
        assert_eq!(recent(&view, looking(None, None, Some((0, 100_000)))), 1_800);
        assert_eq!(recent(&view, looking(None, None, Some((0, 30)))), 60);
        assert_eq!(recent(&view, None), 600);
        assert_eq!(recent(&FactoryView { playing: true, speed: Speed::TenMinutes, ..view }, None), 1_800);
    }

    #[test]
    fn the_view_reads_back_and_defaults() {
        let view = FactoryView { follow: false, playing: true, speed: Speed::Hour, t: Some(Timestamp(42)), ticket: Some(7) };
        let text = serde_json::to_string(&view).unwrap();
        assert_eq!(text, r#"{"follow":false,"playing":true,"speed":"hour","t":42,"ticket":7}"#);
        assert_eq!(serde_json::from_str::<FactoryView>(&text).unwrap(), view);
        assert_eq!(serde_json::from_str::<FactoryView>("{}").unwrap(), FactoryView::default());
        assert!(serde_json::from_str::<FactoryView>(r#"{"speed":"warp"}"#).is_err());
    }
}
