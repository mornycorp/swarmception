//! The one source of the swarm's colours (#15, the owner's): the timeline and the factory both
//! take them from here, so a ticket and a phase look the same in both. Every one is the
//! [`theme`]'s (#152, the owner's: swarm-core's recommendations, in place of scheme B, #40; the
//! palette the swarm's tools share since swarm-core#100, #158), a [`Paint`]: its colour in each
//! look, dark and light (#154), drawn in the look the window is in.
//!
//! - **A ticket's own colour** ([`ticket_colour`]): its issue number's pick of the theme's
//!   [`theme::RANGE_32`], the range for identifiers side by side (Glasbey et al.'s set, via
//!   colorcet), by the FNV-1a hash of the number, so the same issue is the same colour on every
//!   run, surface and machine. Its bar and its merge are drawn in it.
//! - **The stages, from the tokens named for what they mean**: claimed [`CLAIMED`] (`warning`),
//!   working [`WORKING`] (`strong`), waiting for merge [`WAITING`] (`link`), in the gate
//!   [`GATING`] (Swarmception's `gate`), sent back [`SENT_BACK`] (`error`), stopped [`STOPPED`]
//!   (`muted`), and every merge onto main [`MAIN_MERGE`] (`success`); [`NO_TICKET`] (`muted`) for
//!   what is no ticket's. A stage of a ticket takes its colour from [`crate::rows::Stage::colour`]
//!   alone (#117), wherever it is named.
//!
//! The range is chosen for telling identifiers apart, not for keeping clear of the stages' hues:
//! a ticket may be red or green. Its stages say what happened to it, on its strip and in words.
//!
//! - **Each bar's importance** ([`importance`], #108): zoomed out, the timeline packs a column of
//!   narrow bars into one in the colour of its most important (swarm-timeline's spec §6.1), so
//!   what needs attention survives packing: a ticket's *sent back* stretch [`HIGH`]; a *stopped*
//!   stretch, work that ended unfinished, [`MIDDLE`]; claimed, working and waiting [`LOW`]; the
//!   merges the widget's default, 0, as before.

use crate::rows::Style;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::theme::{self, Look, Range, Token};

/// A colour as red, green and blue bytes (sRGB).
pub type Bytes = [u8; 3];

/// A theme colour as bytes.
#[must_use]
pub const fn bytes(colour: theme::Rgb) -> Bytes {
    [colour.r, colour.g, colour.b]
}

/// One of the swarm's colours in each look (#154): a theme token's dark and light, or a range
/// entry's, so the window draws the one its look takes ([`Paint::of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct Paint {
    /// Its colour in the dark look.
    pub dark: Bytes,
    /// Its colour in the light look.
    pub light: Bytes,
}

impl Paint {
    /// A token's colour in each look.
    #[must_use]
    pub const fn token(token: Token) -> Self {
        Self { dark: bytes(token.dark), light: bytes(token.light) }
    }

    /// `range`'s pick of `id` in each look: the same index in both, so an identifier keeps its
    /// colour family when the look changes.
    #[must_use]
    pub fn pick(range: &Range, id: impl std::fmt::Display + Copy) -> Self {
        Self { dark: bytes(range.dark.pick(id)), light: bytes(range.light.pick(id)) }
    }

    /// `range`'s `n`th entry in each look.
    #[must_use]
    pub fn at(range: &Range, n: u64) -> Self {
        Self { dark: bytes(range.dark.at(n)), light: bytes(range.light.at(n)) }
    }

    /// Its colour in `look`.
    #[must_use]
    pub const fn of(self, look: Look) -> Bytes {
        match look {
            Look::Dark => self.dark,
            Look::Light => self.light,
        }
    }
}

/// Claimed: `warning`, something to look at that hasn't failed.
pub const CLAIMED: Paint = Paint::token(theme::WARNING);
/// Waiting for merge: `link`, the shared palette's blue that reads as text (#158). Its `accent`
/// is a blue too, but one that carries no text (2.72:1 on the dark background), and a stage is
/// named in words as well as drawn.
pub const WAITING: Paint = Paint::token(theme::LINK);
/// Sent back: `error`.
pub const SENT_BACK: Paint = Paint::token(theme::ERROR);
/// Stopped: `muted`.
pub const STOPPED: Paint = Paint::token(theme::MUTED);
/// A merge onto main: `success`, one colour for all of them.
pub const MAIN_MERGE: Paint = Paint::token(theme::SUCCESS);
/// What is no ticket's (a merge onto main that no ticket names): `muted`.
pub const NO_TICKET: Paint = Paint::token(theme::MUTED);
/// Working, on a phase's border and strip (#42, the factory's own, and the timeline's strip since
/// #110): `strong`, so it shows on a bar in its ticket's colour.
pub const WORKING: Paint = Paint::token(theme::STRONG);
/// In a gate run (#42, the factory's own; the timeline's strip since #110): Swarmception's
/// `gate`, a magenta.
pub const GATING: Paint = Paint::token(theme::GATE);
/// An idle bench: Swarmception's `idle`, a quiet sea green.
pub const IDLE: Paint = Paint::token(theme::IDLE);

/// A ticket's own colour: [`theme::RANGE_32`]'s pick of its issue number, so the same issue is
/// the same colour everywhere.
#[must_use]
pub fn ticket_colour(issue: u64) -> Paint {
    Paint::pick(&theme::RANGE_32, issue)
}

/// The `n`th colour of a series in order (the Workers pane's model × effort groups): the theme's
/// [`theme::SERIES`] by number, entry `n % 7` (#158: the shared palette's series in place of range8).
#[must_use]
pub fn series_colour(n: u64) -> Paint {
    Paint::at(&theme::SERIES, n)
}

/// The colour a bar of `style` is drawn in, `ticket` the ticket it is (none for a merge onto
/// main): a ticket's working and its merge in its own colour, the rest fixed.
#[must_use]
pub fn style_colour(style: Style, ticket: Option<u64>) -> Paint {
    let own = || ticket.map_or(NO_TICKET, ticket_colour);
    match style {
        Style::MainMerge => MAIN_MERGE,
        Style::Merged | Style::Working | Style::Ticket => own(),
        Style::Claimed => CLAIMED,
        Style::Waiting => WAITING,
        Style::SentBack => SENT_BACK,
        Style::Stopped => STOPPED,
    }
}

/// What needs attention (#108): a ticket sent back.
pub const HIGH: u8 = 12;
/// What ended unfinished (#108): a ticket stopped.
pub const MIDDLE: u8 = 8;
/// What went as it should (#108): claimed, working and waiting.
pub const LOW: u8 = 4;
/// A merge's: the widget's default, as before #108.
pub const MERGE: u8 = 0;

/// How important a bar of `style` is, 0 to 15: what colour a packed column of the timeline takes,
/// its most important bar's (swarm-timeline's spec §6.1; [`HIGH`], [`MIDDLE`], [`LOW`],
/// [`MERGE`]).
#[must_use]
pub const fn importance(style: Style) -> u8 {
    match style {
        Style::SentBack => HIGH,
        Style::Stopped => MIDDLE,
        // A ticket's bar takes its strip's most important (#110: [`crate::rows`]).
        Style::Claimed | Style::Working | Style::Waiting | Style::Ticket => LOW,
        Style::MainMerge | Style::Merged => MERGE,
    }
}

/// Black or white, whichever contrasts more with `fill`: text on it.
#[must_use]
pub fn ink(fill: Bytes) -> Bytes {
    if contrast(fill, BLACK) >= contrast(fill, WHITE) { BLACK } else { WHITE }
}

const BLACK: Bytes = [0x1a, 0x1a, 0x1a];
const WHITE: Bytes = [0xff, 0xff, 0xff];

/// The contrast ratio of two colours, 1 to 21 (WCAG 2's, as the theme measures it).
#[must_use]
pub fn contrast([r1, g1, b1]: Bytes, [r2, g2, b2]: Bytes) -> f64 {
    theme::Rgb::new(r1, g1, b1).contrast(theme::Rgb::new(r2, g2, b2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOKS: [Look; 2] = [Look::Dark, Look::Light];

    /// #152, #154: a ticket's colour is range32's pick of its number in each look, the same index
    /// in both, and the pick swarm-core pins (FNV-1a over its decimal digits).
    #[test]
    fn a_tickets_colour_is_range32s_pick_of_its_number_in_each_look() {
        assert_eq!(ticket_colour(170), ticket_colour(170), "the same issue, the same colour");
        for issue in [0, 7, 169, 170, 4_000, u64::MAX] {
            for look in LOOKS {
                let range = theme::RANGE_32.of(look);
                assert_eq!(ticket_colour(issue).of(look), bytes(range.pick(issue)), "#{issue} {look:?}");
                assert_eq!(ticket_colour(issue).of(look), bytes(range.entries()[range.index_of(issue)]));
            }
        }
        let used: std::collections::BTreeSet<Paint> = (1..=200).map(ticket_colour).collect();
        assert!(used.len() >= 28, "200 tickets spread over the range: {}", used.len());
    }

    /// #152, #154: every stage's colour is a theme token's in each look, the one named for what it
    /// means, and reads on that look's background; a series is the theme's series in order.
    #[test]
    fn the_stages_are_the_tokens_named_for_what_they_mean() {
        let named = [
            (CLAIMED, theme::WARNING),
            (WAITING, theme::LINK),
            (SENT_BACK, theme::ERROR),
            (STOPPED, theme::MUTED),
            (MAIN_MERGE, theme::SUCCESS),
            (NO_TICKET, theme::MUTED),
            (WORKING, theme::STRONG),
            (GATING, theme::GATE),
            (IDLE, theme::IDLE),
        ];
        for (colour, token) in named {
            for look in LOOKS {
                assert_eq!(colour.of(look), bytes(token.of(look)), "{} {look:?}", token.name);
                let background = bytes(theme::BACKGROUND.of(look));
                assert!(contrast(colour.of(look), background) >= theme::TEXT_CONTRAST, "{} reads on the {look:?} background", token.name);
            }
        }
        assert_eq!(series_colour(0), Paint { dark: bytes(theme::SERIES.dark.at(0)), light: bytes(theme::SERIES.light.at(0)) });
        assert_eq!(series_colour(8), series_colour(1), "a series wraps at 7, as the shared palette's");
    }

    #[test]
    fn a_label_reads_on_every_tickets_colour() {
        for look in LOOKS {
            for &entry in theme::RANGE_32.of(look).entries() {
                let colour = bytes(entry);
                assert!(contrast(colour, ink(colour)) >= theme::FILL_CONTRAST, "the label on {colour:02x?}: {:.1}", contrast(colour, ink(colour)));
            }
        }
    }

    #[test]
    fn a_tickets_bar_and_merge_take_its_colour_the_stages_are_fixed() {
        use crate::rows::Stage;
        let stages = [
            (Stage::Claimed, CLAIMED),
            (Stage::Working, WORKING),
            (Stage::Waiting, WAITING),
            (Stage::Gating, GATING),
            (Stage::SentBack, SENT_BACK),
            (Stage::Stopped, STOPPED),
        ];
        for (stage, colour) in stages {
            assert_eq!(stage.colour(), colour, "{stage:?}");
        }
        assert_eq!(style_colour(Style::Merged, Some(58)), ticket_colour(58));
        assert_eq!(style_colour(Style::Ticket, Some(58)), ticket_colour(58));
        assert_eq!(style_colour(Style::MainMerge, Some(58)), MAIN_MERGE);
        assert_eq!(style_colour(Style::MainMerge, None), MAIN_MERGE);
        assert_eq!(style_colour(Style::Merged, None), NO_TICKET);
    }

    /// #83, the owner's: mc-engine #169 as its tracking branch has it (sent back for a conflict,
    /// fixed and submitted again with no `work` logged), and a ticket sent back then re-claimed
    /// by another worker on a new branch, and #169 with its rework's `work` logged (#98). Every bar, chip, brick, moment and details entry of the
    /// ticket is in its own colour or a stage's fixed one; both its runs of work its strip's
    /// working (#117: the tooltip and the details as the strip).
    #[test]
    fn a_tickets_colour_holds_across_a_send_back_and_a_re_claim() {
        use crate::activity::Activity;
        use crate::details::{Subject, details};
        use crate::factory::{DEFAULT_RECENT, scene};
        use crate::rows::{Grouping, RowsSpec, bars_where, rows};
        use crate::tickets::tests::{RECLAIMED, SENT_BACK_169, files, sent_back_169_logged};
        use swarmception_model::{Issues, Timestamp};

        let fixed = [CLAIMED, WAITING, SENT_BACK, STOPPED];
        let logged = sent_back_169_logged();
        let logged: Vec<(&str, &str)> = logged.iter().map(|(i, t)| (*i, t.as_str())).collect();
        for (fixture, issue, rework_at) in
            [(&SENT_BACK_169[..], 169, "2026-11-01T11:34:00Z"), (&logged[..], 169, "2026-11-01T11:33:30Z"), (&RECLAIMED[..], 7, "2026-11-01T02:10:00Z")]
        {
            let activity = Activity::of(&files(fixture));
            let own = ticket_colour(issue);
            for grouping in [Grouping::Flat, Grouping::Workers] {
                let rows = rows(&activity, &[], RowsSpec { grouping, ..RowsSpec::default() });
                let bars = bars_where(&rows, |_, bar| bar.ticket == Some(issue));
                for (at, bar) in &bars {
                    // #110: one bar a ticket a row, in its own colour all along; its stages fixed
                    // colours (working the factory's warm white) on its strip.
                    match bar.style {
                        Style::Ticket | Style::Merged => assert_eq!(bar.colour, own, "#{issue} {at:?} {:?}", bar.style),
                        _ => panic!("#{issue} {at:?}: a {:?} bar", bar.style),
                    }
                    for stage in &bar.segments {
                        assert!(fixed.contains(&stage.colour) || stage.colour == WORKING || stage.colour == GATING, "#{issue} {stage:?}");
                    }
                    for line in &bar.tooltip_lines {
                        for word in ["working", "began"] {
                            assert!(line.colour_of(word).is_none_or(|c| c == WORKING), "#{issue}'s tooltip's {word} in {:?}", line.colour_of(word));
                        }
                    }
                }
                let working = bars
                    .iter()
                    .filter(|(_, b)| b.style == Style::Ticket)
                    .flat_map(|(_, b)| &b.segments)
                    .filter(|s| s.stage == crate::rows::Stage::Working)
                    .count();
                assert_eq!(working, 2, "#{issue}: both runs of work on its strip ({grouping:?})");
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
            assert_eq!(told, Some((own, vec![WORKING, WORKING])), "#{issue}'s details: its chip in its own colour, both runs of work the strip's");
        }
    }

    /// #108: every kind of bar's importance, as the table says, each one the widget takes.
    #[test]
    fn every_kind_of_bar_has_its_importance() {
        use crate::tickets::Phase;
        use timeline_core::data::Importance;
        let phases = [(Phase::SentBack, HIGH), (Phase::Stopped, MIDDLE), (Phase::Claimed, LOW), (Phase::Working, LOW), (Phase::Waiting, LOW)];
        for (phase, expected) in phases {
            assert_eq!(importance(phase.into()), expected, "{phase:?}");
        }
        assert_eq!(importance(Style::Ticket), LOW, "a ticket's bar takes its strip's most important (rows)");
        assert_eq!(importance(Style::MainMerge), MERGE);
        assert_eq!(importance(Style::Merged), MERGE);
        assert_eq!(MERGE, Importance::default().value(), "merges as before");
        const { assert!(HIGH > MIDDLE && MIDDLE > LOW && LOW > MERGE, "a send-back outranks a stop, a stop the rest") };
        for level in [HIGH, MIDDLE, LOW, MERGE] {
            assert_eq!(Importance::new(level).map(Importance::value), Ok(level), "{level} is the widget's");
        }
    }
}
