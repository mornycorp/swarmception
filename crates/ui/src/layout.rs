//! How the panes share the window (#68, G18): the rule as plain functions, which the shell
//! draws by and a test checks.
//!
//! - **The timeline keeps a minimum** ([`timeline_least`]): its toolbar, its rulers, the merges
//!   rows and [`LEAST_ROWS`] rows under them (all of them when there are fewer), and its
//!   scroller.
//! - **The bands under it give way first** ([`share`]): each band (the Gates, the Factory, and
//!   any pane that slots in under the timeline) asks for its height, the person's (dragged) or
//!   its default; when they don't all fit over the timeline's minimum they shrink, each in
//!   proportion to what it has over its own least height, down to that least. Only then does the
//!   timeline give way.
//! - **A band folds to its header** ([`FOLDED_BAND`] high), a click on its title's chevron; the
//!   Details pane folds to a strip ([`FOLDED_STRIP`] wide). Folded or not is the person's choice
//!   once they make it ([`Folded`], saved with the rest of `UiState`); until then the window's
//!   size decides ([`defaults`]): on a small window the Gates band starts folded and the
//!   Factory short, on a smaller one both folded.
//! - **Defaults are sized from the window** ([`defaults`]): a band's height, the details' width,
//!   until the person drags one (then [`crate::PaneSizes`] holds theirs).
//!
//! Without the timeline the last open band fills the rest in its place.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::icons;

/// A folded band's height: its header's row alone.
pub const FOLDED_BAND: f32 = 28.0;
/// The folded Details pane's width: a strip with its chevron and icon.
pub const FOLDED_STRIP: f32 = 30.0;
/// The ticket rows the timeline keeps at least under its merges rows (all when it has fewer).
pub const LEAST_ROWS: usize = 6;
/// The timeline's least height before it has data to measure.
pub const TIMELINE_LEAST_UNMEASURED: f32 = 200.0;
/// The least width the panes left of the Details keep.
pub const CENTRAL_LEAST_WIDTH: f32 = 480.0;

/// Which panes are folded: each the person's choice, or none for what the window's size says
/// ([`defaults`]). Part of [`crate::UiState`] (version 6), so it is saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Folded {
    /// The Gates band, to its header.
    pub gates: Option<bool>,
    /// The Factory band, to its header.
    pub factory: Option<bool>,
    /// The Details pane, to a narrow strip.
    pub details: Option<bool>,
}

impl Folded {
    /// Each choice made, else the default's.
    #[must_use]
    pub fn or(self, default: Self) -> Self {
        Self { gates: self.gates.or(default.gates), factory: self.factory.or(default.factory), details: self.details.or(default.details) }
    }

    /// What `set` chooses over this; what it leaves out stays.
    #[must_use]
    pub fn with(self, set: Self) -> Self {
        set.or(self)
    }
}

/// The panes' sizes and folds the window's size gives them until the person chooses
/// ([`defaults`]): in points, the folds all chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Defaults {
    pub gates: f32,
    pub factory: f32,
    pub details: f32,
    pub folded: Folded,
}

/// What a window of `size` (its inner size, in points) gives the panes until the person chooses:
/// the Factory about a third of the height (220 to 520), the Gates 170 on a tall window, 130
/// otherwise, the Details about 28% of the width (300 to 460); the Gates folded under 760 high,
/// the Factory too under 560, the Details under 1000 wide.
#[must_use]
pub fn defaults(size: egui::Vec2) -> Defaults {
    let (w, h) = (size.x.max(0.0), size.y.max(0.0));
    Defaults {
        gates: if h >= 1_100.0 { 170.0 } else { 130.0 },
        factory: (h * 0.36).clamp(220.0, 520.0),
        details: (w * 0.28).clamp(300.0, 460.0),
        folded: Folded { gates: Some(h < 760.0), factory: Some(h < 560.0), details: Some(w < 1_000.0) },
    }
}

/// A band under the timeline, as [`share`] reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// The height it asks for: the person's, else its default.
    pub wanted: f32,
    /// The least it shrinks to while open.
    pub least: f32,
    pub folded: bool,
}

impl Band {
    /// Its height when there is room.
    #[must_use]
    pub fn size(&self) -> f32 {
        if self.folded { FOLDED_BAND } else { self.wanted.max(self.least) }
    }

    /// How much it can give way: what it has over its least.
    fn give(&self) -> f32 {
        if self.folded { 0.0 } else { self.size() - self.least }
    }
}

/// The heights of `bands` in `available` points, the rest left to what fills (the timeline, or
/// the last open band without it), which keeps `least` while the bands can give way: each band
/// its [`Band::size`]; when they leave less than `least`, they shrink in proportion to what each
/// has over its least, down to that; when even their least and the folded headers don't fit in
/// `available`, all of them shrink in proportion.
#[must_use]
pub fn share(available: f32, least: f32, bands: &[Band]) -> Vec<f32> {
    let available = available.max(0.0);
    let mut sizes: Vec<f32> = bands.iter().map(Band::size).collect();
    let over = sizes.iter().sum::<f32>() - (available - least.max(0.0)).max(0.0);
    let give: f32 = bands.iter().map(Band::give).sum();
    if over > 0.0 && give > 0.0 {
        let take = over.min(give);
        for (size, band) in sizes.iter_mut().zip(bands) {
            *size -= band.give() * take / give;
        }
    }
    let total: f32 = sizes.iter().sum();
    if total > available && total > 0.0 {
        for size in &mut sizes {
            *size *= available / total;
        }
    }
    sizes
}

/// The least height the timeline keeps: `chrome` (its toolbar, the rulers over the rows and the
/// scroller under them) and the first [`LEAST_ROWS`] + 2 of `rows` (each row's height, top to
/// bottom: the merges rows, then the tickets'), all of them when there are fewer.
#[must_use]
pub fn timeline_least(chrome: f32, rows: &[f32]) -> f32 {
    chrome.max(0.0) + rows.iter().take(LEAST_ROWS + 2).map(|h| h.max(0.0)).sum::<f32>()
}

/// A band's place in the window, as `ui.inspect` reports it (whole points).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BandDrawn {
    /// `gates`, `factory`, `details`, …
    pub name: String,
    /// Its height (its width for the Details) last frame.
    pub size: u32,
    /// What it asked for: the person's, else its default.
    pub wanted: u32,
    pub folded: bool,
    /// Its fold is the window's default, not the person's choice.
    pub default_fold: bool,
}

/// How the panes shared the window last frame (#68): `ui.inspect`'s `layout`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayoutDrawn {
    /// The timeline's height, none when it isn't shown.
    pub timeline: Option<u32>,
    /// The least it keeps ([`timeline_least`]).
    pub timeline_least: u32,
    /// The bands under the timeline, top to bottom.
    pub bands: Vec<BandDrawn>,
    /// The Details pane, its width.
    pub details: Option<BandDrawn>,
}

/// A pane's title that folds it: a chevron (down open, right folded, or left for a pane folded
/// to the right), its icon and its name, strong; a click on it toggles the fold.
pub fn fold_title(ui: &mut egui::Ui, icon: &str, name: &str, chevron: Chevron) -> egui::Response {
    let glyph = match chevron {
        Chevron::Open => icons::CHEVRON_DOWN,
        Chevron::Folded => icons::CHEVRON_RIGHT,
        Chevron::FoldRight => icons::CHEVRON_RIGHT,
        Chevron::Left => icons::CHEVRON_LEFT,
    };
    let text = egui::RichText::new(format!("{glyph}  {icon}  {name}")).strong();
    let tip = match chevron {
        Chevron::Open => format!("Fold the {name} to its header"),
        Chevron::FoldRight => format!("Fold the {name} to a strip"),
        Chevron::Folded | Chevron::Left => format!("Open the {name}"),
    };
    ui.add(egui::Button::new(text).frame(false)).on_hover_text(tip)
}

/// Which way a title's chevron points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chevron {
    /// An open band: down.
    Open,
    /// A folded band: right.
    Folded,
    /// An open pane on the right that folds to a strip: right.
    FoldRight,
    /// A pane folded to a strip on the right: left, to open it.
    Left,
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn band(wanted: f32, least: f32, folded: bool) -> Band {
        Band { wanted, least, folded }
    }

    fn near(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    #[test]
    fn bands_take_what_they_ask_when_the_timeline_keeps_its_least() {
        let bands = [band(150.0, 90.0, false), band(300.0, 140.0, false)];
        let sizes = share(1_000.0, 300.0, &bands);
        assert!(near(&sizes, &[150.0, 300.0]), "{sizes:?}");
        // A folded band is its header, whatever it asked.
        let sizes = share(1_000.0, 300.0, &[band(150.0, 90.0, true), band(300.0, 140.0, false)]);
        assert!(near(&sizes, &[FOLDED_BAND, 300.0]), "{sizes:?}");
        // Asking under its least, it gets its least.
        assert!(near(&share(1_000.0, 300.0, &[band(10.0, 90.0, false)]), &[90.0]));
    }

    #[test]
    fn the_bands_give_way_before_the_timeline_in_proportion_to_their_slack() {
        // 450 asked, 300 left over the timeline's 300: 150 given, from slacks of 60 and 160.
        let bands = [band(150.0, 90.0, false), band(300.0, 140.0, false)];
        let sizes = share(600.0, 300.0, &bands);
        assert!((sizes.iter().sum::<f32>() - 300.0).abs() < 0.01, "the timeline keeps 300: {sizes:?}");
        assert!(near(&sizes, &[150.0 - 60.0 * 150.0 / 220.0, 300.0 - 160.0 * 150.0 / 220.0]), "{sizes:?}");
        assert!(sizes[0] >= 90.0 && sizes[1] >= 140.0, "none under its least");
        // A folded band gives nothing; the open one gives it all.
        let sizes = share(500.0, 300.0, &[band(150.0, 90.0, true), band(300.0, 140.0, false)]);
        assert!(near(&sizes, &[FOLDED_BAND, 500.0 - 300.0 - FOLDED_BAND]), "{sizes:?}");
    }

    #[test]
    fn the_timeline_gives_way_last_and_nothing_overflows() {
        let bands = [band(150.0, 90.0, false), band(300.0, 140.0, false)];
        // The bands at their least: the timeline has what is left, under its least.
        let sizes = share(330.0, 300.0, &bands);
        assert!(near(&sizes, &[90.0, 140.0]), "{sizes:?}");
        // Not even their least fits: all shrink in proportion, nothing past the space.
        let sizes = share(115.0, 300.0, &bands);
        assert!((sizes.iter().sum::<f32>() - 115.0).abs() < 0.01 && near(&sizes, &[45.0, 70.0]), "{sizes:?}");
        assert!(share(0.0, 300.0, &bands).iter().all(|s| *s == 0.0));
        assert!(share(-5.0, 300.0, &bands).iter().all(|s| *s == 0.0), "a negative space is none");
        assert!(share(500.0, 300.0, &[]).is_empty(), "no band: the timeline has it all");
    }

    #[test]
    fn the_timelines_least_is_its_chrome_and_the_first_eight_rows() {
        assert_eq!(timeline_least(90.0, &[24.0; 3]), 90.0 + 72.0, "fewer rows than the least: all of them");
        assert_eq!(timeline_least(90.0, &[24.0; 30]), 90.0 + 24.0 * 8.0, "the merges rows and six ticket rows");
        assert_eq!(timeline_least(90.0, &[24.0, 24.0, 40.0, 24.0, 24.0, 24.0, 24.0, 24.0, 99.0]), 90.0 + 208.0, "each row its own height");
        assert_eq!(timeline_least(-1.0, &[]), 0.0);
    }

    #[test]
    fn defaults_follow_the_windows_size() {
        let laptop = defaults(egui::vec2(1_440.0, 900.0));
        assert_eq!(laptop.folded, Folded { gates: Some(false), factory: Some(false), details: Some(false) });
        assert_eq!((laptop.gates, laptop.factory), (130.0, 324.0), "short on a laptop");
        let big = defaults(egui::vec2(2_560.0, 1_440.0));
        assert_eq!((big.gates, big.factory, big.details), (170.0, 518.4, 460.0));
        let small = defaults(egui::vec2(1_280.0, 720.0));
        assert_eq!(small.folded, Folded { gates: Some(true), factory: Some(false), details: Some(false) }, "the Gates start folded");
        let tiny = defaults(egui::vec2(800.0, 500.0));
        assert_eq!(tiny.folded, Folded { gates: Some(true), factory: Some(true), details: Some(true) });
        assert_eq!(tiny.factory, 220.0);
    }

    #[test]
    fn a_choice_goes_over_the_default_and_a_set_over_a_choice() {
        let default = Folded { gates: Some(true), factory: Some(false), details: Some(false) };
        let chosen = Folded { gates: Some(false), ..Folded::default() };
        assert_eq!(chosen.or(default), Folded { gates: Some(false), factory: Some(false), details: Some(false) });
        let set = Folded { details: Some(true), ..Folded::default() };
        assert_eq!(chosen.with(set), Folded { gates: Some(false), factory: None, details: Some(true) });
        let read: Folded = serde_json::from_str(r#"{"gates": true}"#).unwrap();
        assert_eq!(read, Folded { gates: Some(true), ..Folded::default() });
        assert!(serde_json::from_str::<Folded>(r#"{"timeline": true}"#).is_err(), "the timeline never folds");
    }
}
