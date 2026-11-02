//! How each worker did, and each model × effort (#67, G9): every worker's session (`session.start
//! --model --effort`) and what it did over a window, rolled up by the model and effort it ran
//! as, so the owner can compare them. What `workers.compare` returns and the Workers pane draws.
//!
//! - **A worker** is one agent instance that isn't a coordinator: one `session.start` and what
//!   followed, its model and effort as it logged them (none: `unknown`). A worker restarted is a
//!   new instance and a row of its own. One is listed when it was on shift in the window or did
//!   anything counted there.
//! - **On shift**, as `stats.get` reads it ([`crate::stats`]): its sessions less their pauses,
//!   within the window. **Active**: that less the swarm's quiet gaps (over [`QUIET`] with no
//!   event of anyone's), as `stats.get`'s, and less the worker's own **idle** time: every gap of
//!   over [`QUIET`] between its own events while it held no ticket claimed, working or sent back
//!   to it (a worker
//!   left on shift with nothing to do, or waiting on its last PR, isn't working; one on a ticket
//!   for an hour without a word is).
//! - **Claimed**: the tickets it claimed in the window (its own `claim`, not a gate's).
//!   **Landed**: the tickets merged in the window that it held last (as `stats.get`'s workers:
//!   the ticket's last claim or work). **Landed per active hour**: landed ÷ its active hours.
//! - **Claim → merge**: per ticket landed, its first claim or work of it to the merge; **working**:
//!   the time the ticket was *working* by it, summed. Medians over its landed tickets.
//! - **Send backs**: the `sendback`s in the window of a ticket it held before the gate (its last
//!   claim, work or submit, as the factory's moments name them), ÷ its landed; their **causes**
//!   from the note (`cause=conflict: …` is `conflict`; `build: …` is `build`; else `unstated`).
//! - **Ready checks**: its own `ready` gates that started in the window; the pass rate passed ÷
//!   (passed + failed).
//! - **Abandoned**: the tickets whose stretch its session's end cut off (#28), in the window,
//!   whether or not someone took them up after.
//! - **Samples**: every rate and median says how many it rests on (landed tickets, send backs'
//!   landed, ready checks with a result) and is **thin** below [`THIN`]: shown, marked, never
//!   hidden.
//! - **A model × effort**: its workers' sums; its rates and medians over all their samples
//!   pooled (landed ÷ active hours summed, not a mean of the workers' rates).
//!
//! The window defaults to the whole run. Pure: no I/O, no clock.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{Kind, Role, Timestamp};

use crate::activity::{Activity, Agent};
use crate::details::Subject;
use crate::factory::note_field;
use crate::stats::{Cause, Intervals, QUIET, Window, is_ready, median, quiet_gaps, shift};
use crate::tickets::{Phase, Ticket};

/// A rate or median resting on fewer samples than this is thin.
pub const THIN: usize = 3;
const HOUR: f64 = 3_600.0;

/// A figure and how many samples it rests on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    /// None when there is nothing to divide by or take the median of.
    pub value: Option<f64>,
    pub samples: usize,
    /// Fewer than [`THIN`] samples: show it, marked.
    pub thin: bool,
}

impl Measure {
    fn of(value: Option<f64>, samples: usize) -> Self {
        Self { value, samples, thin: samples < THIN }
    }

    /// The median of `seconds`, its samples their count.
    fn median(seconds: Vec<i64>) -> Self {
        let samples = seconds.len();
        Self::of(median(seconds).map(|s| s as f64), samples)
    }
}

/// What one worker, or one model × effort's workers, did in the window (the module's docs
/// define each).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Figures {
    /// The worker sessions it rests on: 1 for a worker.
    pub workers: usize,
    pub on_shift_seconds: i64,
    pub active_seconds: i64,
    pub claimed: usize,
    pub landed: usize,
    /// Landed ÷ the active hours; its samples, the tickets landed.
    pub landed_per_active_hour: Measure,
    /// Medians over the tickets landed, in seconds.
    pub claim_to_merge_seconds: Measure,
    pub working_seconds: Measure,
    pub sendbacks: usize,
    /// Send backs ÷ landed; its samples, the tickets landed.
    pub sendbacks_per_landed: Measure,
    /// The send backs by cause, most first.
    pub sendback_causes: Vec<Cause>,
    /// Its `ready` checks that started in the window, and how they went.
    pub ready_runs: usize,
    pub ready_passed: usize,
    pub ready_failed: usize,
    /// Passed ÷ (passed + failed); its samples, those with a result.
    pub ready_pass_rate: Measure,
    pub abandoned: usize,
}

/// One worker session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Worker {
    pub instance: String,
    pub machine: String,
    pub name: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Its first session's start and its last's end.
    pub start: Timestamp,
    pub end: Timestamp,
    /// Its last session goes on (no stop logged).
    pub open: bool,
    pub figures: Figures,
    /// The tickets it claimed, worked on, landed or abandoned in the window: whose bars a hover
    /// on it lights.
    pub tickets: Vec<u64>,
    /// What the details pane tells of it when it is clicked: its session.
    pub subject: Subject,
}

/// One model × effort, and its workers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `opus 5.5 · high`: the model as a person names it ([`model_name`]), and the effort;
    /// `unknown` for either not logged.
    pub label: String,
    pub figures: Figures,
    /// By their first session's start.
    pub workers: Vec<Worker>,
}

/// The workers compared over a window, by model × effort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    /// The window, its ends included.
    pub start: Timestamp,
    pub end: Timestamp,
    pub span_seconds: i64,
    /// A gap between events longer than this, in seconds, is quiet.
    pub quiet_after_seconds: i64,
    /// A figure on fewer samples than this is thin.
    pub thin_below: usize,
    /// By model, then effort, most effort first; unknown last.
    pub groups: Vec<Group>,
    /// Every worker's together.
    pub all: Figures,
}

/// The samples of one worker or group, before they become [`Figures`].
#[derive(Debug, Default)]
struct Tally {
    workers: usize,
    on_shift: i64,
    active: i64,
    claimed: usize,
    landed: usize,
    claim_to_merge: Vec<i64>,
    working: Vec<i64>,
    sendbacks: BTreeMap<String, usize>,
    ready_runs: usize,
    ready_passed: usize,
    ready_failed: usize,
    abandoned: usize,
}

impl Tally {
    fn add(&mut self, other: &Self) {
        self.workers += other.workers;
        self.on_shift = self.on_shift.saturating_add(other.on_shift);
        self.active = self.active.saturating_add(other.active);
        self.claimed += other.claimed;
        self.landed += other.landed;
        self.claim_to_merge.extend(&other.claim_to_merge);
        self.working.extend(&other.working);
        for (cause, count) in &other.sendbacks {
            *self.sendbacks.entry(cause.clone()).or_default() += count;
        }
        self.ready_runs += other.ready_runs;
        self.ready_passed += other.ready_passed;
        self.ready_failed += other.ready_failed;
        self.abandoned += other.abandoned;
    }

    fn figures(&self) -> Figures {
        let sendbacks: usize = self.sendbacks.values().sum();
        let mut causes: Vec<Cause> = self.sendbacks.iter().map(|(cause, &count)| Cause { cause: cause.clone(), count }).collect();
        causes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.cause.cmp(&b.cause)));
        let with_result = self.ready_passed + self.ready_failed;
        Figures {
            workers: self.workers,
            on_shift_seconds: self.on_shift,
            active_seconds: self.active,
            claimed: self.claimed,
            landed: self.landed,
            landed_per_active_hour: Measure::of((self.active > 0).then(|| self.landed as f64 * HOUR / self.active as f64), self.landed),
            claim_to_merge_seconds: Measure::median(self.claim_to_merge.clone()),
            working_seconds: Measure::median(self.working.clone()),
            sendbacks,
            sendbacks_per_landed: Measure::of((self.landed > 0).then(|| sendbacks as f64 / self.landed as f64), self.landed),
            sendback_causes: causes,
            ready_runs: self.ready_runs,
            ready_passed: self.ready_passed,
            ready_failed: self.ready_failed,
            ready_pass_rate: Measure::of((with_result > 0).then(|| self.ready_passed as f64 / with_result as f64), with_result),
            abandoned: self.abandoned,
        }
    }
}

/// The workers of `activity` compared over `window`, from its `tickets`.
#[must_use]
pub fn compare(activity: &Activity, tickets: &[Ticket], window: Window) -> Comparison {
    let (first, last) = activity.extent.unwrap_or_default();
    let start = window.start.unwrap_or(first);
    let end = window.end.unwrap_or(last).max(start);
    let span = Intervals::span(start.0, end.0);
    let quiet = quiet_gaps(&activity.times);
    let within = |t: Timestamp| start <= t && t <= end;
    let senders = sendback_holders(tickets);
    let mut groups: BTreeMap<GroupKey, (Tally, Vec<Worker>)> = BTreeMap::new();
    let mut all = Tally::default();
    for agent in activity.agents.iter().filter(|a| a.role != Role::Coordinator) {
        let on_shift = Intervals::of(shift(agent)).intersect(&span);
        let active = on_shift.subtract(&quiet).subtract(&idle(activity, agent, tickets));
        let (tally, worked) = tally(activity, tickets, &senders, agent, &within);
        let tally = Tally { on_shift: on_shift.measure(), active: active.measure(), ..tally };
        let counted = tally.claimed + tally.landed + tally.ready_runs + tally.abandoned + tally.sendbacks.len();
        if tally.on_shift == 0 && counted == 0 {
            continue;
        }
        all.add(&tally);
        let key = (rank(agent.model.as_deref(), &[]), agent.model.clone(), rank(agent.effort.as_deref(), &EFFORTS), agent.effort.clone());
        let entry = groups.entry(key).or_default();
        entry.0.add(&tally);
        entry.1.push(worker(agent, tally.figures(), worked, last));
    }
    let groups = groups
        .into_iter()
        .map(|((_, model, _, effort), (tally, mut workers))| {
            workers.sort_by(|a, b| (a.start, &a.name, &a.instance).cmp(&(b.start, &b.name, &b.instance)));
            Group { label: label(model.as_deref(), effort.as_deref()), model, effort, figures: tally.figures(), workers }
        })
        .collect();
    Comparison { start, end, span_seconds: end.0.saturating_sub(start.0), quiet_after_seconds: QUIET, thin_below: THIN, groups, all: all.figures() }
}

/// Where a model × effort sorts, and what it is: the model's rank and name, the effort's.
type GroupKey = (u8, Option<String>, u8, Option<String>);

/// The efforts, most first.
const EFFORTS: [&str; 5] = ["max", "xhigh", "high", "medium", "low"];

/// Where a model or effort sorts: one of `known` in its order, then any other, then none.
fn rank(value: Option<&str>, known: &[&str]) -> u8 {
    match value {
        Some(v) => known.iter().position(|k| k.eq_ignore_ascii_case(v)).unwrap_or(known.len()).min(usize::from(u8::MAX - 1)) as u8,
        None => u8::MAX,
    }
}

/// `opus 5.5 · high`.
fn label(model: Option<&str>, effort: Option<&str>) -> String {
    let model = model.map_or_else(|| "unknown model".to_owned(), model_name);
    format!("{model} · {}", effort.unwrap_or("unknown effort"))
}

/// A model as a person names it: `claude-opus-5-5` is `opus 5.5`, `claude-sonnet-4-5-20250929`
/// `sonnet 4.5 20250929`; one it doesn't read so, as it is.
#[must_use]
pub fn model_name(model: &str) -> String {
    let short = model.strip_prefix("claude-").unwrap_or(model);
    let mut parts = short.split('-');
    let family = parts.next().unwrap_or_default();
    let rest: Vec<&str> = parts.collect();
    let version: Vec<&str> = rest.iter().copied().take_while(|p| !p.is_empty() && p.len() <= 2 && p.chars().all(|c| c.is_ascii_digit())).collect();
    if family.is_empty() || version.is_empty() {
        return short.to_owned();
    }
    let tail: Vec<&str> = rest.iter().copied().skip(version.len()).collect();
    let mut name = format!("{family} {}", version.join("."));
    if !tail.is_empty() {
        name.push(' ');
        name.push_str(&tail.join("-"));
    }
    name
}

fn worker(agent: &Agent, figures: Figures, tickets: Vec<u64>, last: Timestamp) -> Worker {
    let start = agent.sessions.first().map_or(last, |s| s.span.start);
    let end = agent.sessions.last().map_or(last, |s| s.span.end);
    Worker {
        instance: agent.instance.clone(),
        machine: agent.machine.clone(),
        name: agent.name.clone(),
        model: agent.model.clone(),
        effort: agent.effort.clone(),
        start,
        end,
        open: agent.sessions.last().is_some_and(|s| s.span.open),
        figures,
        tickets,
        subject: Subject::Session { instance: agent.instance.clone(), at: Some(start) },
    }
}

/// A worker's idle time: every gap of over [`QUIET`] between its own events, less the time it
/// held a ticket claimed or working, or one was sent back to it.
fn idle(activity: &Activity, agent: &Agent, tickets: &[Ticket]) -> Intervals {
    let me = agent.instance.as_str();
    let mut times: Vec<i64> = agent.sessions.iter().flat_map(|s| [s.span.start.0, s.span.end.0]).collect();
    times.extend(agent.sessions.iter().flat_map(|s| &s.pauses).flat_map(|p| [p.start.0, p.end.0]));
    times.extend(activity.issue_events.iter().filter(|e| e.instance == me).map(|e| e.t.0));
    times.extend(agent.subs.iter().flat_map(|s| [s.span.start.0, s.span.end.0]));
    times.extend(agent.marks.iter().map(|m| m.t.0));
    times.sort_unstable();
    let gaps = Intervals::of(times.windows(2).filter_map(|w| match w {
        [a, b] if b.saturating_sub(*a) > QUIET => Some((*a, *b)),
        _ => None,
    }));
    let mut busy = Vec::new();
    for ticket in tickets {
        for (i, stretch) in ticket.stretches.iter().enumerate() {
            let mine = match stretch.phase {
                Phase::Claimed | Phase::Working => stretch.by == me,
                Phase::SentBack => held_before(ticket, i).is_some_and(|by| by == me),
                Phase::Waiting | Phase::Stopped => false,
            };
            if mine {
                busy.push((stretch.start.0, stretch.end.0));
            }
        }
    }
    gaps.subtract(&Intervals::of(busy))
}

/// Who held `ticket` before its `i`th stretch: its last claimed, working or waiting stretch's
/// instance, as the factory's moments name whom a send back is for.
fn held_before(ticket: &Ticket, i: usize) -> Option<&str> {
    let before = ticket.stretches.get(..i).unwrap_or_default();
    before.iter().rev().find(|s| matches!(s.phase, Phase::Claimed | Phase::Working | Phase::Waiting)).map(|s| s.by.as_str())
}

/// A send back: its ticket and time, the instance that held the ticket before the gate, and its
/// note.
struct SendBack {
    issue: u64,
    t: Timestamp,
    holder: Option<String>,
    note: Option<String>,
}

/// Each send back, with the instance that held the ticket before the gate: its last claimed,
/// working or waiting stretch's.
fn sendback_holders(tickets: &[Ticket]) -> Vec<SendBack> {
    let mut list = Vec::new();
    for ticket in tickets {
        for (i, stretch) in ticket.stretches.iter().enumerate().filter(|(_, s)| s.phase == Phase::SentBack) {
            list.push(SendBack { issue: ticket.issue, t: stretch.start, holder: held_before(ticket, i).map(str::to_owned), note: stretch.note.clone() });
        }
    }
    list
}

/// A send back's cause: its note's `cause=` (to a `:`, `;` or space), else the word before a
/// leading `:`, else `unstated`.
#[must_use]
pub fn sendback_cause(note: Option<&str>) -> String {
    let word = |text: &str| text.trim().chars().take_while(|c| !matches!(c, ':' | ';') && !c.is_whitespace()).take(32).collect::<String>();
    let Some(note) = note else { return "unstated".into() };
    let cause = note_field(note, "cause").map(|c| word(&c)).or_else(|| {
        let (head, _) = note.split_once(':')?;
        (!head.trim().is_empty() && !head.trim().contains(char::is_whitespace)).then(|| word(head))
    });
    cause.filter(|c| !c.is_empty()).unwrap_or_else(|| "unstated".into())
}

/// One worker's counts and samples in the window, and the tickets it touched there.
fn tally(activity: &Activity, tickets: &[Ticket], senders: &[SendBack], agent: &Agent, within: &impl Fn(Timestamp) -> bool) -> (Tally, Vec<u64>) {
    let me = agent.instance.as_str();
    let mut touched = BTreeSet::new();
    let mut claimed = BTreeSet::new();
    for event in activity.issue_events.iter().filter(|e| e.instance == me && e.sub.is_none() && within(e.t)) {
        if matches!(event.kind, Kind::Claim { .. } | Kind::Work { .. } | Kind::Submit { .. }) {
            touched.insert(event.issue);
        }
        if matches!(event.kind, Kind::Claim { .. }) {
            claimed.insert(event.issue);
        }
    }
    let mut tally = Tally { workers: 1, claimed: claimed.len(), ..Tally::default() };
    landed(&mut tally, &mut touched, activity, tickets, me, within);
    for sendback in senders.iter().filter(|b| b.holder.as_deref() == Some(me) && within(b.t)) {
        touched.insert(sendback.issue);
        *tally.sendbacks.entry(sendback_cause(sendback.note.as_deref())).or_default() += 1;
    }
    for sub in agent.subs.iter().filter(|s| is_ready(s) && within(s.span.start)) {
        tally.ready_runs += 1;
        match sub.rc {
            Some(0) => tally.ready_passed += 1,
            Some(_) => tally.ready_failed += 1,
            None => {}
        }
    }
    let abandoned: BTreeSet<u64> =
        tickets.iter().filter(|t| t.stretches.iter().any(|s| s.abandoned.as_ref().is_some_and(|a| a.holder == me && within(a.t)))).map(|t| t.issue).collect();
    tally.abandoned = abandoned.len();
    touched.extend(abandoned);
    (tally, touched.into_iter().collect())
}

/// The tickets `me` landed in the window: counted, and each one's claim → merge and working time.
fn landed(tally: &mut Tally, touched: &mut BTreeSet<u64>, activity: &Activity, tickets: &[Ticket], me: &str, within: &impl Fn(Timestamp) -> bool) {
    for ticket in tickets.iter().filter(|t| t.worker.as_deref() == Some(me)) {
        let Some(merged) = ticket.merged.as_ref().filter(|m| within(m.t)) else { continue };
        tally.landed += 1;
        touched.insert(ticket.issue);
        let first = activity
            .issue_events
            .iter()
            .find(|e| e.instance == me && e.sub.is_none() && e.issue == ticket.issue && matches!(e.kind, Kind::Claim { .. } | Kind::Work { .. }))
            .map(|e| e.t);
        if let Some(from) = first.or(ticket.claimed).filter(|&from| from <= merged.t) {
            tally.claim_to_merge.push(merged.t.0.saturating_sub(from.0));
        }
        let working = ticket
            .stretches
            .iter()
            .filter(|s| s.phase == Phase::Working && s.by == me)
            .map(|s| s.end.0.saturating_sub(s.start.0).max(0))
            .fold(0, i64::saturating_add);
        if working > 0 {
            tally.working.push(working);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tickets::tickets;
    use swarmception_model::{AgentFile, parse_file};

    fn file(instance: &str, lines: &[&str]) -> AgentFile {
        parse_file(instance, &lines.join("\n"))
    }

    /// Two opus high workers and one sonnet low: alder lands #1 (claim 09:00, merge 09:40) and
    /// #2 (sent back on a conflict once, merged 10:30); birch lands #3; cedar, at low effort,
    /// claims #4, fails a ready check and leaves #4 when its session ends.
    fn swarm() -> Vec<AgentFile> {
        let alder = [
            r#"{"t":"2026-10-31T09:00:00Z","event":"session.start","machine":"mini","name":"alder","role":"worker","model":"claude-opus-5-5","effort":"high"}"#,
            r#"{"t":"2026-10-31T09:00:00Z","event":"claim","issue":1,"mr":11}"#,
            r#"{"t":"2026-10-31T09:01:00Z","event":"work","issue":1}"#,
            r#"{"t":"2026-10-31T09:10:00Z","event":"sub.start","name":"ready","sub":"ready-a1"}"#,
            r#"{"t":"2026-10-31T09:14:00Z","event":"sub.stop","note":"rc=0","sub":"ready-a1"}"#,
            r#"{"t":"2026-10-31T09:21:00Z","event":"submit","issue":1,"mr":11}"#,
            r#"{"t":"2026-10-31T09:22:00Z","event":"claim","issue":2,"mr":12}"#,
            r#"{"t":"2026-10-31T09:23:00Z","event":"work","issue":2}"#,
            r#"{"t":"2026-10-31T09:33:00Z","event":"submit","issue":2,"mr":12}"#,
            r#"{"t":"2026-10-31T09:48:00Z","event":"work","issue":2}"#,
            r#"{"t":"2026-10-31T10:00:00Z","event":"submit","issue":2,"mr":12}"#,
            r#"{"t":"2026-10-31T10:40:00Z","event":"session.stop"}"#,
        ];
        let birch = [
            r#"{"t":"2026-10-31T09:00:00Z","event":"session.start","machine":"mini","name":"birch","role":"worker","model":"claude-opus-5-5","effort":"high"}"#,
            r#"{"t":"2026-10-31T09:05:00Z","event":"claim","issue":3,"mr":13}"#,
            r#"{"t":"2026-10-31T09:06:00Z","event":"work","issue":3}"#,
            r#"{"t":"2026-10-31T09:36:00Z","event":"submit","issue":3,"mr":13}"#,
            r#"{"t":"2026-10-31T10:00:00Z","event":"session.stop"}"#,
        ];
        let cedar = [
            r#"{"t":"2026-10-31T09:00:00Z","event":"session.start","machine":"mini","name":"cedar","role":"worker","model":"claude-sonnet-5","effort":"low"}"#,
            r#"{"t":"2026-10-31T09:10:00Z","event":"claim","issue":4,"mr":14}"#,
            r#"{"t":"2026-10-31T09:11:00Z","event":"work","issue":4}"#,
            r#"{"t":"2026-10-31T09:20:00Z","event":"sub.start","name":"ready","sub":"ready-c1"}"#,
            r#"{"t":"2026-10-31T09:25:00Z","event":"sub.stop","note":"rc=1; cause=test","sub":"ready-c1"}"#,
            r#"{"t":"2026-10-31T09:30:00Z","event":"session.stop"}"#,
        ];
        let coordinator = [
            r#"{"t":"2026-10-31T08:59:00Z","event":"session.start","machine":"mini","name":"coordinator","role":"coordinator","model":"claude-opus-5-5","effort":"high"}"#,
            r#"{"t":"2026-10-31T09:35:00Z","event":"sendback","issue":2,"mr":12,"note":"cause=conflict: src/lib.rs (with #1)"}"#,
            r#"{"t":"2026-10-31T09:40:00Z","event":"merge","issue":1,"mr":11}"#,
            r#"{"t":"2026-10-31T09:45:00Z","event":"merge","issue":3,"mr":13}"#,
            r#"{"t":"2026-10-31T10:15:00Z","event":"beat"}"#,
            r#"{"t":"2026-10-31T10:30:00Z","event":"merge","issue":2,"mr":12}"#,
            r#"{"t":"2026-10-31T10:45:00Z","event":"session.stop"}"#,
        ];
        vec![file("mini-aa", &alder), file("mini-bb", &birch), file("mini-cc", &cedar), file("mini-dd", &coordinator)]
    }

    fn run(window: Window) -> Comparison {
        let activity = Activity::of(&swarm());
        compare(&activity, &tickets(&activity), window)
    }

    fn at(text: &str) -> Timestamp {
        Timestamp::parse(text).unwrap()
    }

    #[test]
    fn workers_by_model_and_effort() {
        let all = run(Window::default());
        let labels: Vec<&str> = all.groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["opus 5.5 · high", "sonnet 5 · low"], "no coordinator");
        let opus = &all.groups[0];
        let names: Vec<&str> = opus.workers.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["alder", "birch"]);
        let alder = &opus.workers[0].figures;
        assert_eq!((alder.claimed, alder.landed, alder.sendbacks, alder.abandoned), (2, 2, 1, 0));
        assert_eq!(alder.on_shift_seconds, 100 * 60);
        assert_eq!(alder.active_seconds, 60 * 60, "idle from its last submit at 10:00 to its stop at 10:40");
        assert_eq!(alder.landed_per_active_hour, Measure { value: Some(2.0), samples: 2, thin: true });
        // #1: 09:00 → 09:40; #2: 09:22 → 10:30.
        assert_eq!(alder.claim_to_merge_seconds, Measure { value: Some(((40 + 68) * 60 / 2) as f64), samples: 2, thin: true });
        // #1 worked 20 min; #2 10 + 12 min.
        assert_eq!(alder.working_seconds.value, Some(21.0 * 60.0));
        assert_eq!(alder.sendbacks_per_landed.value, Some(0.5));
        assert_eq!(alder.sendback_causes, [Cause { cause: "conflict".into(), count: 1 }]);
        assert_eq!((alder.ready_runs, alder.ready_pass_rate), (1, Measure { value: Some(1.0), samples: 1, thin: true }));
        assert_eq!(opus.workers[0].tickets, [1, 2]);
        assert_eq!(opus.workers[0].subject, Subject::Session { instance: "mini-aa".into(), at: Some(at("2026-10-31T09:00:00Z")) });
        // Birch: silent 30 minutes on #3 (working: active), then 24 waiting with nothing (idle).
        assert_eq!(opus.workers[1].figures.active_seconds, 36 * 60);
        // Pooled, not a mean of rates: 3 landed in 60 + 36 active minutes.
        let pooled = &opus.figures;
        assert_eq!((pooled.workers, pooled.landed, pooled.claimed, pooled.active_seconds), (2, 3, 3, 96 * 60));
        assert_eq!(pooled.landed_per_active_hour, Measure { value: Some(3.0 * 60.0 / 96.0), samples: 3, thin: false });
        assert_eq!(pooled.claim_to_merge_seconds.samples, 3);
        assert!(!pooled.claim_to_merge_seconds.thin);
        let sonnet = &all.groups[1].workers[0].figures;
        assert_eq!((sonnet.claimed, sonnet.landed, sonnet.abandoned), (1, 0, 1), "#4 left when cedar's session ended");
        assert_eq!(sonnet.landed_per_active_hour, Measure { value: Some(0.0), samples: 0, thin: true });
        assert_eq!(sonnet.claim_to_merge_seconds, Measure { value: None, samples: 0, thin: true });
        assert_eq!(sonnet.sendbacks_per_landed.value, None, "nothing landed to divide by");
        assert_eq!((sonnet.ready_failed, sonnet.ready_pass_rate.value), (1, Some(0.0)));
        assert_eq!((all.all.workers, all.all.landed, all.all.abandoned), (3, 3, 1));
        assert_eq!(all.thin_below, THIN);
    }

    #[test]
    fn a_window_counts_what_happened_in_it() {
        let early = run(Window { start: None, end: Some(at("2026-10-31T09:40:00Z")) });
        let alder = &early.groups[0].workers[0].figures;
        assert_eq!((alder.landed, alder.sendbacks, alder.on_shift_seconds, alder.active_seconds), (1, 1, 40 * 60, 40 * 60), "#1's merge at the end is in it");
        let late = run(Window { start: Some(at("2026-10-31T10:05:00Z")), end: None });
        let names: Vec<&str> = late.groups.iter().flat_map(|g| &g.workers).map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["alder"], "only alder was on shift or landed after 10:05");
        assert_eq!(late.groups[0].workers[0].figures.landed, 1);
        assert_eq!(late.groups[0].workers[0].figures.claimed, 0);
    }

    #[test]
    fn models_read_as_a_person_names_them() {
        assert_eq!(model_name("claude-opus-5-5"), "opus 5.5");
        assert_eq!(model_name("claude-sonnet-4-5-20250929"), "sonnet 4.5 20250929");
        assert_eq!(model_name("claude-haiku-4"), "haiku 4");
        assert_eq!(model_name("gpt-x"), "gpt-x");
        assert_eq!(model_name("claude-"), "");
        assert_eq!(model_name("-5"), "-5");
    }

    #[test]
    fn unknown_models_and_efforts_sort_last() {
        assert_eq!(label(None, None), "unknown model · unknown effort");
        assert!(rank(Some("high"), &EFFORTS) < rank(Some("low"), &EFFORTS));
        assert!(rank(Some("low"), &EFFORTS) < rank(Some("turbo"), &EFFORTS));
        assert!(rank(Some("turbo"), &EFFORTS) < rank(None, &EFFORTS));
        assert_eq!(rank(Some("XHIGH"), &EFFORTS), 1);
    }

    #[test]
    fn sendback_causes_read_from_notes() {
        assert_eq!(sendback_cause(Some("cause=conflict: crates/a.rs (with #78)")), "conflict");
        assert_eq!(sendback_cause(Some("rc=1; cause=build; kind=smoke")), "build");
        assert_eq!(sendback_cause(Some("build: the API changed")), "build");
        assert_eq!(sendback_cause(Some("please rebase: it is stale")), "unstated");
        assert_eq!(sendback_cause(Some("cause=")), "unstated");
        assert_eq!(sendback_cause(None), "unstated");
    }

    #[test]
    fn nothing_compares_as_nothing() {
        let empty = compare(&Activity::of(&[]), &[], Window::default());
        assert!(empty.groups.is_empty());
        assert_eq!((empty.all.workers, empty.all.landed, empty.all.landed_per_active_hour.value, empty.span_seconds), (0, 0, None, 0));
        let activity = Activity::of(&swarm());
        let wide = compare(&activity, &tickets(&activity), Window { start: Some(Timestamp(i64::MIN)), end: Some(Timestamp(i64::MAX)) });
        assert_eq!(wide.all.landed, 3);
        let backward = compare(&activity, &tickets(&activity), Window { start: Some(at("2026-10-31T10:00:00Z")), end: Some(at("2026-10-31T09:00:00Z")) });
        assert_eq!(backward.span_seconds, 0);
    }
}
