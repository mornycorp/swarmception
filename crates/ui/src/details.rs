//! The details pane (#47): the story of what is selected, drawn from what `details.get` returns
//! (`swarmception_domain::details`). What it is about is [`DetailsView`], plain data in the
//! shell's state, which follows the selection: a bar chosen on the timeline (its ticket, or the
//! merge onto main it draws), a ticket chip or a gate run clicked in the factory, or a line,
//! a name or a chip clicked in the pane itself (← goes back).
//!
//! At the top, the header: a chip in the subject's colour with its mark (`#47`), its title, the
//! lines under it (GitHub's state and author, its PR), its labels, and its pages (Open Issue,
//! Open PR, as a bar's right-click opens them); then its figures, what it carries, and the story,
//! oldest first, a line an event: its time, a dot in its colour, what happened in that colour,
//! the rest of its line, who (click: their session) and how long it took; its notes under it.

use egui::text::LayoutJob;
use egui::{Color32, FontId, RichText, Sense, TextFormat, vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::colours::{Paint, ink};
use swarmception_domain::details::{Details, Entry, OnTimeline, PageKind, Subject};
use swarmception_domain::timeline::Link;
use swarmception_domain::tooltip::TipLine;
use swarmception_engine::Session;
use swarmception_engine::details_cmds::{DetailsParams, Get};
use swarmception_engine::stats_cmds::At as When;

use crate::catalog;
use crate::factory_chip::{drawn, rgb, shade};
use crate::fonts;
use crate::icons;
use crate::layout::{Chevron, fold_title};
use crate::part::Part;

/// What the details pane is about: plain data in the shell's state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DetailsView {
    /// What is selected, whose story the pane tells; none before anything is.
    pub subject: Option<Subject>,
}

/// What the pane drew last frame, for a test to check against what `details.get` returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetailsDrawn {
    pub mark: Option<String>,
    pub title: Option<String>,
    pub labels: Vec<String>,
    /// Each figure as `label value`.
    pub facts: Vec<String>,
    /// Each entry's line, [`Entry::text`].
    pub entries: Vec<String>,
    /// A subject clicked in the pane this frame: a line, a name, a chip.
    pub chosen: Option<Subject>,
    /// ← clicked this frame.
    pub back: bool,
    /// A page chosen this frame.
    pub open: Option<Link>,
    /// Its title was clicked this frame: fold it to a strip (#68).
    pub fold: bool,
}

/// The most subjects ← goes back through.
const HISTORY: usize = 32;
/// How far an entry's notes sit in: past its time and its dot, under what happened.
const NOTE_INDENT: i8 = 78;

/// The details the pane shows, and how it got there.
#[derive(Default)]
pub struct DetailsPane {
    details: Option<Details>,
    error: Option<String>,
    /// What the details were asked for, and the engine's revision then.
    asked: Option<(Option<Subject>, (u64, u64))>,
    /// The subjects before this one, the last most recent.
    history: Vec<Subject>,
    drawn: DetailsDrawn,
}

/// `details.get`'s params for `subject`.
#[must_use]
pub fn params(subject: &Subject) -> DetailsParams {
    let at = |t: &Option<swarmception_model::Timestamp>| t.map(|t| When::Seconds(t.0));
    match subject {
        Subject::Ticket { issue } => DetailsParams { ticket: Some(*issue), ..DetailsParams::default() },
        Subject::Gate { sub, at: t } => DetailsParams { gate: Some(sub.clone()), at: at(t), ..DetailsParams::default() },
        Subject::Session { instance, at: t } => DetailsParams { session: Some(instance.clone()), at: at(t), ..DetailsParams::default() },
        Subject::Merge { commit } => DetailsParams { merge: Some(commit.clone()), ..DetailsParams::default() },
    }
}

impl DetailsPane {
    /// The details it shows: what `details.get` returned for its subject.
    #[must_use]
    pub const fn details(&self) -> Option<&Details> {
        self.details.as_ref()
    }

    /// Where `view`'s subject is drawn on the timeline (#55), as `details.get` returned it: none
    /// until its details have come, or when it refused.
    #[must_use]
    pub fn on_timeline(&self, view: &DetailsView) -> Option<&OnTimeline> {
        let asked = self.asked.as_ref().map(|(subject, _)| subject);
        self.details.as_ref().filter(|_| view.subject.is_some() && asked == Some(&view.subject)).map(|d| &d.timeline)
    }

    /// Why there are none, when `details.get` refused.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// What it drew last frame.
    #[must_use]
    pub const fn drawn(&self) -> &DetailsDrawn {
        &self.drawn
    }

    /// Selects `subject`: the view on it, the one before kept for ←.
    pub fn select(&mut self, view: &mut DetailsView, subject: Subject) {
        if view.subject.as_ref() == Some(&subject) {
            return;
        }
        if let Some(before) = view.subject.take() {
            self.history.push(before);
            if self.history.len() > HISTORY {
                self.history.remove(0);
            }
        }
        view.subject = Some(subject);
    }

    /// Back to the subject before.
    pub fn back(&mut self, view: &mut DetailsView) {
        if let Some(before) = self.history.pop() {
            view.subject = Some(before);
        }
    }

    /// Asks `details.get` again when the subject or the engine moved on.
    pub fn sync(&mut self, engine: &mut Session, view: &DetailsView) {
        let asked = Some((view.subject.clone(), engine.revision()));
        if self.asked == asked {
            return;
        }
        self.asked = asked;
        let Some(subject) = &view.subject else {
            self.details = None;
            self.error = None;
            return;
        };
        match engine.call::<Get>(params(subject)) {
            Ok(details) => {
                self.details = Some(details);
                self.error = None;
            }
            Err(error) => {
                self.details = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// Draws the pane; what was clicked in it is in [`Self::drawn`].
    pub fn ui(&mut self, ui: &mut egui::Ui, view: &DetailsView) {
        self.drawn = DetailsDrawn::default();
        if ui.is_sizing_pass() {
            // The panel's first frame lays out unwrapped to size it: a long note would make it
            // as wide as that note. It takes its default width instead.
            return;
        }
        ui.horizontal(|ui| {
            self.drawn.fold = fold_title(ui, icons::INFO, catalog::DETAILS_2, Chevron::FoldRight).clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let back =
                    ui.add_enabled(!self.history.is_empty(), egui::Button::new(icons::BACK).small()).on_hover_text(catalog::BACK_TO_WHAT_WAS_SELECTED_BEFORE);
                self.drawn.back = back.clicked();
            });
        });
        ui.separator();
        let details = self.details.clone();
        match (&view.subject, details, &self.error) {
            (None, ..) => {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.weak(catalog::SELECT_A_BAR_ON_THE_TIMELINE);
                });
            }
            (Some(_), _, Some(error)) => {
                ui.colored_label(ui.visuals().warn_fg_color, format!("{}  {error}", icons::WARNING));
            }
            (Some(_), Some(details), None) => {
                egui::ScrollArea::vertical().auto_shrink(false).animated(false).show(ui, |ui| self.story(ui, &details));
            }
            (Some(_), None, None) => {
                ui.weak(catalog::reading(&icons::SPINNER));
            }
        }
    }

    fn story(&mut self, ui: &mut egui::Ui, d: &Details) {
        self.header(ui, d);
        if !d.facts.is_empty() {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for fact in &d.facts {
                    let colour = fact.colour.map(|c| drawn(c, ui.visuals()));
                    fact_pill(ui, &fact.label, &fact.value, colour);
                    self.drawn.facts.push(format!("{} {}", fact.label, fact.value));
                }
            });
        }
        if !d.related.is_empty() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.weak(catalog::CARRIES);
                for related in &d.related {
                    let response = chip(ui, &related.mark, related.colour, 11.0);
                    let label = ui.add(egui::Label::new(RichText::new(&related.label).small()).sense(Sense::click()));
                    if response.clicked() || label.clicked() {
                        self.drawn.chosen = Some(related.subject.clone());
                    }
                }
            });
        }
        ui.add_space(6.0);
        ui.separator();
        let mut day = None;
        for entry in &d.entries {
            let today = entry.t.to_string().get(..10).map(str::to_owned);
            if today != day {
                ui.add_space(2.0);
                ui.label(RichText::new(today.clone().unwrap_or_default()).small().weak());
                day = today;
            }
            self.entry(ui, entry);
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, d: &Details) {
        ui.horizontal(|ui| {
            chip(ui, &d.mark, d.colour, 15.0);
            ui.add(egui::Label::new(RichText::new(&d.title).font(fonts::font_of(ui.ctx(), &fonts::h2()))).wrap());
        });
        self.drawn.mark = Some(d.mark.clone());
        self.drawn.title = Some(d.title.clone());
        for line in &d.lines {
            ui.label(line_job(line, ui.visuals(), &FontId::proportional(fonts::BODY_SIZE), None));
        }
        if !d.labels.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for label in &d.labels {
                    label_pill(ui, label);
                    self.drawn.labels.push(label.clone());
                }
            });
        }
        if !d.pages.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for page in &d.pages {
                    let text = format!("{}  {}", icons::EXTERNAL, page.label);
                    let mut link = ui.link(RichText::new(text).small());
                    if let Some(url) = &page.url {
                        link = link.on_hover_text(url);
                    }
                    if link.clicked() {
                        self.drawn.open = Some(match page.kind {
                            PageKind::Issue => Link::Issue(page.number),
                            PageKind::Pull => Link::Pull(page.number),
                        });
                    }
                }
            });
        }
    }

    /// One line of the story: its time, a dot in its colour, what happened, the rest of its line,
    /// who and how long; its notes under it. Clicking it goes to what it names.
    fn entry(&mut self, ui: &mut egui::Ui, entry: &Entry) {
        let colour = drawn(entry.colour, ui.visuals());
        let row = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let clock = entry.t.to_string();
            ui.label(RichText::new(clock.get(11..19).unwrap_or(&clock)).monospace().small().weak());
            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.5, colour);
            if entry.end.is_some() {
                ui.painter().circle_stroke(dot.center(), 4.5, egui::Stroke::new(1.0, ui.visuals().strong_text_color().gamma_multiply(0.5)));
            }
            ui.label(RichText::new(&entry.what).strong().color(colour));
            if !entry.detail.spans.is_empty() {
                let body = egui::TextStyle::Body.resolve(ui.style());
                ui.label(line_job(&entry.detail, ui.visuals(), &body, None));
            }
            if let Some(who) = &entry.who {
                let name = ui.add(egui::Label::new(RichText::new(format!("· {}", who.name)).weak()).sense(Sense::click()));
                if name.on_hover_text(catalog::show_their_session(&who.instance)).clicked() {
                    self.drawn.chosen = Some(Subject::Session { instance: who.instance.clone(), at: Some(entry.t) });
                }
            }
            if let Some(took) = &entry.duration {
                let so_far = if entry.open { catalog::SO_FAR } else { "" };
                ui.label(RichText::new(format!("· {took}{so_far}")).strong());
            }
        });
        self.drawn.entries.push(entry.text());
        for note in &entry.notes {
            // In a frame, not a row: a row's labels don't wrap, and would widen the pane.
            let indent = egui::Margin { left: NOTE_INDENT, ..egui::Margin::ZERO };
            egui::Frame::new().inner_margin(indent).show(ui, |ui| {
                let job = line_job(note, ui.visuals(), &fonts::font_of(ui.ctx(), &egui::TextStyle::Small), Some(ui.visuals().weak_text_color()));
                ui.add(egui::Label::new(job).wrap());
            });
        }
        let id = ui.scope_id().with((Part::DetailsEntry, entry.t, &entry.what, self.drawn.entries.len()));
        let response = ui.interact(row.response.rect, id, Sense::click());
        let span = entry.end.map_or_else(|| entry.t.to_string(), |end| format!("{} → {}{}", entry.t, end, if entry.open { catalog::SO_FAR_2 } else { "" }));
        let hint = entry.goes_to.as_ref().map(|s| catalog::click_show(&(described(s)))).unwrap_or_default();
        let response = response.on_hover_text(format!("{span}{hint}"));
        if response.hovered() && entry.goes_to.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() && self.drawn.chosen.is_none() {
            self.drawn.chosen.clone_from(&entry.goes_to);
        }
    }
}

/// How a person names `subject`, in a hint.
fn described(subject: &Subject) -> String {
    match subject {
        Subject::Ticket { issue } => catalog::ticket(&issue),
        Subject::Gate { sub, .. } => catalog::gate_run(&sub),
        Subject::Session { instance, .. } => catalog::s_session(&instance),
        Subject::Merge { commit } => catalog::merge(&(commit.get(..8).unwrap_or(commit))),
    }
}

/// `line` as one text: each run in its colour in the look drawn in (#154), strong or not; runs
/// with none in `plain`, else the theme's text colour.
pub(crate) fn line_job(line: &TipLine, visuals: &egui::Visuals, font: &FontId, plain: Option<Color32>) -> LayoutJob {
    let mut job = LayoutJob::default();
    for span in &line.spans {
        let color = match (span.colour, span.strong) {
            (Some(colour), _) => drawn(colour, visuals),
            (None, true) => visuals.strong_text_color(),
            (None, false) => plain.unwrap_or_else(|| visuals.text_color()),
        };
        job.append(&span.text, 0.0, TextFormat { font_id: font.clone(), color, valign: egui::Align::Center, ..TextFormat::default() });
    }
    job
}

/// A chip in `fill` with `mark` on it in a legible ink: the subject's, as on the timeline and in
/// the factory. Clickable.
fn chip(ui: &mut egui::Ui, mark: &str, colour: Paint, size: f32) -> egui::Response {
    let fill = colour.of(shade(ui.visuals()));
    let (fill, ink) = (rgb(fill), rgb(ink(fill)));
    let font = FontId::monospace(size * 0.8);
    let galley = ui.painter().layout_no_wrap(mark.to_owned(), font, ink);
    let pad = vec2(size * 0.45, size * 0.25);
    let (rect, response) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::click());
    ui.painter().rect_filled(rect, size * 0.3, fill);
    ui.painter().galley(rect.min + pad, galley, ink);
    response
}

/// A figure: its label quiet, its value strong (in its colour when it has one).
pub(crate) fn fact_pill(ui: &mut egui::Ui, label: &str, value: &str, colour: Option<Color32>) {
    let visuals = ui.visuals().clone();
    let value_colour = colour.unwrap_or_else(|| visuals.strong_text_color());
    let mut job = LayoutJob::default();
    let small = TextFormat {
        font_id: fonts::font_of(ui.ctx(), &egui::TextStyle::Small),
        color: visuals.weak_text_color(),
        valign: egui::Align::Center,
        ..TextFormat::default()
    };
    job.append(label, 0.0, small);
    let strong = TextFormat { font_id: egui::TextStyle::Body.resolve(ui.style()), color: value_colour, valign: egui::Align::Center, ..TextFormat::default() };
    job.append(value, 5.0, strong);
    pill(ui, job, visuals.faint_bg_color, egui::Stroke::NONE);
}

/// A GitHub label: a tag in a pill.
fn label_pill(ui: &mut egui::Ui, label: &str) {
    let visuals = ui.visuals().clone();
    let mut job = LayoutJob::default();
    let format = TextFormat { font_id: fonts::font_of(ui.ctx(), &egui::TextStyle::Small), color: visuals.text_color(), ..TextFormat::default() };
    job.append(&format!("{}  {label}", icons::TAG), 0.0, format);
    pill(ui, job, Color32::TRANSPARENT, visuals.widgets.noninteractive.bg_stroke);
}

/// `job` on a rounded pill, laid out unwrapped and placed as one widget of its size, so a
/// wrapping row moves it whole onto the next line (a frame, not knowing its size before it is
/// placed, would overflow the row and widen the pane).
fn pill(ui: &mut egui::Ui, job: LayoutJob, fill: Color32, stroke: egui::Stroke) {
    let galley = ui.painter().layout_job(job);
    let pad = vec2(7.0, 2.5);
    let (rect, _) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::hover());
    ui.painter().rect(rect, 8.0, fill, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(rect.min + pad, galley, ui.visuals().text_color());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_keeps_what_was_before_for_back() {
        let (mut pane, mut view) = (DetailsPane::default(), DetailsView::default());
        let (seven, gate) = (Subject::Ticket { issue: 7 }, Subject::Gate { sub: "gate-pr8".into(), at: None });
        pane.select(&mut view, seven.clone());
        pane.select(&mut view, seven.clone());
        pane.select(&mut view, gate.clone());
        assert_eq!(view.subject.as_ref(), Some(&gate));
        pane.back(&mut view);
        assert_eq!(view.subject.as_ref(), Some(&seven), "the same subject twice is one step");
        pane.back(&mut view);
        assert_eq!(view.subject.as_ref(), Some(&seven), "nothing before it: it stays");
        for issue in 0..100 {
            pane.select(&mut view, Subject::Ticket { issue });
        }
        assert_eq!(pane.history.len(), HISTORY);
    }

    #[test]
    fn a_subjects_params_name_it_alone() {
        let at = Some(swarmception_model::Timestamp(42));
        let gate = params(&Subject::Gate { sub: "g".into(), at });
        assert_eq!((gate.gate.as_deref(), gate.at, gate.ticket), (Some("g"), Some(When::Seconds(42)), None));
        assert_eq!(params(&Subject::Ticket { issue: 7 }).ticket, Some(7));
        assert_eq!(params(&Subject::Merge { commit: "abc1234".into() }).merge.as_deref(), Some("abc1234"));
        assert_eq!(params(&Subject::Session { instance: "m-1".into(), at: None }).session.as_deref(), Some("m-1"));
    }
}
