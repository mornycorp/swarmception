//! The ticket chip (#18, the owner's review): one shape for a ticket's whole life, at every
//! station. Its **fill** is always its ticket's own colour (the domain's `ticket_colour`, from its
//! id, as on the timeline); its **border** is its phase, the same colours everywhere: the
//! timeline's from the domain's `colours`, the theme's tokens (#152: claimed `warning`, waiting
//! `accent`, sent back `error`, stopped `muted`, working `strong`, in the gate `gate`), each drawn
//! in the look egui draws in ([`drawn`], #154). Up next, not started, it is a hollow
//! outline in its colour; merged, a plain brick with no border. Only its size changes, with how
//! much it says: [`Size::Large`] at a bench, [`Size::Medium`] in the queues, [`Size::Small`] on the
//! shelf and in main's wall.
//!
//! [`Motion`] tweens each chip's rectangle, so a ticket glides between stations and shrinks as it
//! goes, and a merging ticket flies onto main and becomes a brick in the wall.

use std::collections::{HashMap, HashSet};

use egui::{Align2, Color32, FontId, Rect, Stroke, StrokeKind, vec2};
use swarmception_domain::colours::{self, Paint};
use swarmception_domain::factory::CrateState;
use swarmception_domain::theme;

pub const CLAIMED: Paint = colours::CLAIMED;
pub const WAITING: Paint = colours::WAITING;
pub const SENT_BACK: Paint = colours::SENT_BACK;
pub const STOPPED: Paint = colours::STOPPED;
/// Main's green: a merge onto it, a gate passed.
pub const PASSED: Paint = colours::MAIN_MERGE;
/// The factory's own: a working border (its fill is the ticket's colour), a run going on: the
/// theme's `strong` (#152).
pub const WORKING: Paint = colours::WORKING;
/// The factory's own: in a gate run, the theme's `gate` (#152).
pub const GATING: Paint = colours::GATING;
/// An idle bench: the theme's `idle` (#152).
pub const IDLE: Paint = colours::IDLE;

/// The theme's look for egui's `visuals` (#154): its dark or light.
#[must_use]
pub const fn shade(visuals: &egui::Visuals) -> theme::Look {
    if visuals.dark_mode { theme::Look::Dark } else { theme::Look::Light }
}

/// One of the swarm's colours in the look egui draws in (#154): its light value on a light look.
#[must_use]
pub fn drawn(paint: Paint, visuals: &egui::Visuals) -> Color32 {
    rgb(paint.of(shade(visuals)))
}

const GLIDE: f64 = 0.7;
/// A brick's key in [`Motion`], beside its ticket's chip's.
pub const BRICK: u64 = 1 << 63;

/// How big a chip is, and so how much it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// At a bench: `#170`, a progress bar under it (room for a title later, #17).
    Large,
    /// Up next, at the dock, in the gate: `#170`, the wait beside it.
    Medium,
    /// On the shelf and done: `#170`; in main's wall, as big as the wall lets it be.
    Small,
}

impl Size {
    #[must_use]
    pub const fn dims(self) -> egui::Vec2 {
        match self {
            Self::Large => vec2(62.0, 28.0),
            Self::Medium => vec2(46.0, 22.0),
            Self::Small => vec2(34.0, 17.0),
        }
    }
}

/// How a chip is outlined: hollow before it starts, its phase's border while it is worked on and
/// waits, plain once finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Hollow,
    Phase(Color32),
    Plain,
}

/// The look of a ticket in `state`, its phase's border in `shade`.
#[must_use]
pub const fn look(state: CrateState, shade: theme::Look) -> Look {
    let phase = match state {
        CrateState::Upcoming => return Look::Hollow,
        CrateState::Merged | CrateState::Done => return Look::Plain,
        CrateState::Claimed => CLAIMED,
        CrateState::Working => WORKING,
        CrateState::Waiting => WAITING,
        CrateState::Gating => GATING,
        CrateState::SentBack => SENT_BACK,
        CrateState::Stopped | CrateState::Shelved => STOPPED,
    };
    Look::Phase(rgb(phase.of(shade)))
}

#[must_use]
pub const fn rgb(colour: [u8; 3]) -> Color32 {
    Color32::from_rgb(colour[0], colour[1], colour[2])
}

/// Black or white, whichever reads on `fill` (the domain's [`colours::ink`]).
#[must_use]
pub fn ink(fill: Color32) -> Color32 {
    rgb(colours::ink([fill.r(), fill.g(), fill.b()]))
}

/// What a chip shows besides its colour.
pub struct Chip<'a> {
    /// Its ticket's colour, in the look drawn in.
    pub colour: Color32,
    pub look: Look,
    pub label: &'a str,
    /// The background a hollow chip shows through.
    pub ground: Color32,
    /// Hovered here or at another station: a ring in `ring`.
    pub highlight: Option<Color32>,
    /// How fresh its phase is, 0 to 1: a glow in its border's colour.
    pub glow: f32,
}

/// Draws `chip` in `rect`; the label shrinks with the chip and is left out when it can't fit.
pub fn paint_chip(painter: &egui::Painter, rect: Rect, chip: &Chip<'_>) {
    let fill = chip.colour;
    let radius = (rect.height() * 0.28).clamp(1.5, 7.0);
    let border = (rect.height() / 9.0).clamp(1.5, 2.5);
    if let (Look::Phase(phase), true) = (chip.look, chip.glow > 0.0) {
        painter.rect_filled(rect.expand(border + 4.0 * chip.glow), radius + 3.0, phase.gamma_multiply(0.4 * chip.glow));
    }
    let (body, stroke, text) = match chip.look {
        Look::Hollow => (chip.ground, Stroke::new(border.max(1.5), fill), fill),
        Look::Phase(phase) => (fill, Stroke::new(border, phase), ink(fill)),
        Look::Plain => (fill, Stroke::new(1.0, fill.lerp_to_gamma(Color32::BLACK, 0.3)), ink(fill)),
    };
    let kind = if chip.look == Look::Plain { StrokeKind::Inside } else { StrokeKind::Outside };
    painter.rect(rect, radius, body, stroke, kind);
    if let Some(ring) = chip.highlight {
        painter.rect_stroke(rect.expand(border + 3.0), radius + 3.0, Stroke::new(2.0, ring), StrokeKind::Outside);
    }
    let size = (rect.height() * 0.5).clamp(7.0, 13.0);
    if rect.height() >= 8.0 && rect.width() >= 18.0 {
        let font = FontId::proportional(size);
        let galley = painter.layout_no_wrap(chip.label.to_owned(), font, text);
        if galley.size().x <= rect.width() - 2.0 {
            let at = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
            painter.galley(at.min, galley, text);
        }
    }
}

/// How far into a glide: eased out.
fn ease(fraction: f64) -> f32 {
    let f = fraction.clamp(0.0, 1.0);
    (1.0 - (1.0 - f).powi(3)) as f32
}

#[derive(Debug, Clone, Copy)]
struct Tween {
    from: Rect,
    to: Rect,
    start: f64,
}

impl Tween {
    fn at(&self, now: f64) -> Rect {
        let f = ease((now - self.start) / GLIDE);
        Rect::from_min_max(self.from.min.lerp(self.to.min, f), self.from.max.lerp(self.to.max, f))
    }

    fn moving(&self, now: f64) -> bool {
        now - self.start < GLIDE && self.from != self.to
    }
}

/// Where each chip and brick was drawn, and how big, so they glide and grow or shrink as they move.
#[derive(Debug, Default)]
pub struct Motion {
    tweens: HashMap<u64, Tween>,
    seen: HashSet<u64>,
    now: f64,
    rect: Option<Rect>,
    snap: bool,
    moving: bool,
}

impl Motion {
    pub fn begin(&mut self, now: f64, rect: Rect) {
        // A pane that changed size moves everything at once: no glide.
        self.snap = self.rect != Some(rect);
        self.rect = Some(rect);
        self.now = now;
        self.seen.clear();
        self.moving = false;
    }

    /// Where `key` draws this frame, gliding to `target`.
    pub fn place(&mut self, key: u64, target: Rect) -> Rect {
        self.seen.insert(key);
        let now = self.now;
        let still = Tween { from: target, to: target, start: now };
        let tween = self.tweens.entry(key).or_insert(still);
        if self.snap {
            *tween = still;
        } else if tween.to.min.distance(target.min) > 0.5 || tween.to.max.distance(target.max) > 0.5 {
            *tween = Tween { from: tween.at(now), to: target, start: now };
        }
        self.moving |= tween.moving(now);
        tween.at(now)
    }

    /// Where `issue`'s brick draws: a new brick flies from where its chip was last drawn,
    /// shrinking to its place in the wall.
    pub fn land(&mut self, issue: u64, target: Rect) -> Rect {
        let key = issue | BRICK;
        if !self.snap
            && !self.tweens.contains_key(&key)
            && !self.seen.contains(&issue)
            && let Some(from) = self.tweens.get(&issue).map(|t| t.at(self.now))
        {
            self.tweens.insert(key, Tween { from, to: target, start: self.now });
        }
        self.place(key, target)
    }

    pub fn end(&mut self, ctx: &egui::Context) {
        let seen = &self.seen;
        self.tweens.retain(|key, _| seen.contains(key));
        if self.moving {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #152: the factory's own borders are the theme's tokens, and read on the dark floor.
    #[test]
    fn the_factorys_own_borders_are_the_themes_tokens() {
        for own in [WORKING, GATING] {
            for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
                let floor = visuals.panel_fill;
                let border = drawn(own, &visuals);
                assert!(colours::contrast([border.r(), border.g(), border.b()], [floor.r(), floor.g(), floor.b()]) >= 4.5, "{own:02x?} reads on the floor");
            }
        }
        assert_eq!(
            (drawn(WORKING, &egui::Visuals::light()), drawn(GATING, &egui::Visuals::light())),
            (rgb(colours::WORKING.light), rgb(colours::GATING.light))
        );
    }

    #[test]
    fn the_fill_is_the_tickets_and_only_the_border_says_the_phase() {
        let dark = theme::Look::Dark;
        assert_eq!(look(CrateState::Upcoming, dark), Look::Hollow);
        assert_eq!(look(CrateState::Merged, dark), Look::Plain);
        assert_eq!(look(CrateState::Done, dark), Look::Plain);
        let phases = [CrateState::Claimed, CrateState::Working, CrateState::Waiting, CrateState::Gating, CrateState::SentBack, CrateState::Stopped];
        for shade in [theme::Look::Dark, theme::Look::Light] {
            let borders: Vec<Look> = phases.iter().map(|s| look(*s, shade)).collect();
            assert_eq!(borders, [CLAIMED, WORKING, WAITING, GATING, SENT_BACK, STOPPED].map(|p| Look::Phase(rgb(p.of(shade)))));
        }
    }

    #[test]
    fn a_landing_chip_flies_and_shrinks_into_its_brick() {
        let mut motion = Motion::default();
        let pane = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1000.0, 600.0));
        let chip = Rect::from_center_size(egui::pos2(100.0, 100.0), Size::Medium.dims());
        let brick = Rect::from_min_size(egui::pos2(800.0, 500.0), vec2(18.0, 11.0));
        motion.begin(0.0, pane);
        motion.place(9, chip);
        motion.begin(0.1, pane);
        motion.begin(0.1, pane);
        let first = motion.land(9, brick);
        assert_eq!(first, chip, "it starts where the chip was");
        motion.begin(0.1 + GLIDE / 2.0, pane);
        let midway = motion.land(9, brick);
        assert!(midway.width() < chip.width() && midway.width() > brick.width(), "{midway:?}");
        motion.begin(0.1 + GLIDE, pane);
        assert_eq!(motion.land(9, brick), brick);
    }
}
