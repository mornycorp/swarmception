//! The ticket chip (#18, the owner's review): one shape for a ticket's whole life, at every
//! station. Its **fill** is always its ticket's own colour (`ticket_colour`, from its id); its
//! **border** is its phase, the same colours everywhere (claimed yellow, working orange, waiting
//! blue, in the gate cyan, sent back red, stopped grey). Up next, not started, it is a hollow
//! outline in its colour; merged, a plain brick with no border. Only its size changes, with how
//! much it says: [`Size::Large`] at a bench, [`Size::Medium`] in the queues, [`Size::Small`] on the
//! shelf and in main's wall.
//!
//! [`Motion`] tweens each chip's rectangle, so a ticket glides between stations and shrinks as it
//! goes, and a merging ticket flies onto main and becomes a brick in the wall.

use std::collections::{HashMap, HashSet};

use egui::{Align2, Color32, FontId, Rect, Stroke, StrokeKind, vec2};
use swarmception_domain::factory::{CrateState, HappeningKind};

pub const CLAIMED: Color32 = Color32::from_rgb(0xf2, 0xc9, 0x4c);
pub const WORKING: Color32 = Color32::from_rgb(0xff, 0x8c, 0x1a);
pub const WAITING: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);
pub const GATING: Color32 = Color32::from_rgb(0x22, 0xd3, 0xee);
pub const SENT_BACK: Color32 = Color32::from_rgb(0xe5, 0x48, 0x4d);
pub const STOPPED: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);
pub const PASSED: Color32 = Color32::from_rgb(0x2e, 0xb8, 0x5c);

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

/// The look of a ticket in `state`.
#[must_use]
pub const fn look(state: CrateState) -> Look {
    match state {
        CrateState::Upcoming => Look::Hollow,
        CrateState::Claimed => Look::Phase(CLAIMED),
        CrateState::Working => Look::Phase(WORKING),
        CrateState::Waiting => Look::Phase(WAITING),
        CrateState::Gating => Look::Phase(GATING),
        CrateState::SentBack => Look::Phase(SENT_BACK),
        CrateState::Stopped | CrateState::Shelved => Look::Phase(STOPPED),
        CrateState::Merged | CrateState::Done => Look::Plain,
    }
}

/// The look a happening leaves its ticket in, for the ticker's chips.
#[must_use]
pub const fn look_after(kind: HappeningKind) -> Look {
    match kind {
        HappeningKind::Claimed => Look::Phase(CLAIMED),
        HappeningKind::Began => Look::Phase(WORKING),
        HappeningKind::Submitted => Look::Phase(WAITING),
        HappeningKind::SentBack | HappeningKind::GateFailed => Look::Phase(SENT_BACK),
        HappeningKind::Stopped | HappeningKind::Released => Look::Phase(STOPPED),
        HappeningKind::Merged | HappeningKind::Done | HappeningKind::GatePassed | HappeningKind::Landed => Look::Plain,
    }
}

#[must_use]
pub const fn rgb(colour: [u8; 3]) -> Color32 {
    Color32::from_rgb(colour[0], colour[1], colour[2])
}

/// Black or white, whichever reads on `fill`.
#[must_use]
pub fn ink(fill: Color32) -> Color32 {
    let luma = 0.299 * f32::from(fill.r()) + 0.587 * f32::from(fill.g()) + 0.114 * f32::from(fill.b());
    if luma > 150.0 { Color32::from_rgb(0x1a, 0x1a, 0x1a) } else { Color32::WHITE }
}

/// What a chip shows besides its colour.
pub struct Chip<'a> {
    pub colour: [u8; 3],
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
    let fill = rgb(chip.colour);
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

    #[test]
    fn the_fill_is_the_tickets_and_only_the_border_says_the_phase() {
        assert_eq!(look(CrateState::Upcoming), Look::Hollow);
        assert_eq!(look(CrateState::Merged), Look::Plain);
        assert_eq!(look(CrateState::Done), Look::Plain);
        let phases = [CrateState::Claimed, CrateState::Working, CrateState::Waiting, CrateState::Gating, CrateState::SentBack, CrateState::Stopped];
        let borders: Vec<Look> = phases.iter().map(|s| look(*s)).collect();
        assert_eq!(borders, [CLAIMED, WORKING, WAITING, GATING, SENT_BACK, STOPPED].map(Look::Phase));
        assert_eq!(look_after(HappeningKind::Submitted), look(CrateState::Waiting));
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
