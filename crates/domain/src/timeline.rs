//! The timeline widget's host data (swarm-timeline's docs/spec.md §3) built from the rows: a
//! track per row (a group, or a Ranges row of bars, each range in its bar's colour), and a `Time`
//! scale. The axis's items are the distinct times the bars name, in order; item `i` spans
//! from its time to the next one's, so a bar is the run of items from its start up to its end,
//! and the Unit scale (every item the same width) reads as event order with idle time cut out.
//! Positions are nanoseconds from [`Built::origin`], the first time floored to the hour, so the
//! ruler's hour ticks fall on the hours of the clock; the data's clock epoch is that origin, so a
//! Clock ruler reads the `Time` scale as the time of day (UTC).

use std::collections::BTreeMap;

use timeline_core::axis::Position;
use timeline_core::data::{
    CategoryId, Colour, DataError, Importance, Instant, Item, ItemIndex, MemoryData, Range, RangeExtent, RangeFlags, TimelineData, TooltipLine, TooltipSpan,
    TrackId, TrackKind, TrackTree,
};
use timeline_core::scale::{Availability, NamedScale, Scale};

use swarmception_model::Timestamp;

use crate::colours::Rgb;
use crate::rows::{Bar, Node};
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
            importance: timeline_core::data::Importance::default(),
        };
        self.built.data.add_range(track, range)?;
        if bar.open {
            self.built.data.set_range_flags(track, place, RangeFlags { open_ended: true, ..RangeFlags::default() })?;
        }
        self.built.range_tips.insert((track, place), bar.tooltip_lines.clone());
        let links = Link::of(bar);
        if !links.is_empty() {
            self.built.range_links.insert((track, place), links);
        }
        Ok(())
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
        let RangeExtent::Items { first, last } = ranges[1].extent else { panic!() };
        let span = scale.range_span(first, last).unwrap();
        assert_eq!(built.time_at(span.start()).to_string(), "2026-10-31T03:53:25Z");
        assert_eq!(built.time_at(span.end()).to_string(), "2026-10-31T04:06:43Z", "up to the submit");
        assert_eq!(built.data.range_tooltip(ticket, 1)[0], "#170 · PR #173");
    }

    #[test]
    fn a_bars_tooltip_reaches_the_widget_in_its_colours() {
        let activity = Activity::of(&sample());
        let rows = rows(&activity, &[], RowsSpec::default());
        let built = build(&rows, false).unwrap();
        let tree = built.data.tracks();
        let ticket = tree.ids().find(|&id| tree.get(id).unwrap().key() == "tickets/2").unwrap();
        let lines = built.data.range_tooltip_lines(ticket, 1);
        let plain = built.data.range_tooltip(ticket, 1);
        assert_eq!(lines.iter().map(TooltipLine::text).collect::<Vec<_>>(), plain, "the same words");
        assert!(lines[0].spans[0].strong, "the title strong");
        let working = &lines[1].spans[0];
        assert_eq!((working.text.as_str(), working.colour), ("working", Some(colour(crate::colours::ticket_colour(170)))));
        assert!(lines[1].spans.iter().any(|s| s.strong && s.text == "13 min 18 s"));
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
        let items = built.data.menu_items(Some(track("tickets/2")), Some(1), None);
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

    #[test]
    fn no_rows_no_items() {
        let built = build(&[], false).unwrap();
        assert_eq!(built.data.item_count(), 0);
    }
}
