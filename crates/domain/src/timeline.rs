//! The timeline widget's host data (swarm-timeline's docs/spec.md §3) built from the rows: a
//! track per row (a group, or a Ranges row of bars, each range in its bar's colour), and a `Time`
//! scale. The axis's items are the distinct times the bars name, in order; item `i` spans
//! from its time to the next one's, so a bar is the run of items from its start up to its end,
//! and the Unit scale (every item the same width) reads as event order with idle time cut out.
//! Positions are nanoseconds from [`Built::origin`], the first time floored to the hour, so the
//! ruler's hour ticks fall on the hours of the clock; the data's clock epoch is that origin, so a
//! Clock ruler reads the `Time` scale as the time of day (UTC). Each range carries its bar's
//! importance (#108), so a packed column of narrow bars shows its failure, not its first pass.

use std::collections::BTreeMap;

use timeline_core::axis::Position;
use timeline_core::data::InstantAt;
use timeline_core::data::{
    CategoryId, Colour, DataError, Importance, Instant, Item, ItemIndex, MemoryData, Range, RangeExtent, RangeFlags, TimelineData, TooltipLine, TooltipSpan,
    TrackId, TrackKind, TrackTree,
};
use timeline_core::marks::{Glyph, Mark, MarkEdge};
use timeline_core::scale::{Availability, NamedScale, Scale};
use timeline_core::strip::Segment;

use swarmception_model::Timestamp;

use crate::colours::Rgb;
use crate::rows::{Bar, MarkKind, Node};
use crate::tooltip::TipLine;

/// The time scale's name.
pub const TIME: &str = "Time";
/// How long the last item lasts: past the last event, so a bar ending there shows.
const LAST_ITEM_SECONDS: i64 = 60;
const NANOS: i64 = 1_000_000_000;

/// A bar's colour as the widget's.
#[must_use]
pub const fn colour([r, g, b]: Rgb) -> Colour {
    Colour::rgb(r, g, b)
}

/// The rows as the widget's data, with their tooltips and their context menus' pages.
#[derive(Debug, Clone, Default)]
pub struct SwarmData {
    data: MemoryData,
    /// Position 0 of the `Time` scale, in nanoseconds from the Unix epoch.
    clock_epoch: Option<i64>,
    /// Each bar's tooltip lines, styled (#35).
    range_tips: BTreeMap<(TrackId, usize), Vec<TipLine>>,
    range_links: BTreeMap<(TrackId, usize), Vec<Link>>,
}

/// A page on GitHub a bar's context menu opens (#17): its ticket's issue, or a pull request (its
/// ticket's, or a merge onto main's). Its [`Link::id`] is the widget's host item id, which the
/// front end reads back with [`Link::parse`] and opens through `ticket.urls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Link {
    Issue(u64),
    Pull(u64),
}

impl Link {
    const ISSUE: &'static str = "github.issue:";
    const PULL: &'static str = "github.pull:";

    /// Its host item id: `github.issue:7`, `github.pull:8`.
    #[must_use]
    pub fn id(self) -> String {
        match self {
            Self::Issue(n) => format!("{}{n}", Self::ISSUE),
            Self::Pull(n) => format!("{}{n}", Self::PULL),
        }
    }

    /// The link a host item id names; none for another id.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        let number = |rest: &str| rest.parse::<u64>().ok().filter(|n| *n > 0);
        id.strip_prefix(Self::ISSUE).and_then(number).map(Self::Issue).or_else(|| id.strip_prefix(Self::PULL).and_then(number).map(Self::Pull))
    }

    /// What its menu item says.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Issue(n) => format!("Open Issue #{n}"),
            Self::Pull(n) => format!("Open PR #{n}"),
        }
    }

    /// The links of `bar`: its ticket's issue, then its pull request.
    #[must_use]
    pub fn of(bar: &Bar) -> Vec<Self> {
        bar.ticket.map(Self::Issue).into_iter().chain(bar.pr.filter(|pr| Some(*pr) != bar.ticket).map(Self::Pull)).collect()
    }
}

/// A tooltip line as the widget's: each run in its colour (the theme's text colour with none),
/// strong or not.
#[must_use]
pub fn tooltip_line(line: &TipLine) -> TooltipLine {
    TooltipLine { spans: line.spans.iter().map(|span| TooltipSpan { text: span.text.clone(), colour: span.colour.map(colour), strong: span.strong }).collect() }
}

/// What [`build`] makes: the data, its time scale and where its positions count from.
#[derive(Debug, Clone)]
pub struct Built {
    pub data: SwarmData,
    /// The `Time` scale, beside the widget's own Unit.
    pub scales: Vec<NamedScale>,
    /// Position 0: the first time, floored to the hour.
    pub origin: Timestamp,
    /// Each item's time, in order.
    pub times: Vec<Timestamp>,
}

impl Built {
    /// The time at a position on the `Time` scale.
    #[must_use]
    pub const fn time_at(&self, position: Position) -> Timestamp {
        Timestamp(self.origin.0.saturating_add(position.div_euclid(NANOS)))
    }
}

/// The widget's data for `rows`; `show_empty` shows rows with nothing in them too.
///
/// # Errors
///
/// Core's refusal of the data, which these rows never cause.
pub fn build(rows: &[Node], show_empty: bool) -> Result<Built, DataError> {
    let mut times: Vec<Timestamp> = Vec::new();
    for leaf in rows.iter().flat_map(Node::leaves) {
        if let Node::Ranges { bars, .. } = leaf {
            times.extend(bars.iter().flat_map(|b| [b.start, b.end]));
            // A ticket's strip's stages and its marks (#110) fall between its ends.
            times.extend(bars.iter().flat_map(|b| b.segments.iter().flat_map(|s| [s.start, s.end])));
            times.extend(bars.iter().flat_map(|b| b.marks.iter().map(|m| m.t)));
        }
    }
    times.sort_unstable();
    times.dedup();
    let origin = Timestamp(times.first().map_or(0, |t| t.0.div_euclid(3_600) * 3_600));
    let mut built = SwarmData { clock_epoch: Some(origin.0.saturating_mul(NANOS)), ..SwarmData::default() };
    let category = built.data.add_category(Colour::rgb(0x80, 0x80, 0x80))?;
    for _ in &times {
        built.data.push_item(Item { category, importance: Importance::default(), selectable: true })?;
    }
    let index: BTreeMap<Timestamp, ItemIndex> = times.iter().copied().zip(0..).collect();
    let mut adder = Adder { built: &mut built, index: &index, show_empty };
    for node in rows {
        adder.add(None, node)?;
    }
    let position = |t: Timestamp| t.0.saturating_sub(origin.0).saturating_mul(NANOS);
    let spans = times.iter().enumerate().map(|(i, t)| {
        let end = times.get(i + 1).map_or(t.0.saturating_add(LAST_ITEM_SECONDS), |next| next.0);
        (position(*t), position(Timestamp(end)))
    });
    let scales = match Scale::spans(spans) {
        Ok(scale) => vec![NamedScale { name: TIME.to_owned(), availability: Availability::Available(scale) }],
        Err(error) => vec![NamedScale { name: TIME.to_owned(), availability: Availability::Unavailable { reason: error.to_string() } }],
    };
    Ok(Built { data: built, scales, origin, times })
}

struct Adder<'a> {
    built: &'a mut SwarmData,
    index: &'a BTreeMap<Timestamp, ItemIndex>,
    show_empty: bool,
}

impl Adder<'_> {
    fn add(&mut self, parent: Option<TrackId>, node: &Node) -> Result<(), DataError> {
        let data = &mut self.built.data;
        match node {
            Node::Group { key, name, children } => {
                let id = data.add_track(parent, key.clone(), TrackKind::Group, name.clone())?;
                if self.show_empty {
                    data.set_show_when_empty(id, true)?;
                }
                for child in children {
                    self.add(Some(id), child)?;
                }
            }
            Node::Ranges { key, name, bars } => {
                let id = data.add_track(parent, key.clone(), TrackKind::Ranges, name.clone())?;
                if self.show_empty {
                    data.set_show_when_empty(id, true)?;
                }
                for (place, bar) in bars.iter().enumerate() {
                    self.bar(id, place, bar)?;
                }
            }
        }
        Ok(())
    }

    fn item(&self, t: Timestamp) -> ItemIndex {
        self.index.get(&t).copied().unwrap_or_default()
    }

    /// A bar is the run of items from its start's up to the one before its end's (which ends at
    /// its end), and at least its start's.
    fn bar(&mut self, track: TrackId, place: usize, bar: &Bar) -> Result<(), DataError> {
        let first = self.item(bar.start);
        let last = self.item(bar.end).saturating_sub(1).max(first);
        let range = Range {
            name: bar.label.clone(),
            extent: RangeExtent::Items { first, last },
            colour: Some(colour(bar.colour)),
            lane: None,
            importance: Importance::new(bar.importance).unwrap_or(Importance::MAXIMUM),
        };
        self.built.data.add_range(track, range)?;
        if bar.open {
            self.built.data.set_range_flags(track, place, RangeFlags { open_ended: true, ..RangeFlags::default() })?;
        }
        self.strip(track, place, bar, (first, last))?;
        for mark in &bar.marks {
            let at = InstantAt::Item(self.item(mark.t));
            let (glyph, edge, importance) = match mark.kind {
                MarkKind::SentBack => (Glyph::Diamond, MarkEdge::Bottom, crate::colours::HIGH),
                MarkKind::Merged => (Glyph::Dot, MarkEdge::Middle, crate::colours::LOW),
            };
            let importance = Importance::new(importance).unwrap_or(Importance::MAXIMUM);
            self.built
                .data
                .add_mark(Mark::dot(track, at, colour(mark.colour), mark.name.clone()).with_glyph(glyph).with_edge(edge).with_importance(importance))?;
        }
        self.built.range_tips.insert((track, place), bar.tooltip_lines.clone());
        let links = Link::of(bar);
        if !links.is_empty() {
            self.built.range_links.insert((track, place), links);
        }
        Ok(())
    }
}

impl Adder<'_> {
    /// A ticket's bar's strip (#110): each stage the run of items from its start's up to the one
    /// before its end's, within the bar's `first..=last`; a stage that would overlap the one
    /// before (one of no items) is left out, so the widget takes them.
    fn strip(&mut self, track: TrackId, place: usize, bar: &Bar, (first, last): (ItemIndex, ItemIndex)) -> Result<(), DataError> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut next = first;
        for stage in &bar.segments {
            let start = self.item(stage.start).max(next);
            let end = self.item(stage.end).saturating_sub(1).min(last);
            if end < start {
                continue;
            }
            let importance = Importance::new(stage.importance).unwrap_or(Importance::MAXIMUM);
            // The widget's tooltip line adds its duration on the `Time` scale.
            segments.push(
                Segment::new(RangeExtent::Items { first: start, last: end }, colour(stage.colour)).with_label(stage.stage.name()).with_importance(importance),
            );
            next = end + 1;
        }
        if segments.is_empty() { Ok(()) } else { self.built.data.set_range_segments(track, place, segments) }
    }
}

impl TimelineData for SwarmData {
    fn tracks(&self) -> &TrackTree {
        self.data.tracks()
    }

    fn item_count(&self) -> u64 {
        self.data.item_count()
    }

    fn item(&self, index: ItemIndex) -> Result<Item, DataError> {
        self.data.item(index)
    }

    fn track_items(&self, track: TrackId) -> Result<&[ItemIndex], DataError> {
        self.data.track_items(track)
    }

    fn ranges(&self, track: TrackId) -> Result<&[Range], DataError> {
        self.data.ranges(track)
    }

    fn instants(&self, track: TrackId) -> Result<&[Instant], DataError> {
        self.data.instants(track)
    }

    fn category_colour(&self, category: CategoryId) -> Result<Colour, DataError> {
        self.data.category_colour(category)
    }

    fn range_tooltip(&self, track: TrackId, place: usize) -> Vec<String> {
        self.range_tips.get(&(track, place)).map_or_else(|| self.data.range_tooltip(track, place), |lines| lines.iter().map(TipLine::text).collect())
    }

    fn range_tooltip_lines(&self, track: TrackId, place: usize) -> Vec<TooltipLine> {
        self.range_tips.get(&(track, place)).map_or_else(|| self.data.range_tooltip_lines(track, place), |lines| lines.iter().map(tooltip_line).collect())
    }

    fn instant_tooltip(&self, track: TrackId, place: usize) -> Vec<String> {
        self.data.instant_tooltip(track, place)
    }

    fn range_flags(&self, track: TrackId, place: usize) -> RangeFlags {
        self.data.range_flags(track, place)
    }

    fn range_segments(&self, track: TrackId, place: usize) -> &[Segment] {
        self.data.range_segments(track, place)
    }

    fn marks(&self) -> &[Mark] {
        self.data.marks()
    }

    fn menu_items(&self, track: Option<TrackId>, range: Option<usize>, _item: Option<ItemIndex>) -> Vec<(String, String)> {
        track.zip(range).and_then(|key| self.range_links.get(&key)).map(|links| links.iter().map(|l| (l.label(), l.id())).collect()).unwrap_or_default()
    }

    fn show_when_empty(&self, track: TrackId) -> bool {
        self.data.show_when_empty(track)
    }

    fn clock_epoch(&self) -> Option<i64> {
        self.clock_epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::Activity;
    use crate::activity::tests::sample;
    use crate::rows::{RowsSpec, rows};

    #[test]
    fn every_row_is_a_track_and_every_time_an_item() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec::default());
        let built = build(&rows, true).unwrap();
        assert_eq!(built.data.tracks().len(), 4 + 2, "the four top rows (#84: the gate's two), and the tickets' two");
        assert_eq!(built.times.len() as u64, built.data.item_count());
        assert_eq!(built.origin.to_string(), "2026-10-31T03:00:00Z");
        assert_eq!(built.data.clock_epoch(), Some(built.origin.0 * 1_000_000_000), "position 0 is the origin, as a Clock ruler reads it");
        assert!(built.scales[0].scale().is_some());
    }

    #[test]
    fn a_bar_ends_at_its_end() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let scale = built.scales[0].scale().unwrap().clone();
        let tree = built.data.tracks();
        let ticket = tree.ids().find(|&id| tree.get(id).unwrap().key() == "tickets/2").unwrap();
        let ranges = built.data.ranges(ticket).unwrap();
        let RangeExtent::Items { first, last } = ranges[0].extent else { panic!() };
        let span = scale.range_span(first, last).unwrap();
        assert_eq!(built.time_at(span.start()).to_string(), "2026-10-31T03:53:22Z", "from its claim");
        assert_eq!(built.time_at(span.end()).to_string(), "2026-10-31T04:28:58Z", "up to its merge");
        assert_eq!(built.data.range_tooltip(ticket, 0)[0], "#170 · PR #173");
        // #110: its stages its strip's segments, each from its start up to its end, in its colour.
        let time = |extent: RangeExtent| {
            let RangeExtent::Items { first, last } = extent else { panic!() };
            let span = scale.range_span(first, last).unwrap();
            (built.time_at(span.start()).to_string(), built.time_at(span.end()).to_string())
        };
        let strip = built.data.range_segments(ticket, 0);
        let stages: Vec<(Option<&str>, (String, String))> = strip.iter().map(|s| (s.label.as_deref(), time(s.extent))).collect();
        assert_eq!(stages[1], (Some("working"), ("2026-10-31T03:53:25Z".to_owned(), "2026-10-31T04:06:43Z".to_owned())));
        assert_eq!(stages.iter().map(|(label, _)| label.unwrap_or_default()).collect::<Vec<_>>(), ["claimed", "working", "waiting for merge", "in the gate"]);
        assert_eq!(strip[1].colour, colour(crate::colours::WORKING));
        assert!(timeline_core::strip::in_order(strip));
        // Its merge, a green dot at its end.
        let marks: Vec<_> = built.data.marks().iter().filter(|m| m.track == ticket).collect();
        assert_eq!(marks.len(), 1);
        assert_eq!((marks[0].drawn_as(), marks[0].colour), (timeline_core::marks::Glyph::Dot, colour(crate::tooltip::PASSED)));
        let timeline_core::marks::MarkShape::Dot(InstantAt::Item(at)) = marks[0].shape else { panic!() };
        assert_eq!(built.time_at(scale.span(at).unwrap().start()).to_string(), "2026-10-31T04:28:58Z");
    }

    #[test]
    fn a_bars_tooltip_reaches_the_widget_in_its_colours() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let tree = built.data.tracks();
        let ticket = tree.ids().find(|&id| tree.get(id).unwrap().key() == "tickets/2").unwrap();
        let lines = built.data.range_tooltip_lines(ticket, 0);
        let plain = built.data.range_tooltip(ticket, 0);
        assert_eq!(lines.iter().map(TooltipLine::text).collect::<Vec<_>>(), plain, "the same words");
        assert!(lines[0].spans[0].strong, "the title strong");
        let working = &lines[2].spans[0];
        assert_eq!((working.text.as_str(), working.colour), ("working", Some(colour(crate::colours::ticket_colour(170)))));
        assert!(lines[2].spans.iter().any(|s| s.strong && s.text == "13 min 18 s"));
        assert!(lines.iter().any(|l| l.spans.first().is_some_and(|s| s.text == "merged" && s.colour == Some(colour(crate::tooltip::PASSED)))));
    }

    #[test]
    fn each_range_is_drawn_in_its_bars_colour_and_a_merge_spans_ten_seconds() {
        use swarmception_model::{MainMerge, parse_merge};
        let activity = Activity::of(&sample());
        let merges: Vec<MainMerge> = parse_merge("be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec").into_iter().collect();
        let rows = rows(&activity, &merges, RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let scale = built.scales[0].scale().unwrap().clone();
        let tree = built.data.tracks();
        for leaf in rows.iter().flat_map(Node::leaves) {
            let Node::Ranges { key, bars, .. } = leaf else { continue };
            let track = tree.ids().find(|&id| tree.get(id).unwrap().key() == key).unwrap();
            let ranges = built.data.ranges(track).unwrap();
            assert_eq!(ranges.iter().map(|r| r.colour).collect::<Vec<_>>(), bars.iter().map(|b| Some(colour(b.colour))).collect::<Vec<_>>(), "{key}");
            if key == "main" || key == "merged" {
                let RangeExtent::Items { first, last } = ranges[0].extent else { panic!() };
                let span = scale.range_span(first, last).unwrap();
                assert_eq!(built.time_at(span.end()).0 - built.time_at(span.start()).0, 10, "{key}: a 10 s span");
            }
        }
    }

    #[test]
    fn a_bars_menu_opens_its_issue_and_its_pr() {
        use swarmception_model::{MainMerge, parse_merge};
        let activity = Activity::of(&sample());
        let merges: Vec<MainMerge> = parse_merge("be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec").into_iter().collect();
        let rows = rows(&activity, &merges, RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let tree = built.data.tracks();
        let track = |key: &str| tree.ids().find(|&id| tree.get(id).unwrap().key() == key).unwrap();
        let items = built.data.menu_items(Some(track("tickets/2")), Some(0), None);
        assert_eq!(items, [("Open Issue #170".to_owned(), "github.issue:170".to_owned()), ("Open PR #173".to_owned(), "github.pull:173".to_owned())]);
        assert_eq!(built.data.menu_items(Some(track("main")), Some(0), None), [("Open PR #173".to_owned(), "github.pull:173".to_owned())]);
        assert!(built.data.menu_items(Some(track("main")), None, None).is_empty(), "a row's own menu has none");
        assert!(built.data.menu_items(None, None, None).is_empty());
        for (_, id) in items {
            assert_eq!(Link::parse(&id).map(Link::id), Some(id));
        }
        assert_eq!(Link::parse("github.issue:0"), None);
        assert_eq!(Link::parse("github.pull:x"), None);
        assert_eq!(Link::parse("other:1"), None);
    }

    /// Sixty tickets, one after another, each claimed, worked on, `ready`-checked, submitted and
    /// gated, a ticket every ten minutes; #17's ready check fails and is rerun, and its gate
    /// fails and sends it back, before it is worked on again and a rerun merges it.
    fn sixty_tickets() -> Vec<swarmception_model::AgentFile> {
        let base = 1_793_404_800; // 2026-10-31T00:00:00Z
        let line = |at: i64, body: &str| format!("{{\"t\":\"{}\",{body}}}", Timestamp(base + at));
        let mut worker = vec![line(0, r#""event":"session.start","machine":"m","name":"worker-1","role":"worker""#)];
        let mut coordinator = vec![line(0, r#""event":"session.start","machine":"m","name":"coordinator","role":"coordinator""#)];
        for i in 1..=60 {
            let (o, pr, bad) = (i * 600, 100 + i, i == 17);
            worker.push(line(o, &format!(r#""event":"claim","issue":{i},"mr":{pr}"#)));
            worker.push(line(o + 10, &format!(r#""event":"work","issue":{i}"#)));
            worker.push(line(o + 100, &format!(r#""event":"sub.start","name":"ready","sub":"ready-{i}""#)));
            worker.push(line(o + 160, &format!(r#""event":"sub.stop","note":"rc={}","sub":"ready-{i}""#, u8::from(bad))));
            if bad {
                worker.push(line(o + 170, &format!(r#""event":"sub.start","name":"ready","sub":"ready-{i}b""#)));
                worker.push(line(o + 200, &format!(r#""event":"sub.stop","note":"rc=0","sub":"ready-{i}b""#)));
            }
            worker.push(line(o + 220, &format!(r#""event":"submit","issue":{i},"mr":{pr}"#)));
            coordinator.push(line(o + 300, &format!(r#""event":"sub.start","name":"gate","note":"local gate: #{pr} (#{i})","sub":"gate-{i}""#)));
            if bad {
                coordinator.push(line(o + 355, &format!(r#""event":"sendback","issue":{i},"mr":{pr},"note":"cause=test","sub":"gate-{i}""#)));
                coordinator.push(line(o + 360, &format!(r#""event":"sub.stop","note":"rc=1; cause=test","sub":"gate-{i}""#)));
                worker.push(line(o + 400, &format!(r#""event":"work","issue":{i}"#)));
                worker.push(line(o + 450, &format!(r#""event":"submit","issue":{i},"mr":{pr}"#)));
                coordinator.push(line(o + 500, &format!(r#""event":"sub.start","name":"gate","note":"local gate: #{pr} (#{i})","sub":"gate-{i}b""#)));
                coordinator.push(line(o + 550, &format!(r#""event":"merge","issue":{i},"mr":{pr},"sub":"gate-{i}b""#)));
                coordinator.push(line(o + 560, &format!(r#""event":"sub.stop","note":"rc=0","sub":"gate-{i}b""#)));
            } else {
                coordinator.push(line(o + 350, &format!(r#""event":"merge","issue":{i},"mr":{pr},"sub":"gate-{i}""#)));
                coordinator.push(line(o + 360, &format!(r#""event":"sub.stop","note":"rc=0","sub":"gate-{i}""#)));
            }
        }
        [("m-w", worker), ("m-c", coordinator)].iter().map(|(id, lines)| swarmception_model::parse_file(id, &lines.join("\n"))).collect()
    }

    /// #108: every range carries its bar's importance.
    #[test]
    fn each_range_carries_its_bars_importance() {
        use crate::colours::{HIGH, LOW, MERGE};
        let activity = Activity::of(&sixty_tickets());
        let rows = rows(&activity, &[], RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let tree = built.data.tracks();
        let mut levels = std::collections::BTreeSet::new();
        for leaf in rows.iter().flat_map(Node::leaves) {
            let Node::Ranges { key, bars, .. } = leaf else { continue };
            let Some(track) = tree.ids().find(|&id| tree.get(id).unwrap().key() == key) else { continue };
            let ranges = built.data.ranges(track).unwrap();
            let importances: Vec<u8> = ranges.iter().map(|r| r.importance.value()).collect();
            assert_eq!(importances, bars.iter().map(|b| b.importance).collect::<Vec<_>>(), "{key}");
            levels.extend(importances);
        }
        assert_eq!(levels.into_iter().collect::<Vec<_>>(), [MERGE, LOW, HIGH], "the merges, the passes and the work, the failures and the send back");
    }

    /// #108, the issue's own case: zoomed out to the whole run, forty items a pixel column, the
    /// failed gate run and the failed ready check each show red in a packed column, though
    /// neither is its column's first range (which, before #108, gave it its colour). The send
    /// back (#110: a ticket's bar in its own colour, its stages a strip) takes its packed column
    /// for its ticket, and its red mark survives the marks' packing.
    #[test]
    fn a_failure_survives_packing() {
        use crate::colours::SENT_BACK;
        use crate::tooltip::FAILED;
        use timeline_core::axis::{Axis, Domain, PixelMap, Span};
        use timeline_core::range_lod::{RangeIndex, range_bars};

        let activity = Activity::of(&sixty_tickets());
        let rows = rows(&activity, &[], RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let count = built.data.item_count();
        let scale = Scale::unit(count).unwrap();
        let end = i64::try_from(count).unwrap();
        let map = PixelMap::new(Axis::new(Domain::Index, Span::new(0, end).unwrap()).window(0, end).unwrap(), count as f32 / 40.0).unwrap();
        let tree = built.data.tracks();
        for (key, red) in [("gates", FAILED), ("ready", FAILED)] {
            let track = tree.ids().find(|&id| tree.get(id).unwrap().key() == key).unwrap();
            let bars = range_bars(&built.data, track, &scale, &RangeIndex::new(&built.data, track).unwrap(), &map).unwrap();
            assert!(bars.iter().all(|b| b.packed), "{key}: zoomed out, every column packs: {bars:?}");
            let shown: Vec<_> = bars.iter().filter(|b| b.colour == Some(colour(red))).collect();
            assert_eq!(shown.len(), 1, "{key}: one red column, the failure's: {bars:?}");
            assert_ne!(shown[0].range, shown[0].shown, "{key}: not its column's first range, so only its importance shows it");
        }
        let track = tree.ids().find(|&id| tree.get(id).unwrap().key() == "tickets/1").unwrap();
        let bars = range_bars(&built.data, track, &scale, &RangeIndex::new(&built.data, track).unwrap(), &map).unwrap();
        let ranges = built.data.ranges(track).unwrap();
        let sent: Vec<usize> = (0..ranges.len()).filter(|&i| ranges[i].importance.value() == crate::colours::HIGH).collect();
        assert_eq!(sent.len(), 1, "one ticket sent back");
        let column = bars.iter().find(|b| b.range <= sent[0] && sent[0] < b.range + b.count).unwrap();
        assert_eq!(column.shown, sent[0], "its ticket shows for its column");
        assert_ne!(column.range, column.shown, "not its column's first");
        let marks = built.data.marks();
        let index = timeline_core::marks::MarkIndex::new(marks, &scale);
        let drawn = timeline_core::marks::draw(marks, &index, track, &scale, &map);
        assert!(drawn.len() < marks.iter().filter(|m| m.track == track).count(), "packed");
        let red: Vec<_> = drawn.iter().filter(|m| m.colour == colour(SENT_BACK)).collect();
        assert_eq!(red.len(), 1, "the send back's mark stands for its neighbours: {drawn:?}");
    }

    #[test]
    fn no_rows_no_items() {
        let built = build(&[], false).unwrap();
        assert_eq!(built.data.item_count(), 0);
    }
}
