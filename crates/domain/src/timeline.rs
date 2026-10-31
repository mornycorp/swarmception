//! The timeline widget's host data (swarm-timeline's docs/spec.md §3) built from the rows: a
//! track per row (a group, a Ranges row of bars, an Instants row of pins), and a `Time` scale.
//! The axis's items are the distinct times the bars and pins name, in order; item `i` spans
//! from its time to the next one's, so a bar is the run of items from its start up to its end,
//! and the Unit scale (every item the same width) reads as event order with idle time cut out.
//! Positions are nanoseconds from [`Built::origin`], the first time floored to the hour, so the
//! ruler's hour ticks fall on the hours of the clock.

use std::collections::BTreeMap;

use timeline_core::axis::Position;
use timeline_core::data::{
    CategoryId, Colour, DataError, Importance, Instant, InstantAt, Item, ItemIndex, MemoryData, Range, RangeExtent, RangeFlags, TimelineData, TrackId,
    TrackKind, TrackTree,
};
use timeline_core::scale::{Availability, NamedScale, Scale};

use swarmception_model::Timestamp;

use crate::rows::{Bar, Node, Pin, Style};

/// The time scale's name.
pub const TIME: &str = "Time";
/// How long the last item lasts: past the last event, so a pin there shows.
const LAST_ITEM_SECONDS: i64 = 60;
const NANOS: i64 = 1_000_000_000;

/// The colour each style draws in.
#[must_use]
pub const fn colour(style: Style) -> Colour {
    match style {
        Style::MainMerge => Colour::rgb(0x5b, 0x3f, 0xa8),
        Style::Merged => Colour::rgb(0x8e, 0x6c, 0xd4),
        Style::Claimed => Colour::rgb(0x6f, 0x9f, 0xd8),
        Style::Working => Colour::rgb(0x3f, 0xa7, 0x6b),
        Style::Waiting => Colour::rgb(0xe0, 0xa1, 0x2e),
        Style::SentBack => Colour::rgb(0xd6, 0x45, 0x45),
        Style::Stopped => Colour::rgb(0x9a, 0x9a, 0x9a),
    }
}

/// The rows as the widget's data, with their tooltips.
#[derive(Debug, Clone, Default)]
pub struct SwarmData {
    data: MemoryData,
    range_tips: BTreeMap<(TrackId, usize), Vec<String>>,
    instant_tips: BTreeMap<(TrackId, usize), Vec<String>>,
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
        match leaf {
            Node::Ranges { bars, .. } => times.extend(bars.iter().flat_map(|b| [b.start, b.end])),
            Node::Instants { pins, .. } => times.extend(pins.iter().map(|p| p.at)),
            Node::Group { .. } => {}
        }
    }
    times.sort_unstable();
    times.dedup();
    let origin = Timestamp(times.first().map_or(0, |t| t.0.div_euclid(3_600) * 3_600));
    let mut built = SwarmData::default();
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
            Node::Instants { key, name, pins } => {
                let id = data.add_track(parent, key.clone(), TrackKind::Instants, name.clone())?;
                if self.show_empty {
                    data.set_show_when_empty(id, true)?;
                }
                for (place, pin) in pins.iter().enumerate() {
                    self.pin(id, place, pin)?;
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
        let range = Range { name: bar.label.clone(), extent: RangeExtent::Items { first, last }, colour: Some(colour(bar.style)), lane: None };
        self.built.data.add_range(track, range)?;
        if bar.open {
            self.built.data.set_range_flags(track, place, RangeFlags { open_ended: true, ..RangeFlags::default() })?;
        }
        self.built.range_tips.insert((track, place), bar.tooltip.clone());
        Ok(())
    }

    fn pin(&mut self, track: TrackId, place: usize, pin: &Pin) -> Result<(), DataError> {
        let instant = Instant { name: pin.label.clone(), at: InstantAt::Item(self.item(pin.at)), colour: Some(colour(pin.style)) };
        self.built.data.add_instant(track, instant)?;
        self.built.instant_tips.insert((track, place), pin.tooltip.clone());
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
        self.range_tips.get(&(track, place)).cloned().unwrap_or_else(|| self.data.range_tooltip(track, place))
    }

    fn instant_tooltip(&self, track: TrackId, place: usize) -> Vec<String> {
        self.instant_tips.get(&(track, place)).cloned().unwrap_or_else(|| self.data.instant_tooltip(track, place))
    }

    fn range_flags(&self, track: TrackId, place: usize) -> RangeFlags {
        self.data.range_flags(track, place)
    }

    fn show_when_empty(&self, track: TrackId) -> bool {
        self.data.show_when_empty(track)
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
        assert_eq!(built.data.tracks().len(), 2 + 2);
        assert_eq!(built.times.len() as u64, built.data.item_count());
        assert_eq!(built.origin.to_string(), "2026-10-31T03:00:00Z");
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
    fn no_rows_no_items() {
        let built = build(&[], false).unwrap();
        assert_eq!(built.data.item_count(), 0);
    }
}
