//! The Workers pane (#67, View ▸ Workers): model and effort compared per worker, drawn from what
//! `workers.compare` returns (`swarmception_domain::workers` defines each figure), asked again
//! only when the window or the engine's data moves on.
//!
//! A window of its own over the others, so it takes no room from the timeline and the bands
//! under it: a comparison is something to open, read and put away. It opens over the bands at
//! the bottom, the timeline's rows left in sight. A table: a row per model ×
//! effort, its workers' rows under it, then every worker's together. Each figure has a small bar
//! beside it, scaled to the column's largest, in its model × effort's colour, and the samples it
//! rests on (`n`); a thin one (under three samples) is marked `~`, its bar only outlined. A
//! column's header sorts by it (again: the other way; the first column: as the command lists
//! them). Hovering a worker lights its tickets' bars on the timeline (the shell hands
//! [`WorkersPane::hovered`] to `TimelinePane::light_hover`), a model × effort's all of its
//! workers'; clicking a worker makes its session the Details pane's subject.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::colours::series_colour;
use swarmception_domain::details::Subject;
use swarmception_domain::workers::{Comparison, Figures, Group, Measure, Worker};
use swarmception_engine::Session;
use swarmception_engine::stats_cmds::{At, GetParams};
use swarmception_engine::workers_cmds::Compare;
use swarmception_model::Timestamp;

use crate::icons;
use crate::timeline::Hover;

/// A column of the table, which it can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Column {
    /// The model × effort, and the worker's name.
    Name,
    Claimed,
    Landed,
    /// Landed per active hour.
    Rate,
    ClaimToMerge,
    Working,
    Sendbacks,
    Ready,
    Abandoned,
    Active,
}

/// What the Workers pane shows: plain data in [`crate::UiState`], kept across a restart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WorkersView {
    /// The whole run, not the timeline's window.
    pub whole_run: bool,
    /// The column it is sorted by; none: as `workers.compare` lists them.
    pub sort: Option<Column>,
    /// Smallest first, not largest.
    pub ascending: bool,
    /// Where its window is (#76): put there by `ui.set`, or where the person dragged it; none:
    /// its default, up from the bottom over the bands.
    pub place: Option<Place>,
}

impl WorkersView {
    /// Sorted by `column` as a first click on its header sorts it: the columns where less is
    /// better (claim → merge, working, send backs, abandoned) smallest first, the others largest
    /// first; `name` as `workers.compare` lists them.
    pub const fn sort_by(&mut self, column: Column) {
        self.sort = if matches!(column, Column::Name) { None } else { Some(column) };
        self.ascending = matches!(column, Column::ClaimToMerge | Column::Working | Column::Sendbacks | Column::Abandoned);
    }
}

/// Where the Workers window is (#76): its top-left corner, in whole points from the app window's
/// top-left. egui keeps a window placed off screen on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub x: i32,
    pub y: i32,
}

impl Place {
    /// The farthest a place may be from the app window's corner, either way, in points.
    pub const MOST: i32 = 16_384;

    /// `pos`, to the nearest point.
    #[must_use]
    pub fn of(pos: egui::Pos2) -> Self {
        Self { x: pos.x.round() as i32, y: pos.y.round() as i32 }
    }

    #[must_use]
    pub fn pos(self) -> egui::Pos2 {
        pos2(self.x as f32, self.y as f32)
    }

    /// Whether it is within [`Self::MOST`] of the corner.
    #[must_use]
    pub const fn sane(self) -> bool {
        self.x.abs() <= Self::MOST && self.y.abs() <= Self::MOST
    }
}

/// A row as the pane drew it.
#[derive(Debug, Clone, PartialEq)]
pub struct RowDrawn {
    /// A model × effort's label, a worker's instance, or `all`.
    pub key: String,
    pub kind: RowKind,
    /// Each column's text, as [`cells`] reads the figures.
    pub cells: Vec<String>,
    pub rect: Rect,
}

/// What a row is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Group,
    Worker,
    All,
}

/// What the pane drew last frame, for a test to check against `workers.compare`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkersDrawn {
    /// The window it drew, as `workers.compare` returned it.
    pub window: Option<(Timestamp, Timestamp)>,
    pub rows: Vec<RowDrawn>,
    /// The row under the pointer (its key), and the subject a click on a worker chose.
    pub hovered: Option<String>,
    pub chosen: Option<Subject>,
    /// A header clicked: the column to sort by.
    pub sorted: Option<Column>,
    /// Each column's header, where it was drawn.
    pub headers: Vec<(Column, Rect)>,
    /// The window, where it was drawn (#76): none when closed.
    pub rect: Option<Rect>,
}

/// What the pane was asked for: the window, and the engine's revision.
type Asked = (Option<(Timestamp, Timestamp)>, (u64, u64));

/// The workers compared over the window, and how they were asked.
#[derive(Debug, Default)]
pub struct WorkersPane {
    compared: Option<Comparison>,
    error: Option<String>,
    asked: Option<Asked>,
    asks: u64,
    hovered: Hover,
    drawn: WorkersDrawn,
    /// The place last handed to egui (#76): a new one moves the window there once, and then the
    /// person can drag it.
    placed: Option<Option<Place>>,
    /// The window's rect the last frame no button was down: one moved since, its size the same,
    /// was dragged, and its new place is kept.
    rest: Option<Rect>,
}

/// Each column: what it is, its header, its width and how it is computed.
const COLUMNS: [(Column, &str, f32, &str); 10] = [
    (Column::Name, "Model × effort  ·  worker", 210.0, "Sort as workers.compare lists them: by model, then effort, most first; each worker by its start."),
    (Column::Claimed, "Claimed", 62.0, "The tickets it claimed in the window (its own claim, not a gate's)."),
    (Column::Landed, "Landed", 62.0, "The tickets merged in the window that it held last."),
    (
        Column::Rate,
        "Landed / active h",
        140.0,
        "Landed ÷ its active hours. Active: on shift, less the swarm's quiet gaps (15 min with no event of anyone's) and its own idle time (15 min without a word while it held no ticket). n: the tickets landed. More is better.",
    ),
    (
        Column::ClaimToMerge,
        "Claim → merge",
        158.0,
        "The median, over its landed tickets, of its first claim or work of each to its merge. n: the tickets. Shorter is quicker.",
    ),
    (Column::Working, "Working", 158.0, "The median time each landed ticket was working by it, summed over its stretches. n: the tickets. Shorter is quicker."),
    (
        Column::Sendbacks,
        "Send backs / landed",
        132.0,
        "The gate's send backs of tickets it held, ÷ the tickets it landed. n: the tickets landed. Fewer is better; the causes on hover.",
    ),
    (Column::Ready, "Ready pass", 120.0, "Its own ready checks before it submits: passed ÷ (passed + failed). n: the checks with a result. More is better."),
    (Column::Abandoned, "Abandoned", 76.0, "The tickets its session's end cut off while it held them (#28), whether or not another took them up."),
    (Column::Active, "Active / on shift", 124.0, "Its active hours (as Landed / active h has them), and its time on shift: its sessions less their pauses."),
];
const HEADER: f32 = 30.0;
const GROUP_ROW: f32 = 25.0;
const WORKER_ROW: f32 = 21.0;
/// The value's place in a figure's cell: right-aligned at this, then its bar, then its `n`.
const VALUE: f32 = 64.0;
const SAMPLES: f32 = 34.0;

/// A model × effort's colour: its place in `workers.compare`'s order, the theme's range8 by number
/// (#152).
fn group_colour(index: usize, visuals: &egui::Visuals) -> Color32 {
    crate::factory_chip::drawn(series_colour(u64::try_from(index).unwrap_or(u64::MAX)), visuals)
}

/// Seconds, as a person reads a duration in a table: `42 s`, `4 min 36 s`, `25 min`, `1 h 05 min`.
#[must_use]
pub fn compact(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "–".to_owned();
    }
    let s = seconds.round().max(0.0) as i64;
    match s {
        0..60 => format!("{s} s"),
        60..600 => format!("{} min {:02} s", s / 60, s % 60),
        600..3_600 => format!("{} min", (s + 30) / 60),
        _ => format!("{} h {:02} min", s / 3_600, (s % 3_600) / 60),
    }
}

/// Hours to one decimal: `18.7 h`.
fn hours(seconds: i64) -> String {
    format!("{:.1} h", seconds.max(0) as f64 / 3_600.0)
}

/// A measure's value as the table shows it, `~` before a thin one; a dash for none.
fn shown(measure: Measure, text: impl Fn(f64) -> String) -> String {
    match measure.value.filter(|v| v.is_finite()) {
        Some(v) if measure.thin => format!("~{}", text(v)),
        Some(v) => text(v),
        None => "–".to_owned(),
    }
}

/// `window` (its id `id`) put at `pos` by its `pivot` corner on this frame. A window dragged by
/// its title bar puts back where egui last kept it after `current_pos` (egui 0.37), so where it
/// has been shown, its kept place moves first.
fn place_window<'a>(ctx: &egui::Context, window: egui::Window<'a>, id: egui::Id, pivot: Align2, pos: egui::Pos2) -> egui::Window<'a> {
    if let Some(mut state) = egui::AreaState::load(ctx, id) {
        state.pivot = pivot;
        state.pivot_pos = Some(pos);
        ctx.memory_mut(|memory| memory.areas_mut().set_state(egui::LayerId::new(egui::Order::Middle, id), state));
    }
    window.current_pos(pos)
}

/// The value of `column` for `figures`, as it sorts and as its bar is long.
fn value(figures: &Figures, column: Column) -> Option<f64> {
    let count = |n: usize| Some(n as f64);
    match column {
        Column::Name => None,
        Column::Claimed => count(figures.claimed),
        Column::Landed => count(figures.landed),
        Column::Rate => figures.landed_per_active_hour.value,
        Column::ClaimToMerge => figures.claim_to_merge_seconds.value,
        Column::Working => figures.working_seconds.value,
        Column::Sendbacks => figures.sendbacks_per_landed.value,
        Column::Ready => figures.ready_pass_rate.value,
        Column::Abandoned => count(figures.abandoned),
        Column::Active => Some(figures.active_seconds as f64),
    }
}

/// The measure behind `column`, for the columns with samples.
const fn measure(figures: &Figures, column: Column) -> Option<Measure> {
    match column {
        Column::Rate => Some(figures.landed_per_active_hour),
        Column::ClaimToMerge => Some(figures.claim_to_merge_seconds),
        Column::Working => Some(figures.working_seconds),
        Column::Sendbacks => Some(figures.sendbacks_per_landed),
        Column::Ready => Some(figures.ready_pass_rate),
        Column::Name | Column::Claimed | Column::Landed | Column::Abandoned | Column::Active => None,
    }
}

/// Each figure column's text for `figures`, in [`COLUMNS`]' order after the name: what a row
/// shows, a measure's with its samples (`2.7 n=50`, `~0.5 n=2`).
#[must_use]
pub fn cells(figures: &Figures) -> Vec<String> {
    let with_n = |m: Measure, text: &dyn Fn(f64) -> String| format!("{} n={}", shown(m, text), m.samples);
    vec![
        figures.claimed.to_string(),
        figures.landed.to_string(),
        with_n(figures.landed_per_active_hour, &|v| format!("{v:.1}")),
        with_n(figures.claim_to_merge_seconds, &compact),
        with_n(figures.working_seconds, &compact),
        with_n(figures.sendbacks_per_landed, &|v| format!("{v:.2}")),
        with_n(figures.ready_pass_rate, &|v| format!("{:.0}%", v * 100.0)),
        figures.abandoned.to_string(),
        format!("{} / {}", hours(figures.active_seconds), hours(figures.on_shift_seconds)),
    ]
}

/// How a cell's figure was made, for its tooltip.
fn how(figures: &Figures, column: Column) -> String {
    let samples = |m: Measure| {
        let thin = if m.thin { format!(": thin, under {} samples", swarmception_domain::workers::THIN) } else { String::new() };
        format!("{} samples{thin}", m.samples)
    };
    match column {
        Column::Name | Column::Claimed | Column::Landed | Column::Abandoned => String::new(),
        Column::Rate => format!(
            "{} landed in {} active ({} on shift) · {}",
            figures.landed,
            hours(figures.active_seconds),
            hours(figures.on_shift_seconds),
            samples(figures.landed_per_active_hour)
        ),
        Column::ClaimToMerge => format!("median claim → merge · {}", samples(figures.claim_to_merge_seconds)),
        Column::Working => format!("median working time per landed ticket · {}", samples(figures.working_seconds)),
        Column::Sendbacks => {
            let causes: Vec<String> = figures.sendback_causes.iter().map(|c| format!("{} {}", c.cause, c.count)).collect();
            let causes = if causes.is_empty() { String::new() } else { format!("\nby cause: {}", causes.join(", ")) };
            format!("{} send backs over {} landed · {}{causes}", figures.sendbacks, figures.landed, samples(figures.sendbacks_per_landed))
        }
        Column::Ready => format!(
            "{} ready checks: {} passed, {} failed, {} with no result · {}",
            figures.ready_runs,
            figures.ready_passed,
            figures.ready_failed,
            figures.ready_runs.saturating_sub(figures.ready_passed + figures.ready_failed),
            samples(figures.ready_pass_rate)
        ),
        Column::Active => format!("active {} of {} on shift", hours(figures.active_seconds), hours(figures.on_shift_seconds)),
    }
}

/// `10-31 09:12` of a time, UTC.
fn day_clock(t: Timestamp) -> String {
    let (_, mo, d, h, m, _) = t.civil();
    format!("{mo:02}-{d:02} {h:02}:{m:02}")
}

/// `groups`' order, sorted by `view`'s column (none last, either way), each one's workers too.
fn sorted(groups: &[Group], view: WorkersView) -> Vec<(usize, Vec<&Worker>)> {
    let mut order: Vec<(usize, Vec<&Worker>)> = groups.iter().enumerate().map(|(i, g)| (i, g.workers.iter().collect())).collect();
    let Some(column) = view.sort.filter(|c| *c != Column::Name) else { return order };
    let compare = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) if view.ascending => a.total_cmp(&b),
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };
    let figures = |i: usize| groups.get(i).map(|g| &g.figures);
    order.sort_by(|(a, _), (b, _)| compare(figures(*a).and_then(|f| value(f, column)), figures(*b).and_then(|f| value(f, column))));
    for (_, workers) in &mut order {
        workers.sort_by(|a, b| compare(value(&a.figures, column), value(&b.figures, column)));
    }
    order
}

impl WorkersPane {
    /// What `workers.compare` returned last.
    #[must_use]
    pub const fn compared(&self) -> Option<&Comparison> {
        self.compared.as_ref()
    }

    /// Why there is none: `workers.compare`'s refusal.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// How many times it has asked `workers.compare`.
    #[must_use]
    pub const fn asks(&self) -> u64 {
        self.asks
    }

    /// What it drew last frame.
    #[must_use]
    pub const fn drawn(&self) -> &WorkersDrawn {
        &self.drawn
    }

    /// The worker (or model × effort) under the pointer last frame, with its tickets.
    #[must_use]
    pub const fn hovered(&self) -> &Hover {
        &self.hovered
    }

    /// Asks `workers.compare` for `window` (the whole run when none) when it or the engine moved
    /// on.
    pub fn sync(&mut self, engine: &mut Session, window: Option<(Timestamp, Timestamp)>) {
        let asked = Some((window, engine.revision()));
        if self.asked == asked {
            return;
        }
        self.asked = asked;
        self.asks += 1;
        let params =
            window.map_or_else(GetParams::default, |(start, end)| GetParams { start: Some(At::Seconds(start.0)), end: Some(At::Seconds(end.0.max(start.0))) });
        match engine.call::<Compare>(params) {
            Ok(compared) => {
                self.compared = Some(compared);
                self.error = None;
            }
            Err(error) => {
                self.compared = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// Forgets the hover: the pane wasn't drawn.
    pub fn leave(&mut self) {
        self.hovered = Hover::Nothing;
        self.drawn = WorkersDrawn::default();
    }

    /// Draws the pane as a window over the others; `open` goes false when its × is clicked;
    /// `windowed`, whether there is a timeline window to follow; `selected`, the Details pane's
    /// subject, outlined when it is a worker's session.
    pub fn show(&mut self, ctx: &egui::Context, view: &mut WorkersView, open: &mut bool, windowed: bool, selected: Option<&Subject>) {
        self.drawn = WorkersDrawn::default();
        self.hovered = Hover::Nothing;
        let screen = ctx.content_rect();
        let width: f32 = COLUMNS.iter().map(|c| c.2).sum::<f32>() + 28.0;
        // Up from the bottom, over the bands under the timeline: the timeline's rows stay in
        // sight, where a hovered worker's tickets light up.
        let default = pos2(screen.left() + ((screen.width() - width).max(32.0) / 2.0), screen.bottom() - 36.0);
        let id = egui::Id::unique("workers-pane");
        let window = egui::Window::new(format!("{}  Workers: model × effort", icons::WORKERS))
            .id(id)
            .open(open)
            .resizable(true)
            .collapsible(true)
            .default_size(vec2(width.min(screen.width() - 32.0), (screen.height() * 0.55).clamp(220.0, 620.0)));
        // A place is its top-left corner; the default its bottom-left.
        let pivot = if view.place.is_some() { Align2::LEFT_TOP } else { Align2::LEFT_BOTTOM };
        let window = window.pivot(pivot).default_pos(default);
        // A new place (or the default again) moves it there once: egui keeps it after.
        let placing = self.placed != Some(view.place);
        let window = if placing { place_window(ctx, window, id, pivot, view.place.map_or(default, Place::pos)) } else { window };
        self.placed = Some(view.place);
        let shown = window.show(ctx, |ui| self.contents(ui, view, windowed, selected));
        self.drawn.rect = shown.map(|shown| shown.response.rect);
        self.keep_place(ctx, view, placing);
        if let Some(column) = self.drawn.sorted {
            if view.sort == Some(column) && column != Column::Name {
                view.ascending = !view.ascending;
            } else {
                view.sort_by(column);
            }
        }
    }

    /// Keeps where the person dragged the window, once they let go: a window that moved since the
    /// last frame no button was down, its size the same (a resize from its top or left edge
    /// moves its corner too, and isn't a move).
    fn keep_place(&mut self, ctx: &egui::Context, view: &mut WorkersView, placing: bool) {
        let Some(rect) = self.drawn.rect else { return };
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        if let Some(before) = self.rest.filter(|_| !placing)
            && before.min.distance(rect.min) >= 0.5
            && (before.size() - rect.size()).length() < 0.5
        {
            view.place = Some(Place::of(rect.min));
            // Handed to egui on the next frame as it is: the pivot turns to the corner, the
            // window staying put.
            self.placed = None;
        }
        self.rest = Some(rect);
    }

    fn contents(&mut self, ui: &mut egui::Ui, view: &mut WorkersView, windowed: bool, selected: Option<&Subject>) {
        self.top(ui, view, windowed);
        let Some(compared) = self.compared.clone() else {
            match &self.error {
                Some(error) => ui.colored_label(ui.visuals().warn_fg_color, format!("{}  {error}", icons::WARNING)),
                None => ui.weak("No workers to compare yet."),
            };
            return;
        };
        self.drawn.window = Some((compared.start, compared.end));
        if compared.groups.is_empty() {
            ui.weak("No worker was on shift in this window.");
            return;
        }
        egui::ScrollArea::both().auto_shrink([false, true]).show(ui, |ui| self.table(ui, &compared, *view, selected));
        ui.add_space(4.0);
        let caveat = "Tickets differ in size and kind, and a worker's tickets aren't drawn at random: read a gap between models as a hint, the more samples (n) the firmer.";
        ui.weak(egui::RichText::new(caveat).small());
    }

    /// The headers, a row per model × effort with its workers under it, then all of them.
    fn table(&mut self, ui: &mut egui::Ui, compared: &Comparison, view: WorkersView, selected: Option<&Subject>) {
        ui.spacing_mut().item_spacing.y = 0.0;
        self.header(ui, view);
        let scales = scales(compared);
        for (index, workers) in sorted(&compared.groups, view) {
            let Some(group) = compared.groups.get(index) else { continue };
            let colour = group_colour(index, ui.visuals());
            let mut tickets: Vec<u64> = group.workers.iter().flat_map(|w| w.tickets.iter().copied()).collect();
            tickets.sort_unstable();
            tickets.dedup();
            let aside = format!("{} worker{}", group.figures.workers, if group.figures.workers == 1 { "" } else { "s" });
            let row = Row {
                key: &group.label,
                kind: RowKind::Group,
                name: &group.label,
                aside,
                figures: &group.figures,
                colour,
                tip: None,
                tickets: &tickets,
                subject: None,
                chosen: false,
            };
            self.row(ui, &row, &scales);
            for worker in workers {
                let chosen = selected.is_some_and(|s| matches!(s, Subject::Session { instance, .. } if *instance == worker.instance));
                let row = Row {
                    key: &worker.instance,
                    kind: RowKind::Worker,
                    name: &worker.name,
                    aside: format!("{}{}", day_clock(worker.start), if worker.open { " · open" } else { "" }),
                    figures: &worker.figures,
                    colour,
                    tip: Some(worker_tip(worker, &group.label)),
                    tickets: &worker.tickets,
                    subject: Some(&worker.subject),
                    chosen,
                };
                self.row(ui, &row, &scales);
            }
        }
        let all = Row {
            key: "all",
            kind: RowKind::All,
            name: "All workers",
            aside: format!("{} sessions", compared.all.workers),
            figures: &compared.all,
            colour: ui.visuals().weak_text_color(),
            tip: None,
            tickets: &[],
            subject: None,
            chosen: false,
        };
        self.row(ui, &all, &scales);
    }

    /// The window it shows, the choice of window or whole run, and the key to the marks.
    fn top(&self, ui: &mut egui::Ui, view: &mut WorkersView, windowed: bool) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if windowed {
                ui.selectable_value(&mut view.whole_run, false, "Timeline's window").on_hover_text("Compare over the timeline's visible window, following it");
                ui.selectable_value(&mut view.whole_run, true, "Whole run").on_hover_text("Compare over the whole run: the first event to the last");
            } else {
                ui.weak("Whole run");
            }
            if let Some(compared) = &self.compared {
                ui.weak(
                    egui::RichText::new(format!("{} → {} UTC · {}", day_clock(compared.start), day_clock(compared.end), hours(compared.span_seconds))).small(),
                )
                .on_hover_text(format!("{} → {}", compared.start, compared.end));
            }
            ui.separator();
            ui.weak(
                egui::RichText::new(format!(
                    "n: the samples a figure rests on · ~ thin: under {} samples, its bar outlined",
                    swarmception_domain::workers::THIN
                ))
                .small(),
            );
        });
        ui.add_space(4.0);
    }

    /// The headers, each sorting by its column when clicked.
    fn header(&mut self, ui: &mut egui::Ui, view: WorkersView) {
        let width: f32 = COLUMNS.iter().map(|c| c.2).sum();
        let (rect, _) = ui.allocate_exact_size(vec2(width, HEADER), Sense::hover());
        let visuals = ui.visuals().clone();
        let mut x = rect.left();
        for (column, title, w, tip) in COLUMNS {
            let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(w, HEADER));
            let response = ui.interact(cell, ui.scope_id().with(("workers-header", title)), Sense::click());
            let mark = match (view.sort, view.ascending) {
                (Some(c), true) if c == column => " ▲",
                (Some(c), false) if c == column => " ▼",
                _ => "",
            };
            let colour = if response.hovered() { visuals.strong_text_color() } else { visuals.text_color() };
            let (anchor, at) = if column == Column::Name {
                (Align2::LEFT_CENTER, pos2(cell.left() + 6.0, cell.center().y))
            } else {
                (Align2::LEFT_CENTER, pos2(cell.left() + 4.0, cell.center().y))
            };
            ui.painter().text(at, anchor, format!("{title}{mark}"), FontId::proportional(11.5), colour);
            if response.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.drawn.sorted = Some(column);
            }
            self.drawn.headers.push((column, cell));
            x += w;
        }
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color));
    }

    /// One row: its name, then each figure with its bar and samples; its hover and click.
    fn row(&mut self, ui: &mut egui::Ui, row: &Row<'_>, scales: &[f64]) {
        let width: f32 = COLUMNS.iter().map(|c| c.2).sum();
        let height = if row.kind == RowKind::Worker { WORKER_ROW } else { GROUP_ROW };
        let sense = if row.subject.is_some() { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        let visuals = ui.visuals().clone();
        let painter = ui.painter_at(rect);
        let hovered = response.hovered() && row.kind != RowKind::All;
        match row.kind {
            RowKind::Group => {
                painter.rect_filled(rect.shrink2(vec2(0.0, 1.0)), 3.0, visuals.faint_bg_color);
            }
            RowKind::All => {
                painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color));
            }
            RowKind::Worker => {}
        }
        if hovered {
            painter.rect_filled(rect, 3.0, visuals.widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
        }
        let look = Look::of(row.kind, &visuals);
        name_cell(&painter, row, rect, &look);
        let texts = cells(row.figures);
        let pointer = response.hover_pos();
        let tip = figure_cells(&painter, row, rect, (&texts, scales), &look, pointer).or_else(|| row.tip.clone());
        if row.chosen {
            painter.rect_stroke(rect.shrink(1.0), 3.0, Stroke::new(1.5, visuals.strong_text_color()), StrokeKind::Inside);
        }
        let mut drawn_cells = vec![row.name.to_owned()];
        drawn_cells.extend(texts);
        self.drawn.rows.push(RowDrawn { key: row.key.to_owned(), kind: row.kind, cells: drawn_cells, rect });
        if hovered {
            self.drawn.hovered = Some(row.key.to_owned());
            self.hovered = Hover::Worker { key: row.key.to_owned(), tickets: row.tickets.to_vec() };
        }
        let response = if row.subject.is_some() { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response };
        if response.clicked() {
            self.drawn.chosen = row.subject.cloned();
        }
        if let Some(tip) = tip {
            response.on_hover_text(tip);
        }
    }
}

/// What one row draws.
struct Row<'a> {
    key: &'a str,
    kind: RowKind,
    name: &'a str,
    aside: String,
    figures: &'a Figures,
    colour: Color32,
    tip: Option<String>,
    /// Whose bars a hover on it lights.
    tickets: &'a [u64],
    /// What a click on it shows in Details, and whether Details shows it now.
    subject: Option<&'a Subject>,
    chosen: bool,
}

/// How a row's text looks: a model × effort's and all workers' strong, a worker's plain.
struct Look {
    strong: bool,
    text: Color32,
    weak: Color32,
    font: FontId,
    small: FontId,
}

impl Look {
    fn of(kind: RowKind, visuals: &egui::Visuals) -> Self {
        let strong = kind != RowKind::Worker;
        Self {
            strong,
            text: if strong { visuals.strong_text_color() } else { visuals.text_color() },
            weak: visuals.weak_text_color(),
            font: FontId::proportional(if strong { 12.5 } else { 12.0 }),
            small: FontId::proportional(10.0),
        }
    }
}

/// A worker's tooltip: who, when, its tickets, and what a click does.
fn worker_tip(worker: &Worker, group: &str) -> String {
    let end = if worker.open { "(going on)".to_owned() } else { day_clock(worker.end) };
    let open = if worker.open { "\nopen: no session.stop logged, so on shift to the last event" } else { "" };
    let tickets: Vec<String> = worker.tickets.iter().map(|n| format!("#{n}")).collect();
    format!(
        "{} · {} · {group}\n{} → {end} UTC{open}\n{} tickets: {}\nclick: its session's story in Details",
        worker.name,
        worker.instance,
        day_clock(worker.start),
        tickets.len(),
        tickets.join(" ")
    )
}

/// The name column: a swatch in its model × effort's colour (indented under it for a worker),
/// the name, and beside it what it is (its workers, its start).
fn name_cell(painter: &egui::Painter, row: &Row<'_>, rect: Rect, look: &Look) {
    let indent = if row.kind == RowKind::Worker { 20.0 } else { 6.0 };
    let side = if look.strong { 9.0 } else { 6.0 };
    let swatch = Rect::from_center_size(pos2(rect.left() + indent + 4.0, rect.center().y), vec2(side, side));
    if row.kind != RowKind::All {
        painter.rect_filled(swatch, 2.0, row.colour);
    }
    let name = painter.text(pos2(swatch.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, row.name, look.font.clone(), look.text);
    let name_width = COLUMNS.first().map_or(0.0, |c| c.2);
    if name.right() + 6.0 < rect.left() + name_width - 4.0 {
        painter.text(pos2(name.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, &row.aside, look.small.clone(), look.weak);
    }
}

/// The figure columns: a measure's value, its bar and its samples; a count or the hours as text.
/// The tooltip of the cell under `pointer`, when it has one.
fn figure_cells(
    painter: &egui::Painter,
    row: &Row<'_>,
    rect: Rect,
    (texts, scales): (&[String], &[f64]),
    look: &Look,
    pointer: Option<egui::Pos2>,
) -> Option<String> {
    let mut x = rect.left() + COLUMNS.first().map_or(0.0, |c| c.2);
    let mut tip = None;
    for (i, (column, _, w, _)) in COLUMNS.iter().copied().enumerate().skip(1) {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
        let text = texts.get(i - 1).map_or("", String::as_str);
        let y = cell.center().y;
        match measure(row.figures, column) {
            Some(m) => {
                let shown = text.rsplit_once(" n=").map_or(text, |(v, _)| v);
                painter.text(pos2(cell.left() + VALUE, y), Align2::RIGHT_CENTER, shown, look.font.clone(), if m.thin { look.weak } else { look.text });
                let room = Rect::from_min_max(pos2(cell.left() + VALUE + 5.0, cell.top()), pos2(cell.right() - SAMPLES - 3.0, cell.bottom()));
                bar(painter, room, m, scales.get(i).copied().unwrap_or(0.0), row.colour, look.strong);
                painter.text(pos2(cell.right() - SAMPLES, y), Align2::LEFT_CENTER, format!("n={}", m.samples), look.small.clone(), look.weak);
            }
            None if column == Column::Active => {
                painter.text(pos2(cell.left() + 8.0, y), Align2::LEFT_CENTER, text, look.font.clone(), look.text);
            }
            None => {
                painter.text(pos2(cell.left() + 30.0, y), Align2::RIGHT_CENTER, text, look.font.clone(), if text == "0" { look.weak } else { look.text });
            }
        }
        if pointer.is_some_and(|p| cell.contains(p)) {
            tip = Some(how(row.figures, column)).filter(|how| !how.is_empty());
        }
        x += w;
    }
    tip
}

/// Each column's largest value over the groups and workers (not every worker's together), its
/// bars' full length; by [`COLUMNS`]' index.
fn scales(compared: &Comparison) -> Vec<f64> {
    let rows: Vec<&Figures> = compared.groups.iter().flat_map(|g| std::iter::once(&g.figures).chain(g.workers.iter().map(|w| &w.figures))).collect();
    COLUMNS.iter().map(|(column, ..)| rows.iter().filter_map(|f| value(f, *column)).filter(|v| v.is_finite()).fold(0.0, f64::max)).collect()
}

/// A figure's bar in `room`, its value of `scale` long, over a faint track; outlined only when
/// thin.
fn bar(painter: &egui::Painter, room: Rect, measure: Measure, scale: f64, colour: Color32, strong: bool) {
    let track = Rect::from_center_size(room.center(), vec2(room.width().max(0.0), if strong { 7.0 } else { 5.0 }));
    if track.width() < 4.0 {
        return;
    }
    painter.rect_filled(track, 2.0, colour.gamma_multiply(0.12));
    let Some(value) = measure.value.filter(|v| v.is_finite() && *v > 0.0) else { return };
    let fraction = if scale > 0.0 { (value / scale).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let filled = Rect::from_min_size(track.min, vec2((track.width() * fraction).max(2.0), track.height()));
    if measure.thin {
        painter.rect_stroke(filled, 2.0, Stroke::new(1.0, colour.gamma_multiply(0.8)), StrokeKind::Inside);
    } else {
        painter.rect_filled(filled, 2.0, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_a_person_reads_them() {
        assert_eq!(compact(42.4), "42 s");
        assert_eq!(compact(276.0), "4 min 36 s");
        assert_eq!(compact(25.0 * 60.0 + 44.0), "26 min");
        assert_eq!(compact(3_900.0), "1 h 05 min");
        assert_eq!(compact(f64::NAN), "–");
        assert_eq!(compact(-5.0), "0 s");
    }

    #[test]
    fn cells_say_their_samples_and_mark_thin_ones() {
        let figures = Figures {
            claimed: 4,
            landed: 2,
            landed_per_active_hour: Measure { value: Some(2.0), samples: 2, thin: true },
            claim_to_merge_seconds: Measure { value: Some(1_500.0), samples: 30, thin: false },
            ready_pass_rate: Measure { value: Some(0.86), samples: 66, thin: false },
            active_seconds: 7_200,
            on_shift_seconds: 36_000,
            ..Figures::default()
        };
        assert_eq!(cells(&figures), ["4", "2", "~2.0 n=2", "25 min n=30", "– n=0", "– n=0", "86% n=66", "0", "2.0 h / 10.0 h"]);
    }

    #[test]
    fn sorting_keeps_none_last_either_way() {
        let group = |label: &str, rate: Option<f64>| Group {
            model: None,
            effort: None,
            label: label.to_owned(),
            figures: Figures { landed_per_active_hour: Measure { value: rate, samples: 5, thin: false }, ..Figures::default() },
            workers: Vec::new(),
        };
        let groups = [group("a", Some(1.0)), group("b", None), group("c", Some(3.0))];
        let order = |view: WorkersView| sorted(&groups, view).into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        assert_eq!(order(WorkersView::default()), [0, 1, 2], "as the command lists them");
        assert_eq!(order(WorkersView { sort: Some(Column::Rate), ..WorkersView::default() }), [2, 0, 1]);
        assert_eq!(order(WorkersView { sort: Some(Column::Rate), ascending: true, ..WorkersView::default() }), [0, 2, 1]);
    }
}
