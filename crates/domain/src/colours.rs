//! The one source of the swarm's colours (#15, the owner's): the timeline and the factory both
//! take them from here, so a ticket and a phase look the same in both.
//!
//! - **A ticket's own colour** ([`ticket_colour`]): its issue number's entry in [`PALETTE`], a
//!   fixed, hand-picked set of muted colours (#40, the owner's scheme B, Tableau-like), taken in
//!   turn so consecutive ids never share one. Its *working* stretches and its merge are drawn in
//!   it.
//! - **The fixed colours**: claimed [`CLAIMED`] yellow, waiting for merge [`WAITING`] blue, sent
//!   back [`SENT_BACK`] red, stopped [`STOPPED`] grey, and every merge onto main [`MAIN_MERGE`]
//!   green; [`NO_TICKET`], a slate grey for what is no ticket's.
//!
//! The palette leaves the phases' own hues to them: it has no blue, no mid green, no amber and no
//! red, and every entry keeps at least [`CLEARANCE`] of hue and [`DISTANCE`] of colour (OKLab)
//! from each fixed colour, so a ticket never reads as a phase. Every colour takes a dark label at
//! a contrast of 4.5 or more, and reads on the dark background.

use crate::rows::Style;
use crate::tickets::Phase;

/// A colour as red, green and blue bytes (sRGB).
pub type Rgb = [u8; 3];

/// The tickets' colours, in the order consecutive ids take them (#40): each is far from the
/// entries either side of it, warm and cool, light and mid in turn.
pub const PALETTE: [Rgb; 11] = [
    [0x45, 0xa6, 0xab], // teal
    [0xf2, 0x8e, 0x2b], // orange
    [0x98, 0xa2, 0xea], // periwinkle
    [0xa7, 0xbc, 0x61], // olive
    [0xdc, 0x7d, 0x97], // rose
    [0x7c, 0xc6, 0xe6], // sky
    [0xb7, 0x87, 0x5f], // brown
    [0xe9, 0xa0, 0xcf], // pink
    [0x7d, 0xd1, 0xb8], // mint
    [0xab, 0x7c, 0xc7], // purple
    [0xe2, 0xb2, 0x94], // sand
];

/// Claimed: a clear yellow, yellower than the proposal's amber so the palette's orange and sand
/// keep clear of it.
pub const CLAIMED: Rgb = [0xe6, 0xc8, 0x4f];
/// Waiting for merge: a mid blue.
pub const WAITING: Rgb = [0x5b, 0x8e, 0xc9];
/// Sent back: a brick red, light enough for a dark label.
pub const SENT_BACK: Rgb = [0xdc, 0x5f, 0x56];
/// Stopped: grey.
pub const STOPPED: Rgb = [0x8a, 0x8a, 0x8a];
/// A merge onto main: a mid green, one colour for all of them.
pub const MAIN_MERGE: Rgb = [0x5a, 0xa4, 0x69];
/// A slate grey, for what is no ticket's (a merge onto main that no ticket names): bluer than
/// [`STOPPED`].
pub const NO_TICKET: Rgb = [0x7f, 0x87, 0x96];

/// The fixed colours with a hue, which a ticket's colour keeps clear of.
pub const FIXED: [Rgb; 4] = [CLAIMED, WAITING, SENT_BACK, MAIN_MERGE];
/// How far, in degrees of OKLCh hue, a ticket's colour keeps from each of [`FIXED`]'s.
pub const CLEARANCE: f64 = 20.0;
/// How far, in OKLab, a ticket's colour keeps from every fixed colour and every other ticket's.
pub const DISTANCE: f64 = 0.08;
/// How far, in OKLab, consecutive ids' colours are apart at least.
pub const NEIGHBOURS: f64 = 0.15;

/// A ticket's own colour: entry `issue` modulo [`PALETTE`]'s size, so consecutive ids differ and
/// the same issue is the same colour everywhere.
#[must_use]
pub fn ticket_colour(issue: u64) -> Rgb {
    const SIZE: u64 = PALETTE.len() as u64;
    usize::try_from(issue % SIZE).ok().and_then(|i| PALETTE.get(i)).copied().unwrap_or(NO_TICKET)
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

/// sRGB's byte, linear, 0 to 1.
fn linear(byte: u8) -> f64 {
    let c = f64::from(byte) / 255.0;
    if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// The relative luminance of `colour`, 0 to 1 (WCAG 2's).
#[must_use]
pub fn luminance(colour: Rgb) -> f64 {
    let [r, g, b] = colour.map(linear);
    0.0722f64.mul_add(b, 0.2126f64.mul_add(r, 0.7152 * g))
}

/// `colour` in OKLab (Björn Ottosson's): lightness 0 to 1, then the green–red and blue–yellow
/// axes. Distances in it are close to how different two colours look.
#[must_use]
pub fn oklab(colour: Rgb) -> [f64; 3] {
    let [r, g, b] = colour.map(linear);
    let l = 0.051_445_992_9f64.mul_add(b, 0.412_221_470_8f64.mul_add(r, 0.536_332_536_3 * g)).cbrt();
    let m = 0.107_396_956_6f64.mul_add(b, 0.211_903_498_2f64.mul_add(r, 0.680_699_545_1 * g)).cbrt();
    let s = 0.629_978_700_5f64.mul_add(b, 0.088_302_461_9f64.mul_add(r, 0.281_718_837_6 * g)).cbrt();
    [
        (-0.004_072_046_8f64).mul_add(s, 0.210_454_255_3f64.mul_add(l, 0.793_617_785_0 * m)),
        0.450_593_709_9f64.mul_add(s, 1.977_998_495_1f64.mul_add(l, -2.428_592_205_0 * m)),
        (-0.808_675_766_0f64).mul_add(s, 0.025_904_037_1f64.mul_add(l, 0.782_771_766_2 * m)),
    ]
}

/// How different two colours look: their distance in OKLab (0 the same; 0.02 just noticeable).
#[must_use]
pub fn distance(a: Rgb, b: Rgb) -> f64 {
    let ([l1, a1, b1], [l2, a2, b2]) = (oklab(a), oklab(b));
    (l1 - l2).hypot(a1 - a2).hypot(b1 - b2)
}

/// `colour`'s chroma in OKLCh: 0 for a grey, about 0.1 for the palette's muted colours.
#[must_use]
pub fn chroma(colour: Rgb) -> f64 {
    let [_, a, b] = oklab(colour);
    a.hypot(b)
}

/// The hue of `colour` in OKLCh, in degrees (0 for a grey).
#[must_use]
pub fn hue_of(colour: Rgb) -> f64 {
    let [_, a, b] = oklab(colour);
    if a.hypot(b) <= 1e-6 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distance between two hues, in degrees, round the wheel.
    fn apart(a: f64, b: f64) -> f64 {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    }

    const DARK: Rgb = [0x1b, 0x1b, 0x1b];

    #[test]
    fn a_tickets_colour_is_its_ids_palette_entry_and_the_entries_are_distinct() {
        assert!((10..=12).contains(&PALETTE.len()), "enough entries that a packed row rarely repeats one");
        assert_eq!(ticket_colour(170), ticket_colour(170), "the same issue, the same colour");
        for issue in 0..100u64 {
            assert_eq!(ticket_colour(issue), PALETTE[(issue % PALETTE.len() as u64) as usize]);
        }
        assert!(PALETTE.contains(&ticket_colour(u64::MAX)));
        for (i, a) in PALETTE.iter().enumerate() {
            for b in &PALETTE[i + 1..] {
                assert!(distance(*a, *b) >= DISTANCE, "{a:?} and {b:?} are too alike: {:.3}", distance(*a, *b));
            }
        }
    }

    #[test]
    fn consecutive_ids_never_share_a_colour() {
        for issue in 0..2_000 {
            let (here, next) = (ticket_colour(issue), ticket_colour(issue + 1));
            assert!(distance(here, next) >= NEIGHBOURS, "#{issue} {here:?} and #{} {next:?}: {:.3}", issue + 1, distance(here, next));
        }
    }

    #[test]
    fn a_tickets_colour_keeps_clear_of_the_phase_colours() {
        for colour in PALETTE {
            for fixed in FIXED {
                let (h, f) = (hue_of(colour), hue_of(fixed));
                assert!(apart(h, f) >= CLEARANCE, "{colour:?} at {h:.0}° is near {fixed:?} at {f:.0}°");
            }
            for fixed in FIXED.into_iter().chain([STOPPED, NO_TICKET]) {
                assert!(distance(colour, fixed) >= DISTANCE, "{colour:?} looks like {fixed:?}: {:.3}", distance(colour, fixed));
            }
            assert!(chroma(colour) >= 0.06, "{colour:?} reads as a grey: chroma {:.3}", chroma(colour));
        }
    }

    #[test]
    fn labels_read_on_every_colour_and_every_colour_reads_on_the_dark_background() {
        for colour in PALETTE.into_iter().chain(FIXED).chain([STOPPED, NO_TICKET]) {
            assert_eq!(ink(colour), BLACK, "a dark label on {colour:?}");
            assert!(contrast(colour, ink(colour)) >= 4.5, "the label on {colour:?}: {:.1}", contrast(colour, ink(colour)));
            assert!(contrast(colour, DARK) >= 4.5, "{colour:?} on the background: {:.1}", contrast(colour, DARK));
        }
    }

    #[test]
    fn every_colour_is_muted_not_neon() {
        for colour in PALETTE.into_iter().chain(FIXED) {
            let [l, ..] = oklab(colour);
            assert!(chroma(colour) <= 0.17, "{colour:?} is neon: chroma {:.3}", chroma(colour));
            assert!((0.6..=0.86).contains(&l), "{colour:?} at lightness {l:.2} is too dark or glares");
        }
        for grey in [STOPPED, NO_TICKET] {
            assert!(chroma(grey) < 0.03, "{grey:?} is grey");
        }
        assert!(distance(STOPPED, NO_TICKET) > 0.02, "no ticket's slate is not stopped's grey");
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

    /// #83, the owner's: mc-engine #169 as its tracking branch has it (sent back for a conflict,
    /// fixed and submitted again with no `work` logged), and a ticket sent back then re-claimed
    /// by another worker on a new branch. Every bar, chip, brick, moment and details entry of the
    /// ticket is in its own colour or a phase's fixed one, and both its runs of work in its own.
    #[test]
    fn a_tickets_colour_holds_across_a_send_back_and_a_re_claim() {
        use crate::activity::Activity;
        use crate::details::{Subject, details};
        use crate::factory::{DEFAULT_RECENT, scene};
        use crate::rows::{Grouping, RowsSpec, bars_where, rows};
        use crate::tickets::tests::{RECLAIMED, SENT_BACK_169, files};
        use swarmception_model::{Issues, Timestamp};

        let fixed = [CLAIMED, WAITING, SENT_BACK, STOPPED];
        for (fixture, issue, rework_at) in [(&SENT_BACK_169[..], 169, "2026-11-01T11:34:00Z"), (&RECLAIMED[..], 7, "2026-11-01T02:10:00Z")] {
            let activity = Activity::of(&files(fixture));
            let own = ticket_colour(issue);
            for grouping in [Grouping::Flat, Grouping::Workers] {
                let rows = rows(&activity, &[], RowsSpec { grouping, ..RowsSpec::default() });
                let bars = bars_where(&rows, |_, bar| bar.ticket == Some(issue));
                for (at, bar) in &bars {
                    match bar.style {
                        Style::Working | Style::Merged => assert_eq!(bar.colour, own, "#{issue} {at:?} {:?}", bar.style),
                        _ => assert!(fixed.contains(&bar.colour), "#{issue} {at:?} {:?} in {:?}", bar.style, bar.colour),
                    }
                    for line in &bar.tooltip_lines {
                        for word in ["working", "began"] {
                            assert!(line.colour_of(word).is_none_or(|c| c == own), "#{issue}'s tooltip's {word} in {:?}", line.colour_of(word));
                        }
                    }
                }
                let working: Vec<_> = bars.iter().filter(|(_, b)| b.style == Style::Working).map(|(_, b)| b.colour).collect();
                assert_eq!(working, [own, own], "#{issue}: both runs of work, in its colour ({grouping:?})");
                assert_eq!(bars.iter().filter(|(_, b)| b.style == Style::Merged).count(), 1);
            }

            let (first, last) = activity.extent.unwrap_or_default();
            for t in (first.0..=last.0 + 60).step_by(60).map(Timestamp) {
                let scene = scene(&activity, &[], t, DEFAULT_RECENT);
                let crates = scene.upcoming.iter().chain(&scene.shelf).chain(&scene.dock).chain(scene.benches.iter().flat_map(|b| &b.crates));
                for chip in crates.filter(|c| c.issue == issue) {
                    assert_eq!(chip.colour, own, "#{issue}'s chip at {t} ({:?})", chip.state);
                }
                for block in scene.shipping.blocks.iter().filter(|b| b.issue == Some(issue)) {
                    assert_eq!(block.colour, own, "#{issue}'s brick at {t}");
                }
                for happening in scene.happenings.iter().filter(|h| h.issue == Some(issue)) {
                    assert_eq!(happening.colour, Some(own), "#{issue}'s happening at {t}: {}", happening.text);
                }
            }
            let at = Timestamp::parse(rework_at).unwrap_or_default();
            let bench = scene(&activity, &[], at, DEFAULT_RECENT).benches.into_iter().flat_map(|b| b.crates).find(|c| c.issue == issue);
            assert_eq!(bench.map(|c| (c.state, c.colour)), Some((crate::factory::CrateState::Working, own)), "#{issue} at work again at {rework_at}");
            for moment in crate::moments::moments(&activity, &[]).iter().filter(|m| m.issue == Some(issue)) {
                assert_eq!(moment.colour, own, "{}", moment.text);
            }

            let told = details(&activity, &[], &Issues::default(), &Subject::Ticket { issue });
            let told = told.as_ref().map(|d| (d.colour, d.entries.iter().filter(|e| e.what == "working").map(|e| e.colour).collect::<Vec<_>>()));
            assert_eq!(told, Some((own, vec![own, own])), "#{issue}'s details: its chip and both runs");
        }
    }

    #[test]
    fn the_phases_keep_their_hues() {
        let within = |colour: Rgb, from: f64, to: f64| (from..=to).contains(&hue_of(colour));
        assert!(within(CLAIMED, 85.0, 110.0), "claimed is yellow: {:.0}°", hue_of(CLAIMED));
        assert!(within(MAIN_MERGE, 135.0, 160.0), "merges are green: {:.0}°", hue_of(MAIN_MERGE));
        assert!(within(WAITING, 240.0, 265.0), "waiting is blue: {:.0}°", hue_of(WAITING));
        assert!(within(SENT_BACK, 15.0, 40.0), "sent back is red: {:.0}°", hue_of(SENT_BACK));
        assert!(hue_of(STOPPED).abs() < f64::EPSILON);
        let [l, a, b] = oklab([0xff, 0xff, 0xff]);
        assert!((l - 1.0).abs() < 1e-3 && a.abs() < 1e-3 && b.abs() < 1e-3, "white is OKLab's white");
    }
}
