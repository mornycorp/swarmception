//! The Gates pane (#61, View ▸ Gates): how the gate behaved over the timeline's window, drawn from
//! what `gates.list` returns (`swarmception_domain::gates` defines each figure), asked again only
//! when the window or the engine's data moves on.
//!
//! A band under the timeline. Along its top, the headline figures: the coordinator's gate's pass
//! rate, median run, how busy it was and its reruns, the longest queue at the dock, and the
//! workers' `ready` checks' pass rate and median. Under them, lined up under the timeline's rows
//! (its window, across the same columns), both lanes' runs per clock hour (#75), stacked by
//! result and mirrored about one baseline, the gate's up and the `ready` checks' down, each on its
//! own scale; in the left column, beside them, the runs each lane started in the window and a
//! short table of the failures by cause. Hovering an hour gives its figures.
//!
//! The runs have no lanes here and no rows on the timeline (#117, reversing #84): a run naming a
//! ticket is the ticket's *in the gate* stretch on its bar, a failure its send-back; the
//! Factory's Gate list tells a run's story. The band keeps its figures, causes and hours.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use swarmception_domain::gates::{Figures, GateHour, GatesList, Lane};
use swarmception_domain::rows::duration;
use swarmception_domain::tooltip::TipLine;
use swarmception_engine::Session;
use swarmception_engine::gates_cmds::List;
use swarmception_engine::stats_cmds::{At, GetParams};
use swarmception_model::Timestamp;

use crate::details::line_job;
use crate::factory_chip::rgb;
use crate::factory_stats::percent;
use crate::factory_tip::legible;
use crate::icons;
use crate::layout::{Chevron, fold_title};
use crate::timeline::TimeAxis;

/// One lane's column of an hour as the pane drew it: the counts it stacked, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct HourDrawn {
    pub start: Timestamp,
    pub lane: Lane,
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub no_result: usize,
    /// The stacked column, from the baseline: as tall as its runs on its lane's scale; none tall
    /// with none.
    pub rect: Rect,
}

/// What the pane drew last frame, for a test to check against `gates.list`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GatesDrawn {
    /// The window it drew, as `gates.list` returned it.
    pub window: Option<(Timestamp, Timestamp)>,
    /// The headline figures, `label value`, in order.
    pub figures: Vec<String>,
    /// The causes table's rows: `cause gate ready`.
    pub causes: Vec<String>,
    /// The hours' columns drawn.
    pub hours: usize,
    /// Each hour's two columns as drawn (#75): the gate's runs, then the `ready` checks', an hour
    /// after another, in `gates.list`'s order.
    pub hour_columns: Vec<HourDrawn>,
    /// Where the window's start and end were across.
    pub axis: Option<(f32, f32)>,
    /// The hour under the pointer.
    pub hovered: Option<Timestamp>,
    /// Its title was clicked this frame: fold it (#68).
    pub fold: bool,
}

/// What the pane was asked for: the window, and the engine's revision.
type Asked = (Option<(Timestamp, Timestamp)>, (u64, u64));

/// The gate's runs over the window, and how they were asked.
#[derive(Debug, Default)]
pub struct GatesPane {
    list: Option<GatesList>,
    error: Option<String>,
    asked: Option<Asked>,
    /// How many times `gates.list` ran: the test that it isn't asked every frame reads it.
    asks: u64,
    drawn: GatesDrawn,
}

/// The least the body takes to be drawn: a chart this tall.
const LEAST_BODY: f32 = 24.0;
/// The tallest the hours' chart grows: both lanes', mirrored.
const CHART: f32 = 110.0;
/// The chart from which each column's count is written at its end.
const COUNTED_CHART: f32 = 64.0;
/// The left column's width when there is no timeline to line up with.
const OWN_COLUMN: f32 = 190.0;
/// The narrowest left column: the lanes' names and the causes table need it.
const NARROWEST_COLUMN: f32 = 150.0;

/// `HH:MM` of a time, UTC.
fn clock(t: Timestamp) -> String {
    let (_, _, _, h, m, _) = t.civil();
    format!("{h:02}:{m:02}")
}

fn span(seconds: Option<i64>) -> String {
    seconds.map_or_else(|| "–".to_owned(), duration)
}

impl GatesPane {
    /// What `gates.list` returned last.
    #[must_use]
    pub const fn list(&self) -> Option<&GatesList> {
        self.list.as_ref()
    }

    /// Why there is none: `gates.list`'s refusal.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// How many times it has asked `gates.list`.
    #[must_use]
    pub const fn asks(&self) -> u64 {
        self.asks
    }

    /// What it drew last frame.
    #[must_use]
    pub const fn drawn(&self) -> &GatesDrawn {
        &self.drawn
    }

    /// Asks `gates.list` for `window` (the whole run when none) when it or the engine moved on.
    pub fn sync(&mut self, engine: &mut Session, window: Option<(Timestamp, Timestamp)>) {
        let asked = Some((window, engine.revision()));
        if self.asked == asked {
            return;
        }
        self.asked = asked;
        self.asks += 1;
        let params =
            window.map_or_else(GetParams::default, |(start, end)| GetParams { start: Some(At::Seconds(start.0)), end: Some(At::Seconds(end.0.max(start.0))) });
        match engine.call::<List>(params) {
            Ok(list) => {
                self.list = Some(list);
                self.error = None;
            }
            Err(error) => {
                self.list = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// Forgets what it drew: the pane wasn't drawn.
    pub fn leave(&mut self) {
        self.drawn = GatesDrawn::default();
    }

    /// Draws the pane; `axis`, where the timeline's times fall across the screen (its window's
    /// ends, through its folds, #86), to line the hours up under it (none: the pane's own); `t`,
    /// the factory's time, marked.
    pub fn ui(&mut self, ui: &mut egui::Ui, axis: Option<&TimeAxis>, t: Option<Timestamp>) {
        self.drawn = GatesDrawn::default();
        let Some(list) = self.list.clone() else {
            ui.horizontal(|ui| {
                self.drawn.fold = fold_title(ui, icons::GATES, "Gates", Chevron::Open).clicked();
                match &self.error {
                    Some(error) => ui.colored_label(ui.visuals().warn_fg_color, format!("{}  {error}", icons::WARNING)),
                    None => ui.weak("No gate runs to show yet."),
                };
            });
            return;
        };
        self.drawn.window = Some((list.start, list.end));
        self.header(ui, &list);
        let body = ui.available_rect_before_wrap();
        if body.height() < LEAST_BODY + 4.0 || body.width() < NARROWEST_COLUMN + 40.0 {
            return;
        }
        let response = ui.allocate_rect(body, Sense::hover());
        let lined = axis.filter(|a| {
            let (x0, x1) = a.ends();
            x0 - body.left() >= NARROWEST_COLUMN && x1 - x0 > 40.0
        });
        let (x0, x1) = match lined {
            Some(a) => (a.ends().0, a.ends().1.min(body.right() + 16.0)),
            None => (body.left() + OWN_COLUMN, body.right() - 8.0),
        };
        self.drawn.axis = Some((x0, x1));
        let scale = Scale { start: list.start, end: list.end, x0, x1, through: lined.cloned() };
        // Out to the timeline's right edge, which the panel's margin can pass.
        let painter = ui.painter_at(body.with_max_x(body.right().max(x1)));
        let visuals = ui.visuals().clone();
        let top = body.top() + 4.0;
        let chart = Rect::from_min_max(pos2(x0, top), pos2(x1, (body.bottom() - 2.0).min(top + CHART)));
        painter.rect_filled(chart.expand2(vec2(0.0, 2.0)), 3.0, visuals.faint_bg_color);
        self.column((&painter, &visuals), &list, body, x0, top);
        if let Some(x) = t.and_then(|t| scale.x(t)) {
            painter.vline(x, body.top()..=body.bottom(), Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.6)));
        }
        let hour = if chart.height() >= 20.0 { self.hours((&painter, &visuals), &list, &scale, chart, response.hover_pos()) } else { None };
        if list.runs.is_empty() {
            painter.text(chart.center(), Align2::CENTER_CENTER, "No gate runs in this window", FontId::proportional(11.0), visuals.weak_text_color());
        }
        if let Some(hour) = hour {
            self.drawn.hovered = Some(hour.start);
            let lines = hour_lines(&hour);
            response.on_hover_ui_at_pointer(|ui| tip(ui, &lines));
        }
    }

    /// The headline figures along the top, each with how it is computed on hover.
    fn header(&mut self, ui: &mut egui::Ui, list: &GatesList) {
        let figures = headline(list);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            self.drawn.fold = fold_title(ui, icons::GATES, "Gates", Chevron::Open).clicked();
            ui.weak(egui::RichText::new(format!("{} → {} UTC", clock(list.start), clock(list.end))).small())
                .on_hover_text(format!("The timeline's visible window: {} → {}", list.start, list.end));
            for (label, value, colour, how) in figures {
                pill(ui, label, &value, colour.map(rgb)).on_hover_text(how);
                self.drawn.figures.push(format!("{label} {value}"));
            }
        });
    }
}

/// A headline figure: its label, its value, its colour, and how it is computed.
type Figure = (&'static str, String, Option<[u8; 3]>, String);

/// How the longest queue was made up, for its tooltip.
fn queue_text(queue: &swarmception_domain::gates::Queue) -> String {
    if queue.count == 0 {
        return "No ticket waited for merge in the window.".to_owned();
    }
    let when = queue.at.map(|t| format!(" at {} UTC", clock(t))).unwrap_or_default();
    let tickets: Vec<String> = queue.tickets.iter().map(|n| format!("#{n}")).collect();
    format!("The most tickets waiting for merge at once (submitted, not yet merged or sent back){when}: {}.", tickets.join(", "))
}

/// The headline figures of `list`, in order.
fn headline(list: &GatesList) -> [Figure; 6] {
    let (gate, ready, queue) = (&list.gate, &list.ready, &list.longest_queue);
    let pass_colour = gate.pass_rate.map(|rate| if rate >= 0.9 { swarmception_domain::tooltip::PASSED } else { swarmception_domain::tooltip::FAILED });
    [
        (
            "gate pass",
            percent(gate.pass_rate),
            pass_colour,
            format!(
                "The coordinator's gate runs that started in the window: {} passed of {} with a result ({} runs: {} failed, {} with no result, {} running; {} read from merges and send backs, no sub.start logged).\nPassed: rc=0, or it merged a ticket. Failed: another rc, or it only sent tickets back.\nThe same runs stats.get counts.",
                gate.passed,
                gate.passed + gate.failed,
                gate.runs,
                gate.failed,
                gate.no_result,
                gate.running,
                gate.inferred
            ),
        ),
        (
            "median",
            span(gate.median_seconds),
            None,
            format!(
                "The median time a gate run took, start to end, over the {} with a result; the longest {}.",
                gate.passed + gate.failed,
                span(gate.longest_seconds)
            ),
        ),
        (
            "busy",
            percent(gate.busy),
            None,
            format!("The time the gate was running in the window: {} of {}.", duration(gate.busy_seconds), duration(list.span_seconds)),
        ),
        ("reruns", gate.reruns.to_string(), None, "Gate runs that reran an earlier run (gate-43b of gate-43, gate-43-rerun, the same id again).".to_owned()),
        ("longest queue", queue.count.to_string(), None, queue_text(queue)),
        (
            "ready pass",
            format!("{} · {}", percent(ready.pass_rate), span(ready.median_seconds)),
            None,
            format!(
                "The workers' own checks before they submit (ready): {} passed of {} with a result ({} runs, {} reruns), median {}; busy {}.",
                ready.passed,
                ready.passed + ready.failed,
                ready.runs,
                ready.reruns,
                span(ready.median_seconds),
                percent(ready.busy)
            ),
        ),
    ]
}

impl GatesPane {
    /// The left column: the runs each lane started in the window, then the failures by cause.
    fn column(&mut self, (painter, visuals): (&egui::Painter, &egui::Visuals), list: &GatesList, body: Rect, x0: f32, top: f32) {
        let right = x0 - 8.0;
        let left = (right - OWN_COLUMN + 16.0).max(body.left() + 6.0);
        let runs = format!("runs  gate {} · ready {}", list.gate.runs, list.ready.runs);
        painter.text(pos2(right, top + 6.0), Align2::RIGHT_CENTER, runs, FontId::proportional(11.5), visuals.text_color());
        let causes_top = top + 14.0;
        let mut causes: Vec<(String, usize, usize)> = Vec::new();
        for (lane, figures) in [(0, &list.gate), (1, &list.ready)] {
            for cause in &figures.causes {
                let at = causes.iter().position(|(c, ..)| *c == cause.cause).unwrap_or_else(|| {
                    causes.push((cause.cause.clone(), 0, 0));
                    causes.len() - 1
                });
                if let Some(row) = causes.get_mut(at) {
                    if lane == 0 { row.1 += cause.count } else { row.2 += cause.count }
                }
            }
        }
        causes.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)).then_with(|| a.0.cmp(&b.0)));
        let font = FontId::proportional(10.5);
        let (gate_x, ready_x) = (right - 34.0, right);
        let row_height = 13.0;
        let mut y = causes_top + 2.0;
        let weak = visuals.weak_text_color();
        if causes.is_empty() {
            painter.text(pos2(right, y + 5.0), Align2::RIGHT_CENTER, "no failures", font, weak);
            return;
        }
        painter.text(pos2(left, y + 5.0), Align2::LEFT_CENTER, "failed by", font.clone(), weak);
        painter.text(pos2(gate_x, y + 5.0), Align2::RIGHT_CENTER, "gate", font.clone(), weak);
        painter.text(pos2(ready_x, y + 5.0), Align2::RIGHT_CENTER, "ready", font.clone(), weak);
        y += row_height;
        let red = legible(rgb(swarmception_domain::tooltip::FAILED), visuals);
        let rows = causes.len();
        for (i, (cause, gate, ready)) in causes.iter().enumerate() {
            let last_room = y + row_height * 2.0 > body.bottom();
            if last_room && i + 1 < rows {
                let more = rows - i;
                painter.text(pos2(left, y + 5.0), Align2::LEFT_CENTER, format!("+{more} more"), font.clone(), weak);
                self.drawn.causes.extend(causes.iter().skip(i).map(|(c, g, r)| format!("{c} {g} {r}")));
                break;
            }
            let cut: String = cause.chars().take(16).collect();
            painter.text(pos2(left, y + 5.0), Align2::LEFT_CENTER, cut, font.clone(), visuals.text_color());
            let count = |n: usize| if n == 0 { "·".to_owned() } else { n.to_string() };
            painter.text(pos2(gate_x, y + 5.0), Align2::RIGHT_CENTER, count(*gate), font.clone(), if *gate > 0 { red } else { weak });
            painter.text(pos2(ready_x, y + 5.0), Align2::RIGHT_CENTER, count(*ready), font.clone(), if *ready > 0 { red } else { weak });
            self.drawn.causes.push(format!("{cause} {gate} {ready}"));
            y += row_height;
        }
    }

    /// Both lanes' runs per clock hour (#75), mirrored about one baseline as the timeline's rows are
    /// ordered: the gate's growing up from it, the `ready` checks' down, each stacked by result
    /// (passed, failed, no result) on its own scale, each hour's column under its hour on the
    /// timeline. The hour under the pointer, if any.
    fn hours(
        &mut self,
        (painter, visuals): (&egui::Painter, &egui::Visuals),
        list: &GatesList,
        scale: &Scale,
        chart: Rect,
        pointer: Option<egui::Pos2>,
    ) -> Option<GateHour> {
        let most = |lane: Lane| list.hours.iter().map(|h| hour_of(h, lane).runs).max().unwrap_or(0).max(1);
        let (most_gate, most_ready) = (most(Lane::Gate), most(Lane::Ready));
        let mid = chart.center().y.round();
        // Room for each column's count at its end, when the chart is tall enough to spare it.
        let room = if chart.height() >= COUNTED_CHART { 10.0 } else { 0.0 };
        let halves = [(Lane::Gate, chart.top() + room, mid - 1.0, most_gate), (Lane::Ready, mid + 1.0, chart.bottom() - room, most_ready)];
        painter.hline(chart.x_range(), mid, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color));
        let mut under = None;
        for hour in &list.hours {
            let Some((a, b)) = scale.across(hour.start, Timestamp(hour.start.0.saturating_add(3_600))) else { continue };
            let inset = ((b - a) * 0.12).clamp(0.5, 6.0);
            let (left, right) = (a + inset, (b - inset).max(a + inset + 1.0));
            for (lane, top, bottom, most) in halves {
                let figures = hour_of(hour, lane);
                let unit = (bottom - top).max(0.0) / most as f32;
                let edge = if lane == Lane::Gate { mid - 1.0 } else { mid + 1.0 };
                let rect = stack((painter, visuals), figures, lane, (left, right, edge), unit, room > 0.0);
                self.drawn.hour_columns.push(HourDrawn {
                    start: hour.start,
                    lane,
                    runs: figures.runs,
                    passed: figures.passed,
                    failed: figures.failed,
                    no_result: figures.no_result,
                    rect,
                });
            }
            let column = Rect::from_min_max(pos2(left, chart.top()), pos2(right, chart.bottom()));
            if pointer.is_some_and(|p| column.x_range().contains(p.x) && chart.y_range().contains(p.y)) {
                painter.rect_stroke(column.expand(0.5), 1.0, Stroke::new(1.0, visuals.strong_text_color()), StrokeKind::Inside);
                under = Some(hour.clone());
            }
            self.drawn.hours += 1;
        }
        scales((painter, visuals), chart, most_gate, most_ready);
        under
    }
}

/// Where times fall across the screen: the window from `x0` to `x1`, through the timeline's own
/// mapping when lined up under it (its folds, #86), else evenly.
struct Scale {
    start: Timestamp,
    end: Timestamp,
    x0: f32,
    x1: f32,
    through: Option<TimeAxis>,
}

impl Scale {
    fn x(&self, t: Timestamp) -> Option<f32> {
        (self.start <= t && t <= self.end).then(|| self.at(t))
    }

    fn at(&self, t: Timestamp) -> f32 {
        if let Some(axis) = &self.through {
            return axis.x(t).clamp(self.x0, self.x1);
        }
        let width = self.end.0.saturating_sub(self.start.0).max(1) as f64;
        let fraction = t.0.saturating_sub(self.start.0) as f64 / width;
        self.x0 + (self.x1 - self.x0) * fraction as f32
    }

    /// `start` to `end` across, clipped to the window; none outside it.
    fn across(&self, start: Timestamp, end: Timestamp) -> Option<(f32, f32)> {
        if end < self.start || start > self.end {
            return None;
        }
        let a = self.at(start.max(self.start));
        let b = self.at(end.min(self.end));
        (a.is_finite() && b.is_finite()).then_some((a, b.max(a)))
    }
}

/// One lane's column of an hour from the baseline at `edge`, between `left` and `right`: up for
/// the gate, down for the `ready` checks, stacked by result, `unit` tall a run; its count at its
/// end when `counted` and the column is wide enough. The column drawn.
fn stack(
    (painter, visuals): (&egui::Painter, &egui::Visuals),
    figures: &swarmception_domain::gates::HourFigures,
    lane: Lane,
    (left, right, edge): (f32, f32, f32),
    unit: f32,
    counted: bool,
) -> Rect {
    let up = lane == Lane::Gate;
    let mut from = edge;
    let colours = [
        (figures.passed, swarmception_domain::tooltip::PASSED),
        (figures.failed, swarmception_domain::tooltip::FAILED),
        (figures.no_result, swarmception_domain::gates::NO_RESULT),
    ];
    for (count, colour) in colours.into_iter().filter(|(count, _)| *count > 0) {
        let height = unit * count as f32;
        let to = if up { from - height } else { from + height };
        let rect = Rect::from_min_max(pos2(left, from.min(to)), pos2(right, from.max(to)));
        painter.rect_filled(rect, 1.0, rgb(colour).gamma_multiply(if up { 0.7 } else { 0.55 }));
        from = to;
    }
    if figures.runs > 0 && counted && right - left >= 16.0 {
        let (at, align) = if up { (from - 1.0, Align2::CENTER_BOTTOM) } else { (from + 1.0, Align2::CENTER_TOP) };
        painter.text(pos2((left + right) / 2.0, at), align, figures.runs.to_string(), FontId::proportional(9.5), visuals.weak_text_color());
    }
    Rect::from_min_max(pos2(left, edge.min(from)), pos2(right, edge.max(from)))
}

/// The hours' scales, over the chart's left corners: the gate's at the top, the `ready` checks'
/// at the bottom, each on a backing so the columns under them don't hide them.
fn scales((painter, visuals): (&egui::Painter, &egui::Visuals), chart: Rect, most_gate: usize, most_ready: usize) {
    let font = FontId::proportional(9.5);
    for (text, at, align) in [
        (format!("▲ gate / h  ≤ {most_gate}"), pos2(chart.left() + 2.0, chart.top()), Align2::LEFT_TOP),
        (format!("▼ ready / h  ≤ {most_ready}"), pos2(chart.left() + 2.0, chart.bottom()), Align2::LEFT_BOTTOM),
    ] {
        let galley = painter.layout_no_wrap(text, font.clone(), visuals.weak_text_color());
        let rect = align.anchor_size(at, galley.size());
        painter.rect_filled(rect.expand2(vec2(2.0, 0.0)), 2.0, visuals.panel_fill);
        painter.galley(rect.min, galley, visuals.weak_text_color());
    }
}

/// One lane's figures of an hour.
const fn hour_of(hour: &GateHour, lane: Lane) -> &swarmception_domain::gates::HourFigures {
    match lane {
        Lane::Gate => &hour.gate,
        Lane::Ready => &hour.ready,
    }
}

/// An hour's figures, both lanes, for its tooltip.
fn hour_lines(hour: &GateHour) -> Vec<TipLine> {
    let lane = |name: &str, f: &swarmception_domain::gates::HourFigures| {
        TipLine::strong(format!("{name}: "))
            .then(format!("{} runs · ", f.runs))
            .then_coloured(format!("{} passed", f.passed), swarmception_domain::tooltip::PASSED)
            .then(" · ")
            .then_coloured(format!("{} failed", f.failed), swarmception_domain::tooltip::FAILED)
            .then(format!(" · pass {} · median {} · busy {}", percent(f.pass_rate), span(f.median_seconds), duration(f.busy_seconds)))
    };
    vec![
        TipLine::strong(format!("{} → {} UTC", clock(hour.start), clock(Timestamp(hour.start.0.saturating_add(3_600))))),
        lane("gate", &hour.gate),
        lane("ready", &hour.ready),
    ]
}

/// `lines` as a tooltip, each in its colours.
fn tip(ui: &mut egui::Ui, lines: &[TipLine]) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    for line in lines {
        let mut job = line_job(line, ui.visuals(), &font, None);
        job.wrap.max_width = ui.spacing().tooltip_width * 1.4;
        ui.label(job);
    }
}

/// A figure: its label quiet, its value strong (in its colour when it has one), on a pill.
fn pill(ui: &mut egui::Ui, label: &str, value: &str, colour: Option<Color32>) -> egui::Response {
    use egui::text::{LayoutJob, TextFormat};
    let visuals = ui.visuals().clone();
    let value_colour = colour.map_or_else(|| visuals.strong_text_color(), |c| legible(c, &visuals));
    let mut job = LayoutJob::default();
    let small = TextFormat { font_id: FontId::proportional(10.5), color: visuals.weak_text_color(), valign: egui::Align::Center, ..TextFormat::default() };
    job.append(label, 0.0, small);
    let strong = TextFormat { font_id: egui::TextStyle::Body.resolve(ui.style()), color: value_colour, valign: egui::Align::Center, ..TextFormat::default() };
    job.append(value, 5.0, strong);
    let galley = ui.painter().layout_job(job);
    let pad = vec2(7.0, 2.5);
    let (rect, response) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::hover());
    ui.painter().rect(rect, 8.0, visuals.faint_bg_color, Stroke::NONE, StrokeKind::Inside);
    ui.painter().galley(rect.min + pad, galley, visuals.text_color());
    response
}

/// The figures of a lane as the header reads them: for a test.
#[must_use]
pub fn lane_text(figures: &Figures) -> String {
    format!("{} · {}", percent(figures.pass_rate), span(figures.median_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale() -> Scale {
        Scale { start: Timestamp(1_000), end: Timestamp(2_000), x0: 100.0, x1: 1_100.0, through: None }
    }

    #[test]
    fn times_fall_across_the_window() {
        let s = scale();
        assert_eq!(s.x(Timestamp(1_500)), Some(600.0));
        assert_eq!(s.x(Timestamp(999)), None);
        assert_eq!(s.across(Timestamp(500), Timestamp(1_100)), Some((100.0, 200.0)), "clipped at the start");
        assert_eq!(s.across(Timestamp(2_500), Timestamp(3_000)), None);
        let empty = Scale { start: Timestamp(5), end: Timestamp(5), x0: 0.0, x1: 10.0, through: None };
        assert_eq!(empty.across(Timestamp(5), Timestamp(5)), Some((0.0, 0.0)), "no width: no division by nothing");
    }

    #[test]
    fn figures_read_as_a_person_reads_them() {
        assert_eq!(clock(Timestamp::parse("2026-10-31T13:05:09Z").unwrap()), "13:05");
        assert_eq!(span(Some(384)), "6 min 24 s");
        assert_eq!(span(None), "–");
        let figures = Figures { pass_rate: Some(0.875), median_seconds: Some(64), ..Figures::default() };
        assert_eq!(lane_text(&figures), "88% · 1 min 04 s");
    }
}
