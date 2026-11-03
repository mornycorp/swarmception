//! The factory pane (#18): the swarm at one moment *t* as a factory building the app, drawn from
//! what `factory.at` returns (`swarmception_domain::factory`), stations left to right: up next
//! and the shelf, the workers' benches, the dock, the gate, and main.
//!
//! **The timeline is the source of truth** (#85): the factory has no clock of its own. *t* is
//! the timeline's, as [`FactoryView`] says:
//!
//! - **following the timeline** (the default), *t* is its playhead, else its window's end;
//! - **playing**, the timeline's playhead runs on through its window at [`Speed`], looping, and
//!   the factory follows it;
//! - **stepping** (#33), ⏮ / ⏭ (and ← / → while the factory has the keyboard: a click in it
//!   takes it) put the timeline's playhead at the previous / next key moment of the kinds
//!   [`Steps`] names, from `factory.step`: a claim, work beginning, a submit, a gate's pass or
//!   fail, a send back, a merge onto main. The moment is named in the header and the chips glide
//!   to it. Shift-click (or Home / End) goes to the first / last event;
//! - **not following**, *t* stays at the last time it showed, which a small label says.
//!
//! The playhead stands on an item of the timeline's axis (a time a bar names), so a time put on
//! it (a step, play, `--at`, an hour's click) is kept precisely while the playhead stays on the
//! item it was put on; once the playhead moves, *t* is the playhead's.
//!
//! The header's buttons come first, at fixed places, the words after them (#85): its text can
//! change width without moving them ([`HeaderDrawn`]).
//!
//! A ticket is the same chip at every station ([`crate::factory_chip`]), gliding and resizing
//! between them when *t* moves, and what happened in the last stretch of *t* glows, fading as it
//! gets older: the pane repaints only while a chip moves or play runs. Hovering a ticket rings it
//! everywhere it is drawn (and the shell highlights its bars on the timeline, #48); clicking it traces
//! its journey through the stations (`FactoryView::ticket`), clicking it again or the floor
//! stops. What is selected (#85: the details' subject, so a bar chosen on the timeline) is ringed
//! in the selection's colour and brought into view ([`crate::factory_paint::Selected`]).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::factory::FactoryScene;
use swarmception_domain::moments::{Direction, MomentKind, Step};
use swarmception_engine::Session;
use swarmception_engine::factory_cmds::{At, AtParams, PlaceAt, PlaceAtParams, RunAt, StepParams, StepTo};
use swarmception_engine::repo_cmds::StatusResult;
use swarmception_model::Timestamp;

use crate::factory_chip::Motion;
use crate::factory_paint::{Drawn, Marks, Selected, WallView, paint};
use crate::factory_stats::StatsStrip;
use crate::icons;
use crate::layout::{Chevron, fold_title};
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

/// The key moments a step takes (#33): every kind, or one sort.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Steps {
    #[default]
    All,
    Claims,
    Work,
    Submits,
    /// Gate runs passing or failing.
    Gates,
    SendBacks,
    Merges,
}

impl Steps {
    pub const ALL: [Self; 7] = [Self::All, Self::Claims, Self::Work, Self::Submits, Self::Gates, Self::SendBacks, Self::Merges];

    /// The kinds of moment it takes: none named, every kind.
    #[must_use]
    pub fn kinds(self) -> Vec<MomentKind> {
        match self {
            Self::All => Vec::new(),
            Self::Claims => vec![MomentKind::Claim],
            Self::Work => vec![MomentKind::Work],
            Self::Submits => vec![MomentKind::Submit],
            Self::Gates => vec![MomentKind::GatePass, MomentKind::GateFail],
            Self::SendBacks => vec![MomentKind::Sendback],
            Self::Merges => vec![MomentKind::Merge],
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "every moment",
            Self::Claims => "claims",
            Self::Work => "work beginning",
            Self::Submits => "submits",
            Self::Gates => "gate runs",
            Self::SendBacks => "send backs",
            Self::Merges => "merges onto main",
        }
    }

    /// Its name on the command line and in the state: `all`, `merges`, `sendBacks`, …
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Claims => "claims",
            Self::Work => "work",
            Self::Submits => "submits",
            Self::Gates => "gates",
            Self::SendBacks => "sendBacks",
            Self::Merges => "merges",
        }
    }
}

/// How the factory follows the timeline, as the shell keeps it: plain data. It holds no time of
/// its own (#85: up to version 8 of the state it did, `t`, which a saved state still reads
/// without).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FactoryView {
    /// *t* follows the timeline: its playhead, else its window's end. Off, *t* holds.
    pub follow: bool,
    /// The timeline's playhead runs on through its window, looping; the factory follows it.
    pub playing: bool,
    pub speed: Speed,
    /// The ticket whose journey through the stations is traced.
    pub ticket: Option<u64>,
    /// The key moments ⏮ / ⏭ step through.
    pub steps: Steps,
    /// The benches of the workers off shift opened out of their fold (#106): after the rest,
    /// scrolling with them. Off, they fold into one line when the benches lack room (#94).
    pub off_shift: bool,
}

impl Default for FactoryView {
    fn default() -> Self {
        Self { follow: true, playing: false, speed: Speed::default(), ticket: None, steps: Steps::All, off_shift: false }
    }
}

/// Where a step or jump asked for puts *t*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// To the previous or next key moment (`factory.step`), or the first or last of them.
    Step(Direction),
    /// To the first event, or the last.
    First,
    Last,
}

/// A step that landed, and the *t* it left the factory at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Landed {
    at: Option<Timestamp>,
    step: Step,
}

/// Where this frame's *t* came from: always the timeline (#85), or where it last was.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// The timeline's playhead (a time put on it kept precisely while it stays on its item).
    Playhead,
    /// The timeline window's end: it has no playhead.
    WindowEnd,
    /// Play running the playhead through the window.
    Playing,
    /// On the playhead at a key moment a step put it at.
    Stepped,
    /// Not following the timeline: the last time it showed.
    Held,
    /// No time yet: the last event.
    #[default]
    Last,
}

impl Source {
    /// What the header's small label says of it; `t`, the time held.
    fn text(self, t: Option<Timestamp>) -> String {
        match self {
            Self::Playhead => "at the timeline's playhead".to_owned(),
            Self::WindowEnd => "at the timeline window's end: no playhead".to_owned(),
            Self::Playing => "playing: the playhead runs through the window".to_owned(),
            Self::Stepped => "on a key moment".to_owned(),
            Self::Held => t.map_or_else(
                || "not following the timeline".to_owned(),
                |t| {
                    let (_, _, _, h, m, s) = t.civil();
                    format!("not following the timeline: held at {h:02}:{m:02}:{s:02} UTC")
                },
            ),
            Self::Last => "at the last event".to_owned(),
        }
    }
}

/// What the header drew (#85): its buttons, each by name and where, which no text moves, and the
/// words after them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeaderDrawn {
    /// `play`, `speed`, `back`, `on`, `steps`, `follow`, in order.
    pub buttons: Vec<(&'static str, egui::Rect)>,
    /// The label after them: the moment a step landed on, else where *t* comes from.
    pub text: String,
}

/// A button's width in the header: the same for ▶ and ⏸, so play's state moves nothing.
const BUTTON: f32 = 28.0;

/// The scene the pane shows, and how it moves.
#[derive(Default)]
pub struct FactoryPane {
    scene: Option<FactoryScene>,
    /// The time it shows (#85: the timeline's), asked of `factory.at`; none before the first.
    t: Option<Timestamp>,
    /// A time put on the timeline's playhead (a step, play, `--at`, an hour's click), kept while
    /// the playhead stays on the item it falls in.
    anchor: Option<Timestamp>,
    /// What it has selected (#85), from the details' subject.
    selected: Option<Selected>,
    header: HeaderDrawn,
    /// What the scene was asked for: *t*, `recent`, the traced ticket, and the engine's revision.
    asked: Option<Asked>,
    error: Option<String>,
    source: Source,
    /// Play's position in fractional seconds, between frames.
    play_at: f64,
    /// The rectangle the floor was drawn in last.
    rect: Option<egui::Rect>,
    motion: Motion,
    wall: WallView,
    drawn: Drawn,
    /// The ticket under the pointer last frame.
    hovered: Option<u64>,
    /// The statistics strip in the header, for the timeline's window.
    strip: StatsStrip,
    /// A step or jump asked for, to run once there is a scene.
    jump: Option<Jump>,
    /// The step that landed last.
    landed: Option<Landed>,
    /// Its title was clicked last frame: fold it (#68).
    fold: bool,
    /// The benches off shift were opened out of their fold last frame (#106): a fold opened since
    /// scrolls its first bench into view.
    off_shift: bool,
}

/// What a scene was asked for: *t*, `recent`, the traced ticket, and the engine's revision.
type Asked = (Option<Timestamp>, i64, Option<u64>, Option<RunAt>, (u64, u64));

/// What a play step does.
struct Play {
    dt: f64,
    window: Option<(Timestamp, Timestamp)>,
}

impl FactoryPane {
    /// Whether its title was clicked last frame, to fold it (#68).
    #[must_use]
    pub const fn fold_asked(&self) -> bool {
        self.fold
    }

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

    /// The rectangle the floor was drawn in last frame.
    #[must_use]
    pub const fn floor(&self) -> Option<egui::Rect> {
        self.rect
    }

    /// The statistics strip in the header.
    #[must_use]
    pub const fn strip(&self) -> &StatsStrip {
        &self.strip
    }

    /// The strip keeps its range after a click on the hour starting at `hour` (#85).
    pub fn keep_strip(&mut self, hour: Timestamp) {
        self.strip.keep(hour);
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

    /// Why there is no scene: `factory.at`'s error.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Where this frame's *t* came from.
    #[must_use]
    pub const fn source(&self) -> Source {
        self.source
    }

    /// The time it shows (#85): the timeline's, or the last it showed; the scene's before the
    /// first (the last event).
    #[must_use]
    pub fn t(&self) -> Option<Timestamp> {
        self.t.or_else(|| self.scene.as_ref().map(|s| s.t))
    }

    /// What it has selected (#85).
    #[must_use]
    pub const fn selected(&self) -> Option<&Selected> {
        self.selected.as_ref()
    }

    /// Selects what the details' subject is in the factory (#85): a ticket's chips and brick, a
    /// merge's brick, a gate run's line; main's wall scrolls a newly selected brick into view.
    pub fn select(&mut self, selected: Option<Selected>) {
        if selected != self.selected {
            self.wall.reveal = selected.is_some();
            self.selected = selected;
        }
    }

    /// What the header drew last frame.
    #[must_use]
    pub const fn header_drawn(&self) -> &HeaderDrawn {
        &self.header
    }

    /// Puts the factory at `t` on the timeline's playhead (#85: `--at`, `ui.set`'s `at`, an
    /// hour's click): following the timeline, play off; the shell puts the playhead there, and the
    /// factory keeps `t` while the playhead stays on the item it falls in.
    pub fn put(&mut self, view: &mut FactoryView, t: Timestamp) {
        self.anchor = Some(t);
        self.t = Some(t);
        self.landed = None;
        view.follow = true;
        view.playing = false;
    }

    /// Moves *t* for this frame, from play, the timeline or neither: the time the timeline's
    /// playhead should be put at, while playing. `engine` places a time put on the playhead.
    pub fn step(&mut self, ctx: &egui::Context, engine: &mut Session, view: &mut FactoryView, looking: Option<Looking>) -> Option<Timestamp> {
        if view.playing {
            view.follow = true;
            let dt = f64::from(ctx.input(|i| i.stable_dt).clamp(0.0, 0.1));
            let t = self.advance(view, Play { dt, window: looking.and_then(|l| l.window) });
            self.anchor = Some(t);
            ctx.request_repaint();
            self.source = Source::Playing;
            return Some(t);
        }
        if !view.follow {
            // The last time it showed; the timeline's when it never showed one.
            self.anchor = None;
            if self.t.is_none() {
                self.t = looking.and_then(followed).map(|(t, _)| t);
            }
            self.source = if self.t.is_some() { Source::Held } else { Source::Last };
            return None;
        }
        let found = match looking {
            // No timeline to read yet: a time put waits on it.
            None => self.anchor.map(|t| (t, Source::Playhead)),
            Some(looking) => {
                let mut on = |t: Timestamp| engine.call::<PlaceAt>(PlaceAtParams { time: t }).ok().map(|p| p.item);
                self.anchor = self.anchor.filter(|t| on(*t).is_some_and(|item| item == looking.playhead_item));
                self.anchor.map(|t| (t, Source::Playhead)).or_else(|| followed(looking))
            }
        };
        match found {
            Some((t, source)) => {
                self.t = Some(t);
                self.source = source;
            }
            None => self.source = if self.t.is_some() { Source::Held } else { Source::Last },
        }
        if self.moment().is_some_and(|step| step.t.is_some()) {
            self.source = Source::Stepped;
        }
        None
    }

    /// Asks for a step or jump, run at the next [`Self::jump`] once there is a scene.
    pub const fn ask(&mut self, jump: Jump) {
        self.jump = Some(jump);
    }

    /// Runs the step or jump asked for: the timeline's playhead (and *t*) to the key moment
    /// `factory.step` names (the kinds `view.steps` says), or to the first or last event, play
    /// off and following on. The time the timeline's playhead should be put at; none when nothing
    /// moved (no moment that way: the step's text says so in the header).
    pub fn jump(&mut self, engine: &mut Session, view: &mut FactoryView) -> Option<Timestamp> {
        let scene = self.scene.as_ref()?;
        let jump = self.jump.take()?;
        let from = self.t.unwrap_or(scene.t);
        let (first, last) = (scene.first, scene.last);
        let (t, step) = match jump {
            Jump::First => (first?, None),
            Jump::Last => (last?, None),
            Jump::Step(direction) => {
                let step = engine.call::<StepTo>(StepParams { t: Some(from), direction, kinds: view.steps.kinds() }).ok()?;
                let Some(t) = step.t else {
                    self.landed = Some(Landed { at: Some(from), step });
                    return None;
                };
                (t, Some(step))
            }
        };
        self.put(view, t);
        self.landed = step.map(|step| Landed { at: Some(t), step });
        Some(t)
    }

    /// The step that put the factory at its *t* (or found no moment from it), while *t* is
    /// still there: what the header names.
    #[must_use]
    pub fn moment(&self) -> Option<&Step> {
        self.landed.as_ref().filter(|l| l.at == self.t).map(|l| &l.step)
    }

    /// ← / → (a step back or on) and Home / End (the first or last event) while the factory has
    /// the keyboard, which a press anywhere in it takes, as a press on the timeline takes it there.
    fn keys(&mut self, ui: &egui::Ui, pane: egui::Rect) {
        let id = ui.scope_id().with("factory-keys");
        ui.interact(pane, id, egui::Sense::focusable_noninteractive());
        let pressed_here = ui.input(|i| (i.pointer.any_pressed() || i.pointer.any_click()) && i.pointer.interact_pos().is_some_and(|p| pane.contains(p)));
        if pressed_here {
            ui.memory_mut(|m| m.request_focus(id));
        }
        if !ui.memory(|m| m.has_focus(id)) {
            return;
        }
        let filter = egui::EventFilter { horizontal_arrows: true, ..egui::EventFilter::default() };
        ui.memory_mut(|m| m.set_focus_lock_filter(id, filter));
        let keys = [
            (egui::Key::ArrowLeft, Jump::Step(Direction::Previous)),
            (egui::Key::ArrowRight, Jump::Step(Direction::Next)),
            (egui::Key::Home, Jump::First),
            (egui::Key::End, Jump::Last),
        ];
        for (key, jump) in keys {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                self.ask(jump);
                ui.ctx().request_repaint();
            }
        }
    }

    /// Play's next *t*: on by `dt` at the view's speed through the window (else the swarm's
    /// first to last event), back to its start after its end.
    fn advance(&mut self, view: &FactoryView, play: Play) -> Timestamp {
        let scene = self.scene.as_ref();
        let whole = scene.and_then(|s| s.first.zip(s.last));
        let (start, end) = play.window.or(whole).unwrap_or_default();
        let (start, end) = (start.0 as f64, end.0.max(start.0) as f64);
        let at = self.t.map_or(start, |t| t.0 as f64);
        if (self.play_at.floor() - at).abs() >= 1.0 {
            self.play_at = at;
        }
        let mut next = self.play_at + play.dt * view.speed.per_second() as f64;
        if !(start..=end).contains(&next) {
            next = start;
        }
        self.play_at = next;
        let t = Timestamp(next.floor() as i64);
        self.t = Some(t);
        t
    }

    /// Asks `factory.at` again when *t*, `recent` or the engine moved on.
    pub fn sync(&mut self, engine: &mut Session, view: &FactoryView, recent: i64) {
        let (t, ticket) = (self.t, view.ticket);
        // A run selected stays in the gate's list, however long ago it ran (#95).
        let run = match &self.selected {
            Some(Selected::Run { sub, at }) => Some(RunAt { sub: sub.clone(), at: *at }),
            _ => None,
        };
        let asked = Some((t, recent, ticket, run.clone(), engine.revision()));
        if self.asked == asked {
            return;
        }
        self.asked = asked;
        match engine.call::<At>(AtParams { t, recent: Some(recent), ticket, run }) {
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
        self.keys(ui, ui.max_rect());
        ui.horizontal_wrapped(|ui| self.header(ui, view));
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
        // Within what shows: a header or strip wider than the pane would widen the floor past its edge.
        let rect = ui.available_rect_before_wrap().intersect(ui.clip_rect());
        ui.allocate_rect(rect, egui::Sense::hover());
        self.rect = Some(rect);
        if view.off_shift && !self.off_shift {
            self.wall.reveal_unfolded = true;
        }
        self.off_shift = view.off_shift;
        let marks = Marks { hovered: self.hovered, traced: view.ticket, selected: self.selected.clone(), off_shift: view.off_shift };
        self.drawn = paint(ui, rect, scene, (&mut self.motion, &mut self.wall), marks);
        if self.drawn.fold_clicked {
            view.off_shift = !view.off_shift;
            ui.ctx().request_repaint();
        }
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

    /// The header (#85): the title, then the buttons at fixed places (play and its speed, the
    /// steps and what they step by, follow), then the words: the moment a step landed on, else
    /// where *t* comes from, in a small label. No clock: the timeline's playhead is the time.
    fn header(&mut self, ui: &mut egui::Ui, view: &mut FactoryView) {
        let mut buttons = Vec::new();
        self.fold = fold_title(ui, icons::FACTORY, "Factory", Chevron::Open).clicked();
        ui.separator();
        let size = egui::vec2(BUTTON, ui.spacing().interact_size.y);
        let play = if view.playing { icons::PAUSE } else { icons::PLAY };
        let response = ui
            .add_sized(size, egui::Button::new(play))
            .on_hover_text("Play: the timeline's playhead runs through its window, looping, and the factory follows it");
        if response.clicked() {
            view.playing = !view.playing;
            view.follow |= view.playing;
        }
        buttons.push(("play", response.rect));
        let speed = fixed(ui, 96.0, |ui| {
            egui::ComboBox::from_id_salt("factory-speed").selected_text(view.speed.label()).width(96.0).truncate().show_ui(ui, |ui| {
                for speed in Speed::ALL {
                    ui.selectable_value(&mut view.speed, speed, speed.label());
                }
            });
        });
        buttons.push(("speed", speed));
        ui.separator();
        let steps = view.steps.label();
        let back = ui
            .add_sized(size, egui::Button::new(icons::START))
            .on_hover_text(format!("The timeline's playhead back to the previous key moment ({steps}; ←)\nShift-click: to the first event (Home)"));
        if back.clicked() {
            self.ask(if ui.input(|i| i.modifiers.shift) { Jump::First } else { Jump::Step(Direction::Previous) });
        }
        buttons.push(("back", back.rect));
        let on = ui
            .add_sized(size, egui::Button::new(icons::END))
            .on_hover_text(format!("The timeline's playhead on to the next key moment ({steps}; →)\nShift-click: to the last event (End)"));
        if on.clicked() {
            self.ask(if ui.input(|i| i.modifiers.shift) { Jump::Last } else { Jump::Step(Direction::Next) });
        }
        buttons.push(("on", on.rect));
        let by = fixed(ui, 176.0, |ui| {
            egui::ComboBox::from_id_salt("factory-steps").selected_text(format!("by {steps}")).width(176.0).truncate().show_ui(ui, |ui| {
                for kind in Steps::ALL {
                    ui.selectable_value(&mut view.steps, kind, kind.label());
                }
            });
        });
        buttons.push(("steps", by));
        ui.separator();
        let follow = ui
            .toggle_value(&mut view.follow, format!("{}  Follow the timeline", icons::FOLLOW))
            .on_hover_text("The factory shows the timeline's playhead, else its window's end; off, it holds where it is");
        buttons.push(("follow", follow.rect));
        ui.separator();
        let text = self.header_words(ui);
        self.header = HeaderDrawn { buttons, text };
    }

    /// The header's words after its buttons: the moment a step landed on, else where *t* comes
    /// from in a small label; not following, that it holds says more than the moment it holds at.
    fn header_words(&self, ui: &mut egui::Ui) -> String {
        match self.moment().filter(|_| self.source != Source::Held) {
            Some(step) => {
                moment_label(ui, step);
                step.text.clone()
            }
            None => {
                let text = self.source.text(self.t);
                ui.weak(egui::RichText::new(&text).small());
                text
            }
        }
    }
}

/// `add` in a place `width` wide, whatever it draws (a combo box truncating its text): where.
fn fixed(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) -> egui::Rect {
    let size = egui::vec2(width, ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        add(ui);
    })
    .response
    .rect
}

/// A step's landing in the header: its ticket's colour, what happened, and in its tooltip
/// every moment at that second with its note, and its place among them all.
fn moment_label(ui: &mut egui::Ui, step: &Step) {
    let colour = step.moments.first().map(|m| crate::factory_chip::rgb(m.colour));
    if let Some(colour) = colour {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, colour);
    }
    let text = if step.t.is_some() { egui::RichText::new(&step.text).strong() } else { egui::RichText::new(&step.text).weak().italics() };
    let mut lines: Vec<String> =
        step.moments.iter().map(|m| m.note.as_ref().map_or_else(|| m.text.clone(), |note| format!("{}\n    {note}", m.text))).collect();
    if let Some(index) = step.index {
        lines.push(format!("key moment {index} of {}", step.count));
    }
    let label = ui.label(text);
    if !lines.is_empty() {
        label.on_hover_text(lines.join("\n"));
    }
}

/// The time the timeline shows (#85): its playhead, else its window's end.
fn followed(looking: Looking) -> Option<(Timestamp, Source)> {
    looking.playhead.map(|t| (t, Source::Playhead)).or_else(|| looking.window.map(|(_, end)| (end, Source::WindowEnd)))
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
        Some(Looking {
            hover: hover.map(Timestamp),
            playhead: playhead.map(Timestamp),
            playhead_item: playhead.and_then(|p| u64::try_from(p).ok()),
            window: window.map(|(a, b)| (Timestamp(a), Timestamp(b))),
        })
    }

    #[test]
    fn following_takes_the_playhead_then_the_windows_end_never_the_pointer() {
        let ctx = egui::Context::default();
        let mut engine = Session::default();
        let mut pane = FactoryPane::default();
        let mut view = FactoryView::default();
        let mut step = |view: &mut FactoryView, l| {
            ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
            pane.step(&ctx, &mut engine, view, l);
            (pane.t, pane.source)
        };
        assert_eq!(step(&mut view, looking(Some(5), Some(7), Some((0, 9)))), (Some(Timestamp(7)), Source::Playhead), "the playhead, not the pointer");
        assert_eq!(step(&mut view, looking(Some(5), None, Some((0, 9)))), (Some(Timestamp(9)), Source::WindowEnd));
        assert_eq!(step(&mut view, looking(None, None, None)), (Some(Timestamp(9)), Source::Held), "nothing to follow: t stays");
        // Not following: it holds where it was, whatever the timeline does.
        view.follow = false;
        assert_eq!(step(&mut view, looking(None, Some(3), Some((0, 9)))), (Some(Timestamp(9)), Source::Held));
        view.follow = true;
        assert_eq!(step(&mut view, looking(None, Some(3), Some((0, 9)))), (Some(Timestamp(3)), Source::Playhead), "following again: the playhead");
    }

    #[test]
    fn a_pane_that_never_showed_a_time_takes_the_timelines_when_not_following() {
        let ctx = egui::Context::default();
        let mut engine = Session::default();
        let mut pane = FactoryPane::default();
        let mut view = FactoryView { follow: false, ..FactoryView::default() };
        pane.step(&ctx, &mut engine, &mut view, None);
        assert_eq!((pane.t, pane.source), (None, Source::Last));
        pane.step(&ctx, &mut engine, &mut view, looking(None, Some(4), Some((0, 9))));
        assert_eq!((pane.t, pane.source), (Some(Timestamp(4)), Source::Held));
        assert_eq!(pane.source.text(pane.t), "not following the timeline: held at 00:00:04 UTC");
    }

    #[test]
    fn play_runs_through_the_window_and_loops() {
        let mut pane = FactoryPane { t: Some(Timestamp(100)), ..FactoryPane::default() };
        let view = FactoryView { playing: true, speed: Speed::Minute, ..FactoryView::default() };
        let window = Some((Timestamp(100), Timestamp(200)));
        let t = pane.advance(&view, Play { dt: 0.5, window });
        assert_eq!(t, Timestamp(130));
        assert_eq!(pane.advance(&view, Play { dt: 0.01, window }), Timestamp(130), "fractions add up between frames");
        assert_eq!(pane.advance(&view, Play { dt: 1.0, window }), Timestamp(190));
        assert_eq!(pane.advance(&view, Play { dt: 1.0, window }), Timestamp(100), "past the end, back to the start");
        pane.t = Some(Timestamp(150));
        assert_eq!(pane.advance(&view, Play { dt: 0.0, window }), Timestamp(150), "a t the playhead moved to is taken up");
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
        let view = FactoryView { follow: false, playing: true, speed: Speed::Hour, ticket: Some(7), steps: Steps::SendBacks, off_shift: true };
        let text = serde_json::to_string(&view).unwrap();
        assert_eq!(text, r#"{"follow":false,"playing":true,"speed":"hour","ticket":7,"steps":"sendBacks"}"#);
        assert_eq!(serde_json::from_str::<FactoryView>(&text).unwrap(), view);
        assert_eq!(serde_json::from_str::<FactoryView>("{}").unwrap(), FactoryView::default());
        assert!(serde_json::from_str::<FactoryView>(r#"{"speed":"warp"}"#).is_err());
    }

    /// The header drawn once, 1400 points wide: what it drew.
    fn header(pane: &mut FactoryPane, view: &mut FactoryView) -> HeaderDrawn {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 200.0));
        let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            ui.horizontal_wrapped(|ui| pane.header(ui, view));
        });
        output.textures_delta.clear();
        pane.header_drawn().clone()
    }

    #[test]
    fn the_headers_buttons_stay_put_whatever_its_text_says() {
        let mut view = FactoryView::default();
        let mut pane = FactoryPane { source: Source::Playhead, ..FactoryPane::default() };
        let first = header(&mut pane, &mut view);
        let names: Vec<&str> = first.buttons.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["play", "speed", "back", "on", "steps", "follow"]);
        assert_eq!(first.text, "at the timeline's playhead");
        // Other words after them, short and long; play's pause for its play; a moment named: the
        // same places.
        let held = FactoryPane { source: Source::Held, t: Some(Timestamp(1_000)), ..FactoryPane::default() };
        let moment = Step {
            t: Some(Timestamp(5)),
            moments: Vec::new(),
            text: "#7 merged onto main (PR #8) · worker-1, and a good deal more said after it".to_owned(),
            index: Some(3),
            count: 9,
        };
        let stepped = FactoryPane { t: Some(Timestamp(5)), landed: Some(Landed { at: Some(Timestamp(5)), step: moment }), ..FactoryPane::default() };
        let cases = [
            (held, FactoryView::default()),
            (FactoryPane { source: Source::WindowEnd, ..FactoryPane::default() }, FactoryView { playing: true, ..FactoryView::default() }),
            (stepped, FactoryView { follow: false, steps: Steps::SendBacks, ..FactoryView::default() }),
            (FactoryPane { source: Source::Last, ..FactoryPane::default() }, FactoryView { speed: Speed::TenSeconds, ..FactoryView::default() }),
        ];
        let mut texts = vec![first.text.clone()];
        for (mut pane, mut view) in cases {
            let drawn = header(&mut pane, &mut view);
            assert_eq!(drawn.buttons, first.buttons, "the buttons where they were, for {:?}", drawn.text);
            texts.push(drawn.text);
        }
        // Whatever the speed and the steps say, the widest of them included.
        for (speed, steps) in Speed::ALL.into_iter().flat_map(|speed| Steps::ALL.map(|steps| (speed, steps))) {
            let drawn = header(&mut FactoryPane::default(), &mut FactoryView { speed, steps, ..FactoryView::default() });
            assert_eq!(drawn.buttons, first.buttons, "the buttons where they were for {speed:?} by {steps:?}");
        }
        texts.dedup();
        assert_eq!(texts.len(), 5, "five different texts: {texts:?}");
        assert!(texts.contains(&"not following the timeline: held at 00:16:40 UTC".to_owned()), "{texts:?}");
    }
}
