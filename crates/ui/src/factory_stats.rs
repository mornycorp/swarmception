//! The statistics strip in the Factory's header (#22): what `stats.get` returns
//! (`swarmception_domain::stats` defines each figure) for the timeline's visible window, asked
//! again only when the window or the engine's data moves on, never every frame.
//!
//! Left to right: **landed per active hour**, large, with "N landed · H active h" beside it; a
//! sparkline of the hourly series, the hour holding the factory's *t* marked, each bar clickable
//! (#31: the shell zooms the timeline's window to that hour and puts *t* in it); then, compact, the
//! median claim → merge, the median wait at the dock, the gates' pass rate (failures by cause in
//! its tooltip), sendbacks per landed, the mean tickets working and in flight, and the workers'
//! utilisation (each worker in its tooltip). Every figure's tooltip says how it is computed.

use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use swarmception_domain::Stats;
use swarmception_domain::factory::short;
use swarmception_domain::stats::Hour;
use swarmception_engine::Session;
use swarmception_engine::stats_cmds::{At as StatsAt, Get as GetStats, GetParams as StatsParams};
use swarmception_model::Timestamp;

use crate::factory_chip::{PASSED, WORKING};

/// What the strip drew, for a test to check against `stats.get`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripDrawn {
    /// The headline, as drawn: landed per active hour.
    pub headline: String,
    /// "N landed · H active h".
    pub landed: String,
    /// The compact figures, in order, as drawn.
    pub figures: Vec<String>,
    /// The sparkline's bars, one an hour, and the one marked (holding *t*).
    pub bars: usize,
    pub marked: Option<usize>,
    /// Where each bar was drawn, its hour's start: for a test to click one.
    pub hours: Vec<(Timestamp, Rect)>,
    /// The hour whose bar was clicked this frame, its start: the shell moves the timeline's
    /// window to it and the factory's *t* with it (#31).
    pub chosen: Option<Timestamp>,
}

/// What a strip was asked for: the window, and the engine's revision.
type Asked = (Option<(Timestamp, Timestamp)>, (u64, u64));

/// The statistics for the window, and when they were asked.
#[derive(Debug, Default)]
pub struct StatsStrip {
    stats: Option<Stats>,
    asked: Option<Asked>,
    /// How many times `stats.get` ran: the test that it isn't asked every frame reads it.
    asks: u64,
    drawn: StripDrawn,
}

/// A rate to one decimal; a dash for none.
#[must_use]
pub fn rate(value: Option<f64>) -> String {
    value.filter(|v| v.is_finite()).map_or_else(|| "–".to_owned(), |v| format!("{v:.1}"))
}

/// A share as a whole percentage; a dash for none.
#[must_use]
pub fn percent(value: Option<f64>) -> String {
    value.filter(|v| v.is_finite()).map_or_else(|| "–".to_owned(), |v| format!("{:.0}%", v * 100.0))
}

/// Seconds as a short duration; a dash for none.
fn span(seconds: Option<i64>) -> String {
    seconds.map_or_else(|| "–".to_owned(), short)
}

/// Hours to one decimal.
fn hours(seconds: i64) -> String {
    format!("{:.1}", seconds.max(0) as f64 / 3_600.0)
}

/// `HH:MM` of a time, UTC.
fn clock(t: Timestamp) -> String {
    let (_, _, _, h, m, _) = t.civil();
    format!("{h:02}:{m:02}")
}

impl StatsStrip {
    /// The statistics it shows.
    #[must_use]
    pub const fn stats(&self) -> Option<&Stats> {
        self.stats.as_ref()
    }

    /// How many times it has asked `stats.get`.
    #[must_use]
    pub const fn asks(&self) -> u64 {
        self.asks
    }

    /// What it drew last frame.
    #[must_use]
    pub const fn drawn(&self) -> &StripDrawn {
        &self.drawn
    }

    /// Asks `stats.get` for `window` (the whole run when none) when it or the engine moved on.
    pub fn sync(&mut self, engine: &mut Session, window: Option<(Timestamp, Timestamp)>) {
        let asked = Some((window, engine.revision()));
        if self.asked == asked {
            return;
        }
        self.asked = asked;
        self.asks += 1;
        let params = window.map_or_else(StatsParams::default, |(start, end)| StatsParams {
            start: Some(StatsAt::Seconds(start.0)),
            end: Some(StatsAt::Seconds(end.0.max(start.0))),
        });
        self.stats = engine.call::<GetStats>(params).ok();
    }

    /// Draws the strip, the sparkline marking the hour that holds `t`.
    pub fn ui(&mut self, ui: &mut egui::Ui, t: Timestamp) {
        self.drawn = StripDrawn::default();
        let Some(stats) = self.stats.clone() else { return };
        // Wrapped, so a narrow pane takes the figures onto a second line rather than clipping them.
        ui.horizontal_wrapped(|ui| {
            self.headline(ui, &stats);
            ui.separator();
            self.sparkline(ui, &stats, t);
            ui.separator();
            self.figures(ui, &stats);
        });
    }

    fn headline(&mut self, ui: &mut egui::Ui, stats: &Stats) {
        let window = format!("{} → {} UTC", clock(stats.start), clock(stats.end));
        ui.weak(egui::RichText::new(window).small()).on_hover_text(format!("The timeline's visible window: {} → {}", stats.start, stats.end));
        let headline = rate(stats.landed_per_active_hour);
        let quiet = short(stats.quiet_after_seconds);
        let how = format!(
            "Tickets landed per active hour: the {} tickets merged in the window ÷ its {} active hours.\n\
             Active: the time at least one worker has a session going ({} h on shift), its pauses left out, \
             less every gap of over {quiet} with no event at all ({} such gaps).",
            stats.landed,
            hours(stats.active_seconds),
            hours(stats.on_shift_seconds),
            stats.quiet_gaps
        );
        ui.label(egui::RichText::new(&headline).size(22.0).strong().color(PASSED)).on_hover_text(&how);
        let landed = format!("{} landed · {} active h", stats.landed, hours(stats.active_seconds));
        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.label(egui::RichText::new("landed / active h").small()).on_hover_text(&how);
            ui.weak(egui::RichText::new(&landed).small()).on_hover_text(&how);
        });
        self.drawn.headline = headline;
        self.drawn.landed = landed;
    }

    /// The hourly series as bars, the hour holding `t` in the working colour and outlined.
    fn sparkline(&mut self, ui: &mut egui::Ui, stats: &Stats, t: Timestamp) {
        let n = stats.hours.len();
        let width = (n as f32 * 7.0).clamp(60.0, 220.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
        let painter = ui.painter_at(rect);
        let weak = ui.visuals().weak_text_color();
        painter.rect_stroke(rect, 2.0, Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color), StrokeKind::Inside);
        let max = stats.hours.iter().filter_map(|h| h.landed_per_active_hour).fold(0.0_f64, f64::max);
        let step = rect.width() / n.max(1) as f32;
        let marked = stats.hours.iter().position(|h| h.start <= t && t.0 < h.start.0.saturating_add(3_600));
        for (i, hour) in stats.hours.iter().enumerate() {
            let x = rect.left() + i as f32 * step;
            let bar = Rect::from_min_max(pos2(x + 0.5, rect.top() + 2.0), pos2(x + step - 0.5, rect.bottom() - 2.0));
            let value = hour.landed_per_active_hour.unwrap_or(0.0);
            let fill = if max > 0.0 { (value / max) as f32 } else { 0.0 };
            let colour = if Some(i) == marked { WORKING } else { PASSED.gamma_multiply(0.75) };
            if hour.active_seconds > 0 || hour.landed > 0 {
                let top = bar.bottom() - (bar.height() * fill).max(1.5);
                painter.rect_filled(bar.with_min_y(top), 1.0, colour);
            } else {
                painter.hline(bar.x_range(), bar.bottom(), Stroke::new(1.0, weak.gamma_multiply(0.5)));
            }
            if Some(i) == marked {
                painter.rect_stroke(bar.expand2(vec2(0.5, 1.5)), 1.0, Stroke::new(1.0, WORKING), StrokeKind::Outside);
            }
            self.drawn.hours.push((hour.start, bar));
        }
        if n == 0 {
            painter.text(rect.center(), Align2::CENTER_CENTER, "no hours", FontId::proportional(10.0), weak);
        }
        let under = |p: egui::Pos2| stats.hours.get(((p.x - rect.left()) / step).floor().max(0.0) as usize);
        let hovered = response.hover_pos().and_then(under);
        if let Some(hour) = hovered {
            let x = rect.left() + step * stats.hours.iter().position(|h| h.start == hour.start).unwrap_or(0) as f32;
            let bar = Rect::from_min_max(pos2(x, rect.top() + 1.0), pos2(x + step, rect.bottom() - 1.0));
            painter.rect_stroke(bar, 1.0, Stroke::new(1.0, ui.visuals().strong_text_color()), StrokeKind::Inside);
        }
        if response.clicked() {
            self.drawn.chosen = response.interact_pointer_pos().and_then(under).map(|h| h.start);
        }
        let response = if hovered.is_some() { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response };
        let lines: Vec<String> = hovered.map_or_else(
            || vec!["Landed per active hour".to_owned(), "Each clock hour (UTC) of the window; the hour holding the factory's t is marked.".to_owned()],
            |hour| vec![format!("{} UTC", clock(hour.start)), hour_text(hour), "Click: the timeline and the factory to this hour".to_owned()],
        );
        crate::factory_tip::show(response, &lines);
        self.drawn.bars = n;
        self.drawn.marked = marked;
    }

    fn figures(&mut self, ui: &mut egui::Ui, stats: &Stats) {
        let quiet = short(stats.quiet_after_seconds);
        let gates = &stats.gates;
        let causes: Vec<String> = gates.causes.iter().map(|c| format!("{} {}", c.cause, c.count)).collect();
        let ready = &stats.ready;
        let figures = [
            (
                "claim → merge",
                span(stats.median_claim_to_merge_seconds),
                format!(
                    "The median, over the {} tickets landed in the window, of the time from a ticket's first claim to its merge (wall clock).",
                    stats.landed
                ),
            ),
            (
                "dock wait",
                span(stats.median_dock_wait_seconds),
                format!("The median wait at the dock, over the {} tickets landed: from a ticket's last submit to its merge (wall clock).", stats.landed),
            ),
            (
                "gates pass",
                percent(gates.pass_rate),
                format!(
                    "The coordinator's gates that started in the window: {} passed of {} with a result ({} started, {} with none).\nPassed: rc=0, or a merge names the gate. Failed: another rc.\nFailures by cause: {}\nWorkers' own ready gates: {} passed of {} ({}).",
                    gates.passed,
                    gates.passed + gates.failed,
                    gates.runs,
                    gates.no_result,
                    if causes.is_empty() { "none".to_owned() } else { causes.join(", ") },
                    ready.passed,
                    ready.passed + ready.failed,
                    percent(ready.pass_rate)
                ),
            ),
            (
                "sendbacks / landed",
                rate(stats.sendbacks_per_landed),
                format!("The {} sendbacks in the window ÷ the {} tickets landed.", stats.sendbacks, stats.landed),
            ),
            (
                "working · in flight",
                format!("{} · {}", rate(stats.mean_working), rate(stats.mean_in_flight)),
                format!(
                    "The mean tickets working (from work to its submit, sendback, stop or merge), and in flight (claimed, working, waiting or sent back), over the active time: each ticket's time in the phase ÷ the {} active hours.",
                    hours(stats.active_seconds)
                ),
            ),
            ("workers busy", percent(mean_utilisation(stats)), workers_text(stats, &quiet)),
        ];
        for (label, value, how) in figures {
            // A weak label, its value strong; the pair shares the tooltip.
            let response = ui
                .horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.weak(egui::RichText::new(label).small());
                    ui.label(egui::RichText::new(&value).strong());
                })
                .response;
            response.on_hover_text(how);
            ui.add_space(6.0);
            self.drawn.figures.push(format!("{label} {value}"));
        }
    }
}

/// The workers' utilisation, their working time ÷ their time on shift, all together.
fn mean_utilisation(stats: &Stats) -> Option<f64> {
    let on: i64 = stats.workers.iter().map(|w| w.on_shift_seconds).sum();
    let working: i64 = stats.workers.iter().map(|w| w.working_seconds).sum();
    (on > 0).then(|| working as f64 / on as f64)
}

fn workers_text(stats: &Stats, quiet: &str) -> String {
    let mut lines = vec![format!(
        "Utilisation: the time a worker's tickets were working ÷ its time on shift, both less the gaps of over {quiet} with no event, in the window. All together: {}.",
        percent(mean_utilisation(stats))
    )];
    for worker in &stats.workers {
        lines.push(format!(
            "{} ({}): {} · {} working of {} on shift · {} landed",
            worker.name,
            worker.machine,
            percent(worker.utilisation),
            short(worker.working_seconds),
            short(worker.on_shift_seconds),
            worker.landed
        ));
    }
    lines.join("\n")
}

/// One hour of the sparkline, for its tooltip.
fn hour_text(hour: &Hour) -> String {
    format!("{} landed in {} active · {} landed / active h", hour.landed, short(hour.active_seconds), rate(hour.landed_per_active_hour))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures_read_as_a_person_reads_them() {
        assert_eq!([rate(Some(4.26)), rate(None), rate(Some(f64::NAN))], ["4.3", "–", "–"]);
        assert_eq!([percent(Some(0.926)), percent(None)], ["93%", "–"]);
        assert_eq!(span(Some(2_520)), "42m");
        assert_eq!(hours(5_400), "1.5");
        assert_eq!(clock(Timestamp::parse("2026-10-31T13:05:00Z").unwrap()), "13:05");
    }
}
