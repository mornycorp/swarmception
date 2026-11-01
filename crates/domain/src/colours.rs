//! The one source of the swarm's colours (#15, the owner's): the timeline and the factory both
//! take them from here, so a ticket and a phase look the same in both.
//!
//! - **A ticket's own colour** ([`ticket_colour`]), from its issue number: hues a golden angle
//!   (137.5°) apart, so tickets next to each other differ clearly, kept out of the arcs around the
//!   fixed colours' hues so a ticket never reads as a phase, and light enough to read on the dark
//!   background with black labels on every hue. Its *working* stretches and its merge are drawn
//!   in it.
//! - **The fixed colours**: claimed [`CLAIMED`] yellow, waiting for merge [`WAITING`] blue, sent
//!   back [`SENT_BACK`] red, stopped [`STOPPED`] grey, and every merge onto main [`MAIN_MERGE`]
//!   green; [`NO_TICKET`], a neutral grey for what is no ticket's.
//!
//! All of them are soft pastels (#34, the owner's: "a bit neon"): a ticket's colour at
//! saturation [`SATURATION`] and value [`VALUE`], the fixed colours a little more saturated than
//! that (at most [`MAX_SATURATION`]) so a phase still stands apart from the tickets around it.

use crate::rows::Style;
use crate::tickets::Phase;

/// A colour as red, green and blue bytes (sRGB).
pub type Rgb = [u8; 3];

/// Claimed: a soft yellow (hue 45°, saturation 0.48, value 0.92).
pub const CLAIMED: Rgb = [0xeb, 0xce, 0x7a];
/// Waiting for merge: a soft blue (hue 217°, saturation 0.48, value 0.93).
pub const WAITING: Rgb = [0x7b, 0xa7, 0xed];
/// Sent back: a soft red (hue 358°, saturation 0.46, value 0.91).
pub const SENT_BACK: Rgb = [0xe8, 0x7d, 0x81];
/// Stopped: grey.
pub const STOPPED: Rgb = [0x9a, 0x9a, 0x9a];
/// A merge onto main: a soft green (hue 140°, saturation 0.44, value 0.83), one colour for all of
/// them.
pub const MAIN_MERGE: Rgb = [0x77, 0xd4, 0x96];
/// A neutral grey, for what is no ticket's (a merge onto main that no ticket names).
pub const NO_TICKET: Rgb = [0x7a, 0x7d, 0x86];

/// The fixed colours a ticket's colour keeps clear of.
pub const FIXED: [Rgb; 4] = [CLAIMED, WAITING, SENT_BACK, MAIN_MERGE];
/// How far, in degrees of hue, a ticket's colour keeps from each of [`FIXED`]'s.
pub const CLEARANCE: f64 = 20.0;
/// The arcs of hue, in degrees, a ticket's colour is taken from: the wheel less at least
/// [`CLEARANCE`] around each fixed colour (yellow 45°, green 140°, blue 217°, red 358°; 30°
/// round yellow and blue, which a light tint of a hue next to them looks most like), less the
/// sliver of orange between red and yellow.
const ARCS: [(f64, f64); 3] = [(75.0, 118.0), (162.0, 192.0), (247.0, 336.0)];
/// The golden angle as a fraction of the wheel: 137.5° / 360°.
const GOLDEN: f64 = 0.381_966_011_250_105;
/// A ticket's colour's saturation: a pastel (0.52 before #34).
pub const SATURATION: f64 = 0.34;
/// A ticket's colour's value: a touch under full, so the pastels don't glare (1.0 before #34).
pub const VALUE: f64 = 0.93;
/// The most saturated any of the swarm's colours is: none of them neon (#34).
pub const MAX_SATURATION: f64 = 0.5;

/// A ticket's own colour: hue `issue` × 137.5°, spread over [`ARCS`], so neighbouring ids differ
/// by a third of the hues there are and the same issue is the same colour everywhere.
#[must_use]
pub fn ticket_colour(issue: u64) -> Rgb {
    let turn = ((issue % 1_000_003) as f64 * GOLDEN).fract();
    hsv(hue(turn) / 360.0, SATURATION, VALUE)
}

/// The hue, in degrees, a `turn` (0 to 1) of [`ARCS`] lands on.
fn hue(turn: f64) -> f64 {
    let total: f64 = ARCS.iter().map(|(from, to)| to - from).sum();
    let mut left = turn.clamp(0.0, 1.0) * total;
    for (from, to) in ARCS {
        if left <= to - from {
            return from + left;
        }
        left -= to - from;
    }
    ARCS.last().map_or(0.0, |arc| arc.1)
}

/// The colour a phase of `issue`'s is drawn in: working in the ticket's own, the rest fixed.
#[must_use]
pub fn phase_colour(phase: Phase, issue: u64) -> Rgb {
    match phase {
        Phase::Claimed => CLAIMED,
        Phase::Working => ticket_colour(issue),
        Phase::Waiting => WAITING,
        Phase::SentBack => SENT_BACK,
        Phase::Stopped => STOPPED,
    }
}

/// The colour a bar of `style` is drawn in, `ticket` the ticket it is (none for a merge onto
/// main): a ticket's working and its merge in its own colour, the rest fixed.
#[must_use]
pub fn style_colour(style: Style, ticket: Option<u64>) -> Rgb {
    let own = || ticket.map_or(NO_TICKET, ticket_colour);
    match style {
        Style::MainMerge => MAIN_MERGE,
        Style::Merged | Style::Working => own(),
        Style::Claimed => CLAIMED,
        Style::Waiting => WAITING,
        Style::SentBack => SENT_BACK,
        Style::Stopped => STOPPED,
    }
}

/// Black or white, whichever contrasts more with `fill`: text on it.
#[must_use]
pub fn ink(fill: Rgb) -> Rgb {
    if contrast(fill, BLACK) >= contrast(fill, WHITE) { BLACK } else { WHITE }
}

const BLACK: Rgb = [0x1a, 0x1a, 0x1a];
const WHITE: Rgb = [0xff, 0xff, 0xff];

/// The contrast ratio of two colours, 1 to 21 (WCAG 2's).
#[must_use]
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The relative luminance of `colour`, 0 to 1 (WCAG 2's).
#[must_use]
pub fn luminance(colour: Rgb) -> f64 {
    let linear = |byte: u8| {
        let c = f64::from(byte) / 255.0;
        if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let [r, g, b] = colour;
    0.0722f64.mul_add(linear(b), 0.2126f64.mul_add(linear(r), 0.7152 * linear(g)))
}

/// The hue of `colour`, in degrees (0 for a grey).
#[must_use]
pub fn hue_of(colour: Rgb) -> f64 {
    let [r, g, b] = colour.map(|c| f64::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let span = max - min;
    if span <= f64::EPSILON {
        return 0.0;
    }
    let sector = if (max - r).abs() <= f64::EPSILON {
        ((g - b) / span).rem_euclid(6.0)
    } else if (max - g).abs() <= f64::EPSILON {
        (b - r) / span + 2.0
    } else {
        (r - g) / span + 4.0
    };
    sector * 60.0
}

fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let sector = (h * 6.0).floor();
    let f = h.mul_add(6.0, -sector);
    let (p, q, r) = (v * (1.0 - s), v * f.mul_add(-s, 1.0), v * (1.0 - f).mul_add(-s, 1.0));
    let (red, green, blue) = match sector as i64 % 6 {
        0 => (v, r, p),
        1 => (q, v, p),
        2 => (p, v, r),
        3 => (p, q, v),
        4 => (r, p, v),
        _ => (v, p, q),
    };
    let byte = |x: f64| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(red), byte(green), byte(blue)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distance between two hues, in degrees, round the wheel.
    fn apart(a: f64, b: f64) -> f64 {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    }

    #[test]
    fn a_tickets_colour_is_stable_and_neighbours_differ_clearly() {
        assert_eq!(ticket_colour(170), ticket_colour(170), "the same issue, the same colour");
        for issue in 0..2_000 {
            let (here, next) = (hue_of(ticket_colour(issue)), hue_of(ticket_colour(issue + 1)));
            assert!(apart(here, next) >= 60.0, "#{issue} at {here:.0}° and #{} at {next:.0}° are too alike", issue + 1);
        }
        assert_ne!(ticket_colour(u64::MAX), [0, 0, 0]);
    }

    #[test]
    fn a_tickets_colour_keeps_clear_of_the_fixed_ones() {
        let fixed: Vec<f64> = FIXED.iter().map(|&c| hue_of(c)).collect();
        for issue in 0..2_000 {
            let colour = ticket_colour(issue);
            let hue = hue_of(colour);
            for (fixed, f) in FIXED.iter().zip(&fixed) {
                assert!(apart(hue, *f) >= CLEARANCE - 1.0, "#{issue} at {hue:.0}° is near {fixed:?} at {f:.0}°");
            }
            assert!(!FIXED.contains(&colour) && colour != STOPPED && colour != NO_TICKET);
        }
    }

    #[test]
    fn labels_read_on_every_tickets_colour_and_it_reads_on_the_dark_background() {
        let dark = [0x1b, 0x1b, 0x1b];
        for issue in 0..2_000 {
            let colour = ticket_colour(issue);
            assert!(contrast(colour, ink(colour)) >= 4.5, "#{issue}'s label on {colour:?}: {:.1}", contrast(colour, ink(colour)));
            assert!(contrast(colour, dark) >= 4.5, "#{issue}'s {colour:?} on the background: {:.1}", contrast(colour, dark));
        }
        for fixed in FIXED.into_iter().chain([STOPPED]) {
            assert!(contrast(fixed, ink(fixed)) >= 4.5, "a label on {fixed:?}");
        }
    }

    #[test]
    fn working_and_merges_take_the_tickets_colour_the_rest_are_fixed() {
        assert_eq!(phase_colour(Phase::Working, 58), ticket_colour(58));
        assert_eq!(phase_colour(Phase::Claimed, 58), CLAIMED);
        assert_eq!(phase_colour(Phase::Waiting, 58), WAITING);
        assert_eq!(phase_colour(Phase::SentBack, 58), SENT_BACK);
        assert_eq!(phase_colour(Phase::Stopped, 58), STOPPED);
        assert_eq!(style_colour(Style::Merged, Some(58)), ticket_colour(58));
        assert_eq!(style_colour(Style::Working, Some(58)), phase_colour(Phase::Working, 58));
        assert_eq!(style_colour(Style::MainMerge, Some(58)), MAIN_MERGE);
        assert_eq!(style_colour(Style::MainMerge, None), MAIN_MERGE);
        assert_eq!(style_colour(Style::Merged, None), NO_TICKET);
    }

    /// HSV's saturation and value of `colour`, 0 to 1.
    fn saturation_value(colour: Rgb) -> (f64, f64) {
        let [r, g, b] = colour.map(|c| f64::from(c) / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        (if max <= f64::EPSILON { 0.0 } else { (max - min) / max }, max)
    }

    #[test]
    fn every_colour_is_a_soft_pastel_and_the_phases_stand_apart_from_the_tickets() {
        for issue in 0..2_000 {
            let (s, v) = saturation_value(ticket_colour(issue));
            assert!((s - SATURATION).abs() < 0.01 && (v - VALUE).abs() < 0.01, "#{issue}: saturation {s:.2}, value {v:.2}");
        }
        for fixed in FIXED {
            let (s, v) = saturation_value(fixed);
            assert!(s <= MAX_SATURATION, "{fixed:?} is too saturated: {s:.2}");
            assert!(s >= SATURATION + 0.08, "{fixed:?} at saturation {s:.2} is as pale as a ticket's");
            assert!(v < 0.95, "{fixed:?} at value {v:.2} glares");
        }
        for grey in [STOPPED, NO_TICKET] {
            assert!(saturation_value(grey).0 < 0.1, "{grey:?} is grey");
        }
    }

    #[test]
    fn hues() {
        assert!(apart(hue_of(CLAIMED), 45.0) < 1.0);
        assert!(apart(hue_of(MAIN_MERGE), 140.0) < 1.0);
        assert!(apart(hue_of(WAITING), 217.0) < 1.0);
        assert!(apart(hue_of(SENT_BACK), 358.0) < 1.0);
        assert!(hue_of(STOPPED).abs() < f64::EPSILON);
        assert!((hue(0.0) - 75.0).abs() < 1e-9 && (hue(1.0) - 336.0).abs() < 1e-9);
    }
}
