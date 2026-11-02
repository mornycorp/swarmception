//! The Gates pane (#61, View ▸ Gates): how the gate behaved over the timeline's window, drawn from
//! what `gates.list` returns (`swarmception_domain::gates` defines each figure), asked again only
//! when the window or the engine's data moves on.
//!
//! A band under the timeline. Along its top, the headline figures: the coordinator's gate's pass
//! rate, median run, how busy it was and its reruns, the longest queue at the dock, and the
//! workers' `ready` checks' pass rate and median. Under them, the runs along time, lined up under
//! the timeline's rows (its window, across the same columns): a lane of the coordinator's gate
//! runs and one of the workers' `ready` checks, each run a bar as long as it took, in its result's
//! colour (passed green, failed red, no result grey, running blue); under the lanes, the gate's
//! runs per clock hour, stacked by result. In the left column, beside them, the lanes' names and
//! a short table of the failures by cause. Hovering a run shows its story in brief and lights
//! its tickets' bars on the timeline (the shell hands [`GatesPane::hovered`] to
//! `TimelinePane::light_hover`); clicking it makes it the Details pane's subject.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use swarmception_domain::details::Subject;
use swarmception_domain::gates::{Figures, GateHour, GatesList, Lane, Run};
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
use crate::timeline::Hover;

/// A run as the pane drew it.
#[derive(Debug, Clone, PartialEq)]
pub struct RunDrawn {
    pub sub: String,
    pub start: Timestamp,
    pub lane: Lane,
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
    /// Each run drawn: the coordinator's lane, then the workers', each in `gates.list`'s order.
    pub runs: Vec<RunDrawn>,
    /// The hours' columns drawn.
    pub hours: usize,
    /// Where the window's start and end were across.
    pub axis: Option<(f32, f32)>,
    /// The run under the pointer, and the subject a click on one chose.
    pub hovered: Option<(String, Timestamp)>,
    pub chosen: Option<Subject>,
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
    /// The run under the pointer: what the timeline lights.
    hovered: Hover,
    drawn: GatesDrawn,
}

const GATE_LANE: f32 = 20.0;
const READY_LANE: f32 = 14.0;
const LANE_GAP: f32 = 5.0;
/// The tallest the hours' chart grows.
const CHART: f32 = 60.0;
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

    /// The run under the pointer last frame, with the tickets it carried.
    #[must_use]
    pub const fn hovered(&self) -> &Hover {
        &self.hovered
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

    /// Forgets the hover: the pane wasn't drawn.
    pub fn leave(&mut self) {
        self.hovered = Hover::Nothing;
        self.drawn = GatesDrawn::default();
    }

    /// Draws the pane; `axis`, where the timeline's window starts and ends across the screen, to
    /// line the runs up under it (none: the pane's own); `selected`, the Details pane's subject,
    /// outlined; `t`, the factory's time, marked.
    pub fn ui(&mut self, ui: &mut egui::Ui, axis: Option<(f32, f32)>, selected: Option<&Subject>, t: Option<Timestamp>) {
        self.drawn = GatesDrawn::default();
        self.hovered = Hover::Nothing;
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
        if body.height() < GATE_LANE + 4.0 || body.width() < NARROWEST_COLUMN + 40.0 {
            return;
        }
        let response = ui.allocate_rect(body, Sense::click());
        let (x0, x1) = match axis {
            Some((x0, x1)) if x0 - body.left() >= NARROWEST_COLUMN && x1 - x0 > 40.0 => (x0, x1.min(body.right() + 16.0)),
            _ => (body.left() + OWN_COLUMN, body.right() - 8.0),
        };
        self.drawn.axis = Some((x0, x1));
        let scale = Scale { start: list.start, end: list.end, x0, x1 };
        // Out to the timeline's right edge, which the panel's margin can pass.
        let painter = ui.painter_at(body.with_max_x(body.right().max(x1)));
        let visuals = ui.visuals().clone();
        let gate_top = body.top() + 4.0;
        let ready_top = gate_top + GATE_LANE + LANE_GAP;
        let hours_top = ready_top + READY_LANE + LANE_GAP + 2.0;
        let lanes = Rect::from_min_max(pos2(x0, gate_top - 2.0), pos2(x1, ready_top + READY_LANE + 2.0));
        painter.rect_filled(lanes, 3.0, visuals.faint_bg_color);
        self.column((&painter, &visuals), &list, body, x0, (gate_top, ready_top, hours_top));
        if let Some(x) = t.and_then(|t| scale.x(t)) {
            painter.vline(x, body.top()..=body.bottom(), Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.6)));
        }
        self.lane((&painter, &visuals), &list, &scale, (Lane::Gate, gate_top, GATE_LANE, 2), selected);
        self.lane((&painter, &visuals), &list, &scale, (Lane::Ready, ready_top, READY_LANE, 3), selected);
        let chart = Rect::from_min_max(pos2(x0, hours_top), pos2(x1, (body.bottom() - 2.0).min(hours_top + CHART)));
        let hour = if chart.height() >= 14.0 { self.hours((&painter, &visuals), &list, &scale, chart, response.hover_pos()) } else { None };
        if list.runs.is_empty() {
            painter.text(lanes.center(), Align2::CENTER_CENTER, "No gate runs in this window", FontId::proportional(11.0), visuals.weak_text_color());
        }
        self.interact(ui, response, &list, hour);
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
                "The coordinator's gate runs that started in the window: {} passed of {} with a result ({} runs: {} failed, {} with no result, {} running).\nPassed: rc=0, or it merged a ticket. Failed: another rc, or it only sent tickets back.",
                gate.passed,
                gate.passed + gate.failed,
                gate.runs,
                gate.failed,
                gate.no_result,
                gate.running
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
    /// The left column: the lanes' names and counts, then the failures by cause.
    fn column(&mut self, (painter, visuals): (&egui::Painter, &egui::Visuals), list: &GatesList, body: Rect, x0: f32, tops: (f32, f32, f32)) {
        let (gate_top, ready_top, hours_top) = tops;
        let right = x0 - 8.0;
        let left = (right - OWN_COLUMN + 16.0).max(body.left() + 6.0);
        let name = |y: f32, text: String| {
            painter.text(pos2(right, y), Align2::RIGHT_CENTER, text, FontId::proportional(11.5), visuals.text_color());
        };
        name(gate_top + GATE_LANE / 2.0, format!("gate runs  {}", list.gate.runs));
        name(ready_top + READY_LANE / 2.0, format!("ready checks  {}", list.ready.runs));
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
        let mut y = hours_top + 2.0;
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

    /// One lane's runs, packed into at most `rows` rows where they overlap.
    fn lane(
        &mut self,
        (painter, visuals): (&egui::Painter, &egui::Visuals),
        list: &GatesList,
        scale: &Scale,
        (lane, top, height, rows): (Lane, f32, f32, usize),
        selected: Option<&Subject>,
    ) {
        let runs: Vec<&Run> = list.runs.iter().filter(|r| r.lane == lane).collect();
        let used = packed_rows(&runs, list.end, rows);
        let count = used.iter().copied().max().map_or(1, |m| m + 1);
        let row_height = height / count as f32;
        for (run, row) in runs.iter().zip(used) {
            let Some((a, b)) = scale.across(run.start, run.end.unwrap_or(list.end.max(run.start))) else { continue };
            let y = top + row_height * row as f32;
            let rect = Rect::from_min_max(pos2(a, y + 0.5), pos2(b.max(a + 2.0), y + row_height - 0.5));
            let fill = rgb(run.colour);
            painter.rect_filled(rect, 2.0, fill);
            if run.end.is_none() {
                // Still running: its end fades, it goes on.
                let tail = Rect::from_min_max(pos2((rect.right() - 6.0).max(rect.left()), rect.top()), rect.max);
                painter.rect_filled(tail, 0.0, fill.gamma_multiply(0.5));
            }
            if run.rerun_of.is_some() && rect.width() >= 5.0 {
                painter.vline(rect.left() + 1.0, rect.y_range(), Stroke::new(1.5, visuals.strong_text_color().gamma_multiply(0.7)));
            }
            if selected == Some(&run.subject) && lane == Lane::Gate {
                painter.rect_stroke(rect.expand(1.5), 2.0, Stroke::new(1.5, visuals.strong_text_color()), StrokeKind::Outside);
            }
            self.drawn.runs.push(RunDrawn { sub: run.sub.clone(), start: run.start, lane, rect });
        }
    }

    /// The gate's runs per clock hour, stacked by result: passed, failed, no result. The hour
    /// under the pointer, if any.
    fn hours(
        &mut self,
        (painter, visuals): (&egui::Painter, &egui::Visuals),
        list: &GatesList,
        scale: &Scale,
        chart: Rect,
        pointer: Option<egui::Pos2>,
    ) -> Option<GateHour> {
        let most = list.hours.iter().map(|h| h.gate.runs).max().unwrap_or(0).max(1);
        painter.hline(chart.x_range(), chart.bottom(), Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color));
        painter.text(
            pos2(chart.left() + 2.0, chart.top()),
            Align2::LEFT_TOP,
            format!("gate runs / h (most {most})"),
            FontId::proportional(9.5),
            visuals.weak_text_color(),
        );
        let mut under = None;
        for hour in &list.hours {
            let Some((a, b)) = scale.across(hour.start, Timestamp(hour.start.0.saturating_add(3_600))) else { continue };
            let inset = ((b - a) * 0.12).clamp(0.5, 6.0);
            let column = Rect::from_min_max(pos2(a + inset, chart.top() + 12.0), pos2((b - inset).max(a + inset + 1.0), chart.bottom() - 1.0));
            let unit = column.height() / most as f32;
            if hour.gate.runs > 0 && column.width() >= 16.0 {
                let top = column.bottom() - unit * hour.gate.runs as f32;
                painter.text(
                    pos2(column.center().x, top - 1.0),
                    Align2::CENTER_BOTTOM,
                    hour.gate.runs.to_string(),
                    FontId::proportional(9.5),
                    visuals.weak_text_color(),
                );
            }
            let mut bottom = column.bottom();
            let colours = [
                (hour.gate.passed, swarmception_domain::tooltip::PASSED),
                (hour.gate.failed, swarmception_domain::tooltip::FAILED),
                (hour.gate.no_result, swarmception_domain::gates::NO_RESULT),
            ];
            for (count, colour) in colours {
                if count == 0 {
                    continue;
                }
                let top = bottom - unit * count as f32;
                painter.rect_filled(Rect::from_min_max(pos2(column.left(), top), pos2(column.right(), bottom)), 1.0, rgb(colour).gamma_multiply(0.7));
                bottom = top;
            }
            if pointer.is_some_and(|p| column.x_range().contains(p.x) && chart.y_range().contains(p.y)) {
                painter.rect_stroke(column.expand(0.5), 1.0, Stroke::new(1.0, visuals.strong_text_color()), StrokeKind::Inside);
                under = Some(hour.clone());
            }
            self.drawn.hours += 1;
        }
        under
    }

    /// The pointer over the band: a run's tooltip and the timeline's highlight, a click making it
    /// the Details' subject; or an hour's figures.
    fn interact(&mut self, ui: &egui::Ui, response: egui::Response, list: &GatesList, hour: Option<GateHour>) {
        let found = response.hover_pos().and_then(|p| {
            self.drawn
                .runs
                .iter()
                .filter(|r| r.rect.expand2(vec2(2.0, 1.0)).contains(p))
                .min_by(|a, b| a.rect.width().total_cmp(&b.rect.width()))
                .and_then(|drawn| list.runs.iter().find(|r| r.sub == drawn.sub && r.start == drawn.start && r.lane == drawn.lane))
        });
        if let Some(run) = found {
            if let Some(drawn) = self.drawn.runs.iter().find(|d| d.sub == run.sub && d.start == run.start) {
                ui.painter().rect_stroke(drawn.rect.expand(1.0), 2.0, Stroke::new(1.5, ui.visuals().strong_text_color()), StrokeKind::Outside);
            }
            self.drawn.hovered = Some((run.sub.clone(), run.start));
            self.hovered = Hover::Run { sub: run.sub.clone(), start: run.start, tickets: run.tickets.clone() };
            if response.clicked() {
                self.drawn.chosen = Some(run.subject.clone());
            }
            let lines = run.tooltip.clone();
            response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_ui_at_pointer(|ui| tip(ui, &lines));
        } else if let Some(hour) = hour {
            let lines = hour_lines(&hour);
            response.on_hover_ui_at_pointer(|ui| tip(ui, &lines));
        }
    }
}

/// Where times fall across the screen: the window from `x0` to `x1`.
struct Scale {
    start: Timestamp,
    end: Timestamp,
    x0: f32,
    x1: f32,
}

impl Scale {
    fn x(&self, t: Timestamp) -> Option<f32> {
        (self.start <= t && t <= self.end).then(|| self.at(t))
    }

    fn at(&self, t: Timestamp) -> f32 {
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

/// Each run's row in its lane: the first whose last run ended by the time it started (runs that
/// went on at once side by side; a run still going, to `last`), at most `rows`, the last taking
/// what is left.
fn packed_rows(runs: &[&Run], last: Timestamp, rows: usize) -> Vec<usize> {
    let mut ends: Vec<Timestamp> = Vec::new();
    runs.iter()
        .map(|run| {
            let end = run.end.unwrap_or(last).max(run.start);
            let row = ends.iter().position(|&e| e <= run.start).unwrap_or_else(|| {
                ends.push(Timestamp(i64::MIN));
                ends.len() - 1
            });
            let row = row.min(rows.saturating_sub(1));
            if let Some(e) = ends.get_mut(row) {
                *e = (*e).max(end);
            }
            row
        })
        .collect()
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
        Scale { start: Timestamp(1_000), end: Timestamp(2_000), x0: 100.0, x1: 1_100.0 }
    }

    #[test]
    fn times_fall_across_the_window() {
        let s = scale();
        assert_eq!(s.x(Timestamp(1_500)), Some(600.0));
        assert_eq!(s.x(Timestamp(999)), None);
        assert_eq!(s.across(Timestamp(500), Timestamp(1_100)), Some((100.0, 200.0)), "clipped at the start");
        assert_eq!(s.across(Timestamp(2_500), Timestamp(3_000)), None);
        let empty = Scale { start: Timestamp(5), end: Timestamp(5), x0: 0.0, x1: 10.0 };
        assert_eq!(empty.across(Timestamp(5), Timestamp(5)), Some((0.0, 0.0)), "no width: no division by nothing");
    }

    #[test]
    fn overlapping_runs_take_rows_up_to_a_limit() {
        use swarmception_domain::gates::GateResult;
        let run = |start: i64, end: i64| Run {
            sub: format!("ready-{start}"),
            lane: Lane::Ready,
            instance: String::new(),
            name: String::new(),
            start: Timestamp(start),
            end: Some(Timestamp(end)),
            duration_seconds: end - start,
            kind: None,
            result: GateResult::Passed,
            cause: None,
            prs: Vec::new(),
            tickets: Vec::new(),
            merges: Vec::new(),
            sendbacks: Vec::new(),
            rerun_of: None,
            inferred: false,
            colour: [0, 0, 0],
            subject: Subject::Ticket { issue: 1 },
            tooltip: Vec::new(),
        };
        let runs = [run(1_000, 1_200), run(1_100, 1_300), run(1_150, 1_400), run(1_250, 1_500), run(1_600, 1_700)];
        let refs: Vec<&Run> = runs.iter().collect();
        assert_eq!(packed_rows(&refs, Timestamp(2_000), 3), [0, 1, 2, 0, 0]);
        assert_eq!(packed_rows(&refs, Timestamp(2_000), 2), [0, 1, 1, 0, 0], "the last row takes the rest");
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
