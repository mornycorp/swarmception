//! How the gate behaved over time (#61, G9): every gate run, the coordinator's and the workers'
//! own `ready` checks kept apart, with what it carried, what it did and how it ended, and the
//! figures over a window. What the Gates pane draws, and `gates.list` returns.
//!
//! **The one definition of a gate run** (#75): [`runs`] reads them, and both `gates.list`
//! ([`gates`]) and `stats.get` (`Stats::gates` and `ready`) count them with [`figures`], so the
//! same window gives the same counts from both.
//!
//! - **A run** is a coordinator's gate ([`Sub::is_gate`]: its `sub.start` to its `sub.stop`) or a
//!   worker's `ready` check (the same). A gate that logged neither, but whose id a `merge` or
//!   `sendback` names, is a run too, read from those events (its first to its last): it is
//!   **inferred**, counted, and the figures say how many were (`inferred`), so an inferred figure
//!   is never mixed in silently. Its **result**: *passed* (`rc=0`, or no `rc` but a merge naming
//!   it while it ran), *failed* (another `rc`, its cause the stop note's `cause=…`, and a `ready`
//!   check's the `ci` step it failed at, `step=…`, #77; an older note's cause is unknown; an inferred
//!   run that only sent tickets back failed too, its cause the send back's), *no result* (it ended
//!   without saying, or never finished: a gate at its coordinator's session's end or the next
//!   gate's start, #42; a `ready` at its worker's session's end) or *running* (nothing ended it
//!   and its session goes on). Its **duration**: start to end; a running one's, to the last event.
//! - **What it carried**: the pull requests and tickets its start note and its events name (and
//!   for a `ready`, the ticket its worker was on and that ticket's PR); **its merges**: the
//!   tickets its events merged, each with its commit onto main; its **send backs**.
//! - **A rerun**: `gate-43b` of `gate-43` (and `gate-43c` of `gate-43b`), `gate-43-rerun` of
//!   `gate-43`, and a run whose id an earlier run of its lane had: each of the latest earlier run
//!   of that id. None when there is no such run.
//! - **The figures**, per lane, over the runs that *started* in the window: runs, passed,
//!   failed, no result, running, inferred, the **pass rate** (passed ÷ (passed + failed)), the
//!   **median** and the longest duration of the runs with a result, the time it was **busy** (the
//!   union of its runs' spans within the window, a running one to the last event) and that ÷ the
//!   window's span, the reruns, and the failures by cause (`unstated` without one; with a `ready`
//!   check's step when it says more, `build/clippy`: [`failure_label`]), most first.
//! - **Per hour** (clock hours, UTC, from the first run's hour in the window to the last's): each
//!   lane's runs started in it, passed, failed, no result (running ones among them), its pass
//!   rate and median duration, and its busy seconds in the hour. The Gates band draws both
//!   lanes' (#75).
//! - **The longest queue**: the most tickets *waiting for merge* at once (submitted, not yet
//!   merged or sent back) within the window, the first time it was reached, and which.
//!
//! Pure: no I/O, no clock.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Kind, MainMerge, Timestamp};

use crate::activity::{Activity, Agent, Sub};
use crate::colours::{self, Rgb};
use crate::details::Subject;
use crate::factory::{GateRun, NEVER_FINISHED, Outcome, gate_runs};
use crate::rows::duration;
use crate::stats::{Cause, Intervals, MAX_HOURS, Window, cause_of, failure_label, fraction, is_ready, median, ratio, step_of};
use crate::tickets::{Phase, Ticket};
use crate::tooltip::{FAILED, PASSED, TipLine};

/// What a running run is drawn in: waiting's blue, as the tickets it holds wait.
pub const RUNNING: Rgb = colours::WAITING;
/// What a run with no result is drawn in: stopped's grey.
pub const NO_RESULT: Rgb = colours::STOPPED;
const HOUR: i64 = 3_600;

/// Whose gate a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Lane {
    /// The coordinator's gate, before it merges.
    Gate,
    /// A worker's own check before it submits (`ready`).
    Ready,
}

/// How a run went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum GateResult {
    Passed,
    Failed,
    /// It ended without saying, or never finished.
    NoResult,
    /// Nothing ended it, and its session goes on.
    Running,
}

impl GateResult {
    /// How a person names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::NoResult => "no result",
            Self::Running => "running",
        }
    }

    /// Its colour: passed green, failed red, no result grey, running blue.
    #[must_use]
    pub const fn colour(self) -> Rgb {
        match self {
            Self::Passed => PASSED,
            Self::Failed => FAILED,
            Self::NoResult => NO_RESULT,
            Self::Running => RUNNING,
        }
    }
}

/// A ticket a run merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunMerge {
    pub t: Timestamp,
    pub issue: u64,
    pub pr: Option<u64>,
    /// Its merge commit onto main, when `main` has one for its PR.
    pub commit: Option<String>,
}

/// One gate run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    /// Its sub-agent's id: `gate-43`, `ready-4bb1de4`.
    pub sub: String,
    pub lane: Lane,
    /// The instance that ran it, and its name.
    pub instance: String,
    pub name: String,
    pub start: Timestamp,
    /// When it ended; none while it runs.
    pub end: Option<Timestamp>,
    /// To its end; a running one's, to the last event.
    pub duration_seconds: i64,
    /// Its notes' `kind=`: smoke, full, docs.
    pub kind: Option<String>,
    pub result: GateResult,
    /// Why it failed (its stop note's `cause=`, a send back's note), or `never finished`.
    pub cause: Option<String>,
    /// The `ci` step a failed `ready` check failed at: its stop note's `step=` (#77). None for a
    /// gate run, a pass, and a note from before swarm-core wrote it.
    pub step: Option<String>,
    /// The pull requests and the tickets it carried.
    pub prs: Vec<u64>,
    pub tickets: Vec<u64>,
    pub merges: Vec<RunMerge>,
    /// The tickets it sent back.
    pub sendbacks: Vec<u64>,
    /// The run it reran: `gate-43` for `gate-43b`.
    pub rerun_of: Option<String>,
    /// Read from its merges and send backs: it logged no `sub.start`.
    pub inferred: bool,
    /// Its result's colour.
    pub colour: Rgb,
    /// What the details pane tells of it, when it is chosen: the gate run itself; for a `ready`,
    /// the ticket it checked, else its worker's session.
    pub subject: Subject,
    /// Its tooltip, in colour.
    pub tooltip: Vec<TipLine>,
}

/// One lane's figures over a window (the module's docs define each).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Figures {
    /// The runs that started in the window.
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub no_result: usize,
    pub running: usize,
    /// Of the runs, those read from merges and send backs: no `sub.start` logged.
    pub inferred: usize,
    /// Passed ÷ (passed + failed); none when neither.
    pub pass_rate: Option<f64>,
    /// Over the runs with a result.
    pub median_seconds: Option<i64>,
    pub longest_seconds: Option<i64>,
    /// The union of its runs' spans within the window.
    pub busy_seconds: i64,
    /// Busy ÷ the window's span; none with no span.
    pub busy: Option<f64>,
    pub reruns: usize,
    /// The failures by cause, most first.
    pub causes: Vec<Cause>,
}

/// One lane's runs in a clock hour.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HourFigures {
    /// The runs that started in it.
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub no_result: usize,
    pub pass_rate: Option<f64>,
    pub median_seconds: Option<i64>,
    /// The time its runs went on within the hour.
    pub busy_seconds: i64,
}

/// A clock hour of the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GateHour {
    /// The hour's start, UTC.
    pub start: Timestamp,
    pub gate: HourFigures,
    pub ready: HourFigures,
}

/// The most tickets waiting for merge at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub count: usize,
    /// The first time it was reached; none when nothing waited.
    pub at: Option<Timestamp>,
    /// The tickets waiting then.
    pub tickets: Vec<u64>,
}

/// The gate's runs over a window, and its figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatesList {
    /// The window, its ends included.
    pub start: Timestamp,
    pub end: Timestamp,
    pub span_seconds: i64,
    /// Every run that went on in the window (overlapping it), by start: both lanes.
    pub runs: Vec<Run>,
    /// The coordinator's gates' figures, and the workers' `ready` checks'.
    pub gate: Figures,
    pub ready: Figures,
    /// Each clock hour from the first run's to the last's.
    pub hours: Vec<GateHour>,
    /// Whether the hours were cut to their last [`MAX_HOURS`].
    pub hours_cut: bool,
    pub longest_queue: Queue,
}

/// The gate's runs and figures over `window` (its ends left out: the run's first and last
/// events), from the activity, its tickets and `main`'s merges.
#[must_use]
pub fn gates(activity: &Activity, tickets: &[Ticket], merges: &[MainMerge], window: Window) -> GatesList {
    let bounds = Bounds::of(activity, window);
    let Bounds { start, end, last } = bounds;
    let reach = |run: &Run| run.end.unwrap_or(last).max(run.start);
    let mut runs: Vec<Run> = runs(activity, tickets, merges).into_iter().filter(|r| r.start <= end && reach(r) >= start).collect();
    for run in &mut runs {
        run.tooltip = tooltip(activity, run);
    }
    let (hours, hours_cut) = hourly(&runs, start, end, last);
    GatesList {
        start,
        end,
        span_seconds: end.0.saturating_sub(start.0),
        gate: lane_figures(&runs, Lane::Gate, bounds),
        ready: lane_figures(&runs, Lane::Ready, bounds),
        hours,
        hours_cut,
        longest_queue: longest_queue(tickets, start, end),
        runs,
    }
}

/// A window's ends, and the run's last event (where a running run's span stops).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bounds {
    pub(crate) start: Timestamp,
    pub(crate) end: Timestamp,
    pub(crate) last: Timestamp,
}

impl Bounds {
    /// `window`'s ends, each the run's own when left out; an end before the start is the start.
    pub(crate) fn of(activity: &Activity, window: Window) -> Self {
        let (first, last) = activity.extent.unwrap_or_default();
        let start = window.start.unwrap_or(first);
        Self { start, end: window.end.unwrap_or(last).max(start), last }
    }

    /// The window as a set of seconds.
    pub(crate) fn span(self) -> Intervals {
        Intervals::span(self.start.0, self.end.0)
    }
}

/// Every gate run of both lanes, by start (the lane, then the id, at a tie), each with its
/// result and what it reran: the module's one definition of a gate run, which `gates.list` and
/// `stats.get` both count (#75). Their tooltips are left empty: [`gates`] adds them to the runs
/// it returns.
pub(crate) fn runs(activity: &Activity, tickets: &[Ticket], merges: &[MainMerge]) -> Vec<Run> {
    let (_, last) = activity.extent.unwrap_or_default();
    let mut all = coordinator_runs(activity, tickets, merges, last);
    all.extend(ready_runs(activity, tickets, last));
    all.sort_by(|a, b| (a.start, a.lane, &a.sub).cmp(&(b.start, b.lane, &b.sub)));
    mark_reruns(&mut all);
    all
}

/// One lane's figures over `bounds`, from every run: the counts `gates.list` and `stats.get`
/// both give.
pub(crate) fn lane_figures(runs: &[Run], lane: Lane, bounds: Bounds) -> Figures {
    figures(runs.iter().filter(|r| r.lane == lane), &bounds.span(), bounds.last, bounds.start, bounds.end)
}

/// The instance and name of `instance`.
fn who(activity: &Activity, instance: &str) -> (String, String) {
    let name = activity.agent(instance).map_or_else(|| instance.to_owned(), |a| a.name.clone());
    (instance.to_owned(), name)
}

/// The number `n` is a ticket's issue.
fn is_ticket(tickets: &[Ticket], n: u64) -> bool {
    tickets.iter().any(|t| t.issue == n)
}

fn push_unique(list: &mut Vec<u64>, value: u64) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// Splits `refs` (a note's `#<n>`s, PRs and issues alike) into PRs and tickets, adding the
/// tickets whose PR is named and the PRs of the tickets named.
fn carried(tickets: &[Ticket], refs: &[u64], issues: &[u64], prs: &[u64]) -> (Vec<u64>, Vec<u64>) {
    let (mut carried_prs, mut carried_tickets) = (Vec::new(), Vec::new());
    for &n in issues {
        push_unique(&mut carried_tickets, n);
    }
    for &n in refs.iter().chain(prs) {
        if is_ticket(tickets, n) {
            push_unique(&mut carried_tickets, n);
        } else {
            push_unique(&mut carried_prs, n);
        }
    }
    for ticket in tickets {
        if ticket.pr.is_some_and(|pr| carried_prs.contains(&pr)) {
            push_unique(&mut carried_tickets, ticket.issue);
        }
    }
    for &issue in &carried_tickets.clone() {
        if let Some(pr) = tickets.iter().find(|t| t.issue == issue).and_then(|t| t.pr) {
            push_unique(&mut carried_prs, pr);
        }
    }
    (carried_prs, carried_tickets)
}

/// The main merge commit whose pull request is `pr`, the first at or after `at`.
fn main_commit(merges: &[MainMerge], pr: Option<u64>, at: Timestamp) -> Option<String> {
    let pr = pr?;
    let mut found = merges.iter().filter(|m| m.pr == Some(pr));
    found.clone().find(|m| m.t >= at).or_else(|| found.next()).map(|m| m.commit.clone())
}

/// The coordinator's gate runs: as the factory reads them, with their results as `stats.get`
/// counts them.
fn coordinator_runs(activity: &Activity, tickets: &[Ticket], merges: &[MainMerge], last: Timestamp) -> Vec<Run> {
    gate_runs(activity, Timestamp(i64::MAX)).into_iter().map(|run| coordinator_run(activity, tickets, merges, last, run)).collect()
}

fn coordinator_run(activity: &Activity, tickets: &[Ticket], merges: &[MainMerge], last: Timestamp, run: GateRun) -> Run {
    let owner = activity.agents.iter().find(|a| a.subs.iter().any(|s| s.sub == run.sub && s.span.start == run.start));
    let until = run.end.unwrap_or(Timestamp(i64::MAX));
    let events: Vec<_> = activity.issue_events.iter().filter(|e| e.sub.as_deref() == Some(run.sub.as_str()) && run.start <= e.t && e.t <= until).collect();
    let instance = owner.map(|a| a.instance.clone()).or_else(|| events.first().map(|e| e.instance.clone())).unwrap_or_default();
    let mut run_merges: Vec<RunMerge> = Vec::new();
    let mut sendbacks = Vec::new();
    let mut event_prs = Vec::new();
    for event in &events {
        event_prs.extend(event.kind.mr());
        match event.kind {
            Kind::Merge { mr, .. } if !run_merges.iter().any(|m| m.issue == event.issue) => {
                run_merges.push(RunMerge { t: event.t, issue: event.issue, pr: mr, commit: main_commit(merges, mr, run.start) });
            }
            Kind::Sendback { .. } => push_unique(&mut sendbacks, event.issue),
            _ => {}
        }
    }
    let result = match run.outcome {
        Outcome::Running => GateResult::Running,
        Outcome::Passed => GateResult::Passed,
        Outcome::Failed => GateResult::Failed,
        // As `stats.get`: no `rc`, but it merged a ticket.
        Outcome::Unknown if !run_merges.is_empty() => GateResult::Passed,
        Outcome::Unknown => GateResult::NoResult,
    };
    let cause = match result {
        GateResult::Failed | GateResult::NoResult => run.cause.clone(),
        GateResult::Passed | GateResult::Running => None,
    };
    let (prs, carried_tickets) = carried(tickets, &run.refs, &run.issues, &event_prs);
    let (instance, name) = who(activity, &instance);
    let duration_seconds = run.end.unwrap_or(last).0.saturating_sub(run.start.0).max(0);
    Run {
        subject: Subject::Gate { sub: run.sub.clone(), at: Some(run.start) },
        sub: run.sub,
        lane: Lane::Gate,
        instance,
        name,
        start: run.start,
        end: run.end,
        duration_seconds,
        kind: run.kind,
        result,
        cause,
        step: None,
        prs,
        tickets: carried_tickets,
        merges: run_merges,
        sendbacks,
        rerun_of: None,
        inferred: run.inferred,
        colour: result.colour(),
        tooltip: Vec::new(),
    }
}

/// The workers' own `ready` checks.
fn ready_runs(activity: &Activity, tickets: &[Ticket], last: Timestamp) -> Vec<Run> {
    activity.agents.iter().flat_map(|agent| agent.subs.iter().filter(|s| is_ready(s)).map(move |sub| ready_run(activity, tickets, agent, sub, last))).collect()
}

fn ready_run(activity: &Activity, tickets: &[Ticket], agent: &Agent, sub: &Sub, last: Timestamp) -> Run {
    let start = sub.span.start;
    let left = sub.span.open.then(|| agent.session_ended_after(start)).flatten();
    let (result, end, cause) = match (sub.rc, sub.span.open, left) {
        (Some(0), ..) => (GateResult::Passed, Some(sub.span.end), None),
        (Some(_), ..) => (GateResult::Failed, Some(sub.span.end), sub.stop_note.as_deref().and_then(cause_of)),
        (None, true, Some(session)) => (GateResult::NoResult, Some(session.span.end.max(start)), Some(NEVER_FINISHED.to_owned())),
        (None, true, None) => (GateResult::Running, None, None),
        (None, false, _) => (GateResult::NoResult, Some(sub.span.end), None),
    };
    // The `ci` step it failed at (#77): an older note names none, its cause unknown as before.
    let step = if result == GateResult::Failed { sub.stop_note.as_deref().and_then(step_of) } else { None };
    let on = agent.work.iter().filter(|w| w.span.start <= start && start <= w.span.end).map(|w| (w.issue, w.mr)).next_back();
    let mut issues = sub.issues.clone();
    let mut prs = sub.prs.clone();
    if let Some((issue, mr)) = on {
        push_unique(&mut issues, issue);
        prs.extend(mr);
    }
    let (prs, carried_tickets) = carried(tickets, &[], &issues, &prs);
    let kind = [&sub.start_note, &sub.stop_note].into_iter().flatten().find_map(|n| crate::factory::note_field(n, "kind"));
    let subject =
        carried_tickets.first().map_or_else(|| Subject::Session { instance: agent.instance.clone(), at: Some(start) }, |&issue| Subject::Ticket { issue });
    let (instance, name) = who(activity, &agent.instance);
    Run {
        sub: sub.sub.clone(),
        lane: Lane::Ready,
        instance,
        name,
        start,
        end,
        duration_seconds: end.unwrap_or(last).0.saturating_sub(start.0).max(0),
        kind: kind.and_then(|k| k.split_whitespace().next().map(str::to_owned)),
        result,
        cause,
        step,
        prs,
        tickets: carried_tickets,
        merges: Vec::new(),
        sendbacks: Vec::new(),
        rerun_of: None,
        inferred: false,
        colour: result.colour(),
        subject,
        tooltip: Vec::new(),
    }
}

/// The ids `sub` reruns, nearest first: `gate-43` (and `gate-43b`) for `gate-43c`, `gate-43` for
/// `gate-43-rerun`, and `sub` itself.
fn rerun_bases(sub: &str) -> Vec<String> {
    let mut bases = Vec::new();
    if let Some(base) = sub.strip_suffix("-rerun").filter(|b| !b.is_empty()) {
        bases.push(base.to_owned());
    }
    let mut chars = sub.chars().rev();
    if let (Some(letter @ 'b'..='z'), Some(before)) = (chars.next(), chars.next())
        && before.is_ascii_digit()
        && let Some(base) = sub.get(..sub.len() - 1)
    {
        for earlier in ('b'..letter).rev() {
            bases.push(format!("{base}{earlier}"));
        }
        bases.push(base.to_owned());
    }
    bases.push(sub.to_owned());
    bases
}

/// Each run's `rerun_of`: the latest earlier run of its lane with an id it reruns.
fn mark_reruns(runs: &mut [Run]) {
    for i in 0..runs.len() {
        let Some(run) = runs.get(i) else { continue };
        let (lane, start) = (run.lane, run.start);
        let found = rerun_bases(&run.sub)
            .into_iter()
            .find(|base| runs.get(..i).unwrap_or_default().iter().any(|earlier| earlier.lane == lane && earlier.start <= start && &earlier.sub == base));
        if let Some(run) = runs.get_mut(i) {
            run.rerun_of = found;
        }
    }
}

/// `HH:MM:SS` of a time, UTC.
fn clock(t: Timestamp) -> String {
    let (_, _, _, h, m, s) = t.civil();
    format!("{h:02}:{m:02}:{s:02}")
}

fn numbers(prefix: &str, list: &[u64]) -> String {
    list.iter().map(|n| format!("{prefix}{n}")).collect::<Vec<_>>().join(", ")
}

/// A run's tooltip, in colour: what it is, how it went and how long it took, when, what it
/// carried and did, what it reran, its notes.
fn tooltip(activity: &Activity, run: &Run) -> Vec<TipLine> {
    let mut lines = head_lines(run);
    lines.extend(did_lines(run));
    let logged = activity.agent(&run.instance).and_then(|a| a.subs.iter().find(|s| s.sub == run.sub && s.span.start == run.start));
    for note in logged.into_iter().flat_map(|s| [&s.start_note, &s.stop_note, &s.unfinished]).flatten() {
        lines.push(TipLine::plain(crate::titles::short(note, 160)));
    }
    lines.push(TipLine::plain(match run.lane {
        Lane::Gate => "click: its story in Details",
        Lane::Ready => "click: its ticket's story in Details",
    }));
    lines
}

/// What a run is, how it went and how long it took, and when.
fn head_lines(run: &Run) -> Vec<TipLine> {
    let what = match run.lane {
        Lane::Gate => "gate run ",
        Lane::Ready => "ready check ",
    };
    let kind = run.kind.as_ref().map(|k| format!(" · {k}")).unwrap_or_default();
    let head = TipLine::strong(format!("{what}{}", run.sub)).then(kind).then(format!(" · {}", run.name));
    let cause = match (&run.cause, &run.step) {
        (None, None) => String::new(),
        (cause, step) => format!(": {}", failure_label(cause.as_deref(), step.as_deref())),
    };
    let so_far = if run.end.is_none() { " so far" } else { "" };
    let result = TipLine::coloured(run.result.name(), run.result.colour())
        .then_coloured(cause, run.result.colour())
        .then(" · ")
        .then_strong(duration(run.duration_seconds))
        .then(so_far);
    let when = run.end.map_or_else(|| format!("{} → (running) UTC", clock(run.start)), |end| format!("{} → {} UTC", clock(run.start), clock(end)));
    vec![head, result, TipLine::plain(format!("{} {when}", run.start.to_string().get(..10).unwrap_or_default()))]
}

/// What a run carried, merged and sent back, and what it reran.
fn did_lines(run: &Run) -> Vec<TipLine> {
    let mut lines = Vec::new();
    if run.inferred {
        lines.push(TipLine::plain("read from its merges and send backs: no sub.start logged"));
    }
    let mut parts = Vec::new();
    if !run.prs.is_empty() {
        parts.push(format!("PR {}", numbers("#", &run.prs)));
    }
    if !run.tickets.is_empty() {
        parts.push(format!("tickets {}", numbers("#", &run.tickets)));
    }
    if !parts.is_empty() {
        lines.push(TipLine::plain(format!("carried {}", parts.join(" · "))));
    }
    for merge in &run.merges {
        let pr = merge.pr.map(|pr| format!(" (PR #{pr})")).unwrap_or_default();
        let commit = merge.commit.as_ref().map(|c| format!(" onto main {}", c.get(..8).unwrap_or(c))).unwrap_or_default();
        lines.push(TipLine::coloured(format!("merged #{}", merge.issue), PASSED).then(pr).then(commit));
    }
    if !run.sendbacks.is_empty() {
        lines.push(TipLine::coloured(format!("sent back {}", numbers("#", &run.sendbacks)), FAILED));
    }
    if let Some(of) = &run.rerun_of {
        lines.push(TipLine::plain("rerun of ").then_strong(of.clone()));
    }
    lines
}

/// A lane's figures over the runs of it that started in `[start, end]`; busy within `span`.
fn figures<'a>(runs: impl Iterator<Item = &'a Run>, span: &Intervals, last: Timestamp, start: Timestamp, end: Timestamp) -> Figures {
    let runs: Vec<&Run> = runs.collect();
    let busy = Intervals::of(runs.iter().map(|r| (r.start.0, r.end.unwrap_or(last).0))).intersect(span).measure();
    let started: Vec<&&Run> = runs.iter().filter(|r| start <= r.start && r.start <= end).collect();
    let count = |result: GateResult| started.iter().filter(|r| r.result == result).count();
    let (passed, failed) = (count(GateResult::Passed), count(GateResult::Failed));
    let with_result: Vec<i64> = started.iter().filter(|r| matches!(r.result, GateResult::Passed | GateResult::Failed)).map(|r| r.duration_seconds).collect();
    let mut causes: BTreeMap<String, usize> = BTreeMap::new();
    for run in started.iter().filter(|r| r.result == GateResult::Failed) {
        *causes.entry(failure_label(run.cause.as_deref(), run.step.as_deref())).or_default() += 1;
    }
    let mut causes: Vec<Cause> = causes.into_iter().map(|(cause, count)| Cause { cause, count }).collect();
    causes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.cause.cmp(&b.cause)));
    Figures {
        runs: started.len(),
        passed,
        failed,
        no_result: count(GateResult::NoResult),
        running: count(GateResult::Running),
        inferred: started.iter().filter(|r| r.inferred).count(),
        pass_rate: ratio(passed, passed + failed),
        longest_seconds: with_result.iter().copied().max(),
        median_seconds: median(with_result),
        busy_seconds: busy,
        busy: fraction(busy, span.measure()),
        reruns: started.iter().filter(|r| r.rerun_of.is_some()).count(),
        causes,
    }
}

/// One lane's runs in the hour from `at`.
fn hour_figures<'a>(runs: impl Iterator<Item = &'a Run> + Clone, at: i64, last: Timestamp) -> HourFigures {
    let next = at.saturating_add(HOUR);
    let started: Vec<&Run> = runs.clone().filter(|r| at <= r.start.0 && r.start.0 < next).collect();
    let count = |result: GateResult| started.iter().filter(|r| r.result == result).count();
    let (passed, failed) = (count(GateResult::Passed), count(GateResult::Failed));
    HourFigures {
        runs: started.len(),
        passed,
        failed,
        no_result: count(GateResult::NoResult) + count(GateResult::Running),
        pass_rate: ratio(passed, passed + failed),
        median_seconds: median(started.iter().filter(|r| matches!(r.result, GateResult::Passed | GateResult::Failed)).map(|r| r.duration_seconds).collect()),
        busy_seconds: Intervals::of(runs.map(|r| (r.start.0, r.end.unwrap_or(last).0))).measure_in(at, next),
    }
}

/// Each clock hour from the first run's (in the window) to the last's, cut to the last
/// [`MAX_HOURS`].
fn hourly(runs: &[Run], start: Timestamp, end: Timestamp, last: Timestamp) -> (Vec<GateHour>, bool) {
    let floor = |t: i64| t.div_euclid(HOUR).saturating_mul(HOUR);
    let lo = runs.iter().map(|r| r.start.0.max(start.0)).min();
    // A run's last second, so one ending on the hour adds no empty hour after it.
    let hi = runs.iter().map(|r| r.end.unwrap_or(last).0.saturating_sub(1).max(r.start.0).min(end.0)).max();
    let (Some(lo), Some(hi)) = (lo, hi) else { return (Vec::new(), false) };
    let (lo, hi) = (floor(lo), floor(hi.max(lo)));
    let cut = usize::try_from(hi.saturating_sub(lo) / HOUR).map_or(true, |n| n >= MAX_HOURS);
    let lo = if cut { hi.saturating_sub(HOUR.saturating_mul(i64::try_from(MAX_HOURS).unwrap_or(i64::MAX).saturating_sub(1))) } else { lo };
    let lane = |lane: Lane| runs.iter().filter(move |r| r.lane == lane);
    let mut hours = Vec::new();
    let mut at = lo;
    while at <= hi && hours.len() < MAX_HOURS {
        hours.push(GateHour { start: Timestamp(at), gate: hour_figures(lane(Lane::Gate), at, last), ready: hour_figures(lane(Lane::Ready), at, last) });
        let next = at.saturating_add(HOUR);
        if next == at {
            break;
        }
        at = next;
    }
    (hours, cut)
}

/// The most tickets waiting for merge at once within `[start, end]`.
fn longest_queue(tickets: &[Ticket], start: Timestamp, end: Timestamp) -> Queue {
    let waits: Vec<(u64, i64, i64)> = tickets
        .iter()
        .flat_map(|t| t.stretches.iter().filter(|s| s.phase == Phase::Waiting).map(move |s| (t.issue, s.start.0.max(start.0), s.end.0.min(end.0))))
        .filter(|(_, a, b)| a < b)
        .collect();
    // Ends before starts at a tie: a ticket merged as the next one waits is one in the queue.
    let mut edges: Vec<(i64, i8)> = waits.iter().flat_map(|&(_, a, b)| [(a, 1), (b, -1)]).collect();
    edges.sort_unstable();
    let (mut now, mut best, mut at) = (0_usize, 0_usize, None);
    for (t, step) in edges {
        if step < 0 {
            now = now.saturating_sub(1);
        } else {
            now += 1;
            if now > best {
                best = now;
                at = Some(t);
            }
        }
    }
    let tickets = at.map(|t| waits.iter().filter(|&&(_, a, b)| a <= t && t < b).map(|&(issue, ..)| issue).collect()).unwrap_or_default();
    Queue { count: best, at: at.map(Timestamp), tickets }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Gates, stats};
    use crate::tickets::tickets;
    use swarmception_model::{AgentFile, parse_file};

    const BASE: i64 = 1_793_404_800; // 2026-10-31T00:00:00Z

    fn at(h: i64, m: i64) -> Timestamp {
        Timestamp(BASE + h * HOUR + m * 60)
    }

    fn line(h: i64, m: i64, body: &str) -> String {
        format!("{{\"t\":\"{}\",{body}}}", at(h, m))
    }

    fn file(instance: &str, mut lines: Vec<String>) -> AgentFile {
        lines.sort_by_key(|l| l.get(6..26).map(str::to_owned));
        parse_file(instance, &lines.join("\n"))
    }

    fn gate(sub: &str, h: i64, m0: i64, m1: i64, start: &str, stop: Option<&str>) -> Vec<String> {
        let mut lines = vec![line(h, m0, &format!(r#""event":"sub.start","name":"gate","note":"{start}","sub":"{sub}""#))];
        lines.extend(stop.map(|note| line(h, m1, &format!(r#""event":"sub.stop","note":"{note}","sub":"{sub}""#))));
        lines
    }

    /// Worker-1 on #1 (PR 11) and #2 (PR 12), worker-2 on #3 (PR 13); the coordinator's gates:
    /// gate-11 passes and merges #1 (01:00–01:10), gate-12 fails on clippy (02:00–02:20),
    /// gate-12b reruns it and merges #2 (02:30–02:40), gate-13 sends #3 back with no stop
    /// logged and never finishes (the coordinator's session ends at 03:30); worker-1's ready
    /// checks pass and fail; worker-2's last one runs on.
    fn swarm() -> Vec<AgentFile> {
        let mut coordinator = vec![line(0, 0, r#""event":"session.start","machine":"mini","name":"coordinator","role":"coordinator""#)];
        coordinator.extend(gate("gate-11", 1, 0, 10, "local gate: #11 (#1); kind=smoke", Some("rc=0; kind=smoke; merged #11")));
        coordinator.push(line(1, 8, r#""event":"merge","issue":1,"mr":11,"sub":"gate-11""#));
        coordinator.extend(gate("gate-12", 2, 0, 20, "local gate: #12 (#2); kind=smoke", Some("rc=1; cause=clippy; kind=smoke")));
        coordinator.extend(gate("gate-12b", 2, 30, 40, "local gate: #12 (#2); kind=full", Some("merged #12")));
        coordinator.push(line(2, 38, r#""event":"merge","issue":2,"mr":12,"sub":"gate-12b""#));
        coordinator.extend(gate("gate-13", 3, 0, 0, "local gate: #13", None));
        coordinator.push(line(3, 5, r#""event":"sendback","issue":3,"mr":13,"note":"tests failed","sub":"gate-13""#));
        coordinator.push(line(3, 30, r#""event":"session.stop""#));
        let one = vec![
            line(0, 0, r#""event":"session.start","machine":"mini","name":"worker-1","role":"worker""#),
            line(0, 1, r#""event":"claim","issue":1,"mr":11"#),
            line(0, 2, r#""event":"work","issue":1"#),
            line(0, 40, r#""event":"sub.start","name":"ready","sub":"ready-aaa""#),
            line(0, 45, r#""event":"sub.stop","note":"rc=0","sub":"ready-aaa""#),
            line(0, 50, r#""event":"submit","issue":1,"mr":11"#),
            line(0, 51, r#""event":"claim","issue":2,"mr":12"#),
            line(0, 52, r#""event":"work","issue":2"#),
            line(1, 30, r#""event":"sub.start","name":"ready","sub":"ready-bbb""#),
            line(1, 35, r#""event":"sub.stop","note":"rc=1; cause=test","sub":"ready-bbb""#),
            line(1, 40, r#""event":"sub.start","name":"ready","sub":"ready-bbb-rerun""#),
            line(1, 42, r#""event":"sub.stop","note":"rc=0","sub":"ready-bbb-rerun""#),
            line(1, 50, r#""event":"submit","issue":2,"mr":12"#),
        ];
        let two = vec![
            line(0, 0, r#""event":"session.start","machine":"mini","name":"worker-2","role":"worker""#),
            line(0, 5, r#""event":"claim","issue":3,"mr":13"#),
            line(0, 6, r#""event":"work","issue":3"#),
            line(1, 0, r#""event":"submit","issue":3,"mr":13"#),
            line(3, 10, r#""event":"work","issue":3"#),
            line(3, 50, r#""event":"sub.start","name":"ready","sub":"ready-ccc""#),
            line(4, 0, r#""event":"beat""#),
        ];
        vec![file("mini-cc", coordinator), file("mini-aa", one), file("mini-bb", two)]
    }

    fn list(files: &[AgentFile], window: Window) -> GatesList {
        let activity = Activity::of(files);
        let tickets = tickets(&activity);
        let merges = [MainMerge { t: at(1, 9), commit: "1234567890abcdef".into(), pr: Some(11), subject: "Merge pull request #11".into() }];
        gates(&activity, &tickets, &merges, window)
    }

    fn sub_of(list: &GatesList, sub: &str) -> Run {
        list.runs.iter().find(|r| r.sub == sub).cloned().unwrap()
    }

    #[test]
    fn every_run_with_its_result_cargo_and_merges() {
        let list = list(&swarm(), Window::default());
        let subs: Vec<(&str, Lane, GateResult)> = list.runs.iter().map(|r| (r.sub.as_str(), r.lane, r.result)).collect();
        assert_eq!(
            subs,
            [
                ("ready-aaa", Lane::Ready, GateResult::Passed),
                ("gate-11", Lane::Gate, GateResult::Passed),
                ("ready-bbb", Lane::Ready, GateResult::Failed),
                ("ready-bbb-rerun", Lane::Ready, GateResult::Passed),
                ("gate-12", Lane::Gate, GateResult::Failed),
                ("gate-12b", Lane::Gate, GateResult::Passed),
                ("gate-13", Lane::Gate, GateResult::NoResult),
                ("ready-ccc", Lane::Ready, GateResult::Running),
            ]
        );
        let eleven = sub_of(&list, "gate-11");
        assert_eq!((eleven.start, eleven.end, eleven.duration_seconds), (at(1, 0), Some(at(1, 10)), 600));
        assert_eq!((eleven.prs.as_slice(), eleven.tickets.as_slice(), eleven.kind.as_deref()), (&[11][..], &[1][..], Some("smoke")));
        assert_eq!(eleven.merges, [RunMerge { t: at(1, 8), issue: 1, pr: Some(11), commit: Some("1234567890abcdef".into()) }]);
        assert_eq!((eleven.colour, eleven.subject.clone()), (PASSED, Subject::Gate { sub: "gate-11".into(), at: Some(at(1, 0)) }));
        assert_eq!((eleven.instance.as_str(), eleven.name.as_str()), ("mini-cc", "coordinator"));
        let twelve = sub_of(&list, "gate-12");
        assert_eq!((twelve.cause.as_deref(), twelve.colour), (Some("clippy"), FAILED));
        let rerun = sub_of(&list, "gate-12b");
        assert_eq!(
            (rerun.rerun_of.as_deref(), rerun.result, rerun.kind.as_deref()),
            (Some("gate-12"), GateResult::Passed, Some("full")),
            "no rc, but it merged"
        );
        let thirteen = sub_of(&list, "gate-13");
        assert_eq!((thirteen.end, thirteen.cause.as_deref(), thirteen.sendbacks.as_slice()), (Some(at(3, 30)), Some(NEVER_FINISHED), &[3][..]));
        assert_eq!((thirteen.tickets.as_slice(), thirteen.prs.as_slice()), (&[3][..], &[13][..]));
        let ready = sub_of(&list, "ready-bbb");
        assert_eq!((ready.tickets.as_slice(), ready.prs.as_slice(), ready.cause.as_deref()), (&[2][..], &[12][..], Some("test")));
        assert_eq!((ready.subject.clone(), ready.name.as_str()), (Subject::Ticket { issue: 2 }, "worker-1"));
        assert_eq!(sub_of(&list, "ready-bbb-rerun").rerun_of.as_deref(), Some("ready-bbb"));
        let running = sub_of(&list, "ready-ccc");
        assert_eq!((running.end, running.duration_seconds, running.colour), (None, 600, RUNNING), "to the last event");
        let tip: Vec<String> = eleven.tooltip.iter().map(TipLine::text).collect();
        assert_eq!(tip.get(..3).unwrap(), ["gate run gate-11 · smoke · coordinator", "passed · 10 min 00 s", "2026-10-31 01:00:00 → 01:10:00 UTC"]);
        assert!(tip.contains(&"merged #1 (PR #11) onto main 12345678".to_owned()), "{tip:?}");
        let tip: Vec<String> = rerun.tooltip.iter().map(TipLine::text).collect();
        assert!(tip.contains(&"rerun of gate-12".to_owned()), "{tip:?}");
    }

    #[test]
    fn figures_per_lane_agree_with_stats() {
        let files = swarm();
        let list = list(&files, Window::default());
        let gate = &list.gate;
        assert_eq!((gate.runs, gate.passed, gate.failed, gate.no_result, gate.running, gate.reruns), (4, 2, 1, 1, 0, 1));
        assert_eq!((gate.pass_rate, gate.median_seconds, gate.longest_seconds), (Some(2.0 / 3.0), Some(600), Some(1_200)));
        assert_eq!(gate.busy_seconds, 10 * 60 + 20 * 60 + 10 * 60 + 30 * 60, "gate-13 to the session's end");
        assert_eq!(gate.busy, Some(70.0 / 240.0));
        assert_eq!(gate.causes, [Cause { cause: "clippy".into(), count: 1 }]);
        let ready = &list.ready;
        assert_eq!((ready.runs, ready.passed, ready.failed, ready.running, ready.reruns), (4, 2, 1, 1, 1));
        assert_eq!(ready.causes, [Cause { cause: "test".into(), count: 1 }]);
        let activity = Activity::of(&files);
        let stats = stats(&activity, &tickets(&activity), &[], Window::default());
        assert_eq!((Gates::from(gate), Gates::from(ready)), (stats.gates, stats.ready), "as stats.get counts them");
    }

    /// #77: a failed `ready` check's stop names its cause and the `ci` step it failed at
    /// (swarm-core's `rc=<n>; cause=<cause>; step=<step>`); an older one, `rc=1` alone, reads as
    /// before: cause unknown, `unstated` in the figures.
    #[test]
    fn a_ready_stop_with_a_cause_and_step_and_one_without() {
        let worker = vec![
            line(0, 0, r#""event":"session.start","machine":"mini","name":"worker-1","role":"worker""#),
            line(0, 1, r#""event":"claim","issue":1,"mr":11"#),
            line(0, 2, r#""event":"work","issue":1"#),
            line(0, 10, r#""event":"sub.start","name":"ready","sub":"ready-aaa""#),
            line(0, 15, r#""event":"sub.stop","note":"rc=101; cause=build; step=clippy","sub":"ready-aaa""#),
            line(0, 20, r#""event":"sub.start","name":"ready","sub":"ready-bbb""#),
            line(0, 25, r#""event":"sub.stop","note":"rc=1","sub":"ready-bbb""#),
            line(0, 30, r#""event":"sub.start","name":"ready","sub":"ready-ccc""#),
            line(0, 35, r#""event":"sub.stop","note":"rc=100; cause=test; step=test","sub":"ready-ccc""#),
            line(0, 40, r#""event":"sub.start","name":"ready","sub":"ready-ddd""#),
            line(0, 45, r#""event":"sub.stop","note":"rc=0","sub":"ready-ddd""#),
        ];
        let files = [file("mini-aa", worker)];
        let list = list(&files, Window::default());
        let read = |sub: &str| {
            let run = sub_of(&list, sub);
            (run.result, run.cause, run.step)
        };
        let failed = |cause: Option<&str>, step: Option<&str>| (GateResult::Failed, cause.map(str::to_owned), step.map(str::to_owned));
        assert_eq!(read("ready-aaa"), failed(Some("build"), Some("clippy")));
        assert_eq!(read("ready-bbb"), failed(None, None), "an older note: cause unknown");
        assert_eq!(read("ready-ccc"), failed(Some("test"), Some("test")));
        assert_eq!(read("ready-ddd"), (GateResult::Passed, None, None));
        let causes: Vec<(&str, usize)> = list.ready.causes.iter().map(|c| (c.cause.as_str(), c.count)).collect();
        assert_eq!(causes, [("build/clippy", 1), ("test", 1), ("unstated", 1)], "the step where it says more");
        let tip = |sub: &str| sub_of(&list, sub).tooltip.get(1).map(TipLine::text).unwrap_or_default();
        assert_eq!(tip("ready-aaa"), "failed: build/clippy · 5 min 00 s");
        assert_eq!(tip("ready-bbb"), "failed · 5 min 00 s");
        let activity = Activity::of(&files);
        let stats = stats(&activity, &tickets(&activity), &[], Window::default());
        assert_eq!(Gates::from(&list.ready), stats.ready, "stats.get counts the same causes");
    }

    /// [`swarm`], and a second coordinator's two gates that logged no `sub.start`: gate-pr21
    /// merges #4 at 04:10, gate-pr22 sends #5 back on a conflict at 04:20.
    fn with_inferred() -> Vec<AgentFile> {
        let mut files = swarm();
        files.push(file(
            "mini-dd",
            vec![
                line(4, 0, r#""event":"session.start","machine":"mini","name":"coordinator-2","role":"coordinator""#),
                line(4, 10, r#""event":"merge","issue":4,"mr":21,"sub":"gate-pr21""#),
                line(4, 20, r#""event":"sendback","issue":5,"mr":22,"note":"cause=conflict","sub":"gate-pr22""#),
            ],
        ));
        files
    }

    #[test]
    fn gates_list_and_stats_get_count_one_definition_inferred_runs_said() {
        let files = with_inferred();
        let activity = Activity::of(&files);
        let tickets = tickets(&activity);
        let windows = [
            Window::default(),
            Window { start: Some(at(2, 0)), end: Some(at(4, 15)) },
            Window { start: Some(at(4, 15)), end: None },
            Window { start: Some(at(0, 30)), end: Some(at(1, 0)) },
            Window { start: Some(at(9, 0)), end: None },
        ];
        for window in windows {
            let list = gates(&activity, &tickets, &[], window);
            let stats = stats(&activity, &tickets, &[], window);
            assert_eq!((Gates::from(&list.gate), Gates::from(&list.ready)), (stats.gates.clone(), stats.ready.clone()), "{window:?}");
            let started = |lane: Lane| list.runs.iter().filter(|r| r.lane == lane && list.start <= r.start && r.start <= list.end).count();
            assert_eq!((stats.gates.runs, stats.ready.runs), (started(Lane::Gate), started(Lane::Ready)), "{window:?}: the runs it lists that started in it");
        }
        let whole = gates(&activity, &tickets, &[], Window::default());
        let gate = &whole.gate;
        assert_eq!((gate.runs, gate.inferred, gate.passed, gate.failed, gate.no_result), (6, 2, 3, 2, 1));
        assert_eq!(gate.causes, [Cause { cause: "clippy".into(), count: 1 }, Cause { cause: "conflict".into(), count: 1 }]);
        let inferred: Vec<(&str, GateResult)> = whole.runs.iter().filter(|r| r.inferred).map(|r| (r.sub.as_str(), r.result)).collect();
        assert_eq!(inferred, [("gate-pr21", GateResult::Passed), ("gate-pr22", GateResult::Failed)]);
        let late = stats(&activity, &tickets, &[], Window { start: Some(at(4, 15)), end: None });
        assert_eq!((late.gates.runs, late.gates.inferred, late.gates.failed, late.gates.pass_rate), (1, 1, 1, Some(0.0)));
    }

    #[test]
    fn hours_and_the_longest_queue() {
        let list = list(&swarm(), Window::default());
        let hours: Vec<(Timestamp, usize, usize, i64)> = list.hours.iter().map(|h| (h.start, h.gate.runs, h.ready.runs, h.gate.busy_seconds)).collect();
        assert_eq!(
            hours,
            [(at(0, 0), 0, 1, 0), (at(1, 0), 1, 2, 600), (at(2, 0), 2, 0, 1_800), (at(3, 0), 1, 1, 1_800)],
            "the last run runs to 04:00, the last event"
        );
        let two = &list.hours[2].gate;
        assert_eq!((two.passed, two.failed, two.pass_rate, two.median_seconds), (1, 1, Some(0.5), Some(900)));
        // #1 waits 00:50–01:08, #3 01:00–03:05, #2 01:50–02:38: two at once, from 01:00.
        assert_eq!(list.longest_queue, Queue { count: 2, at: Some(at(1, 0)), tickets: vec![1, 3] });
    }

    #[test]
    fn a_window_keeps_the_runs_it_overlaps_and_counts_those_it_started() {
        let list = list(&swarm(), Window { start: Some(at(2, 10)), end: Some(at(2, 35)) });
        let subs: Vec<&str> = list.runs.iter().map(|r| r.sub.as_str()).collect();
        assert_eq!(subs, ["gate-12", "gate-12b"]);
        assert_eq!((list.gate.runs, list.gate.passed, list.gate.failed), (1, 1, 0), "gate-12 started before it");
        assert_eq!(list.gate.busy_seconds, 10 * 60 + 5 * 60);
        assert_eq!(list.span_seconds, 25 * 60);
        assert_eq!(list.hours.len(), 1);
        assert_eq!(list.longest_queue.count, 2, "#2 and #3");
    }

    #[test]
    fn reruns_read_from_ids() {
        assert_eq!(rerun_bases("gate-43c"), ["gate-43b", "gate-43", "gate-43c"]);
        assert_eq!(rerun_bases("gate-43-rerun"), ["gate-43", "gate-43-rerun"]);
        assert_eq!(rerun_bases("gate-full-8e3e2ce"), ["gate-full-8e3e2ce"], "a commit's letters after a letter");
        assert_eq!(rerun_bases("gate-12a"), ["gate-12a"], "a is a first run");
        assert_eq!(rerun_bases("-rerun"), ["-rerun"]);
        assert_eq!(rerun_bases(""), [""]);
        // The same id twice: the second reruns the first.
        let mut coordinator = vec![line(0, 0, r#""event":"session.start","machine":"mini","name":"coordinator","role":"coordinator""#)];
        coordinator.extend(gate("gate-7", 1, 0, 5, "", Some("rc=1; cause=conflict")));
        coordinator.extend(gate("gate-7", 2, 0, 5, "", Some("rc=0")));
        coordinator.extend(gate("gate-8c", 3, 0, 5, "", Some("rc=0")));
        let list = list(&[file("mini-cc", coordinator)], Window::default());
        let reruns: Vec<Option<&str>> = list.runs.iter().map(|r| r.rerun_of.as_deref()).collect();
        assert_eq!(reruns, [None, Some("gate-7"), None], "gate-8c: no gate-8 or gate-8b ran");
    }

    #[test]
    fn nothing_and_hostile_windows_never_panic() {
        let empty = list(&[], Window::default());
        assert!(empty.runs.is_empty() && empty.hours.is_empty());
        assert_eq!((empty.gate.pass_rate, empty.gate.busy, empty.longest_queue.count), (None, None, 0));
        let wide = list(&swarm(), Window { start: Some(Timestamp(i64::MIN)), end: Some(Timestamp(i64::MAX)) });
        assert_eq!(wide.runs.len(), 8);
        assert_eq!(wide.hours.len(), 4);
        let backward = list(&swarm(), Window { start: Some(at(3, 0)), end: Some(at(1, 0)) });
        assert_eq!(backward.span_seconds, 0);
        let edge = list(&swarm(), Window { start: Some(Timestamp(i64::MAX)), end: None });
        assert!(edge.runs.is_empty());
    }
}
