//! The story of one thing the swarm did, in time order (#47): what the details pane shows for
//! what is selected. A [`Subject`] is a ticket, a gate run, an agent's session or a merge onto
//! main; [`details`] tells it:
//!
//! - **a ticket**: its stretches (claimed, working, waiting for merge, sent back with the cause,
//!   stopped), each with its times, its duration and who; its holder leaving it (abandoned); its
//!   worker's `ready` checks; the gate runs that named it; its releases; its merge (and the
//!   commit onto main) or its `stop done`;
//! - **a gate run**: its start and its note, what it did (each merge and send back, the commits
//!   onto main), and how it ended (passed, failed and why, no result, never finished);
//! - **a session**: an agent instance's session going at a time (else its last): its start, its
//!   pauses, its work on each issue, its claims, submits, merges, send backs, stops and releases,
//!   its helpers and gate runs, and its end;
//! - **a merge onto main**: the gate run that merged it, its ticket's merge and the commit.
//!
//! Each has a header (a chip in its colour with its mark, its title, the lines under it and
//! GitHub's labels), its figures ([`Fact`]), what it links to ([`Related`], to select in turn)
//! and the pages it opens on GitHub ([`Page`]; the engine fills their URLs). Each entry is a line:
//! its time, its colour (the ticket's own for working and its merge, the phases' fixed colours,
//! green for what passed or merged, red for what failed or was sent back), what happened, who,
//! and how long it took (`1 h 05 min`, `3 min 20 s`). Pure: the engine reads the issues and the
//! slug, the UI draws the lines.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{IssueKind, IssueState, Issues, Kind, MainMerge, Role, Timestamp};

use crate::activity::{Activity, Agent, IssueEvent, Mark, Session, Sub};
use crate::colours::{self, Rgb, phase_colour, ticket_colour};
use crate::factory::{GateRun, Outcome, gate_runs};
use crate::gates::Lane;
use crate::rows::{Bar, BarRef, MAIN_KEY, Node, Style, bars_where, duration, groups_holding};
use crate::stats::{cause_of, failure_label, step_of};
use crate::tickets::{Merged, Phase, Stretch, Ticket, tickets};
use crate::titles;
use crate::tooltip::{FAILED, PASSED, TipLine};

/// What a gate run going on is drawn in: waiting's blue, as the tickets it holds wait.
pub const RUNNING: Rgb = colours::WAITING;
/// What a session's own start and end are drawn in: the slate of what is no ticket's.
pub const SESSION: Rgb = colours::NO_TICKET;

/// What the details are of.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Subject {
    /// A ticket, by its issue number.
    Ticket { issue: u64 },
    /// A gate run, by its sub-agent's id (`gate-pr50`); `at`, a time in it, picks one of a
    /// rerun's runs (the first when left out).
    Gate {
        sub: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Timestamp>,
    },
    /// An agent instance's session: the one going at `at`, else its last before it, else its last.
    Session {
        instance: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Timestamp>,
    },
    /// A merge onto main, by its commit (or the commit's start, seven characters or more).
    Merge { commit: String },
}

/// A page on GitHub: an issue's or a pull request's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PageKind {
    Issue,
    Pull,
}

/// A page the details open: `Issue #47`, `PR #50`, its URL when the engine knows the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub kind: PageKind,
    pub number: u64,
    pub label: String,
    pub url: Option<String>,
}

impl Page {
    fn issue(number: u64) -> Self {
        Self { kind: PageKind::Issue, number, label: format!("Issue #{number}"), url: None }
    }

    fn pull(number: u64) -> Self {
        Self { kind: PageKind::Pull, number, label: format!("PR #{number}"), url: None }
    }
}

/// Who did something: the worker's name and its instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Who {
    pub name: String,
    pub instance: String,
}

/// A figure in the header: `claim → merge 2 h 05 min`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub label: String,
    pub value: String,
    /// Its value's colour; the theme's text colour with none.
    pub colour: Option<Rgb>,
}

impl Fact {
    fn new(label: &str, value: impl Into<String>) -> Self {
        Self { label: label.to_owned(), value: value.into(), colour: None }
    }

    fn coloured(label: &str, value: impl Into<String>, colour: Rgb) -> Self {
        Self { colour: Some(colour), ..Self::new(label, value) }
    }
}

/// Something the details name, to select in turn: a gate run's tickets, a merge's gate run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Related {
    pub subject: Subject,
    /// Its chip's mark (`#7`), and what it says beside it (its title).
    pub mark: String,
    pub label: String,
    pub colour: Rgb,
}

/// One line of the story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub t: Timestamp,
    /// When it ended, for a stretch of time; none for an instant.
    pub end: Option<Timestamp>,
    /// How long it took, in seconds and as a person reads it (`3 min 20 s`).
    pub seconds: Option<i64>,
    pub duration: Option<String>,
    /// Nothing ended it yet: it runs on to the last event.
    pub open: bool,
    pub colour: Rgb,
    /// What happened: `working`, `sent back`, `gate run passed`.
    pub what: String,
    /// The rest of its line, styled: `PR #8 · in gate-pr8`.
    pub detail: TipLine,
    pub who: Option<Who>,
    /// Notes under it: the event's note, a gate's cause (red), a stop note.
    pub notes: Vec<TipLine>,
    /// What selecting it goes to: a gate run, a ticket, a merge.
    pub goes_to: Option<Subject>,
}

impl Entry {
    fn at(t: Timestamp, colour: Rgb, what: impl Into<String>) -> Self {
        Self {
            t,
            end: None,
            seconds: None,
            duration: None,
            open: false,
            colour,
            what: what.into(),
            detail: TipLine::default(),
            who: None,
            notes: Vec::new(),
            goes_to: None,
        }
    }

    /// From `t` to `end`, open or not: its duration set.
    fn span(t: Timestamp, end: Timestamp, open: bool, colour: Rgb, what: impl Into<String>) -> Self {
        let seconds = end.0.saturating_sub(t.0).max(0);
        Self { end: Some(end), seconds: Some(seconds), duration: Some(duration(seconds)), open, ..Self::at(t, colour, what) }
    }

    fn detail(mut self, detail: TipLine) -> Self {
        self.detail = detail;
        self
    }

    fn by(mut self, who: Option<Who>) -> Self {
        self.who = who;
        self
    }

    fn note(mut self, note: Option<TipLine>) -> Self {
        self.notes.extend(note);
        self
    }

    fn goes_to(mut self, subject: Option<Subject>) -> Self {
        self.goes_to = subject;
        self
    }

    /// The line's text, after its time: what, the detail, who and how long.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = self.what.clone();
        let detail = self.detail.text();
        if !detail.is_empty() {
            text.push_str(&format!(" {detail}"));
        }
        if let Some(who) = &self.who {
            text.push_str(&format!(" · {}", who.name));
        }
        if let Some(took) = &self.duration {
            text.push_str(&format!(" · {took}{}", if self.open { " so far" } else { "" }));
        }
        text
    }
}

/// The story of a subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub subject: Subject,
    /// The header's chip: its colour and its mark (`#47`, `gate`, `w19`).
    pub colour: Rgb,
    pub mark: String,
    /// `#47 A details pane for what is selected`, `gate run gate-pr50`, `worker-19 · mini`.
    pub title: String,
    /// The lines under the title: state and author, its PR, its span.
    pub lines: Vec<TipLine>,
    /// GitHub's labels on it.
    pub labels: Vec<String>,
    pub facts: Vec<Fact>,
    pub related: Vec<Related>,
    /// What happened, oldest first.
    pub entries: Vec<Entry>,
    pub pages: Vec<Page>,
    /// Where it is drawn on the timeline (#55): [`details_on`] fills it; [`details`], with no
    /// rows to read, leaves it empty.
    #[serde(default)]
    pub timeline: OnTimeline,
}

/// Where a subject is drawn on the timeline (#55): its bars, which the timeline highlights while
/// the details pane shows it, and the times they span, to bring them into view.
///
/// - a ticket: its bars (its stretches and its merge);
/// - a gate run: the merges onto main it made, and the bars of the tickets it named;
/// - a session: the bars of what it began in its time (the stretches its events began), the
///   tickets it merged and their merges onto main;
/// - a merge onto main: its bar on main's row, and its ticket's bars.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnTimeline {
    /// Its bars, sorted by row key and place, each once.
    pub bars: Vec<BarRef>,
    /// The first bar's start and the last one's end; none with no bars.
    pub start: Option<Timestamp>,
    pub end: Option<Timestamp>,
    /// The groups its bars' rows are in, outermost first (#68): by worker, a collapsed one opens
    /// to show them.
    #[serde(default)]
    pub groups: Vec<String>,
}

/// What the details read: the activity, the merges onto main, GitHub's issues, and what is
/// computed from them once.
struct Ctx<'a> {
    activity: &'a Activity,
    merges: &'a [MainMerge],
    issues: &'a Issues,
    tickets: Vec<Ticket>,
    runs: Vec<GateRun>,
    /// The workers' `ready` checks (#95): a `Gate` subject names one by its id as a gate run.
    checks: Vec<GateRun>,
}

impl<'a> Ctx<'a> {
    fn new(activity: &'a Activity, merges: &'a [MainMerge], issues: &'a Issues) -> Self {
        let end = Timestamp(i64::MAX);
        Self { activity, merges, issues, tickets: tickets(activity), runs: gate_runs(activity, end), checks: crate::factory::ready_checks(activity, end) }
    }

    fn who(&self, instance: &str) -> Who {
        let name = self.activity.agent(instance).map_or_else(|| instance.to_owned(), |a| a.name.clone());
        Who { name, instance: instance.to_owned() }
    }

    fn ticket(&self, issue: u64) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.issue == issue)
    }

    /// A ticket's title, as GitHub has it (its chip carries its number), else `ticket #7`.
    fn ticket_title(&self, issue: u64) -> String {
        titles::title(self.issues, issue).map_or_else(|| format!("ticket #{issue}"), |t| titles::short(t, titles::TOOLTIP_TITLE))
    }

    /// Who ran the gate run that began at `start` as `sub`: its coordinator.
    fn gate_owner(&self, sub: &str, start: Timestamp) -> Option<Who> {
        let agent = self.activity.agents.iter().find(|a| a.subs.iter().any(|s| s.sub == sub && s.span.start == start))?;
        Some(self.who(&agent.instance))
    }

    fn related_ticket(&self, issue: u64) -> Related {
        let label = titles::title(self.issues, issue).map(|t| titles::short(t, titles::LABEL_TITLE)).unwrap_or_default();
        Related { subject: Subject::Ticket { issue }, mark: format!("#{issue}"), label, colour: ticket_colour(issue) }
    }

    /// The gate run `sub` that holds `at` (began by it, ended at or after it), else the first;
    /// a worker's `ready` check by its id too (#95).
    fn run(&self, sub: &str, at: Option<Timestamp>) -> Option<&GateRun> {
        let mut runs = self.runs.iter().chain(&self.checks).filter(|r| r.sub == sub);
        let first = runs.clone().next();
        let Some(at) = at else { return first };
        runs.rfind(|r| r.start <= at).or(first)
    }
}

/// The details of `subject`, from the activity, the merges onto main and GitHub's issues; none
/// when the swarm has no such ticket, gate run, session or merge.
#[must_use]
pub fn details(activity: &Activity, merges: &[MainMerge], issues: &Issues, subject: &Subject) -> Option<Details> {
    told(&Ctx::new(activity, merges, issues), subject)
}

/// The details of `subject`, as [`details`], with where it is drawn on the timeline's `rows`
/// ([`OnTimeline`], #55).
#[must_use]
pub fn details_on(activity: &Activity, merges: &[MainMerge], issues: &Issues, rows: &[Node], subject: &Subject) -> Option<Details> {
    let ctx = Ctx::new(activity, merges, issues);
    let mut found = told(&ctx, subject)?;
    found.timeline = on_timeline(&ctx, rows, &found.subject);
    Some(found)
}

fn told(ctx: &Ctx<'_>, subject: &Subject) -> Option<Details> {
    match subject {
        Subject::Ticket { issue } => ticket_details(ctx, *issue),
        Subject::Gate { sub, at } => gate_details(ctx, sub, *at),
        Subject::Session { instance, at } => session_details(ctx, instance, *at),
        Subject::Merge { commit } => merge_details(ctx, commit),
    }
}

/// Whether `bar`, on row `key`, draws one of `commits` onto main.
fn on_main(key: &str, bar: &Bar, commits: &[&MainMerge]) -> bool {
    key == MAIN_KEY && bar.style == Style::MainMerge && commits.iter().any(|m| m.t == bar.start && m.pr == bar.pr)
}

/// Where `subject` (as its details name it: a gate run's or a session's `at` its start) is drawn
/// on `rows`.
fn on_timeline(ctx: &Ctx<'_>, rows: &[Node], subject: &Subject) -> OnTimeline {
    let found = match subject {
        Subject::Ticket { issue } => bars_where(rows, |_, bar| bar.ticket == Some(*issue)),
        Subject::Gate { sub, at } => {
            let Some(run) = ctx.run(sub, *at) else { return OnTimeline::default() };
            let named: Vec<u64> = ctx.tickets.iter().filter(|t| names_ticket(run, t)).map(|t| t.issue).collect();
            let commits: Vec<&MainMerge> = merged_prs(&run_events(ctx, run)).into_iter().filter_map(|pr| main_commit(ctx, Some(pr), run.start)).collect();
            // Its own bar on the Gate runs row (#84) too.
            let drawn = |bar: &Bar| bar.run.as_ref().is_some_and(|r| r.sub == run.sub && bar.start == run.start);
            bars_where(rows, |key, bar| bar.ticket.is_some_and(|issue| named.contains(&issue)) || on_main(key, bar, &commits) || drawn(bar))
        }
        Subject::Session { instance, at } => session_bars(ctx, rows, instance, *at),
        Subject::Merge { commit } => {
            let Some(merge) = ctx.merges.iter().find(|m| m.commit == *commit) else { return OnTimeline::default() };
            let ticket = merge.pr.and_then(|pr| ctx.tickets.iter().find(|t| t.merged.as_ref().is_some_and(|m| m.pr == Some(pr)))).map(|t| t.issue);
            bars_where(rows, |key, bar| on_main(key, bar, &[merge]) || (ticket.is_some() && bar.ticket == ticket))
        }
    };
    let start = found.iter().map(|(_, bar)| bar.start).min();
    let end = found.iter().map(|(_, bar)| bar.end).max();
    let mut bars: Vec<BarRef> = found.into_iter().map(|(at, _)| at).collect();
    bars.sort();
    bars.dedup();
    let keys: Vec<&str> = bars.iter().map(|bar| bar.track.as_str()).collect();
    let groups = groups_holding(rows, &keys);
    OnTimeline { bars, start, end, groups }
}

/// A session's bars: the tickets' bars holding the stretches its events began in its time, the tickets it merged then and
/// their merges onto main.
fn session_bars<'r>(ctx: &Ctx<'_>, rows: &'r [Node], instance: &str, at: Option<Timestamp>) -> Vec<(BarRef, &'r Bar)> {
    let Some(session) = ctx.activity.agent(instance).and_then(|agent| session_at(agent, at)) else { return Vec::new() };
    let inside = |t: Timestamp| session.span.start <= t && t <= session.span.end;
    let mut began = Vec::new();
    let mut merged = Vec::new();
    for ticket in &ctx.tickets {
        let mine = ticket.stretches.iter().filter(|s| s.by == instance && inside(s.start));
        began.extend(mine.map(|s| (ticket.issue, s.start)));
        merged.extend(ticket.merged.as_ref().filter(|m| m.by == instance && inside(m.t)).map(|m| (ticket.issue, m)));
    }
    let commits: Vec<&MainMerge> = merged.iter().filter_map(|(_, m)| main_commit(ctx, m.pr, m.t)).collect();
    bars_where(rows, |key, bar| match bar.ticket {
        Some(issue) if bar.style == Style::Merged => merged.iter().any(|(merged, _)| *merged == issue),
        // A ticket's bar holding a stretch the session began (#110: one bar a ticket a row).
        Some(issue) => began.iter().any(|&(began, t)| began == issue && bar.start <= t && t <= bar.end),
        None => on_main(key, bar, &commits),
    })
}

/// What the bar `range` of row `track` is the details of: its ticket, the merge onto main it
/// draws, or a gate run's subject (#84: the run; a `ready` check's ticket, else its worker's
/// session, as a click on the Gates band's lane told); none for a bar that isn't there.
#[must_use]
pub fn bar_subject(rows: &[Node], merges: &[MainMerge], track: &str, range: usize) -> Option<Subject> {
    let bar = rows.iter().flat_map(Node::leaves).find_map(|leaf| match leaf {
        Node::Ranges { key, bars, .. } if key == track => bars.get(range),
        _ => None,
    })?;
    if let Some(run) = &bar.run {
        return Some(run.subject.clone());
    }
    match (bar.ticket, bar.style) {
        (Some(issue), _) => Some(Subject::Ticket { issue }),
        (None, Style::MainMerge) => merges.iter().find(|m| m.t == bar.start && m.pr == bar.pr).map(|m| Subject::Merge { commit: m.commit.clone() }),
        (None, _) => None,
    }
}

/// Entries in time order, a tie kept in the order they were added.
fn in_order(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by_key(|e| e.t);
    entries
}

// --- A ticket ---

fn ticket_details(ctx: &Ctx<'_>, issue: u64) -> Option<Details> {
    let ticket = ctx.ticket(issue)?;
    let runs: Vec<&GateRun> = ctx.runs.iter().filter(|r| names_ticket(r, ticket)).collect();
    let mut entries = Vec::new();
    for stretch in &ticket.stretches {
        entries.push(stretch_entry(ctx, ticket, stretch));
        if let Some(abandoned) = &stretch.abandoned {
            let ended = abandoned.ended_by.as_ref().map(|by| format!(" (ended by {})", ctx.who(by).name)).unwrap_or_default();
            let detail = TipLine::plain(format!("{}'s session ended{ended}", ctx.who(&abandoned.holder).name));
            entries.push(Entry::at(abandoned.t, FAILED, "abandoned").detail(detail).by(Some(ctx.who(&abandoned.holder))));
        }
    }
    entries.extend(ready_checks(ctx, ticket));
    entries.extend(runs.iter().map(|run| run_entry(ctx, run)));
    entries.extend(ctx.activity.issue_events.iter().filter(|e| e.issue == issue).filter_map(|e| match &e.kind {
        Kind::Release { reason, .. } => {
            let note = reason.clone().or_else(|| e.note.clone()).map(TipLine::plain);
            Some(Entry::at(e.t, colours::STOPPED, "released").by(Some(ctx.who(&e.instance))).note(note))
        }
        _ => None,
    }));
    if let Some(merged) = &ticket.merged {
        entries.push(ticket_merge_entry(ctx, merged));
    }
    if let Some(done) = &ticket.done {
        let note = done.note.clone().map(TipLine::plain);
        entries.push(Entry::at(done.t, colours::STOPPED, "done without a merge").by(Some(ctx.who(&done.by))).note(note));
    }
    let (lines, labels) = ticket_header(ctx, ticket);
    let pr = ticket.pr.or_else(|| ctx.issues.pr_of(issue, None).map(|p| p.number)).filter(|pr| *pr != issue);
    let pages = std::iter::once(Page::issue(issue)).chain(pr.map(Page::pull)).collect();
    Some(Details {
        subject: Subject::Ticket { issue },
        colour: ticket_colour(issue),
        mark: format!("#{issue}"),
        title: ctx.ticket_title(issue),
        lines,
        labels,
        facts: ticket_facts(ctx, ticket, &runs),
        related: Vec::new(),
        entries: in_order(entries),
        pages,
        timeline: OnTimeline::default(),
    })
}

/// Whether `run` named `ticket`: its issue, or its pull request, among the run's.
fn names_ticket(run: &GateRun, ticket: &Ticket) -> bool {
    run.issues.contains(&ticket.issue) || run.refs.contains(&ticket.issue) || ticket.pr.is_some_and(|pr| run.refs.contains(&pr))
}

fn stretch_entry(ctx: &Ctx<'_>, ticket: &Ticket, stretch: &Stretch) -> Entry {
    let colour = phase_colour(stretch.phase, ticket.issue);
    let entry = Entry::span(stretch.start, stretch.end, stretch.open, colour, stretch.phase.name()).by(Some(ctx.who(&stretch.by)));
    let detail = match stretch.phase {
        Phase::Waiting => ticket.pr.map(|pr| TipLine::plain(format!("on PR #{pr}"))),
        _ => None,
    };
    let note = stretch.note.as_ref().map(|note| if stretch.phase == Phase::SentBack { TipLine::coloured(note, FAILED) } else { TipLine::plain(note) });
    entry.detail(detail.unwrap_or_default()).note(note)
}

/// The `ready` checks its worker ran while it held the ticket: a sub-agent of the holder's that
/// began within one of its claimed or working stretches.
fn ready_checks(ctx: &Ctx<'_>, ticket: &Ticket) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut seen: Vec<(&str, Timestamp)> = Vec::new();
    for stretch in ticket.stretches.iter().filter(|s| matches!(s.phase, Phase::Claimed | Phase::Working | Phase::SentBack)) {
        let Some(agent) = ctx.activity.agent(&stretch.by) else { continue };
        for sub in agent.subs.iter().filter(|s| !s.is_gate() && stretch.start <= s.span.start && s.span.start <= stretch.end) {
            if !seen.contains(&(sub.sub.as_str(), sub.span.start)) {
                seen.push((sub.sub.as_str(), sub.span.start));
                entries.push(helper_entry(ctx, agent, sub));
            }
        }
    }
    entries
}

/// A helper sub-agent's run: `ready check passed`, its duration, why it failed when its stop says
/// (`cause build/clippy`, its `cause=` and `step=`, #77: an older one's is unknown), its notes.
fn helper_entry(ctx: &Ctx<'_>, agent: &Agent, sub: &Sub) -> Entry {
    let name = sub.name.clone().unwrap_or_else(|| sub.sub.clone());
    let (colour, verb) = match sub.rc {
        Some(0) => (PASSED, "passed"),
        Some(_) => (FAILED, "failed"),
        None if sub.span.open => (RUNNING, "running"),
        None => (colours::STOPPED, "ended"),
    };
    let what = if name == "ready" { format!("ready check {verb}") } else { format!("{name} {verb}") };
    let entry = Entry::span(sub.span.start, sub.span.end, sub.span.open, colour, what).by(Some(ctx.who(&agent.instance)));
    let detail = (sub.sub != name).then(|| TipLine::plain(sub.sub.clone()));
    let failed = sub.stop_note.as_deref().filter(|_| matches!(sub.rc, Some(rc) if rc != 0));
    let why = failed.map(|note| (cause_of(note), step_of(note))).filter(|(cause, step)| cause.is_some() || step.is_some());
    let why = why.map(|(cause, step)| TipLine::coloured(format!("cause {}", failure_label(cause.as_deref(), step.as_deref())), FAILED));
    let notes = sub.start_note.iter().chain(&sub.stop_note).map(TipLine::plain);
    let mut entry = entry.detail(detail.unwrap_or_default()).note(why);
    entry.notes.extend(notes);
    // A ready check tells its own story (#95).
    if crate::stats::is_ready(sub) {
        entry = entry.goes_to(Some(Subject::Gate { sub: sub.sub.clone(), at: Some(sub.span.start) }));
    }
    entry
}

/// How a gate run went, in its colour.
const fn outcome_look(outcome: Outcome) -> (Rgb, &'static str) {
    match outcome {
        Outcome::Running => (RUNNING, "running"),
        Outcome::Passed => (PASSED, "passed"),
        Outcome::Failed => (FAILED, "failed"),
        Outcome::Unknown => (colours::STOPPED, "no result"),
    }
}

/// A gate run as a line of a ticket's or a session's story: `gate run passed gate-pr8 · smoke`.
fn run_entry(ctx: &Ctx<'_>, run: &GateRun) -> Entry {
    let (colour, verb) = outcome_look(run.outcome);
    let end = run.end.unwrap_or_else(|| ctx.activity.extent.map_or(run.start, |(_, last)| last).max(run.start));
    let mut detail = TipLine::plain(run.sub.clone());
    if let Some(kind) = &run.kind {
        detail = detail.then(format!(" · {kind}"));
    }
    let cause = run.cause.as_ref().map(|c| if run.outcome == Outcome::Failed { TipLine::coloured(c, FAILED) } else { TipLine::plain(c) });
    Entry::span(run.start, end, run.end.is_none(), colour, format!("gate run {verb}"))
        .detail(detail)
        .by(ctx.gate_owner(&run.sub, run.start))
        .note(cause)
        .goes_to(Some(Subject::Gate { sub: run.sub.clone(), at: Some(run.start) }))
}

/// The main merge commit whose pull request is `pr`, the first at or after `at`.
fn main_commit<'a>(ctx: &Ctx<'a>, pr: Option<u64>, at: Timestamp) -> Option<&'a MainMerge> {
    let pr = pr?;
    let mut found = ctx.merges.iter().filter(|m| m.pr == Some(pr));
    found.clone().find(|m| m.t >= at).or_else(|| found.next())
}

fn ticket_merge_entry(ctx: &Ctx<'_>, merged: &Merged) -> Entry {
    let mut detail = TipLine::default();
    if let Some(pr) = merged.pr {
        detail = detail.then(format!("PR #{pr}"));
    }
    if let Some(sub) = &merged.sub {
        detail = detail.then(format!(" in {sub}"));
    }
    let commit = main_commit(ctx, merged.pr, merged.t);
    let note = commit.map(|m| TipLine::coloured("onto main ", PASSED).then(format!("{} {}", short_commit(&m.commit), m.subject)));
    let goes_to = merged.sub.as_ref().map(|sub| Subject::Gate { sub: sub.clone(), at: Some(merged.t) });
    Entry::at(merged.t, PASSED, "merged").detail(detail).by(Some(ctx.who(&merged.by))).note(note).goes_to(goes_to)
}

fn short_commit(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

/// The lines under a ticket's title and its labels: GitHub's state and author, its pull request.
fn ticket_header(ctx: &Ctx<'_>, ticket: &Ticket) -> (Vec<TipLine>, Vec<String>) {
    let Some(issue) = ctx.issues.get(ticket.issue) else {
        let line = ticket.pr.map_or_else(|| TipLine::plain("not in the issues fetched from GitHub"), |pr| TipLine::plain(format!("PR #{pr}")));
        return (vec![line], Vec::new());
    };
    let mut state = match issue.state {
        IssueState::Closed | IssueState::Merged => TipLine::coloured(issue.state.name(), PASSED),
        IssueState::Open | IssueState::Unknown => TipLine::strong(issue.state.name()),
    };
    if let Some(author) = &issue.author {
        state = state.then(format!(" · opened by {author}"));
    }
    let mut lines = vec![state];
    if let Some(pr) = ctx.issues.pr_of(ticket.issue, ticket.pr).filter(|pr| pr.number != ticket.issue) {
        let mut line = TipLine::plain(format!("PR #{}", pr.number));
        if !pr.title.is_empty() {
            line = line.then(format!(" {}", titles::short(&pr.title, titles::TOOLTIP_TITLE)));
        }
        line = match pr.state {
            IssueState::Merged => line.then(" (").then_coloured("merged", PASSED).then(")"),
            IssueState::Unknown => line,
            other => line.then(format!(" ({})", other.name())),
        };
        lines.push(line);
    }
    (lines, issue.labels.clone())
}

fn seconds_in(ticket: &Ticket, phase: Phase) -> i64 {
    ticket.stretches.iter().filter(|s| s.phase == phase).map(|s| s.end.0.saturating_sub(s.start.0).max(0)).sum()
}

fn ticket_facts(ctx: &Ctx<'_>, ticket: &Ticket, runs: &[&GateRun]) -> Vec<Fact> {
    let mut facts = Vec::new();
    let from = ticket.claimed.or(ticket.began).or_else(|| ticket.stretches.first().map(|s| s.start));
    match (from, &ticket.merged, &ticket.done) {
        (Some(from), Some(merged), _) => facts.push(Fact::coloured("claim → merge", duration(merged.t.0.saturating_sub(from.0)), PASSED)),
        (Some(from), None, Some(done)) => facts.push(Fact::new("claim → done", duration(done.t.0.saturating_sub(from.0)))),
        (Some(from), None, None) => {
            let last = ticket.stretches.last().map_or(from, |s| s.end);
            let what = if ticket.abandoned.is_some() { "abandoned after" } else { "so far" };
            facts.push(Fact::new(what, duration(last.0.saturating_sub(from.0))));
        }
        (None, ..) => {}
    }
    let working = seconds_in(ticket, Phase::Working);
    if working > 0 {
        facts.push(Fact::coloured("working", duration(working), ticket_colour(ticket.issue)));
    }
    let waiting = seconds_in(ticket, Phase::Waiting);
    if waiting > 0 {
        facts.push(Fact::coloured("waiting", duration(waiting), colours::WAITING));
    }
    if !runs.is_empty() {
        let passed = runs.iter().filter(|r| r.outcome == Outcome::Passed).count();
        let failed = runs.iter().filter(|r| r.outcome == Outcome::Failed).count();
        let colour = if failed > 0 { FAILED } else { PASSED };
        facts.push(Fact::coloured("gate runs", format!("{} ({passed} passed, {failed} failed)", runs.len()), colour));
    }
    let sent_back = ticket.stretches.iter().filter(|s| s.phase == Phase::SentBack).count();
    if sent_back > 0 {
        facts.push(Fact::coloured("sent back", format!("{sent_back}×"), FAILED));
    }
    if let Some(worker) = &ticket.worker {
        facts.push(Fact::new("worker", ctx.who(worker).name));
    }
    facts
}

// --- A gate run ---

fn gate_details(ctx: &Ctx<'_>, sub: &str, at: Option<Timestamp>) -> Option<Details> {
    let run = ctx.run(sub, at)?;
    let logged = ctx.activity.agents.iter().find_map(|a| a.subs.iter().find(|s| s.sub == run.sub && s.span.start == run.start).map(|s| (a, s)));
    let (colour, verb) = outcome_look(run.outcome);
    let owner = ctx.gate_owner(&run.sub, run.start);
    let mut entries = vec![{
        let note = logged.and_then(|(_, s)| s.start_note.clone()).map(TipLine::plain);
        let what = if run.inferred { "first event" } else { "started" };
        Entry::at(run.start, RUNNING, what)
            .detail(TipLine::plain(run.kind.clone().map(|k| format!("{k} gate")).unwrap_or_default()))
            .by(owner.clone())
            .note(note)
    }];
    let events = run_events(ctx, run);
    let merged_prs = merged_prs(&events);
    entries.extend(events.iter().map(|event| gate_event_entry(ctx, event)));
    for pr in &merged_prs {
        if let Some(commit) = main_commit(ctx, Some(*pr), run.start) {
            let detail = TipLine::plain(format!("{} {}", short_commit(&commit.commit), commit.subject));
            entries.push(Entry::at(commit.t, PASSED, "onto main").detail(detail).goes_to(Some(Subject::Merge { commit: commit.commit.clone() })));
        }
    }
    entries.extend(gate_end_entry(run, owner.clone(), logged.map(|(_, s)| s)));
    Some(Details {
        subject: Subject::Gate { sub: run.sub.clone(), at: Some(run.start) },
        colour,
        mark: if run.lane == Lane::Ready { "ready" } else { "gate" }.to_owned(),
        title: format!("{} {}", if run.lane == Lane::Ready { "ready check" } else { "gate run" }, run.sub),
        lines: gate_lines(ctx, run, owner.as_ref(), verb, colour),
        labels: Vec::new(),
        facts: gate_facts(run, &merged_prs, &events),
        related: gate_related(ctx, run),
        entries: in_order(entries),
        pages: gate_pages(ctx, run),
        timeline: OnTimeline::default(),
    })
}

/// The issue events a gate run logged, in its time.
fn run_events<'a>(ctx: &Ctx<'a>, run: &GateRun) -> Vec<&'a IssueEvent> {
    let until = run.end.unwrap_or(Timestamp(i64::MAX));
    ctx.activity.issue_events.iter().filter(|e| e.sub.as_deref() == Some(run.sub.as_str()) && run.start <= e.t && e.t <= until).collect()
}

/// The pull requests `events` merged, each once, in order.
fn merged_prs(events: &[&IssueEvent]) -> Vec<u64> {
    let mut merged = Vec::new();
    for event in events {
        if let Kind::Merge { mr: Some(m), .. } = event.kind
            && !merged.contains(&m)
        {
            merged.push(m);
        }
    }
    merged
}

/// What a gate run did to a ticket: merged it, sent it back (why, in red), or another event.
fn gate_event_entry(ctx: &Ctx<'_>, event: &IssueEvent) -> Entry {
    let n = event.issue;
    let pr = |mr: &Option<u64>| TipLine::plain(mr.map(|m| format!("PR #{m}")).unwrap_or_default());
    let entry = match &event.kind {
        Kind::Merge { mr, .. } => Entry::at(event.t, ticket_colour(n), format!("merged #{n}")).detail(pr(mr)),
        Kind::Sendback { mr, .. } => {
            Entry::at(event.t, FAILED, format!("sent back #{n}")).detail(pr(mr)).note(event.note.clone().map(|note| TipLine::coloured(note, FAILED)))
        }
        other => Entry::at(event.t, colours::STOPPED, format!("{} #{n}", other.name())).note(event.note.clone().map(TipLine::plain)),
    };
    entry.by(Some(ctx.who(&event.instance))).goes_to(Some(Subject::Ticket { issue: n }))
}

/// How a gate run ended, when it did: passed, failed (its cause in red) or no result, how long it
/// took, its stop note.
fn gate_end_entry(run: &GateRun, owner: Option<Who>, logged: Option<&Sub>) -> Option<Entry> {
    let end = run.end?;
    let (colour, verb) = outcome_look(run.outcome);
    let mut finished = Entry { t: end, ..Entry::span(run.start, end, false, colour, verb) }.by(owner);
    if let Some(cause) = &run.cause {
        finished.detail = if run.outcome == Outcome::Failed { TipLine::coloured(cause, FAILED) } else { TipLine::plain(cause) };
    }
    if let Some(sub) = logged {
        finished.notes.extend(sub.stop_note.iter().chain(&sub.unfinished).map(TipLine::plain));
    }
    Some(finished)
}

fn gate_lines(ctx: &Ctx<'_>, run: &GateRun, owner: Option<&Who>, verb: &str, colour: Rgb) -> Vec<TipLine> {
    let mut line = TipLine::coloured(verb, colour);
    if let Some(end) = run.end {
        line = line.then(" in ").then_strong(duration(end.0.saturating_sub(run.start.0)));
    } else {
        let last = ctx.activity.extent.map_or(run.start, |(_, last)| last);
        line = line.then(" for ").then_strong(duration(last.0.saturating_sub(run.start.0))).then(" so far");
    }
    if let Some(owner) = owner {
        line = line.then(format!(" · by {}", owner.name));
    }
    let mut lines = vec![line, TipLine::plain(format!("{} → {}", run.start, run.end.map_or_else(|| "…".to_owned(), |e| e.to_string())))];
    if run.inferred {
        lines.push(TipLine::plain("read from its merges and send backs: no sub.start was logged"));
    }
    lines
}

fn gate_facts(run: &GateRun, merged: &[u64], events: &[&IssueEvent]) -> Vec<Fact> {
    let (colour, verb) = outcome_look(run.outcome);
    let mut facts = vec![Fact::coloured("outcome", verb, colour)];
    if let Some(end) = run.end {
        facts.push(Fact::new("took", duration(end.0.saturating_sub(run.start.0))));
    }
    if let Some(kind) = &run.kind {
        facts.push(Fact::new("kind", kind.clone()));
    }
    if !run.refs.is_empty() {
        facts.push(Fact::new("carried", run.refs.iter().map(|r| format!("#{r}")).collect::<Vec<_>>().join(" ")));
    }
    if !merged.is_empty() {
        facts.push(Fact::coloured("merged", merged.iter().map(|r| format!("PR #{r}")).collect::<Vec<_>>().join(" "), PASSED));
    }
    let sent = events.iter().filter(|e| matches!(e.kind, Kind::Sendback { .. })).count();
    if sent > 0 {
        facts.push(Fact::coloured("sent back", format!("{sent}"), FAILED));
    }
    facts
}

/// The tickets a gate run carried: those its events name, and those whose issue or pull request
/// its note names.
fn gate_related(ctx: &Ctx<'_>, run: &GateRun) -> Vec<Related> {
    let mut issues: Vec<u64> = run.issues.clone();
    for r in &run.refs {
        let ticket = ctx.tickets.iter().find(|t| t.issue == *r || t.pr == Some(*r)).map(|t| t.issue);
        if let Some(n) = ticket.filter(|n| !issues.contains(n)) {
            issues.push(n);
        }
    }
    issues.into_iter().map(|n| ctx.related_ticket(n)).collect()
}

/// The pull requests a gate run carried, then its tickets' issues.
fn gate_pages(ctx: &Ctx<'_>, run: &GateRun) -> Vec<Page> {
    let is_pr = |n: u64| ctx.issues.get(n).is_some_and(|i| i.kind == IssueKind::PullRequest) || ctx.tickets.iter().any(|t| t.pr == Some(n));
    let mut pages: Vec<Page> = run.refs.iter().filter(|r| is_pr(**r)).map(|r| Page::pull(*r)).collect();
    pages.extend(gate_related(ctx, run).iter().filter_map(|r| match r.subject {
        Subject::Ticket { issue } => Some(Page::issue(issue)),
        _ => None,
    }));
    pages
}

// --- A session ---

/// `worker-19` as a chip's mark: its first letter and its number (`w19`), else its first two
/// letters.
fn mark_of(name: &str) -> String {
    let digits: String = name.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
    let first = name.chars().next().map(String::from).unwrap_or_default();
    if digits.is_empty() { name.chars().take(2).collect() } else { format!("{first}{digits}") }
}

fn role_name(role: &Role) -> &str {
    match role {
        Role::Worker => "worker",
        Role::Coordinator => "coordinator",
        Role::Other(other) => other,
    }
}

/// The session of `agent` going at `at`, else its last begun by then, else its last.
fn session_at(agent: &Agent, at: Option<Timestamp>) -> Option<&Session> {
    let last = agent.sessions.last();
    let Some(at) = at else { return last };
    agent.sessions.iter().rfind(|s| s.span.start <= at).or(last)
}

fn session_details(ctx: &Ctx<'_>, instance: &str, at: Option<Timestamp>) -> Option<Details> {
    let agent = ctx.activity.agent(instance)?;
    let session = session_at(agent, at)?;
    let (start, end) = (session.span.start, session.span.end);
    let inside = |t: Timestamp| start <= t && t <= end;
    let me = Some(ctx.who(instance));
    let mut entries = vec![Entry::at(start, SESSION, "session started").note(session.notes.first().map(TipLine::plain))];
    entries.extend(agent.marks.iter().filter(|m| inside(m.t)).map(mark_entry));
    entries.extend(agent.work.iter().filter(|w| inside(w.span.start)).map(|w| {
        let detail =
            TipLine::coloured(format!("#{}", w.issue), ticket_colour(w.issue)).then(w.ended.as_ref().map(|e| format!(" · ended by {e}")).unwrap_or_default());
        Entry::span(w.span.start, w.span.end, w.span.open, ticket_colour(w.issue), "working on")
            .detail(detail)
            .goes_to(Some(Subject::Ticket { issue: w.issue }))
    }));
    for sub in agent.subs.iter().filter(|s| inside(s.span.start)) {
        let run = ctx.runs.iter().find(|r| r.sub == sub.sub && r.start == sub.span.start).filter(|_| sub.is_gate());
        entries.push(run.map_or_else(|| helper_entry(ctx, agent, sub), |run| run_entry(ctx, run)));
    }
    entries.extend(session.pauses.iter().map(|p| Entry::span(p.start, p.end, p.open, colours::STOPPED, "paused")));
    if !session.span.open {
        let by = session.ended_by.as_ref().map(|by| TipLine::plain(format!("ended by {}", ctx.who(by).name)));
        let mut stopped = Entry::at(end, SESSION, "session ended").detail(by.unwrap_or_default());
        stopped.notes.extend(session.notes.iter().skip(1).map(TipLine::plain));
        entries.push(stopped);
    }
    let entries: Vec<Entry> = in_order(entries).into_iter().map(|e| if e.who.is_none() { e.by(me.clone()) } else { e }).collect();
    Some(Details {
        subject: Subject::Session { instance: instance.to_owned(), at: Some(start) },
        colour: SESSION,
        mark: mark_of(&agent.name),
        title: format!("{} · {}", agent.name, agent.machine),
        lines: session_lines(ctx, agent, session),
        labels: Vec::new(),
        facts: session_facts(agent, session, &entries),
        related: Vec::new(),
        entries,
        pages: Vec::new(),
        timeline: OnTimeline::default(),
    })
}

/// An instant of a session's: a claim, a submit, a merge, a send back, a stop, a release, a
/// session it ended, an event this reader doesn't know.
fn mark_entry(mark: &Mark) -> Entry {
    let colour = match mark.event.as_str() {
        "claim" => colours::CLAIMED,
        "submit" => colours::WAITING,
        "merge" => PASSED,
        "sendback" => FAILED,
        _ => colours::STOPPED,
    };
    let what = match mark.event.as_str() {
        "claim" => "claimed",
        "submit" => "submitted",
        "merge" => "merged",
        "sendback" => "sent back",
        "stop" => "stopped",
        "release" => "released",
        "session.stop" => "ended a session",
        other => other,
    };
    let mut detail = TipLine::default();
    if let Some(issue) = mark.issue {
        detail = detail.then_coloured(format!("#{issue}"), ticket_colour(issue));
    }
    if let Some(mr) = mark.mr {
        detail = detail.then(format!(" PR #{mr}"));
    }
    if let Some(reason) = &mark.reason {
        detail = detail.then(format!(" ({reason})"));
    }
    if let Some(sub) = &mark.sub {
        detail = detail.then(format!(" in {sub}"));
    }
    let note = mark.note.as_ref().map(|n| if mark.event == "sendback" { TipLine::coloured(n, FAILED) } else { TipLine::plain(n) });
    let goes_to = mark.issue.map(|issue| Subject::Ticket { issue });
    Entry::at(mark.t, colour, what).detail(detail).note(note).goes_to(goes_to)
}

fn session_lines(ctx: &Ctx<'_>, agent: &Agent, session: &Session) -> Vec<TipLine> {
    let mut first = TipLine::strong(role_name(&agent.role));
    for part in [&agent.model, &agent.effort].into_iter().flatten() {
        first = first.then(format!(" · {part}"));
    }
    let mut second = TipLine::plain(format!("instance {}", agent.instance));
    if let Some(parent) = &agent.parent {
        second = second.then(format!(" · started by {}", ctx.who(parent).name));
    }
    let end = if session.span.open { "…".to_owned() } else { session.span.end.to_string() };
    let took = duration(session.span.end.0.saturating_sub(session.span.start.0));
    let third = TipLine::plain(format!("{} → {end} (", session.span.start)).then_strong(took).then(if session.span.open { " so far)" } else { ")" });
    vec![first, second, third]
}

fn session_facts(agent: &Agent, session: &Session, entries: &[Entry]) -> Vec<Fact> {
    let on = session.span.end.0.saturating_sub(session.span.start.0).max(0);
    let paused: i64 = session.pauses.iter().map(|p| p.end.0.saturating_sub(p.start.0).max(0)).sum();
    let mut tickets: Vec<u64> = entries
        .iter()
        .filter_map(|e| match e.goes_to {
            Some(Subject::Ticket { issue }) => Some(issue),
            _ => None,
        })
        .collect();
    tickets.sort_unstable();
    tickets.dedup();
    let mut facts = vec![Fact::new("on shift", duration(on))];
    if paused > 0 {
        facts.push(Fact::coloured("paused", duration(paused), colours::STOPPED));
    }
    facts.push(Fact::new("tickets", tickets.len().to_string()));
    let merged = entries.iter().filter(|e| e.what == "merged").count();
    if merged > 0 {
        facts.push(Fact::coloured("merged", merged.to_string(), PASSED));
    }
    let subs = agent.subs.iter().filter(|s| session.span.start <= s.span.start && s.span.start <= session.span.end).count();
    if subs > 0 {
        facts.push(Fact::new("helpers", subs.to_string()));
    }
    facts
}

// --- A merge onto main ---

fn merge_details(ctx: &Ctx<'_>, commit: &str) -> Option<Details> {
    let commit = commit.trim().to_ascii_lowercase();
    let merge = ctx.merges.iter().find(|m| m.commit == commit || (commit.len() >= 7 && m.commit.starts_with(&commit)))?;
    let ticket = merge.pr.and_then(|pr| ctx.tickets.iter().find(|t| t.merged.as_ref().is_some_and(|m| m.pr == Some(pr))));
    let mut entries = Vec::new();
    let mut related = Vec::new();
    if let Some(ticket) = ticket {
        related.push(ctx.related_ticket(ticket.issue));
        if let Some(finished) = ticket.finished {
            let by = ticket.stretches.iter().rev().find(|s| s.phase == Phase::Waiting).map(|s| ctx.who(&s.by));
            entries.push(
                Entry::at(finished, colours::WAITING, format!("#{} submitted", ticket.issue)).by(by).goes_to(Some(Subject::Ticket { issue: ticket.issue })),
            );
        }
        if let Some(merged) = &ticket.merged {
            if let Some(run) = merged.sub.as_deref().and_then(|sub| ctx.run(sub, Some(merged.t))) {
                entries.push(run_entry(ctx, run));
                related.push(Related {
                    subject: Subject::Gate { sub: run.sub.clone(), at: Some(run.start) },
                    mark: "gate".into(),
                    label: run.sub.clone(),
                    colour: outcome_look(run.outcome).0,
                });
            }
            let entry = Entry::at(merged.t, ticket_colour(ticket.issue), format!("#{} merged", ticket.issue)).by(Some(ctx.who(&merged.by)));
            entries.push(entry.goes_to(Some(Subject::Ticket { issue: ticket.issue })));
        }
    }
    entries.push(Entry::at(merge.t, PASSED, "onto main").detail(TipLine::plain(format!("{} {}", short_commit(&merge.commit), merge.subject))));
    let title = match merge.pr {
        Some(pr) => titles::title(ctx.issues, pr)
            .map_or_else(|| format!("PR #{pr} merged onto main"), |t| format!("PR #{pr} {}", titles::short(t, titles::TOOLTIP_TITLE))),
        None => format!("{} merged onto main", short_commit(&merge.commit)),
    };
    let mark = match (ticket, merge.pr) {
        (Some(t), _) => format!("#{}", t.issue),
        (None, Some(pr)) => format!("!{pr}"),
        (None, None) => short_commit(&merge.commit).chars().take(4).collect(),
    };
    let mut facts = vec![Fact::new("commit", short_commit(&merge.commit))];
    if let Some(from) = ticket.and_then(|t| t.claimed.or(t.began)) {
        facts.push(Fact::coloured("claim → main", duration(merge.t.0.saturating_sub(from.0)), PASSED));
    }
    if ticket.is_none() {
        facts.push(Fact::coloured("ticket", "none names it", colours::NO_TICKET));
    }
    let pages = ticket.map(|t| Page::issue(t.issue)).into_iter().chain(merge.pr.map(Page::pull)).collect();
    Some(Details {
        subject: Subject::Merge { commit: merge.commit.clone() },
        colour: ticket.map_or(colours::MAIN_MERGE, |t| ticket_colour(t.issue)),
        mark,
        title,
        lines: vec![TipLine::plain(merge.subject.clone()), TipLine::plain(format!("{} · ", merge.t)).then_coloured("merged onto main", PASSED)],
        labels: Vec::new(),
        facts,
        related,
        entries: in_order(entries),
        pages,
        timeline: OnTimeline::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::tests::sample;
    use crate::titles::tests::issues;
    use swarmception_model::{parse_file, parse_merge};

    fn merges() -> Vec<MainMerge> {
        ["be043bc0aa\u{1f}1793420970\u{1f}Merge pull request #173 from mornycorp/170-spec"].into_iter().filter_map(parse_merge).collect()
    }

    fn of(subject: Subject) -> Details {
        details(&Activity::of(&sample()), &merges(), &issues(), &subject).unwrap()
    }

    fn whats(details: &Details) -> Vec<&str> {
        details.entries.iter().map(|e| e.what.as_str()).collect()
    }

    /// #77: a failed `ready` check in a ticket's story says why: its stop's cause and the `ci`
    /// step it failed at; an older stop, `rc=1` alone, has no cause line, as before.
    #[test]
    fn a_failed_ready_check_says_its_cause_and_step() {
        let worker = r#"{"event":"session.start","format":3,"machine":"mini","name":"worker-2","role":"worker","t":"2026-10-31T03:53:06Z"}
{"event":"claim","issue":170,"mr":173,"t":"2026-10-31T03:53:22Z"}
{"event":"work","issue":170,"t":"2026-10-31T03:53:25Z"}
{"event":"sub.start","name":"ready","sub":"ready-aaa","t":"2026-10-31T04:00:00Z"}
{"event":"sub.stop","note":"rc=101; cause=build; step=clippy","sub":"ready-aaa","t":"2026-10-31T04:01:00Z"}
{"event":"sub.start","name":"ready","sub":"ready-bbb","t":"2026-10-31T04:02:00Z"}
{"event":"sub.stop","note":"rc=1","sub":"ready-bbb","t":"2026-10-31T04:03:00Z"}
{"event":"sub.start","name":"ready","sub":"ready-ccc","t":"2026-10-31T04:04:00Z"}
{"event":"sub.stop","note":"rc=0","sub":"ready-ccc","t":"2026-10-31T04:05:00Z"}"#;
        let files = [parse_file("mini-w2", worker)];
        let d = details(&Activity::of(&files), &[], &issues(), &Subject::Ticket { issue: 170 }).unwrap();
        let checks: Vec<(&str, Vec<String>)> =
            d.entries.iter().filter(|e| e.what.starts_with("ready check")).map(|e| (e.what.as_str(), crate::tooltip::texts(&e.notes))).collect();
        assert_eq!(
            checks,
            [
                ("ready check failed", vec!["cause build/clippy".to_owned(), "rc=101; cause=build; step=clippy".to_owned()]),
                ("ready check failed", vec!["rc=1".to_owned()]),
                ("ready check passed", vec!["rc=0".to_owned()]),
            ]
        );
        let cause = d.entries.iter().find_map(|e| e.notes.first().filter(|n| n.text().starts_with("cause"))).unwrap();
        assert_eq!(cause, &TipLine::coloured("cause build/clippy", FAILED), "in the failed colour");
    }

    #[test]
    fn a_tickets_story_is_its_stretches_checks_gate_runs_and_merge_in_order() {
        let d = of(Subject::Ticket { issue: 170 });
        assert_eq!(d.title, "The spec, written down");
        assert_eq!((d.mark.as_str(), d.colour), ("#170", ticket_colour(170)));
        assert_eq!(d.labels, ["type::task", "area::docs"]);
        assert_eq!(d.lines[0].text(), "closed · opened by cmaughan");
        assert_eq!(d.lines[1].text(), "PR #173 Spec: the first draft (merged)");
        assert_eq!(whats(&d), ["claimed", "working", "ready check passed", "waiting for merge", "gate run passed", "merged"]);
        let times: Vec<Timestamp> = d.entries.iter().map(|e| e.t).collect();
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "oldest first");
        let working = &d.entries[1];
        assert_eq!((working.colour, working.duration.as_deref()), (ticket_colour(170), Some("13 min 18 s")));
        assert_eq!(working.who.as_ref().map(|w| w.name.as_str()), Some("worker-2"));
        assert_eq!(working.text(), "working · worker-2 · 13 min 18 s");
        assert_eq!(d.entries[0].colour, colours::CLAIMED);
        assert_eq!(d.entries[3].colour, colours::WAITING);
        let gate = &d.entries[4];
        assert_eq!((gate.colour, gate.detail.text().as_str()), (PASSED, "gate-pr173 · smoke"));
        assert_eq!(gate.goes_to, Some(Subject::Gate { sub: "gate-pr173".into(), at: Some(gate.t) }));
        let merged = &d.entries[5];
        assert_eq!(merged.text(), "merged PR #173 in gate-pr173 · coordinator");
        assert_eq!(merged.notes[0].text(), "onto main be043bc0 Merge pull request #173 from mornycorp/170-spec");
        let facts: Vec<(String, String)> = d.facts.iter().map(|f| (f.label.clone(), f.value.clone())).collect();
        assert_eq!(facts[0], ("claim → merge".into(), "35 min 36 s".into()));
        assert!(facts.contains(&("gate runs".into(), "1 (1 passed, 0 failed)".into())), "{facts:?}");
        assert!(facts.contains(&("worker".into(), "worker-2".into())));
        assert_eq!(d.pages, [Page::issue(170), Page::pull(173)]);
    }

    #[test]
    fn an_abandoned_ticket_says_whose_session_ended() {
        let d = of(Subject::Ticket { issue: 168 });
        assert_eq!(whats(&d), ["working", "abandoned"]);
        assert_eq!(d.entries[1].colour, FAILED);
        assert_eq!(d.entries[1].detail.text(), "worker-1's session ended (ended by coordinator)");
        assert_eq!(d.facts[0].label, "abandoned after");
        assert_eq!(d.lines[0].text(), "open", "GitHub's state; no author or PR");
    }

    #[test]
    fn a_ready_check_tells_its_own_story() {
        // #95: as a gate run's, by its id; the ticket it checked one click away.
        let d = of(Subject::Gate { sub: "ready-4bb1de4".into(), at: None });
        assert_eq!((d.mark.as_str(), d.title.as_str()), ("ready", "ready check ready-4bb1de4"));
        assert_eq!(d.subject, Subject::Gate { sub: "ready-4bb1de4".into(), at: Timestamp::parse("2026-10-31T04:06:00Z") });
        assert!(d.lines[0].text().contains("by worker-2"), "{:?}", d.lines[0].text());
        assert_eq!(whats(&d), ["started", "passed"]);
        assert_eq!(d.related.iter().map(|r| r.subject.clone()).collect::<Vec<_>>(), [Subject::Ticket { issue: 170 }]);
        // Its line in the ticket's story goes to it.
        let ticket = of(Subject::Ticket { issue: 170 });
        let entry = ticket.entries.iter().find(|e| e.what == "ready check passed").unwrap();
        assert_eq!(entry.goes_to, Some(d.subject.clone()));
    }

    #[test]
    fn a_gate_runs_story_is_its_start_its_merges_and_its_end() {
        let d = of(Subject::Gate { sub: "gate-pr173".into(), at: None });
        assert_eq!(d.title, "gate run gate-pr173");
        assert_eq!(d.colour, PASSED);
        assert_eq!(whats(&d), ["started", "merged #170", "passed", "onto main"]);
        assert_eq!(d.entries[0].notes[0].text(), "local gate: #173 (#170); capacity=3 free=15.0; kind=smoke");
        assert_eq!(d.entries[1].goes_to, Some(Subject::Ticket { issue: 170 }));
        assert_eq!(d.entries[2].duration.as_deref(), Some("9 min 02 s"));
        assert_eq!(d.entries[2].notes[0].text(), "rc=0; kind=smoke; merged #173");
        assert_eq!(d.related.iter().map(|r| r.mark.as_str()).collect::<Vec<_>>(), ["#170"]);
        assert_eq!(d.related[0].label, "The spec, written down");
        assert_eq!(d.pages, [Page::pull(173), Page::issue(170)]);
        assert!(d.facts.iter().any(|f| f.label == "merged" && f.value == "PR #173"));
        assert_eq!(d.lines[0].text(), "passed in 9 min 02 s · by coordinator");
    }

    #[test]
    fn a_sessions_story_is_everything_it_did() {
        let d = of(Subject::Session { instance: "mini-79ba63".into(), at: None });
        assert_eq!(d.title, "worker-2 · mini");
        assert_eq!(d.mark, "w2");
        assert_eq!(whats(&d), ["session started", "claimed", "working on", "ready check passed", "submitted", "paused", "stopped", "session ended"]);
        assert_eq!(d.entries[2].goes_to, Some(Subject::Ticket { issue: 170 }));
        assert_eq!(d.entries[5].duration.as_deref(), Some("2 min 00 s"));
        assert_eq!(d.lines[0].text(), "worker · claude-opus-5-5 · high");
        assert!(d.entries.iter().all(|e| e.who.as_ref().is_some_and(|w| w.name == "worker-2")));
        assert_eq!(d.facts[0], Fact::new("on shift", "23 min 58 s"));
        let coordinator = of(Subject::Session { instance: "mini-8d44f3".into(), at: None });
        assert!(whats(&coordinator).contains(&"gate run passed"));
        assert!(whats(&coordinator).contains(&"ended a session"));
    }

    #[test]
    fn a_merge_onto_mains_story_is_its_gate_its_tickets_merge_and_the_commit() {
        let d = of(Subject::Merge { commit: "be043bc".into() });
        assert_eq!(d.title, "PR #173 Spec: the first draft");
        assert_eq!(d.mark, "#170");
        assert_eq!(whats(&d), ["#170 submitted", "gate run passed", "#170 merged", "onto main"]);
        assert_eq!(d.related.iter().map(|r| r.mark.as_str()).collect::<Vec<_>>(), ["#170", "gate"]);
        assert!(details(&Activity::of(&sample()), &merges(), &issues(), &Subject::Merge { commit: "be04".into() }).is_none(), "too short a prefix");
    }

    #[test]
    fn what_the_swarm_never_had_has_no_details() {
        let (activity, issues) = (Activity::of(&sample()), issues());
        for subject in [
            Subject::Ticket { issue: 5 },
            Subject::Gate { sub: "gate-nope".into(), at: None },
            Subject::Session { instance: "x-1".into(), at: None },
            Subject::Merge { commit: "deadbeef".into() },
        ] {
            assert!(details(&activity, &merges(), &issues, &subject).is_none(), "{subject:?}");
        }
        assert!(details(&Activity::of(&[]), &[], &Issues::default(), &Subject::Ticket { issue: 170 }).is_none());
    }

    #[test]
    fn a_bar_is_its_ticket_or_its_merge_onto_main() {
        let activity = Activity::of(&sample());
        let rows = crate::rows::rows(&activity, &merges(), crate::RowsSpec::default());
        assert_eq!(bar_subject(&rows, &merges(), crate::rows::MAIN_KEY, 0), Some(Subject::Merge { commit: "be043bc0aa".into() }));
        assert_eq!(bar_subject(&rows, &merges(), crate::rows::MERGED_KEY, 0), Some(Subject::Ticket { issue: 170 }));
        assert_eq!(bar_subject(&rows, &merges(), "tickets/2", 0), Some(Subject::Ticket { issue: 170 }), "#110: its one bar");
        assert_eq!(bar_subject(&rows, &merges(), "tickets/2", 99), None);
        assert_eq!(bar_subject(&rows, &merges(), "nope", 0), None);
        // #84: a gate run's bar is the run; #95: a ready check's, the check.
        let at = Timestamp::parse("2026-10-31T04:20:00Z");
        assert_eq!(bar_subject(&rows, &merges(), crate::rows::GATES_KEY, 0), Some(Subject::Gate { sub: "gate-pr173".into(), at }));
        let ready = activity.agents.iter().flat_map(|a| &a.subs).find(|s| crate::stats::is_ready(s)).unwrap();
        assert_eq!(bar_subject(&rows, &merges(), crate::rows::READY_KEY, 0), Some(Subject::Gate { sub: ready.sub.clone(), at: Some(ready.span.start) }));
        assert_eq!(bar_subject(&rows, &merges(), crate::rows::READY_KEY, 1), None);
    }

    #[test]
    fn each_subject_names_its_bars_on_the_timeline() {
        let (activity, issues) = (Activity::of(&sample()), issues());
        for grouping in [crate::Grouping::Flat, crate::Grouping::Workers] {
            let rows = crate::rows::rows(&activity, &merges(), crate::RowsSpec { grouping, collapse: crate::Collapse::Off });
            let on = |subject: Subject| details_on(&activity, &merges(), &issues, &rows, &subject).unwrap().timeline;
            let of = |keep: &dyn Fn(&str, &Bar) -> bool| {
                let mut bars: Vec<BarRef> = bars_where(&rows, keep).into_iter().map(|(at, _)| at).collect();
                bars.sort();
                bars
            };
            let main = BarRef { track: MAIN_KEY.into(), range: 0 };
            let merged = BarRef { track: crate::rows::MERGED_KEY.into(), range: 0 };

            // A ticket: its bar (#110: one a row) and its merge, from its claim to its merge's end.
            let ticket = on(Subject::Ticket { issue: 170 });
            let tickets = of(&|_, bar| bar.ticket == Some(170));
            assert_eq!(ticket.bars, tickets, "{grouping:?}");
            assert_eq!(ticket.bars.len(), 2, "its bar and its merge");
            assert_eq!(ticket.start, Timestamp::parse("2026-10-31T03:53:22Z"));
            assert_eq!(ticket.end, Timestamp::parse("2026-10-31T04:29:08Z"), "its merge's 10 s");
            // The groups its rows are in (#68): by worker, its machine's and its worker's.
            let keys: Vec<&str> = tickets.iter().map(|bar| bar.track.as_str()).collect();
            assert_eq!(ticket.groups, crate::rows::groups_holding(&rows, &keys));
            assert_eq!(ticket.groups.is_empty(), grouping == crate::Grouping::Flat, "{grouping:?}: {:?}", ticket.groups);

            // A gate run: the merge onto main it made, its ticket's bars and its own on the Gate
            // runs row (#84).
            let gate = on(Subject::Gate { sub: "gate-pr173".into(), at: None });
            let mut want = tickets.clone();
            want.push(main.clone());
            want.sort();
            let mut with_run = want.clone();
            with_run.push(BarRef { track: crate::rows::GATES_KEY.into(), range: 0 });
            with_run.sort();
            assert_eq!(gate.bars, with_run, "{grouping:?}");
            // A merge onto main: its bar on main's row, and its ticket's.
            assert_eq!(on(Subject::Merge { commit: "be043bc".into() }).bars, want);

            // A session: what it began in its time. The worker's: #170's stretches, not its merge.
            let worker = on(Subject::Session { instance: "mini-79ba63".into(), at: None });
            assert_eq!(worker.bars, of(&|key, bar| bar.ticket == Some(170) && key != crate::rows::MERGED_KEY), "{grouping:?}");
            // The coordinator's: the ticket it merged and its merge onto main.
            assert_eq!(on(Subject::Session { instance: "mini-8d44f3".into(), at: None }).bars, [main.clone(), merged.clone()]);
            // The abandoned worker's: #168's stretch.
            assert_eq!(on(Subject::Session { instance: "mini-dead01".into(), at: None }).bars, of(&|_, bar| bar.ticket == Some(168)));
        }
        // With no rows to read, nothing is drawn.
        assert_eq!(of(Subject::Ticket { issue: 170 }).timeline, OnTimeline::default());
        let none = details_on(&activity, &merges(), &issues, &[], &Subject::Ticket { issue: 170 }).unwrap().timeline;
        assert_eq!((none.bars.len(), none.start, none.end), (0, None, None));
    }

    #[test]
    fn a_subject_reads_back() {
        let subject = Subject::Gate { sub: "gate-pr8".into(), at: None };
        let text = serde_json::to_string(&subject).unwrap();
        assert_eq!(text, r#"{"gate":{"sub":"gate-pr8"}}"#);
        assert_eq!(serde_json::from_str::<Subject>(&text).unwrap(), subject);
        assert_eq!(serde_json::from_str::<Subject>(r#"{"ticket":{"issue":7}}"#).unwrap(), Subject::Ticket { issue: 7 });
        assert!(serde_json::from_str::<Subject>(r#"{"ticket":{"issue":7,"x":1}}"#).is_err());
        assert_eq!(mark_of("coordinator"), "co");
        assert_eq!(mark_of("worker-19"), "w19");
    }
}
